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
use std::path::{
	Path, PathBuf
};
use serde_json;
use crate::init::Meta;
use std::ffi::OsStr;

pub type DownloadStream = ReceiverStream<Result<GameData, Status>>;

pub async fn handle_game_distributor(request: Request<DownloadRequest>) -> Result<Response<DownloadStream>, Status>
{
	let exe_path 	= env::current_exe().expect("Couldnt get exe path");
	let root 		= exe_path.parent().expect("Couldnt get root path");
	let game_path	= root.join("games");

	let (tx, rx)	= tokio::sync::mpsc::channel(128);

	let req 		= request.into_inner();

	println!("game id: {}", req.game_id);
	let target_path = game_path.join(req.game_id);
	
	let _ = search_game(target_path.clone()).await;

	let mut zip_path: Option<PathBuf> = None;
	for entry in fs::read_dir(target_path)?
	{
		let entry 	= entry?;
		let path 	= entry.path();

		if (path.is_file() && path.file_name() != Some(OsStr::new("meta.json"))) 
		{
			zip_path = Some(path);
			break;
		}
	}

	let zip_path = match zip_path
	{
		Some(p) => p,
		None 	=> return Err(Status::not_found("zipファイルが見つかりません")),
	};

	tokio::spawn(async move {
		let mut file 	= match tokio::fs::File::open(zip_path).await
		{
			Ok(f) => f,
			Err(_) => return,
		};
		let mut buffer	= vec![0u8; 1024 * 128];
		let mut index 	= 0;

		while (true)
		{
			let bytes_read = file.read(&mut buffer).await.expect("Couldnt read zip file");
			if (bytes_read == 0) { break; };
	
			let chunk = GameData
			{
				data: buffer[..bytes_read].to_vec(),
				index,
			};
	
			if tx.send(Ok(chunk)).await.is_err() { break; };
			index += 1;
		}
	});

	Ok(Response::new(ReceiverStream::new(rx)))
}
	
async fn search_game(path: PathBuf) -> Result<(), Status>
{
	if path.exists() && path.is_dir() 	{ Ok(()) }
	else
	{
		println!("ゲームが存在しません"); 
		Err(Status::not_found("ゲームが存在しません"))
	}
}
