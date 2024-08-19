use bytes::Buf;
use mp4::ReadBox;
use std::collections::HashMap;

use crate::dash::settings::Setting;

use super::Error;

const LABEL: &str = "Dash MoQ";

pub type RepID = usize;

// TODO see catalog print, something is off with 4k

pub struct Publisher {
	buf: HashMap<RepID, bytes::BytesMut>,

	settings: super::Settings<std::path::PathBuf>,
	tracks: HashMap<RepID, Track>,
	broadcast: moq_transport::serve::TracksWriter,

	catalog_broadcast: moq_transport::serve::GroupsWriter,
	catalog: moq_catalog::MoqCatalog,

	ftyp: HashMap<RepID, bytes::Bytes>,
	moov: HashMap<RepID, mp4::MoovBox>,

	prft: HashMap<RepID, bytes::Bytes>,
}

impl Publisher {
	pub fn new(
		mut broadcast: moq_transport::serve::TracksWriter,
		settings: super::Settings<std::path::PathBuf>,
	) -> Result<Self, Error> {
		let Some(catalog_broadcast) = broadcast.create(".catalog") else {
			println!("Error: failed to create catalog track");
			return Err(Error::Crate(
				"moq_transport".to_string(),
				"broadcast closed".to_string(),
			));
		};
		let catalog_broadcast = match catalog_broadcast.groups() {
			Ok(c) => c,
			Err(e) => {
				println!("Error: {}", e);
				return Err(Error::Crate("moq_transport".to_string(), e.to_string()));
			}
		};
		let mut catalog = moq_catalog::MoqCatalog::new();

		let mut csf = moq_catalog::CommonStructFields::new("", moq_catalog::Packaging::CMAF);
		csf.set_alt_group(1)
			.set_label(LABEL)
			.set_namespace(&broadcast.namespace);

		catalog.enable_delta_updates().set_common_track_fields(csf);

		Ok(Self {
			buf: HashMap::new(),
			settings,
			tracks: HashMap::new(),
			broadcast,
			catalog_broadcast,
			catalog,
			ftyp: HashMap::new(),
			moov: HashMap::new(),
			prft: HashMap::new(),
		})
	}

	pub fn publish(&mut self, rep_id: RepID, data: &[u8]) -> Result<(), Error> {
		let buf = self.get_mut(rep_id);
		buf.extend_from_slice(data);

		self.parse(rep_id)?;

		Ok(())
	}

	fn parse(&mut self, rep_id: RepID) -> Result<(), Error> {
		while self.parse_atom(rep_id)? {}
		Ok(())
	}

	fn parse_atom(&mut self, rep_id: RepID) -> Result<bool, Error> {
		let buf = self.get_mut(rep_id);
		let Some(atom) = next_atom(buf)? else {
			return Ok(false);
		};

		let mut reader = std::io::Cursor::new(&atom);
		let header = match mp4::BoxHeader::read(&mut reader) {
			Ok(h) => h,
			Err(e) => {
				println!("Error: {}", e);
				return Err(Error::Crate("mp4".to_string(), e.to_string()));
			}
		};

		match header.name {
			n if n.to_string() == "prft" => {
				self.prft.insert(rep_id, atom);
			}
			mp4::BoxType::FtypBox => {
				if self.ftyp.get(&rep_id).is_some() {
					println!("Error: multiple ftyp on track {rep_id}");
					return Err(Error::Crate("mp4".to_string(), "multiple ftyp on track".to_string()));
				}

				self.ftyp.insert(rep_id, atom);
			}
			mp4::BoxType::MoovBox => {
				if self.moov.get(&rep_id).is_some() {
					println!("Error: multiple moov on track {rep_id}");
					return Err(Error::Crate("mp4".to_string(), "multiple moov on track".to_string()));
				}

				let moov = match mp4::MoovBox::read_box(&mut reader, header.size) {
					Ok(m) => m,
					Err(e) => {
						println!("Error: {}", e);
						return Err(Error::Crate("mp4".to_string(), e.to_string()));
					}
				};

				self.setup(&moov, atom, rep_id)?;
				self.moov.insert(rep_id, moov);
			}
			mp4::BoxType::MoofBox => {
				let moof = match mp4::MoofBox::read_box(&mut reader, header.size) {
					Ok(m) => m,
					Err(e) => {
						println!("Error: {}", e);
						return Err(Error::Crate("mp4".to_string(), e.to_string()));
					}
				};

				let fragment = Fragment::new(moof)?;

				let Some(track) = self.tracks.get_mut(&rep_id) else {
					println!("Error: track {rep_id} not available");
					return Err(Error::Missing);
				};

				if fragment.keyframe && track.handler == mp4::TrackType::Video {
					track.end_group();
				}

				if let Err(e) = track.header(atom, fragment) {
					println!("Error: {}", e);
					return Err(Error::Crate("moq".to_string(), e.to_string()));
				}
			}
			mp4::BoxType::MdatBox => {
				let Some(track) = self.tracks.get_mut(&rep_id) else {
					println!("Error: track {rep_id} not available");
					return Err(Error::Missing);
				};

				if let Some(prft) = self.prft.get(&rep_id) {
					let mut data = atom.clone().to_vec();
					data.extend_from_slice(prft);
					if let Err(e) = track.data(data.into()) {
						println!("Error: {}", e);
						return Err(Error::Crate("moq".to_string(), e.to_string()));
					}
				} else if let Err(e) = track.data(atom) {
					println!("Error: {}", e);
					return Err(Error::Crate("moq".to_string(), e.to_string()));
				}
			}
			x => {
				// println!("Other: {x}");
			}
		}

		Ok(true)
	}

