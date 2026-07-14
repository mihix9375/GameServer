use std::fs::{self};
use std::path::{Path, PathBuf};
use serde_json::Value;
use crate::init::Meta;
use crate::src::zip_utils::{
	extract_zip_clean, update_zip_with_new_meta
};

fn find_meta_file(dir: &Path) -> Option<PathBuf>
{
	if let Ok(entries) = fs::read_dir(dir)
	{
		for entry in entries.flatten()
		{
			let path = entry.path();
			if path.is_file() && path.file_name() == Some(std::ffi::OsStr::new("meta.json")) { return Some(path); }
			if path.is_dir()
			{
				if let Some(inner) = find_meta_file(&path) { return Some(inner); }
			}
		}
	}
	None
}

fn clean_stem(s: &str) -> String
{
	let mut s_clean = s.trim_end_matches(".exe");
	while let Some(idx) = s_clean.find('_')
	{
		if idx > 0 && s_clean[..idx].chars().all(|c| c.is_ascii_digit())
		{
			s_clean = &s_clean[idx + 1..];
		}
		else
		{
			break;
		}
	}
	s_clean.to_string()
}

fn exe_exists(dir: &Path, exe_name: &str) -> bool
{
	if let Ok(entries) = fs::read_dir(dir)
	{
		for entry in entries.flatten()
		{
			let p = entry.path();
			if p.is_file() && p.file_name().map(|n| n.to_string_lossy() == exe_name).unwrap_or(false) { return true; }
			if p.is_dir() && exe_exists(&p, exe_name) { return true; }
		}
	}
	false
}

fn find_any_exe(dir: &Path) -> Option<String>
{
	if let Ok(entries) = fs::read_dir(dir)
	{
		for entry in entries.flatten()
		{
			let p = entry.path();
			if p.is_file() && p.extension().map(|e| e.to_string_lossy() == "exe").unwrap_or(false)
			{
				let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
				if !name.contains("UnityCrashHandler") && !name.contains("UnityPlayer") { return Some(name); }
			}
			else if p.is_dir()
			{
				if let Some(found) = find_any_exe(&p) { return Some(found); }
			}
		}
	}
	None
}

fn write_games_json(games_dir: &Path)
{
	let mut all_metas: Vec<Meta> = Vec::new();
	if let Ok(entries) = fs::read_dir(games_dir)
	{
		for entry in entries.flatten()
		{
			if entry.path().is_dir()
			{
				let mut dir_path = entry.path();
				let mut name = entry.file_name().to_string_lossy().to_string();
				if name.ends_with(".exe")
				{
					let clean_name = name.trim_end_matches(".exe");
					if games_dir.join(clean_name).exists() || !dir_path.join("meta.json").exists() { let _ = fs::remove_dir_all(dir_path); }
					continue;
				}
				let cleaned_name = clean_stem(&name);
				if cleaned_name != name && !cleaned_name.is_empty()
				{
					let new_dir_path = games_dir.join(&cleaned_name);
					if !new_dir_path.exists()
					{
						let _ = fs::rename(&dir_path, &new_dir_path);
						dir_path = new_dir_path;
						name = cleaned_name.clone();
					}
					else if new_dir_path != dir_path
					{
						let _ = fs::remove_dir_all(&dir_path);
						continue;
					}
				}
				let meta_path = dir_path.join("meta.json");
				if let Ok(content) = fs::read_to_string(&meta_path)
				{
					if let Ok(mut meta) = serde_json::from_str::<Meta>(&content)
					{
						let cleaned = clean_stem(&name);
						if meta.id.is_empty() || meta.id.chars().all(|c| c.is_ascii_digit()) || meta.id.contains('_') { meta.id = cleaned.clone(); }
						meta.id = clean_stem(&meta.id);
						if meta.game.is_empty() { meta.game = format!("{}.exe", meta.id); }
						let _ = fs::write(&meta_path, serde_json::to_string_pretty(&meta).unwrap_or_default());
						all_metas.push(meta);
					}
				}
			}
		}
	}
	all_metas.sort_by(|a, b| a.id.cmp(&b.id));
	if let Ok(json_array) = serde_json::to_string_pretty(&all_metas) { let _ = fs::write(games_dir.join("games.json"), &json_array); }
}

pub fn extract_games(root: PathBuf, games: PathBuf)
{
	let temp = root.join("temp");
	if let Ok(entries) = fs::read_dir(temp)
	{
		for entry in entries.flatten()
		{
			let path = entry.path();
			if path.is_file() && path.extension().map(|e| e.to_string_lossy() == "zip").unwrap_or(false)
			{
				let file_name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
				let temp_extract_dir = root.join("temp_extracted").join(&file_name);
				let _ = fs::remove_dir_all(&temp_extract_dir);
				if let Err(e) = extract_zip_clean(&path, &temp_extract_dir)
				{
					println!("ZIP Extract Error {}: {}", file_name, e);
					continue;
				}
				let mut meta = Meta::default();
				if let Some(extracted_meta_path) = find_meta_file(&temp_extract_dir)
				{
					if let Ok(content) = fs::read_to_string(&extracted_meta_path)
					{
						if let Ok(m) = serde_json::from_str::<Meta>(&content) { meta = m; }
					}
				}
				if meta.game.is_empty() || !exe_exists(&temp_extract_dir, &meta.game)
				{
					if let Some(real_exe) = find_any_exe(&temp_extract_dir) { meta.game = real_exe; }
				}
				let cleaned_stem = clean_stem(&file_name);
				if meta.id.is_empty() || meta.id.chars().all(|c| c.is_ascii_digit()) || meta.id.contains('_') { meta.id = cleaned_stem.clone(); }
				meta.id = clean_stem(&meta.id);
				if meta.game.is_empty() { meta.game = format!("{}.exe", meta.id); }
				if meta.title.is_empty()
				{
					if let Some(Value::String(t)) = meta.extra.remove("name") { meta.title = t; } else { meta.title = meta.id.clone(); }
				}
				if meta.title_image.is_empty()
				{
					if let Some(Value::String(img)) = meta.extra.remove("image").or_else(|| meta.extra.remove("title_image")) { meta.title_image = img; }
				}
				if meta.latest_update.is_empty()
				{
					if let Some(Value::String(upd)) = meta.extra.remove("lastUpdate").or_else(|| meta.extra.remove("latest_update")) { meta.latest_update = upd; }
				}
				let game_dir = games.join(&meta.id);
				let _ = fs::create_dir_all(&game_dir);
				if let Ok(pretty_json) = serde_json::to_string_pretty(&meta)
				{
					let dst_zip = game_dir.join(format!("{}.zip", meta.id));
					let _ = update_zip_with_new_meta(&path, &dst_zip, &pretty_json);
					let _ = fs::remove_file(&path);
					let _ = fs::write(game_dir.join("meta.json"), pretty_json);
				}
				let _ = fs::remove_dir_all(&temp_extract_dir);
			}
		}
	}
	write_games_json(&games);
}
