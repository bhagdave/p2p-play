use crate::types::{Channel, ChannelSubscription, ChannelSubscriptions, Channels, Stories, Story};
use log::{error, info};
use rusqlite::Connection;
use std::error::Error;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::{Mutex, RwLock};

use crate::migrations;

const PEER_NAME_FILE_PATH: &str = "./peer_name.json"; // Keep for backward compatibility

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

async fn get_db_connection() -> Result<Arc<Mutex<Connection>>, Box<dyn Error>> {
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
    info!("Creating new SQLite database connection: {}", current_path);
    let conn = Connection::open(&current_path)?;
    let conn_arc = Arc::new(Mutex::new(conn));

    // Update the stored connection and path
    {
        let mut state = DB_STATE.write().await;
        *state = Some((conn_arc.clone(), current_path));
    }

    info!("Successfully connected to SQLite database");
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
    info!("Initializing SQLite database at: {}", db_path);

    // Ensure the directory exists for the database file
    if let Some(parent) = std::path::Path::new(&db_path).parent() {
        if !parent.exists() {
            info!("Creating directory: {:?}", parent);
            tokio::fs::create_dir_all(parent).await?;
        }
    }

    let _conn = get_db_connection().await?;
    create_tables().await?;
    info!("SQLite database and tables initialized");
    Ok(())
}

