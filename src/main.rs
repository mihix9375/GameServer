use tonic::{transport::Server, Request, Response, Status};

pub mod gamelauncher
{
	tonic::include_proto!("gamelauncher");
}

use gamelauncher::game_service_server::{GameService, GameServiceServer};
use gamelauncher::{
	DownloadRequest, GameData, Identificial, UpdateNotice, VersionRequest, VersionResponse,
};

mod net;
mod init;

#[derive(Default)]
pub struct GameLauncherServer {}

#[tonic::async_trait]
impl GameService for GameLauncherServer
{
	async fn check_version(
		&self,
		request: Request<VersionRequest>,
	) -> Result<Response<VersionResponse>, Status> {
		net::game_version_checker::handle_check_version(request).await
    }

	type DownloadGameStream = net::game_distributor::DownloadStream;
	async fn download_game(
		&self,
		request: Request<DownloadRequest>,
	) -> Result<Response<Self::DownloadGameStream>, Status> {
		net::game_distributor::handle_game_distributor(request).await
	}

	type WaitUpdateStream = tonic::codegen::tokio_stream::wrappers::ReceiverStream<Result<UpdateNotice, Status>>;
	async fn wait_update(
		&self,
		request: Request<Identificial>,
	) -> Result<Response<Self::WaitUpdateStream>, Status> {
		todo!()
	}
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> 
{
    init::create_dirs();

    let addr 	= "[::1]:50050".parse()?;
	let server 	= GameLauncherServer::default();

	println!("Server Start. Listening On {}", addr);

	Server::builder()
		.add_service(GameServiceServer::new(server))
		.serve(addr)
		.await?;

	Ok(())
}
