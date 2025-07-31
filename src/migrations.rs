use rusqlite::Connection;
use std::error::Error;

/// Creates the database tables if they don't exist
pub fn create_tables(conn: &Connection) -> Result<(), Box<dyn Error>> {
    // Create stories table
    conn.execute(
        r#"
        CREATE TABLE IF NOT EXISTS stories (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            header TEXT NOT NULL,
            body TEXT NOT NULL,
            public BOOLEAN NOT NULL DEFAULT 0,
            channel TEXT NOT NULL DEFAULT 'general',
            created_at INTEGER NOT NULL DEFAULT 0
        )
        "#,
        [],
    )?;

    // Create channels table
    conn.execute(
        r#"
        CREATE TABLE IF NOT EXISTS channels (
            name TEXT PRIMARY KEY,
            description TEXT NOT NULL,
            created_by TEXT NOT NULL,
            created_at INTEGER NOT NULL
        )
        "#,
        [],
    )?;

    // Create channel_subscriptions table
    conn.execute(
        r#"
        CREATE TABLE IF NOT EXISTS channel_subscriptions (
            peer_id TEXT NOT NULL,
            channel_name TEXT NOT NULL,
            subscribed_at INTEGER NOT NULL,
            PRIMARY KEY (peer_id, channel_name),
            FOREIGN KEY (channel_name) REFERENCES channels(name)
        )
        "#,
        [],
    )?;

    // Create peer_name table (single row table for local peer name)
    conn.execute(
        r#"
        CREATE TABLE IF NOT EXISTS peer_name (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            name TEXT NOT NULL
        )
        "#,
        [],
    )?;

    // Add channel column to existing stories if it doesn't exist
    let add_channel_result = conn.execute(
        "ALTER TABLE stories ADD COLUMN channel TEXT DEFAULT 'general'",
        [],
    );
    // Ignore error if column already exists
    let _ = add_channel_result;

    // Add created_at column to existing stories if it doesn't exist
    let add_created_at_result = conn.execute(
        "ALTER TABLE stories ADD COLUMN created_at INTEGER DEFAULT 0",
        [],
    );
    // Ignore error if column already exists
    let _ = add_created_at_result;

    // Insert default 'general' channel if it doesn't exist
    conn.execute(
        r#"
        INSERT OR IGNORE INTO channels (name, description, created_by, created_at)
        VALUES ('general', 'Default general discussion channel', 'system', 0)
        "#,
        [],
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_create_tables_success() {
        let conn = Connection::open(":memory:").expect("Failed to create in-memory database");

        let result = create_tables(&conn);
        assert!(result.is_ok(), "create_tables should succeed");
    }

    #[test]
    fn test_stories_table_creation() {
        let conn = Connection::open(":memory:").expect("Failed to create in-memory database");

        create_tables(&conn).expect("Failed to create tables");

        // Check that stories table exists and has correct structure
        let mut stmt = conn
            .prepare("SELECT name, sql FROM sqlite_master WHERE type='table' AND name='stories'")
            .expect("Failed to prepare query");

        let table_info: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("Failed to execute query")
            .collect::<Result<Vec<_>, _>>()
            .expect("Failed to collect results");

        assert_eq!(table_info.len(), 1);
        assert_eq!(table_info[0].0, "stories");

        // Verify the table has expected columns
        let sql = &table_info[0].1;
        assert!(sql.contains("id INTEGER PRIMARY KEY"));
        assert!(sql.contains("name TEXT NOT NULL"));
        assert!(sql.contains("header TEXT NOT NULL"));
        assert!(sql.contains("body TEXT NOT NULL"));
        assert!(sql.contains("public BOOLEAN NOT NULL DEFAULT 0"));
        assert!(sql.contains("channel TEXT NOT NULL DEFAULT 'general'"));
        assert!(sql.contains("created_at INTEGER NOT NULL DEFAULT 0"));
    }

    #[test]
    fn test_channels_table_creation() {
        let conn = Connection::open(":memory:").expect("Failed to create in-memory database");

        create_tables(&conn).expect("Failed to create tables");

        // Check that channels table exists
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='channels'")
            .expect("Failed to prepare query");

        let table_exists: bool = stmt.exists([]).expect("Failed to check table existence");
        assert!(table_exists, "channels table should exist");

        // Verify default general channel was inserted
        let mut stmt = conn
            .prepare("SELECT name, description, created_by FROM channels WHERE name='general'")
            .expect("Failed to prepare query");

        let general_channel: Vec<(String, String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .expect("Failed to execute query")
            .collect::<Result<Vec<_>, _>>()
            .expect("Failed to collect results");

        assert_eq!(general_channel.len(), 1);
        assert_eq!(general_channel[0].0, "general");
        assert_eq!(general_channel[0].1, "Default general discussion channel");
        assert_eq!(general_channel[0].2, "system");
    }

    #[test]
    fn test_channel_subscriptions_table_creation() {
        let conn = Connection::open(":memory:").expect("Failed to create in-memory database");

        create_tables(&conn).expect("Failed to create tables");

        // Check that channel_subscriptions table exists with proper foreign key constraint
        let mut stmt = conn.prepare("SELECT name, sql FROM sqlite_master WHERE type='table' AND name='channel_subscriptions'")
            .expect("Failed to prepare query");

        let table_info: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("Failed to execute query")
            .collect::<Result<Vec<_>, _>>()
            .expect("Failed to collect results");

        assert_eq!(table_info.len(), 1);
        assert_eq!(table_info[0].0, "channel_subscriptions");

        let sql = &table_info[0].1;
        assert!(sql.contains("peer_id TEXT NOT NULL"));
        assert!(sql.contains("channel_name TEXT NOT NULL"));
        assert!(sql.contains("subscribed_at INTEGER NOT NULL"));
        assert!(sql.contains("PRIMARY KEY (peer_id, channel_name)"));
        assert!(sql.contains("FOREIGN KEY (channel_name) REFERENCES channels(name)"));
    }

    #[test]
    fn test_peer_name_table_creation() {
        let conn = Connection::open(":memory:").expect("Failed to create in-memory database");

        create_tables(&conn).expect("Failed to create tables");

        // Check that peer_name table exists
        let mut stmt = conn
            .prepare("SELECT name, sql FROM sqlite_master WHERE type='table' AND name='peer_name'")
            .expect("Failed to prepare query");

        let table_info: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("Failed to execute query")
            .collect::<Result<Vec<_>, _>>()
            .expect("Failed to collect results");

        assert_eq!(table_info.len(), 1);
        assert_eq!(table_info[0].0, "peer_name");

        let sql = &table_info[0].1;
        assert!(sql.contains("id INTEGER PRIMARY KEY CHECK (id = 1)"));
        assert!(sql.contains("name TEXT NOT NULL"));
    }

    #[test]
    fn test_create_tables_idempotency() {
        let conn = Connection::open(":memory:").expect("Failed to create in-memory database");

        // Create tables multiple times should not fail
        create_tables(&conn).expect("First call should succeed");
        create_tables(&conn).expect("Second call should succeed");
        create_tables(&conn).expect("Third call should succeed");

        // Verify tables still exist and have correct structure
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('stories', 'channels', 'channel_subscriptions', 'peer_name')")
            .expect("Failed to prepare query");

        let table_count: i64 = stmt
            .query_row([], |row| row.get(0))
            .expect("Failed to get table count");

        assert_eq!(table_count, 4, "All four tables should exist");

        // Verify general channel is still there and not duplicated
        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM channels WHERE name='general'")
            .expect("Failed to prepare query");

        let general_count: i64 = stmt
            .query_row([], |row| row.get(0))
            .expect("Failed to get general channel count");

        assert_eq!(general_count, 1, "Should have exactly one general channel");
    }

    #[test]
    fn test_alter_table_column_addition() {
        let conn = Connection::open(":memory:").expect("Failed to create in-memory database");

        // First create stories table without channel column (simulate old schema)
        conn.execute(
            r#"
            CREATE TABLE stories (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                header TEXT NOT NULL,
                body TEXT NOT NULL,
                public BOOLEAN NOT NULL DEFAULT 0
            )
            "#,
            [],
        )
        .expect("Failed to create stories table");

        // Now run create_tables which should add the channel column
        create_tables(&conn).expect("Failed to run create_tables");

        // Verify channel column was added
        let mut stmt = conn
            .prepare("PRAGMA table_info(stories)")
            .expect("Failed to prepare pragma query");

        let columns: Vec<String> = stmt
            .query_map([], |row| {
                let name: String = row.get(1)?;
                Ok(name)
            })
            .expect("Failed to execute query")
            .collect::<Result<Vec<_>, _>>()
            .expect("Failed to collect results");

        assert!(
            columns.contains(&"channel".to_string()),
            "Channel column should exist"
        );
        assert!(
            columns.contains(&"created_at".to_string()),
            "Created_at column should exist"
        );
    }
}
