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
            channel TEXT NOT NULL DEFAULT 'general'
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
