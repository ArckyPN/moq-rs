use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
	#[error("crate={krayt} err={error}")]
	External { krayt: String, error: String },

	#[error("cannot add tracks, because catalogs are already present")]
	CatalogsAlreadySet,

	#[error("cannot add catalog, because tracks are already present")]
	TracksAlreadySet,
}
