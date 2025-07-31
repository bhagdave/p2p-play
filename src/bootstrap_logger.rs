use chrono;
use log::{debug, warn};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

/// Logger specifically for bootstrap-related messages that should go to a file
/// instead of the TUI to reduce UI clutter
pub struct BootstrapLogger {
    file_path: String,
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
        let log_entry = format!("[{}] BOOTSTRAP: {}\n", timestamp, message);

        // Use debug logging to stdout (which goes to log files via env_logger)
        debug!("Bootstrap: {}", message);

        // Also write to dedicated bootstrap log file
        if let Err(e) = self.write_to_file(&log_entry) {
            // If file writing fails, fall back to warn logging
            warn!("Failed to write to bootstrap log file: {}", e);
        }
    }

    /// Log bootstrap initialization and configuration messages
    pub fn log_init(&self, message: &str) {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let log_entry = format!("[{}] BOOTSTRAP_INIT: {}\n", timestamp, message);

        debug!("Bootstrap Init: {}", message);

        if let Err(e) = self.write_to_file(&log_entry) {
            warn!("Failed to write bootstrap init to log file: {}", e);
        }
    }

    /// Log bootstrap attempt messages
    pub fn log_attempt(&self, message: &str) {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let log_entry = format!("[{}] BOOTSTRAP_ATTEMPT: {}\n", timestamp, message);

        debug!("Bootstrap Attempt: {}", message);

        if let Err(e) = self.write_to_file(&log_entry) {
            warn!("Failed to write bootstrap attempt to log file: {}", e);
        }
    }

    /// Log bootstrap status updates
    pub fn log_status(&self, message: &str) {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let log_entry = format!("[{}] BOOTSTRAP_STATUS: {}\n", timestamp, message);

        debug!("Bootstrap Status: {}", message);

        if let Err(e) = self.write_to_file(&log_entry) {
            warn!("Failed to write bootstrap status to log file: {}", e);
        }
    }

    /// Log bootstrap errors
    pub fn log_error(&self, message: &str) {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let log_entry = format!("[{}] BOOTSTRAP_ERROR: {}\n", timestamp, message);

        // Bootstrap errors should still be visible via warn level
        warn!("Bootstrap Error: {}", message);

        if let Err(e) = self.write_to_file(&log_entry) {
            warn!("Failed to write bootstrap error to log file: {}", e);
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_bootstrap_logger_creation() {
        let logger = BootstrapLogger::new("bootstrap.log");
        assert_eq!(logger.file_path, "bootstrap.log");
    }

    #[test]
    fn test_log_bootstrap_message() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();
        let logger = BootstrapLogger::new(path);

        logger.log("Test bootstrap message");

        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("BOOTSTRAP: Test bootstrap message"));
        assert!(content.contains("UTC"));
    }

    #[test]
    fn test_log_init_message() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();
        let logger = BootstrapLogger::new(path);

        logger.log_init("Bootstrap initialized with 3 peers");

        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("BOOTSTRAP_INIT: Bootstrap initialized with 3 peers"));
        assert!(content.contains("UTC"));
    }

    #[test]
    fn test_log_attempt_message() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();
        let logger = BootstrapLogger::new(path);

        logger.log_attempt("Attempting automatic DHT bootstrap (attempt 1/5)");

        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("BOOTSTRAP_ATTEMPT: Attempting automatic DHT bootstrap (attempt 1/5)"));
        assert!(content.contains("UTC"));
    }

    #[test]
    fn test_log_status_message() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();
        let logger = BootstrapLogger::new(path);

        logger.log_status("DHT: Connected (5 peers, 30s ago)");

        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("BOOTSTRAP_STATUS: DHT: Connected (5 peers, 30s ago)"));
        assert!(content.contains("UTC"));
    }

    #[test]
    fn test_log_error_message() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();
        let logger = BootstrapLogger::new(path);

        logger.log_error("Failed to start DHT bootstrap: timeout");

        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("BOOTSTRAP_ERROR: Failed to start DHT bootstrap: timeout"));
        assert!(content.contains("UTC"));
    }

    #[test]
    fn test_multiple_bootstrap_logs() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();
        let logger = BootstrapLogger::new(path);

        logger.log_init("Bootstrap config loaded");
        logger.log_attempt("Starting bootstrap attempt");
        logger.log_status("Bootstrap successful");

        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("BOOTSTRAP_INIT: Bootstrap config loaded"));
        assert!(content.contains("BOOTSTRAP_ATTEMPT: Starting bootstrap attempt"));
        assert!(content.contains("BOOTSTRAP_STATUS: Bootstrap successful"));

        // Should have three lines
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_clear_bootstrap_log() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();
        let logger = BootstrapLogger::new(path);

        logger.log("Test message");
        assert!(Path::new(path).exists());

        logger.clear_log().unwrap();
        assert!(!Path::new(path).exists());
    }

    #[test]
    fn test_clear_nonexistent_bootstrap_log() {
        let logger = BootstrapLogger::new("nonexistent_bootstrap.log");
        // Should not panic when clearing a file that doesn't exist
        assert!(logger.clear_log().is_ok());
    }
}