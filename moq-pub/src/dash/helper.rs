use std::{fs, path};

use super::Error;

/// create full directory path
pub fn init_output<P>(output: P) -> anyhow::Result<()>
where
	P: AsRef<path::Path>,
{
	fs::create_dir_all(output)?;
	Ok(())
}

/// remove directory and all its contents
pub fn clear_output<P>(output: P) -> anyhow::Result<()>
where
	P: AsRef<path::Path>,
{
	fs::remove_dir_all(output)?;
	Ok(())
}

/// split byte `vec` at the first occurrence of `sep`
pub fn split_vec_once(vec: Vec<u8>, sep: &[u8]) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
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

	Ok((first, second))
}

/// attempts to convert `path` to a String
pub fn path_to_string<P>(path: P) -> Option<String>
where
	P: AsRef<path::Path>,
{
	Some(path.as_ref().as_os_str().to_str()?.to_string())
}

/// removes possible trailing ".tmp"
pub fn clean_path<P>(path: P) -> anyhow::Result<String>
where
	P: AsRef<path::Path>,
{
	let Some(path) = path_to_string(path) else {
		return Err(Error::FailedToConvert.into());
	};

	let path = path.replace(".tmp", "");

	Ok(path)
}
