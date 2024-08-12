use anyhow::Context;
use bytes::Buf;
use std::{collections::HashMap, fs, path};

use super::helper;

#[derive(Debug, Clone)]
pub struct Settings {
	pub gop_num: u64,
	pub audio_sampling_rate: u64,
	pub audio_bitrate: u64,
	pub fps: u64,
	pub target_segment_duration: f64,
	pub reps: HashMap<String, Setting>,
	pub order: Vec<String>,
	current: usize,
}

impl Settings {
	pub fn new<P>(file: P) -> anyhow::Result<Self>
	where
		P: AsRef<path::Path>,
	{
		let buf = fs::read(file)?;

		let (key_pairs, csv_vec) = helper::split_vec_once(buf, "===".as_bytes())?;

		let (gop_num, audio_sampling_rate, audio_bitrate, fps, target_segment_duration) =
			Self::parse_key_pairs(key_pairs)?;

		let (reps, order) = Setting::map_from_bytes(csv_vec)?;

		Ok(Self {
			gop_num,
			audio_sampling_rate,
			audio_bitrate,
			fps,
			target_segment_duration,
			reps,
			order,
			current: 0,
		})
	}

	pub fn to_args<P>(&self, input: P, output: P, no_audio: bool, looping: bool) -> anyhow::Result<Vec<String>>
	where
		P: AsRef<path::Path>,
	{
		let mut args = Vec::new();

		let fragment_duration = format!("{:.3}", 1_f64 / self.fps as f64);
		let segment_duration = format!("{:.3}", self.parse_segment_duration());

		let mut input_args = vec!["-fflags", "+genpts", "-re"];

		if looping {
			input_args.append(&mut vec!["-stream_loop", "-1"]);
		}

		let input = input.as_ref().to_str().context("input not found")?;

		input_args.append(&mut vec!["-i", input]);

		// if input == "/dev/video0" {
		// 	// FIXME record audio, this doesn't work
		// 	input_args.append(&mut vec!["-f", "alsa", "-i", "hw:1"]);
		// }

		args.append(&mut input_args);

		let mut args: Vec<String> = args.iter().map(|a| a.to_string()).collect();

		args.append(&mut self.filter_complex());
		args.append(&mut self.qualities());
		args.append(&mut self.audio(no_audio));
		args.append(&mut self.maps());

		let output = output.as_ref().join("source.mpd");
		let output_args = vec![
			"-preset",
			"ultrafast",
			"-sc_threshold",
			"0",
			"-r",
			"25", // TODO use framerate
			"-keyint_min",
			"48",
			"-g", // TODO use gop_num
			"48",
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
			"-frag_duration",
			&fragment_duration,
			"-init_seg_name",
			"source_init_rep_$RepresentationID$.$ext$",
			"-media_seg_name",
			"source_chunk_$Number%05d$_rep_$RepresentationID$.$ext$",
			"-f",
			"dash",
			output.to_str().unwrap(),
		];

		let mut output_args = output_args.iter().map(|a| a.to_string()).collect();

		args.append(&mut output_args);

		Ok(args)
	}

	pub fn save<P>(&self, path: P, args: &[String]) -> anyhow::Result<()>
	where
		P: AsRef<path::Path>,
	{
		// FIXME input and output path to work from the save location
		let mut buf = b"#!/bin/bash\n\n".to_vec();

		let mut ffmpeg = "ffmpeg".as_bytes().to_vec();
		buf.append(&mut ffmpeg);

		let (input, args) = args.split_at(args.iter().position(|arg| arg == "-i").context("missing input flag")? + 2);
		let input = vec![input[0].clone()];
		helper::append_shell(&mut buf, &input);

		let (filter_complex, args) = args.split_at(
			args.iter()
				.position(|arg| arg == "-filter_complex")
				.context("missing filter_complex flag")?
				+ 2,
		);

		if filter_complex.len() != 2 {
			anyhow::bail!("invalid filter complex flag");
		}

		let filter_complex = vec![
			filter_complex[0].clone(),
			format!("\"{}\"", filter_complex[1].replace(';', "; \\\n\t\t")),
		];
		helper::append_shell(&mut buf, &filter_complex);

		let (streams, args) = args.split_at(args.iter().position(|arg| arg == "-map").context("missing map flag")?);
		let chunks = streams.chunks(6);
		for chunk in chunks {
			helper::append_shell(&mut buf, chunk);
		}

		let chunks = args.chunks(2);
		for chunk in chunks {
			helper::append_shell(&mut buf, chunk);
		}

		std::fs::write(path, buf)?;
		Ok(())
	}

