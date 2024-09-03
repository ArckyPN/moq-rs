use crate::{Error, Packaging, STREAMING_FORMAT, STREAMING_FORMAT_VERSION, VERSION};

use std::str::FromStr;

use base64::prelude::*;
use serde::{Deserialize, Serialize};

#[mixin::declare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct R {
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

#[mixin::expand]
impl R {
	pub fn version(&self) -> &str {
		&self.version
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

	pub fn get_track(&self, name: &str) -> Option<Track> {
		self.tracks?
			.iter()
			.find(|track| track.name == name.to_string())
			.cloned()
	}

	pub fn get_track_ref(&self, name: &str) -> Option<&Track> {
		self.tracks?.iter().find(|track| track.name == name.to_string())
	}

	pub fn get_track_mut(&mut self, name: &str) -> Option<&mut Track> {
		self.tracks?
			.iter_mut()
			.find(|track| track.name == name.to_string())
			.as_deref_mut()
	}

	pub fn remove_track(&mut self, name: &str) -> &mut Self {
		// TODO replace self.tracks
		self.tracks
			.map(|tracks| tracks.iter().filter(|track| track.name != name.to_string()));
		self
	}

	pub fn get_catalog(&self, name: &str) -> Option<Catalog> {
		self.catalogs?
			.iter()
			.find(|catalog| catalog.name == name.to_string())
			.cloned()
	}

	pub fn get_catalog_ref(&self, name: &str) -> Option<&Catalog> {
		self.catalogs?.iter().find(|catalog| catalog.name == name.to_string())
	}

	pub fn get_catalog_mut(&mut self, name: &str) -> Option<&mut Catalog> {
		self.catalogs?
			.iter_mut()
			.find(|catalog| catalog.name == name.to_string())
			.as_deref_mut()
	}

	pub fn remove_catalog(&mut self, name: &str) -> &mut Self {
		// TODO replace self.catalogs
		self.catalogs
			.map(|catalogs| catalogs.iter().filter(|catalog| catalog.name != name.to_string()));
		self
	}

	pub fn tracks(&self) -> Option<&[Track]> {
		self.tracks.as_deref()
	}

	pub fn tracks_mut(&mut self) -> Option<&mut [Track]> {
		self.tracks.as_deref_mut()
	}

	pub fn tracks_len(&self) -> Option<usize> {
		Some(self.tracks?.len())
	}

	pub fn catalogs(&self) -> Option<&[Catalog]> {
		self.catalogs.as_deref()
	}

	pub fn catalogs_mut(&mut self) -> Option<&mut [Catalog]> {
		self.catalogs.as_deref_mut()
	}

	pub fn catalogs_len(&self) -> Option<usize> {
		Some(self.catalogs?.len())
	}
}

#[mixin::declare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RC {
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
}

#[mixin::expand]
impl RC {
	pub fn streaming_format(&self) -> &str {
		&self.streaming_format
	}

	pub fn streaming_format_version(&self) -> &str {
		&self.streaming_format_version
	}

	pub fn enable_delta_updates(&mut self) -> &mut Self {
		self.supports_delta_updates = Some(true);
		self
	}

	pub fn disable_delta_updates(&mut self) -> &mut Self {
		self.supports_delta_updates = Some(false);
		self
	}

	pub fn supports_delta_updates(&self) -> bool {
		self.supports_delta_updates.unwrap_or(false)
	}
}

#[mixin::declare]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[allow(clippy::upper_case_acronyms)]
pub struct TFC {
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

#[mixin::expand]
impl TFC {
	pub fn set_namespace(&mut self, name: &str) -> &mut Self {
		self.namespace = Some(name.to_string());
		self
	}

	pub fn namespace(&self) -> Option<&str> {
		self.namespace.as_ref().map(|x| x.as_str())
	}

	pub fn name(&self) -> &str {
		&self.name
	}
}

#[mixin::declare]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TF {
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

#[mixin::expand]
impl TF {
	pub fn packaging(&self) -> &Packaging {
		&self.packaging
	}

	pub fn label(&self) -> Option<&str> {
		self.label.as_ref().map(|x| x.as_str())
	}

	pub fn set_label(&mut self, label: &str) -> &mut Self {
		self.label = Some(label.to_string());
		self
	}

	pub fn render_group(&self) -> Option<usize> {
		self.render_group
	}

	pub fn set_render_group(&mut self, alt_group: usize) -> &mut Self {
		self.render_group = Some(alt_group);
		self
	}

	pub fn alt_group(&self) -> Option<usize> {
		self.alt_group
	}

	pub fn set_alt_group(&mut self, alt_group: usize) -> &mut Self {
		self.alt_group = Some(alt_group);
		self
	}

	pub fn init_data(&self) -> Option<&str> {
		self.init_data.as_ref().map(|x| x.as_str())
	}

	pub fn set_init_data(&mut self, init_data: &str) -> &mut Self {
		let b64 = BASE64_STANDARD.encode(init_data);
		self.init_data = Some(b64);
		self
	}

	pub fn init_track(&self) -> Option<&str> {
		self.init_track.as_ref().map(|x| x.as_str())
	}

	pub fn set_init_track(&mut self, track: &str) -> &mut Self {
		self.init_track = Some(track.to_string());
		self
	}

