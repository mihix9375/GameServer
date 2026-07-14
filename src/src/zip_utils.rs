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
	use zip::ZipArchive;

	let file = File::open(src).map_err(|e| e.to_string())?;
	let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;

	for i in 0..archive.len()
	{
		let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
		let decoded_name = decode_filename(file.name_raw());

		let mut outpath = dir.as_ref().to_path_buf();
		for component in Path::new(&decoded_name).components()
		{
			if let std::path::Component::Normal(c) = component { outpath.push(c); }
		}

		if file.is_dir() || decoded_name.ends_with('/') || decoded_name.ends_with('\\')
		{
			let _ = fs::create_dir_all(&outpath);
		}
		else
		{
			if let Some(parent) = outpath.parent() { let _ = fs::create_dir_all(parent); }
			let mut outfile = File::create(&outpath).map_err(|e| e.to_string())?;
			std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
		}
	}
	Ok(())
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