	fn setup(&mut self, moov: &mp4::MoovBox, raw: bytes::Bytes, rep_id: RepID) -> Result<(), Error> {
		if moov.traks.len() != 1 {
			println!("Error: multiple tracks in moov");
			return Err(Error::Crate("mp4".to_string(), "multiple tracks in moov".to_string()));
		}

		let Some(settings) = self.settings.get_rep(rep_id) else {
			println!("Error: missing Settings for rep {}", rep_id);
			return Err(Error::Missing);
		};
		let track_name = match settings {
			Setting::Audio(ref a) => a.name.clone(),
			Setting::Video(ref v) => v.name.clone(),
		};

		let trak = &moov.traks[0];
		let id = trak.tkhd.track_id;
		let timescale = track_timescale(moov, id);
		let handler = match (&trak.mdia.hdlr.handler_type).try_into() {
			Ok(h) => h,
			Err(_) => {
				println!("Error: cannot convert handler type");
				return Err(Error::Crate(
					"mp4".to_string(),
					"cannot convert handler type".to_string(),
				));
			}
		};
		let Some(track) = self.broadcast.create(&track_name) else {
			println!("Error: failed to create catalog track");
			return Err(Error::Crate(
				"moq_transport".to_string(),
				"broadcast closed".to_string(),
			));
		};
		let track = Track::new(track, handler, timescale);
		self.tracks.insert(rep_id, track);

		let Some(init) = self.ftyp.get(&rep_id) else {
			println!("Error: missing ftyp for track {rep_id}");
			return Err(Error::Crate("mp4".to_string(), "missing ftyp for track".to_string()));
		};
		let mut init = init.to_vec();
		init.extend_from_slice(&raw);

		let mut catalog_track = moq_catalog::Track::new(&track_name, moq_catalog::Packaging::CMAF);
		let mut params = moq_catalog::SelectionParams::new();

		let stsd = &trak.mdia.minf.stbl.stsd;
		if let Some(avc1) = &stsd.avc1 {
			let profile = avc1.avcc.avc_profile_indication;
			let constraints = avc1.avcc.profile_compatibility; // Not 100% certain here, but it's 0x00 on my current test video
			let level = avc1.avcc.avc_level_indication;

			let width = avc1.width;
			let height = avc1.height;

			let codec = rfc6381_codec::Codec::avc1(profile, constraints, level);
			let codec_str = codec.to_string();

			let bitrate = match settings {
				Setting::Video(v) => v.bitrate,
				_ => 0,
			};
			// let bitrate = if let Setting::Video(s) = settings { s.bitrate } else { 0 };

			params
				.set_height(height)
				.set_width(width)
				.set_codec(&codec_str)
				.set_bitrate(bitrate);

			if let Err(e) = params.set_mime_type("video/mp4") {
				println!("Error: {}", e);
				return Err(Error::Crate("moq_catalog".to_string(), e.to_string()));
			}
		} else if let Some(_hev1) = &stsd.hev1 {
			return Err(Error::Crate("pub".to_string(), "HEVC not yet supported".to_string()));
		} else if let Some(mp4a) = &stsd.mp4a {
			let desc = if let Some(d) = &mp4a.esds.as_ref() {
				&d.es_desc.dec_config
			} else {
				println!("Error: missing mp4a description");
				return Err(Error::Missing);
			};

			let codec_str = format!("mp4a.{:02x}.{}", desc.object_type_indication, desc.dec_specific.profile);

			params.set_codec(&codec_str).set_sample_rate(mp4a.samplerate.value());

			if let Err(e) = params.set_mime_type("audio/mp4") {
				println!("Error: {}", e);
				return Err(Error::Crate("moq_catalog".to_string(), e.to_string()));
			}

			let bitrate = core::cmp::max(desc.max_bitrate, desc.avg_bitrate);
			if bitrate > 0 {
				params.set_bitrate(bitrate as u64);
			}
		} else if let Some(_vp09) = &stsd.vp09 {
			return Err(Error::Crate("pub".to_string(), "VP9 not yet supported".to_string()));
		} else {
			return Err(Error::Crate("pub".to_string(), "unknown codec".to_string()));
		}

		catalog_track
			.set_selection_params(params)
			.set_init_data(&init)
			.set_label(&track_name);

		if let Err(e) = self.catalog.insert_track(catalog_track) {
			println!("Error: {}", e);
			return Err(Error::Crate("moq_catalog".to_string(), e.to_string()));
		}

		log::info!("published catalog");
		println!("{}", self.catalog);

		let buf = match self.catalog.encode() {
			Ok(b) => b,
			Err(e) => {
				println!("Error: {}", e);
				return Err(Error::Crate("moq_catalog".to_string(), e.to_string()));
			}
		};

		// Create a single fragment for the segment.
		match self.catalog_broadcast.append(0) {
			Ok(mut g) => {
				if let Err(e) = g.write(buf.into()) {
					println!("Error: {}", e);
					return Err(Error::Crate("moq".to_string(), e.to_string()));
				}
			}
			Err(e) => {
				println!("Error: {}", e);
				return Err(Error::Crate("moq".to_string(), e.to_string()));
			}
		}

		Ok(())
	}

