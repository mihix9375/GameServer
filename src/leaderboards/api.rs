use std::net::SocketAddr;

use axum::extract::{ConnectInfo, Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use super::{validate_game_id, GameLeaderboards, LeaderboardStore, SlotDefinition, LeaderboardError};

const LIST_ROUTE: &str = "/v1/games/{game_id}/leaderboards";
const SUBMIT_ROUTE: &str = "/v1/games/{game_id}/leaderboards/{board_id}/scores";

#[derive(Debug, Deserialize)]
struct SubmitScoreRequest
{
	player_name: String,
	score: crate::score::Score,
}

#[derive(Debug, Serialize)]
struct SubmitScoreResponse
{
	ok: bool,
	rank: usize,
}

#[derive(Debug, Serialize)]
struct ErrorResponse
{
	ok: bool,
	message: String,
}

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ErrorResponse>)>;

fn bad_request(message: String) -> (StatusCode, Json<ErrorResponse>)
{
	(StatusCode::BAD_REQUEST, Json(ErrorResponse { ok: false, message }))
}

fn store_error(error: LeaderboardError) -> (StatusCode, Json<ErrorResponse>) {
	let status = match &error { LeaderboardError::Invalid(_) => StatusCode::BAD_REQUEST, LeaderboardError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR };
	(status, Json(ErrorResponse { ok: false, message: error.to_string() }))
}

async fn list_leaderboards(
	State(store): State<LeaderboardStore>,
	ConnectInfo(address): ConnectInfo<SocketAddr>,
	Path(game_id): Path<String>,
) -> ApiResult<GameLeaderboards>
{
	validate_game_id(&game_id).map_err(bad_request)?;
	let response = store.get(&game_id).await;
	let response_bytes = serde_json::to_vec(&response).map_or(0, |value| value.len() as u64);
	crate::client_metrics::record_ip(address.ip(), response_bytes, game_id.len() as u64);
	Ok(Json(response))
}

async fn submit_score(
	State(store): State<LeaderboardStore>,
	ConnectInfo(address): ConnectInfo<SocketAddr>,
	Path((game_id, board_id)): Path<(String, String)>,
	Json(request): Json<SubmitScoreRequest>,
) -> ApiResult<SubmitScoreResponse>
{
	let rank = store
		.submit(&game_id, &board_id, &request.player_name, request.score)
		.await
		.map_err(store_error)?;
	let response = SubmitScoreResponse { ok: true, rank };
	let received_bytes = (game_id.len() + board_id.len() + request.player_name.len() + std::mem::size_of::<i64>()) as u64;
	let sent_bytes = serde_json::to_vec(&response).map_or(0, |value| value.len() as u64);
	crate::client_metrics::record_ip(address.ip(), sent_bytes, received_bytes);
	Ok(Json(response))
}

#[derive(Debug, Deserialize)]
struct SyncLeaderboardsRequest
{
	leaderboards: Vec<SlotDefinition>,
}

async fn sync_leaderboards(
	State(store): State<LeaderboardStore>,
	ConnectInfo(address): ConnectInfo<SocketAddr>,
	Path(game_id): Path<String>,
	Json(request): Json<SyncLeaderboardsRequest>,
) -> ApiResult<GameLeaderboards>
{
	let received_bytes = game_id.len() as u64
		+ serde_json::to_vec(&request.leaderboards).map_or(0, |value| value.len() as u64);
	let response = store.sync_slots(&game_id, request.leaderboards).await.map_err(store_error)?;
	let sent_bytes = serde_json::to_vec(&response).map_or(0, |value| value.len() as u64);
	crate::client_metrics::record_ip(address.ip(), sent_bytes, received_bytes);
	Ok(Json(response))
}

pub async fn serve(store: LeaderboardStore, bind: &str) -> Result<(), String>
{
	let address: SocketAddr = bind.parse()
		.map_err(|error| format!("admin-config.jsonのleaderboard_bindが不正です: {error}"))?;
	let app = Router::new()
		.route("/v1/games/{game_id}/metadata", get(crate::catalog::metadata))
		.route(LIST_ROUTE, get(list_leaderboards).put(sync_leaderboards))
		.route(SUBMIT_ROUTE, post(submit_score))
		.with_state(store);
	let listener = tokio::net::TcpListener::bind(address).await
		.map_err(|error| format!("ランキングAPIを{bind}で起動できません: {error}"))?;

	tracing::info!("Leaderboard API listening on http://{bind}");
	axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await.map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
	use super::*;
	#[tokio::test]
	async fn handlers_distinguish_invalid_input_from_storage_failure() {
		let root = std::env::temp_dir().join(format!("ranking-handler-{}-{}", std::process::id(), super::super::unix_time()));
		tokio::fs::create_dir_all(&root).await.unwrap();
		let store = LeaderboardStore::load(&root).await.unwrap();
		let address: SocketAddr = "127.0.0.1:12345".parse().unwrap();
		let request = || Json(SyncLeaderboardsRequest { leaderboards: vec![SlotDefinition { name: "Score".into(), order: super::super::RankingOrder::HighScore, enabled: true }] });
		let _ = sync_leaderboards(State(store.clone()), ConnectInfo(address), Path("game".into()), request()).await.unwrap();
		let invalid = sync_leaderboards(State(store.clone()), ConnectInfo(address), Path("game".into()), Json(SyncLeaderboardsRequest { leaderboards: vec![SlotDefinition { name: "x".repeat(41), order: super::super::RankingOrder::HighScore, enabled: true }] })).await.unwrap_err();
		assert_eq!(invalid.0, StatusCode::BAD_REQUEST);
		let path = root.join("leaderboards.json");
		let backup = root.join("backup.json");
		tokio::fs::rename(&path, &backup).await.unwrap();
		tokio::fs::create_dir(&path).await.unwrap();
		let failed = submit_score(State(store.clone()), ConnectInfo(address), Path(("game".into(), "0".into())), Json(SubmitScoreRequest { player_name: "A".into(), score: 42.into() })).await.unwrap_err();
		assert_eq!(failed.0, StatusCode::INTERNAL_SERVER_ERROR);
		assert!(failed.1.0.message.contains("保存できません"));
		assert!(store.get("game").await.leaderboards[0].entries.is_empty());
		tokio::fs::remove_dir_all(root).await.unwrap();
	}
}
