use crate::errors::StorageResult;
use crate::storage::{mappers, utils};
use crate::types::{
    BootstrapConfig, Channel, ChannelSubscription, ChannelSubscriptions, Channels,
    DirectMessageConfig, NetworkConfig, Stories, Story, UnifiedNetworkConfig,
};
use log::debug;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs;
use tokio::sync::{Mutex, RwLock};

use crate::migrations;

#[allow(dead_code)]
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

// Type alias for our connection pool
type DbPool = Pool<SqliteConnectionManager>;

// Thread-safe database connection pool with support for dynamic paths
static DB_POOL_STATE: once_cell::sync::Lazy<RwLock<Option<(DbPool, String)>>> =
    once_cell::sync::Lazy::new(|| RwLock::new(None));

/// Configuration for the database connection pool
struct PoolConfig {
    max_size: u32,
    min_idle: Option<u32>,
    connection_timeout: Duration,
    idle_timeout: Duration,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_size: 10,      // Allow up to 10 concurrent connections
            min_idle: Some(2), // Keep at least 2 connections idle
            connection_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(600), // 10 minutes
        }
    }
}

/// Create a new database connection pool
fn create_db_pool(db_path: &str) -> StorageResult<DbPool> {
    let config = PoolConfig::default();

    // Create connection manager
    let manager = SqliteConnectionManager::file(db_path).with_init(|conn| {
        // Enable foreign key constraints and set optimal pragmas
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
                 PRAGMA synchronous = NORMAL;
                 PRAGMA cache_size = -64000;
                 PRAGMA temp_store = MEMORY;
                 PRAGMA journal_mode = WAL;",
        )?;
        Ok(())
    });

    // Build the pool with configuration
    let pool = Pool::builder()
        .max_size(config.max_size)
        .min_idle(config.min_idle)
        .connection_timeout(config.connection_timeout)
        .idle_timeout(Some(config.idle_timeout))
        .build(manager)
        .map_err(|e| format!("Failed to create database pool: {e}"))?;

    debug!(
        "Created database pool with max_size={}, min_idle={:?}",
        config.max_size, config.min_idle
    );

    Ok(pool)
}

/// Get a database connection from the pool
pub async fn get_db_connection() -> StorageResult<Arc<Mutex<Connection>>> {
    let current_path = get_database_path();

    // Check if we have an existing pool with the same path
    {
        let state = DB_POOL_STATE.read().await;
        if let Some((pool, stored_path)) = state.as_ref() {
            if stored_path == &current_path {
                // Get a connection from the pool - pooled connections implement Deref to Connection
                let _pooled_conn = pool
                    .get()
                    .map_err(|e| format!("Failed to get connection from pool: {e}"))?;

                // For now, create a new connection with the same path to maintain compatibility
                // In a future iteration, we could optimize this further
                let conn = Connection::open(&current_path)?;
                conn.execute_batch(
                    "PRAGMA foreign_keys = ON;
                     PRAGMA synchronous = NORMAL;
                     PRAGMA cache_size = -64000;
                     PRAGMA temp_store = MEMORY;
                     PRAGMA journal_mode = WAL;",
                )?;

                return Ok(Arc::new(Mutex::new(conn)));
            }
        }
    }

    // Need to create or update pool
    debug!("Creating new SQLite database connection pool: {current_path}");
    let pool = create_db_pool(&current_path)?;

    // Create a direct connection for immediate use while pool is ready for future requests
    let conn = Connection::open(&current_path)?;
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA synchronous = NORMAL;
         PRAGMA cache_size = -64000;
         PRAGMA temp_store = MEMORY;
         PRAGMA journal_mode = WAL;",
    )?;
    let conn_arc = Arc::new(Mutex::new(conn));

    // Update the stored pool and path
    {
        let mut state = DB_POOL_STATE.write().await;
        *state = Some((pool, current_path));
    }

    debug!("Successfully created database connection pool with optimized pragmas");
    Ok(conn_arc)
}

/// Reset database connection pool (useful for testing)
pub async fn reset_db_connection_for_testing() -> StorageResult<()> {
    let mut state = DB_POOL_STATE.write().await;
    *state = None;
    // Add a small delay to ensure any pending database operations complete
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    Ok(())
}

/// Get database pool statistics for monitoring
pub async fn get_pool_stats() -> Option<(u32, u32, u32)> {
    let state = DB_POOL_STATE.read().await;
    if let Some((pool, _)) = state.as_ref() {
        let state = pool.state();
        Some((state.connections, state.idle_connections, pool.max_size()))
    } else {
        None
    }
}

/// Execute a function with a database transaction
/// This helps group related operations and provides better error handling
pub async fn with_transaction<F, R>(f: F) -> StorageResult<R>
where
    F: FnOnce(&Connection) -> StorageResult<R>,
{
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    // Start transaction
    conn.execute("BEGIN TRANSACTION", [])?;

    match f(&conn) {
        Ok(result) => {
            conn.execute("COMMIT", [])?;
            Ok(result)
        }
        Err(e) => {
            // Rollback on error
            let _ = conn.execute("ROLLBACK", []);
            Err(e)
        }
    }
}

/// Execute a function with a read-only database transaction
/// This is optimized for read operations and reduces lock contention
pub async fn with_read_transaction<F, R>(f: F) -> StorageResult<R>
where
    F: FnOnce(&Connection) -> StorageResult<R>,
{
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    // Start read-only transaction
    conn.execute("BEGIN DEFERRED", [])?;

    match f(&conn) {
        Ok(result) => {
            conn.execute("COMMIT", [])?;
            Ok(result)
        }
        Err(e) => {
            // Rollback on error
            let _ = conn.execute("ROLLBACK", []);
            Err(e)
        }
    }
}

