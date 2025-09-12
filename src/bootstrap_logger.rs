use log::warn;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

pub struct BootstrapLogger {
    pub file_path: String,
}

impl BootstrapLogger {
    pub fn new(file_path: &str) -> Self {
        Self {
            file_path: file_path.to_string(),
        }
    }

    pub fn log(&self, message: &str) {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let log_entry = format!("[{timestamp}] BOOTSTRAP: {message}\n");

        if let Err(e) = self.write_to_file(&log_entry) {
            warn!("Failed to write to bootstrap log file: {e}");
        }
    }

    pub fn log_init(&self, message: &str) {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let log_entry = format!("[{timestamp}] BOOTSTRAP_INIT: {message}\n");

        if let Err(e) = self.write_to_file(&log_entry) {
            warn!("Failed to write bootstrap init to log file: {e}");
        }
    }

    pub fn log_attempt(&self, message: &str) {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let log_entry = format!("[{timestamp}] BOOTSTRAP_ATTEMPT: {message}\n");

        if let Err(e) = self.write_to_file(&log_entry) {
            warn!("Failed to write bootstrap attempt to log file: {e}");
        }
    }

    pub fn log_status(&self, message: &str) {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let log_entry = format!("[{timestamp}] BOOTSTRAP_STATUS: {message}\n");

        if let Err(e) = self.write_to_file(&log_entry) {
            warn!("Failed to write bootstrap status to log file: {e}");
        }
    }

    pub fn log_error(&self, message: &str) {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let log_entry = format!("[{timestamp}] BOOTSTRAP_ERROR: {message}\n");

        // Bootstrap errors should still be visible via warn level
        warn!("Bootstrap Error: {message}");

        if let Err(e) = self.write_to_file(&log_entry) {
            warn!("Failed to write bootstrap error to log file: {e}");
        }
    }

    fn write_to_file(&self, content: &str) -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)?;
        file.write_all(content.as_bytes())?;
        file.flush()?;
        Ok(())
    }

    pub fn clear_log(&self) -> std::io::Result<()> {
        if Path::new(&self.file_path).exists() {
            std::fs::remove_file(&self.file_path)?;
        }
        Ok(())
    }
}
