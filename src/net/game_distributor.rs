use tonic::{
	Response, Request, Status
};
use tokio::io::AsyncReadExt;
use tonic::codegen::tokio_stream::wrappers::ReceiverStream;
use crate::gamelauncher::{
	DownloadRequest, GameData
};
use std::env;
use std::fs;
use std::path::PathBuf;
use serde_json;
use crate::init::Meta;
use std::ffi::OsStr;

pub type DownloadStream = ReceiverStream<Result<GameData, Status>>;

pub async fn handle_game_distributor(request: Request<DownloadRequest>) -> Result<Response<DownloadStream>, Status>
{
	let exe_path 	= env::current_exe().expect("Couldnt get exe path");
	let root 		= exe_path.parent().expect("Couldnt get root path");
	let game_path	= root.join("games");

	let (tx, rx)	= tokio::sync::mpsc::channel(32);

	let req 		= request.into_inner();
	println!("game id: {}", req.game_id);

	let clean_id = req.game_id.trim_end_matches(".exe");
	let mut target_path = game_path.join(clean_id);
	if !target_path.exists() || !target_path.is_dir()
	{
		let raw_path = game_path.join(&req.game_id);
		if raw_path.exists() && raw_path.is_dir()
		{
			target_path = raw_path;
		}
		else if let Ok(entries) = fs::read_dir(&game_path)
		{
			for entry in entries.flatten()
			{
				let p = entry.path();
				if p.is_dir()
				{
					if entry.file_name() == OsStr::new(clean_id) || entry.file_name() == OsStr::new(&req.game_id)
					{
						target_path = p;
						break;
					}
					let meta_file = p.join("meta.json");
					if let Ok(c) = fs::read_to_string(&meta_file)
					{
						if let Ok(m) = serde_json::from_str::<Meta>(&c)
						{
							if m.id == req.game_id || m.id == clean_id || m.game == req.game_id || m.game == clean_id
							{
								target_path = p;
								break;
							}
						}
					}
				}
			}
		}
	}
	
	let _ = search_game(target_path.clone()).await;

	let mut zip_path: Option<PathBuf> = None;
	if let Ok(entries) = fs::read_dir(&target_path)
	{
		for entry in entries.flatten()
		{
			let path = entry.path();
			if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("zip")
			{
				zip_path = Some(path);
				break;
			}
		}
	}

	let zip_path = match zip_path
	{
		Some(p) => p,
		None 	=> return Err(Status::not_found("zipファイルが見つかりません")),
	};

	tokio::spawn(async move {
		let mut file = match tokio::fs::File::open(zip_path).await
		{
			Ok(f) => f,
			Err(_) => return,
		};
		let mut buffer = vec![0u8; 1024 * 1024 * 2];
		let mut index = 0;

		loop
		{
			let bytes_read = match file.read(&mut buffer).await
			{
				Ok(n) => n,
				Err(_) => break,
			};
			if bytes_read == 0 { break; }
	
			let chunk = GameData
			{
				data: buffer[..bytes_read].to_vec(),
				index,
			};
	
			if tx.send(Ok(chunk)).await.is_err() { break; }
			index += 1;
		}
	});

	Ok(Response::new(ReceiverStream::new(rx)))
}
	
async fn search_game(path: PathBuf) -> Result<(), Status>
{
	if path.exists() && path.is_dir() { Ok(()) }
	else
	{
		println!("ゲームが存在しません"); 
		Err(Status::not_found("ゲームが存在しません"))
	}
}