/// Initialize a clean test database with proper isolation
pub async fn init_test_database() -> StorageResult<()> {
    reset_db_connection_for_testing().await?;
    ensure_stories_file_exists().await?;

    // Clear all existing data for clean test (only if tables exist)
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    // Disable foreign key checks temporarily to allow clean deletion
    conn.execute("PRAGMA foreign_keys = OFF", [])?;

    // Check if tables exist and clear them
    let table_exists = |table_name: &str| -> StorageResult<bool> {
        let mut stmt = conn.prepare(&format!(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='{table_name}'"
        ))?;
        let exists = stmt.exists([])?;
        Ok(exists)
    };

    if table_exists("channel_subscriptions")? {
        conn.execute("DELETE FROM channel_subscriptions", [])?;
    }
    if table_exists("channels")? {
        conn.execute("DELETE FROM channels", [])?;
    }
    if table_exists("stories")? {
        conn.execute("DELETE FROM stories", [])?;
    }
    if table_exists("peer_names")? {
        conn.execute("DELETE FROM peer_names", [])?;
    }
    if table_exists("story_read_status")? {
        conn.execute("DELETE FROM story_read_status", [])?;
    }

    // Re-enable foreign key checks
    conn.execute("PRAGMA foreign_keys = ON", [])?;

    drop(conn); // Release the lock

    // Add a small delay to ensure all operations complete
    tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;

    Ok(())
}

async fn create_tables() -> StorageResult<()> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    migrations::create_tables(&conn)?;
    Ok(())
}

pub async fn ensure_stories_file_exists() -> StorageResult<()> {
    let db_path = get_database_path();
    debug!("Initializing SQLite database at: {db_path}");

    // Ensure the directory exists for the database file
    if let Some(parent) = std::path::Path::new(&db_path).parent() {
        if !parent.exists() {
            debug!("Creating directory: {parent:?}");
            tokio::fs::create_dir_all(parent).await?;
        }
    }

    let _conn = get_db_connection().await?;
    create_tables().await?;
    debug!("SQLite database and tables initialized");
    Ok(())
}

pub async fn read_local_stories() -> StorageResult<Stories> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let mut stmt =
        conn.prepare("SELECT id, name, header, body, public, channel, created_at FROM stories ORDER BY created_at DESC")?;
    let story_iter = stmt.query_map([], mappers::map_row_to_story)?;

    let mut stories = Vec::new();
    for story in story_iter {
        stories.push(story?);
    }

    Ok(stories)
}

pub async fn read_local_stories_from_path(path: &str) -> StorageResult<Stories> {
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
        let story_iter = stmt.query_map([], mappers::map_row_to_story)?;

        let mut stories = Vec::new();
        for story in story_iter {
            stories.push(story?);
        }

        Ok(stories)
    }
}

/// Read local stories with filtering applied at the database level for efficient story synchronization
pub async fn read_local_stories_for_sync(
    last_sync_timestamp: u64,
    subscribed_channels: &[String],
) -> StorageResult<Stories> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    // Build dynamic query with proper filtering
    let mut query = "SELECT id, name, header, body, public, channel, created_at FROM stories WHERE public = 1 AND created_at > ?".to_string();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(last_sync_timestamp as i64)];

    // Add channel filtering if channels are specified
    if !subscribed_channels.is_empty() {
        let placeholders = vec!["?"; subscribed_channels.len()].join(",");
        query.push_str(&format!(" AND channel IN ({placeholders})"));
        for channel in subscribed_channels {
            params.push(Box::new(channel.clone()));
        }
    }

    query.push_str(" ORDER BY created_at DESC");

    let mut stmt = conn.prepare(&query)?;

    // Convert params to the proper type for query_map
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let story_iter = stmt.query_map(param_refs.as_slice(), mappers::map_row_to_story)?;

    let mut stories = Vec::new();
    for story in story_iter {
        stories.push(story?);
    }

    Ok(stories)
}

/// Extract unique channel metadata for stories being shared during sync
/// Creates default metadata for channels that don't exist in the local database
pub async fn get_channels_for_stories(stories: &[Story]) -> StorageResult<Channels> {
    if stories.is_empty() {
        debug!("No stories provided, returning empty channel list");
        return Ok(Vec::new());
    }

    debug!("Getting channel metadata for {} stories", stories.len());

    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    // Get unique channel names from stories
    let mut unique_channels: std::collections::HashSet<String> = std::collections::HashSet::new();
    for story in stories {
        unique_channels.insert(story.channel.clone());
    }

    debug!(
        "Found {} unique channel names in stories: {:?}",
        unique_channels.len(),
        unique_channels
    );

    if unique_channels.is_empty() {
        debug!("No unique channels found in stories");
        return Ok(Vec::new());
    }

    // Build query to get channel metadata for these channels
    let placeholders = vec!["?"; unique_channels.len()].join(",");
    let query = format!(
        "SELECT name, description, created_by, created_at FROM channels WHERE name IN ({placeholders}) ORDER BY name"
    );

    debug!(
        "Executing query: {} with params: {:?}",
        query, unique_channels
    );

    let mut stmt = conn.prepare(&query)?;

    // Convert channel names to query parameters
    let channel_names: Vec<String> = unique_channels.clone().into_iter().collect();
    let param_refs: Vec<&dyn rusqlite::ToSql> = channel_names
        .iter()
        .map(|s| s as &dyn rusqlite::ToSql)
        .collect();

    let channel_iter = stmt.query_map(param_refs.as_slice(), mappers::map_row_to_channel)?;

    let mut channels = Vec::new();
    let mut found_channel_names = std::collections::HashSet::new();

    for channel in channel_iter {
        let channel = channel?;
        found_channel_names.insert(channel.name.clone());
        channels.push(channel);
    }

    // For any channels that don't exist in our database, create default metadata
    // This ensures that peers can discover channels even if the original creator
    // doesn't have the metadata stored locally
    for channel_name in unique_channels {
        if !found_channel_names.contains(&channel_name) {
            debug!(
                "Creating default metadata for unknown channel: {}",
                channel_name
            );
            let default_channel = crate::types::Channel::new(
                channel_name.clone(),
                format!("Channel: {}", channel_name),
                "unknown".to_string(),
            );
            channels.push(default_channel);
        }
    }

    debug!(
        "Found/created {} channel metadata entries for {} stories: {:?}",
        channels.len(),
        stories.len(),
        channels.iter().map(|c| &c.name).collect::<Vec<_>>()
    );
    Ok(channels)
}

