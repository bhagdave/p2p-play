use super::protocol::{DaemonRequest, DaemonResponse};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
#[cfg(unix)]
use tokio::net::UnixStream;

#[cfg(not(unix))]
pub async fn send_request(
    _socket_path: &Path,
    _req: &DaemonRequest,
) -> Result<DaemonResponse, String> {
    Err("Daemon communication is only supported on Unix systems.".to_string())
}

#[cfg(unix)]
pub async fn send_request(
    socket_path: &Path,
    req: &DaemonRequest,
) -> Result<DaemonResponse, String> {
    let stream = UnixStream::connect(socket_path).await.map_err(|e| {
        format!(
            "Cannot connect to daemon at '{}': {e}\nIs the daemon running? Try: p2p-play daemon",
            socket_path.display()
        )
    })?;

    let (read_half, mut write_half) = stream.into_split();

    let mut json = serde_json::to_string(req).map_err(|e| format!("Serialize error: {e}"))?;
    json.push('\n');
    write_half
        .write_all(json.as_bytes())
        .await
        .map_err(|e| format!("Write error: {e}"))?;

    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .await
        .map_err(|e| format!("Read error: {e}"))?;

    serde_json::from_str(line.trim()).map_err(|e| format!("Parse error: {e} — got: {line}"))
}

pub fn print_response(resp: &DaemonResponse) {
    match resp {
        DaemonResponse::Peers { peers } => {
            if peers.is_empty() {
                println!("No connected peers.");
            } else {
                println!("{:<20} {}", "NAME", "PEER ID");
                println!("{}", "-".repeat(72));
                for p in peers {
                    println!("{:<20} {}", p.name, p.peer_id);
                }
            }
        }

        DaemonResponse::Channels { channels } => {
            if channels.is_empty() {
                println!("No channels found.");
            } else {
                println!("{:<20} DESCRIPTION", "CHANNEL");
                println!("{}", "-".repeat(72));
                for channel in channels {
                    println!("{:<20} {}", channel.name, channel.description);
                }
            }
        }

        DaemonResponse::Stories { channel, stories } => {
            if stories.is_empty() {
                println!("No stories found in channel '{channel}'.");
            } else {
                println!("Stories in channel '{channel}':");
                println!("{:>4} {:<10} NAME", "ID", "VISIBILITY");
                println!("{}", "-".repeat(72));
                for story in stories {
                    let visibility = if story.public { "public" } else { "private" };
                    println!("{:>4} {:<10} {}", story.id, visibility, story.name);
                }
            }
        }

        DaemonResponse::Conversations { conversations } => {
            if conversations.is_empty() {
                println!("No conversations.");
            } else {
                println!("{:<20} {:>7} {}", "PEER", "UNREAD", "LAST ACTIVITY");
                println!("{}", "-".repeat(60));
                for c in conversations {
                    let ts = format_timestamp(c.last_activity);
                    println!("{:<20} {:>7} {}", c.peer_name, c.unread_count, ts);
                }
            }
        }

        DaemonResponse::Unread { messages } => {
            if messages.is_empty() {
                println!("No unread messages.");
            } else {
                for m in messages {
                    println!("Peer: {} ({})", m.peer_name, m.peer_id);
                    for msg in &m.messages {
                        let ts = format_timestamp(msg.timestamp);
                        println!("  - [{}] {}", ts, msg.content);
                    }
                    println!();
                }
            }
        }

        DaemonResponse::Error { message } => {
            eprintln!("Error: {message}");
        }
    }
}

fn format_timestamp(ts: u64) -> String {
    if ts == 0 {
        return "never".to_string();
    }
    // Format as seconds-ago for simplicity (no chrono dependency needed).
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(ts);
    let ago = now.saturating_sub(ts);
    if ago < 60 {
        format!("{ago}s ago")
    } else if ago < 3600 {
        format!("{}m ago", ago / 60)
    } else if ago < 86400 {
        format!("{}h ago", ago / 3600)
    } else {
        format!("{}d ago", ago / 86400)
    }
}
