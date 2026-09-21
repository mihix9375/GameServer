use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::extract::{DefaultBodyLimit, Multipart, Path as AxumPath, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::sync::{broadcast, Mutex};

use crate::gamelauncher::{Comment, UpdateNotice};
use crate::init::Meta;

const INDEX_HTML: &str = include_str!("../admin/index.html");
const APP_CSS: &str = include_str!("../admin/app.css");
const APP_JS: &str = include_str!("../admin/app.js");

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AdminConfig
{
	#[serde(default = "default_bind")]
	bind: String,
	#[serde(default)]
	token: String,
	#[serde(default = "default_leaderboard_bind")]
	leaderboard_bind: String,
}

fn default_bind() -> String { "127.0.0.1:50051".to_string() }
fn default_leaderboard_bind() -> String { "0.0.0.0:50052".to_string() }

impl Default for AdminConfig
{
	fn default() -> Self
	{
		Self { bind: default_bind(), token: String::new(), leaderboard_bind: default_leaderboard_bind() }
	}
}

#[derive(Clone)]
struct AdminState
{
	root: PathBuf,
	bind: String,
	leaderboard_bind: String,
	token: String,
	started_at: Instant,
	updates: Arc<broadcast::Sender<UpdateNotice>>,
	operation_lock: Arc<Mutex<()>>,
	leaderboards: crate::leaderboards::LeaderboardStore,
}

#[derive(Serialize)]
struct StatusResponse
{
	status: &'static str,
	grpc: [&'static str; 2],
	admin: String,
	leaderboard_api: String,
	uptime_seconds: u64,
	games_directory: String,
	comments_file: String,
	remote_access: bool,
}

#[derive(Serialize)]
struct AdminComment
{
	id: String,
	game_id: String,
	author: String,
	content: String,
	created_at: i64,
}

impl From<Comment> for AdminComment
{
	fn from(value: Comment) -> Self
	{
		Self {
			id: value.id,
			game_id: value.game_id,
			author: value.author,
			content: value.content,
			created_at: value.created_at,
		}
	}
}

#[derive(Serialize)]
struct ActionResponse
{
	ok: bool,
	message: String,
}

#[derive(Deserialize)]
struct GameUpdate
{
	title: String,
	author: String,
	version: String,
	latest_update: String,
	description: String,
	tags: Vec<String>,
}

#[derive(Deserialize)]
struct LeaderboardUpdate
{
	leaderboards: Vec<crate::leaderboards::BoardDefinition>,
}

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ActionResponse>)>;

fn api_error(status: StatusCode, message: impl Into<String>) -> (StatusCode, Json<ActionResponse>)
{
	(status, Json(ActionResponse { ok: false, message: message.into() }))
}

fn authorize(state: &AdminState, headers: &HeaderMap) -> Result<(), (StatusCode, Json<ActionResponse>)>
{
	if state.token.is_empty()
	{
		return Ok(());
	}
	let expected = format!("Bearer {}", state.token);
	let supplied = headers.get(header::AUTHORIZATION).and_then(|value| value.to_str().ok());
	if supplied == Some(expected.as_str())
	{
		Ok(())
	}
	else
	{
		Err(api_error(StatusCode::UNAUTHORIZED, "管理トークンが必要です"))
	}
}

fn executable_root() -> Result<PathBuf, String>
{
	let executable = std::env::current_exe().map_err(|error| error.to_string())?;
	executable.parent().map(Path::to_path_buf).ok_or_else(|| "実行ファイルの場所を取得できません".to_string())
}

fn load_config(root: &Path) -> Result<AdminConfig, String>
{
	let path = root.join("admin-config.json");
	if !path.exists()
	{
		let config = AdminConfig::default();
		let json = serde_json::to_string_pretty(&config).map_err(|error| error.to_string())?;
		std::fs::write(&path, json).map_err(|error| format!("admin-config.jsonを作成できません: {error}"))?;
		return Ok(config);
	}
	let content = std::fs::read_to_string(path).map_err(|error| format!("admin-config.jsonを読み込めません: {error}"))?;
	serde_json::from_str(&content).map_err(|error| format!("admin-config.jsonが不正です: {error}"))
}

pub fn leaderboard_bind(root: &Path) -> Result<String, String>
{
	Ok(load_config(root)?.leaderboard_bind)
}

async fn index() -> Html<&'static str> { Html(INDEX_HTML) }

async fn css() -> impl IntoResponse
{
	([(header::CONTENT_TYPE, "text/css; charset=utf-8")], APP_CSS)
}

async fn js() -> impl IntoResponse
{
	([(header::CONTENT_TYPE, "text/javascript; charset=utf-8")], APP_JS)
}

async fn status(State(state): State<AdminState>, headers: HeaderMap) -> ApiResult<StatusResponse>
{
	authorize(&state, &headers)?;
	let address: SocketAddr = state.bind.parse().map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "管理画面のアドレスが不正です"))?;
	Ok(Json(StatusResponse {
		status: "online",
		grpc: ["0.0.0.0:50050", "[::]:50050"],
		admin: state.bind.clone(),
		leaderboard_api: state.leaderboard_bind.clone(),
		uptime_seconds: state.started_at.elapsed().as_secs(),
		games_directory: state.root.join("games").display().to_string(),
		comments_file: state.root.join("comments.jsonl").display().to_string(),
		remote_access: !address.ip().is_loopback(),
	}))
}

