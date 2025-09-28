use crate::file_logger::FileLogger;
use log::warn;

pub struct ErrorLogger {
    logger: FileLogger,
}

impl ErrorLogger {
    pub fn new(file_path: &str) -> Self {
        Self {
            logger: FileLogger::new(file_path),
        }
    }

    pub fn file_path(&self) -> &str {
        self.logger.file_path()
    }

    pub fn log_error(&self, error_message: &str) {
        self.logger.log_with_category("ERROR", error_message);
    }

    pub fn log_network_error(&self, source: &str, error_message: &str) {
        self.logger
            .log_with_category(&format!("NETWORK_ERROR [{}]", source), error_message);
    }

    pub fn log_network_error_fmt(&self, source: &str, args: std::fmt::Arguments) {
        self.logger
            .log_with_category_fmt(&format!("NETWORK_ERROR [{}]", source), args);
    }

    pub fn clear_log(&self) -> std::io::Result<()> {
        self.logger.clear_log()
    }
}

#[macro_export]
macro_rules! log_network_error {
    ($logger:expr, $source:expr, $($arg:tt)*) => {
        $logger.log_network_error_fmt($source, format_args!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::NamedTempFile;

    #[test]
    fn test_error_logger_creation() {
        let error_logger = ErrorLogger::new("test.log");
        assert_eq!(error_logger.file_path(), "test.log");
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

    #[test]
    fn test_log_network_error_fmt() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();
        let error_logger = ErrorLogger::new(path);

        // Test the new formatting method
        error_logger.log_network_error_fmt(
            "test_source",
            format_args!("Error {} with code {}", "connection", 404),
        );

        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("NETWORK_ERROR [test_source]: Error connection with code 404"));
        assert!(content.contains("UTC"));
    }

    #[test]
    fn test_log_network_error_macro() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();
        let error_logger = ErrorLogger::new(path);

        // Test the macro
        log_network_error!(
            error_logger,
            "macro_test",
            "Failed to connect to peer {} with error {}",
            "peer123",
            "timeout"
        );

        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains(
            "NETWORK_ERROR [macro_test]: Failed to connect to peer peer123 with error timeout"
        ));
        assert!(content.contains("UTC"));
    }

    #[test]
    fn test_log_network_error() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();
        let error_logger = ErrorLogger::new(path);

        // Test the direct log_network_error method
        error_logger.log_network_error("mdns", "Discovery failed for peer 12345");

        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("NETWORK_ERROR [mdns]: Discovery failed for peer 12345"));
        assert!(content.contains("UTC"));
    }
}
