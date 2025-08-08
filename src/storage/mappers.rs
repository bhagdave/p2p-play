use super::utils::{db_bool_to_rust, get_optional_string_with_default, get_timestamp_with_default};
/// Result mapping functions for converting database rows to structs
use crate::types::{Channel, ChannelSubscription, Story, StoryReadStatus};
use rusqlite::Row;

/// Map a database row to a Story struct
/// Standard mapping for: id, name, header, body, public, channel, created_at
pub fn map_row_to_story(row: &Row) -> Result<Story, rusqlite::Error> {
    Ok(Story {
        id: row.get::<_, i64>(0)? as usize,
        name: row.get(1)?,
        header: row.get(2)?,
        body: row.get(3)?,
        public: db_bool_to_rust(row.get::<_, i64>(4)?),
        channel: get_optional_string_with_default(row, 5, "general")?,
        created_at: get_timestamp_with_default(row, 6)?,
    })
}

/// Map a database row to a Channel struct
/// Standard mapping for: name, description, created_by, created_at
pub fn map_row_to_channel(row: &Row) -> Result<Channel, rusqlite::Error> {
    Ok(Channel {
        name: row.get(0)?,
        description: row.get(1)?,
        created_by: row.get(2)?,
        created_at: row.get::<_, i64>(3)? as u64,
    })
}

/// Map a database row to a ChannelSubscription struct  
/// Standard mapping for: peer_id, channel_name, subscribed_at
pub fn map_row_to_channel_subscription(row: &Row) -> Result<ChannelSubscription, rusqlite::Error> {
    Ok(ChannelSubscription {
        peer_id: row.get(0)?,
        channel_name: row.get(1)?,
        subscribed_at: row.get::<_, i64>(2)? as u64,
    })
}

/// Map a database row to a StoryReadStatus struct
/// Standard mapping for: story_id, peer_id, read_at, channel_name
pub fn map_row_to_story_read_status(row: &Row) -> Result<StoryReadStatus, rusqlite::Error> {
    Ok(StoryReadStatus {
        story_id: row.get::<_, i64>(0)? as usize,
        peer_id: row.get(1)?,
        read_at: row.get::<_, i64>(2)? as u64,
        channel_name: row.get(3)?,
    })
}

/// Map a simple string result from database
pub fn map_row_to_string(row: &Row) -> Result<String, rusqlite::Error> {
    row.get(0)
}

/// Map a simple integer result from database  
pub fn map_row_to_i64(row: &Row) -> Result<i64, rusqlite::Error> {
    row.get(0)
}

/// Map a simple usize result from database
pub fn map_row_to_usize(row: &Row) -> Result<usize, rusqlite::Error> {
    Ok(row.get::<_, i64>(0)? as usize)
}

/// Map a row to a tuple of (String, usize) for unread counts
pub fn map_row_to_channel_unread_count(row: &Row) -> Result<(String, usize), rusqlite::Error> {
    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_bool_conversion_in_mapping() {
        let conn = Connection::open_in_memory().unwrap();

        conn.execute(
            "CREATE TABLE test_stories (
                id INTEGER, name TEXT, header TEXT, body TEXT, 
                public INTEGER, channel TEXT, created_at INTEGER
            )",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO test_stories VALUES (1, 'test', 'header', 'body', 1, 'general', 1000)",
            [],
        )
        .unwrap();

        let mut stmt = conn
            .prepare("SELECT id, name, header, body, public, channel, created_at FROM test_stories")
            .unwrap();
        let story_result = stmt.query_row([], map_row_to_story).unwrap();

        assert_eq!(story_result.id, 1);
        assert_eq!(story_result.name, "test");
        assert!(story_result.public);
        assert_eq!(story_result.channel, "general");
        assert_eq!(story_result.created_at, 1000);
    }
}
