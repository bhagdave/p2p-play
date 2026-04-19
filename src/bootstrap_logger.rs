use crate::file_logger::{CategoryLoggerBase, FileLogger};
use log::warn;

pub struct BootstrapLogger {
    logger: FileLogger,
}

impl CategoryLoggerBase for BootstrapLogger {
    fn inner_logger(&self) -> &FileLogger {
        &self.logger
    }
}

impl BootstrapLogger {
    pub fn new(file_path: &str) -> Self {
        Self {
            logger: FileLogger::new(file_path),
        }
    }

    /// Returns the path to the log file.
    pub fn file_path(&self) -> &str {
        CategoryLoggerBase::file_path(self)
    }

    /// Removes the log file (no-op if it does not exist).
    pub fn clear_log(&self) -> std::io::Result<()> {
        CategoryLoggerBase::clear_log(self)
    }

    pub fn log(&self, message: &str) {
        self.logger.log_with_category("BOOTSTRAP", message);
    }

    pub fn log_init(&self, message: &str) {
        self.logger.log_with_category("BOOTSTRAP_INIT", message);
    }

    pub fn log_attempt(&self, message: &str) {
        self.logger.log_with_category("BOOTSTRAP_ATTEMPT", message);
    }

    pub fn log_status(&self, message: &str) {
        self.logger.log_with_category("BOOTSTRAP_STATUS", message);
    }

    pub fn log_error(&self, message: &str) {
        warn!("Bootstrap Error: {message}");
        self.logger.log_with_category("BOOTSTRAP_ERROR", message);
    }
}
