use std::io::Read;
use std::path::Path;

pub const MAX_DESCRIPTION_BYTES: u64 = 1024 * 1024;

/// A single-line .md value is a sibling filename; all other values are inline Markdown.
pub fn markdown_file(value: &str) -> Result<Option<&str>, String>
{
	let name = value.trim();
	if name.contains(['\n', '\r']) || !name.to_ascii_lowercase().ends_with(".md")
	{
		return Ok(None);
	}
	if name.contains(['/', '\\', ':']) || name == ".md"
	{
		return Err("descriptionにはmeta.jsonと同じ階層のMDファイル名を指定してください".into());
	}
	Ok(Some(name))
}

/// Read the Markdown next to meta.json inside the distribution ZIP, including wrapper folders.
pub fn read_from_archive<R: Read + std::io::Seek>(
	archive: &mut zip::ZipArchive<R>,
	meta_path: &str,
	value: &str,
) -> Result<Option<String>, String>
{
	let Some(name) = markdown_file(value)? else { return Ok(None); };
	let meta_path = meta_path.replace('\\', "/");
	let expected = match meta_path.rsplit_once('/')
	{
		Some((directory, _)) => format!("{directory}/{name}"),
		None => name.to_string(),
	};
	for index in 0..archive.len()
	{
		let entry = archive.by_index(index).map_err(|error| error.to_string())?;
		let entry_name = crate::src::zip_utils::decode_filename(entry.name_raw()).replace('\\', "/");
		if !entry_name.eq_ignore_ascii_case(&expected) { continue; }
		if entry.is_dir() || entry.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000)
		{
			return Err(format!("説明ファイルは通常のファイルで指定してください: {name}"));
		}
		if entry.size() > MAX_DESCRIPTION_BYTES { return Err("説明ファイルは1 MiB以内にしてください".into()); }
		let mut bytes = Vec::new();
		entry.take(MAX_DESCRIPTION_BYTES + 1).read_to_end(&mut bytes).map_err(|error| error.to_string())?;
		if bytes.len() as u64 > MAX_DESCRIPTION_BYTES { return Err("説明ファイルは1 MiB以内にしてください".into()); }
		let text = String::from_utf8(bytes).map_err(|_| format!("説明ファイルはUTF-8で保存してください: {name}"))?;
		return Ok(Some(text.trim_start_matches('\u{feff}').to_string()));
	}
	Err(format!("説明ファイルがZIP内にありません: {expected}"))
}

pub fn read_from_zip(path: &Path, value: &str) -> Result<Option<String>, String>
{
	if markdown_file(value)?.is_none() { return Ok(None); }
	let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
	let mut archive = zip::ZipArchive::new(file).map_err(|error| error.to_string())?;
	let mut meta_path = None;
	for index in 0..archive.len()
	{
		let entry = archive.by_index(index).map_err(|error| error.to_string())?;
		let name = crate::src::zip_utils::decode_filename(entry.name_raw()).replace('\\', "/");
		if name == "meta.json" || name.ends_with("/meta.json")
		{
			if meta_path.is_some() { return Err("ZIP内にmeta.jsonが複数あります".into()); }
			meta_path = Some(name);
		}
	}
	let meta_path = meta_path.ok_or_else(|| "ZIP内にmeta.jsonがありません".to_string())?;
	read_from_archive(&mut archive, &meta_path, value)
}

pub fn read_from_game(directory: &Path, game_id: &str, value: &str) -> Result<Option<String>, String>
{
	if markdown_file(value)?.is_none() { return Ok(None); }
	let archive = crate::distribution::find_archive(directory, game_id).map_err(|error| error.to_string())?;
	read_from_zip(&archive, value)
}

#[cfg(test)]
mod tests
{
	use super::*;
	use std::io::{Cursor, Write};

	#[test]
	fn resolves_sibling_markdown_in_wrapped_archive()
	{
		let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
		for (name, content) in [("Game/meta.json", "{}"), ("Game/README.md", "\u{feff}# Overview\n\n- Play")]
		{
			writer.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
			writer.write_all(content.as_bytes()).unwrap();
		}
		let mut archive = zip::ZipArchive::new(writer.finish().unwrap()).unwrap();
		assert_eq!(read_from_archive(&mut archive, "Game/meta.json", "README.md").unwrap(), Some("# Overview\n\n- Play".into()));
		assert!(read_from_archive(&mut archive, "meta.json", "README.md").is_err());
		assert!(read_from_archive(&mut archive, "Game/meta.json", "missing.md").is_err());
	}

	#[test]
	fn rejects_non_sibling_paths_and_preserves_inline_text()
	{
		for name in ["../README.md", "folder/README.md", "C:\\README.md", "https://example.com/a.md"]
		{
			assert!(markdown_file(name).is_err());
		}
		assert_eq!(markdown_file("# Overview\nREADME.md").unwrap(), None);
		assert_eq!(markdown_file("ゲームの説明").unwrap(), None);
	}
}