/// Process and save discovered channels from story sync
pub async fn process_discovered_channels(
    channels: &[Channel],
    peer_name: &str,
) -> StorageResult<usize> {
    if channels.is_empty() {
        debug!("No channels to process from peer {}", peer_name);
        return Ok(0);
    }

    debug!(
        "Processing {} discovered channels from peer {}",
        channels.len(),
        peer_name
    );

    let mut saved_count = 0;
    for channel in channels {
        debug!(
            "Processing channel: name='{}', description='{}', created_by='{}'",
            channel.name, channel.description, channel.created_by
        );

        // Validate channel data before saving
        if channel.name.is_empty() || channel.description.is_empty() {
            debug!(
                "Skipping invalid channel with empty name or description: name='{}', description='{}'",
                channel.name, channel.description
            );
            continue;
        }

        // Check if channel already exists before trying to create it
        match channel_exists(&channel.name).await {
            Ok(true) => {
                debug!(
                    "Channel '{}' already exists in database, skipping",
                    channel.name
                );
                // Channel already exists, don't count as new discovery
            }
            Ok(false) => {
                // Channel doesn't exist, try to create it
                match create_channel(&channel.name, &channel.description, &channel.created_by).await
                {
                    Ok(_) => {
                        saved_count += 1;
                        debug!(
                            "Successfully saved discovered channel '{}' from peer {}",
                            channel.name, peer_name
                        );
                    }
                    Err(e) => {
                        debug!(
                            "Failed to save discovered channel '{}': {}",
                            channel.name, e
                        );
                    }
                }
            }
            Err(e) => {
                debug!(
                    "Failed to check if channel '{}' exists: {}",
                    channel.name, e
                );
            }
        }
    }

    debug!(
        "Processed {} channels from peer {}, saved {} new channels",
        channels.len(),
        peer_name,
        saved_count
    );
    Ok(saved_count)
}

pub async fn write_local_stories(stories: &Stories) -> StorageResult<()> {
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

pub async fn write_local_stories_to_path(stories: &Stories, path: &str) -> StorageResult<()> {
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

pub async fn create_new_story(name: &str, header: &str, body: &str) -> StorageResult<()> {
    create_new_story_with_channel(name, header, body, "general").await
}

pub async fn create_new_story_with_channel(
    name: &str,
    header: &str,
    body: &str,
    channel: &str,
) -> StorageResult<()> {
    let conn_arc = get_db_connection().await?;

    // Get the next ID using utility function
    let next_id = utils::get_next_id(&conn_arc, "stories").await?;

    // Get the current timestamp using utility function
    let created_at = utils::get_current_timestamp();

    // Load auto-share configuration to determine if story should be public
    let should_be_public = match load_unified_network_config().await {
        Ok(config) => config.auto_share.global_auto_share,
        Err(_) => true, // Default to true if config can't be loaded
    };

    let conn = conn_arc.lock().await;
    // Insert the new story
    conn.execute(
        "INSERT INTO stories (id, name, header, body, public, channel, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        [
            &next_id.to_string(),
            name,
            header,
            body,
            &utils::rust_bool_to_db(should_be_public), // Use auto-share configuration
            channel,
            &created_at.to_string(),
        ],
    )?;

    debug!("Created story:");
    debug!("Name: {name}");
    debug!("Header: {header}");
    debug!("Body: {body}");

    Ok(())
}

pub async fn create_new_story_in_path(
    name: &str,
    header: &str,
    body: &str,
    path: &str,
) -> StorageResult<usize> {
    let mut local_stories: Vec<Story> =
        read_local_stories_from_path(path).await.unwrap_or_default();
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
        auto_share: None, // Use global setting by default
    });
    write_local_stories_to_path(&local_stories, path).await?;
    Ok(new_id)
}

pub async fn publish_story(
    id: usize,
    sender: tokio::sync::mpsc::UnboundedSender<Story>,
) -> StorageResult<()> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    // Update the story to be public
    let rows_affected = conn.execute(
        "UPDATE stories SET public = ? WHERE id = ?",
        [&utils::rust_bool_to_db(true), &id.to_string()],
    )?;

    if rows_affected > 0 {
        // Fetch the updated story to send it
        let mut stmt = conn.prepare(
            "SELECT id, name, header, body, public, channel, created_at FROM stories WHERE id = ?",
        )?;
        let story_result = stmt.query_row([&id.to_string()], mappers::map_row_to_story);

        if let Ok(story) = story_result {
            if let Err(e) = sender.send(story) {
                let error_logger = crate::error_logger::ErrorLogger::new("errors.log");
                crate::log_network_error!(
                    error_logger,
                    "storage",
                    "error sending story for broadcast: {}",
                    e
                );
            }
        }
    }

    Ok(())
}

pub async fn delete_local_story(id: usize) -> StorageResult<bool> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    // Delete the story with the given ID
    let rows_affected = conn.execute("DELETE FROM stories WHERE id = ?", [&id.to_string()])?;

    if rows_affected > 0 {
        debug!("Deleted story with ID: {id}");
        Ok(true)
    } else {
        debug!("No story found with ID: {id}");
        Ok(false)
    }
}

