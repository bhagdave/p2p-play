use crate::types::{
    BootstrapConfig, Channel, ChannelSubscription, ChannelSubscriptions, Channels,
    DirectMessageConfig, Stories, Story,
};
use log::{debug, error};
use rusqlite::Connection;
use std::error::Error;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::{Mutex, RwLock};

use crate::migrations;

const PEER_NAME_FILE_PATH: &str = "./peer_name.json"; // Keep for backward compatibility
pub const NODE_DESCRIPTION_FILE_PATH: &str = "node_description.txt";

/// Get the database path, checking environment variables for custom paths
fn get_database_path() -> String {
    // Check for test database path first (for integration tests)
    if let Ok(test_path) = std::env::var("TEST_DATABASE_PATH") {
        return test_path;
    }

    // Check for custom database path
    if let Ok(db_path) = std::env::var("DATABASE_PATH") {
        return db_path;
    }

    // Default production path
    "./stories.db".to_string()
}

// Thread-safe database connection storage with support for dynamic paths
static DB_STATE: once_cell::sync::Lazy<RwLock<Option<(Arc<Mutex<Connection>>, String)>>> =
    once_cell::sync::Lazy::new(|| RwLock::new(None));

pub async fn get_db_connection() -> Result<Arc<Mutex<Connection>>, Box<dyn Error>> {
    let current_path = get_database_path();

    // Check if we have an existing connection with the same path
    {
        let state = DB_STATE.read().await;
        if let Some((conn, stored_path)) = state.as_ref() {
            if stored_path == &current_path {
                return Ok(conn.clone());
            }
        }
    }

    // Need to create or update connection
    debug!("Creating new SQLite database connection: {}", current_path);
    let conn = Connection::open(&current_path)?;
    let conn_arc = Arc::new(Mutex::new(conn));

    // Update the stored connection and path
    {
        let mut state = DB_STATE.write().await;
        *state = Some((conn_arc.clone(), current_path));
    }

    debug!("Successfully connected to SQLite database");
    Ok(conn_arc)
}

/// Reset database connection (useful for testing)
pub async fn reset_db_connection_for_testing() -> Result<(), Box<dyn Error>> {
    let mut state = DB_STATE.write().await;
    *state = None;
    Ok(())
}

async fn create_tables() -> Result<(), Box<dyn Error>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    migrations::create_tables(&conn)?;
    Ok(())
}

pub async fn ensure_stories_file_exists() -> Result<(), Box<dyn Error>> {
    let db_path = get_database_path();
    debug!("Initializing SQLite database at: {}", db_path);

    // Ensure the directory exists for the database file
    if let Some(parent) = std::path::Path::new(&db_path).parent() {
        if !parent.exists() {
            debug!("Creating directory: {:?}", parent);
            tokio::fs::create_dir_all(parent).await?;
        }
    }

    let _conn = get_db_connection().await?;
    create_tables().await?;
    debug!("SQLite database and tables initialized");
    Ok(())
}

pub async fn read_local_stories() -> Result<Stories, Box<dyn Error>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let mut stmt =
        conn.prepare("SELECT id, name, header, body, public, channel, created_at FROM stories ORDER BY created_at DESC")?;
    let story_iter = stmt.query_map([], |row| {
        Ok(Story {
            id: row.get::<_, i64>(0)? as usize,
            name: row.get(1)?,
            header: row.get(2)?,
            body: row.get(3)?,
            public: row.get::<_, i64>(4)? != 0, // Convert integer to boolean
            channel: row
                .get::<_, Option<String>>(5)?
                .unwrap_or_else(|| "general".to_string()),
            created_at: row.get::<_, i64>(6).unwrap_or(0) as u64,
        })
    })?;

    let mut stories = Vec::new();
    for story in story_iter {
        stories.push(story?);
    }

    Ok(stories)
}

