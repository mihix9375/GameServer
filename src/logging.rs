use std::fmt;
use std::fs::OpenOptions;
use std::path::Path;

use chrono::Local;
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::layer::{Layer, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{filter::filter_fn, fmt as tracing_fmt};

/// Events with this target are written only to the detail JSONL file.
pub const DETAIL_TARGET: &str = "gameserver::detail";

#[derive(Clone, Copy)]
struct LocalTimer;

impl FormatTime for LocalTimer
{
	fn format_time(&self, writer: &mut Writer<'_>) -> fmt::Result
	{
		write!(writer, "{}", Local::now().format("%Y-%m-%dT%H:%M:%S%.3f%:z"))
	}
}

/// Initializes timestamped console logging and a file-only detailed JSONL log.
/// The returned guard must stay alive until the process exits so buffered lines are flushed.
pub fn init(root: &Path) -> Result<WorkerGuard, Box<dyn std::error::Error>>
{
	let log_directory = root.join("logs");
	std::fs::create_dir_all(&log_directory)?;
	let detail_file = OpenOptions::new()
		.create(true)
		.append(true)
		.open(log_directory.join("server-details.jsonl"))?;
	let (detail_writer, guard) = tracing_appender::non_blocking::NonBlockingBuilder::default()
		.lossy(false)
		.finish(detail_file);

	let console_layer = tracing_fmt::layer()
		.with_timer(LocalTimer)
		.with_target(false)
		.with_filter(filter_fn(|metadata| {
			metadata.target() != DETAIL_TARGET && *metadata.level() <= Level::INFO
		}));
	let detail_layer = tracing_fmt::layer()
		.json()
		.with_timer(LocalTimer)
		.with_writer(detail_writer)
		.with_ansi(false)
		.with_current_span(false)
		.with_span_list(false)
		.with_thread_ids(true)
		.with_thread_names(true)
		.with_file(true)
		.with_line_number(true)
		.with_filter(filter_fn(|metadata| metadata.target() == DETAIL_TARGET));

	tracing_subscriber::registry()
		.with(console_layer)
		.with(detail_layer)
		.try_init()?;
	Ok(guard)
}