pub async fn publish_story_in_path(id: usize, path: &str) -> StorageResult<Option<Story>> {
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

pub async fn save_received_story(story: Story) -> StorageResult<()> {
    let conn_arc = get_db_connection().await?;

    // Check if story already exists (by name and content to avoid duplicates)
    {
        let conn = conn_arc.lock().await;
        let mut stmt =
            conn.prepare("SELECT id FROM stories WHERE name = ? AND header = ? AND body = ?")?;
        let existing = stmt.query_row(
            [&story.name, &story.header, &story.body],
            mappers::map_row_to_i64,
        );

        if existing.is_ok() {
            debug!("Story already exists locally, skipping save");
            return Ok(());
        }
    }

    // Story doesn't exist, get the next ID
    let new_id = utils::get_next_id(&conn_arc, "stories").await?;

    // Insert the story with the new ID and mark as public
    let conn = conn_arc.lock().await;
    conn.execute(
        "INSERT INTO stories (id, name, header, body, public, channel, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        [
            &new_id.to_string(),
            &story.name,
            &story.header,
            &story.body,
            &utils::rust_bool_to_db(true), // Mark as public since it was published
            &story.channel,
            &story.created_at.to_string(),
        ],
    )?;

    debug!("Saved received story to local storage with ID: {new_id}");
    Ok(())
}

pub async fn save_received_story_to_path(mut story: Story, path: &str) -> StorageResult<usize> {
    let mut local_stories: Vec<Story> =
        read_local_stories_from_path(path).await.unwrap_or_default();

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

pub async fn save_local_peer_name(name: &str) -> StorageResult<()> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    // Insert or replace the peer name (there should only be one row)
    conn.execute(
        "INSERT OR REPLACE INTO peer_name (id, name) VALUES (1, ?)",
        [name],
    )?;

    Ok(())
}

pub async fn save_local_peer_name_to_path(name: &str, path: &str) -> StorageResult<()> {
    // For test compatibility, keep writing to JSON file
    let json = serde_json::to_string(name)?;
    fs::write(path, &json).await?;
    Ok(())
}

pub async fn load_local_peer_name() -> StorageResult<Option<String>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let mut stmt = conn.prepare("SELECT name FROM peer_name WHERE id = 1")?;
    let result = stmt.query_row([], |row| row.get::<_, String>(0));

    match result {
        Ok(name) => Ok(Some(name)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub async fn load_local_peer_name_from_path(path: &str) -> StorageResult<Option<String>> {
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
pub async fn create_channel(name: &str, description: &str, created_by: &str) -> StorageResult<()> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let timestamp = utils::get_current_timestamp();

    conn.execute(
        "INSERT OR IGNORE INTO channels (name, description, created_by, created_at) VALUES (?, ?, ?, ?)",
        [name, description, created_by, &timestamp.to_string()],
    )?;

    debug!("Created channel: {name} - {description}");
    Ok(())
}

/// Check if a channel exists in the database
pub async fn channel_exists(name: &str) -> StorageResult<bool> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let mut stmt = conn.prepare("SELECT 1 FROM channels WHERE name = ? LIMIT 1")?;
    let exists = stmt.exists([name])?;
    Ok(exists)
}

pub async fn read_channels() -> StorageResult<Channels> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let mut stmt = conn
        .prepare("SELECT name, description, created_by, created_at FROM channels ORDER BY name")?;
    let channel_iter = stmt.query_map([], mappers::map_row_to_channel)?;

    let mut channels = Vec::new();
    for channel in channel_iter {
        channels.push(channel?);
    }

    Ok(channels)
}

pub async fn subscribe_to_channel(peer_id: &str, channel_name: &str) -> StorageResult<()> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let timestamp = utils::get_current_timestamp();

    conn.execute(
        "INSERT OR REPLACE INTO channel_subscriptions (peer_id, channel_name, subscribed_at) VALUES (?, ?, ?)",
        [peer_id, channel_name, &timestamp.to_string()],
    )?;

    debug!("Subscribed {peer_id} to channel: {channel_name}");
    Ok(())
}

pub async fn unsubscribe_from_channel(peer_id: &str, channel_name: &str) -> StorageResult<()> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    conn.execute(
        "DELETE FROM channel_subscriptions WHERE peer_id = ? AND channel_name = ?",
        [peer_id, channel_name],
    )?;

    debug!("Unsubscribed {peer_id} from channel: {channel_name}");
    Ok(())
}

