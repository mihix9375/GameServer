use std::fs::{self};
use std::env;
use serde_json::{Map, Value};
use serde::{
	Deserialize, Serialize
};
use crate::src::{
	extract_games
};

fn any_to_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
	D: serde::Deserializer<'de>,
{
	let val = Option::<Value>::deserialize(deserializer)?;
	match val
	{
		Some(Value::String(s)) => Ok(s),
		Some(Value::Number(n)) => Ok(n.to_string()),
		Some(Value::Bool(b)) => Ok(b.to_string()),
		_ => Ok(String::new()),
	}
}

fn date_to_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
	D: serde::Deserializer<'de>,
{
	any_to_string(deserializer).map(|value| normalize_meta_date(&value))
}

/// Normalizes commonly used meta.json date spellings for consistent display.
/// Unknown or invalid values are preserved so metadata is never lost silently.
pub fn normalize_meta_date(value: &str) -> String
{
	let original = value.trim();
	if original.is_empty()
	{
		return String::new();
	}

	let date_part = original.split(['T', ' ']).next().unwrap_or(original);
	let normalized = date_part
		.replace('年', "/")
		.replace('月', "/")
		.replace('日', "")
		.replace(['-', '.'], "/");
	let parts: Vec<&str> = normalized.split('/').filter(|part| !part.is_empty()).collect();
	let parsed = if parts.len() == 3
	{
		Some((parts[0], parts[1], parts[2]))
	}
	else if normalized.len() == 8 && normalized.bytes().all(|byte| byte.is_ascii_digit())
	{
		Some((&normalized[0..4], &normalized[4..6], &normalized[6..8]))
	}
	else
	{
		None
	};

	let Some((year, month, day)) = parsed else { return original.to_string(); };
	let (Ok(year), Ok(month), Ok(day)) = (year.parse::<u32>(), month.parse::<u32>(), day.parse::<u32>()) else
	{
		return original.to_string();
	};
	let leap_year = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
	let max_day = match month
	{
		1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
		4 | 6 | 9 | 11 => 30,
		2 if leap_year => 29,
		2 => 28,
		_ => return original.to_string(),
	};
	if !(1..=max_day).contains(&day)
	{
		return original.to_string();
	}
	format!("{year:04}/{month:02}/{day:02}")
}

fn any_to_vec_string<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
	D: serde::Deserializer<'de>,
{
	let val = Option::<Value>::deserialize(deserializer)?;
	match val
	{
		Some(Value::Array(arr)) => {
			let mut res = Vec::new();
			for item in arr
			{
				match item
				{
					Value::String(s) => res.push(s),
					Value::Number(n) => res.push(n.to_string()),
					Value::Bool(b) => res.push(b.to_string()),
					_ => {}
				}
			}
			Ok(res)
		}
		_ => Ok(Vec::new()),
	}
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Default)]
pub struct Meta
{
	#[serde(default, deserialize_with = "any_to_string")]
	pub id: String,
	#[serde(default, deserialize_with = "any_to_string")]
	pub title: String,
	#[serde(default, deserialize_with = "any_to_string")]
	pub author: String,
	#[serde(rename = "titleImage", alias = "title_image", alias = "TitleImage", alias = "image", alias = "imgName", default, deserialize_with = "any_to_string")]
	pub title_image: String,
	#[serde(default, deserialize_with = "any_to_vec_string")]
	pub tags: Vec<String>,
	#[serde(rename = "game", alias = "exeName", alias = "exe", alias = "gameExe", default, deserialize_with = "any_to_string")]
	pub game: String,
	#[serde(default, deserialize_with = "any_to_string")]
	pub version: String,
	#[serde(rename = "latestUpdate", alias = "lastUpdate", alias = "latest_update", default, deserialize_with = "date_to_string")]
	pub latest_update: String,
	#[serde(default, deserialize_with = "any_to_string")]
	pub description: String,
	// Only populated in the published game list; meta.json retains the sibling filename.
	#[serde(rename = "descriptionSource", default, skip_serializing_if = "Option::is_none")]
	pub description_source: Option<String>,
	
	#[serde(flatten, skip_serializing)]
	pub extra: Map<String, Value>,
}

#[cfg(test)]
mod tests
{
	use super::normalize_meta_date;

	#[test]
	fn normalizes_common_meta_date_spellings()
	{
		for value in ["2026/9/6", "2026-09-06", "2026.9.6", "2026年9月6日", "20260906", "2026-09-06T12:34:56Z"]
		{
			assert_eq!(normalize_meta_date(value), "2026/09/06");
		}
	}

	#[test]
	fn preserves_unknown_or_invalid_meta_dates()
	{
		assert_eq!(normalize_meta_date("秋ごろ"), "秋ごろ");
		assert_eq!(normalize_meta_date("2026-02-30"), "2026-02-30");
	}
}

pub fn init()
{
	let exe_path	= env::current_exe().expect("Couldnt get exe path");
	let root		= exe_path.parent().expect("Couldnt get exe parent");

	tracing::info!(server_root = %root.display(), "Server data directory ready");
	let games = root.join("games");

	fs::create_dir_all(&games).expect("Couldnt create dir.");

	extract_games::extract_games(root.to_path_buf(), games);
}
