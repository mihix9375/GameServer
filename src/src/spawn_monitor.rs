use std::env;
use std::fs;
use std::sync::Arc;
use tokio::sync::broadcast;
use crate::gamelauncher::UpdateNotice;
use serde_json;
use crate::src::extract_games;
use crate::init::Meta;

pub fn spawn_monitor() -> Arc<broadcast::Sender<UpdateNotice>>
{	
	let exe_path	= env::current_exe().expect("Couldnt get exe path");
	let root		= exe_path.parent().expect("Couldnt get root path");
	let _temp_path 	= root.join("temp");
	let game_path	= root.join("games");
	let games_list_path 	= game_path.join("games.json"); 

	let (tx, _rx) 	= broadcast::channel::<UpdateNotice>(128);
	let shared_tx 	= Arc::new(tx);

	let mut meta_cache: Vec<Meta> = Vec::new();
	let monitor_tx = Arc::clone(&shared_tx);
	let monitor_root = root.to_path_buf();
	let monitor_games = game_path.clone();

	tokio::spawn(async move {
		loop
		{
			extract_games::extract_games(monitor_root.clone(), monitor_games.clone());

			if let Ok(content) = fs::read_to_string(&games_list_path)
			{
				if let Ok(meta) = serde_json::from_str::<Vec<Meta>>(&content)
				{
					if meta != meta_cache
					{
						for game_meta in &meta
						{
							let clean_id = if !game_meta.id.is_empty() { game_meta.id.trim_end_matches(".exe").to_string() } else { game_meta.game.trim_end_matches(".exe").to_string() };
							let _ = monitor_tx.send(UpdateNotice {
								game_id: clean_id,
								version: game_meta.version.clone(),
							});
						}
						meta_cache = meta;
					}
				}
			}

			tokio::time::sleep(tokio::time::Duration::from_millis(10000)).await;
		}
	});

	shared_tx
}