	fn filter_complex(&self) -> Vec<String> {
		let mut split = format!("split={}", self.reps.len());
		let mut resolutions = Vec::new();

		for (i, rep) in self.reps.iter() {
			split += format!("[v{}]", i).as_str();
			resolutions.push(format!("[v{}]scale={}[v{}]", i, rep.resolution.clone().unwrap(), i));
		}

		let split = format!("{};{}", split, resolutions.join(";"));

		vec!["-filter_complex".to_string(), split]
	}

	fn qualities(&self) -> Vec<String> {
		let mut args = Vec::new();

		for (i, rep) in self.clone().enumerate() {
			args.push(format!("-b:v:{i}"));
			args.push(format!("{}K", rep.bitrate.unwrap()));

			args.push(format!("-maxrate:v:{i}"));
			args.push(format!("{}K", rep.max_rate.unwrap()));

			args.push(format!("-bufsize:v:{i}"));
			args.push(format!("{}K", rep.buffer_size.unwrap()));
		}

		args
	}

	fn audio(&self, no_audio: bool) -> Vec<String> {
		if no_audio {
			return vec!["-an".to_string()];
		}
		vec![
			"-c:a".to_string(),
			"aac".to_string(),
			"-b:a".to_string(),
			format!("{}K", self.audio_bitrate),
			"-ar".to_string(),
			format!("{}", self.audio_sampling_rate),
		]
	}

	fn maps(&self) -> Vec<String> {
		let mut args = Vec::new();

		let map = "-map".to_string();
		for i in 0..self.reps.len() {
			args.append(&mut vec![map.clone(), format!("[v{i}]")]);
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

		let divider = greatest_common_divider(1024 * self.fps, self.audio_sampling_rate);
		let base = 1024_f64 / divider as f64;
		let multiplier = (self.target_segment_duration / base) as u64;

		base * multiplier as f64
	}

	fn parse_key_pairs(key_pairs: Vec<u8>) -> anyhow::Result<(u64, u64, u64, u64, f64)> {
		let re = regex::Regex::new(r" +#.+\n")?;
		let str = re.replace_all(&String::from_utf8(key_pairs)?, "\n").to_string();

		let key_pairs = str.as_bytes().to_vec();

		let (gop_num, key_pairs) = Self::parse_u64(key_pairs)?;
		let (audio_sampling_rate, key_pairs) = Self::parse_u64(key_pairs)?;
		let (audio_bitrate, key_pairs) = Self::parse_u64(key_pairs)?;
		let (fps, key_pairs) = Self::parse_u64(key_pairs)?;
		let (target_segment_duration, _) = Self::parse_f64(key_pairs)?;

		Ok((
			gop_num,
			audio_sampling_rate,
			audio_bitrate,
			fps,
			target_segment_duration,
		))
	}

	fn parse_u64(buf: Vec<u8>) -> anyhow::Result<(u64, Vec<u8>)> {
		let (data, buf) = helper::split_vec_once(buf.to_vec(), b"\n")?;

		let (_, data) = helper::split_vec_once(data, b"=")?;
		let str = String::from_utf8(data)?;

		let num = str.parse::<u64>()?;

		Ok((num, buf))
	}

	fn parse_f64(buf: Vec<u8>) -> anyhow::Result<(f64, Vec<u8>)> {
		let (data, buf) = helper::split_vec_once(buf.to_vec(), b"\n")?;

		let (_, data) = helper::split_vec_once(data, b"=")?;
		let str = String::from_utf8(data)?;

		let num = str.parse::<f64>()?;

		Ok((num, buf))
	}
}

impl std::iter::Iterator for Settings {
	type Item = Setting;

	fn next(&mut self) -> Option<Self::Item> {
		if self.current == self.order.len() {
			return None;
		}
		let next = self.reps.get(&self.order[self.current]);
		self.current += 1;
		next.cloned()
	}
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Setting {
	pub resolution: Option<String>,
	pub bitrate: Option<u64>,
	pub max_rate: Option<u64>,
	pub buffer_size: Option<u64>,
}

impl Setting {
	pub fn map_from_bytes(buf: Vec<u8>) -> anyhow::Result<(HashMap<String, Self>, Vec<String>)> {
		let mut vec = Vec::new();
		let mut map = HashMap::new();
		let mut reader = csv::Reader::from_reader(buf.as_slice().reader());

		for (counter, res) in reader.deserialize().enumerate() {
			let key = format!("{}", counter);
			map.insert(key.clone(), res?);
			vec.push(key);
		}

		Ok((map, vec))
	}
}