async fn get_leaderboards(
	State(state): State<AdminState>,
	headers: HeaderMap,
	AxumPath(game_id): AxumPath<String>,
) -> ApiResult<Vec<crate::leaderboards::BoardDefinition>>
{
	authorize(&state, &headers)?;
	let clean_id = crate::net::normalize_game_id(&game_id)
		.map_err(|error| api_error(StatusCode::BAD_REQUEST, error.message().to_string()))?;
	find_game_directory(&state.root, &clean_id)?;
	Ok(Json(state.leaderboards.definitions(&clean_id).await))
}

async fn update_leaderboards(
	State(state): State<AdminState>,
	headers: HeaderMap,
	AxumPath(game_id): AxumPath<String>,
	Json(update): Json<LeaderboardUpdate>,
) -> ApiResult<ActionResponse>
{
	authorize(&state, &headers)?;
	let clean_id = crate::net::normalize_game_id(&game_id)
		.map_err(|error| api_error(StatusCode::BAD_REQUEST, error.message().to_string()))?;
	find_game_directory(&state.root, &clean_id)?;
	state.leaderboards.configure(&clean_id, update.leaderboards).await
		.map_err(|error| api_error(StatusCode::BAD_REQUEST, error))?;
	Ok(Json(ActionResponse { ok: true, message: format!("{clean_id}のランキング設定を保存しました") }))
}

async fn games(State(state): State<AdminState>, headers: HeaderMap) -> ApiResult<Vec<Meta>>
{
	authorize(&state, &headers)?;
	let path = state.root.join("games").join("games.json");
	let content = match tokio::fs::read_to_string(path).await
	{
		Ok(content) => content,
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Json(Vec::new())),
		Err(error) => return Err(api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("ゲーム一覧を読めません: {error}"))),
	};
	let games = serde_json::from_str(&content)
		.map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("games.jsonが不正です: {error}")))?;
	Ok(Json(games))
}

async fn comments(State(state): State<AdminState>, headers: HeaderMap) -> ApiResult<Vec<AdminComment>>
{
	authorize(&state, &headers)?;
	let comments = crate::net::comments::list_all_comments().await
		.map_err(|message| api_error(StatusCode::INTERNAL_SERVER_ERROR, message))?;
	Ok(Json(comments.into_iter().map(AdminComment::from).collect()))
}

async fn remove_comment(
	State(state): State<AdminState>,
	headers: HeaderMap,
	AxumPath(comment_id): AxumPath<String>,
) -> ApiResult<ActionResponse>
{
	authorize(&state, &headers)?;
	let removed = crate::net::comments::delete_comment(&comment_id).await
		.map_err(|message| api_error(StatusCode::INTERNAL_SERVER_ERROR, message))?;
	if !removed
	{
		return Err(api_error(StatusCode::NOT_FOUND, "コメントが見つかりません"));
	}
	Ok(Json(ActionResponse { ok: true, message: "コメントを削除しました".to_string() }))
}

async fn rescan(State(state): State<AdminState>, headers: HeaderMap) -> ApiResult<ActionResponse>
{
	authorize(&state, &headers)?;
	let _guard = state.operation_lock.lock().await;
	let root = state.root.clone();
	let games = root.join("games");
	tokio::task::spawn_blocking(move || crate::src::extract_games::extract_games(root, games))
		.await
		.map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("再スキャンに失敗しました: {error}")))?;
	Ok(Json(ActionResponse { ok: true, message: "ゲームを再スキャンしました".to_string() }))
}

