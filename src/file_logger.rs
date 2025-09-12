use log::warn;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

pub struct FileLogger {
    file_path: String,
}

impl FileLogger {
    pub fn new(file_path: &str) -> Self {
        Self {
            file_path: file_path.to_string(),
        }
    }

    pub fn log_with_category(&self, category: &str, message: &str) {
        let timestamp = Self::format_timestamp();
        let log_entry = format!("[{timestamp}] {category}: {message}\n");

        if let Err(e) = self.write_to_file(&log_entry) {
            warn!("Failed to write to log file: {e}");
        }
    }

    pub fn log_with_category_and_warn(&self, category: &str, message: &str) {
        let timestamp = Self::format_timestamp();
        let log_entry = format!("[{timestamp}] {category}: {message}\n");

        warn!("{}: {message}", category);

        if let Err(e) = self.write_to_file(&log_entry) {
            warn!("Failed to write to log file: {e}");
        }
    }

    pub fn log_with_category_fmt(&self, category: &str, args: std::fmt::Arguments) {
        let timestamp = Self::format_timestamp();
        let log_entry = format!("[{timestamp}] {category}: {args}\n");

        if let Err(e) = self.write_to_file(&log_entry) {
            warn!("Failed to write to log file: {e}");
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

    fn format_timestamp() -> String {
        chrono::Utc::now()
            .format("%Y-%m-%d %H:%M:%S UTC")
            .to_string()
    }

    pub fn file_path(&self) -> &str {
        &self.file_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_file_logger_creation() {
        let logger = FileLogger::new("test.log");
        assert_eq!(logger.file_path(), "test.log");
    }

    #[test]
    fn test_log_with_category() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();
        let logger = FileLogger::new(path);

        logger.log_with_category("TEST", "Test message");

        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("TEST: Test message"));
        assert!(content.contains("UTC"));
    }

    #[test]
    fn test_log_with_category_fmt() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();
        let logger = FileLogger::new(path);

        logger.log_with_category_fmt(
            "TEST",
            format_args!("Error {} with code {}", "connection", 404),
        );

        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("TEST: Error connection with code 404"));
        assert!(content.contains("UTC"));
    }

    #[test]
    fn test_multiple_logs() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();
        let logger = FileLogger::new(path);

        logger.log_with_category("FIRST", "First message");
        logger.log_with_category("SECOND", "Second message");

        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("FIRST: First message"));
        assert!(content.contains("SECOND: Second message"));

        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_clear_log() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();
        let logger = FileLogger::new(path);

        logger.log_with_category("TEST", "Test message");
        assert!(Path::new(path).exists());

        logger.clear_log().unwrap();
        assert!(!Path::new(path).exists());
    }

    #[test]
    fn test_clear_nonexistent_log() {
        let logger = FileLogger::new("nonexistent.log");
        assert!(logger.clear_log().is_ok());
    }

    #[test]
    fn test_timestamp_format() {
        let timestamp = FileLogger::format_timestamp();
        assert!(timestamp.contains("UTC"));
        assert!(timestamp.len() > 10);
    }
}
