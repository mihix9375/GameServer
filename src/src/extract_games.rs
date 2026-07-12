use std::fs::{self, File};
use std::path::{Path, PathBuf};
use ripunzip::UnzipOptions;
use serde_json::Value;
use crate::init::Meta;

struct MetaFilter {}
impl ripunzip::FilenameFilter for MetaFilter {
	fn should_unzip(&self, filename: &str) -> bool {
		Path::new(filename)
			.file_name()
			.map(|name| name == "meta.json")
			.unwrap_or(false)
	}
}

fn unzip_meta<S: AsRef<Path>, D: AsRef<Path>>(src: S, dir: D) -> Result<(), String> {
	let file = File::open(src).map_err(|e| e.to_string())?;
	let zip = ripunzip::UnzipEngine::for_file(file).map_err(|e| e.to_string())?;
	zip.unzip(UnzipOptions {
		output_directory: Some(dir.as_ref().into()),
		password: None,
		single_threaded: false,
		filename_filter: Some(Box::new(MetaFilter {})),
		progress_reporter: Box::new(ripunzip::NullProgressReporter {}),
	}).map_err(|e| e.to_string())?;

	Ok(())
}

fn find_meta_file(dir: &Path) -> Option<PathBuf> {
	if let Ok(entries) = fs::read_dir(dir) {
		for entry in entries.flatten() {
			let path = entry.path();
			if path.is_file() && path.file_name() == Some(std::ffi::OsStr::new("meta.json")) {
				return Some(path);
			} else if path.is_dir() {
				if let Some(found) = find_meta_file(&path) {
					return Some(found);
				}
			}
		}
	}
	None
}

fn update_zip_with_new_meta(src_zip: &Path, dst_zip: &Path, new_meta_content: &str) -> Result<(), String> {
	use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};
	use std::io::Write;

	let src_file = File::open(src_zip).map_err(|e| e.to_string())?;
	let mut archive = ZipArchive::new(src_file).map_err(|e| e.to_string())?;

	let dst_file = File::create(dst_zip).map_err(|e| e.to_string())?;
	let mut writer = ZipWriter::new(dst_file);

	for i in 0..archive.len() {
		let file = archive.by_index(i).map_err(|e| e.to_string())?;
		let name = file.name().to_string();

		if name.ends_with("meta.json") || name == "meta.json" {
			let options = SimpleFileOptions::default()
				.compression_method(zip::CompressionMethod::Deflated);
			writer.start_file(&name, options).map_err(|e| e.to_string())?;
			writer.write_all(new_meta_content.as_bytes()).map_err(|e| e.to_string())?;
		} else {
			writer.raw_copy_file(file).map_err(|e| e.to_string())?;
		}
	}

	writer.finish().map_err(|e| e.to_string())?;
	Ok(())
}

fn write_games_json(games_dir: &Path) {
	let mut all_metas: Vec<Meta> = Vec::new();
	if let Ok(entries) = fs::read_dir(games_dir) {
		for entry in entries.flatten() {
			if entry.path().is_dir() {
				let name = entry.file_name().to_string_lossy().to_string();
				if name.ends_with(".exe") {
					let clean_name = name.trim_end_matches(".exe");
					if games_dir.join(clean_name).exists() || !entry.path().join("meta.json").exists() {
						let _ = fs::remove_dir_all(entry.path());
					}
					continue;
				}
				let meta_path = entry.path().join("meta.json");
				if let Ok(content) = fs::read_to_string(&meta_path) {
					if let Ok(mut meta) = serde_json::from_str::<Meta>(&content) {
						if meta.id.is_empty() {
							meta.id = name.clone();
						}
						meta.id = meta.id.trim_end_matches(".exe").to_string();
						if meta.game.is_empty() {
							meta.game = format!("{}.exe", meta.id);
						}
						all_metas.push(meta);
					}
				}
			}
		}
	}
	if let Ok(json_array) = serde_json::to_string_pretty(&all_metas) {
		let _ = fs::write(games_dir.join("games.json"), &json_array);
	}
}

pub fn extract_games(root: PathBuf, games: PathBuf)
{
	if !games.join("games.json").exists() {
		write_games_json(&games);
	}

	let temp = root.join("temp");
	if !temp.exists() || !temp.is_dir() {
		return;
	}

	let entries = match fs::read_dir(&temp) {
		Ok(e) => e,
		Err(_) => return,
	};

	let mut _extracted_any = false;
	for entry in entries.flatten() {
		let path = entry.path();
		if path.is_file() {
			let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
			if ext == "zip" {
				let file_name = match path.file_stem().and_then(|s| s.to_str()) {
					Some(name) => name,
					None => continue,
				};

				let game_dir = games.join(file_name);
				let _ = fs::create_dir_all(&game_dir);

				let temp_extract_dir = temp.join(format!("_extract_{}", file_name));
				let _ = fs::create_dir_all(&temp_extract_dir);

				if unzip_meta(&path, &temp_extract_dir).is_ok() {
					if let Some(extracted_meta_path) = find_meta_file(&temp_extract_dir) {
						if let Ok(content) = fs::read_to_string(&extracted_meta_path) {
							if let Ok(mut meta) = serde_json::from_str::<Meta>(&content) {
								if meta.id.is_empty() {
									meta.id = file_name.to_string();
								}
								meta.id = meta.id.trim_end_matches(".exe").to_string();
								if meta.game.is_empty() {
									meta.game = format!("{}.exe", meta.id);
								}
								if meta.title.is_empty() {
									if let Some(Value::String(t)) = meta.extra.remove("name") {
										meta.title = t;
									} else {
										meta.title = file_name.to_string();
									}
								}
								if meta.title_image.is_empty() {
									if let Some(Value::String(img)) = meta.extra.remove("image").or_else(|| meta.extra.remove("title_image")) {
										meta.title_image = img;
									}
								}
								if meta.latest_update.is_empty() {
									if let Some(Value::String(upd)) = meta.extra.remove("lastUpdate").or_else(|| meta.extra.remove("latest_update")) {
										meta.latest_update = upd;
									}
								}

								if let Ok(pretty_json) = serde_json::to_string_pretty(&meta) {
									let _ = fs::write(game_dir.join("meta.json"), &pretty_json);

									let dst_zip = game_dir.join(format!("{}.zip", file_name));
									let _ = update_zip_with_new_meta(&path, &dst_zip, &pretty_json);
									let _ = fs::remove_file(&path);
									_extracted_any = true;
								}
							}
						}
					}
				}

				let _ = fs::remove_dir_all(&temp_extract_dir);
			}
		}
	}
	write_games_json(&games);
}
