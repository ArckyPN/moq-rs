use bytes::Buf;

use super::{helper, Error};

const INPUT_DEFAULT: &str = "/dev/video0";

#[derive(Debug, Clone)]
pub struct Settings<P>
where
	P: AsRef<std::path::Path>,
{
	pub gop_num: u64,
	pub fps: u64,
	pub target_segment_duration: f64,
	pub audio: Vec<AudioSetting>,
	pub video: Vec<VideoSetting>,
	input: P,
	output: P,
	no_audio: bool,
	looping: bool,
}

impl<P> Settings<P>
where
	P: AsRef<std::path::Path>,
{
	pub fn new(settings_file: P, input: P, output: P, no_audio: bool, looping: bool) -> Result<Self, Error> {
		let buf = match std::fs::read(settings_file) {
			Ok(b) => b,
			Err(e) => {
				println!("Error: {}", e);
				return Err(Error::Crate("fs".to_string(), e.to_string()));
			}
		};

		let (key_pairs, csv_vec) = helper::split_vec_once(buf, "===AUDIO===\n".as_bytes());

		let (audio, video) = helper::split_vec_once(csv_vec, b"===VIDEO===\n");

		let (gop_num, fps, target_segment_duration) = Self::parse_key_pairs(&key_pairs)?;

		let video = VideoSetting::vec_from_bytes(&video)?;

		let audio = AudioSetting::vec_from_bytes(&audio)?;

		Ok(Self {
			gop_num,
			fps,
			target_segment_duration,
			audio,
			video,
			input,
			output,
			no_audio,
			looping,
		})
	}

	pub fn to_args(&self) -> Result<Vec<String>, Error> {
		let mut args = Vec::new();

		let segment_duration = format!("{:.3}", self.parse_segment_duration());

		let mut input_args = vec!["-fflags", "+genpts", "-re"];

		if self.looping {
			input_args.append(&mut vec!["-stream_loop", "-1"]);
		}

		let Some(input) = self.input.as_ref().to_str() else {
			println!("Error: input path is not a valid string");
			return Err(Error::FailedToConvert);
		};

		let fps = format!("{}", self.fps);
		if input == INPUT_DEFAULT {
			input_args.append(&mut vec![
				"-f",
				"alsa",
				"-ac",
				"2",
				"-thread_queue_size",
				"512",
				"-i",
				"default",
				"-f",
				"video4linux2",
				"-s",
				"1280x720",
				"-r",
				&fps,
				"-i",
				input,
			]);
		} else {
			input_args.append(&mut vec!["-i", input]);
		}

		args.append(&mut input_args);

		let mut args: Vec<String> = args.iter().map(|a| a.to_string()).collect();

		args.append(&mut self.audio());
		args.append(&mut self.qualities()?);

		let gop = format!(
			"{}",
			(self.gop_num as f64 * self.fps as f64 * self.parse_segment_duration()) as u64
		);

		let output = self.output.as_ref().join("source.mpd");
		let output_args = vec![
			"-f",
			"dash",
			"-dash_segment_type",
			"mp4",
			"-preset",
			"ultrafast",
			"-sc_threshold",
			"0",
			"-r",
			&fps,
			"-keyint_min",
			&gop,
			"-g",
			&gop,
			"-aspect",
			"16:9",
			"-c:v",
			"libx264",
			"-pix_fmt",
			"yuv420p",
			"-color_primaries",
			"bt709",
			"-color_trc",
			"bt709",
			"-colorspace",
			"bt709",
			"-tune",
			"zerolatency",
			"-x264-params",
			"sliced-threads=0:nal-hrd=cbr",
			"-seg_duration",
			&segment_duration,
			"-adaptation_sets",
			"id=0,streams=v id=1,streams=a",
			"-use_timeline",
			"1",
			"-streaming",
			"1",
			"-window_size",
			"3",
			"-extra_window_size",
			"0",
			"-frag_type",
			"every_frame",
			"-utc_timing_url",
			"https://time.akamai.com/?iso",
			"-write_prft",
			"1",
			"-flags",
			"+global_header",
			"-metadata",
			"title=MoQ",
			"-ldash",
			"1",
			"-init_seg_name",
			"source_init_rep_$RepresentationID$.$ext$",
			"-media_seg_name",
			"source_chunk_$Number%05d$_rep_$RepresentationID$.$ext$",
			output.to_str().unwrap(),
		];

		let mut output_args = output_args.iter().map(|a| a.to_string()).collect();

		args.append(&mut output_args);

		Ok(args)
	}

	fn qualities(&self) -> Result<Vec<String>, Error> {
		let Some(input) = self.input.as_ref().to_str() else {
			println!("Error: input path is not a valid string");
			return Err(Error::FailedToConvert);
		};

		let mut args = Vec::new();

		for (i, rep) in self.video.iter().enumerate() {
			let map = if self.no_audio || self.audio.is_empty() || input != INPUT_DEFAULT {
				"0:v:0".to_string()
			} else {
				"1:v:0".to_string()
			};

			let mut arg = vec![
				"-map".to_string(),
				map,
				format!("-s:v:{i}"),
				rep.resolution.clone(),
				format!("-b:v:{i}"),
				format!("{}", rep.bitrate),
				format!("-maxrate:v:{i}"),
				format!("{}", rep.max_rate),
				format!("-bufsize:v:{i}"),
				format!("{}", rep.buffer_size),
			];

			args.append(&mut arg);
		}

		Ok(args)
	}

	fn audio(&self) -> Vec<String> {
		if self.no_audio || self.audio.is_empty() {
			return vec!["-an".to_string()];
		}

		let mut args = Vec::new();

		for (i, rep) in self.audio.iter().enumerate() {
			let mut arg = vec![
				"-map".to_string(),
				"0:a:0".to_string(),
				format!("-c:a:{i}"),
				"aac".to_string(),
				format!("-b:a:{i}"),
				format!("{}", rep.bitrate),
				format!("-ar:{i}"),
				format!("{}", rep.sampling_rate),
			];
			args.append(&mut arg);
		}

		args
	}

	fn parse_segment_duration(&self) -> f64 {
		let greatest_common_divider = |x: u64, y: u64| {
			let mut y = y;
			let mut x = x;
			while y != 0 {
				let t = y;
				y = x % y;
				x = t;
			}
			x
		};

		let sampling_rate = if !self.audio.is_empty() {
			self.audio[0].sampling_rate
		} else {
			128_000
		};

		let divider = greatest_common_divider(1024 * self.fps, sampling_rate);
		let base = 1024_f64 / divider as f64;
		let multiplier = (self.target_segment_duration / base) as u64;

		base * multiplier as f64
	}

	fn parse_key_pairs(key_pairs: &[u8]) -> Result<(u64, u64, f64), Error> {
		let re = match regex::Regex::new(r" +#.+\n") {
			Ok(r) => r,
			Err(e) => {
				println!("Regex: {}", e);
				return Err(Error::Crate("regex".to_string(), e.to_string()));
			}
		};
		let key_pairs = match String::from_utf8(key_pairs.to_vec()) {
			Ok(v) => v,
			Err(e) => {
				println!("Error: {}", e);
				return Err(Error::Crate("String".to_string(), e.to_string()));
			}
		};
		let str = re.replace_all(&key_pairs, "\n").to_string();

		let key_pairs = str.as_bytes().to_vec();

		let (gop_num, key_pairs) = Self::parse_u64(key_pairs)?;
		let (fps, key_pairs) = Self::parse_u64(key_pairs)?;
		let (target_segment_duration, _) = Self::parse_f64(key_pairs)?;

		Ok((gop_num, fps, target_segment_duration))
	}

	pub fn get_rep(&self, index: usize) -> Option<Setting> {
		if index >= self.rep_len() {
			return None;
		}

		let audio = self.audio.len();
		if index < audio {
			Some(Setting::Audio(self.audio[index].clone()))
		} else {
			Some(Setting::Video(self.video[index - audio].clone()))
		}
	}

	pub fn rep_len(&self) -> usize {
		self.audio.len() + self.video.len()
	}

	fn parse_u64(buf: Vec<u8>) -> Result<(u64, Vec<u8>), Error> {
		let (data, buf) = helper::split_vec_once(buf.to_vec(), b"\n");

		let (_, data) = helper::split_vec_once(data, b"=");
		let str = match String::from_utf8(data) {
			Ok(s) => s,
			Err(e) => {
				println!("Error: {}", e);
				return Err(Error::Crate("string".to_string(), e.to_string()));
			}
		};

		let num = str.parse::<u64>().unwrap_or_default();

		Ok((num, buf))
	}

	fn parse_f64(buf: Vec<u8>) -> Result<(f64, Vec<u8>), Error> {
		let (data, buf) = helper::split_vec_once(buf.to_vec(), b"\n");

		let (_, data) = helper::split_vec_once(data, b"=");
		let str = match String::from_utf8(data) {
			Ok(s) => s,
			Err(e) => {
				println!("Error: {}", e);
				return Err(Error::Crate("string".to_string(), e.to_string()));
			}
		};

		let num = str.parse::<f64>().unwrap_or_default();

		Ok((num, buf))
	}

	pub fn save(&self, path: P) -> Result<(), Error> {
		let args = self.to_args()?;
		let mut buf = b"#!/bin/bash\n\n".to_vec();

		let mut ffmpeg = b"ffmpeg".to_vec();
		buf.append(&mut ffmpeg);

		// check if there is a webcam input (double -f -i inputs)
		let f_pos = args.iter().position(|arg| arg == "-f").unwrap();
		let i_pos = args.iter().position(|arg| arg == "-i").unwrap();
		let args = if f_pos < i_pos {
			// when there if a format before input, append all flags until first -f
			let (input, args) = args.split_at(args.iter().position(|arg| arg == "-f").unwrap_or_default());
			helper::append_shell(&mut buf, input);
			args
		} else {
			// do nothing otherwise
			&args
		};

		// find the first input flags
		let (input, args) = args.split_at(args.iter().position(|arg| arg == "-i").unwrap_or_default() + 2);
		helper::append_shell(&mut buf, input);

		// try to find the second input flag, if found append
		let (input, args) = args.split_at(args.iter().position(|arg| arg == "-map").unwrap_or_default());
		if !input.is_empty() {
			helper::append_shell(&mut buf, input);
		}

		// find all audio flags, append in chunks of 8
		let (input, args) = args.split_at(args.iter().position(|arg| arg == "-s:v:0").unwrap_or_default() - 2);
		let chunks = input.chunks(8);
		for chunk in chunks {
			helper::append_shell(&mut buf, chunk);
		}

		// find all video flags, append in chunks of 10
		let (streams, args) = args.split_at(args.iter().position(|arg| arg == "-f").unwrap_or_default());
		let chunks = streams.chunks(10);
		for chunk in chunks {
			helper::append_shell(&mut buf, chunk);
		}

		// append the rest in chunks of 2
		let chunks = args.chunks(2);
		for chunk in chunks {
			helper::append_shell(&mut buf, chunk);
		}

		if let Err(e) = std::fs::write(path, buf) {
			println!("Error: {}", e);
			return Err(Error::Crate("fs".to_string(), e.to_string()));
		};
		Ok(())
	}
}

