pub mod game_distributor;
pub mod game_version_checker;
pub mod update_notice;
pub mod comments;
pub mod game_files;

use std::cmp::Ordering;
use std::path::{Component, Path};
use tonic::Status;

pub fn normalize_game_id(game_id: &str) -> Result<String, Status>
{
	let value = game_id.trim();
	let value = if value.to_ascii_lowercase().ends_with(".exe")
	{
		&value[..value.len() - 4]
	}
	else
	{
		value
	};

	if value.is_empty()
		|| value == "."
		|| value == ".."
		|| value.contains(['/', '\\', ':'])
		|| !matches!(Path::new(value).components().collect::<Vec<_>>().as_slice(), [Component::Normal(_)])
	{
		return Err(Status::invalid_argument("不正なゲームIDです"));
	}

	Ok(value.to_string())
}

pub fn compare_versions(left: &str, right: &str) -> Result<Ordering, Status>
{
	fn parse(value: &str) -> Option<Vec<u64>>
	{
		let value = value.trim().strip_prefix(['v', 'V']).unwrap_or(value.trim());
		if value.is_empty() { return None; }
		value.split('.').map(|part| part.parse::<u64>().ok()).collect()
	}

	let mut left = parse(left).ok_or_else(|| Status::invalid_argument("現在のバージョン形式が不正です"))?;
	let mut right = parse(right).ok_or_else(|| Status::internal("配布バージョン形式が不正です"))?;
	let length = left.len().max(right.len());
	left.resize(length, 0);
	right.resize(length, 0);
	Ok(left.cmp(&right))
}

#[cfg(test)]
mod tests
{
	use super::*;

	#[test]
	fn rejects_game_id_paths()
	{
		for value in ["../game", "folder/game", "folder\\game", "C:\\game", ""]
		{
			assert!(normalize_game_id(value).is_err(), "{value}");
		}
	}

	#[test]
	fn compares_prefixed_versions_numerically()
	{
		assert_eq!(compare_versions("v1.9.0", "v2.0.0").unwrap(), Ordering::Less);
		assert_eq!(compare_versions("1.10", "v1.2.0").unwrap(), Ordering::Greater);
		assert_eq!(compare_versions("v1.0", "1.0.0").unwrap(), Ordering::Equal);
	}
}
