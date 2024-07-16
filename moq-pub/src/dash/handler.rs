use std::{collections::HashMap, io::SeekFrom, path, sync::Arc};

use notify::{
	event::{AccessKind::Close, AccessMode::Write, CreateKind::File, ModifyKind::Data},
	EventKind::{Access, Create, Modify},
};
use tokio::{
	fs,
	io::{self, AsyncReadExt, AsyncSeekExt},
	sync::RwLock,
};

use super::{helper, Chunk, Error};

pub struct FsEventHandler(Arc<RwLock<HashMap<String, usize>>>);

impl FsEventHandler {
	pub fn new() -> Self {
		Self(Arc::new(RwLock::new(HashMap::new())))
	}

	pub async fn handle(&mut self, event: notify::Event, tx: tokio::sync::mpsc::Sender<Chunk>) -> anyhow::Result<()> {
		// println!("Event {:?}", event);
		match event.kind {
			Create(File) => {
				self.insert(&event.paths).await?;
			}
			Modify(Data(_)) => {
				self.send_chunk(&event.paths, tx).await?;
			}
			Access(Close(Write)) => {
				self.send_chunk(&event.paths, tx).await?;

				self.delete(&event.paths).await?;
			}
			// Modify(Name(To))?
			_ => (),
		}
		Ok(())
	}

	async fn send_chunk(
		&mut self,
		paths: &[path::PathBuf],
		tx: tokio::sync::mpsc::Sender<Chunk>,
	) -> anyhow::Result<()> {
		if paths.len() != 1 {
			return Err(Error::InvalidPathNum(1, paths.len()).into());
		}

		let path = &paths[0];
		let chunk = self.read_chunk(&path).await?;

		if chunk.is_empty() {
			return Ok(());
		}

		let chunk = Chunk::new(path, chunk)?;

		tx.send(chunk).await?;

		Ok(())
	}

	async fn read_chunk<P>(&mut self, path: P) -> anyhow::Result<Vec<u8>>
	where
		P: AsRef<path::Path>,
	{
		let Some(path) = helper::path_to_string(path) else {
			return Err(Error::FailedToConvert.into());
		};

		let offset = self.get(&path).await;

		let mut fp = match fs::File::open(&path).await {
			Ok(fp) => fp,
			Err(e) => {
				if e.kind() != std::io::ErrorKind::NotFound {
					return Err(e.into());
				}
				fs::File::open(path.replace(".tmp", "")).await?
			}
		};
		fp.seek(SeekFrom::Start(offset as u64)).await?;

		let size = fp.metadata().await?.len() as usize;

		let mut contents = vec![0u8; size - offset];
		fp.read_exact(&mut contents).await?;

		self.set(&path, size).await;

		Ok(contents)
	}

	async fn insert(&mut self, paths: &[path::PathBuf]) -> anyhow::Result<()> {
		if paths.len() != 1 {
			return Err(Error::InvalidPathNum(1, paths.len()).into());
		}

		let path = &paths[0];
		let Some(path) = helper::path_to_string(path) else {
			return Err(Error::FailedToConvert.into());
		};

		// only cache segments
		if !path.ends_with(".m4s.tmp") {
			return Ok(());
		}

		// cache segment path and offset
		self.set(&path, 0).await;

		Ok(())
	}

	async fn delete(&mut self, paths: &[path::PathBuf]) -> anyhow::Result<()> {
		if paths.len() != 1 {
			return Err(Error::InvalidPathNum(1, paths.len()).into());
		}

		let path = &paths[0];
		let Some(path) = helper::path_to_string(path) else {
			return Err(Error::FailedToConvert.into());
		};

		let mut lock = self.0.write().await;
		lock.remove(&path);

		Ok(())
	}

	async fn get(&self, key: &str) -> usize {
		let lock = self.0.read().await;
		let value = lock.get(key);
		if value.is_some() {
			value.copied().unwrap()
		} else {
			0
		}
	}

	async fn set(&mut self, key: &str, offset: usize) {
		let mut lock = self.0.write().await;
		lock.insert(key.to_string(), offset);
	}
}