pub enum Setting {
	Audio(AudioSetting),
	Video(VideoSetting),
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct VideoSetting {
	pub name: String,
	pub resolution: String,
	pub bitrate: u64,
	pub max_rate: u64,
	pub buffer_size: u64,
}

impl VideoSetting {
	pub fn vec_from_bytes(buf: &[u8]) -> Result<Vec<Self>, Error> {
		let mut vec = Vec::new();
		let mut reader = csv::ReaderBuilder::new()
			.has_headers(true)
			.delimiter(b',')
			.comment(Some(b'#'))
			.trim(csv::Trim::All)
			.from_reader(buf.reader());

		for res in reader.deserialize() {
			let res = match res {
				Ok(r) => r,
				Err(e) => {
					println!("Error: {}", e);
					return Err(Error::Crate("csv".to_string(), e.to_string()));
				}
			};
			vec.push(res);
		}

		Ok(vec)
	}
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AudioSetting {
	pub name: String,
	#[serde(rename = "sampling")]
	pub sampling_rate: u64,
	pub bitrate: u64,
}

impl AudioSetting {
	pub fn vec_from_bytes(buf: &[u8]) -> Result<Vec<Self>, Error> {
		let mut vec = Vec::new();
		// let mut reader = csv::Reader::from_reader(buf.as_slice().reader());
		let mut reader = csv::ReaderBuilder::new()
			.has_headers(true)
			.delimiter(b',')
			.comment(Some(b'#'))
			.trim(csv::Trim::All)
			.from_reader(buf.reader());

		for res in reader.deserialize() {
			let res = match res {
				Ok(r) => r,
				Err(e) => {
					println!("Error: {}", e);
					return Err(Error::Crate("csv".to_string(), e.to_string()));
				}
			};
			vec.push(res);
		}

		Ok(vec)
	}
}
