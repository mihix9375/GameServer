use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;
use std::time::UNIX_EPOCH;

use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, Mutex};
use tonic::codegen::tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use zip::ZipArchive;

use crate::gamelauncher::{DownloadRequest, GameFile, GameFileData, GameFilesRequest, GameManifest};

const FILE_CHUNK_SIZE: usize = 2 * 1024 * 1024;
const STREAM_QUEUE_SIZE: usize = 8;

pub type GameFileStream = ReceiverStream<Result<GameFileData, Status>>;

#[derive(Clone)]
struct CachedManifest
{
	archive_size: u64,
	modified_nanos: u128,
	manifest: GameManifest,
}

static MANIFEST_CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedManifest>>> = OnceLock::new();

pub async fn handle_manifest(
	request: Request<DownloadRequest>,
) -> Result<Response<GameManifest>, Status>
{
	let request = request.into_inner();
	let game_id = crate::net::normalize_game_id(&request.game_id)?;
	let (archive_path, version) = find_archive_and_version(&game_id).await?;
	validate_requested_version(&request.version, &version)?;
	let manifest = load_manifest(archive_path, game_id, version).await?;
	Ok(Response::new(manifest))
}

pub async fn handle_download_files(
	request: Request<GameFilesRequest>,
) -> Result<Response<GameFileStream>, Status>
{
	let request = request.into_inner();
	let game_id = crate::net::normalize_game_id(&request.game_id)?;
	let (archive_path, version) = find_archive_and_version(&game_id).await?;
	validate_requested_version(&request.version, &version)?;
	let manifest = load_manifest(archive_path.clone(), game_id, version).await?;
	let requested_paths = validate_requested_paths(&manifest, request.paths)?;

	let (sender, receiver) = mpsc::channel(STREAM_QUEUE_SIZE);
	tokio::task::spawn_blocking(move || {
		if let Err(error) = stream_files(&archive_path, &requested_paths, &sender)
		{
			let _ = sender.blocking_send(Err(error));
		}
	});
	Ok(Response::new(ReceiverStream::new(receiver)))
}

async fn find_archive_and_version(game_id: &str) -> Result<(PathBuf, String), Status>
{
	let executable = std::env::current_exe()
		.map_err(|error| Status::internal(format!("実行ファイルの場所を取得できません: {error}")))?;
	let root = executable.parent()
		.ok_or_else(|| Status::internal("実行ファイルの場所を取得できません"))?;
	let game_directory = root.join("games").join(game_id);
	let meta_content = tokio::fs::read_to_string(game_directory.join("meta.json")).await
		.map_err(|_| Status::not_found("meta.jsonが見つかりません"))?;
	let meta: crate::init::Meta = serde_json::from_str(&meta_content)
		.map_err(|error| Status::internal(format!("meta.jsonを解析できません: {error}")))?;
	let expected_archive = game_directory.join(format!("{game_id}.zip"));
	let archive = if expected_archive.is_file()
	{
		expected_archive
	}
	else
	{
		let candidates = std::fs::read_dir(&game_directory)
			.map_err(|_| Status::not_found("配布ZIPが見つかりません"))?
			.flatten()
			.map(|entry| entry.path())
			.filter(|path| path.extension().and_then(|value| value.to_str())
				.is_some_and(|value| value.eq_ignore_ascii_case("zip")))
			.collect::<Vec<_>>();
		match candidates.as_slice()
		{
			[archive] => archive.clone(),
			[] => return Err(Status::not_found("配布ZIPが見つかりません")),
			_ => return Err(Status::failed_precondition("配布対象のZIPを一意に決定できません")),
		}
	};
	Ok((archive, meta.version))
}

fn validate_requested_version(requested: &str, current: &str) -> Result<(), Status>
{
	if requested.trim().is_empty() || !crate::net::compare_versions(requested, current)?.is_eq()
	{
		return Err(Status::failed_precondition(format!(
			"要求されたバージョン {requested} は配布できません（最新: {current}）"
		)));
	}
	Ok(())
}

