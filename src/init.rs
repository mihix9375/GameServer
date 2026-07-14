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
	#[serde(rename = "latestUpdate", alias = "lastUpdate", alias = "latest_update", default, deserialize_with = "any_to_string")]
	pub latest_update: String,
	#[serde(default, deserialize_with = "any_to_string")]
	pub description: String,
	
	#[serde(flatten, skip_serializing)]
	pub extra: Map<String, Value>,
}

pub fn init()
{
	let exe_path	= env::current_exe().expect("Couldnt get exe path");
	let root		= exe_path.parent().expect("Couldnt get exe parent");

	println!("{}", root.display());
	let games = root.join("games");

	fs::create_dir_all(&games).expect("Couldnt create dir.");

	extract_games::extract_games(root.to_path_buf(), games);
}