fn find_game_directory(root: &Path, game_id: &str) -> Result<PathBuf, (StatusCode, Json<ActionResponse>)>
{
	let clean_id = crate::net::normalize_game_id(game_id)
		.map_err(|error| api_error(StatusCode::BAD_REQUEST, error.message().to_string()))?;
	let target = root.join("games").join(clean_id);
	if !target.is_dir()
	{
		return Err(api_error(StatusCode::NOT_FOUND, "ゲームが見つかりません"));
	}
	Ok(target)
}

fn find_distribution_zip(game_directory: &Path, game_id: &str) -> Result<PathBuf, (StatusCode, Json<ActionResponse>)>
{
	let expected = game_directory.join(format!("{game_id}.zip"));
	if expected.is_file()
	{
		return Ok(expected);
	}
	let candidates: Vec<PathBuf> = std::fs::read_dir(game_directory)
		.into_iter().flatten().flatten()
		.map(|entry| entry.path())
		.filter(|path| path.is_file() && path.extension().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case("zip")))
		.collect();
	match candidates.as_slice()
	{
		[only] => Ok(only.clone()),
		[] => Err(api_error(StatusCode::NOT_FOUND, "配布ZIPが見つかりません")),
		_ => Err(api_error(StatusCode::CONFLICT, "配布ZIPを一意に決定できません")),
	}
}

async fn upload_game(
	State(state): State<AdminState>,
	headers: HeaderMap,
	mut multipart: Multipart,
) -> ApiResult<ActionResponse>
{
	authorize(&state, &headers)?;
	let _guard = state.operation_lock.lock().await;
	let temp_directory = state.root.join("temp");
	tokio::fs::create_dir_all(&temp_directory).await
		.map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("tempフォルダーを作成できません: {error}")))?;
	let mut uploaded_path = None;
	while let Some(mut field) = multipart.next_field().await
		.map_err(|error| api_error(StatusCode::BAD_REQUEST, format!("アップロードを読み取れません: {error}")))?
	{
		if field.name() != Some("game") { continue; }
		let file_name = field.file_name().and_then(|name| Path::new(name).file_name()).and_then(|name| name.to_str())
			.ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "ファイル名が不正です"))?.to_string();
		if !file_name.to_ascii_lowercase().ends_with(".zip")
		{
			return Err(api_error(StatusCode::BAD_REQUEST, "ZIPファイルのみアップロードできます"));
		}
		if Path::new(&file_name).components().any(|part| !matches!(part, Component::Normal(_)))
		{
			return Err(api_error(StatusCode::BAD_REQUEST, "ファイル名が不正です"));
		}
		let destination = temp_directory.join(&file_name);
		if destination.exists()
		{
			return Err(api_error(StatusCode::CONFLICT, "同名ファイルを処理中です。少し待ってから再試行してください"));
		}
		let mut file = tokio::fs::File::create(&destination).await
			.map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("アップロード先を作成できません: {error}")))?;
		let mut total_bytes: u64 = 0;
		while let Some(chunk) = field.chunk().await
			.map_err(|error| api_error(StatusCode::BAD_REQUEST, format!("アップロードを読み取れません: {error}")))?
		{
			total_bytes += chunk.len() as u64;
			if total_bytes > 20 * 1024 * 1024 * 1024
			{
				let _ = tokio::fs::remove_file(&destination).await;
				return Err(api_error(StatusCode::PAYLOAD_TOO_LARGE, "ZIPは20GB以内にしてください"));
			}
			file.write_all(&chunk).await
				.map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("アップロードを保存できません: {error}")))?;
		}
		file.flush().await
			.map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("アップロードを保存できません: {error}")))?;
		drop(file);
		uploaded_path = Some(destination);
		break;
	}
	let uploaded_path = uploaded_path.ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "アップロードするZIPを選択してください"))?;
	let validation_path = uploaded_path.clone();
	let valid_zip = tokio::task::spawn_blocking(move || {
		let file = std::fs::File::open(validation_path).map_err(|error| error.to_string())?;
		let archive = zip::ZipArchive::new(file).map_err(|error| error.to_string())?;
		if archive.is_empty() { Err("ZIPが空です".to_string()) } else { Ok(()) }
	}).await.map_err(|error| error.to_string()).and_then(|result| result);
	if let Err(error) = valid_zip
	{
		let _ = tokio::fs::remove_file(&uploaded_path).await;
		return Err(api_error(StatusCode::BAD_REQUEST, format!("有効なZIPではありません: {error}")));
	}
	let root = state.root.clone();
	let games = root.join("games");
	tokio::task::spawn_blocking(move || crate::src::extract_games::extract_games(root, games)).await
		.map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("ゲームの取り込みに失敗しました: {error}")))?;
	if uploaded_path.exists()
	{
		let _ = tokio::fs::remove_file(uploaded_path).await;
		return Err(api_error(StatusCode::INTERNAL_SERVER_ERROR, "ゲームを配布一覧へ取り込めませんでした"));
	}
	Ok(Json(ActionResponse { ok: true, message: "ゲームをアップロードし、配布一覧へ反映しました".to_string() }))
}

