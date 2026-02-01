use super::utils::{db_bool_to_rust, get_optional_string_with_default, get_timestamp_with_default};
use crate::types::{Channel, Story, WasmOffering, WasmParameter, WasmResourceRequirements};
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

/// Map a database row to a WasmOffering struct
/// Expected columns: id, name, description, ipfs_cid, parameters_json, resource_requirements_json, version, enabled, created_at, updated_at
pub fn map_row_to_wasm_offering(row: &Row) -> Result<WasmOffering, rusqlite::Error> {
    let parameters_json: String = row.get(4)?;
    let resource_requirements_json: String = row.get(5)?;

    // deserialize parameters and throw error if the json is invalid
    let parameters: Vec<WasmParameter> = serde_json::from_str(&parameters_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            parameters_json.len(),
            rusqlite::types::Type::Text,
            Box::new(e),
        )
    })?;
    
    let resource_requirements: WasmResourceRequirements =
        serde_json::from_str(&resource_requirements_json).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                resource_requirements_json.len(),
                rusqlite::types::Type::Text,
                Box::new(e),
            )
        })?;

    Ok(WasmOffering {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        ipfs_cid: row.get(3)?,
        parameters,
        resource_requirements,
        version: row.get(6)?,
        enabled: db_bool_to_rust(row.get::<_, i64>(7)?),
        created_at: row.get::<_, i64>(8)? as u64,
        updated_at: row.get::<_, i64>(9)? as u64,
    })
}

/// Map a database row to a WasmOffering from the discovered_wasm_offerings table
/// Expected columns: id, peer_id, name, description, ipfs_cid, parameters_json, resource_requirements_json, version, discovered_at, last_seen_at
pub fn map_row_to_discovered_wasm_offering(
    row: &Row,
) -> Result<(String, WasmOffering), rusqlite::Error> {
    let peer_id: String = row.get(1)?;
    let parameters_json: String = row.get(5)?;
    let resource_requirements_json: String = row.get(6)?;

    let parameters: Vec<WasmParameter> = serde_json::from_str(&parameters_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            parameters_json.len(),
            rusqlite::types::Type::Text,
            Box::new(e),
        )
    })?;
    let resource_requirements: WasmResourceRequirements =
        serde_json::from_str(&resource_requirements_json).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                resource_requirements_json.len(),
                rusqlite::types::Type::Text,
                Box::new(e),
            )
        })?;

    let offering = WasmOffering {
        id: row.get(0)?,
        name: row.get(2)?,
        description: row.get(3)?,
        ipfs_cid: row.get(4)?,
        parameters,
        resource_requirements,
        version: row.get(7)?,
        enabled: true, // Discovered offerings are always considered enabled
        created_at: row.get::<_, i64>(8)? as u64,   // discovered_at
        updated_at: row.get::<_, i64>(9)? as u64,   // last_seen_at
    };

    Ok((peer_id, offering))
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
