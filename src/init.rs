use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::env;
use ripunzip::UnzipOptions;
use serde_json::{Map, Value};
use serde::{
	Deserialize, Serialize
};

fn null_to_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
	D: serde::Deserializer<'de>,
	T: Default + Deserialize<'de>,
{
	let opt = Option::<T>::deserialize(deserializer)?;
	Ok(opt.unwrap_or_default())
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Meta
{
	#[serde(default, deserialize_with = "null_to_default")]
	pub title: String,
	#[serde(default, deserialize_with = "null_to_default")]
	pub author: String,
	#[serde(rename = "titleImage", alias = "title_image", alias = "TitleImage", alias = "image", alias = "imgName", default, deserialize_with = "null_to_default")]
	pub title_image: String,
	#[serde(default, deserialize_with = "null_to_default")]
	pub tags: Vec<String>,
	#[serde(rename = "game", alias = "exeName", alias = "exe", alias = "gameExe", default, deserialize_with = "null_to_default")]
	pub game: String,
	#[serde(default, deserialize_with = "null_to_default")]
	pub version: String,
	#[serde(rename = "latestUpdate", alias = "lastUpdate", alias = "latest_update", default, deserialize_with = "null_to_default")]
	pub latest_update: String,
	#[serde(default, deserialize_with = "null_to_default")]
	pub description: String,
	
	#[serde(flatten, skip_serializing)]
	pub extra: Map<String, Value>,
}

pub fn create_dirs()
{
	let exe_path	= env::current_exe().expect("Couldnt get exe path");
	let root		= exe_path.parent().expect("Couldnt get exe parent");

	println!("{}", root.display());
	let games = root.join("games");

	fs::create_dir_all(&games).expect("Couldnt create dir.");

	extract_games(root.to_path_buf(), games);
}

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
		if Path::new(&name).file_name() == Some(std::ffi::OsStr::new("meta.json")) {
			writer.start_file(&name, SimpleFileOptions::default()).map_err(|e| e.to_string())?;
			writer.write_all(new_meta_content.as_bytes()).map_err(|e| e.to_string())?;
		} else {
			writer.raw_copy_file(file).map_err(|e| e.to_string())?;
		}
	}
	writer.finish().map_err(|e| e.to_string())?;
	Ok(())
}

fn extract_games(root: PathBuf, gamespath: PathBuf)
{
	let temp_path = root.join("temp");
	if !temp_path.exists()
	{
		return;
	}

	for entry in fs::read_dir(temp_path).expect("Couldnt get files in temp")
	{
		let entry = match entry
		{
			Ok(e) => e,
			Err(_) => continue,
		};
		let zip_path = entry.path();

		if !zip_path.is_file()
		{
			continue;
		}

		let content = {
			let timestamp = std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.map(|d| d.as_nanos())
				.unwrap_or(0);
			let temp_dir = env::temp_dir().join(format!("gamelauncher_meta_{}_{}", std::process::id(), timestamp));
			let _ = fs::create_dir_all(&temp_dir);

			let res = if unzip_meta(&zip_path, &temp_dir).is_ok() {
				find_meta_file(&temp_dir).and_then(|path| fs::read_to_string(path).ok())
			} else {
				None
			};
			let _ = fs::remove_dir_all(&temp_dir);
			match res {
				Some(buf) => buf,
				None => continue,
			}
		};

		let meta = match serde_json::from_str::<Meta>(&content)
		{
			Ok(m) => m,
			Err(_) => continue,
		};

		let id_exe = meta.game.clone();
		let id = id_exe.trim_end_matches(".exe");

		let game_dir = gamespath.join(id);
		fs::create_dir_all(&game_dir).expect("Couldnt create dir.");

		let meta_path = game_dir.join("meta.json");
		let json_str = serde_json::to_string_pretty(&meta).expect("Couldnt create json string");

		fs::write(&meta_path, &json_str).expect("Couldnt write meta.json");

		let new_zip_path = game_dir.join(entry.file_name());
		if let Err(e) = update_zip_with_new_meta(&zip_path, &new_zip_path, &json_str) {
			eprintln!("Could not update meta.json inside zip, falling back to copy: {}", e);
			let _ = fs::copy(&zip_path, &new_zip_path);
		}
		let _ = fs::remove_file(&zip_path);
	}
}
