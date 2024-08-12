use anyhow::Context;
use bytes::Buf;
use mp4::ReadBox;
use std::collections::HashMap;

use super::settings::Settings;

pub type RepID = String;

pub struct Dash {
	settings: Settings,
	tracks: HashMap<RepID, Track>,
	broadcast: moq_transport::serve::TracksWriter,

	catalog_pub: moq_transport::serve::GroupsWriter,
	catalog: moq_catalog::MoqCatalog,

	ftyp: HashMap<RepID, bytes::Bytes>,
	moov: HashMap<RepID, mp4::MoovBox>,

	prft: HashMap<RepID, bytes::Bytes>,
}

impl Dash {
	pub fn new(mut broadcast: moq_transport::serve::TracksWriter, settings: Settings) -> anyhow::Result<Self> {
		let catalog_pub = broadcast.create(".catalog").context("broadcast closed")?.groups()?;
		let mut catalog = moq_catalog::MoqCatalog::new();

		let mut csf = moq_catalog::CommonStructFields::new("", moq_catalog::Packaging::CMAF);
		csf.set_alt_group(1)
			.set_label("Dash MoQ")
			.set_namespace(&broadcast.namespace);

		catalog.enable_delta_updates().set_common_track_fields(csf);

		Ok(Self {
			settings,
			tracks: HashMap::new(),
			broadcast,
			catalog_pub,
			catalog,
			ftyp: HashMap::new(),
			moov: HashMap::new(),
			prft: HashMap::new(),
		})
	}

	pub fn parse<B: bytes::Buf>(&mut self, buf: &mut B, rep_id: RepID) -> anyhow::Result<()> {
		while self.parse_atom(buf, rep_id.clone())? {}
		Ok(())
	}

	fn parse_atom<B: bytes::Buf>(&mut self, buf: &mut B, rep_id: RepID) -> anyhow::Result<bool> {
		let Some(atom) = next_atom(buf)? else {
			return Ok(false);
		};

		// TODO
		// init segment = ftyp + moov
		// then a segemnt is prft + moof + mdat very often
		// => one object = one set of these

		let mut reader = std::io::Cursor::new(&atom);
		let header = mp4::BoxHeader::read(&mut reader)?;

		match header.name {
			h if h.to_string() == *"prft" => {
				self.prft.insert(rep_id, atom);
			}
			mp4::BoxType::FtypBox => {
				if self.ftyp.get(&rep_id).is_some() {
					anyhow::bail!("multiple ftyp atoms");
				}

				self.ftyp.insert(rep_id, atom);
			}
			mp4::BoxType::MoovBox => {
				if self.moov.get(&rep_id).is_some() {
					anyhow::bail!("multiple moov atoms");
				}

				let moov = mp4::MoovBox::read_box(&mut reader, header.size)?;

				self.setup(&moov, atom, rep_id.clone())?;
				self.moov.insert(rep_id, moov);
			}
			mp4::BoxType::MoofBox => {
				let moof = mp4::MoofBox::read_box(&mut reader, header.size)?;

				let fragment = Fragment::new(moof)?;

				let track = self.tracks.get_mut(&rep_id).context("failed to find track")?;

				if fragment.keyframe && track.handler == mp4::TrackType::Video {
					track.end_group();
				}

				track.header(atom, fragment).context("failed to publish moof atoms")?;
			}
			mp4::BoxType::MdatBox => {
				let track = self.tracks.get_mut(&rep_id).context("failed to find track")?;

				track.data(atom).context("failed to publish mdat")?;

				if let Some(prft) = self.prft.remove(&rep_id) {
					track.data(prft).context("failed to publish prft")?;
				}
			}
			_ => {
				// Skip other atmos
			}
		}

		Ok(true)
	}

