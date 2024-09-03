mod error;
// mod internal;

// pub use internal::{Catalog, CommonStructFields, MoqCatalog, SelectionParams, Track};

mod old;

pub use old::{Catalog, CommonStructFields, MoqCatalog, SelectionParams, Track};

pub use error::Error;

use serde::{Deserialize, Serialize};

const VERSION: &str = "1";
const STREAMING_FORMAT: &str = "1";
const STREAMING_FORMAT_VERSION: &str = "1";

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum Packaging {
	#[serde(rename = "cmaf")]
	#[default]
	CMAF,

	#[serde(rename = "loc")]
	LOC,
}
