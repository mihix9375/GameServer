use std::io::Write;
use std::path::Path;

/// Replace a complete file on the same filesystem. Never truncate/delete the old file first.
pub fn write_atomic(path: &Path, content: &[u8]) -> std::io::Result<()> {
	static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
	let nonce = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
	let time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
	let temporary = path.with_file_name(format!(".storage-{}-{time}-{nonce}.tmp", std::process::id()));
	let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temporary)?;
	let result = (|| {
		file.write_all(content)?;
		file.sync_all()?;
		drop(file);
		std::fs::rename(&temporary, path)
	})();
	if result.is_err() { let _ = std::fs::remove_file(&temporary); }
	result
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn replaces_complete_files_and_cleans_up_failed_replacements() {
		let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
		let root = std::env::temp_dir().join(format!("gameserver-atomic-{}-{stamp}", std::process::id()));
		std::fs::create_dir_all(&root).unwrap();
		let path = root.join("catalog.json");
		write_atomic(&path, b"old").unwrap();
		write_atomic(&path, b"new").unwrap();
		assert_eq!(std::fs::read(&path).unwrap(), b"new");
		let blocked = root.join("blocked");
		std::fs::create_dir(&blocked).unwrap();
		assert!(write_atomic(&blocked, b"content").is_err());
		assert!(blocked.is_dir());
		assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
		std::fs::remove_dir_all(root).unwrap();
	}
}
