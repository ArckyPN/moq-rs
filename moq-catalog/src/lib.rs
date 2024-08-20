mod error;

use std::str::FromStr;

pub use error::Error;

use base64::prelude::*;
use serde::{Deserialize, Serialize};

pub static VERSION: &str = "1";
pub static STREAMING_FORMAT: &str = "1";
pub static STREAMING_FORMAT_VERSION: &str = "1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoqCatalog {
	/// Catalog Version
	///
	/// Versions of this catalog specification are defined using
	/// monotonically increasing integers.  There is no guarantee that future
	/// catalog versions are backwards compatible and field definitions and
	/// interpretation may change between versions.  A subscriber MUST NOT
	/// attempt to parse a catalog version which it does not understand.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-catalog-version)
	version: String,

	/// Streaming Format
	///
	/// A number indicating the streaming format type.  Every MoQ Streaming
	/// Format normatively referencing this catalog format MUST register
	/// itself in the "MoQ Streaming Format Type" table.  See Section 5 for
	/// additional details.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-streaming-format)
	#[serde(rename = "streamingFormat")]
	streaming_format: String,

	/// Streaming Format Version
	///
	/// A string indicating the version of the streaming format to which this
	/// catalog applies.  The structure of the version string is defined by
	/// the streaming format.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-streaming-format-version)
	#[serde(rename = "streamingFormatVersion")]
	streaming_format_version: String,

	/// Supports Delta Updates
	///
	/// A boolean that if true indicates that the publisher MAY issue
	/// incremental (delta) updates - see Section 3.3.  If false or absent,
	/// then the publisher guarantees that they will NOT issue any
	/// incremental updates and that any future updates to the catalog will
	/// be independent.  The default value is false.  This field MUST be
	/// present if its value is true, but may be omitted if the value is
	/// false.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-supports-delta-updates)
	#[serde(rename = "supportsDeltaUpdates")]
	supports_delta_updates: Option<bool>,

	/// Common Track Fields
	///
	/// An object holding a collection of Track Fields (objects with a
	/// location of TF or TFC in table 1) which are to be inherited by all
	/// tracks.  A field defined at the Track object level always supercedes
	/// any value inherited from the Common Track Fields object.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-common-track-fields)
	#[serde(rename = "commonTrackFields")]
	common_track_fields: Option<CommonStructFields>,

	/// Tracks
	///
	/// An array of track objects Section 3.2.8.  If the 'tracks' field is
	/// present then the 'catalog' field MUST NOT be present.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#section-3.2.5)
	tracks: Option<Vec<Track>>,

	/// Catalogs
	///
	/// An array of catalog objects Section 3.2.7.  If the 'catalogs' field
	/// is present then the 'tracks' field MUST NOT be present.  A catalog
	/// MUST NOT list itself in the catalog array.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-catalogs)
	catalogs: Option<Vec<Catalog>>,
}

impl MoqCatalog {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn enable_delta_updates(&mut self) -> &mut Self {
		self.supports_delta_updates = Some(true);
		self
	}

	pub fn disable_delta_updates(&mut self) -> &mut Self {
		self.supports_delta_updates = Some(false);
		self
	}

	pub fn supports_delta_updates(&self) -> Option<bool> {
		self.supports_delta_updates
	}

	pub fn set_common_track_fields(&mut self, csf: CommonStructFields) -> &mut Self {
		self.common_track_fields = Some(csf);
		self
	}

	pub fn common_track_fields(&self) -> Option<&CommonStructFields> {
		self.common_track_fields.as_ref()
	}

	pub fn common_track_fields_mut(&mut self) -> Option<&mut CommonStructFields> {
		self.common_track_fields.as_mut()
	}

	pub fn set_tracks(&mut self, tracks: &[Track]) -> Result<&mut Self, Error> {
		if self.catalogs.is_some() {
			return Err(Error::CatalogsAlreadySet);
		}

		self.tracks = Some(tracks.to_vec());
		Ok(self)
	}

