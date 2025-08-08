use log::{debug, warn};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

/// Logger specifically for bootstrap-related messages that should go to a file
/// instead of the TUI to reduce UI clutter
pub struct BootstrapLogger {
    pub file_path: String,
}

impl BootstrapLogger {
    pub fn new(file_path: &str) -> Self {
        Self {
            file_path: file_path.to_string(),
        }
    }

    /// Log bootstrap-related messages to file
    pub fn log(&self, message: &str) {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let log_entry = format!("[{timestamp}] BOOTSTRAP: {message}\n");

        // Use debug logging to stdout (which goes to log files via env_logger)
        debug!("Bootstrap: {message}");

        // Also write to dedicated bootstrap log file
        if let Err(e) = self.write_to_file(&log_entry) {
            // If file writing fails, fall back to warn logging
            warn!("Failed to write to bootstrap log file: {e}");
        }
    }

    /// Log bootstrap initialization and configuration messages
    pub fn log_init(&self, message: &str) {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let log_entry = format!("[{timestamp}] BOOTSTRAP_INIT: {message}\n");

        debug!("Bootstrap Init: {message}");

        if let Err(e) = self.write_to_file(&log_entry) {
            warn!("Failed to write bootstrap init to log file: {e}");
        }
    }

    /// Log bootstrap attempt messages
    pub fn log_attempt(&self, message: &str) {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let log_entry = format!("[{timestamp}] BOOTSTRAP_ATTEMPT: {message}\n");

        debug!("Bootstrap Attempt: {message}");

        if let Err(e) = self.write_to_file(&log_entry) {
            warn!("Failed to write bootstrap attempt to log file: {e}");
        }
    }

    /// Log bootstrap status updates
    pub fn log_status(&self, message: &str) {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let log_entry = format!("[{timestamp}] BOOTSTRAP_STATUS: {message}\n");

        debug!("Bootstrap Status: {message}");

        if let Err(e) = self.write_to_file(&log_entry) {
            warn!("Failed to write bootstrap status to log file: {e}");
        }
    }

    /// Log bootstrap errors
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

    /// Clear the bootstrap log file
    pub fn clear_log(&self) -> std::io::Result<()> {
        if Path::new(&self.file_path).exists() {
            std::fs::remove_file(&self.file_path)?;
        }
        Ok(())
    }
}