async fn update_game(
	State(state): State<AdminState>,
	headers: HeaderMap,
	AxumPath(game_id): AxumPath<String>,
	Json(update): Json<GameUpdate>,
) -> ApiResult<ActionResponse>
{
	authorize(&state, &headers)?;
	let _guard = state.operation_lock.lock().await;
	let clean_id = crate::net::normalize_game_id(&game_id)
		.map_err(|error| api_error(StatusCode::BAD_REQUEST, error.message().to_string()))?;
	let game_directory = find_game_directory(&state.root, &clean_id)?;
	let meta_path = game_directory.join("meta.json");
	let content = tokio::fs::read_to_string(&meta_path).await
		.map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("meta.jsonを読めません: {error}")))?;
	let mut meta: Meta = serde_json::from_str(&content)
		.map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("meta.jsonが不正です: {error}")))?;
	if update.title.trim().is_empty() || update.version.trim().is_empty()
	{
		return Err(api_error(StatusCode::BAD_REQUEST, "タイトルとバージョンは必須です"));
	}
	crate::net::compare_versions(update.version.trim(), update.version.trim())
		.map_err(|error| api_error(StatusCode::BAD_REQUEST, error.message().to_string()))?;
	meta.title = update.title.trim().to_string();
	meta.author = update.author.trim().to_string();
	meta.version = update.version.trim().trim_start_matches(['v', 'V']).to_string();
	meta.latest_update = update.latest_update.trim().to_string();
	meta.description = update.description.trim().to_string();
	meta.tags = update.tags.into_iter().map(|tag| tag.trim().to_string()).filter(|tag| !tag.is_empty()).collect();
	let json = serde_json::to_string_pretty(&meta)
		.map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("meta.jsonを変換できません: {error}")))?;
	let zip_path = find_distribution_zip(&game_directory, &clean_id)?;
	let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
	let temp_zip = game_directory.join(format!(".meta-update-{stamp}.zip"));
	let task_source = zip_path.clone();
	let task_destination = temp_zip.clone();
	let new_meta = json.clone();
	tokio::task::spawn_blocking(move || crate::src::zip_utils::update_zip_with_new_meta(&task_source, &task_destination, &new_meta))
		.await
		.map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("ZIP更新処理に失敗しました: {error}")))?
		.map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("ZIPを更新できません: {error}")))?;
	let backup_zip = game_directory.join(format!(".meta-backup-{stamp}.zip"));
	tokio::fs::rename(&zip_path, &backup_zip).await
		.map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("ZIPを退避できません: {error}")))?;
	if let Err(error) = tokio::fs::rename(&temp_zip, &zip_path).await
	{
		let _ = tokio::fs::rename(&backup_zip, &zip_path).await;
		return Err(api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("ZIPを置き換えられません: {error}")));
	}
	if let Err(error) = tokio::fs::write(&meta_path, &json).await
	{
		let _ = tokio::fs::remove_file(&zip_path).await;
		let _ = tokio::fs::rename(&backup_zip, &zip_path).await;
		return Err(api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("meta.jsonを保存できません: {error}")));
	}
	let _ = tokio::fs::remove_file(backup_zip).await;
	let root = state.root.clone();
	let games = root.join("games");
	let _ = tokio::task::spawn_blocking(move || crate::src::extract_games::extract_games(root, games)).await;
	let receivers = state.updates.send(UpdateNotice { game_id: clean_id.clone(), version: meta.version.clone() }).unwrap_or(0);
	Ok(Json(ActionResponse { ok: true, message: format!("{clean_id}を更新しました（通知先: {receivers}）") }))
}

