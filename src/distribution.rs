use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum ArchiveError
{
	Missing,
	Ambiguous,
}

impl std::fmt::Display for ArchiveError
{
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
	{
		formatter.write_str(match self {
			Self::Missing => "配布ZIPが見つかりません",
			Self::Ambiguous => "配布ZIPを一意に決定できません",
		})
	}
}

/// Prefer the ID-named archive; a legacy name is allowed only when there is one candidate.
pub fn find_archive(directory: &Path, game_id: &str) -> Result<PathBuf, ArchiveError>
{
	let expected = directory.join(format!("{game_id}.zip"));
	if expected.is_file() { return Ok(expected); }
	let candidates: Vec<_> = std::fs::read_dir(directory)
		.map_err(|_| ArchiveError::Missing)?
		.flatten().map(|entry| entry.path())
		.filter(|path| path.is_file() && path.extension().and_then(|value| value.to_str())
			.is_some_and(|value| value.eq_ignore_ascii_case("zip")))
		.collect();
	match candidates.as_slice()
	{
		[archive] => Ok(archive.clone()),
		[] => Err(ArchiveError::Missing),
		_ => Err(ArchiveError::Ambiguous),
	}
}

#[cfg(test)]
mod tests
{
	use super::*;

	#[test]
	fn archive_selection_handles_legacy_names_and_ambiguity()
	{
		let root = std::env::temp_dir().join(format!("gameserver-distribution-{}", std::process::id()));
		std::fs::create_dir_all(&root).unwrap();
		assert!(matches!(find_archive(&root, "game"), Err(ArchiveError::Missing)));
		std::fs::create_dir_all(root.join("not-a-file.zip")).unwrap();
		std::fs::write(root.join("legacy.ZIP"), []).unwrap();
		assert_eq!(find_archive(&root, "game").unwrap(), root.join("legacy.ZIP"));
		std::fs::write(root.join("other.zip"), []).unwrap();
		assert!(matches!(find_archive(&root, "game"), Err(ArchiveError::Ambiguous)));
		std::fs::write(root.join("game.zip"), []).unwrap();
		assert_eq!(find_archive(&root, "game").unwrap(), root.join("game.zip"));
		std::fs::remove_dir_all(root).unwrap();
	}
}
