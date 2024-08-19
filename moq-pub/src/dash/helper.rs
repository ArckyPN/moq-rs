use std::{fs, path};

use super::Error;

/// create full directory path
pub fn init_output<P>(output: P) -> Result<(), Error>
where
	P: AsRef<path::Path>,
{
	if let Err(e) = fs::create_dir_all(output) {
		println!("Error: {}", e);
		return Err(Error::Crate("fs".to_string(), e.to_string()));
	}
	Ok(())
}

/// remove directory and all its contents
pub fn clear_output<P>(output: P) -> Result<(), Error>
where
	P: AsRef<path::Path>,
{
	if let Err(e) = fs::remove_dir_all(output) {
		println!("Error: {}", e);
		return Err(Error::Crate("fs".to_string(), e.to_string()));
	}
	Ok(())
}

/// split byte `vec` at the first occurrence of `sep`
pub fn split_vec_once(vec: Vec<u8>, sep: &[u8]) -> (Vec<u8>, Vec<u8>) {
	let mut first = Vec::new();
	let mut second = Vec::new();

	let mut split = false;
	let mut i = 0;
	while i < vec.len() {
		let c = vec[i];
		match split {
			true => second.push(c),
			false => {
				if &vec[i..i + sep.len()] == sep {
					split = true;
					i += sep.len();
					continue;
				}
				first.push(c)
			}
		}
		i += 1;
	}

	(first, second)
}

/// attempts to convert `path` to a String
pub fn path_to_string<P>(path: P) -> Option<String>
where
	P: AsRef<path::Path>,
{
	Some(path.as_ref().as_os_str().to_str()?.to_string())
}

/// removes possible trailing ".tmp"
pub fn clean_path<P>(path: P) -> Result<String, Error>
where
	P: AsRef<path::Path>,
{
	let Some(path) = path_to_string(path) else {
		return Err(Error::FailedToConvert);
	};

	let path = path.replace(".tmp", "");

	Ok(path)
}

pub fn append_shell(buf: &mut Vec<u8>, slice: &[String]) {
	let slice = if slice[0] == "-adaptation_sets" {
		vec![slice[0].clone(), format!("\"{}\"", slice[1])]
	} else {
		slice.to_vec()
	};
	let mut b = format!(" \\\n\t{}", slice.join(" "))
		.replace('$', "\\$")
		.as_bytes()
		.to_vec();
	buf.append(&mut b);
}
