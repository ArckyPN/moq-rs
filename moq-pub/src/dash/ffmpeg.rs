use anyhow::Context;
use bytes::BytesMut;
use notify::Watcher;
use std::{collections::HashMap, io::Read, net, path, thread};
use tokio::sync::broadcast::error::TryRecvError;

use signal_hook::consts::signal::*;
use signal_hook_tokio::Signals;

use futures::stream::StreamExt;

use super::{dash, Chunk, FsEventHandler, Settings};
use crate::{dash::helper, Dash};

pub struct PubInfo {
	pub tls: moq_native::tls::Args,
	pub url: url::Url,
	pub bind: net::SocketAddr,
	pub namespace: String,
	pub tracks: Settings,
}

pub struct FFmpeg {
	args: Vec<String>,
	output: path::PathBuf,
	info: PubInfo,
}

impl FFmpeg {
	pub fn new(cli: Dash) -> anyhow::Result<Self> {
		let settings = Settings::new(cli.settings_file)?;
		let args = settings.to_args(cli.input, cli.output.clone(), cli.no_audio, cli.looping)?;

		settings.save(
			format!(
				"{}/{}.sh",
				cli.output
					.as_path()
					.parent()
					.context("invalid path")?
					.to_str()
					.context("not a string")?,
				cli.name,
			),
			&args,
		)?;

		Ok(Self {
			args,
			output: cli.output,
			info: PubInfo {
				tls: cli.tls,
				url: cli.url,
				bind: cli.bind,
				namespace: cli.name.clone(),
				tracks: settings,
			},
		})
	}

	pub async fn run(self) -> anyhow::Result<()> {
		helper::init_output(&self.output)?;

		// spawn ffmpeg child process
		let mut ffmpeg = std::process::Command::new("ffmpeg")
			.args(self.args.clone())
			.stdout(std::process::Stdio::null())
			.stderr(std::process::Stdio::piped())
			.spawn()?;

		let mut output = ffmpeg.stderr.take();

		let (tx, rx) = tokio::sync::mpsc::channel(1);
		let (control_tx, mut control_rx) = tokio::sync::broadcast::channel(1);

		// spawn closer thread
		let ctrl_tx = control_tx.clone();
		thread::Builder::new().name("closer".to_string()).spawn(move || {
			let rt = tokio::runtime::Runtime::new().unwrap();
			rt.block_on(async {
				if let Err(err) = close(ctrl_tx.clone()).await {
					log::error!("close thread died: {}", err);
					ctrl_tx.send(()).expect("closer error");
				}
			})
		})?;

		// spawn watcher thread
		let ctrl_rx = control_tx.subscribe();
		let ctrl_tx = control_tx.clone();
		let target = self.output.clone();
		thread::Builder::new().name("watcher".to_string()).spawn(move || {
			let rt = tokio::runtime::Runtime::new().unwrap();
			rt.block_on(async {
				if let Err(err) = watch(ctrl_rx, tx, target).await {
					log::error!("watch thread died: {}", err);
					ctrl_tx.send(()).expect("watch error");
				}
			})
		})?;

		// spawn publisher thread
		let ctrl_rx = control_tx.subscribe();
		let ctrl_tx = control_tx.clone();
		thread::Builder::new().name("publisher".to_string()).spawn(move || {
			let rt = tokio::runtime::Runtime::new().unwrap();
			rt.block_on(async {
				if let Err(err) = publish(ctrl_rx, rx, self.info).await {
					log::error!("publish thread died: {}", err);
					ctrl_tx.send(()).expect("publisher error");
				}
			})
		})?;

		let re = regex::Regex::new(r"speed=(?<speed>(?:0|1)\.\d{3})x")?;
		let pb = indicatif::ProgressBar::new_spinner();
		pb.enable_steady_tick(std::time::Duration::from_millis(100));
		pb.set_style(
			indicatif::ProgressStyle::with_template("{spinner} {msg}")?
				.tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
		);

		// block until control channel stops
		loop {
			match control_rx.try_recv() {
				Ok(_) | Err(TryRecvError::Closed) | Err(TryRecvError::Lagged(_)) => break,
				Err(TryRecvError::Empty) => (),
			}

			if let Some(ref mut out) = output {
				let mut buf = [0; 1024];
				let read = out.read(&mut buf)?;

				let text = match String::from_utf8(buf[..read].to_vec()) {
					Ok(v) => v,
					Err(_) => continue,
				};

				let matches = match re.captures(&text) {
					Some(v) => v,
					None => continue,
				};

				pb.set_message(format!("Speed: {}x", &matches["speed"]));
			}
		}

		log::info!("received termination signal, closing gracefully");

		// kill ffmpeg on exit
		ffmpeg.kill()?;

		helper::clear_output(&self.output)?;

		Ok(())
	}
}