	pub fn insert_track(&mut self, track: Track) -> Result<&mut Self, Error> {
		if self.catalogs.is_some() {
			return Err(Error::CatalogsAlreadySet);
		}

		match &mut self.tracks {
			Some(tracks) => tracks.push(track),
			None => self.tracks = Some(vec![track]),
		}
		Ok(self)
	}

	pub fn set_catalog(&mut self, catalog: &[Catalog]) -> Result<&mut Self, Error> {
		if self.tracks.is_some() {
			return Err(Error::TracksAlreadySet);
		}

		self.catalogs = Some(catalog.to_vec());
		Ok(self)
	}

	pub fn insert_catalog(&mut self, catalog: Catalog) -> Result<&mut Self, Error> {
		if self.tracks.is_some() {
			return Err(Error::TracksAlreadySet);
		}

		match &mut self.catalogs {
			Some(catalogs) => catalogs.push(catalog),
			None => self.catalogs = Some(vec![catalog]),
		}
		Ok(self)
	}

	pub fn encode(&self) -> Result<Vec<u8>, Error> {
		match serde_json::to_vec(&self) {
			Ok(v) => Ok(v),
			Err(err) => {
				log::error!("encode [MoqCatalog]: {}", err);
				Err(Error::External {
					krayt: "serde_json".to_string(),
					error: err.to_string(),
				})
			}
		}
	}
}

impl std::fmt::Display for MoqCatalog {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let mut out = format!(
			"MoqCatalog v{}, format: {} Version {}\n",
			self.version, self.streaming_format, self.streaming_format_version
		);
		if self.tracks.is_some() {
			out += &format!("containing {} tracks:\n", self.tracks.as_ref().unwrap().len());
			let (mut res, mut bitrate, mut mime, mut codec, mut name) = (0, 0, 0, 0, 0);
			for track in self.tracks.as_ref().unwrap().iter() {
				if let Some(params) = track.selection_params() {
					let width = params.width.unwrap_or_default();
					let height = params.height.unwrap_or_default();

					let res_len = width.checked_ilog10().unwrap_or(1) + height.checked_ilog10().unwrap_or(1) + 3;
					if res_len > res {
						res = res_len;
					}

					if let Some(sample) = params.sample_rate {
						let sample = sample / 1_000;
						let sample = sample.checked_ilog10().unwrap_or_default() + 5;
						if sample > res {
							res = sample;
						}
					}

					let br = params.bitrate.unwrap_or_default() / 1_000;
					let bitrate_len = br.checked_ilog10().unwrap_or(1) + 1;
					if bitrate_len > bitrate {
						bitrate = bitrate_len;
					}

					let mim = params.mime_type.clone().unwrap_or("no mime".to_string());
					let mime_len = mim.len();
					if mime_len > mime {
						mime = mime_len;
					}

					let code = params.codec.clone().unwrap_or("no codec".to_string());
					let codec_len = code.len();
					if codec_len > codec {
						codec = codec_len;
					}
				}

				let name_len = track.name.len();
				if name_len > name {
					name = name_len;
				}
			}
			for (i, track) in self.tracks.as_ref().unwrap().iter().enumerate() {
				let (res_str, mime_str, codec_str, br) = if let Some(params) = track.selection_params() {
					let res_str = match (params.width, params.height, params.sample_rate) {
						(Some(w), Some(h), None) => format!("{}x{}", w, h),
						(None, None, Some(s)) => format!("{} kbps", s / 1_000),
						_ => "-".to_string(),
					};
					let mime_str = params.mime_type.clone().unwrap_or("no mime".to_string());
					let codec_str = params.codec.clone().unwrap_or("no codec".to_string());
					let br = params.bitrate.unwrap_or(0) / 1_000;
					(res_str, mime_str, codec_str, br)
				} else {
					("0x0".to_string(), "no_mime".to_string(), "no codec".to_string(), 0)
				};
				out += &format!(
					"{i:>3}: {name:>name_width$}, {bitrate:>bitrate_width$} kbps {resolution:>resolution_width$} {codec:>codec_width$} {mime:>mime_width$}\n",
					name = track.name,
					name_width = name,
					bitrate = br,
					bitrate_width = bitrate as usize,
					resolution = res_str,
					resolution_width = res as usize,
					codec = codec_str,
					codec_width = codec,
					mime = mime_str,
					mime_width = mime,
				);
			}
		}
		if self.catalogs.is_some() {
			out += &format!("containing {} catalogs:\n", self.catalogs.as_ref().unwrap().len());
			for (i, catalog) in self.catalogs.as_ref().unwrap().iter().enumerate() {
				out += &format!("{i:3}: {}", catalog.name);
			}
		}
		write!(f, "{}", out)
	}
}

