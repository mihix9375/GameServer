use std::net::SocketAddr;
use std::path::Path;

use axum::extract::{ConnectInfo, Path as ApiPath};
use axum::http::StatusCode;
use axum::Json;

use crate::init::Meta;

fn select_metadata(content: &str, game_id: &str) -> Result<Meta, (StatusCode, String)>
{
	let games: Vec<Meta> = serde_json::from_str(content)
		.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "ゲーム一覧が不正です".into()))?;
	let mut game = games.into_iter().find(|game| game.id == game_id)
		.ok_or_else(|| (StatusCode::NOT_FOUND, "ゲームが見つかりません".to_string()))?;
	// Relative thumbnail paths refer to files in the ZIP, not to this HTTP server.
	if !game.title_image.starts_with("https://") && !game.title_image.starts_with("http://") && !game.title_image.starts_with("data:image/")
	{
		game.title_image.clear();
	}
	game.description_source = None;
	Ok(game)
}

pub async fn metadata(
	ApiPath(game_id): ApiPath<String>,
	ConnectInfo(address): ConnectInfo<SocketAddr>,
) -> Result<Json<Meta>, (StatusCode, String)>
{
	let game_id = crate::net::normalize_game_id(&game_id)
		.map_err(|_| (StatusCode::BAD_REQUEST, "ゲームIDが不正です".into()))?;
	let executable = std::env::current_exe()
		.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Serverの場所を取得できません".to_string()))?;
	let root: &Path = executable.parent()
		.ok_or_else(|| (StatusCode::INTERNAL_SERVER_ERROR, "Serverの場所を取得できません".to_string()))?;
	let content = tokio::fs::read_to_string(root.join("games/games.json")).await
		.map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "ゲーム一覧を読み込めません".to_string()))?;
	let game = select_metadata(&content, &game_id)?;
	let sent_bytes = serde_json::to_vec(&game).map_or(0, |bytes| bytes.len() as u64);
	crate::client_metrics::record_ip(address.ip(), sent_bytes, game_id.len() as u64);
	Ok(Json(game))
}

#[cfg(test)]
mod tests
{
	use super::*;

	#[test]
	fn publishes_title_and_resolved_description_without_local_image_paths()
	{
		let content = r##"[{"id":"internal-id","title":"表示用タイトル","description":"# 遊び方\n\n- 移動","descriptionSource":"README.md","titleImage":"title.png","version":"1.0.0"}]"##;
		let game = select_metadata(content, "internal-id").unwrap();
		assert_eq!(game.title, "表示用タイトル");
		assert_eq!(game.description, "# 遊び方\n\n- 移動");
		assert_eq!(game.description_source, None);
		assert!(game.title_image.is_empty());
		assert_eq!(select_metadata(content, "missing").unwrap_err().0, StatusCode::NOT_FOUND);
	}
}