	fn get_mut(&mut self, key: RepID) -> &mut bytes::BytesMut {
		// if key is not present, insert new entry
		self.buf.entry(key).or_default();

		// return mutable reference
		self.buf.get_mut(&key).unwrap()
	}
}

fn next_atom<B: bytes::Buf>(buf: &mut B) -> Result<Option<bytes::Bytes>, Error> {
	let mut peek = std::io::Cursor::new(buf.chunk());

	if peek.remaining() < 8 {
		if buf.remaining() != buf.chunk().len() {
			// TODO figure out a way to peek at the first 8 bytes
			println!("TODO: vectored Buf not yet supported");
			return Err(Error::Other);
		}

		return Ok(None);
	}

	// Convert the first 4 bytes into the size.
	let size = peek.get_u32();
	let _type = peek.get_u32();

	let size = match size {
		// Runs until the end of the file.
		0 => {
			println!("TODO: unsupported EOF atom");
			return Err(Error::Other);
		}

		// The next 8 bytes are the extended size to be used instead.
		1 => {
			let size_ext = peek.get_u64();

			if size_ext < 16 {
				println!("impossible extended box size: {}", size_ext);
				return Err(Error::Other);
			}
			size_ext as usize
		}

		2..=7 => {
			println!("impossible box size: {}", size);
			return Err(Error::Other);
		}

		size => size as usize,
	};

	if buf.remaining() < size {
		return Ok(None);
	}

	let atom = buf.copy_to_bytes(size);

	Ok(Some(atom))
}

struct Track {
	// The track we're producing
	track: moq_transport::serve::GroupsWriter,

	// The current segment
	current: Option<moq_transport::serve::GroupWriter>,

	// The number of units per second.
	timescale: u64,

	// The type of track, ex. "vide" or "soun"
	handler: mp4::TrackType,
}

impl Track {
	fn new(track: moq_transport::serve::TrackWriter, handler: mp4::TrackType, timescale: u64) -> Self {
		Self {
			track: track.groups().unwrap(),
			current: None,
			timescale,
			handler,
		}
	}

