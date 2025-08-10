/// Common utility functions for storage operations
use crate::errors::StorageResult;
use rusqlite::{Connection, Row};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Get the current Unix timestamp in seconds
pub fn get_current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Get the next available ID for a table
pub async fn get_next_id(conn: &Arc<Mutex<Connection>>, table: &str) -> StorageResult<i64> {
    let conn = conn.lock().await;
    let query = format!("SELECT COALESCE(MAX(id), -1) + 1 as next_id FROM {table}");
    let mut stmt = conn.prepare(&query)?;
    let next_id: i64 = stmt.query_row([], |row| row.get(0))?;
    Ok(next_id)
}

/// Convert database boolean (0/1) to Rust bool
pub fn db_bool_to_rust(value: i64) -> bool {
    value != 0
}

/// Convert Rust bool to database integer (0/1)
pub fn rust_bool_to_db(value: bool) -> String {
    if value { "1" } else { "0" }.to_string()
}

/// Get optional string with default value
pub fn get_optional_string_with_default(
    row: &Row,
    index: usize,
    default: &str,
) -> Result<String, rusqlite::Error> {
    Ok(row
        .get::<_, Option<String>>(index)?
        .unwrap_or_else(|| default.to_string()))
}

/// Get timestamp with default value of 0
pub fn get_timestamp_with_default(row: &Row, index: usize) -> Result<u64, rusqlite::Error> {
    Ok(row.get::<_, i64>(index).unwrap_or(0) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp_generation() {
        let ts1 = get_current_timestamp();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let ts2 = get_current_timestamp();
        assert!(ts2 >= ts1);
    }

    #[test]
    fn test_bool_conversion() {
        assert!(!db_bool_to_rust(0));
        assert!(db_bool_to_rust(1));
        assert!(db_bool_to_rust(42));

        assert_eq!(rust_bool_to_db(false), "0");
        assert_eq!(rust_bool_to_db(true), "1");
    }
}
