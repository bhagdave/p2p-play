use super::utils::{db_bool_to_rust, get_optional_string_with_default, get_timestamp_with_default};
use crate::types::{Channel, Story};
use rusqlite::Row;

pub fn map_row_to_story(row: &Row) -> Result<Story, rusqlite::Error> {
    Ok(Story {
        id: row.get::<_, i64>(0)? as usize,
        name: row.get(1)?,
        header: row.get(2)?,
        body: row.get(3)?,
        public: db_bool_to_rust(row.get::<_, i64>(4)?),
        channel: get_optional_string_with_default(row, 5, "general")?,
        created_at: get_timestamp_with_default(row, 6)?,
        auto_share: None, // Default for backwards compatibility, to be updated when DB schema is migrated
    })
}

pub fn map_row_to_channel(row: &Row) -> Result<Channel, rusqlite::Error> {
    Ok(Channel {
        name: row.get(0)?,
        description: row.get(1)?,
        created_by: row.get(2)?,
        created_at: row.get::<_, i64>(3)? as u64,
    })
}

pub fn map_row_to_i64(row: &Row) -> Result<i64, rusqlite::Error> {
    row.get(0)
}

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
