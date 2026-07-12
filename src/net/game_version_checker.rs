use std::env;
use std::path::PathBuf;
use tonic::{
	Response, Request, Status
};
use crate::gamelauncher::{
	VersionRequest, VersionResponse
};
use serde_json;
use crate::init::Meta;

pub async fn handle_check_version(request: Request<VersionRequest>) -> Result<Response<VersionResponse>, Status>
{
	let exe_path	= env::current_exe().expect("Couldnt get exe path");
	let root		= exe_path.parent().expect("Couldnt get root path");
	let game_path	= root.join("games");

	let req = request.into_inner();

	let target_game = req.game_id;
	let current_version = req.current_version;
	let target_path = game_path.join(target_game);
	println!("game id : {:?}", target_path);

	let latest_version = search_game(target_path).await?;

	let parse_ver = |ver: &str| -> (i32, i32, i32)
	{
		let mut parts = ver.split('.').map(|s| s.parse::<i32>().unwrap_or(0));
		(
			parts.next().unwrap_or(0),
			parts.next().unwrap_or(0),
			parts.next().unwrap_or(0),
		)
	};

	let cv = parse_ver(&current_version);
	let lv = parse_ver(&latest_version);
	let need_update: bool = cv < lv;

	let res = VersionResponse
	{
		latest_version: latest_version,
		is_update_available: need_update
	};

	Ok(Response::new(res))
}

async fn search_game(gamepath: PathBuf) -> Result<String, Status>
{
	if gamepath.exists()
	{
		if gamepath.is_dir()
		{
			let version = check_version(gamepath).await?;
			Ok(version)
		}
		else
		{
			println!("Arent dir");
			Err(Status::not_found(format!("ゲームがディレクトリではありません")))
		}
	}
	else
	{
		println!("Couldnt find");
		Err(Status::not_found(format!("ゲームが存在しません")))
	}
}

async fn check_version(gamepath: PathBuf) -> Result<String, Status>
{
	let meta_file = gamepath.join("meta.json");

	if meta_file.exists()
	{
		if meta_file.is_file()
		{
			let json_str = match tokio::fs::read_to_string(&meta_file).await
			{
				Ok(s) => s,
				Err(_) => return Err(Status::not_found(format!("meta.jsonが存在しません"))),
			};

			let meta = match serde_json::from_str::<Meta>(&json_str)
			{
				Ok(d) => d,
				Err(e) => return Err(Status::internal(format!("jsonのパースができませんでした: {}", e))),
			};

			Ok(meta.version)
		}
		else
		{
			println!("Couldnt find meta file");
			Err(Status::not_found(format!("metaファイルがありません")))
		}
	}
	else
	{
		println!("Couldnt find meta file");
		Err(Status::not_found(format!("metaファイルがありません")))
	}
}
