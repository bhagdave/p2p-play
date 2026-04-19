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
