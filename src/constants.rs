//! Shared application-wide constants.
//!
//! Centralising user-facing filename strings here prevents drift when the
//! same filename needs to be referenced from multiple modules.

/// Filename used for the bootstrap connection log.
pub const BOOTSTRAP_LOG_FILE: &str = "bootstrap.log";