pub async fn read_local_stories_from_path(path: &str) -> Result<Stories, Box<dyn Error>> {
    // For test compatibility, if path points to a JSON file, read it as JSON
    if path.ends_with(".json") {
        let content = fs::read(path).await?;
        let result = serde_json::from_slice(&content)?;
        Ok(result)
    } else {
        // Treat as SQLite database path - create a temporary connection
        let conn = Connection::open(path)?;

        let mut stmt = conn
            .prepare("SELECT id, name, header, body, public, channel, created_at FROM stories ORDER BY created_at DESC")?;
        let story_iter = stmt.query_map([], |row| {
            Ok(Story {
                id: row.get::<_, i64>(0)? as usize,
                name: row.get(1)?,
                header: row.get(2)?,
                body: row.get(3)?,
                public: row.get::<_, i64>(4)? != 0, // Convert integer to boolean
                channel: row
                    .get::<_, Option<String>>(5)?
                    .unwrap_or_else(|| "general".to_string()),
                created_at: row.get::<_, i64>(6).unwrap_or(0) as u64,
            })
        })?;

        let mut stories = Vec::new();
        for story in story_iter {
            stories.push(story?);
        }

        Ok(stories)
    }
}

pub async fn write_local_stories(stories: &Stories) -> Result<(), Box<dyn Error>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    // Clear existing stories and insert new ones
    conn.execute("DELETE FROM stories", [])?;

    for story in stories {
        conn.execute(
            "INSERT INTO stories (id, name, header, body, public, channel, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
            [
                &story.id.to_string(),
                &story.name,
                &story.header,
                &story.body,
                &(if story.public {
                    "1".to_string()
                } else {
                    "0".to_string()
                }),
                &story.channel,
                &story.created_at.to_string(),
            ],
        )?;
    }

    Ok(())
}

pub async fn write_local_stories_to_path(
    stories: &Stories,
    path: &str,
) -> Result<(), Box<dyn Error>> {
    // For test compatibility, if path is a JSON file, write as JSON
    if path.ends_with(".json") {
        let json = serde_json::to_string(&stories)?;
        fs::write(path, &json).await?;
        Ok(())
    } else {
        // Treat as SQLite database path - create a temporary connection
        let conn = Connection::open(path)?;

        // Create tables if they don't exist
        migrations::create_tables(&conn)?;

        // Clear existing stories and insert new ones
        conn.execute("DELETE FROM stories", [])?;

        for story in stories {
            conn.execute(
                "INSERT INTO stories (id, name, header, body, public, channel, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
                [
                    &story.id.to_string(),
                    &story.name,
                    &story.header,
                    &story.body,
                    &(if story.public {
                        "1".to_string()
                    } else {
                        "0".to_string()
                    }),
                    &story.channel,
                    &story.created_at.to_string(),
                ],
            )?;
        }

        Ok(())
    }
}

pub async fn create_new_story(name: &str, header: &str, body: &str) -> Result<(), Box<dyn Error>> {
    create_new_story_with_channel(name, header, body, "general").await
}

pub async fn create_new_story_with_channel(
    name: &str,
    header: &str,
    body: &str,
    channel: &str,
) -> Result<(), Box<dyn Error>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    // Get the next ID
    let mut stmt = conn.prepare("SELECT COALESCE(MAX(id), -1) + 1 as next_id FROM stories")?;
    let next_id: i64 = stmt.query_row([], |row| row.get(0))?;

    // Get the current timestamp
    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Insert the new story
    conn.execute(
        "INSERT INTO stories (id, name, header, body, public, channel, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        [
            &next_id.to_string(),
            name,
            header,
            body,
            "0", // New stories start as private (0 = false)
            channel,
            &created_at.to_string(),
        ],
    )?;

    debug!("Created story:");
    debug!("Name: {}", name);
    debug!("Header: {}", header);
    debug!("Body: {}", body);

    Ok(())
}

