use p2p_play::bootstrap_logger::BootstrapLogger;
use tempfile::NamedTempFile;
use std::fs;

#[test]
fn test_bootstrap_messages_logged_to_file() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();
    let bootstrap_logger = BootstrapLogger::new(path);
    
    // Test different types of bootstrap messages
    bootstrap_logger.log_init("Bootstrap initialized with 3 peers");
    bootstrap_logger.log_attempt("Attempting automatic DHT bootstrap (attempt 1/5)");
    bootstrap_logger.log_status("DHT: Connected (5 peers, 30s ago)");
    bootstrap_logger.log_error("Failed to start DHT bootstrap: timeout");
    bootstrap_logger.log("General bootstrap message");
    
    // Read the log file content
    let content = fs::read_to_string(path).unwrap();
    
    // Verify all message types are logged
    assert!(content.contains("BOOTSTRAP_INIT: Bootstrap initialized with 3 peers"));
    assert!(content.contains("BOOTSTRAP_ATTEMPT: Attempting automatic DHT bootstrap (attempt 1/5)"));
    assert!(content.contains("BOOTSTRAP_STATUS: DHT: Connected (5 peers, 30s ago)"));
    assert!(content.contains("BOOTSTRAP_ERROR: Failed to start DHT bootstrap: timeout"));
    assert!(content.contains("BOOTSTRAP: General bootstrap message"));
    
    // Verify timestamps are included
    assert!(content.contains("UTC"));
    
    // Verify we have the expected number of lines
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 5);
}

#[test]
fn test_bootstrap_logger_file_creation() {
    // Test that the logger creates a new file if it doesn't exist
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap().to_string();
    
    // Remove the temp file to test creation
    drop(temp_file);
    
    let bootstrap_logger = BootstrapLogger::new(&path);
    bootstrap_logger.log("Test message");
    
    // File should exist now
    assert!(std::path::Path::new(&path).exists());
    
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("BOOTSTRAP: Test message"));
    
    // Clean up
    let _ = fs::remove_file(&path);
}