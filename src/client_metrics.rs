use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tonic::Request;

const HISTORY_SECONDS: u64 = 60;

#[derive(Default)]
struct TrafficBucket
{
	timestamp: u64,
	sent_bytes: u64,
	received_bytes: u64,
}

#[derive(Default)]
struct ClientRecord
{
	connections: u32,
	connected_since: u64,
	last_seen: u64,
	total_sent_bytes: u64,
	total_received_bytes: u64,
	history: VecDeque<TrafficBucket>,
}

#[derive(Serialize)]
pub struct TrafficPoint
{
	pub timestamp: u64,
	pub sent_bytes: u64,
	pub received_bytes: u64,
}

#[derive(Serialize)]
pub struct ClientSnapshot
{
	pub ip: String,
	pub connections: u32,
	pub connected_since: u64,
	pub last_seen: u64,
	pub total_sent_bytes: u64,
	pub total_received_bytes: u64,
	pub sent_bytes_per_second: u64,
	pub received_bytes_per_second: u64,
	pub history: Vec<TrafficPoint>,
}

static CLIENTS: OnceLock<Mutex<HashMap<String, ClientRecord>>> = OnceLock::new();

fn clients() -> &'static Mutex<HashMap<String, ClientRecord>>
{
	CLIENTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn unix_seconds() -> u64
{
	SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

fn canonical_ip(ip: IpAddr) -> String
{
	match ip
	{
		IpAddr::V6(ip) => ip.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(IpAddr::V6(ip)).to_string(),
		IpAddr::V4(ip) => ip.to_string(),
	}
}

pub fn request_ip<T>(request: &Request<T>) -> Option<String>
{
	request.remote_addr().map(|address| canonical_ip(address.ip()))
}

fn update_record(ip: &str, sent_bytes: u64, received_bytes: u64)
{
	let now = unix_seconds();
	let Ok(mut clients) = clients().lock() else { return; };
	let record = clients.entry(ip.to_string()).or_default();
	record.last_seen = now;
	record.total_sent_bytes = record.total_sent_bytes.saturating_add(sent_bytes);
	record.total_received_bytes = record.total_received_bytes.saturating_add(received_bytes);
	while record.history.front().is_some_and(|bucket| bucket.timestamp + HISTORY_SECONDS <= now)
	{
		record.history.pop_front();
	}
	if record.history.back().is_none_or(|bucket| bucket.timestamp != now)
	{
		record.history.push_back(TrafficBucket { timestamp: now, ..TrafficBucket::default() });
	}
	if let Some(bucket) = record.history.back_mut()
	{
		bucket.sent_bytes = bucket.sent_bytes.saturating_add(sent_bytes);
		bucket.received_bytes = bucket.received_bytes.saturating_add(received_bytes);
	}
}

pub fn record(ip: Option<&str>, sent_bytes: u64, received_bytes: u64)
{
	if let Some(ip) = ip { update_record(ip, sent_bytes, received_bytes); }
}

pub fn record_ip(ip: IpAddr, sent_bytes: u64, received_bytes: u64)
{
	update_record(&canonical_ip(ip), sent_bytes, received_bytes);
}

pub struct ConnectionGuard
{
	ip: Option<String>,
}

impl ConnectionGuard
{
	pub fn new(ip: Option<String>) -> Self
	{
		if let Some(ip) = ip.as_deref()
		{
			let now = unix_seconds();
			if let Ok(mut clients) = clients().lock()
			{
				let record = clients.entry(ip.to_string()).or_default();
				if record.connections == 0 { record.connected_since = now; }
				record.connections = record.connections.saturating_add(1);
				record.last_seen = now;
			}
		}
		Self { ip }
	}
}

impl Drop for ConnectionGuard
{
	fn drop(&mut self)
	{
		let Some(ip) = self.ip.as_deref() else { return; };
		if let Ok(mut clients) = clients().lock()
		{
			if let Some(record) = clients.get_mut(ip)
			{
				record.connections = record.connections.saturating_sub(1);
				record.last_seen = unix_seconds();
			}
		}
	}
}

pub fn snapshots() -> Vec<ClientSnapshot>
{
	let now = unix_seconds();
	let Ok(clients) = clients().lock() else { return Vec::new(); };
	let mut snapshots = clients.iter()
		.filter(|(_, record)| record.connections > 0)
		.map(|(ip, record)| {
			let history = ((now.saturating_sub(HISTORY_SECONDS - 1))..=now)
				.map(|timestamp| {
					let bucket = record.history.iter().find(|bucket| bucket.timestamp == timestamp);
					TrafficPoint {
						timestamp,
						sent_bytes: bucket.map_or(0, |bucket| bucket.sent_bytes),
						received_bytes: bucket.map_or(0, |bucket| bucket.received_bytes),
					}
				})
				.collect::<Vec<_>>();
			let recent = history.iter().rev().take(2);
			let sent_bytes_per_second = recent.clone().map(|point| point.sent_bytes).sum::<u64>() / 2;
			let received_bytes_per_second = recent.map(|point| point.received_bytes).sum::<u64>() / 2;
			ClientSnapshot {
				ip: ip.clone(),
				connections: record.connections,
				connected_since: record.connected_since,
				last_seen: record.last_seen,
				total_sent_bytes: record.total_sent_bytes,
				total_received_bytes: record.total_received_bytes,
				sent_bytes_per_second,
				received_bytes_per_second,
				history,
			}
		})
		.collect::<Vec<_>>();
	snapshots.sort_by(|left, right| left.ip.cmp(&right.ip));
	snapshots
}

#[cfg(test)]
mod tests
{
	use super::*;

	#[test]
	fn canonicalizes_ipv4_mapped_addresses()
	{
		assert_eq!(canonical_ip("::ffff:192.168.1.20".parse().unwrap()), "192.168.1.20");
	}
}