pub async fn create_new_story_in_path(
    name: &str,
    header: &str,
    body: &str,
    path: &str,
) -> Result<usize, Box<dyn Error>> {
    let mut local_stories = match read_local_stories_from_path(path).await {
        Ok(stories) => stories,
        Err(_) => Vec::new(),
    };
    let new_id = match local_stories.iter().max_by_key(|r| r.id) {
        Some(v) => v.id + 1,
        None => 0,
    };
    local_stories.push(Story {
        id: new_id,
        name: name.to_owned(),
        header: header.to_owned(),
        body: body.to_owned(),
        public: false,
        channel: "general".to_string(),
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    });
    write_local_stories_to_path(&local_stories, path).await?;
    Ok(new_id)
}

pub async fn publish_story(
    id: usize,
    sender: tokio::sync::mpsc::UnboundedSender<Story>,
) -> Result<(), Box<dyn Error>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    // Update the story to be public
    let rows_affected = conn.execute(
        "UPDATE stories SET public = ? WHERE id = ?",
        [&"1".to_string(), &id.to_string()], // 1 = true
    )?;

    if rows_affected > 0 {
        // Fetch the updated story to send it
        let mut stmt = conn.prepare(
            "SELECT id, name, header, body, public, channel, created_at FROM stories WHERE id = ?",
        )?;
        let story_result = stmt.query_row([&id.to_string()], |row| {
            Ok(Story {
                id: row.get::<_, i64>(0)? as usize,
                name: row.get(1)?,
                header: row.get(2)?,
                body: row.get(3)?,
                public: row.get::<_, i64>(4)? != 0, // Convert integer to boolean
                channel: row
                    .get::<_, Option<String>>(5)?
                    .unwrap_or_else(|| "general".to_string()),
                created_at: row.get::<_, i64>(6).unwrap_or(0) as u64,
            })
        });

        if let Ok(story) = story_result {
            if let Err(e) = sender.send(story) {
                error!("error sending story for broadcast: {}", e);
            }
        }
    }

    Ok(())
}

pub async fn delete_local_story(id: usize) -> Result<bool, Box<dyn Error>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    // Delete the story with the given ID
    let rows_affected = conn.execute("DELETE FROM stories WHERE id = ?", [&id.to_string()])?;

    if rows_affected > 0 {
        debug!("Deleted story with ID: {}", id);
        Ok(true)
    } else {
        debug!("No story found with ID: {}", id);
        Ok(false)
    }
}

pub async fn publish_story_in_path(id: usize, path: &str) -> Result<Option<Story>, Box<dyn Error>> {
    let mut local_stories = read_local_stories_from_path(path).await?;
    let mut published_story = None;

    for story in local_stories.iter_mut() {
        if story.id == id {
            story.public = true;
            published_story = Some(story.clone());
            break;
        }
    }

    write_local_stories_to_path(&local_stories, path).await?;
    Ok(published_story)
}

pub async fn save_received_story(story: Story) -> Result<(), Box<dyn Error>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    // Check if story already exists (by name and content to avoid duplicates)
    let mut stmt =
        conn.prepare("SELECT id FROM stories WHERE name = ? AND header = ? AND body = ?")?;
    let existing = stmt.query_row([&story.name, &story.header, &story.body], |row| {
        row.get::<_, i64>(0)
    });

    if existing.is_err() {
        // Story doesn't exist
        // Get the next ID
        let mut stmt = conn.prepare("SELECT COALESCE(MAX(id), -1) + 1 as next_id FROM stories")?;
        let new_id: i64 = stmt.query_row([], |row| row.get(0))?;

        // Insert the story with the new ID and mark as public
        conn.execute(
            "INSERT INTO stories (id, name, header, body, public, channel, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
            [
                &new_id.to_string(),
                &story.name,
                &story.header,
                &story.body,
                "1", // Mark as public since it was published (1 = true)
                &story.channel,
                &story.created_at.to_string(),
            ],
        )?;

        debug!("Saved received story to local storage with ID: {}", new_id);
    } else {
        debug!("Story already exists locally, skipping save");
    }

    Ok(())
}

