use std::path::PathBuf;
use std::sync::{atomic::{AtomicU64, Ordering}, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};

use crate::gamelauncher::{
	AddCommentRequest, Comment, CommentListRequest, CommentListResponse,
};

const MAX_AUTHOR_CHARS: usize = 40;
const MAX_CONTENT_CHARS: usize = 1000;
const MAX_RETURNED_COMMENTS: usize = 200;

static FILE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static COMMENT_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredComment
{
	id: String,
	game_id: String,
	author: String,
	content: String,
	created_at: i64,
}

impl From<StoredComment> for Comment
{
	fn from(value: StoredComment) -> Self
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

fn storage_path() -> Result<PathBuf, Status>
{
	let executable = std::env::current_exe()
		.map_err(|e| Status::internal(format!("保存先を取得できません: {e}")))?;
	let directory = executable.parent()
		.ok_or_else(|| Status::internal("保存先ディレクトリを取得できません"))?;
	Ok(directory.join("comments.jsonl"))
}

pub async fn list_all_comments() -> Result<Vec<Comment>, String>
{
	let _guard = FILE_LOCK.get_or_init(|| Mutex::new(())).lock().await;
	let path = storage_path().map_err(|error| error.message().to_string())?;
	let content = match tokio::fs::read_to_string(path).await
	{
		Ok(content) => content,
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
		Err(error) => return Err(format!("コメントを読み込めません: {error}")),
	};
	let mut comments: Vec<StoredComment> = content.lines()
		.filter_map(|line| serde_json::from_str::<StoredComment>(line).ok())
		.collect();
	comments.sort_by(|left, right| right.created_at.cmp(&left.created_at));
	Ok(comments.into_iter().map(Comment::from).collect())
}

pub async fn delete_comment(comment_id: &str) -> Result<bool, String>
{
	let comment_id = comment_id.trim();
	if comment_id.is_empty()
	{
		return Ok(false);
	}
	let _guard = FILE_LOCK.get_or_init(|| Mutex::new(())).lock().await;
	let path = storage_path().map_err(|error| error.message().to_string())?;
	let content = match tokio::fs::read_to_string(&path).await
	{
		Ok(content) => content,
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
		Err(error) => return Err(format!("コメントを読み込めません: {error}")),
	};
	let mut removed = false;
	let mut retained = Vec::new();
	for line in content.lines()
	{
		match serde_json::from_str::<StoredComment>(line)
		{
			Ok(comment) if comment.id == comment_id => removed = true,
			_ => retained.push(line),
		}
	}
	if removed
	{
		let output = if retained.is_empty() { String::new() } else { format!("{}\n", retained.join("\n")) };
		tokio::fs::write(path, output).await
			.map_err(|error| format!("コメントを更新できません: {error}"))?;
	}
	Ok(removed)
}

pub async fn delete_comments_for_game(game_id: &str) -> Result<usize, String>
{
	let game_id = crate::net::normalize_game_id(game_id).map_err(|error| error.message().to_string())?;
	let _guard = FILE_LOCK.get_or_init(|| Mutex::new(())).lock().await;
	let path = storage_path().map_err(|error| error.message().to_string())?;
	let content = match tokio::fs::read_to_string(&path).await
	{
		Ok(content) => content,
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
		Err(error) => return Err(format!("コメントを読み込めません: {error}")),
	};
	let mut removed = 0;
	let mut retained = Vec::new();
	for line in content.lines()
	{
		match serde_json::from_str::<StoredComment>(line)
		{
			Ok(comment) if comment.game_id == game_id => removed += 1,
			_ => retained.push(line),
		}
	}
	if removed > 0
	{
		let output = if retained.is_empty() { String::new() } else { format!("{}\n", retained.join("\n")) };
		tokio::fs::write(path, output).await
			.map_err(|error| format!("コメントを更新できません: {error}"))?;
	}
	Ok(removed)
}

fn validate_text(value: &str, field: &str, max_chars: usize, allow_empty: bool) -> Result<String, Status>
{
	let value = value.trim();
	if !allow_empty && value.is_empty()
	{
		return Err(Status::invalid_argument(format!("{field}を入力してください")));
	}
	if value.chars().count() > max_chars
	{
		return Err(Status::invalid_argument(format!("{field}は{max_chars}文字以内で入力してください")));
	}
	Ok(value.to_string())
}

pub async fn handle_list_comments(
	request: Request<CommentListRequest>,
) -> Result<Response<CommentListResponse>, Status>
{
	let game_id = crate::net::normalize_game_id(&request.into_inner().game_id)?;
	let _guard = FILE_LOCK.get_or_init(|| Mutex::new(())).lock().await;
	let path = storage_path()?;
	let content = match tokio::fs::read_to_string(path).await
	{
		Ok(content) => content,
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
		Err(error) => return Err(Status::internal(format!("コメントを読み込めません: {error}"))),
	};

	let mut comments: Vec<StoredComment> = content.lines()
		.filter_map(|line| serde_json::from_str::<StoredComment>(line).ok())
		.filter(|comment| comment.game_id == game_id)
		.collect();
	comments.sort_by_key(|comment| comment.created_at);
	if comments.len() > MAX_RETURNED_COMMENTS
	{
		comments.drain(..comments.len() - MAX_RETURNED_COMMENTS);
	}

	Ok(Response::new(CommentListResponse {
		comments: comments.into_iter().map(Comment::from).collect(),
	}))
}

pub async fn handle_add_comment(
	request: Request<AddCommentRequest>,
) -> Result<Response<Comment>, Status>
{
	let request = request.into_inner();
	let game_id = crate::net::normalize_game_id(&request.game_id)?;
	let author = validate_text(&request.author, "名前", MAX_AUTHOR_CHARS, true)?;
	let content = validate_text(&request.content, "コメント", MAX_CONTENT_CHARS, false)?;
	let now = SystemTime::now().duration_since(UNIX_EPOCH)
		.map_err(|_| Status::internal("システム時刻が不正です"))?;
	let sequence = COMMENT_COUNTER.fetch_add(1, Ordering::Relaxed);
	let comment = StoredComment {
		id: format!("{}-{}-{sequence}", now.as_nanos(), std::process::id()),
		game_id,
		author: if author.is_empty() { "匿名".to_string() } else { author },
		content,
		created_at: now.as_secs() as i64,
	};

	let _guard = FILE_LOCK.get_or_init(|| Mutex::new(())).lock().await;
	let path = storage_path()?;
	let mut file = tokio::fs::OpenOptions::new()
		.create(true)
		.append(true)
		.open(path)
		.await
		.map_err(|e| Status::internal(format!("コメント保存ファイルを開けません: {e}")))?;
	let mut line = serde_json::to_vec(&comment)
		.map_err(|e| Status::internal(format!("コメントを変換できません: {e}")))?;
	line.push(b'\n');
	file.write_all(&line).await
		.map_err(|e| Status::internal(format!("コメントを保存できません: {e}")))?;
	file.flush().await
		.map_err(|e| Status::internal(format!("コメントを保存できません: {e}")))?;

	Ok(Response::new(Comment::from(comment)))
}

#[cfg(test)]
mod tests
{
	use super::*;

	#[test]
	fn validates_comment_fields()
	{
		assert!(validate_text("", "コメント", MAX_CONTENT_CHARS, false).is_err());
		assert_eq!(validate_text("  hello  ", "コメント", MAX_CONTENT_CHARS, false).unwrap(), "hello");
		assert!(validate_text(&"a".repeat(MAX_AUTHOR_CHARS + 1), "名前", MAX_AUTHOR_CHARS, true).is_err());
	}
}
