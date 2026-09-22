use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

pub fn decode_filename(raw: &[u8]) -> String
{
	if let Ok(s) = std::str::from_utf8(raw)
	{
		s.to_string()
	}
	else
	{
		let (cow, _, _) = encoding_rs::SHIFT_JIS.decode(raw);
		cow.into_owned()
	}
}

pub fn extract_zip_clean<S: AsRef<Path>, D: AsRef<Path>>(src: S, dir: D) -> Result<(), String>
{
	use std::path::Component;
	use std::sync::atomic::{AtomicUsize, Ordering};
	use std::sync::Arc;
	use zip::ZipArchive;

	let source = src.as_ref();
	let destination = dir.as_ref();
	let file = File::open(source).map_err(|e| e.to_string())?;
	let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;
	let mut files = Vec::new();
	let mut seen = HashSet::new();

	for i in 0..archive.len()
	{
		let file = archive.by_index(i).map_err(|e| e.to_string())?;
		let decoded_name = decode_filename(file.name_raw());
		let relative = Path::new(&decoded_name);
		if decoded_name.trim().is_empty()
			|| decoded_name.contains(':')
			|| relative.components().any(|part| !matches!(part, Component::Normal(_)))
		{
			return Err(format!("ZIP内に不正なパスがあります: {decoded_name}"));
		}
		if !seen.insert(relative.to_path_buf())
		{
			return Err(format!("ZIP内のパスが重複しています: {decoded_name}"));
		}
		if file.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000)
		{
			return Err(format!("ZIP内のシンボリックリンクは使用できません: {decoded_name}"));
		}

		let outpath = destination.join(relative);
		if file.is_dir() || decoded_name.ends_with('/') || decoded_name.ends_with('\\')
		{
			fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
		}
		else
		{
			if let Some(parent) = outpath.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
			files.push((i, outpath));
		}
	}
	drop(archive);

	if files.is_empty() { return Ok(()); }
	let files = Arc::new(files);
	let next = AtomicUsize::new(0);
	let worker_count = std::thread::available_parallelism().map(usize::from).unwrap_or(1)
		.min(4)
		.min(files.len());
	std::thread::scope(|scope| -> Result<(), String> {
		let mut workers = Vec::with_capacity(worker_count);
		for _ in 0..worker_count
		{
			let files = Arc::clone(&files);
			let next = &next;
			workers.push(scope.spawn(move || -> Result<(), String> {
				let file = File::open(source).map_err(|e| e.to_string())?;
				let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;
				loop
				{
					let task = next.fetch_add(1, Ordering::Relaxed);
					let Some((entry_index, output_path)) = files.get(task) else { break; };
					let mut input = archive.by_index(*entry_index).map_err(|e| e.to_string())?;
					let mut output = File::create(output_path).map_err(|e| e.to_string())?;
					std::io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
				}
				Ok(())
			}));
		}
		for worker in workers
		{
			worker.join().map_err(|_| "ZIP展開スレッドが異常終了しました".to_string())??;
		}
		Ok(())
	})
}

pub fn update_zip_with_new_meta(src_zip: &Path, dst_zip: &Path, new_meta_content: &str) -> Result<(), String>
{
	use zip::{
		ZipArchive, ZipWriter, write::FileOptions
	};

	if let Some(parent) = dst_zip.parent() { let _ = fs::create_dir_all(parent); }

	let in_file = File::open(src_zip).map_err(|e| e.to_string())?;
	let mut archive = ZipArchive::new(in_file).map_err(|e| e.to_string())?;

	let out_file = File::create(dst_zip).map_err(|e| e.to_string())?;
	let mut zip_writer = ZipWriter::new(out_file);

	let options: FileOptions<'_, ()> = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

	let mut meta_written = false;
	for i in 0..archive.len()
	{
		let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
		let name = decode_filename(file.name_raw());

		if name == "meta.json" || name.ends_with("/meta.json") || name.ends_with("\\meta.json")
		{
			zip_writer.start_file(&name, options).map_err(|e| e.to_string())?;
			zip_writer.write_all(new_meta_content.as_bytes()).map_err(|e| e.to_string())?;
			meta_written = true;
		}
		else if file.is_dir()
		{
			zip_writer.add_directory(&name, options).map_err(|e| e.to_string())?;
		}
		else
		{
			zip_writer.start_file(&name, options).map_err(|e| e.to_string())?;
			std::io::copy(&mut file, &mut zip_writer).map_err(|e| e.to_string())?;
		}
	}

	if !meta_written
	{
		zip_writer.start_file("meta.json", options).map_err(|e| e.to_string())?;
		zip_writer.write_all(new_meta_content.as_bytes()).map_err(|e| e.to_string())?;
	}

	zip_writer.finish().map_err(|e| e.to_string())?;
	Ok(())
}

#[cfg(test)]
mod tests
{
	use super::*;
	use zip::write::SimpleFileOptions;

	#[test]
	fn extracts_entries_with_parallel_workers()
	{
		let root = std::env::temp_dir().join(format!("gameserver-extract-{}", std::process::id()));
		let _ = fs::remove_dir_all(&root);
		fs::create_dir_all(&root).unwrap();
		let archive_path = root.join("game.zip");
		let destination = root.join("output");
		let output = File::create(&archive_path).unwrap();
		let mut archive = zip::ZipWriter::new(output);
		for index in 0..8
		{
			archive.start_file(format!("data/{index}.txt"), SimpleFileOptions::default()).unwrap();
			archive.write_all(format!("entry-{index}").as_bytes()).unwrap();
		}
		archive.finish().unwrap();

		extract_zip_clean(&archive_path, &destination).unwrap();
		for index in 0..8
		{
			assert_eq!(fs::read_to_string(destination.join(format!("data/{index}.txt"))).unwrap(), format!("entry-{index}"));
		}
		let _ = fs::remove_dir_all(root);
	}
}