async fn watch<P>(
	mut control_rx: tokio::sync::broadcast::Receiver<()>,
	tx: tokio::sync::mpsc::Sender<Chunk>,
	target: P,
) -> anyhow::Result<()>
where
	P: AsRef<path::Path>,
{
	let (send, recv) = std::sync::mpsc::channel();
	let mut handler = FsEventHandler::new();

	let mut watcher = notify::RecommendedWatcher::new(send, notify::Config::default())?;

	watcher.watch(target.as_ref(), notify::RecursiveMode::NonRecursive)?;

	// receive fs events
	for evt in recv {
		let event = evt?;

		// verify if we're still good
		match control_rx.try_recv() {
			Ok(_) | Err(TryRecvError::Closed) | Err(TryRecvError::Lagged(_)) => break,
			Err(TryRecvError::Empty) => (),
		}

		handler.handle(event, tx.clone()).await?;
	}

	Ok(())
}

async fn publish(
	control_rx: tokio::sync::broadcast::Receiver<()>,
	rx: tokio::sync::mpsc::Receiver<Chunk>,
	info: PubInfo,
) -> anyhow::Result<()> {
	let (writer, _, reader) = moq_transport::serve::Tracks::new(info.namespace.clone()).produce();

	let tls = info.tls.load()?;

	let quic = moq_native::quic::Endpoint::new(moq_native::quic::Config {
		bind: info.bind,
		tls: tls.clone(),
	})?;

	log::info!("connecting to relay: url={}", info.url);
	let session = quic.client.connect(&info.url).await?;

	let (session, mut publisher) = moq_transport::session::Publisher::connect(session)
		.await
		.context("failed to create MoQ Transport publisher")?;

	let settings = info.tracks;
	tokio::select! {
		res = session.run() => res.context("session error")?,
		res = run_media(control_rx, rx, writer, settings) => res.context("media error")?,
		res = publisher.announce(reader) => res.context("publisher error")?,
	}

	Ok(())
}

async fn close(control_tx: tokio::sync::broadcast::Sender<()>) -> anyhow::Result<()> {
	let mut signals = Signals::new([SIGHUP, SIGTERM, SIGINT, SIGQUIT])?;
	let handle = signals.handle();

	// loop until termination signal has been received
	while let Some(signal) = signals.next().await {
		println!("{}", signal);
		match signal {
			SIGHUP | SIGTERM | SIGINT | SIGQUIT => break,
			_ => (),
		}
	}

	// clean up signal listener
	handle.close();

	// signal other threads to terminate
	control_tx.send(())?;

	Ok(())
}

async fn run_media(
	mut control_rx: tokio::sync::broadcast::Receiver<()>,
	mut rx: tokio::sync::mpsc::Receiver<Chunk>,
	broadcast: moq_transport::serve::TracksWriter,
	settings: Settings,
) -> anyhow::Result<()> {
	let re = regex::Regex::new(r"rep_(?P<rep_id>.+)\.m4s$")?;

	let mut dash = dash::Dash::new(broadcast, settings)?;

	let mut buf = HashMap::new();

	while let Some(chunk) = rx.recv().await {
		let Chunk { name, data } = chunk;

		let Some(matches) = re.captures(&name) else {
			continue;
		};

		let rep_id = matches["rep_id"].to_string();

		// if rep_id is new create new map entry
		if !buf.contains_key(&rep_id) {
			buf.insert(rep_id.clone(), BytesMut::new());
		}

		// append chunk to buffer and continue parsing + publishing
		let b: &mut BytesMut = buf.get_mut(&rep_id).unwrap();
		b.extend_from_slice(&data);
		dash.parse(b, rep_id)?;

		// check if runtime is still alive
		match control_rx.try_recv() {
			Ok(_) | Err(TryRecvError::Closed) | Err(TryRecvError::Lagged(_)) => break,
			Err(TryRecvError::Empty) => (),
		}
	}

	Ok(())
}