	pub fn header(&mut self, raw: bytes::Bytes, fragment: Fragment) -> Result<(), Error> {
		if let Some(current) = self.current.as_mut() {
			// Use the existing segment
			if let Err(e) = current.write(raw) {
				println!("Error: {}", e);
				return Err(Error::Crate("moq".to_string(), e.to_string()));
			}
			return Ok(());
		}

		// Otherwise make a new segment

		// Compute the timestamp in milliseconds.
		// Overflows after 583 million years, so we're fine.
		let timestamp: u32 = match fragment.timestamp(self.timescale).as_millis().try_into() {
			Ok(t) => t,
			Err(e) => {
				println!("Error: {}", e);
				return Err(Error::Crate("moq".to_string(), e.to_string()));
			}
		};

		let Some(priority) = u32::MAX.checked_sub(timestamp) else {
			println!("Error: priority too large");
			return Err(Error::Crate("moq".to_string(), "priority too large".to_string()));
		};

		// Create a new segment.
		let mut segment = match self.track.append(priority.into()) {
			Ok(s) => s,
			Err(e) => {
				println!("Error: {}", e);
				return Err(Error::Crate("moq".to_string(), e.to_string()));
			}
		};

		// Write the fragment in it's own object.
		if let Err(e) = segment.write(raw) {
			println!("Error: {}", e);
			return Err(Error::Crate("moq".to_string(), e.to_string()));
		}

		// Save for the next iteration
		self.current = Some(segment);

		Ok(())
	}

	pub fn data(&mut self, raw: bytes::Bytes) -> Result<(), Error> {
		let Some(segment) = self.current.as_mut() else {
			println!("Error: missing current fragment");
			return Err(Error::Crate("moq".to_string(), "missing current fragment".to_string()));
		};
		if let Err(e) = segment.write(raw) {
			println!("Error: {}", e);
			return Err(Error::Crate("moq".to_string(), e.to_string()));
		}

		Ok(())
	}

	pub fn end_group(&mut self) {
		self.current = None;
	}
}

struct Fragment {
	// The track for this fragment.
	track: u32,

	// The timestamp of the first sample in this fragment, in timescale units.
	timestamp: u64,

	// True if this fragment is a keyframe.
	keyframe: bool,
}

impl Fragment {
	fn new(moof: mp4::MoofBox) -> Result<Self, Error> {
		// We can't split the mdat atom, so this is impossible to support
		if moof.trafs.len() != 1 {
			println!("Error: multiple tracks per moof atom");
			return Err(Error::Crate(
				"mp4".to_string(),
				"multiple tracks per moof atom".to_string(),
			));
		}

		let track = moof.trafs[0].tfhd.track_id;

		// Parse the moof to get some timing information to sleep.
		let timestamp = sample_timestamp(&moof).expect("couldn't find timestamp");

		// Detect if we should start a new segment.
		let keyframe = sample_keyframe(&moof);

		Ok(Self {
			track,
			timestamp,
			keyframe,
		})
	}

	// Convert from timescale units to a duration.
	fn timestamp(&self, timescale: u64) -> std::time::Duration {
		std::time::Duration::from_millis(1000 * self.timestamp / timescale)
	}
}

fn sample_timestamp(moof: &mp4::MoofBox) -> Option<u64> {
	Some(moof.trafs.first()?.tfdt.as_ref()?.base_media_decode_time)
}

fn sample_keyframe(moof: &mp4::MoofBox) -> bool {
	for traf in &moof.trafs {
		// TODO trak default flags if this is None
		let default_flags = traf.tfhd.default_sample_flags.unwrap_or_default();
		let trun = match &traf.trun {
			Some(t) => t,
			None => return false,
		};

		for i in 0..trun.sample_count {
			let mut flags = match trun.sample_flags.get(i as usize) {
				Some(f) => *f,
				None => default_flags,
			};

			if i == 0 && trun.first_sample_flags.is_some() {
				flags = trun.first_sample_flags.unwrap();
			}

			// https://chromium.googlesource.com/chromium/src/media/+/master/formats/mp4/track_run_iterator.cc#177
			let keyframe = (flags >> 24) & 0x3 == 0x2; // kSampleDependsOnNoOther
			let non_sync = (flags >> 16) & 0x1 == 0x1; // kSampleIsNonSyncSample

			if keyframe && !non_sync {
				return true;
			}
		}
	}

	false
}

// Find the timescale for the given track.
fn track_timescale(moov: &mp4::MoovBox, track_id: u32) -> u64 {
	let trak = moov
		.traks
		.iter()
		.find(|trak| trak.tkhd.track_id == track_id)
		.expect("failed to find trak");

	trak.mdia.mdhd.timescale as u64
}