pub async fn save_received_story_to_path(
    mut story: Story,
    path: &str,
) -> Result<usize, Box<dyn Error>> {
    let mut local_stories = match read_local_stories_from_path(path).await {
        Ok(stories) => stories,
        Err(_) => Vec::new(),
    };

    // Check if story already exists
    let already_exists = local_stories
        .iter()
        .any(|s| s.name == story.name && s.header == story.header && s.body == story.body);

    if !already_exists {
        let new_id = match local_stories.iter().max_by_key(|r| r.id) {
            Some(v) => v.id + 1,
            None => 0,
        };
        story.id = new_id;
        story.public = true;
        // Set created_at if not already set
        if story.created_at == 0 {
            story.created_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
        }

        local_stories.push(story);
        write_local_stories_to_path(&local_stories, path).await?;
        Ok(new_id)
    } else {
        // Return the existing story's ID
        let existing = local_stories
            .iter()
            .find(|s| s.name == story.name && s.header == story.header && s.body == story.body)
            .unwrap();
        Ok(existing.id)
    }
}

pub async fn save_local_peer_name(name: &str) -> Result<(), Box<dyn Error>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    // Insert or replace the peer name (there should only be one row)
    conn.execute(
        "INSERT OR REPLACE INTO peer_name (id, name) VALUES (1, ?)",
        [name],
    )?;

    Ok(())
}

pub async fn save_local_peer_name_to_path(name: &str, path: &str) -> Result<(), Box<dyn Error>> {
    // For test compatibility, keep writing to JSON file
    let json = serde_json::to_string(name)?;
    fs::write(path, &json).await?;
    Ok(())
}

pub async fn load_local_peer_name() -> Result<Option<String>, Box<dyn Error>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let mut stmt = conn.prepare("SELECT name FROM peer_name WHERE id = 1")?;
    let result = stmt.query_row([], |row| row.get::<_, String>(0));

    match result {
        Ok(name) => Ok(Some(name)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(Box::new(e)),
    }
}

pub async fn load_local_peer_name_from_path(path: &str) -> Result<Option<String>, Box<dyn Error>> {
    // For test compatibility, keep reading from JSON file
    match fs::read(path).await {
        Ok(content) => {
            let name: String = serde_json::from_slice(&content)?;
            Ok(Some(name))
        }
        Err(_) => Ok(None), // File doesn't exist or can't be read, return None
    }
}

// Channel management functions
pub async fn create_channel(
    name: &str,
    description: &str,
    created_by: &str,
) -> Result<(), Box<dyn Error>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    conn.execute(
        "INSERT OR IGNORE INTO channels (name, description, created_by, created_at) VALUES (?, ?, ?, ?)",
        [name, description, created_by, &timestamp.to_string()],
    )?;

    debug!("Created channel: {} - {}", name, description);
    Ok(())
}

pub async fn read_channels() -> Result<Channels, Box<dyn Error>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let mut stmt = conn
        .prepare("SELECT name, description, created_by, created_at FROM channels ORDER BY name")?;
    let channel_iter = stmt.query_map([], |row| {
        Ok(Channel {
            name: row.get(0)?,
            description: row.get(1)?,
            created_by: row.get(2)?,
            created_at: row.get::<_, i64>(3)? as u64,
        })
    })?;

    let mut channels = Vec::new();
    for channel in channel_iter {
        channels.push(channel?);
    }

    Ok(channels)
}

pub async fn subscribe_to_channel(peer_id: &str, channel_name: &str) -> Result<(), Box<dyn Error>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    conn.execute(
        "INSERT OR REPLACE INTO channel_subscriptions (peer_id, channel_name, subscribed_at) VALUES (?, ?, ?)",
        [peer_id, channel_name, &timestamp.to_string()],
    )?;

    debug!("Subscribed {} to channel: {}", peer_id, channel_name);
    Ok(())
}

