use std::fs;
use std::io::Read;
use std::path::PathBuf;
use zip::ZipArchive;
use std::env;
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

		let content =
		{
			let zip_file = match fs::File::open(&zip_path)
			{
				Ok(f) => f,
				Err(_) => continue,
			};
			let mut archive = match ZipArchive::new(zip_file)
			{
				Ok(a) => a,
				Err(_) => continue,
			};

			let mut meta_index = None;
			for i in 0..archive.len()
			{
				if let Ok(file) = archive.by_index(i)
				{
					if file.name() == "meta.json" || file.name().ends_with("/meta.json") || file.name().ends_with("\\meta.json")
					{
						meta_index = Some(i);
						break;
					}
				}
			}

			let match_index = match meta_index
			{
				Some(i) => i,
				None => continue,
			};
			let mut meta_file = match archive.by_index(match_index)
			{
				Ok(f) => f,
				Err(_) => continue,
			};

			let mut buf = String::new();
			if meta_file.read_to_string(&mut buf).is_err()
			{
				continue;
			}
			buf
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

		fs::write(&meta_path, json_str).expect("Couldnt write meta.json");

		let new_zip_path = game_dir.join(entry.file_name());
		fs::copy(&zip_path, &new_zip_path).expect("Couldnt copy zip file");
	}
}
