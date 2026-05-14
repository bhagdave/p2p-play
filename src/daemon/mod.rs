use std::path::{PathBuf};
use tokio::net::UnixListener;

pub struct DaemonServer{
    listener: UnixListener,
    socket_path: PathBuf,
    pid_file_path: PathBuf,
}

impl DaemonServer {
    pub async fn new(socket_path: PathBuf, pid_file_path: PathBuf) -> std::io::Result<Self> {
        let socket_path = socket_path.to_path_buf();
        let pid_file_path = pid_file_path.to_path_buf();
        if socket_path.exists() {
            std::fs::remove_file(&socket_path)?;
        }

        let listener = UnixListener::bind(&socket_path)?;
        std::fs::write(&pid_file_path, format!("{}\n", std::process::id()))?; 
        Ok(Self { listener, socket_path, pid_file_path })
    }

    pub async fn run(&self) -> std::io::Result<()> {
        let socket_path = self.socket_path.clone();
        let pid_file_path = self.pid_file_path.clone();

        loop {
            match self.listener.accept().await {
                Ok((stream, _)) => {
                    println!("New client connected");
                    // Handle the client connection in a separate task
                    tokio::spawn(async move {
                        // Here you would handle the client connection
                        // For example, read/write data from/to the stream
                    });
                }
                Err(e) => {
                    eprintln!("Failed to accept client connection: {}", e);
                }
            }
        }
    }
}
