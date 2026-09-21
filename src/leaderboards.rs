use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

mod api;

pub use api::serve;

const MAX_BOARDS: usize = 2;
const MAX_BOARD_ID_LENGTH: usize = 32;
const MAX_BOARD_NAME_LENGTH: usize = 40;
const MAX_PLAYER_NAME_LENGTH: usize = 24;
const MAX_STORED_ENTRIES: usize = 100;
const PUBLIC_ENTRY_LIMIT: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RankingOrder
{
	HighScore,
	LowScore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardDefinition
{
	pub id: String,
	pub name: String,
	pub order: RankingOrder,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreEntry
{
	pub player_name: String,
	pub score: i64,
	pub submitted_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Leaderboard
{
	#[serde(flatten)]
	pub definition: BoardDefinition,
	#[serde(default)]
	pub entries: Vec<ScoreEntry>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct LeaderboardData
{
	#[serde(default)]
	games: HashMap<String, Vec<Leaderboard>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RankedEntry
{
	pub rank: usize,
	pub player_name: String,
	pub score: i64,
	pub submitted_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicBoard
{
	pub id: String,
	pub name: String,
	pub order: RankingOrder,
	pub entries: Vec<RankedEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GameLeaderboards
{
	pub game_id: String,
	pub leaderboards: Vec<PublicBoard>,
}

#[derive(Clone)]
pub struct LeaderboardStore
{
	path: PathBuf,
	data: Arc<Mutex<LeaderboardData>>,
}

impl LeaderboardStore
{
	pub async fn load(root: &Path) -> Result<Self, String>
	{
		let path = root.join("leaderboards.json");
		let data = match tokio::fs::read_to_string(&path).await
		{
			Ok(content) => serde_json::from_str(&content)
				.map_err(|error| format!("leaderboards.jsonが不正です: {error}"))?,
			Err(error) if error.kind() == std::io::ErrorKind::NotFound => LeaderboardData::default(),
			Err(error) => return Err(format!("leaderboards.jsonを読めません: {error}")),
		};
		Ok(Self {
			path,
			data: Arc::new(Mutex::new(data)),
		})
	}

	pub async fn get(&self, game_id: &str) -> GameLeaderboards
	{
		let data = self.data.lock().await;
		let leaderboards = data.games
			.get(game_id)
			.into_iter()
			.flatten()
			.map(public_board)
			.collect();
		GameLeaderboards {
			game_id: game_id.to_string(),
			leaderboards,
		}
	}

	pub async fn definitions(&self, game_id: &str) -> Vec<BoardDefinition>
	{
		let data = self.data.lock().await;
		data.games
			.get(game_id)
			.into_iter()
			.flatten()
			.map(|board| board.definition.clone())
			.collect()
	}

	pub async fn configure(
		&self,
		game_id: &str,
		definitions: Vec<BoardDefinition>,
	) -> Result<(), String>
	{
		validate_game_id(game_id)?;
		if definitions.len() > MAX_BOARDS
		{
			return Err("ランキングは1ゲームにつき最大2つです".to_string());
		}
		validate_definitions(&definitions)?;

		let mut data = self.data.lock().await;
		let mut previous_entries = data.games
			.remove(game_id)
			.unwrap_or_default()
			.into_iter()
			.map(|board| (board.definition.id, board.entries))
			.collect::<HashMap<_, _>>();
		let boards = definitions
			.into_iter()
			.map(|definition| build_board(definition, &mut previous_entries))
			.collect::<Vec<_>>();
		if !boards.is_empty()
		{
			data.games.insert(game_id.to_string(), boards);
		}
		self.persist(&data).await
	}

	pub async fn submit(
		&self,
		game_id: &str,
		board_id: &str,
		player_name: &str,
		score: i64,
	) -> Result<usize, String>
	{
		validate_game_id(game_id)?;
		validate_board_id(board_id)?;
		let player_name = player_name.trim();
		if player_name.is_empty() || player_name.chars().count() > MAX_PLAYER_NAME_LENGTH
		{
			return Err("プレイヤー名は1〜24文字で指定してください".to_string());
		}
		if player_name.chars().any(char::is_control)
		{
			return Err("プレイヤー名に制御文字は使えません".to_string());
		}

		let mut data = self.data.lock().await;
		let board = data.games
			.get_mut(game_id)
			.and_then(|boards| boards.iter_mut().find(|board| board.definition.id == board_id))
			.ok_or_else(|| "ランキングが見つかりません".to_string())?;
		let rank = insertion_rank(board, score);
		board.entries.push(ScoreEntry {
			player_name: player_name.to_string(),
			score,
			submitted_at: unix_time(),
		});
		sort_entries(board);
		board.entries.truncate(MAX_STORED_ENTRIES);
		self.persist(&data).await?;
		Ok(rank)
	}

	pub async fn remove_game(&self, game_id: &str) -> Result<(), String>
	{
		let mut data = self.data.lock().await;
		if data.games.remove(game_id).is_some()
		{
			self.persist(&data).await?;
		}
		Ok(())
	}

	async fn persist(&self, data: &LeaderboardData) -> Result<(), String>
	{
		let json = serde_json::to_string_pretty(data)
			.map_err(|error| format!("ランキングを保存できません: {error}"))?;
		tokio::fs::write(&self.path, json).await
			.map_err(|error| format!("ランキングを保存できません: {error}"))
	}
}

fn build_board(
	mut definition: BoardDefinition,
	previous_entries: &mut HashMap<String, Vec<ScoreEntry>>,
) -> Leaderboard
{
	definition.id = definition.id.trim().to_string();
	definition.name = definition.name.trim().to_string();
	let entries = previous_entries.remove(&definition.id).unwrap_or_default();
	let mut board = Leaderboard { definition, entries };
	sort_entries(&mut board);
	board
}

fn public_board(board: &Leaderboard) -> PublicBoard
{
	PublicBoard {
		id: board.definition.id.clone(),
		name: board.definition.name.clone(),
		order: board.definition.order.clone(),
		entries: board.entries
			.iter()
			.take(PUBLIC_ENTRY_LIMIT)
			.enumerate()
			.map(|(index, entry)| RankedEntry {
				rank: index + 1,
				player_name: entry.player_name.clone(),
				score: entry.score,
				submitted_at: entry.submitted_at,
			})
			.collect(),
	}
}

fn sort_entries(board: &mut Leaderboard)
{
	match board.definition.order
	{
		RankingOrder::HighScore => board.entries.sort_by(|a, b| {
			b.score.cmp(&a.score).then(a.submitted_at.cmp(&b.submitted_at))
		}),
		RankingOrder::LowScore => board.entries.sort_by(|a, b| {
			a.score.cmp(&b.score).then(a.submitted_at.cmp(&b.submitted_at))
		}),
	}
}

fn insertion_rank(board: &Leaderboard, score: i64) -> usize
{
	let higher_ranked_entries = board.entries.iter().filter(|entry| match board.definition.order {
		RankingOrder::HighScore => entry.score >= score,
		RankingOrder::LowScore => entry.score <= score,
	});
	1 + higher_ranked_entries.count()
}

fn validate_definitions(definitions: &[BoardDefinition]) -> Result<(), String>
{
	let mut ids = HashSet::new();
	for definition in definitions
	{
		let id = definition.id.trim();
		validate_board_id(id)?;

		let name = definition.name.trim();
		if name.is_empty() || name.chars().count() > MAX_BOARD_NAME_LENGTH
		{
			return Err("ランキング名は1〜40文字で指定してください".to_string());
		}
		if !ids.insert(id)
		{
			return Err("ランキングIDが重複しています".to_string());
		}
	}
	Ok(())
}

fn validate_game_id(value: &str) -> Result<(), String>
{
	let is_invalid = value.is_empty()
		|| value.len() > 100
		|| value.contains(['/', '\\'])
		|| value == "."
		|| value == "..";
	if is_invalid
	{
		Err("ゲームIDが不正です".to_string())
	}
	else { Ok(()) }
}

fn validate_board_id(value: &str) -> Result<(), String>
{
	let value = value.trim();
	let has_valid_characters = value.bytes()
		.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
	if value.is_empty() || value.len() > MAX_BOARD_ID_LENGTH || !has_valid_characters
	{
		Err("ランキングIDは32文字以内の半角英数字・_・-で指定してください".to_string())
	}
	else { Ok(()) }
}

fn unix_time() -> i64
{
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap_or_default()
		.as_secs() as i64
}

#[cfg(test)]
mod tests
{
	use super::*;

	fn test_root(name: &str) -> PathBuf
	{
		std::env::temp_dir().join(format!("gameserver-leaderboard-{name}-{}", std::process::id()))
	}

	async fn store(name: &str) -> LeaderboardStore
	{
		let root = test_root(name);
		let _ = tokio::fs::remove_dir_all(&root).await;
		tokio::fs::create_dir_all(&root).await.unwrap();
		LeaderboardStore::load(&root).await.unwrap()
	}

	#[tokio::test]
	async fn high_and_low_rankings_sort_in_opposite_directions()
	{
		let store = store("sort").await;
		store.configure("game", vec![
			BoardDefinition { id: "score".into(), name: "Score".into(), order: RankingOrder::HighScore },
			BoardDefinition { id: "time".into(), name: "Time".into(), order: RankingOrder::LowScore },
		]).await.unwrap();
		assert_eq!(store.submit("game", "score", "A", 10).await.unwrap(), 1);
		assert_eq!(store.submit("game", "score", "B", 20).await.unwrap(), 1);
		assert_eq!(store.submit("game", "time", "A", 1000).await.unwrap(), 1);
		assert_eq!(store.submit("game", "time", "B", 900).await.unwrap(), 1);
		let boards = store.get("game").await.leaderboards;
		assert_eq!(boards[0].entries.iter().map(|entry| entry.score).collect::<Vec<_>>(), [20, 10]);
		assert_eq!(boards[1].entries.iter().map(|entry| entry.score).collect::<Vec<_>>(), [900, 1000]);
		let _ = tokio::fs::remove_dir_all(test_root("sort")).await;
	}

	#[tokio::test]
	async fn rejects_more_than_two_boards()
	{
		let store = store("limit").await;
		let boards = (0..3).map(|index| BoardDefinition {
			id: format!("board{index}"), name: format!("Board {index}"), order: RankingOrder::HighScore,
		}).collect();
		assert!(store.configure("game", boards).await.is_err());
		let _ = tokio::fs::remove_dir_all(test_root("limit")).await;
	}
}
