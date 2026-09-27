use std::path::{Path, PathBuf};

use tonic::{transport::Server, Request, Response, Status};

pub mod gamelauncher
{
	tonic::include_proto!("gamelauncher");
}

use gamelauncher::game_service_server::{GameService, GameServiceServer};
use gamelauncher::{
	AddCommentRequest, Comment, CommentListRequest, CommentListResponse, DownloadRequest,
	GameFilesRequest, GameManifest, Identificial, UpdateNotice, VersionRequest, VersionResponse,
};

mod net;
mod init;
mod src;
mod admin;
mod leaderboards;
mod client_metrics;
mod logging;

use crate::src::spawn_monitor;

#[derive(Clone)]
pub struct GameLauncherServer {
	pub shared_tx: std::sync::Arc<tokio::sync::broadcast::Sender<UpdateNotice>>,
}

fn track_request<T: prost::Message>(rpc: &'static str, request: &Request<T>) -> Option<String>
{
	let ip = client_metrics::request_ip(request);
	let request_bytes = request.get_ref().encoded_len() as u64;
	client_metrics::record(ip.as_deref(), 0, request_bytes);
	tracing::debug!(
		target: logging::DETAIL_TARGET,
		event = "grpc_request",
		rpc,
		client_ip = ip.as_deref().unwrap_or("unknown"),
		request_bytes,
	);
	ip
}

fn track_response<T: prost::Message>(rpc: &'static str, ip: Option<&str>, response: &Response<T>)
{
	let response_bytes = response.get_ref().encoded_len() as u64;
	client_metrics::record(ip, response_bytes, 0);
	tracing::debug!(
		target: logging::DETAIL_TARGET,
		event = "grpc_response",
		rpc,
		client_ip = ip.unwrap_or("unknown"),
		response_bytes,
	);
}

fn track_stream_response(rpc: &'static str, ip: Option<&str>)
{
	tracing::debug!(
		target: logging::DETAIL_TARGET,
		event = "grpc_stream_opened",
		rpc,
		client_ip = ip.unwrap_or("unknown"),
	);
}

fn track_failure(rpc: &'static str, ip: Option<&str>, status: &Status)
{
	tracing::debug!(
		target: logging::DETAIL_TARGET,
		event = "grpc_error",
		rpc,
		client_ip = ip.unwrap_or("unknown"),
		status_code = ?status.code(),
		message = %status.message(),
	);
}

#[tonic::async_trait]
impl GameService for GameLauncherServer
{
	async fn check_version(
		&self,
		request: Request<VersionRequest>,
	) -> Result<Response<VersionResponse>, Status>
	{
		let ip = track_request("check_version", &request);
		match net::game_version_checker::handle_check_version(request).await
		{
			Ok(response) => {
				track_response("check_version", ip.as_deref(), &response);
				Ok(response)
			},
			Err(status) => {
				track_failure("check_version", ip.as_deref(), &status);
				Err(status)
			},
		}
	}

	type DownloadGameStream = net::game_distributor::DownloadStream;
	async fn download_game(
		&self,
		request: Request<DownloadRequest>,
	) -> Result<Response<Self::DownloadGameStream>, Status>
	{
		let ip = track_request("download_game", &request);
		match net::game_distributor::handle_game_distributor(request).await
		{
			Ok(response) => {
				track_stream_response("download_game", ip.as_deref());
				Ok(response)
			},
			Err(status) => {
				track_failure("download_game", ip.as_deref(), &status);
				Err(status)
			},
		}
	}

	async fn get_game_manifest(
		&self,
		request: Request<DownloadRequest>,
	) -> Result<Response<GameManifest>, Status>
	{
		let ip = track_request("get_game_manifest", &request);
		match net::game_files::handle_manifest(request).await
		{
			Ok(response) => {
				track_response("get_game_manifest", ip.as_deref(), &response);
				Ok(response)
			},
			Err(status) => {
				track_failure("get_game_manifest", ip.as_deref(), &status);
				Err(status)
			},
		}
	}

	type DownloadGameFilesStream = net::game_files::GameFileStream;
	async fn download_game_files(
		&self,
		request: Request<GameFilesRequest>,
	) -> Result<Response<Self::DownloadGameFilesStream>, Status>
	{
		let ip = track_request("download_game_files", &request);
		match net::game_files::handle_download_files(request).await
		{
			Ok(response) => {
				track_stream_response("download_game_files", ip.as_deref());
				Ok(response)
			},
			Err(status) => {
				track_failure("download_game_files", ip.as_deref(), &status);
				Err(status)
			},
		}
	}

	type WaitUpdateStream = tonic::codegen::tokio_stream::wrappers::ReceiverStream<Result<UpdateNotice, Status>>;
	async fn wait_update(
		&self,
		request: Request<Identificial>,
	) -> Result<Response<Self::WaitUpdateStream>, Status>
	{
		let ip = track_request("wait_update", &request);
		match net::update_notice::send_update_notice(request, &self.shared_tx).await
		{
			Ok(response) => {
				track_stream_response("wait_update", ip.as_deref());
				Ok(response)
			},
			Err(status) => {
				track_failure("wait_update", ip.as_deref(), &status);
				Err(status)
			},
		}
	}

	async fn list_comments(
		&self,
		request: Request<CommentListRequest>,
	) -> Result<Response<CommentListResponse>, Status>
	{
		let ip = track_request("list_comments", &request);
		match net::comments::handle_list_comments(request).await
		{
			Ok(response) => {
				track_response("list_comments", ip.as_deref(), &response);
				Ok(response)
			},
			Err(status) => {
				track_failure("list_comments", ip.as_deref(), &status);
				Err(status)
			},
		}
	}

	async fn add_comment(
		&self,
		request: Request<AddCommentRequest>,
	) -> Result<Response<Comment>, Status>
	{
		let ip = track_request("add_comment", &request);
		match net::comments::handle_add_comment(request).await
		{
			Ok(response) => {
				track_response("add_comment", ip.as_deref(), &response);
				Ok(response)
			},
			Err(status) => {
				track_failure("add_comment", ip.as_deref(), &status);
				Err(status)
			},
		}
	}
}

fn executable_root() -> Result<PathBuf, std::io::Error>
{
	let executable = std::env::current_exe()?;
	executable.parent()
		.map(Path::to_path_buf)
		.ok_or_else(|| std::io::Error::other("実行ファイルの場所を取得できません"))
}

async fn spawn_http_services(root: &Path, server: &GameLauncherServer) -> Result<(), String>
{
	let leaderboard_store = leaderboards::LeaderboardStore::load(root).await?;
	let leaderboard_bind = admin::leaderboard_bind(root)?;

	let admin_updates = server.shared_tx.clone();
	let admin_leaderboards = leaderboard_store.clone();
	tokio::spawn(async move {
		if let Err(error) = admin::serve(admin_updates, admin_leaderboards).await
		{
			tracing::error!("Admin UI error: {error}");
		}
	});

	tokio::spawn(async move {
		if let Err(error) = leaderboards::serve(leaderboard_store, &leaderboard_bind).await
		{
			tracing::error!("Leaderboard API error: {error}");
		}
	});
	Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>>
{
	let root = executable_root()?;
	let _log_guard = logging::init(&root)?;
	init::init();

	let shared_tx = spawn_monitor::spawn_monitor();
	let server = GameLauncherServer { shared_tx };
	spawn_http_services(&root, &server).await
		.map_err(std::io::Error::other)?;

	let addr_v4 = "0.0.0.0:50050".parse()?;
	let addr_v6 = "[::]:50050".parse()?;

	tracing::info!("GameLauncher Server listening on {} and {}", addr_v4, addr_v6);

	let server_v4 = server.clone();
	let handle_v4 = tokio::spawn(async move {
		if let Err(e) = Server::builder()
			.initial_stream_window_size(Some(1024 * 1024 * 64))
			.initial_connection_window_size(Some(1024 * 1024 * 256))
			.http2_adaptive_window(Some(false))
			.tcp_nodelay(true)
			.add_service(GameServiceServer::new(server_v4))
			.serve(addr_v4)
			.await
		{
			tracing::error!("IPv4 server error: {}", e);
		}
	});

	let server_v6 = server.clone();
	let handle_v6 = tokio::spawn(async move {
		if let Err(e) = Server::builder()
			.initial_stream_window_size(Some(1024 * 1024 * 64))
			.initial_connection_window_size(Some(1024 * 1024 * 256))
			.http2_adaptive_window(Some(false))
			.tcp_nodelay(true)
			.add_service(GameServiceServer::new(server_v6))
			.serve(addr_v6)
			.await
		{
			tracing::error!("IPv6 server error: {}", e);
		}
	});

	let _ = tokio::join!(handle_v4, handle_v6);
	Ok(())
}
