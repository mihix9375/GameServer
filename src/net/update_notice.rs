use crate::gamelauncher::{
	Identificial, UpdateAction, UpdateNotice
};
use tonic::{
	Response, Request, Status
};
use tonic::codegen::tokio_stream::wrappers::ReceiverStream;

use std::sync::Arc;
use std::collections::HashSet;
use std::path::PathBuf;
use tokio::sync::broadcast;

static REMOVALS_LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();

pub type UpdateNoticeStream = ReceiverStream<Result<UpdateNotice, Status>>;

pub fn upsert_notice(game_id: String, version: String) -> UpdateNotice
{
	UpdateNotice { game_id, version, action: UpdateAction::Upsert as i32 }
}

pub fn delete_notice(game_id: String) -> UpdateNotice
{
	UpdateNotice { game_id, version: String::new(), action: UpdateAction::Delete as i32 }
}

fn removals_path() -> Result<PathBuf, String>
{
	let executable = std::env::current_exe().map_err(|error| error.to_string())?;
	let root = executable.parent().ok_or_else(|| "実行ファイルの場所を取得できません".to_string())?;
	Ok(root.join("removed-games.json"))
}

async fn read_removals(path: &PathBuf) -> Result<HashSet<String>, String>
{
	match tokio::fs::read_to_string(path).await
	{
		Ok(content) => serde_json::from_str(&content)
			.map_err(|error| format!("removed-games.jsonが不正です: {error}")),
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(HashSet::new()),
		Err(error) => Err(format!("removed-games.jsonを読めません: {error}")),
	}
}

async fn write_removals(path: &PathBuf, removals: &HashSet<String>) -> Result<(), String>
{
	let mut values = removals.iter().cloned().collect::<Vec<_>>();
	values.sort();
	let content = serde_json::to_string_pretty(&values).map_err(|error| error.to_string())?;
	tokio::fs::write(path, content).await
		.map_err(|error| format!("removed-games.jsonを保存できません: {error}"))
}

pub async fn record_removal(game_id: &str) -> Result<(), String>
{
	let _guard = REMOVALS_LOCK.get_or_init(|| tokio::sync::Mutex::new(())).lock().await;
	let path = removals_path()?;
	let mut removals = read_removals(&path).await?;
	removals.insert(game_id.to_string());
	write_removals(&path, &removals).await
}

pub async fn clear_published_removals<'a>(game_ids: impl IntoIterator<Item = &'a String>) -> Result<(), String>
{
	let _guard = REMOVALS_LOCK.get_or_init(|| tokio::sync::Mutex::new(())).lock().await;
	let path = removals_path()?;
	let mut removals = read_removals(&path).await?;
	let previous_len = removals.len();
	for game_id in game_ids { removals.remove(game_id); }
	if removals.len() != previous_len { write_removals(&path, &removals).await?; }
	Ok(())
}

async fn removed_game_ids() -> Result<HashSet<String>, String>
{
	let _guard = REMOVALS_LOCK.get_or_init(|| tokio::sync::Mutex::new(())).lock().await;
	read_removals(&removals_path()?).await
}

fn deletion_targets(
	installed: Vec<String>,
	published: &HashSet<String>,
	removed: &HashSet<String>,
) -> Vec<String>
{
	let mut unique = HashSet::new();
	installed.into_iter()
		.filter_map(|game_id| crate::net::normalize_game_id(&game_id).ok())
		.filter(|game_id| !published.contains(game_id) && removed.contains(game_id))
		.filter(|game_id| unique.insert(game_id.clone()))
		.collect()
}

pub async fn send_update_notice(
	request: Request<Identificial>,
	tx: &Arc<broadcast::Sender<UpdateNotice>>,
) -> Result<Response<UpdateNoticeStream>, Status>
{
	let installed_game_ids = request.into_inner().installed_game_ids;
	let removed_game_ids = removed_game_ids().await.unwrap_or_default();
	let mut rx = tx.subscribe();
	let (tx_stream, rx_stream) = tokio::sync::mpsc::channel(32);

	tokio::spawn(async move {
		if let Ok(exe_path) = std::env::current_exe() {
			if let Some(root) = exe_path.parent() {
				let games_json = root.join("games").join("games.json");
				if let Ok(content) = std::fs::read_to_string(&games_json) {
					if let Ok(metas) = serde_json::from_str::<Vec<crate::init::Meta>>(&content) {
						let mut published_ids = std::collections::HashSet::new();
						for meta in metas {
							let raw_id = if !meta.id.is_empty() { &meta.id } else { &meta.game };
							let Ok(clean_id) = crate::net::normalize_game_id(raw_id) else { continue; };
							published_ids.insert(clean_id.clone());
							let notice = upsert_notice(clean_id, meta.version.clone());
							if tx_stream.send(Ok(notice)).await.is_err() {
								return;
							}
						}
						for clean_id in deletion_targets(installed_game_ids, &published_ids, &removed_game_ids)
						{
							if tx_stream.send(Ok(delete_notice(clean_id))).await.is_err() { return; }
						}
					}
				}
			}
		}

		while let Ok(notice) = rx.recv().await {
			if tx_stream.send(Ok(notice)).await.is_err() {
				break;
			}
		}
	});

	Ok(Response::new(ReceiverStream::new(rx_stream)))
}

#[cfg(test)]
mod tests
{
	use super::*;

	#[test]
	fn reconciles_only_games_explicitly_removed_by_this_server()
	{
		let installed = vec!["removed".into(), "local-only".into(), "published".into(), "removed".into()];
		let published = HashSet::from(["published".to_string()]);
		let removed = HashSet::from(["removed".to_string(), "published".to_string()]);
		assert_eq!(deletion_targets(installed, &published, &removed), ["removed"]);
	}
}