	pub fn selection_params(&self) -> Option<&SelectionParams> {
		self.selection_params.as_ref()
	}

	pub fn selection_params_mut(&mut self) -> Option<&mut SelectionParams> {
		self.selection_params.as_mut()
	}
}

#[mixin::declare]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct T {
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

#[mixin::expand]
impl T {
	pub fn depends(&self) -> Option<&[String]> {
		self.depends.as_deref()
	}

	pub fn set_depends(&mut self, depends: &[String]) -> &mut Self {
		self.depends = Some(depends.to_vec());
		self
	}

	pub fn insert_depends(&mut self, depends: &str) -> &mut Self {
		if self.depends.is_none() {
			self.depends = Some(Vec::new());
		}

		self.depends.unwrap().push(depends.to_string());
		self
	}

	pub fn temporal_id(&self) -> Option<usize> {
		self.temporal_id
	}

	pub fn set_temporal_id(&mut self, temporal_id: usize) -> &mut Self {
		self.temporal_id = Some(temporal_id);
		self
	}

	pub fn spatial_id(&self) -> Option<usize> {
		self.spatial_id
	}

	pub fn set_spatial_id(&mut self, spatial_id: usize) -> &mut Self {
		self.spatial_id = Some(spatial_id);
		self
	}
}

#[mixin::declare]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct S {
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

#[mixin::expand]
impl S {
	pub fn codec(&mut self) -> Option<&str> {
		self.codec.as_ref().map(|x| x.as_str())
	}

	pub fn set_codec(&mut self, codec: &str) -> &mut Self {
		// TODO: force only values from webcodec registry?
		self.codec = Some(codec.to_string());
		self
	}

	pub fn mime_type(&mut self) -> Option<&str> {
		self.mime_type.as_ref().map(|x| x.as_str())
	}

	pub fn set_mime_type(&mut self, mime: &str) -> Result<&mut Self, Error> {
		let mime = match mime::Mime::from_str(mime) {
			core::result::Result::Ok(v) => v,
			core::result::Result::Err(err) => {
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

	pub fn framerate(&self) -> Option<u64> {
		self.framerate
	}

	pub fn set_framerate(&mut self, framerate: u64) -> &mut Self {
		self.framerate = Some(framerate);
		self
	}

	pub fn bitrate(&self) -> Option<u64> {
		self.bitrate
	}

	pub fn set_bitrate(&mut self, bitrate: u64) -> &mut Self {
		self.bitrate = Some(bitrate);
		self
	}

	pub fn width(&self) -> Option<u16> {
		self.width
	}

	pub fn set_width(&mut self, width: u16) -> &mut Self {
		self.width = Some(width);
		self
	}

	pub fn height(&self) -> Option<u16> {
		self.height
	}

	pub fn set_height(&mut self, height: u16) -> &mut Self {
		self.height = Some(height);
		self
	}

	pub fn display_width(&self) -> Option<u16> {
		self.display_width
	}

	pub fn set_display_width(&mut self, width: u16) -> &mut Self {
		self.display_width = Some(width);
		self
	}

	pub fn display_height(&self) -> Option<u16> {
		self.display_height
	}

	pub fn set_display_height(&mut self, height: u16) -> &mut Self {
		self.display_height = Some(height);
		self
	}

	pub fn sample_rate(&self) -> Option<u16> {
		self.sample_rate
	}

	pub fn set_sample_rate(&mut self, sample_rate: u16) -> &mut Self {
		// TODO make sure self.codec is audio codec
		self.sample_rate = Some(sample_rate);
		self
	}

	pub fn language(&self) -> Option<&str> {
		self.language.as_ref().map(|x| x.as_str())
	}

	pub fn set_language(&mut self, lang: &str) -> Result<&mut Self, Error> {
		let tag = match language_tags::LanguageTag::parse(lang) {
			core::result::Result::Ok(v) => v,
			core::result::Result::Err(err) => {
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

// FIXME this error occurs once a fn in impl returns a Result
#[mixin::insert(R, RC)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoqCatalog {}

impl MoqCatalog {
	pub fn new() -> Self {
		Self::default()
	}
}

impl std::default::Default for MoqCatalog {
	fn default() -> Self {
		Self {
			streaming_format: STREAMING_FORMAT.to_string(),
			streaming_format_version: STREAMING_FORMAT_VERSION.to_string(),
			supports_delta_updates: None,
			version: VERSION.to_string(),
			common_track_fields: None,
			tracks: None,
			catalogs: None,
		}
	}
}

#[mixin::insert(T, TF, TFC)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {}

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
}

#[mixin::insert(RC, TFC)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {}

impl Catalog {
	pub fn new(name: &str) -> Self {
		Self {
			namespace: None,
			name: name.to_string(),
			streaming_format: STREAMING_FORMAT.to_string(),
			streaming_format_version: STREAMING_FORMAT_VERSION.to_string(),
			supports_delta_updates: None,
		}
	}
}

#[mixin::insert(TF, TFC)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommonStructFields {}

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
}

// FIXME this error occurs once a fn in impl returns a Result
#[mixin::insert(S)]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SelectionParams {}

impl SelectionParams {
	pub fn new() -> Self {
		Self::default()
	}
}