async fn load_manifest(
	archive_path: PathBuf,
	game_id: String,
	version: String,
) -> Result<GameManifest, Status>
{
	let metadata = tokio::fs::metadata(&archive_path).await
		.map_err(|error| Status::internal(format!("配布ZIPを確認できません: {error}")))?;
	let modified_nanos = metadata.modified().unwrap_or(UNIX_EPOCH)
		.duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
	let cache = MANIFEST_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let mut cache = cache.lock().await;
	if let Some(cached) = cache.get(&archive_path)
	{
		if cached.archive_size == metadata.len() && cached.modified_nanos == modified_nanos
		{
			return Ok(cached.manifest.clone());
		}
	}

	let build_path = archive_path.clone();
	let manifest = tokio::task::spawn_blocking(move || build_manifest(&build_path, game_id, version))
		.await
		.map_err(|error| Status::internal(format!("マニフェスト生成処理に失敗しました: {error}")))??;
	cache.insert(archive_path, CachedManifest {
		archive_size: metadata.len(),
		modified_nanos,
		manifest: manifest.clone(),
	});
	Ok(manifest)
}

fn build_manifest(
	archive_path: &Path,
	game_id: String,
	version: String,
) -> Result<GameManifest, Status>
{
	let file = File::open(archive_path)
		.map_err(|error| Status::internal(format!("配布ZIPを開けません: {error}")))?;
	let mut archive = ZipArchive::new(file)
		.map_err(|error| Status::internal(format!("配布ZIPが不正です: {error}")))?;
	let mut files = Vec::new();
	let mut paths = HashSet::new();
	let mut buffer = vec![0u8; FILE_CHUNK_SIZE];

	for index in 0..archive.len()
	{
		let mut entry = archive.by_index(index)
			.map_err(|error| Status::internal(format!("ZIPエントリを読めません: {error}")))?;
		if entry.is_dir() { continue; }
		if entry.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000)
		{
			return Err(Status::failed_precondition("ZIP内のシンボリックリンクは使用できません"));
		}
		let path = safe_archive_path(entry.name_raw())?;
		if !paths.insert(path.clone())
		{
			return Err(Status::failed_precondition(format!("ZIP内のパスが重複しています: {path}")));
		}
		let mut hasher = Sha256::new();
		loop
		{
			let read = entry.read(&mut buffer)
				.map_err(|error| Status::internal(format!("ZIPエントリを読めません: {error}")))?;
			if read == 0 { break; }
			hasher.update(&buffer[..read]);
		}
		files.push(GameFile {
			path,
			size: entry.size(),
			sha256: format!("{:x}", hasher.finalize()),
		});
	}
	files.sort_by(|left, right| left.path.cmp(&right.path));
	let archive_size = std::fs::metadata(archive_path)
		.map_err(|error| Status::internal(format!("配布ZIPを確認できません: {error}")))?
		.len();
	Ok(GameManifest { game_id, version, files, archive_size })
}

fn validate_requested_paths(
	manifest: &GameManifest,
	requested: Vec<String>,
) -> Result<Vec<String>, Status>
{
	let available = manifest.files.iter().map(|file| file.path.as_str()).collect::<HashSet<_>>();
	let mut unique = HashSet::new();
	let mut paths = Vec::new();
	for path in requested
	{
		if !available.contains(path.as_str())
		{
			return Err(Status::invalid_argument(format!("配布対象にないファイルです: {path}")));
		}
		if unique.insert(path.clone()) { paths.push(path); }
	}
	Ok(paths)
}

fn stream_files(
	archive_path: &Path,
	requested_paths: &[String],
	sender: &mpsc::Sender<Result<GameFileData, Status>>,
) -> Result<(), Status>
{
	let file = File::open(archive_path)
		.map_err(|error| Status::internal(format!("配布ZIPを開けません: {error}")))?;
	let mut archive = ZipArchive::new(file)
		.map_err(|error| Status::internal(format!("配布ZIPが不正です: {error}")))?;
	let requested = requested_paths.iter().map(String::as_str).collect::<HashSet<_>>();
	let mut indexes = HashMap::new();
	for index in 0..archive.len()
	{
		let entry = archive.by_index(index)
			.map_err(|error| Status::internal(format!("ZIPエントリを読めません: {error}")))?;
		if !entry.is_dir()
		{
			let path = safe_archive_path(entry.name_raw())?;
			if requested.contains(path.as_str()) { indexes.insert(path, index); }
		}
	}

	let mut buffer = vec![0u8; FILE_CHUNK_SIZE];
	for path in requested_paths
	{
		let index = indexes.get(path)
			.ok_or_else(|| Status::not_found(format!("ZIP内にファイルがありません: {path}")))?;
		let mut entry = archive.by_index(*index)
			.map_err(|error| Status::internal(format!("ZIPエントリを読めません: {error}")))?;
		let mut offset = 0u64;
		loop
		{
			let read = entry.read(&mut buffer)
				.map_err(|error| Status::internal(format!("ZIPエントリを読めません: {error}")))?;
			if read == 0 { break; }
			sender.blocking_send(Ok(GameFileData {
				path: path.clone(),
				data: buffer[..read].to_vec(),
				offset,
				complete: false,
			})).map_err(|_| Status::cancelled("ダウンロードが中断されました"))?;
			offset += read as u64;
		}
		sender.blocking_send(Ok(GameFileData {
			path: path.clone(),
			data: Vec::new(),
			offset,
			complete: true,
		})).map_err(|_| Status::cancelled("ダウンロードが中断されました"))?;
	}
	Ok(())
}