pub async fn unsubscribe_from_channel(
    peer_id: &str,
    channel_name: &str,
) -> Result<(), Box<dyn Error>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    conn.execute(
        "DELETE FROM channel_subscriptions WHERE peer_id = ? AND channel_name = ?",
        [peer_id, channel_name],
    )?;

    debug!("Unsubscribed {} from channel: {}", peer_id, channel_name);
    Ok(())
}

pub async fn read_subscribed_channels(peer_id: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let mut stmt = conn.prepare(
        "SELECT channel_name FROM channel_subscriptions WHERE peer_id = ? ORDER BY channel_name",
    )?;
    let channel_iter = stmt.query_map([peer_id], |row| row.get::<_, String>(0))?;

    let mut channels = Vec::new();
    for channel in channel_iter {
        channels.push(channel?);
    }

    Ok(channels)
}

pub async fn read_channel_subscriptions() -> Result<ChannelSubscriptions, Box<dyn Error>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let mut stmt = conn.prepare("SELECT peer_id, channel_name, subscribed_at FROM channel_subscriptions ORDER BY channel_name, peer_id")?;
    let subscription_iter = stmt.query_map([], |row| {
        Ok(ChannelSubscription {
            peer_id: row.get(0)?,
            channel_name: row.get(1)?,
            subscribed_at: row.get::<_, i64>(2)? as u64,
        })
    })?;

    let mut subscriptions = Vec::new();
    for subscription in subscription_iter {
        subscriptions.push(subscription?);
    }

    Ok(subscriptions)
}

pub async fn get_stories_by_channel(channel_name: &str) -> Result<Stories, Box<dyn Error>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let mut stmt = conn.prepare("SELECT id, name, header, body, public, channel, created_at FROM stories WHERE channel = ? AND public = 1 ORDER BY created_at DESC")?;
    let story_iter = stmt.query_map([channel_name], |row| {
        Ok(Story {
            id: row.get::<_, i64>(0)? as usize,
            name: row.get(1)?,
            header: row.get(2)?,
            body: row.get(3)?,
            public: row.get::<_, i64>(4)? != 0,
            channel: row.get(5)?,
            created_at: row.get::<_, i64>(6).unwrap_or(0) as u64,
        })
    })?;

    let mut stories = Vec::new();
    for story in story_iter {
        stories.push(story?);
    }

    Ok(stories)
}

/// Clears all data from the database and ensures fresh test database (useful for testing)
pub async fn clear_database_for_testing() -> Result<(), Box<dyn Error>> {
    // Reset the connection to ensure we're using the test database path
    reset_db_connection_for_testing().await?;

    // Ensure the test database is initialized
    ensure_stories_file_exists().await?;

    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    // Clear all tables
    conn.execute("DELETE FROM channel_subscriptions", [])?;
    conn.execute("DELETE FROM stories", [])?;
    conn.execute("DELETE FROM channels WHERE name != 'general'", [])?; // Keep general channel
    conn.execute("DELETE FROM peer_name", [])?;

    debug!("Test database cleared and reset");
    Ok(())
}

/// Save node description to file (limited to 1024 bytes)
pub async fn save_node_description(description: &str) -> Result<(), Box<dyn Error>> {
    if description.len() > 1024 {
        return Err("Description exceeds 1024 bytes limit".into());
    }

    fs::write(NODE_DESCRIPTION_FILE_PATH, description).await?;
    Ok(())
}

