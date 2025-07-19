use log::{error, warn};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

/// Error logger that writes errors to a file instead of the UI
pub struct ErrorLogger {
    file_path: String,
}

impl ErrorLogger {
    pub fn new(file_path: &str) -> Self {
        Self {
            file_path: file_path.to_string(),
        }
    }

    pub fn log_error(&self, error_message: &str) {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let log_entry = format!("[{}] ERROR: {}\n", timestamp, error_message);

        if let Err(e) = self.write_to_file(&log_entry) {
            // If file writing fails, fall back to stderr
            eprintln!("Failed to write to error log file: {}", e);
            eprintln!("{}", log_entry.trim());
        }
    }

    /// Log network/connection errors that should be hidden from UI but preserved in logs
    pub fn log_network_error(&self, source: &str, error_message: &str) {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let log_entry = format!("[{}] NETWORK_ERROR [{}]: {}\n", timestamp, source, error_message);

        if let Err(e) = self.write_to_file(&log_entry) {
            // If file writing fails, use warn instead of error to avoid console spam
            warn!("Failed to write network error to log file: {}", e);
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_error_logger_creation() {
        let error_logger = ErrorLogger::new("test.log");
        assert_eq!(error_logger.file_path, "test.log");
    }

    #[test]
    fn test_log_error_to_file() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();
        let error_logger = ErrorLogger::new(path);

        error_logger.log_error("Test error message");

        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("ERROR: Test error message"));
        assert!(content.contains("UTC"));
    }

    #[test]
    fn test_multiple_error_logs() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();
        let error_logger = ErrorLogger::new(path);

        error_logger.log_error("First error");
        error_logger.log_error("Second error");

        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("First error"));
        assert!(content.contains("Second error"));

        // Should have two lines
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_clear_log() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();
        let error_logger = ErrorLogger::new(path);

        error_logger.log_error("Test error");
        assert!(Path::new(path).exists());

        error_logger.clear_log().unwrap();
        assert!(!Path::new(path).exists());
    }

    #[test]
    fn test_clear_nonexistent_log() {
        let error_logger = ErrorLogger::new("nonexistent.log");
        // Should not panic when clearing a file that doesn't exist
        assert!(error_logger.clear_log().is_ok());
    }
}
