use notify::Watcher;
use notify::{
	event::{AccessKind::Close, AccessMode::Write, CreateKind::File, ModifyKind::Data},
	EventKind::{Access, Create, Modify},
};
use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use super::helper;
use super::Error;

pub struct MoqWatcher {
	store: HashMap<String, usize>,
	publisher: super::Publisher,
	re: regex::Regex,
}

impl MoqWatcher {
	pub fn new(
		broadcast: moq_transport::serve::TracksWriter,
		settings: super::Settings<std::path::PathBuf>,
	) -> Result<Self, Error> {
		let re = match regex::Regex::new(r"rep_(?<rep>\d+)\.m4s") {
			Ok(r) => r,
			Err(e) => {
				println!("Error: {}", e);
				return Err(Error::Crate("regex".to_string(), e.to_string()));
			}
		};
		Ok(Self {
			store: HashMap::new(),
			publisher: super::Publisher::new(broadcast, settings)?,
			re,
		})
	}

	pub async fn run<P>(&mut self, target: P) -> Result<(), Error>
	where
		P: AsRef<std::path::Path>,
	{
		let (tx, rx) = std::sync::mpsc::channel();

		let mut watcher = match notify::recommended_watcher(tx) {
			Ok(w) => w,
			Err(e) => {
				println!("Error: {}", e);
				return Err(Error::Crate("notify".to_string(), e.to_string()));
			}
		};

		if let Err(e) = watcher.watch(target.as_ref(), notify::RecursiveMode::NonRecursive) {
			println!("Error: {}", e);
			return Err(Error::Crate("notify".to_string(), e.to_string()));
		}

		for event in rx {
			let event = match event {
				Ok(e) => e,
				Err(e) => {
					println!("Error: {}", e);
					return Err(Error::Crate("notify".to_string(), e.to_string()));
				}
			};

			self.handle(event).await?;
		}
		Ok(())
	}

	async fn handle(&mut self, event: notify::Event) -> Result<(), Error> {
		if self.is_mpd(&event) {
			return Ok(());
		}
		match event.kind {
			Create(File) => {
				// watch segment files in chunks
				self.insert(&event.paths).await?;
			}
			Modify(Data(_)) => {
				// new chunk has been written, send to publisher
				self.send_chunk(&event.paths).await?;
			}
			Access(Close(Write)) => {
				// file is finished, make sure to really have everything
				self.send_chunk(&event.paths).await?;

				self.delete(&event.paths).await?;
			}
			_ => (),
		}
		Ok(())
	}

	async fn send_chunk(&mut self, paths: &[std::path::PathBuf]) -> Result<(), Error> {
		if paths.len() != 1 {
			println!("Error: invalid num of paths");
			return Err(Error::InvalidPathNum(1, paths.len()));
		}

		let path = &paths[0];
		let chunk = self.read_chunk(&path).await?;

		if chunk.is_empty() {
			return Ok(());
		}

		let rep_id = self.parse_path(path)?;
		self.publisher.publish(rep_id, &chunk)?;

		Ok(())
	}

	async fn read_chunk<P>(&mut self, path: P) -> Result<Vec<u8>, Error>
	where
		P: AsRef<std::path::Path>,
	{
		let Some(path) = helper::path_to_string(path) else {
			println!("Error: could not convert path to string");
			return Err(Error::FailedToConvert);
		};

		let offset = self.get(&path).await;

		let mut fp = match tokio::fs::File::open(&path).await {
			Ok(f) => f,
			Err(e) => {
				if e.kind() != std::io::ErrorKind::NotFound {
					println!("Error: missing file");
					return Err(Error::Crate("tokio::fs".to_string(), e.to_string()));
				}
				match tokio::fs::File::open(path.replace(".tmp", "")).await {
					Ok(f) => f,
					Err(e) => {
						println!("Error: missing file");
						return Err(Error::Crate("tokio::fs".to_string(), e.to_string()));
					}
				}
			}
		};

		if let Err(e) = fp.seek(std::io::SeekFrom::Start(offset as u64)).await {
			println!("Error: {}", e);
			return Err(Error::Crate("tokio::fs".to_string(), e.to_string()));
		}

		let size = match fp.metadata().await {
			Ok(m) => m.len() as usize,
			Err(e) => {
				println!("Error: {}", e);
				return Err(Error::Crate("tokio::fs".to_string(), e.to_string()));
			}
		};

		let mut chunk = vec![0u8; size - offset];
		let read = match fp.read_exact(&mut chunk).await {
			Ok(r) => r,
			Err(e) => {
				println!("Error: {}", e);
				return Err(Error::Crate("tokio::fs".to_string(), e.to_string()));
			}
		};

		assert_eq!(read, size - offset);

		self.set(&path, size).await;

		Ok(chunk)
	}

	async fn insert(&mut self, paths: &[std::path::PathBuf]) -> Result<(), Error> {
		if paths.len() != 1 {
			println!("Error: invalid num of paths");
			return Err(Error::InvalidPathNum(1, paths.len()));
		}

		let path = &paths[0];
		let Some(path) = helper::path_to_string(path) else {
			println!("Error: could not convert path to string");
			return Err(Error::FailedToConvert);
		};

		if !path.ends_with(".m4s.tmp") {
			return Ok(());
		}

		self.set(&path, 0).await;

		Ok(())
	}

	async fn delete(&mut self, paths: &[std::path::PathBuf]) -> Result<(), Error> {
		if paths.len() != 1 {
			println!("Error: invalid num of paths");
			return Err(Error::InvalidPathNum(1, paths.len()));
		}

		let path = &paths[0];
		let Some(path) = helper::path_to_string(path) else {
			println!("Error: could not convert path to string");
			return Err(Error::FailedToConvert);
		};

		self.store.remove(&path);

		Ok(())
	}

	fn is_mpd(&self, event: &notify::Event) -> bool {
		for path in &event.paths {
			let Some(path) = helper::path_to_string(path) else {
				return false;
			};
			if path.contains(".mpd") {
				return true;
			}
		}
		false
	}

	fn parse_path<P>(&self, path: P) -> Result<usize, Error>
	where
		P: AsRef<std::path::Path>,
	{
		let Some(path) = helper::path_to_string(path) else {
			println!("Error: could not convert path to string");
			return Err(Error::FailedToConvert);
		};

		let matches = match self.re.captures(&path) {
			Some(m) => m,
			None => {
				println!("Error: missing rep id in path");
				return Err(Error::Missing);
			}
		};

		let rep_id = match matches["rep"].parse() {
			Ok(r) => r,
			Err(_) => {
				println!("Error: failed to parse {} to usize", &matches["rep"]);
				return Err(Error::FailedToConvert);
			}
		};

		Ok(rep_id)
	}

	async fn get(&self, key: &str) -> usize {
		let value = self.store.get(key);
		*value.unwrap_or(&0)
	}

	async fn set(&mut self, key: &str, offset: usize) {
		self.store.insert(key.to_string(), offset);
	}
}
