use bytes::BytesMut;
use std::{fs, net, path};
use url::Url;

use anyhow::Context;
use clap::{Args, Parser, Subcommand};
use tokio::io::AsyncReadExt;

use moq_native::quic;
use moq_pub::Media;
use moq_transport::{serve, session::Publisher};

mod dash;

#[derive(Parser)]
pub struct Cli {
	#[command(subcommand)]
	pub(crate) command: Commands,
}
#[derive(Subcommand)]
enum Commands {
	/// Original Publisher
	Run(Original),

	/// Dash Publisher
	Dash(Dash),
}

#[derive(Args, Clone)]
struct Original {
	/// Listen for UDP packets on the given address.
	#[arg(long, default_value = "[::]:0")]
	pub bind: net::SocketAddr,

	/// Advertise this frame rate in the catalog (informational)
	// TODO auto-detect this from the input when not provided
	#[arg(long, default_value = "24")]
	pub fps: u8,

	/// Advertise this bit rate in the catalog (informational)
	// TODO auto-detect this from the input when not provided
	#[arg(long, default_value = "1500000")]
	pub bitrate: u32,

	/// Connect to the given URL starting with https://
	#[arg()]
	pub url: Url,

	/// The name of the broadcast
	#[arg(long)]
	pub name: String,

	/// The TLS configuration.
	#[command(flatten)]
	pub tls: moq_native::tls::Args,
}

#[derive(Args, Clone)]
struct Dash {
	/// The path to ffmpeg input, default is integrated laptop camera (Linux: /dev/video0)
	#[arg(short, long, default_value = "/dev/video0")]
	pub input: path::PathBuf,

	/// The path to DASH Manifest output file (.mpd)
	#[arg(short, long)]
	pub output: path::PathBuf,

	/// The path to the Settings file
	#[arg(short = 's', long = "settings", default_value = "../media/settings.csv")]
	pub settings_file: path::PathBuf,

	/// The name of the broadcast
	#[arg(long)]
	pub name: String,

	/// Set to not publish audio
	#[arg(long)]
	pub no_audio: bool,

	/// Listen for UDP packets on the given address.
	#[arg(long, default_value = "[::]:0")]
	pub bind: net::SocketAddr,

	/// Connect to the given URL starting with https://
	#[arg()]
	pub url: Url,

	/// The TLS configuration.
	#[command(flatten)]
	pub tls: moq_native::tls::Args,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	env_logger::init();

	// Disable tracing so we don't get a bunch of Quinn spam.
	let tracer = tracing_subscriber::FmtSubscriber::builder()
		.with_max_level(tracing::Level::WARN)
		.finish();
	tracing::subscriber::set_global_default(tracer).unwrap();

	let cli = Cli::parse();

	match cli.command {
		Commands::Run(args) => run_orignal(args).await.unwrap(),
		Commands::Dash(args) => run_dash(args).await.unwrap(),
	}

	Ok(())
}

async fn run_orignal(cli: Original) -> anyhow::Result<()> {
	let (writer, _, reader) = serve::Tracks::new(cli.name).produce();
	let media = Media::new(writer)?;

	let tls = cli.tls.load()?;

	let quic = quic::Endpoint::new(moq_native::quic::Config {
		bind: cli.bind,
		tls: tls.clone(),
	})?;

	log::info!("connecting to relay: url={}", cli.url);
	let session = quic.client.connect(&cli.url).await?;

	let (session, mut publisher) = Publisher::connect(session)
		.await
		.context("failed to create MoQ Transport publisher")?;

	tokio::select! {
		res = session.run() => res.context("session error")?,
		res = run_media(media) => res.context("media error")?,
		res = publisher.announce(reader) => res.context("publisher error")?,
	}

	Ok(())
}

async fn run_dash(cli: Dash) -> anyhow::Result<()> {
	let ffmpeg = dash::FFmpeg::new(cli)?;

	ffmpeg.run().await?;

	Ok(())
}

async fn run_media(mut media: Media) -> anyhow::Result<()> {
	let mut input = tokio::io::stdin();
	let mut buf = BytesMut::new();

	loop {
		input.read_buf(&mut buf).await.context("failed to read from stdin")?;
		media.parse(&mut buf).context("failed to parse media")?;
	}
}