pub async fn read_subscribed_channels(peer_id: &str) -> StorageResult<Vec<String>> {
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

/// Get full channel details for channels that the user is subscribed to
pub async fn read_subscribed_channels_with_details(peer_id: &str) -> StorageResult<Channels> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let mut stmt = conn.prepare(
        r#"
        SELECT c.name, c.description, c.created_by, c.created_at 
        FROM channels c
        INNER JOIN channel_subscriptions cs ON c.name = cs.channel_name AND cs.peer_id = ?
        ORDER BY c.name
        "#,
    )?;

    let channel_iter = stmt.query_map([peer_id], |row| {
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

/// Get channels that are available but not subscribed to by the given peer
pub async fn read_unsubscribed_channels(peer_id: &str) -> StorageResult<Channels> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let mut stmt = conn.prepare(
        r#"
        SELECT c.name, c.description, c.created_by, c.created_at 
        FROM channels c
        LEFT JOIN channel_subscriptions cs ON c.name = cs.channel_name AND cs.peer_id = ?
        WHERE cs.channel_name IS NULL
        ORDER BY c.name
        "#,
    )?;

    let channel_iter = stmt.query_map([peer_id], |row| {
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

/// Get the count of current auto-subscriptions for a peer (to check against limits)
pub async fn get_auto_subscription_count(peer_id: &str) -> StorageResult<usize> {
    let subscribed = read_subscribed_channels(peer_id).await?;
    // For now, we'll count all subscriptions as auto-subscriptions
    // In the future, we could add a flag to track which were auto vs manual
    Ok(subscribed.len())
}

pub async fn read_channel_subscriptions() -> StorageResult<ChannelSubscriptions> {
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

pub async fn save_direct_message(
    message: &crate::types::DirectMessage,
    peer_names: Option<&std::collections::HashMap<libp2p::PeerId, String>>,
) -> StorageResult<()> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;
    let is_read = message.is_outgoing; // Outgoing messages are considered read by default

    // Determine the remote peer ID
    let origin_peer_id = if message.is_outgoing {
        &message.to_peer_id
    } else {
        &message.from_peer_id
    };

    let conversation_id = create_or_find_conversation(&conn, origin_peer_id, peer_names)?;

    conn.execute(
        "INSERT INTO direct_messages (remote_peer_id, to_peer_id, message, timestamp, is_outgoing, is_read, conversation_id) VALUES (?, ?, ?, ?, ?, ?, ?)",
        [
            &message.from_peer_id,
            &message.to_peer_id,
            &message.message,
            &message.timestamp.to_string(),
            &utils::rust_bool_to_db(message.is_outgoing),
            &utils::rust_bool_to_db(is_read),
            &conversation_id.to_string(),
        ],
    )?;

    Ok(())
}

fn create_or_find_conversation(
    conn: &Connection,
    peer_id: &str,
    peer_names: Option<&std::collections::HashMap<libp2p::PeerId, String>>,
) -> StorageResult<i64> {
    let mut stmt = conn.prepare("SELECT id, peer_name FROM conversations WHERE peer_id = ?")?;
    let existing_result = stmt.query_row([peer_id], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    });

    match existing_result {
        Ok((id, current_peer_name)) => {
            if let Some(names) = peer_names {
                if let Ok(parsed_peer_id) = peer_id.parse::<libp2p::PeerId>() {
                    if let Some(actual_name) = names.get(&parsed_peer_id) {
                        if current_peer_name == peer_id || current_peer_name != *actual_name {
                            conn.execute(
                                "UPDATE conversations SET peer_name = ? WHERE id = ?",
                                [actual_name, &id.to_string()],
                            )?;
                        }
                    }
                }
            }
            Ok(id)
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            let peer_name = if let Some(names) = peer_names {
                if let Ok(parsed_peer_id) = peer_id.parse::<libp2p::PeerId>() {
                    names
                        .get(&parsed_peer_id)
                        .cloned()
                        .unwrap_or_else(|| peer_id.to_string())
                } else {
                    peer_id.to_string()
                }
            } else {
                peer_id.to_string()
            };

            conn.execute(
                "INSERT INTO conversations (peer_id, peer_name) VALUES (?, ?)",
                [peer_id, &peer_name],
            )?;

            let new_id = conn.last_insert_rowid();
            Ok(new_id)
        }
        Err(e) => Err(e.into()),
    }
}

pub async fn get_stories_by_channel(channel_name: &str) -> StorageResult<Stories> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let mut stmt = conn.prepare("SELECT id, name, header, body, public, channel, created_at FROM stories WHERE channel = ? AND public = 1 ORDER BY created_at DESC")?;
    let story_iter = stmt.query_map([channel_name], mappers::map_row_to_story)?;

    let mut stories = Vec::new();
    for story in story_iter {
        stories.push(story?);
    }

    Ok(stories)
}

/// Clears all data from the database and ensures fresh test database (useful for testing)
pub async fn clear_database_for_testing() -> StorageResult<()> {
    // Reset the connection to ensure we're using the test database path
    reset_db_connection_for_testing().await?;

    // Ensure the test database is initialized
    ensure_stories_file_exists().await?;

    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    // Clear all tables in correct order (respecting foreign key constraints)
    conn.execute("DELETE FROM story_read_status", [])?; // Clear first - has FKs to stories and channels
    conn.execute("DELETE FROM channel_subscriptions", [])?; // Clear second - has FK to channels
    conn.execute("DELETE FROM stories", [])?; // Clear third - referenced by story_read_status
    conn.execute("DELETE FROM channels", [])?; // Clear fourth - referenced by subscriptions and read_status
    conn.execute("DELETE FROM peer_name", [])?; // Clear last - no FKs

    // Recreate the general channel to ensure consistent test state
    conn.execute(
        r#"
        INSERT INTO channels (name, description, created_by, created_at)
        VALUES ('general', 'Default general discussion channel', 'system', 0)
        "#,
        [],
    )?;

    debug!("Test database cleared and reset");
    Ok(())
}

/// Save node description to file (limited to 1024 bytes)
pub async fn save_node_description(description: &str) -> StorageResult<()> {
    if description.len() > 1024 {
        return Err("Description exceeds 1024 bytes limit".into());
    }

    fs::write(NODE_DESCRIPTION_FILE_PATH, description).await?;
    Ok(())
}

/// Load local node description from file
pub async fn load_node_description() -> StorageResult<Option<String>> {
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
pub async fn save_bootstrap_config(config: &BootstrapConfig) -> StorageResult<()> {
    save_bootstrap_config_to_path(config, "bootstrap_config.json").await
}

/// Save bootstrap configuration to specific path
pub async fn save_bootstrap_config_to_path(
    config: &BootstrapConfig,
    path: &str,
) -> StorageResult<()> {
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
pub async fn load_bootstrap_config() -> StorageResult<BootstrapConfig> {
    load_bootstrap_config_from_path("bootstrap_config.json").await
}

/// Load bootstrap configuration from specific path, creating default if missing
pub async fn load_bootstrap_config_from_path(path: &str) -> StorageResult<BootstrapConfig> {
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
pub async fn ensure_bootstrap_config_exists() -> StorageResult<()> {
    if tokio::fs::metadata("bootstrap_config.json").await.is_err() {
        let default_config = BootstrapConfig::default();
        save_bootstrap_config(&default_config).await?;
        debug!("Created default bootstrap config file");
    }
    Ok(())
}

/// Save direct message configuration to file
pub async fn save_direct_message_config(config: &DirectMessageConfig) -> StorageResult<()> {
    save_direct_message_config_to_path(config, "direct_message_config.json").await
}

/// Save direct message configuration to specific path
pub async fn save_direct_message_config_to_path(
    config: &DirectMessageConfig,
    path: &str,
) -> StorageResult<()> {
    let json = serde_json::to_string_pretty(config)?;
    fs::write(path, json).await?;
    debug!("Saved direct message config to {path}");
    Ok(())
}

/// Load direct message configuration from file, creating default if missing
pub async fn load_direct_message_config() -> StorageResult<DirectMessageConfig> {
    load_direct_message_config_from_path("direct_message_config.json").await
}

/// Load direct message configuration from specific path, creating default if missing
pub async fn load_direct_message_config_from_path(
    path: &str,
) -> StorageResult<DirectMessageConfig> {
    match fs::read_to_string(path).await {
        Ok(content) => {
            let config: DirectMessageConfig = serde_json::from_str(&content)?;

            // Validate the loaded config
            config.validate()?;

            debug!("Loaded direct message config from {path}");
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
pub async fn ensure_direct_message_config_exists() -> StorageResult<()> {
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

/// Save network configuration to file
pub async fn save_network_config(config: &NetworkConfig) -> StorageResult<()> {
    save_network_config_to_path(config, "network_config.json").await
}

/// Save network configuration to specific path
pub async fn save_network_config_to_path(config: &NetworkConfig, path: &str) -> StorageResult<()> {
    let json = serde_json::to_string_pretty(config)?;
    fs::write(path, json).await?;
    debug!("Saved network config to {path}");
    Ok(())
}

/// Load network configuration from file, creating default if missing
pub async fn load_network_config() -> StorageResult<NetworkConfig> {
    load_network_config_from_path("network_config.json").await
}

/// Load network configuration from specific path, creating default if missing
pub async fn load_network_config_from_path(path: &str) -> StorageResult<NetworkConfig> {
    match fs::read_to_string(path).await {
        Ok(content) => {
            let config: NetworkConfig = serde_json::from_str(&content)?;

            // Validate the loaded config
            config.validate()?;

            debug!("Loaded network config from {path}");
            Ok(config)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            debug!("No network config file found, creating default");
            let default_config = NetworkConfig::default();

            // Save the default config for future use
            save_network_config_to_path(&default_config, path).await?;

            Ok(default_config)
        }
        Err(e) => Err(e.into()),
    }
}

/// Ensure network config file exists with defaults
pub async fn ensure_network_config_exists() -> StorageResult<()> {
    if tokio::fs::metadata("network_config.json").await.is_err() {
        let default_config = NetworkConfig::default();
        save_network_config(&default_config).await?;
        debug!("Created default network config file");
    }
    Ok(())
}

/// Save unified network configuration to file
pub async fn save_unified_network_config(config: &UnifiedNetworkConfig) -> StorageResult<()> {
    save_unified_network_config_to_path(config, "unified_network_config.json").await
}

/// Save unified network configuration to a specific path
pub async fn save_unified_network_config_to_path(
    config: &UnifiedNetworkConfig,
    path: &str,
) -> StorageResult<()> {
    config
        .validate()
        .map_err(|e| format!("Configuration validation failed: {e}"))?;
    let json = serde_json::to_string_pretty(config)?;
    fs::write(path, &json).await?;
    Ok(())
}

/// Load unified network configuration from file
pub async fn load_unified_network_config() -> StorageResult<UnifiedNetworkConfig> {
    load_unified_network_config_from_path("unified_network_config.json").await
}

/// Load unified network configuration from a specific path
pub async fn load_unified_network_config_from_path(
    path: &str,
) -> StorageResult<UnifiedNetworkConfig> {
    match fs::read_to_string(path).await {
        Ok(content) => {
            let config: UnifiedNetworkConfig = serde_json::from_str(&content)?;
            config
                .validate()
                .map_err(|e| format!("Configuration validation failed: {e}"))?;
            Ok(config)
        }
        Err(_) => {
            // File doesn't exist, create with defaults
            let default_config = UnifiedNetworkConfig::new();
            save_unified_network_config_to_path(&default_config, path).await?;
            debug!("Created default unified network config file at: {path}");
            Ok(default_config)
        }
    }
}

/// Ensure unified network config file exists with defaults
pub async fn ensure_unified_network_config_exists() -> StorageResult<()> {
    if tokio::fs::metadata("unified_network_config.json")
        .await
        .is_err()
    {
        let default_config = UnifiedNetworkConfig::default();
        save_unified_network_config(&default_config).await?;
        debug!("Created default unified network config file");
    }
    Ok(())
}

/// Mark a story as read for a specific peer
pub async fn mark_story_as_read(
    story_id: usize,
    peer_id: &str,
    channel_name: &str,
) -> StorageResult<()> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let read_at = utils::get_current_timestamp();

    conn.execute(
        "INSERT OR REPLACE INTO story_read_status (story_id, peer_id, read_at, channel_name) VALUES (?, ?, ?, ?)",
        [&story_id.to_string(), peer_id, &read_at.to_string(), channel_name],
    )?;

    debug!("Marked story {story_id} as read for peer {peer_id} in channel {channel_name}");
    Ok(())
}

/// Get unread story count for each channel for a specific peer
pub async fn get_unread_counts_by_channel(peer_id: &str) -> StorageResult<HashMap<String, usize>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let mut stmt = conn.prepare(
        r#"
        SELECT s.channel, COUNT(*) as unread_count
        FROM stories s
        LEFT JOIN story_read_status srs ON s.id = srs.story_id AND srs.peer_id = ?
        WHERE s.public = 1 AND srs.story_id IS NULL
        GROUP BY s.channel
        "#,
    )?;

    let unread_iter = stmt.query_map([peer_id], mappers::map_row_to_channel_unread_count)?;

    let mut unread_counts = HashMap::new();
    for result in unread_iter {
        let (channel, count) = result?;
        unread_counts.insert(channel, count);
    }

    Ok(unread_counts)
}

/// Check if a specific story is read by a peer
pub async fn is_story_read(story_id: usize, peer_id: &str) -> StorageResult<bool> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let mut stmt =
        conn.prepare("SELECT COUNT(*) FROM story_read_status WHERE story_id = ? AND peer_id = ?")?;

    let count: i64 = stmt.query_row([&story_id.to_string(), peer_id], mappers::map_row_to_i64)?;

    Ok(count > 0)
}

pub async fn get_conversations_with_status() -> StorageResult<Vec<crate::types::Conversation>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let mut stmt = conn.prepare(
        r#"
        SELECT 
            c.peer_id, 
            c.peer_name,
            COUNT(CASE WHEN dm.is_read = 0 AND dm.is_outgoing = 0 THEN 1 END) as unread_count,
            MAX(dm.timestamp) as last_activity
        FROM conversations c
        LEFT JOIN direct_messages dm ON c.id = dm.conversation_id
        GROUP BY c.id, c.peer_id, c.peer_name
        ORDER BY last_activity DESC NULLS LAST
        "#,
    )?;

    let conversation_iter = stmt.query_map([], |row| {
        let peer_id: String = row.get(0)?;
        let peer_name: String = row.get(1)?;
        let unread_count: i64 = row.get(2)?;
        let last_activity: Option<i64> = row.get(3)?;

        Ok(crate::types::Conversation {
            peer_id,
            peer_name,
            messages: Vec::new(),
            unread_count: unread_count as usize,
            last_activity: last_activity.unwrap_or(0) as u64,
        })
    })?;

    let mut conversations = Vec::new();
    for conversation in conversation_iter {
        conversations.push(conversation?);
    }

    Ok(conversations)
}

pub async fn mark_conversation_messages_as_read(peer_id: &str) -> StorageResult<()> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    conn.execute(
        "UPDATE direct_messages SET is_read = 1 WHERE remote_peer_id = ? AND is_outgoing = 0 AND is_read = 0",
        [peer_id],
    )?;

    Ok(())
}

pub async fn get_conversation_messages(
    peer_id: &str,
) -> StorageResult<Vec<crate::types::DirectMessage>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let mut stmt = conn.prepare(
        "SELECT remote_peer_id, to_peer_id, message, timestamp, is_outgoing, is_read FROM direct_messages dm 
         JOIN conversations c ON dm.conversation_id = c.id 
         WHERE c.peer_id = ? 
         ORDER BY dm.timestamp ASC"
    )?;

    let message_iter = stmt.query_map([peer_id], |row| {
        Ok(crate::types::DirectMessage {
            from_peer_id: row.get(0)?,
            to_peer_id: row.get(1)?,
            from_name: String::new(),
            to_name: String::new(),
            message: row.get(2)?,
            timestamp: row.get(3)?,
            is_outgoing: utils::db_bool_to_rust(row.get(4)?),
        })
    })?;

    let mut messages = Vec::new();
    for message in message_iter {
        messages.push(message?);
    }

    Ok(messages)
}

/// Get all unread story IDs for a specific channel and peer
pub async fn get_unread_story_ids_for_channel(
    peer_id: &str,
    channel_name: &str,
) -> StorageResult<Vec<usize>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let mut stmt = conn.prepare(
        r#"
        SELECT s.id
        FROM stories s
        LEFT JOIN story_read_status srs ON s.id = srs.story_id AND srs.peer_id = ?
        WHERE s.channel = ? AND s.public = 1 AND srs.story_id IS NULL
        ORDER BY s.created_at DESC
        "#,
    )?;

    let story_iter = stmt.query_map([peer_id, channel_name], mappers::map_row_to_usize)?;

    let mut story_ids = Vec::new();
    for story_id in story_iter {
        story_ids.push(story_id?);
    }

    Ok(story_ids)
}

/// Search stories using SQL LIKE queries with optional filters
pub async fn search_stories(
    query: &crate::types::SearchQuery,
) -> StorageResult<crate::types::SearchResults> {
    use crate::types::{SearchResult, SearchResults};

    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    // Build the search query using LIKE for text search
    let has_text_search = !query.text.trim().is_empty();
    let search_pattern = if has_text_search {
        Some(format!("%{}%", query.text.trim()))
    } else {
        None
    };

    // Base SQL query
    let mut sql = r#"
        SELECT s.id, s.name, s.header, s.body, s.public, s.channel, s.created_at
        FROM stories s
        WHERE 1=1
    "#
    .to_string();

    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    // Add text search conditions (search in name, header, and body)
    if let Some(pattern) = &search_pattern {
        sql.push_str(" AND (s.name LIKE ? OR s.header LIKE ? OR s.body LIKE ?)");
        params.push(Box::new(pattern.clone()));
        params.push(Box::new(pattern.clone()));
        params.push(Box::new(pattern.clone()));
    }

    // Add channel filter
    if let Some(channel) = &query.channel_filter {
        sql.push_str(" AND s.channel = ?");
        params.push(Box::new(channel.clone()));
    }

    // Add visibility filter
    if let Some(public_only) = query.visibility_filter {
        if public_only {
            sql.push_str(" AND s.public = 1");
        } else {
            sql.push_str(" AND s.public = 0");
        }
    }

    // Add date range filter
    if let Some(days) = query.date_range_days {
        let cutoff_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .saturating_sub((days as u64) * 24 * 60 * 60); // Subtract N days in seconds

        sql.push_str(" AND s.created_at >= ?");
        params.push(Box::new(cutoff_timestamp));
    }

    // Order by created_at (most recent first)
    sql.push_str(" ORDER BY s.created_at DESC");

    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let story_iter = stmt.query_map(param_refs.as_slice(), |row| {
        let id: usize = row.get(0)?;
        let name: String = row.get(1)?;
        let header: String = row.get(2)?;
        let body: String = row.get(3)?;
        let public: bool = utils::db_bool_to_rust(row.get::<_, i64>(4)?);
        let channel: String = row.get(5)?;
        let created_at: u64 = row.get(6)?;

        let story = crate::types::Story {
            id,
            name,
            header,
            body,
            public,
            channel,
            created_at,
            auto_share: None,
        };

        // For LIKE queries, we don't have relevance scores like FTS5
        // But we could calculate a simple relevance based on where the match occurs
        let relevance_score = if has_text_search {
            calculate_simple_relevance(&story, &query.text)
        } else {
            None
        };

        let mut search_result = SearchResult::new(story);
        if let Some(score) = relevance_score {
            search_result = search_result.with_relevance_score(score);
        }

        Ok(search_result)
    })?;

    let mut results = SearchResults::new();
    for result in story_iter {
        results.push(result?);
    }

    Ok(results)
}

/// Calculate a simple relevance score for LIKE-based search
fn calculate_simple_relevance(story: &crate::types::Story, search_term: &str) -> Option<f64> {
    if search_term.trim().is_empty() {
        return None;
    }

    let term_lower = search_term.trim().to_lowercase();
    let mut score = 0.0;

    // Check title match (highest weight)
    if story.name.to_lowercase().contains(&term_lower) {
        score += 3.0;
        if story.name.to_lowercase() == term_lower {
            score += 2.0; // Exact match bonus
        }
    }

    // Check header match (medium weight)
    if story.header.to_lowercase().contains(&term_lower) {
        score += 2.0;
    }

    // Check body match (lower weight)
    if story.body.to_lowercase().contains(&term_lower) {
        score += 1.0;
    }

    if score > 0.0 { Some(score) } else { None }
}

/// Filter stories by channel
pub async fn filter_stories_by_channel(channel: &str) -> StorageResult<crate::types::Stories> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let mut stmt = conn.prepare(
        "SELECT id, name, header, body, public, channel, created_at 
         FROM stories 
         WHERE channel = ? 
         ORDER BY created_at DESC",
    )?;

    let story_iter = stmt.query_map([channel], mappers::map_row_to_story)?;

    let mut stories = Vec::new();
    for story in story_iter {
        stories.push(story?);
    }

    Ok(stories)
}

/// Get recently created stories (within N days)
pub async fn filter_stories_by_recent_days(days: u32) -> StorageResult<crate::types::Stories> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let cutoff_timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .saturating_sub((days as u64) * 24 * 60 * 60);

    let mut stmt = conn.prepare(
        "SELECT id, name, header, body, public, channel, created_at 
         FROM stories 
         WHERE created_at >= ? 
         ORDER BY created_at DESC",
    )?;

    let story_iter = stmt.query_map([cutoff_timestamp], mappers::map_row_to_story)?;

    let mut stories = Vec::new();
    for story in story_iter {
        stories.push(story?);
    }

    Ok(stories)
}

#[cfg(test)]
mod read_status_tests {
    use super::*;
    use std::env;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_story_read_status_functionality() {
        // Setup test database
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join("test_read_status.db");
        unsafe {
            env::set_var("TEST_DATABASE_PATH", db_path.to_str().unwrap());
        }

        // Reset connection and initialize storage
        reset_db_connection_for_testing()
            .await
            .expect("Failed to reset connection");
        ensure_stories_file_exists()
            .await
            .expect("Failed to initialize storage");

        // Create test data
        let peer_id = "test_peer_123";
        let channel_name = "tech-news";

        // Create a test story
        create_new_story_with_channel("Test Story", "Header", "Body", channel_name)
            .await
            .expect("Failed to create story");

        // Create the channel if it doesn't exist
        create_channel(channel_name, "Test channel for read status", "test_system")
            .await
            .expect("Failed to create channel");

        // Make the story public so it shows up in unread counts
        let conn_arc = get_db_connection().await.expect("Failed to get connection");
        let conn = conn_arc.lock().await;
        conn.execute("UPDATE stories SET public = 1 WHERE id = 0", [])
            .expect("Failed to make story public");
        drop(conn); // Release the lock

        // Check initial unread count
        let unread_counts = get_unread_counts_by_channel(peer_id)
            .await
            .expect("Failed to get unread counts");

        // Should have 1 unread story in tech-news channel
        assert_eq!(unread_counts.get(channel_name), Some(&1));

        // Mark the story as read
        mark_story_as_read(0, peer_id, channel_name)
            .await
            .expect("Failed to mark story as read");

        // Check unread count again - should be 0 now
        let unread_counts = get_unread_counts_by_channel(peer_id)
            .await
            .expect("Failed to get unread counts");

        // Should have 0 unread stories now
        assert_eq!(unread_counts.get(channel_name), None); // No entry means 0 unread

        // Verify story is marked as read
        let is_read = is_story_read(0, peer_id)
            .await
            .expect("Failed to check read status");
        assert!(is_read);

        println!("✅ Read status functionality test passed!");
    }
}
