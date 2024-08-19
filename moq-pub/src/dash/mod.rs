use futures::StreamExt;
use signal_hook::consts::signal::{SIGHUP, SIGINT, SIGQUIT, SIGTERM};
use std::io::Read;
use std::path;

mod error;
mod helper;
mod publisher;
mod settings;
mod watcher;

use error::Error;
use publisher::Publisher;
use settings::Settings;

pub struct PubInfo {
	pub tls: moq_native::tls::Args,
	pub url: url::Url,
	pub bind: std::net::SocketAddr,
	pub namespace: String,
}

pub struct Dash {
	settings: settings::Settings<std::path::PathBuf>,
	output: path::PathBuf,
	info: PubInfo,
}

impl Dash {
	pub fn new(cli: super::Dash) -> Result<Self, Error> {
		let settings = settings::Settings::new(
			cli.settings_file,
			cli.input,
			cli.output.clone(),
			cli.no_audio,
			cli.looping,
		)?;

		settings.save(cli.output.with_file_name("dash.sh"))?;

		Ok(Self {
			settings,
			output: cli.output,
			info: PubInfo {
				tls: cli.tls,
				url: cli.url,
				bind: cli.bind,
				namespace: cli.name,
			},
		})
	}

	pub async fn run(self) -> Result<(), Error> {
		helper::init_output(&self.output)?;

		let args = self.settings.to_args()?;
		let mut ffmpeg = match std::process::Command::new("ffmpeg")
			.args(args)
			.stdout(std::process::Stdio::null())
			.stderr(std::process::Stdio::piped())
			.spawn()
		{
			Ok(c) => c,
			Err(e) => {
				println!("Error: {}", e);
				return Err(Error::Crate("process".to_string(), e.to_string()));
			}
		};

		let Some(output) = ffmpeg.stderr.take() else {
			println!("Error: failed to take FFmpeg stderr");
			return Err(Error::Crate("process".to_string(), "failed to take stderr".to_string()));
		};

		let (session, mut publisher, writer, reader) = create(self.info).await?;

		tokio::select! {
			res = session.run() => println!("Session: {:#?}", res),
			res = run(&self.output, writer, self.settings) => println!("run: {:#?}", res),
			res = publisher.announce(reader) => println!("Publisher: {:#?}", res),
			res = close() => println!("close: {:#?}", res),
			res = read_output(output) => println!("output: {:#?}", res),
		}

		log::info!("termination initiated, cleaning up");

		if let Err(e) = ffmpeg.kill() {
			println!("Error: {}", e);
			return Err(Error::Crate("process".to_string(), e.to_string()));
		}

		helper::clear_output(&self.output)?;

		Ok(())
	}
}

pub async fn create(
	info: PubInfo,
) -> Result<
	(
		moq_transport::session::Session,
		moq_transport::session::Publisher,
		moq_transport::serve::TracksWriter,
		moq_transport::serve::TracksReader,
	),
	Error,
> {
	let (writer, _, reader) = moq_transport::serve::Tracks::new(info.namespace.clone()).produce();

	let tls = match info.tls.load() {
		Ok(t) => t,
		Err(e) => {
			println!("Error: {}", e);
			return Err(Error::Crate("tls".to_string(), e.to_string()));
		}
	};

	let quic = match moq_native::quic::Endpoint::new(moq_native::quic::Config {
		bind: info.bind,
		tls: tls.clone(),
	}) {
		Ok(q) => q,
		Err(e) => {
			println!("Error: {}", e);
			return Err(Error::Crate("moq_native".to_string(), e.to_string()));
		}
	};

	log::info!("connecting to relay: url={}", info.url);
	let session = match quic.client.connect(&info.url).await {
		Ok(s) => s,
		Err(e) => {
			println!("Error: {}", e);
			return Err(Error::Crate("moq_native".to_string(), e.to_string()));
		}
	};

	let (session, publisher) = match moq_transport::session::Publisher::connect(session).await {
		Ok(v) => v,
		Err(e) => {
			println!("Error: {}", e);
			return Err(Error::Crate("moq_transport".to_string(), e.to_string()));
		}
	};

	Ok((session, publisher, writer, reader))
}

pub async fn run<P>(
	target: P,
	writer: moq_transport::serve::TracksWriter,
	settings: Settings<std::path::PathBuf>,
) -> Result<(), Error>
where
	P: AsRef<std::path::Path>,
{
	let mut watcher = watcher::MoqWatcher::new(writer, settings)?;

	watcher.run(target).await?;

	Ok(())
}

async fn close() -> anyhow::Result<()> {
	let mut signals = signal_hook_tokio::Signals::new([SIGHUP, SIGTERM, SIGINT, SIGQUIT])?;
	let handle = signals.handle();

	while let Some(signal) = signals.next().await {
		match signal {
			SIGHUP | SIGTERM | SIGINT | SIGQUIT => break,
			_ => (),
		}
	}

	handle.close();

	Ok(())
}

async fn read_output(mut stderr: std::process::ChildStderr) -> anyhow::Result<()> {
	let re = regex::Regex::new(r"speed=(?<speed>(?:0|1)\.\d{3}x)")?;
	let pb = indicatif::ProgressBar::new_spinner();
	pb.enable_steady_tick(std::time::Duration::from_millis(100));
	pb.set_style(
		indicatif::ProgressStyle::with_template("{spinner} {msg}")?
			.tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
	);

	loop {
		let mut buf = [0; 1024];
		let read = stderr.read(&mut buf)?;

		let text = match String::from_utf8(buf[..read].to_vec()) {
			Ok(v) => v,
			Err(_) => continue,
		};

		let matches = match re.captures(&text) {
			Some(v) => v,
			None => continue,
		};

		pb.set_message(format!("Speed: {}", &matches["speed"]));

		tokio::time::sleep(tokio::time::Duration::from_millis(1_000)).await;
	}
}
