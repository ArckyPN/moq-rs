use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
	#[error("invalid number of paths given. expected {0}, got {1}")]
	InvalidPathNum(usize, usize),

	#[error("failed to convert")]
	FailedToConvert,

	#[error("getter returned None")]
	ElementNotCached,
}