fn safe_archive_path(raw_name: &[u8]) -> Result<String, Status>
{
	let decoded = crate::src::zip_utils::decode_filename(raw_name);
	let path = Path::new(&decoded);
	if decoded.trim().is_empty()
		|| decoded.contains(':')
		|| decoded.eq_ignore_ascii_case(".gamelauncher-manifest.json")
		|| path.components().any(|component| !matches!(component, Component::Normal(_)))
	{
		return Err(Status::failed_precondition(format!("ZIP内に不正なパスがあります: {decoded}")));
	}
	Ok(path.components()
		.filter_map(|component| match component { Component::Normal(value) => Some(value.to_string_lossy()), _ => None })
		.collect::<Vec<_>>()
		.join("/"))
}

#[cfg(test)]
mod tests
{
	use super::*;
	use std::io::Write;
	use zip::write::SimpleFileOptions;

	fn test_directory(name: &str) -> PathBuf
	{
		std::env::temp_dir().join(format!("gameserver-manifest-{name}-{}", std::process::id()))
	}

	#[test]
	fn builds_sorted_manifest_with_sha256()
	{
		let root = test_directory("build");
		let _ = std::fs::remove_dir_all(&root);
		std::fs::create_dir_all(&root).unwrap();
		let archive_path = root.join("game.zip");
		let output = File::create(&archive_path).unwrap();
		let mut zip = zip::ZipWriter::new(output);
		zip.start_file("b.txt", SimpleFileOptions::default()).unwrap();
		zip.write_all(b"second").unwrap();
		zip.start_file("a.txt", SimpleFileOptions::default()).unwrap();
		zip.write_all(b"first").unwrap();
		zip.finish().unwrap();

		let manifest = build_manifest(&archive_path, "game".into(), "1.0.0".into()).unwrap();
		assert_eq!(manifest.files.iter().map(|file| file.path.as_str()).collect::<Vec<_>>(), ["a.txt", "b.txt"]);
		assert_eq!(manifest.files[0].size, 5);
		assert_eq!(manifest.archive_size, std::fs::metadata(&archive_path).unwrap().len());
		assert_eq!(manifest.files[0].sha256, format!("{:x}", Sha256::digest(b"first")));
		let _ = std::fs::remove_dir_all(root);
	}

	#[test]
	fn rejects_unsafe_archive_paths()
	{
		assert!(safe_archive_path(b"../outside.txt").is_err());
		assert!(safe_archive_path(b"C:/outside.txt").is_err());
		assert!(safe_archive_path(b".gamelauncher-manifest.json").is_err());
	}

	#[tokio::test]
	async fn streams_only_requested_files()
	{
		let root = test_directory("stream");
		let _ = std::fs::remove_dir_all(&root);
		std::fs::create_dir_all(&root).unwrap();
		let archive_path = root.join("game.zip");
		let output = File::create(&archive_path).unwrap();
		let mut zip = zip::ZipWriter::new(output);
		zip.start_file("wanted.txt", SimpleFileOptions::default()).unwrap();
		zip.write_all(b"wanted").unwrap();
		zip.start_file("ignored.txt", SimpleFileOptions::default()).unwrap();
		zip.write_all(b"ignored").unwrap();
		zip.finish().unwrap();

		let (sender, mut receiver) = mpsc::channel(8);
		let task_path = archive_path.clone();
		let task = tokio::task::spawn_blocking(move || {
			stream_files(&task_path, &["wanted.txt".into()], &sender)
		});
		let mut received = Vec::new();
		while let Some(chunk) = receiver.recv().await
		{
			let chunk = chunk.unwrap();
			assert_eq!(chunk.path, "wanted.txt");
			received.extend(chunk.data);
			if chunk.complete { break; }
		}
		task.await.unwrap().unwrap();
		assert_eq!(received, b"wanted");
		let _ = std::fs::remove_dir_all(root);
	}
}