async fn delete_game(
	State(state): State<AdminState>,
	headers: HeaderMap,
	AxumPath(game_id): AxumPath<String>,
) -> ApiResult<ActionResponse>
{
	authorize(&state, &headers)?;
	let _guard = state.operation_lock.lock().await;
	let clean_id = crate::net::normalize_game_id(&game_id)
		.map_err(|error| api_error(StatusCode::BAD_REQUEST, error.message().to_string()))?;
	let games_root = state.root.join("games").canonicalize()
		.map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("gamesフォルダーを確認できません: {error}")))?;
	let target = find_game_directory(&state.root, &clean_id)?.canonicalize()
		.map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("ゲームフォルダーを確認できません: {error}")))?;
	if target.parent() != Some(games_root.as_path()) || !target.starts_with(&games_root)
	{
		return Err(api_error(StatusCode::BAD_REQUEST, "削除対象がgamesフォルダー外です"));
	}
	tokio::fs::remove_dir_all(&target).await
		.map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("ゲームを削除できません: {error}")))?;
	crate::net::comments::delete_comments_for_game(&clean_id).await
		.map_err(|message| api_error(StatusCode::INTERNAL_SERVER_ERROR, message))?;
	state.leaderboards.remove_game(&clean_id).await
		.map_err(|message| api_error(StatusCode::INTERNAL_SERVER_ERROR, message))?;
	let root = state.root.clone();
	let games = root.join("games");
	let _ = tokio::task::spawn_blocking(move || crate::src::extract_games::extract_games(root, games)).await;
	Ok(Json(ActionResponse { ok: true, message: format!("{clean_id}と関連コメントを削除しました") }))
}

async fn notify(
	State(state): State<AdminState>,
	headers: HeaderMap,
	AxumPath(game_id): AxumPath<String>,
) -> ApiResult<ActionResponse>
{
	authorize(&state, &headers)?;
	let clean_id = crate::net::normalize_game_id(&game_id)
		.map_err(|error| api_error(StatusCode::BAD_REQUEST, error.message().to_string()))?;
	let path = state.root.join("games").join("games.json");
	let content = tokio::fs::read_to_string(path).await
		.map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("ゲーム一覧を読めません: {error}")))?;
	let games: Vec<Meta> = serde_json::from_str(&content)
		.map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("games.jsonが不正です: {error}")))?;
	let game = games.into_iter().find(|game| {
		crate::net::normalize_game_id(if game.id.is_empty() { &game.game } else { &game.id })
			.ok().as_deref() == Some(clean_id.as_str())
	}).ok_or_else(|| api_error(StatusCode::NOT_FOUND, "ゲームが見つかりません"))?;
	let receivers = state.updates.send(UpdateNotice { game_id: clean_id.clone(), version: game.version.clone() }).unwrap_or(0);
	Ok(Json(ActionResponse {
		ok: true,
		message: format!("{clean_id} v{} の更新通知を送信しました（受信: {receivers}）", game.version),
	}))
}

pub async fn serve(
	updates: Arc<broadcast::Sender<UpdateNotice>>,
	leaderboards: crate::leaderboards::LeaderboardStore,
) -> Result<(), String>
{
	let root = executable_root()?;
	let config = load_config(&root)?;
	let address: SocketAddr = config.bind.parse()
		.map_err(|error| format!("admin-config.jsonのbindが不正です: {error}"))?;
	if !address.ip().is_loopback() && config.token.trim().is_empty()
	{
		return Err("LAN公開時はadmin-config.jsonのtoken設定が必須です".to_string());
	}
	let state = AdminState {
		root,
		bind: config.bind.clone(),
		leaderboard_bind: config.leaderboard_bind.clone(),
		token: config.token.trim().to_string(),
		started_at: Instant::now(),
		updates,
		operation_lock: Arc::new(Mutex::new(())),
		leaderboards,
	};
	let app = Router::new()
		.route("/", get(index))
		.route("/app.css", get(css))
		.route("/app.js", get(js))
		.route("/api/status", get(status))
		.route("/api/games", get(games))
		.route("/api/games/upload", post(upload_game).layer(DefaultBodyLimit::disable()))
		.route("/api/games/{game_id}", patch(update_game).delete(delete_game))
		.route("/api/leaderboards/{game_id}", get(get_leaderboards).put(update_leaderboards))
		.route("/api/comments", get(comments))
		.route("/api/comments/{comment_id}", delete(remove_comment))
		.route("/api/rescan", post(rescan))
		.route("/api/notify/{game_id}", post(notify))
		.with_state(state);
	let listener = tokio::net::TcpListener::bind(address).await
		.map_err(|error| format!("管理画面を{}で起動できません: {error}", config.bind))?;
	println!("Admin UI listening on http://{}", config.bind);
	axum::serve(listener, app).await.map_err(|error| error.to_string())
}