pub async fn read_local_stories() -> Result<Stories, Box<dyn Error>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let mut stmt =
        conn.prepare("SELECT id, name, header, body, public, channel FROM stories ORDER BY id")?;
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
            .prepare("SELECT id, name, header, body, public, channel FROM stories ORDER BY id")?;
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
            "INSERT INTO stories (id, name, header, body, public, channel) VALUES (?, ?, ?, ?, ?, ?)",
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
                "INSERT INTO stories (id, name, header, body, public, channel) VALUES (?, ?, ?, ?, ?, ?)",
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

    // Insert the new story
    conn.execute(
        "INSERT INTO stories (id, name, header, body, public, channel) VALUES (?, ?, ?, ?, ?, ?)",
        [
            &next_id.to_string(),
            name,
            header,
            body,
            "0", // New stories start as private (0 = false)
            channel,
        ],
    )?;

    info!("Created story:");
    info!("Name: {}", name);
    info!("Header: {}", header);
    info!("Body: {}", body);

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
        let mut stmt = conn
            .prepare("SELECT id, name, header, body, public, channel FROM stories WHERE id = ?")?;
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
    let rows_affected = conn.execute(
        "DELETE FROM stories WHERE id = ?",
        [&id.to_string()],
    )?;

    if rows_affected > 0 {
        info!("Deleted story with ID: {}", id);
        Ok(true)
    } else {
        info!("No story found with ID: {}", id);
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
            "INSERT INTO stories (id, name, header, body, public, channel) VALUES (?, ?, ?, ?, ?, ?)",
            [
                &new_id.to_string(),
                &story.name,
                &story.header,
                &story.body,
                "1", // Mark as public since it was published (1 = true)
                &story.channel,
            ],
        )?;

        info!("Saved received story to local storage with ID: {}", new_id);
    } else {
        info!("Story already exists locally, skipping save");
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

    info!("Created channel: {} - {}", name, description);
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

    info!("Subscribed {} to channel: {}", peer_id, channel_name);
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

    info!("Unsubscribed {} from channel: {}", peer_id, channel_name);
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

    let mut stmt = conn.prepare("SELECT id, name, header, body, public, channel FROM stories WHERE channel = ? AND public = 1 ORDER BY id")?;
    let story_iter = stmt.query_map([channel_name], |row| {
        Ok(Story {
            id: row.get::<_, i64>(0)? as usize,
            name: row.get(1)?,
            header: row.get(2)?,
            body: row.get(3)?,
            public: row.get::<_, i64>(4)? != 0,
            channel: row.get(5)?,
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

    info!("Test database cleared and reset");
    Ok(())
}

/// Save node description to file (limited to 1024 bytes)
pub async fn save_node_description(description: &str) -> Result<(), Box<dyn Error>> {
    if description.len() > 1024 {
        return Err("Description exceeds 1024 bytes limit".into());
    }
    
    fs::write("node_description.txt", description).await?;
    Ok(())
}

/// Load local node description from file
pub async fn load_node_description() -> Result<Option<String>, Box<dyn Error>> {
    match fs::read_to_string("node_description.txt").await {
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use tokio::sync::mpsc;

    async fn create_temp_stories_file() -> NamedTempFile {
        let temp_file = NamedTempFile::new().unwrap();
        let initial_stories = vec![Story {
            id: 0,
            name: "Initial Story".to_string(),
            header: "Initial Header".to_string(),
            body: "Initial Body".to_string(),
            public: false,
            channel: "general".to_string(),
        }];
        write_local_stories_to_path(&initial_stories, temp_file.path().to_str().unwrap())
            .await
            .unwrap();
        temp_file
    }

    #[tokio::test]
    async fn test_write_and_read_stories() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        let stories = vec![
            Story {
                id: 1,
                name: "Test Story".to_string(),
                header: "Test Header".to_string(),
                body: "Test Body".to_string(),
                public: true,
                channel: "general".to_string(),
            },
            Story {
                id: 2,
                name: "Another Story".to_string(),
                header: "Another Header".to_string(),
                body: "Another Body".to_string(),
                public: false,
                channel: "tech".to_string(),
            },
        ];

        write_local_stories_to_path(&stories, path).await.unwrap();
        let read_stories = read_local_stories_from_path(path).await.unwrap();

        assert_eq!(stories, read_stories);
    }

    #[tokio::test]
    async fn test_read_nonexistent_file() {
        let result = read_local_stories_from_path("/nonexistent/path").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_new_story() {
        let temp_file = create_temp_stories_file().await;
        let path = temp_file.path().to_str().unwrap();

        let new_id = create_new_story_in_path("New Story", "New Header", "New Body", path)
            .await
            .unwrap();
        let stories = read_local_stories_from_path(path).await.unwrap();

        assert_eq!(stories.len(), 2);
        assert_eq!(new_id, 1); // Should be next ID after 0

        let new_story = stories.iter().find(|s| s.id == new_id).unwrap();
        assert_eq!(new_story.name, "New Story");
        assert_eq!(new_story.header, "New Header");
        assert_eq!(new_story.body, "New Body");
        assert!(!new_story.public); // Should start as private
    }

    #[tokio::test]
    async fn test_create_story_in_empty_file() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        let new_id = create_new_story_in_path("First Story", "Header", "Body", path)
            .await
            .unwrap();
        let stories = read_local_stories_from_path(path).await.unwrap();

        assert_eq!(stories.len(), 1);
        assert_eq!(new_id, 0); // First story should have ID 0
        assert_eq!(stories[0].name, "First Story");
    }

    #[tokio::test]
    async fn test_publish_story() {
        let temp_file = create_temp_stories_file().await;
        let path = temp_file.path().to_str().unwrap();

        // Create a new story first
        let story_id = create_new_story_in_path("Test Publish", "Header", "Body", path)
            .await
            .unwrap();

        // Publish it
        let published_story = publish_story_in_path(story_id, path).await.unwrap();
        assert!(published_story.is_some());

        // Verify it's now public
        let stories = read_local_stories_from_path(path).await.unwrap();
        let published = stories.iter().find(|s| s.id == story_id).unwrap();
        assert!(published.public);
    }

    #[tokio::test]
    async fn test_publish_nonexistent_story() {
        let temp_file = create_temp_stories_file().await;
        let path = temp_file.path().to_str().unwrap();

        let result = publish_story_in_path(999, path).await.unwrap();
        assert!(result.is_none()); // Should return None for nonexistent story
    }

    #[tokio::test]
    async fn test_save_received_story() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        let received_story = Story {
            id: 999, // This should be overwritten
            name: "Received Story".to_string(),
            header: "Received Header".to_string(),
            body: "Received Body".to_string(),
            public: false, // This should be set to true
            channel: "general".to_string(),
        };

        let new_id = save_received_story_to_path(received_story, path)
            .await
            .unwrap();
        let stories = read_local_stories_from_path(path).await.unwrap();

        assert_eq!(stories.len(), 1);
        assert_eq!(new_id, 0); // Should get new ID

        let saved_story = &stories[0];
        assert_eq!(saved_story.id, 0); // ID should be reassigned
        assert_eq!(saved_story.name, "Received Story");
        assert!(saved_story.public); // Should be marked as public
    }

    #[tokio::test]
    async fn test_save_duplicate_received_story() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        let story = Story {
            id: 1,
            name: "Duplicate".to_string(),
            header: "Header".to_string(),
            body: "Body".to_string(),
            public: false,
            channel: "general".to_string(),
        };

        // Save first time
        let first_id = save_received_story_to_path(story.clone(), path)
            .await
            .unwrap();

        // Save same story again
        let second_id = save_received_story_to_path(story, path).await.unwrap();

        assert_eq!(first_id, second_id); // Should return same ID

        let stories = read_local_stories_from_path(path).await.unwrap();
        assert_eq!(stories.len(), 1); // Should still only have one story
    }

    #[tokio::test]
    async fn test_story_id_sequencing() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        // Create multiple stories and verify ID sequencing
        let id1 = create_new_story_in_path("Story 1", "H1", "B1", path)
            .await
            .unwrap();
        let id2 = create_new_story_in_path("Story 2", "H2", "B2", path)
            .await
            .unwrap();
        let id3 = create_new_story_in_path("Story 3", "H3", "B3", path)
            .await
            .unwrap();

        assert_eq!(id1, 0);
        assert_eq!(id2, 1);
        assert_eq!(id3, 2);

        let stories = read_local_stories_from_path(path).await.unwrap();
        assert_eq!(stories.len(), 3);
    }

    #[tokio::test]
    async fn test_publish_with_channel() {
        let temp_file = create_temp_stories_file().await;
        let path = temp_file.path().to_str().unwrap();

        let (sender, mut receiver) = mpsc::unbounded_channel();

        // This test can't directly test the main publish_story function because it uses
        // the global file path, but we can test the logic
        let story_id = create_new_story_in_path("Channel Test", "Header", "Body", path)
            .await
            .unwrap();
        let published = publish_story_in_path(story_id, path)
            .await
            .unwrap()
            .unwrap();

        // Simulate sending through channel
        sender.send(published.clone()).unwrap();

        let received = receiver.recv().await.unwrap();
        assert_eq!(received.name, "Channel Test");
        assert!(received.public);
    }

    #[tokio::test]
    async fn test_invalid_json_file() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        // Write invalid JSON
        fs::write(path, "invalid json content").await.unwrap();

        let result = read_local_stories_from_path(path).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_save_and_load_peer_name() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        // Save a peer name
        let test_name = "TestPeer";
        save_local_peer_name_to_path(test_name, path).await.unwrap();

        // Load it back
        let loaded_name = load_local_peer_name_from_path(path).await.unwrap();
        assert_eq!(loaded_name, Some(test_name.to_string()));
    }

    #[tokio::test]
    async fn test_load_peer_name_no_file() {
        // Should return None when file doesn't exist
        let loaded_name = load_local_peer_name_from_path("/nonexistent/path")
            .await
            .unwrap();
        assert_eq!(loaded_name, None);
    }

    #[tokio::test]
    async fn test_save_empty_peer_name() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        // Save an empty peer name
        let empty_name = "";
        save_local_peer_name_to_path(empty_name, path)
            .await
            .unwrap();

        // Load it back
        let loaded_name = load_local_peer_name_from_path(path).await.unwrap();
        assert_eq!(loaded_name, Some(empty_name.to_string()));
    }

    #[tokio::test]
    async fn test_file_permissions() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        // Create a story first
        let stories = vec![Story::new(
            1,
            "Test".to_string(),
            "Header".to_string(),
            "Body".to_string(),
            false,
        )];
        write_local_stories_to_path(&stories, path).await.unwrap();

        // Verify we can read it back
        let read_stories = read_local_stories_from_path(path).await.unwrap();
        assert_eq!(read_stories.len(), 1);
        assert_eq!(read_stories[0].name, "Test");
    }

    #[tokio::test]
    async fn test_large_story_content() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        // Create a story with large content
        let large_content = "A".repeat(1000);
        let _id = create_new_story_in_path("Large Story", &large_content, &large_content, path)
            .await
            .unwrap();

        let stories = read_local_stories_from_path(path).await.unwrap();
        assert_eq!(stories.len(), 1);
        assert_eq!(stories[0].header.len(), 1000);
        assert_eq!(stories[0].body.len(), 1000);
    }

    #[tokio::test]
    async fn test_story_with_special_characters() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        // Create a story with special characters
        let special_content = "Hello 🌍! \"Quotes\" & <tags>";
        let _id = create_new_story_in_path(special_content, special_content, special_content, path)
            .await
            .unwrap();

        let stories = read_local_stories_from_path(path).await.unwrap();
        assert_eq!(stories.len(), 1);
        assert_eq!(stories[0].name, special_content);
        assert_eq!(stories[0].header, special_content);
        assert_eq!(stories[0].body, special_content);
    }

    #[tokio::test]
    async fn test_publish_all_stories() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        // Create multiple stories
        let mut story_ids = vec![];
        for i in 0..3 {
            let id = create_new_story_in_path(
                &format!("Story {}", i),
                &format!("Header {}", i),
                &format!("Body {}", i),
                path,
            )
            .await
            .unwrap();
            story_ids.push(id);
        }

        // Publish all stories
        for id in story_ids {
            let result = publish_story_in_path(id, path).await.unwrap();
            assert!(result.is_some());
        }

        // Verify all are public
        let stories = read_local_stories_from_path(path).await.unwrap();
        assert_eq!(stories.len(), 3);
        for story in stories {
            assert!(story.public);
        }
    }

    #[tokio::test]
    async fn test_ensure_stories_file_exists() {
        let temp_dir = tempfile::tempdir().unwrap();

        // We can't easily test the function that uses the database path,
        // but we can test the logic by creating a temporary file
        let test_path = temp_dir.path().join("test_stories.json");
        let test_path_str = test_path.to_str().unwrap();

        // File shouldn't exist initially
        assert!(!test_path.exists());

        // Create empty stories file
        let empty_stories: Stories = Vec::new();
        write_local_stories_to_path(&empty_stories, test_path_str)
            .await
            .unwrap();

        // Now it should exist and be readable
        assert!(test_path.exists());
        let stories = read_local_stories_from_path(test_path_str).await.unwrap();
        assert_eq!(stories.len(), 0);
    }

    #[tokio::test]
    async fn test_delete_local_story_database() {
        use tempfile::NamedTempFile;
        
        // Use a temporary database file for this test
        let temp_db = NamedTempFile::new().unwrap();
        let db_path = temp_db.path().to_str().unwrap();
        
        // Create a direct database connection for testing
        let conn = rusqlite::Connection::open(db_path).unwrap();
        
        // Create tables
        migrations::create_tables(&conn).unwrap();
        
        // Create test stories directly in the test database
        conn.execute(
            "INSERT INTO stories (id, name, header, body, public, channel) VALUES (?, ?, ?, ?, ?, ?)",
            ["0", "Story 1", "Header 1", "Body 1", "0", "general"],
        ).unwrap();
        conn.execute(
            "INSERT INTO stories (id, name, header, body, public, channel) VALUES (?, ?, ?, ?, ?, ?)",
            ["1", "Story 2", "Header 2", "Body 2", "0", "general"],
        ).unwrap();
        conn.execute(
            "INSERT INTO stories (id, name, header, body, public, channel) VALUES (?, ?, ?, ?, ?, ?)",
            ["2", "Story 3", "Header 3", "Body 3", "0", "general"],
        ).unwrap();
        
        // Verify stories exist
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM stories").unwrap();
        let count: i64 = stmt.query_row([], |row| row.get(0)).unwrap();
        assert_eq!(count, 3);

        // Test deleting existing story
        let rows_affected = conn.execute("DELETE FROM stories WHERE id = ?", ["1"]).unwrap();
        assert_eq!(rows_affected, 1);

        // Verify story was deleted
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM stories").unwrap();
        let count_after: i64 = stmt.query_row([], |row| row.get(0)).unwrap();
        assert_eq!(count_after, 2);
        
        // Verify specific story with id=1 is gone
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM stories WHERE id = ?").unwrap();
        let id_count: i64 = stmt.query_row(["1"], |row| row.get(0)).unwrap();
        assert_eq!(id_count, 0);

        // Test deleting non-existent story
        let rows_affected_nonexistent = conn.execute("DELETE FROM stories WHERE id = ?", ["999"]).unwrap();
        assert_eq!(rows_affected_nonexistent, 0);

        // Verify count unchanged
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM stories").unwrap();
        let final_count: i64 = stmt.query_row([], |row| row.get(0)).unwrap();
        assert_eq!(final_count, 2);
    }

    #[tokio::test]
    async fn test_save_and_load_node_description() {
        // Test saving a description
        let description = "This is my node description";
        save_node_description(description).await.unwrap();

        // Test loading it back
        let loaded = load_node_description().await.unwrap();
        assert_eq!(loaded, Some(description.to_string()));
    }

    #[tokio::test]
    async fn test_save_node_description_too_long() {
        // Test description that exceeds 1024 bytes
        let long_description = "A".repeat(1025);
        let result = save_node_description(&long_description).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("1024 bytes limit"));
    }

    #[tokio::test]
    async fn test_load_node_description_no_file() {
        // Delete the file if it exists
        let _ = tokio::fs::remove_file("node_description.txt").await;
        
        let result = load_node_description().await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_save_empty_node_description() {
        // Test saving empty description
        save_node_description("").await.unwrap();
        
        let loaded = load_node_description().await.unwrap();
        assert_eq!(loaded, None); // Empty should return None
    }

    #[tokio::test]
    async fn test_node_description_max_size() {
        // Test description at exactly 1024 bytes
        let description = "A".repeat(1024);
        save_node_description(&description).await.unwrap();
        
        let loaded = load_node_description().await.unwrap();
        assert_eq!(loaded, Some(description));
    }
}
