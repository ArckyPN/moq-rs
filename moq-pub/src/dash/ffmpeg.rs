use notify::Watcher;
use std::{path, thread};
use tokio::sync::broadcast::error::TryRecvError;

use signal_hook::consts::signal::*;
use signal_hook_tokio::Signals;

use futures::stream::StreamExt;

use super::{Chunk, FsEventHandler, Settings};
use crate::{dash::helper, Dash};

pub struct FFmpeg {
	name: String,
	args: Vec<String>,
	output: path::PathBuf,
}

impl FFmpeg {
	pub fn new(cli: Dash) -> anyhow::Result<Self> {
		let settings = Settings::new(cli.settings_file)?;
		let args = settings.to_args(cli.input, cli.output.clone(), cli.no_audio)?;

		Ok(Self {
			name: "ffmpeg".to_string(),
			args,
			output: cli.output,
		})
	}

	pub async fn run(self) -> anyhow::Result<()> {
		helper::init_output(&self.output)?;

		// spawn ffmpeg child process
		let mut ffmpeg = std::process::Command::new(self.name.clone())
			.args(self.args.clone())
			.stdout(std::process::Stdio::null())
			.stderr(std::process::Stdio::null())
			.spawn()?;

		let (tx, rx) = tokio::sync::mpsc::channel(1);
		let (control_tx, mut control_rx) = tokio::sync::broadcast::channel(1);

		let ctrl_tx = control_tx.clone();
		thread::Builder::new().name("closer".to_string()).spawn(move || {
			let rt = tokio::runtime::Runtime::new().unwrap();
			rt.block_on(async { close(ctrl_tx).await.expect("close error") })
		})?;
		let ctrl_rx = control_tx.subscribe();
		let target = self.output.clone();
		thread::Builder::new().name("watcher".to_string()).spawn(move || {
			let rt = tokio::runtime::Runtime::new().unwrap();
			rt.block_on(async { watch(ctrl_rx, tx, target).await.expect("watch error") })
		})?;
		let ctrl_rx = control_tx.subscribe();
		thread::Builder::new().name("publisher".to_string()).spawn(move || {
			let rt = tokio::runtime::Runtime::new().unwrap();
			rt.block_on(async { publish(ctrl_rx, rx).await.expect("publisher error") })
		})?;

		loop {
			match control_rx.try_recv() {
				Ok(_) | Err(TryRecvError::Closed) | Err(TryRecvError::Lagged(_)) => break,
				Err(TryRecvError::Empty) => (),
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

	for evt in recv {
		// TODO maybe skip Error in Event, if often crashing?
		let event = evt?;

		match control_rx.try_recv() {
			Ok(_) | Err(TryRecvError::Closed) | Err(TryRecvError::Lagged(_)) => break,
			Err(TryRecvError::Empty) => (),
		}

		handler.handle(event, tx.clone()).await?;
	}

	Ok(())
}

async fn publish(
	mut control_rx: tokio::sync::broadcast::Receiver<()>,
	mut rx: tokio::sync::mpsc::Receiver<Chunk>,
) -> anyhow::Result<()> {
	// TODO receive chunks from channel and publish to moq
	while let Some(chunk) = rx.recv().await {
		println!("Publish: {}", chunk);
		match control_rx.try_recv() {
			Ok(_) | Err(TryRecvError::Closed) | Err(TryRecvError::Lagged(_)) => break,
			Err(TryRecvError::Empty) => (),
		}
	}

	Ok(())
}

async fn close(control_tx: tokio::sync::broadcast::Sender<()>) -> anyhow::Result<()> {
	let mut signals = Signals::new([SIGHUP, SIGTERM, SIGINT, SIGQUIT])?;
	let handle = signals.handle();

	// loop until termination signal has been received
	while let Some(signal) = signals.next().await {
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
