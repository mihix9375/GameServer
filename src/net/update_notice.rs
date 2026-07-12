use crate::gamelauncher::{
	Identificial, UpdateNotice
};
use tonic::{
	Response, Request, Status
};
use tonic::codegen::tokio_stream::wrappers::ReceiverStream;

use std::sync::Arc;
use tokio::sync::broadcast;

pub type UpdateNoticeStream = ReceiverStream<Result<UpdateNotice, Status>>;

pub async fn send_update_notice(
	_request: Request<Identificial>,
	tx: &Arc<broadcast::Sender<UpdateNotice>>,
) -> Result<Response<UpdateNoticeStream>, Status>
{
	let mut rx = tx.subscribe();
	let (tx_stream, rx_stream) = tokio::sync::mpsc::channel(32);

	tokio::spawn(async move {
		if let Ok(exe_path) = std::env::current_exe() {
			if let Some(root) = exe_path.parent() {
				let games_json = root.join("games").join("games.json");
				if let Ok(content) = std::fs::read_to_string(&games_json) {
					if let Ok(metas) = serde_json::from_str::<Vec<crate::init::Meta>>(&content) {
						for meta in metas {
							let clean_id = if !meta.id.is_empty() { meta.id.trim_end_matches(".exe").to_string() } else { meta.game.trim_end_matches(".exe").to_string() };
							let notice = UpdateNotice {
								game_id: clean_id,
								version: meta.version.clone(),
							};
							if tx_stream.send(Ok(notice)).await.is_err() {
								return;
							}
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
