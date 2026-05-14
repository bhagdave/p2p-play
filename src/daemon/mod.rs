use std::path::{PathBuf};
use tokio::net::UnixListener;

pub struct DaemonServer{
    listener: UnixListener,
    socket_path: PathBuf,
    pid_file_path: PathBuf,
}

impl DaemonServer {
    pub async fn new(socket_path: PathBuf, pid_file_path: PathBuf) -> std::io::Result<Self> {
        // Ensure the socket file does not already exist
        if socket_path.exists() {
            std::fs::remove_file(&socket_path)?;
        }

        let listener = UnixListener::bind(&socket_path)?;
        Ok(Self { listener, socket_path, pid_file_path })
    }

    pub async fn run(&self) -> std::io::Result<()> {
        // Write the PID file
        let pid = std::process::id();
        std::fs::write(&self.pid_file_path, pid.to_string())?;

        println!("Daemon server running with PID {}. Listening on {:?}", pid, self.socket_path);

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
