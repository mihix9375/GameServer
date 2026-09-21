use tonic::{transport::Server, Request, Response, Status};

pub mod gamelauncher
{
	tonic::include_proto!("gamelauncher");
}

use gamelauncher::game_service_server::{GameService, GameServiceServer};
use gamelauncher::{
	AddCommentRequest, Comment, CommentListRequest, CommentListResponse, DownloadRequest,
	Identificial, UpdateNotice, VersionRequest, VersionResponse,
};

mod net;
mod init;
mod src;
mod admin;

use crate::src::spawn_monitor;

#[derive(Clone)]
pub struct GameLauncherServer {
	pub shared_tx: std::sync::Arc<tokio::sync::broadcast::Sender<UpdateNotice>>,
}

#[tonic::async_trait]
impl GameService for GameLauncherServer
{
	async fn check_version(
		&self,
		request: Request<VersionRequest>,
	) -> Result<Response<VersionResponse>, Status>
	{
		net::game_version_checker::handle_check_version(request).await
	}

	type DownloadGameStream = net::game_distributor::DownloadStream;
	async fn download_game(
		&self,
		request: Request<DownloadRequest>,
	) -> Result<Response<Self::DownloadGameStream>, Status>
	{
		net::game_distributor::handle_game_distributor(request).await
	}

	type WaitUpdateStream = tonic::codegen::tokio_stream::wrappers::ReceiverStream<Result<UpdateNotice, Status>>;
	async fn wait_update(
		&self,
		request: Request<Identificial>,
	) -> Result<Response<Self::WaitUpdateStream>, Status>
	{
		net::update_notice::send_update_notice(request, &self.shared_tx).await
	}

	async fn list_comments(
		&self,
		request: Request<CommentListRequest>,
	) -> Result<Response<CommentListResponse>, Status>
	{
		net::comments::handle_list_comments(request).await
	}

	async fn add_comment(
		&self,
		request: Request<AddCommentRequest>,
	) -> Result<Response<Comment>, Status>
	{
		net::comments::handle_add_comment(request).await
	}
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> 
{
	init::init();

	let shared_tx = spawn_monitor::spawn_monitor();
	let server 	= GameLauncherServer { shared_tx };
	let admin_tx = server.shared_tx.clone();
	tokio::spawn(async move {
		if let Err(error) = admin::serve(admin_tx).await
		{
			eprintln!("Admin UI error: {error}");
		}
	});

	let addr_v4 = "0.0.0.0:50050".parse()?;
	let addr_v6 = "[::]:50050".parse()?;

	println!("GameLauncher Server listening on {} and {}", addr_v4, addr_v6);

	let server_v4 = server.clone();
	let handle_v4 = tokio::spawn(async move {
		if let Err(e) = Server::builder()
			.initial_stream_window_size(Some(1024 * 1024 * 16))
			.initial_connection_window_size(Some(1024 * 1024 * 32))
			.tcp_nodelay(true)
			.add_service(GameServiceServer::new(server_v4))
			.serve(addr_v4)
			.await
		{
			eprintln!("IPv4 server error: {}", e);
		}
	});

	let server_v6 = server.clone();
	let handle_v6 = tokio::spawn(async move {
		if let Err(e) = Server::builder()
			.initial_stream_window_size(Some(1024 * 1024 * 16))
			.initial_connection_window_size(Some(1024 * 1024 * 32))
			.tcp_nodelay(true)
			.add_service(GameServiceServer::new(server_v6))
			.serve(addr_v6)
			.await
		{
			eprintln!("IPv6 server error: {}", e);
		}
	});

	let _ = tokio::join!(handle_v4, handle_v6);
	Ok(())
}