	fn setup(&mut self, moov: &mp4::MoovBox, raw: bytes::Bytes, rep_id: RepID) -> anyhow::Result<()> {
		for trak in &moov.traks {
			let id = trak.tkhd.track_id;
			let timescale = track_timescale(moov, id);
			let handler = (&trak.mdia.hdlr.handler_type).try_into()?;
			let track = self.broadcast.create(&rep_id).context("broadcast closed")?;
			let track = Track::new(track, handler, timescale);
			self.tracks.insert(rep_id.clone(), track);
		}

		let mut init = self.ftyp.remove(&rep_id).context("missing ftyp")?.to_vec();
		init.extend_from_slice(&raw);

		for trak in &moov.traks {
			let mut track = moq_catalog::Track::new(&rep_id, moq_catalog::Packaging::CMAF);
			let mut params = moq_catalog::SelectionParams::new();
			// TODO audio stuff

			let bitrate = self
				.settings
				.reps
				.get(&rep_id)
				.context("missing representation")?
				.bitrate
				.context("missing bitrate")?
				* 1_000;
			let framerate = self.settings.fps;

			if let Some(avc1) = &trak.mdia.minf.stbl.stsd.avc1 {
				let width = avc1.width;
				let height = avc1.height;

				let profile = avc1.avcc.avc_profile_indication;
				let constraints = avc1.avcc.profile_compatibility; // Not 100% certain here, but it's 0x00 on my current test video
				let level = avc1.avcc.avc_level_indication;
				let codec = rfc6381_codec::Codec::avc1(profile, constraints, level);
				let codec_str = codec.to_string();

				params
					.set_width(width)
					.set_height(height)
					.set_codec(&codec_str)
					.set_mime_type("video/mp4")?
					.set_bitrate(bitrate)
					.set_framerate(framerate);
			}

			track.set_init_data(&init).set_selection_params(params).set_alt_group(1);

			// TODO maybe Patch update in the future?
			self.catalog.insert_track(track)?;
		}

		log::info!("published catalog: {:#?}", self.catalog);

		let buf = self.catalog.encode()?;

		self.catalog_pub.append(0)?.write(buf.into())?;

		Ok(())
	}
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

	pub fn header(&mut self, raw: bytes::Bytes, fragment: Fragment) -> anyhow::Result<()> {
		if let Some(current) = self.current.as_mut() {
			// Use the existing segment
			current.write(raw)?;
			return Ok(());
		}

		// Otherwise make a new segment

		// Compute the timestamp in milliseconds.
		// Overflows after 583 million years, so we're fine.
		let timestamp: u32 = fragment
			.timestamp(self.timescale)
			.as_millis()
			.try_into()
			.context("timestamp too large")?;

		let priority = u32::MAX.checked_sub(timestamp).context("priority too large")?.into();

		// Create a new segment.
		let mut segment = self.track.append(priority)?;

		// Write the fragment in it's own object.
		segment.write(raw)?;

		// Save for the next iteration
		self.current = Some(segment);

		Ok(())
	}

	pub fn data(&mut self, raw: bytes::Bytes) -> anyhow::Result<()> {
		let segment = self.current.as_mut().context("missing current fragment")?;
		segment.write(raw)?;

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
	fn new(moof: mp4::MoofBox) -> anyhow::Result<Self> {
		// We can't split the mdat atom, so this is impossible to support
		anyhow::ensure!(moof.trafs.len() == 1, "multiple tracks per moof atom");
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

fn next_atom<B: bytes::Buf>(buf: &mut B) -> anyhow::Result<Option<bytes::Bytes>> {
	let mut peek = std::io::Cursor::new(buf.chunk());

	if peek.remaining() < 8 {
		if buf.remaining() != buf.chunk().len() {
			// TODO figure out a way to peek at the first 8 bytes
			anyhow::bail!("TODO: vectored Buf not yet supported");
		}

		return Ok(None);
	}

	// Convert the first 4 bytes into the size.
	let size = peek.get_u32();
	let _type = peek.get_u32();

	let size = match size {
		// Runs until the end of the file.
		0 => anyhow::bail!("TODO: unsupported EOF atom"),

		// The next 8 bytes are the extended size to be used instead.
		1 => {
			let size_ext = peek.get_u64();
			anyhow::ensure!(size_ext >= 16, "impossible extended box size: {}", size_ext);
			size_ext as usize
		}

		2..=7 => {
			anyhow::bail!("impossible box size: {}", size)
		}

		size => size as usize,
	};

	if buf.remaining() < size {
		return Ok(None);
	}

	let atom = buf.copy_to_bytes(size);

	Ok(Some(atom))
}
