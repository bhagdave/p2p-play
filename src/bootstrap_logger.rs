use crate::file_logger::FileLogger;
use log::warn;

pub struct BootstrapLogger {
    logger: FileLogger,
}

impl BootstrapLogger {
    pub fn new(file_path: &str) -> Self {
        Self {
            logger: FileLogger::new(file_path),
        }
    }

    pub fn file_path(&self) -> &str {
        self.logger.file_path()
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

    pub fn clear_log(&self) -> std::io::Result<()> {
        self.logger.clear_log()
    }
}
