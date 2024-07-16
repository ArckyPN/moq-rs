use std::{fmt, path};

use super::helper;

pub struct Chunk {
	pub name: String,
	pub data: Vec<u8>,
}

impl Chunk {
	pub fn new<P>(path: P, data: Vec<u8>) -> anyhow::Result<Self>
	where
		P: AsRef<path::Path>,
	{
		let name = helper::clean_path(path)?;

		Ok(Self { name, data })
	}
}

impl fmt::Display for Chunk {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "Chunk {{ name: {}, len: {} }}", self.name, self.data.len())
	}
}
