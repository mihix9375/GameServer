use std::net::SocketAddr;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use super::{validate_game_id, GameLeaderboards, LeaderboardStore};

const LIST_ROUTE: &str = "/v1/games/{game_id}/leaderboards";
const SUBMIT_ROUTE: &str = "/v1/games/{game_id}/leaderboards/{board_id}/scores";

#[derive(Debug, Deserialize)]
struct SubmitScoreRequest
{
	player_name: String,
	score: i64,
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

async fn list_leaderboards(
	State(store): State<LeaderboardStore>,
	Path(game_id): Path<String>,
) -> ApiResult<GameLeaderboards>
{
	validate_game_id(&game_id).map_err(bad_request)?;
	Ok(Json(store.get(&game_id).await))
}

async fn submit_score(
	State(store): State<LeaderboardStore>,
	Path((game_id, board_id)): Path<(String, String)>,
	Json(request): Json<SubmitScoreRequest>,
) -> ApiResult<SubmitScoreResponse>
{
	let rank = store
		.submit(&game_id, &board_id, &request.player_name, request.score)
		.await
		.map_err(bad_request)?;
	Ok(Json(SubmitScoreResponse { ok: true, rank }))
}

pub async fn serve(store: LeaderboardStore, bind: &str) -> Result<(), String>
{
	let address: SocketAddr = bind.parse()
		.map_err(|error| format!("admin-config.jsonのleaderboard_bindが不正です: {error}"))?;
	let app = Router::new()
		.route(LIST_ROUTE, get(list_leaderboards))
		.route(SUBMIT_ROUTE, post(submit_score))
		.with_state(store);
	let listener = tokio::net::TcpListener::bind(address).await
		.map_err(|error| format!("ランキングAPIを{bind}で起動できません: {error}"))?;

	println!("Leaderboard API listening on http://{bind}");
	axum::serve(listener, app).await.map_err(|error| error.to_string())
}