/// Load local node description from file
pub async fn load_node_description() -> Result<Option<String>, Box<dyn Error>> {
    match fs::read_to_string(NODE_DESCRIPTION_FILE_PATH).await {
        Ok(content) => {
            if content.is_empty() {
                Ok(None)
            } else {
                // Ensure it doesn't exceed the limit even if file was modified externally
                if content.len() > 1024 {
                    return Err("Description file exceeds 1024 bytes limit".into());
                }
                Ok(Some(content))
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Save bootstrap configuration to file
pub async fn save_bootstrap_config(config: &BootstrapConfig) -> Result<(), Box<dyn Error>> {
    save_bootstrap_config_to_path(config, "bootstrap_config.json").await
}

/// Save bootstrap configuration to specific path
pub async fn save_bootstrap_config_to_path(
    config: &BootstrapConfig,
    path: &str,
) -> Result<(), Box<dyn Error>> {
    // Validate the config before saving
    config.validate()?;

    let json = serde_json::to_string_pretty(config)?;
    fs::write(path, json).await?;
    debug!(
        "Bootstrap config saved with {} peers",
        config.bootstrap_peers.len()
    );
    Ok(())
}

/// Load bootstrap configuration from file, creating default if missing
pub async fn load_bootstrap_config() -> Result<BootstrapConfig, Box<dyn Error>> {
    load_bootstrap_config_from_path("bootstrap_config.json").await
}

/// Load bootstrap configuration from specific path, creating default if missing
pub async fn load_bootstrap_config_from_path(
    path: &str,
) -> Result<BootstrapConfig, Box<dyn Error>> {
    match fs::read_to_string(path).await {
        Ok(content) => {
            let config: BootstrapConfig = serde_json::from_str(&content)?;

            // Validate the loaded config
            config.validate()?;

            debug!(
                "Loaded bootstrap config with {} peers",
                config.bootstrap_peers.len()
            );
            Ok(config)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            debug!("No bootstrap config file found, creating default");
            let default_config = BootstrapConfig::default();

            // Save the default config for future use
            save_bootstrap_config_to_path(&default_config, path).await?;

            Ok(default_config)
        }
        Err(e) => Err(e.into()),
    }
}

/// Ensure bootstrap config file exists with defaults
pub async fn ensure_bootstrap_config_exists() -> Result<(), Box<dyn Error>> {
    if tokio::fs::metadata("bootstrap_config.json").await.is_err() {
        let default_config = BootstrapConfig::default();
        save_bootstrap_config(&default_config).await?;
        debug!("Created default bootstrap config file");
    }
    Ok(())
}

/// Save direct message configuration to file
pub async fn save_direct_message_config(
    config: &DirectMessageConfig,
) -> Result<(), Box<dyn Error>> {
    save_direct_message_config_to_path(config, "direct_message_config.json").await
}

/// Save direct message configuration to specific path
pub async fn save_direct_message_config_to_path(
    config: &DirectMessageConfig,
    path: &str,
) -> Result<(), Box<dyn Error>> {
    let json = serde_json::to_string_pretty(config)?;
    fs::write(path, json).await?;
    debug!("Saved direct message config to {}", path);
    Ok(())
}

/// Load direct message configuration from file, creating default if missing
pub async fn load_direct_message_config() -> Result<DirectMessageConfig, Box<dyn Error>> {
    load_direct_message_config_from_path("direct_message_config.json").await
}

/// Load direct message configuration from specific path, creating default if missing
pub async fn load_direct_message_config_from_path(
    path: &str,
) -> Result<DirectMessageConfig, Box<dyn Error>> {
    match fs::read_to_string(path).await {
        Ok(content) => {
            let config: DirectMessageConfig = serde_json::from_str(&content)?;

            // Validate the loaded config
            config.validate()?;

            debug!("Loaded direct message config from {}", path);
            Ok(config)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            debug!("No direct message config file found, creating default");
            let default_config = DirectMessageConfig::default();

            // Save the default config for future use
            save_direct_message_config_to_path(&default_config, path).await?;

            Ok(default_config)
        }
        Err(e) => Err(e.into()),
    }
}

/// Ensure direct message config file exists with defaults
pub async fn ensure_direct_message_config_exists() -> Result<(), Box<dyn Error>> {
    if tokio::fs::metadata("direct_message_config.json")
        .await
        .is_err()
    {
        let default_config = DirectMessageConfig::default();
        save_direct_message_config(&default_config).await?;
        debug!("Created default direct message config file");
    }
    Ok(())
}
