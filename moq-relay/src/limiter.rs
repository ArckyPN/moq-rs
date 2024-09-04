use std::{process::Command, sync::Arc};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use tokio::{
	sync::RwLock,
	task::JoinHandle,
	time::{sleep, Duration},
};

fn default_trajectory_mode() -> String {
	"cascade".to_string()
}

#[derive(Debug)]
pub struct Limiter {
	current_limit: Option<u32>,
	default_latency: u32,
	network_interfaces: Vec<String>,
	running_handle: Option<JoinHandle<anyhow::Result<()>>>,
}

impl Limiter {
	pub fn new(default_latency: Option<u32>) -> anyhow::Result<Self> {
		if std::env::consts::OS != "linux" {
			anyhow::bail!("tc only supported on linux");
		}

		let network_interfaces = Self::get_interfaces()?;

		let default_latency = default_latency.unwrap_or(50);

		Ok(Self {
			current_limit: None,
			default_latency,
			network_interfaces,
			running_handle: None,
		})
	}

	pub fn set_handle(&mut self, handle: JoinHandle<anyhow::Result<()>>) {
		if let Some(current) = self.running_handle.replace(handle) {
			current.abort();
		}
	}

	pub fn abort(&mut self) {
		if let Some(current) = self.running_handle.take() {
			current.abort();
		}
	}

	fn get_interfaces() -> anyhow::Result<Vec<String>> {
		let mut interfaces = Vec::new();
		for file in std::fs::read_dir("/sys/class/net")? {
			interfaces.push(file?.file_name().to_str().context("invalid file path")?.to_string());
		}
		interfaces.retain(|interface| interface != "lo");
		Ok(interfaces)
	}
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Trajectory {
	pub limit: u32,
	pub duration: u32,
	pub latency: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TrajectoryQuery {
	#[serde(default)]
	pub looping: bool,
	#[serde(default = "default_trajectory_mode")]
	pub mode: String,
}

pub async fn set_bandwidth(limiter: Arc<RwLock<Limiter>>, limit: i64, latency: i64) -> anyhow::Result<()> {
	if limit < 0 {
		_ = delete_all_qdiscs(&limiter).await;
		return Ok(());
	}
	let latency = match latency {
		..=0 => limiter.read().await.default_latency,
		l => l as u32,
	};
	let trajectory = Trajectory {
		limit: limit as u32,
		duration: 0,
		latency,
	};
	set_trajectory(limiter, vec![trajectory], None).await?;
	Ok(())
}

pub async fn unset_bandwidth(limiter: Arc<RwLock<Limiter>>) -> anyhow::Result<()> {
	log::debug!("Limiter: aborting...");
	let l1 = limiter.clone();
	{
		let mut lock = l1.write().await;
		lock.abort();
	}
	log::debug!("Limiter: aborted");
	delete_all_qdiscs(&limiter).await
}

pub async fn set_trajectory(
	limiter: Arc<RwLock<Limiter>>,
	trajectory: Vec<Trajectory>,
	query: Option<TrajectoryQuery>,
) -> anyhow::Result<()> {
	let (looping, mode) = match query {
		Some(q) => (q.looping, q.mode),
		None => (false, "-".to_string()),
	};

	let trajectory = match mode.as_str() {
		"cascade" => {
			let buf = include_bytes!("cascade.json");
			serde_json::from_slice(buf)?
		}
		"4g" => {
			let buf = include_bytes!("4g_trajectory.json");
			serde_json::from_slice(buf)?
		}
		_ => trajectory,
	};

	if trajectory.is_empty() {
		anyhow::bail!("cannot set empty trajectory");
	}

	log::debug!("Limiter: limiting bandwidth...");

	loop {
		for step in &trajectory {
			let limiter = limiter.clone();
			let bandwidth = format!("{}kbit", step.limit);
			let latency = match step.latency {
				0 => format!("{}ms", limiter.read().await.default_latency),
				l => format!("{l}ms"),
			};

			{
				let mut lock = limiter.write().await;
				lock.current_limit.replace(step.limit);
			}

			_ = delete_all_qdiscs(&limiter).await;

			if step.duration == 0 {
				log::debug!("Limiter: limiting to {bandwidth} for eternity (or until reset)");
			} else {
				log::debug!("Limiter: limiting to {bandwidth} for {}ms", step.duration);
			}

			for interface in &limiter.read().await.network_interfaces {
				Command::new("tc")
					// if this doesnÄt work use the original args from Björn:
					// "qdisc", "add", "dev", interface, "root", "tbf", "rate", &bandwidth, "latency", &latency, "burst", "1540"
					.args([
						"qdisc", "add", "dev", interface, "root", "netem", "delay", &latency, "rate", &bandwidth,
					])
					.output()
					.context("failed adding qdisc")?;
			}

			if step.duration == 0 {
				return Ok(());
			}

			sleep(Duration::from_millis(step.duration as u64)).await;
		}

		if !looping {
			break;
		}
	}

	{
		let mut lock = limiter.write().await;
		lock.abort();
	}

	_ = delete_all_qdiscs(&limiter).await;

	log::debug!("Limiter: finished");

	Ok(())
}

async fn delete_all_qdiscs(limiter: &Arc<RwLock<Limiter>>) -> anyhow::Result<()> {
	for interface in &limiter.read().await.network_interfaces {
		Command::new("tc")
			.args(["qdisc", "delete", "dev", interface, "root"])
			.output()
			.context("failed deleting qdiscs")?;
	}

	log::debug!("Limiter: removed all limits");

	Ok(())
}
