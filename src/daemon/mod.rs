pub mod client;
pub mod protocol;

pub use crate::constants::{PID_FILE, SOCKET_FILE};
use protocol::{DaemonCommand, DaemonRequest, DaemonResponse};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, oneshot};

pub struct DaemonServer {
    listener: UnixListener,
    socket_path: PathBuf,
    pid_file_path: PathBuf,
    cmd_sender: mpsc::UnboundedSender<DaemonCommand>,
}

impl DaemonServer {
    pub fn new(
        socket_path: impl AsRef<Path>,
        pid_file_path: impl AsRef<Path>,
        cmd_sender: mpsc::UnboundedSender<DaemonCommand>,
    ) -> std::io::Result<Self> {
        let socket_path = socket_path.as_ref().to_path_buf();
        let pid_file_path = pid_file_path.as_ref().to_path_buf();

        if socket_path.exists() {
            std::fs::remove_file(&socket_path)?;
        }

        let listener = UnixListener::bind(&socket_path)?;

        std::fs::write(&pid_file_path, format!("{}\n", std::process::id()))?;

        Ok(Self {
            listener,
            socket_path,
            pid_file_path,
            cmd_sender,
        })
    }

    pub async fn run(self, mut shutdown_rx: oneshot::Receiver<()>) {
        let socket_path = self.socket_path.clone();
        let pid_file_path = self.pid_file_path.clone();

        loop {
            tokio::select! {
                accept = self.listener.accept() => {
                    match accept {
                        Ok((stream, _)) => {
                            let sender = self.cmd_sender.clone();
                            tokio::spawn(handle_connection(stream, sender));
                        }
                        Err(e) => {
                            eprintln!("[daemon] Accept error: {e}");
                        }
                    }
                }
                _ = &mut shutdown_rx => {
                    break;
                }
            }
        }

        // Cleanup socket and PID file on shutdown.
        let _ = std::fs::remove_file(&socket_path);
        let _ = std::fs::remove_file(&pid_file_path);
    }
}
#[cfg(unix)]
async fn handle_connection(stream: UnixStream, cmd_sender: mpsc::UnboundedSender<DaemonCommand>) {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();

    if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
        return;
    }

    let req: DaemonRequest = match serde_json::from_str(line.trim()) {
        Ok(r) => r,
        Err(e) => {
            let resp = DaemonResponse::Error {
                message: format!("Invalid request: {e}"),
            };
            write_response(&mut write_half, &resp).await;
            return;
        }
    };

    let (tx, rx) = oneshot::channel::<DaemonResponse>();

    if cmd_sender.send((req, tx)).is_err() {
        let resp = DaemonResponse::Error {
            message: "Daemon event loop is unavailable".to_string(),
        };
        write_response(&mut write_half, &resp).await;
        return;
    }

    let response = match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
        Ok(Ok(resp)) => resp,
        Ok(Err(_)) => DaemonResponse::Error {
            message: "Daemon dropped the response channel".to_string(),
        },
        Err(_) => DaemonResponse::Error {
            message: "Request timed out after 30 seconds".to_string(),
        },
    };

    write_response(&mut write_half, &response).await;
}

#[cfg(unix)]
async fn write_response(
    write_half: &mut tokio::net::unix::OwnedWriteHalf,
    resp: &DaemonResponse,
) {
    match serde_json::to_string(resp) {
        Ok(mut json) => {
            json.push('\n');
            let _ = write_half.write_all(json.as_bytes()).await;
        }
        Err(e) => {
            let fallback = format!("{{\"type\":\"error\",\"message\":\"serialize error: {e}\"}}\n");
            let _ = write_half.write_all(fallback.as_bytes()).await;
        }
    }
}