impl std::default::Default for MoqCatalog {
	fn default() -> Self {
		Self {
			version: VERSION.to_string(),
			streaming_format: STREAMING_FORMAT.to_string(),
			streaming_format_version: STREAMING_FORMAT_VERSION.to_string(),
			supports_delta_updates: None,
			common_track_fields: None,
			tracks: None,
			catalogs: None,
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
	/// Streaming Format
	///
	/// A number indicating the streaming format type.  Every MoQ Streaming
	/// Format normatively referencing this catalog format MUST register
	/// itself in the "MoQ Streaming Format Type" table.  See Section 5 for
	/// additional details.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-streaming-format)
	#[serde(rename = "streamingFormat")]
	streaming_format: String,

	/// Streaming Format Version
	///
	/// A string indicating the version of the streaming format to which this
	/// catalog applies.  The structure of the version string is defined by
	/// the streaming format.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-streaming-format-version)
	#[serde(rename = "streamingFormatVersion")]
	streaming_format_version: String,

	/// Supports Delta Updates
	///
	/// A boolean that if true indicates that the publisher MAY issue
	/// incremental (delta) updates - see Section 3.3.  If false or absent,
	/// then the publisher gaurantees that they will NOT issue any
	/// incremental updates and that any future updates to the catalog will
	/// be independent.  The default value is false.  This field MUST be
	/// present if its value is true, but may be omitted if the value is
	/// false.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-supports-delta-updates)
	#[serde(rename = "supportsDeltaUpdates")]
	supports_delta_updates: Option<bool>,

	/// Track Namespace
	///
	/// The name space under which the track name is defined.  See section
	/// 2.3 of [MoQTransport](https://datatracker.ietf.org/doc/html/draft-ietf-moq-transport-05).  
	/// The track namespace is optional.  If it is not declared within the
	/// Common Track Fields object or within a track, then each track MUST
	/// inherit the namespace of the catalog track.  A namespace declared
	/// in a track object or catalog object overwrites any inherited name
	/// space.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-track-namespace)
	namespace: Option<String>,

	/// Track Name
	///
	/// A string defining the name of the track.  See section 2.3 of
	/// [MoQTransport](https://datatracker.ietf.org/doc/html/draft-ietf-moq-transport-05).  
	/// Within the catalog, track names MUST be unique per namespace.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-track-name)
	name: String,
}

impl Catalog {
	pub fn new(name: &str) -> Self {
		Self {
			streaming_format: STREAMING_FORMAT.to_string(),
			streaming_format_version: STREAMING_FORMAT_VERSION.to_string(),
			supports_delta_updates: None,
			namespace: None,
			name: name.to_string(),
		}
	}

	pub fn enable_delta_updates(&mut self) -> &mut Self {
		self.supports_delta_updates = Some(true);
		self
	}

	pub fn disable_delta_updates(&mut self) -> &mut Self {
		self.supports_delta_updates = Some(false);
		self
	}

	pub fn supports_delta_updates(&self) -> Option<bool> {
		self.supports_delta_updates
	}

	pub fn set_namespace(&mut self, name: &str) -> &mut Self {
		self.namespace = Some(name.to_string());
		self
	}

	pub fn namespace(&self) -> Option<&String> {
		self.namespace.as_ref()
	}

	pub fn name(&self) -> &str {
		&self.name
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommonStructFields {
	/// Track Namespace
	///
	/// The name space under which the track name is defined.  See section
	/// 2.3 of [MoQTransport](https://datatracker.ietf.org/doc/html/draft-ietf-moq-transport-05).  
	/// The track namespace is optional.  If it is not declared within the
	/// Common Track Fields object or within a track, then each track MUST
	/// inherit the namespace of the catalog track.  A namespace declared
	/// in a track object or catalog object overwrites any inherited name
	/// space.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-track-namespace)
	namespace: Option<String>,

	/// Track Name
	///
	/// A string defining the name of the track.  See section 2.3 of
	/// [MoQTransport](https://datatracker.ietf.org/doc/html/draft-ietf-moq-transport-05).  
	/// Within the catalog, track names MUST be unique per namespace.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-track-name)
	name: String,

	/// Packaging
	///
	/// A string defining the type of payload encapsulation.  Allowed values
	/// are strings as defined in Table 3.
	///
	/// |Name|Value|Draft|
	/// |---|---|---|
	/// |CMAF|"cmaf"|See [CMAF]|
	/// |LOC|"loc"|See RFC XXXX|
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-packaging)
	packaging: Packaging,

	/// Track Label
	///
	/// A string defining a human-readable label for the track.  Examples
	/// might be "Overhead camera view" or "Deutscher Kommentar".  Note that
	/// the [JSON](https://www.rfc-editor.org/rfc/rfc8259)
	/// spec requires UTF-8 support by decoders.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-track-label)
	label: Option<String>,

	/// Render Group
	///
	/// An integer specifying a group of tracks which are designed to be
	/// rendered together.  Tracks with the same group number SHOULD be
	/// rendered simultaneously, are usually time-aligned and are designed to
	/// accompany one another.  A common example would be tying together
	/// audio and video tracks.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-render-group)
	#[serde(rename = "renderGroup")]
	render_group: Option<usize>,

	/// Alternate Group
	///
	/// An integer specifying a group of tracks which are alternate versions
	/// of one-another.  Alternate tracks represent the same media content,
	/// but differ in their selection properties.  Alternate tracks SHOULD
	/// have matching framerate Section 3.2.23 and media time sequences.  A
	/// subscriber typically subscribes to one track from a set of tracks
	/// specifying the same alternate group number.  A common example would
	/// be a set video tracks of the same content offered in alternate
	/// bitrates.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-alternate-group)
	#[serde(rename = "altGroup")]
	alt_group: Option<usize>,

	/// Initialization Data
	///
	/// A string holding Base64 [BASE64](https://www.rfc-editor.org/rfc/rfc4648)
	/// encoded initialization data for the track.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-initialization-data)
	#[serde(rename = "initData")]
	init_data: Option<String>, // use base64 lib

	/// Initialization Track
	///
	/// A string specifying the track name of another track which holds
	/// initialization data for the current track.  Initialization tracks
	/// MUST NOT be added to the tracks array Section 3.2.5.  They are
	/// referenced only via the initialization track field of the track which
	/// they initialize.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-initialization-track)
	#[serde(rename = "initTrack")]
	init_track: Option<String>,

	/// Selection Parameters
	///
	/// An object holding a series of name/value pairs which a subscriber can
	/// use to select tracks for subscription.  If present, the selection
	/// parameters object MUST NOT be empty.  Any selection parameters
	/// declared at the root level are inherited by all tracks.  A selection
	/// parameters object may exist at both the root and track level.  Any
	/// declaration of a selection parameter at the track level overrides the
	/// inherited root value.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-selection-parameters)
	#[serde(rename = "selectionParams")]
	selection_params: Option<SelectionParams>,
}

impl CommonStructFields {
	pub fn new(name: &str, packaging: Packaging) -> Self {
		Self {
			namespace: None,
			name: name.to_string(),
			packaging,
			label: None,
			render_group: None,
			alt_group: None,
			init_data: None,
			init_track: None,
			selection_params: None,
		}
	}

	pub fn set_namespace(&mut self, name: &str) -> &mut Self {
		self.namespace = Some(name.to_string());
		self
	}

	pub fn namespace(&self) -> Option<&String> {
		self.namespace.as_ref()
	}

	pub fn set_label(&mut self, label: &str) -> &mut Self {
		self.label = Some(label.to_string());
		self
	}

	pub fn set_alt_group(&mut self, alt: usize) -> &mut Self {
		self.alt_group = Some(alt);
		self
	}

	pub fn set_init_data(&mut self, init: &[u8]) -> &mut Self {
		let b64 = BASE64_STANDARD.encode(init);
		self.init_data = Some(b64);
		self
	}

	pub fn set_selection_params(&mut self, params: SelectionParams) -> &mut Self {
		self.selection_params = Some(params);
		self
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
	/// Track Namespace
	///
	/// The name space under which the track name is defined.  See section
	/// 2.3 of [MoQTransport](https://datatracker.ietf.org/doc/html/draft-ietf-moq-transport-05).  
	/// The track namespace is optional.  If it is not declared within the Common
	/// Track Fields object or within a track, then each track MUST inherit the
	/// namespace of the catalog track.  A namespace declared in a track object
	/// or catalog object overwrites any inherited name space.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-track-namespace)
	namespace: Option<String>,

	/// Track Name
	///
	/// A string defining the name of the track.  See section 2.3 of
	/// [MoQTransport](https://datatracker.ietf.org/doc/html/draft-ietf-moq-transport-05).  
	/// Within the catalog, track names MUST be unique per namespace.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-track-name)
	name: String,

	/// Packaging
	///
	/// A string defining the type of payload encapsulation.  Allowed values
	/// are strings as defined in Table 3.
	///
	/// |Name|Value|Draft|
	/// |---|---|---|
	/// |CMAF|"cmaf"|See [CMAF]|
	/// |LOC|"loc"|See RFC XXXX|
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-packaging)
	packaging: Packaging,

	/// Track Label
	///
	/// A string defining a human-readable label for the track.  Examples
	/// might be "Overhead camera view" or "Deutscher Kommentar".  Note that
	/// the [JSON](https://www.rfc-editor.org/rfc/rfc8259) spec requires UTF-8
	/// support by decoders.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-track-label)
	label: Option<String>,

	/// Render Group
	///
	/// An integer specifying a group of tracks which are designed to be
	/// rendered together.  Tracks with the same group number SHOULD be
	/// rendered simultaneously, are usually time-aligned and are designed to
	/// accompany one another.  A common example would be tying together
	/// audio and video tracks.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-render-group)
	#[serde(rename = "renderGroup")]
	render_group: Option<usize>,

	/// Alternate Group
	///
	/// An integer specifying a group of tracks which are alternate versions
	/// of one-another.  Alternate tracks represent the same media content,
	/// but differ in their selection properties.  Alternate tracks SHOULD
	/// have matching framerate Section 3.2.23 and media time sequences.  A
	/// subscriber typically subscribes to one track from a set of tracks
	/// specifying the same alternate group number.  A common example would
	/// be a set video tracks of the same content offered in alternate
	/// bitrates.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-alternate-group)
	#[serde(rename = "altGroup")]
	alt_group: Option<usize>,

	/// Initialization Data
	///
	/// A string holding Base64 [BASE64](https://www.rfc-editor.org/rfc/rfc4648)
	/// encoded initialization data for the track.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-initialization-data)
	#[serde(rename = "initData")]
	init_data: Option<String>, // use base64 lib

	/// Initialization Track
	///
	/// A string specifying the track name of another track which holds
	/// initialization data for the current track.  Initialization tracks
	/// MUST NOT be added to the tracks array Section 3.2.5.  They are
	/// referenced only via the initialization track field of the track which
	/// they initialize.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-initialization-track)
	#[serde(rename = "initTrack")]
	init_track: Option<String>,

	/// Selection Parameters
	///
	/// An object holding a series of name/value pairs which a subscriber can
	/// use to select tracks for subscription.  If present, the selection
	/// parameters object MUST NOT be empty.  Any selection parameters
	/// declared at the root level are inherited by all tracks.  A selection
	/// parameters object may exist at both the root and track level.  Any
	/// declaration of a selection parameter at the track level overrides the
	/// inherited root value.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-selection-parameters)
	#[serde(rename = "selectionParams")]
	selection_params: Option<SelectionParams>,

	/// Dependencies
	///
	/// Certain tracks may depend on other tracks for decoding.  Dependencies
	/// holds an array of track names Section 3.2.10 on which the current
	/// track is dependent.  Since only the track name is signaled, the
	/// namespace of the dependencies is assumed to match that of the track
	/// declaring the dependencies.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-dependencies)
	depends: Option<Vec<String>>,

	/// Temporal ID
	///
	/// A number identifying the temporal layer/sub-layer encoding of the
	/// track, starting with 0 for the base layer, and increasing with higher
	/// temporal fidelity.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-temporal-id)
	#[serde(rename = "temporalId")]
	temporal_id: Option<usize>,

	/// Spatial ID
	///
	/// A number identifying the spatial layer encoding of the track,
	/// starting with 0 for the base layer, and increasing with higher
	/// fidelity.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-spatial-id)
	#[serde(rename = "spatialId")]
	spatial_id: Option<usize>,
}

impl Track {
	pub fn new(name: &str, packaging: Packaging) -> Self {
		Self {
			namespace: None,
			name: name.to_string(),
			packaging,
			label: None,
			render_group: None,
			alt_group: None,
			init_data: None,
			init_track: None,
			selection_params: None,
			depends: None,
			temporal_id: None,
			spatial_id: None,
		}
	}

	pub fn set_namespace(&mut self, name: &str) -> &mut Self {
		self.namespace = Some(name.to_string());
		self
	}

	pub fn namespace(&self) -> Option<&String> {
		self.namespace.as_ref()
	}

	pub fn set_label(&mut self, label: &str) -> &mut Self {
		self.label = Some(label.to_string());
		self
	}

	pub fn set_alt_group(&mut self, alt: usize) -> &mut Self {
		self.alt_group = Some(alt);
		self
	}

	pub fn set_init_data(&mut self, init: &[u8]) -> &mut Self {
		let b64 = BASE64_STANDARD.encode(init);
		self.init_data = Some(b64);
		self
	}

	pub fn set_selection_params(&mut self, params: SelectionParams) -> &mut Self {
		self.selection_params = Some(params);
		self
	}

	pub fn selection_params(&self) -> Option<&SelectionParams> {
		self.selection_params.as_ref()
	}
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum Packaging {
	#[serde(rename = "cmaf")]
	#[default]
	CMAF,

	#[serde(rename = "loc")]
	LOC,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SelectionParams {
	/// Codec
	///
	/// A string defining the codec used to encode the track.  For LOC
	/// packaged content, the string codec registrations are defined in Sect
	/// 3 and Section 4 of [WEBCODECS-CODEC-REGISTRY](https://www.w3.org/TR/webcodecs-codec-registry/).  
	/// For CMAF packaged content, the string codec registrations are defined
	/// in XXX.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-codec)
	codec: Option<String>,

	/// Mimetype
	///
	/// A string defining the mime type [MIME](https://www.rfc-editor.org/rfc/rfc6838)
	/// of the track.  This parameter is typically supplied with
	/// CMAF packaged content.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-mimetype)
	#[serde(rename = "mimeType")]
	mime_type: Option<String>,

	/// Framerate
	///
	/// A number defining the framerate of the track, expressed as frames per
	/// second.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-framerate)
	framerate: Option<u64>,

	/// Bitrate
	///
	/// A number defining the bitrate of track, expressed in bits second.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-bitrate)
	bitrate: Option<u64>,

	/// Width
	///
	/// A number expressing the encoded width of the track content in pixels.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-width)
	width: Option<u16>,

	/// Height
	///
	/// A number expressing the encoded height of the video frames in pixels.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-height)
	height: Option<u16>,

	/// Audio Sample Rate
	///
	/// The number of audio frame samples per second.  This property SHOULD
	/// only accompany audio codecs.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-audio-sample-rate)
	#[serde(rename = "samplerate")]
	sample_rate: Option<u16>,

	/// Channel Config
	///
	/// A string specifying the audio channel configuration.  This property
	/// SHOULD only accompany audio codecs.  A string is used in order to
	/// provide the flexibility to describe complex channel configurations
	/// for multi-channel and Next Generation Audio schemas.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-channel-configuration)
	#[serde(rename = "channelConfig")]
	channel_config: Option<String>,

	/// Display Width
	///
	/// A number expressing the intended display width of the track content
	/// in pixels.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-display-width)
	#[serde(rename = "displayWidth")]
	display_width: Option<u16>,

	/// Display Height
	///
	/// A number expressing the intended display height of the track content
	/// in pixels.
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-display-height)
	#[serde(rename = "displayHeight")]
	display_height: Option<u16>,

	/// Language
	///
	/// A string defining the dominant language of the track.  The string
	/// MUST be one of the standard Tags for Identifying Languages as defined
	/// by [LANG](https://www.rfc-editor.org/rfc/rfc5646).
	///
	/// Source: [draft-ietf-moq-catalogformat-01](https://www.ietf.org/archive/id/draft-ietf-moq-catalogformat-01.html#name-language)
	#[serde(rename = "lang")]
	language: Option<String>,
}

impl SelectionParams {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn set_codec(&mut self, codec: &str) -> &mut Self {
		// TODO: force only values from webcodec registry?
		self.codec = Some(codec.to_string());
		self
	}

	pub fn set_mime_type(&mut self, mime: &str) -> Result<&mut Self, Error> {
		let mime = match mime::Mime::from_str(mime) {
			Ok(v) => v,
			Err(err) => {
				log::error!("parse mime type: {}", err);
				return Err(Error::External {
					krayt: "mime".to_string(),
					error: err.to_string(),
				});
			}
		};

		self.mime_type = Some(mime.to_string());
		Ok(self)
	}

	pub fn set_framerate(&mut self, framerate: u64) -> &mut Self {
		self.framerate = Some(framerate);
		self
	}

	pub fn set_bitrate(&mut self, bitrate: u64) -> &mut Self {
		self.bitrate = Some(bitrate);
		self
	}

	pub fn set_width(&mut self, width: u16) -> &mut Self {
		self.width = Some(width);
		self
	}

	pub fn set_height(&mut self, height: u16) -> &mut Self {
		self.height = Some(height);
		self
	}

	pub fn set_sample_rate(&mut self, sample_rate: u16) -> &mut Self {
		// TODO make sure self.codec is audio codec
		self.sample_rate = Some(sample_rate);
		self
	}

	pub fn set_language(&mut self, lang: &str) -> Result<&mut Self, Error> {
		let tag = match language_tags::LanguageTag::parse(lang) {
			Ok(v) => v,
			Err(err) => {
				log::error!("parse language tag: {}", err);
				return Err(Error::External {
					krayt: "language_tags".to_string(),
					error: err.to_string(),
				});
			}
		};

		self.language = Some(tag.to_string());
		Ok(self)
	}
}
