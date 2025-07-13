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
            public BOOLEAN NOT NULL DEFAULT 0
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

    Ok(())
}
