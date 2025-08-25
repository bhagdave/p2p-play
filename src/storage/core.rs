use crate::errors::StorageResult;
use crate::storage::{mappers, utils};
use crate::types::{
    BootstrapConfig, Channel, Channels, DirectMessage, Stories, Story, UnifiedNetworkConfig,
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

fn get_database_path() -> String {
    if let Ok(test_path) = std::env::var("TEST_DATABASE_PATH") {
        return test_path;
    }

    if let Ok(db_path) = std::env::var("DATABASE_PATH") {
        return db_path;
    }

    "./stories.db".to_string()
}

type DbPool = Pool<SqliteConnectionManager>;

static DB_POOL_STATE: once_cell::sync::Lazy<RwLock<Option<(DbPool, String)>>> =
    once_cell::sync::Lazy::new(|| RwLock::new(None));

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

fn create_db_pool(db_path: &str) -> StorageResult<DbPool> {
    let config = PoolConfig::default();

    let manager = SqliteConnectionManager::file(db_path).with_init(|conn| {
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
                 PRAGMA synchronous = NORMAL;
                 PRAGMA cache_size = -64000;
                 PRAGMA temp_store = MEMORY;
                 PRAGMA journal_mode = WAL;",
        )?;
        Ok(())
    });

    let pool = Pool::builder()
        .max_size(config.max_size)
        .min_idle(config.min_idle)
        .connection_timeout(config.connection_timeout)
        .idle_timeout(Some(config.idle_timeout))
        .build(manager)
        .map_err(|e| format!("Failed to create database pool: {e}"))?;

    Ok(pool)
}

pub async fn get_db_connection() -> StorageResult<Arc<Mutex<Connection>>> {
    let current_path = get_database_path();

    {
        let state = DB_POOL_STATE.read().await;
        if let Some((pool, stored_path)) = state.as_ref() {
            if stored_path == &current_path {
                let _pooled_conn = pool
                    .get()
                    .map_err(|e| format!("Failed to get connection from pool: {e}"))?;

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

    let pool = create_db_pool(&current_path)?;

    let conn = Connection::open(&current_path)?;
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA synchronous = NORMAL;
         PRAGMA cache_size = -64000;
         PRAGMA temp_store = MEMORY;
         PRAGMA journal_mode = WAL;",
    )?;
    let conn_arc = Arc::new(Mutex::new(conn));

    {
        let mut state = DB_POOL_STATE.write().await;
        *state = Some((pool, current_path));
    }

    Ok(conn_arc)
}

pub async fn reset_db_connection_for_testing() -> StorageResult<()> {
    let mut state = DB_POOL_STATE.write().await;
    *state = None;
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    Ok(())
}

pub async fn with_transaction<F, R>(f: F) -> StorageResult<R>
where
    F: FnOnce(&Connection) -> StorageResult<R>,
{
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    conn.execute("BEGIN TRANSACTION", [])?;

    match f(&conn) {
        Ok(result) => {
            conn.execute("COMMIT", [])?;
            Ok(result)
        }
        Err(e) => {
            let _ = conn.execute("ROLLBACK", []);
            Err(e)
        }
    }
}

pub async fn with_read_transaction<F, R>(f: F) -> StorageResult<R>
where
    F: FnOnce(&Connection) -> StorageResult<R>,
{
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    conn.execute("BEGIN DEFERRED", [])?;

    match f(&conn) {
        Ok(result) => {
            conn.execute("COMMIT", [])?;
            Ok(result)
        }
        Err(e) => {
            let _ = conn.execute("ROLLBACK", []);
            Err(e)
        }
    }
}

async fn create_tables() -> StorageResult<()> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    migrations::create_tables(&conn)?;
    Ok(())
}

pub async fn ensure_stories_file_exists() -> StorageResult<()> {
    let db_path = get_database_path();

    if let Some(parent) = std::path::Path::new(&db_path).parent() {
        if !parent.exists() {
            tokio::fs::create_dir_all(parent).await?;
        }
    }

    let _conn = get_db_connection().await?;
    create_tables().await?;
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

pub async fn read_local_stories_for_sync(
    last_sync_timestamp: u64,
    subscribed_channels: &[String],
) -> StorageResult<Stories> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let mut query = "SELECT id, name, header, body, public, channel, created_at FROM stories WHERE public = 1 AND created_at > ?".to_string();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(last_sync_timestamp as i64)];

    if !subscribed_channels.is_empty() {
        let placeholders = vec!["?"; subscribed_channels.len()].join(",");
        query.push_str(&format!(" AND channel IN ({placeholders})"));
        for channel in subscribed_channels {
            params.push(Box::new(channel.clone()));
        }
    }

    query.push_str(" ORDER BY created_at DESC");

    let mut stmt = conn.prepare(&query)?;

    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let story_iter = stmt.query_map(param_refs.as_slice(), mappers::map_row_to_story)?;

    let mut stories = Vec::new();
    for story in story_iter {
        stories.push(story?);
    }

    Ok(stories)
}

pub async fn get_channels_for_stories(stories: &[Story]) -> StorageResult<Channels> {
    if stories.is_empty() {
        return Ok(Vec::new());
    }

    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    // Get unique channel names from stories
    let mut unique_channels: std::collections::HashSet<String> = std::collections::HashSet::new();
    for story in stories {
        unique_channels.insert(story.channel.clone());
    }

    if unique_channels.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = vec!["?"; unique_channels.len()].join(",");
    let query = format!(
        "SELECT name, description, created_by, created_at FROM channels WHERE name IN ({placeholders}) ORDER BY name"
    );

    let mut stmt = conn.prepare(&query)?;

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

    for channel_name in unique_channels {
        if !found_channel_names.contains(&channel_name) {
            let default_channel = crate::types::Channel::new(
                channel_name.clone(),
                format!("Channel: {}", channel_name),
                "unknown".to_string(),
            );
            channels.push(default_channel);
        }
    }

    Ok(channels)
}

pub async fn process_discovered_channels(channels: &[Channel]) -> StorageResult<usize> {
    if channels.is_empty() {
        return Ok(0);
    }

    let mut saved_count = 0;
    for channel in channels {
        if channel.name.is_empty() || channel.description.is_empty() {
            continue;
        }

        match channel_exists(&channel.name).await {
            Ok(true) => {}
            Ok(false) => {
                match create_channel(&channel.name, &channel.description, &channel.created_by).await
                {
                    Ok(_) => {
                        saved_count += 1;
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

    Ok(saved_count)
}

pub async fn create_new_story_with_channel(
    name: &str,
    header: &str,
    body: &str,
    channel: &str,
) -> StorageResult<()> {
    let conn_arc = get_db_connection().await?;

    let next_id = utils::get_next_id(&conn_arc, "stories").await?;

    let created_at = utils::get_current_timestamp();

    let should_be_public = match load_unified_network_config().await {
        Ok(config) => config.auto_share.global_auto_share,
        Err(_) => true, // Default to true if config can't be loaded
    };

    let conn = conn_arc.lock().await;
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
    Ok(())
}

pub async fn publish_story(
    id: usize,
    sender: tokio::sync::mpsc::UnboundedSender<Story>,
) -> StorageResult<()> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let rows_affected = conn.execute(
        "UPDATE stories SET public = ? WHERE id = ?",
        [&utils::rust_bool_to_db(true), &id.to_string()],
    )?;

    if rows_affected > 0 {
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

    let rows_affected = conn.execute("DELETE FROM stories WHERE id = ?", [&id.to_string()])?;

    if rows_affected > 0 {
        Ok(true)
    } else {
        Ok(false)
    }
}

pub async fn save_received_story(story: Story) -> StorageResult<()> {
    let conn_arc = get_db_connection().await?;

    {
        let conn = conn_arc.lock().await;
        let mut stmt =
            conn.prepare("SELECT id FROM stories WHERE name = ? AND header = ? AND body = ?")?;
        let existing = stmt.query_row(
            [&story.name, &story.header, &story.body],
            mappers::map_row_to_i64,
        );

        if existing.is_ok() {
            return Ok(());
        }
    }

    let new_id = utils::get_next_id(&conn_arc, "stories").await?;

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

    Ok(())
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

pub async fn create_channel(name: &str, description: &str, created_by: &str) -> StorageResult<()> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let timestamp = utils::get_current_timestamp();

    conn.execute(
        "INSERT OR IGNORE INTO channels (name, description, created_by, created_at) VALUES (?, ?, ?, ?)",
        [name, description, created_by, &timestamp.to_string()],
    )?;

    Ok(())
}

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

    Ok(())
}

pub async fn unsubscribe_from_channel(peer_id: &str, channel_name: &str) -> StorageResult<()> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    conn.execute(
        "DELETE FROM channel_subscriptions WHERE peer_id = ? AND channel_name = ?",
        [peer_id, channel_name],
    )?;

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

pub async fn get_auto_subscription_count(peer_id: &str) -> StorageResult<usize> {
    let subscribed = read_subscribed_channels(peer_id).await?;
    Ok(subscribed.len())
}

pub async fn save_node_description(description: &str) -> StorageResult<()> {
    if description.len() > 1024 {
        return Err("Description exceeds 1024 bytes limit".into());
    }

    fs::write(NODE_DESCRIPTION_FILE_PATH, description).await?;
    Ok(())
}

pub async fn load_node_description() -> StorageResult<Option<String>> {
    match fs::read_to_string(NODE_DESCRIPTION_FILE_PATH).await {
        Ok(content) => {
            if content.is_empty() {
                Ok(None)
            } else {
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

pub async fn save_bootstrap_config(config: &BootstrapConfig) -> StorageResult<()> {
    save_bootstrap_config_to_path(config, "bootstrap_config.json").await
}

pub async fn save_bootstrap_config_to_path(
    config: &BootstrapConfig,
    path: &str,
) -> StorageResult<()> {
    config.validate()?;

    let json = serde_json::to_string_pretty(config)?;
    fs::write(path, json).await?;
    Ok(())
}

pub async fn load_bootstrap_config() -> StorageResult<BootstrapConfig> {
    load_bootstrap_config_from_path("bootstrap_config.json").await
}

pub async fn load_bootstrap_config_from_path(path: &str) -> StorageResult<BootstrapConfig> {
    match fs::read_to_string(path).await {
        Ok(content) => {
            let config: BootstrapConfig = serde_json::from_str(&content)?;

            config.validate()?;

            Ok(config)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let default_config = BootstrapConfig::default();

            save_bootstrap_config_to_path(&default_config, path).await?;

            Ok(default_config)
        }
        Err(e) => Err(e.into()),
    }
}

pub async fn save_unified_network_config(config: &UnifiedNetworkConfig) -> StorageResult<()> {
    save_unified_network_config_to_path(config, "unified_network_config.json").await
}

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

pub async fn load_unified_network_config() -> StorageResult<UnifiedNetworkConfig> {
    load_unified_network_config_from_path("unified_network_config.json").await
}

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
            Ok(default_config)
        }
    }
}

pub async fn ensure_unified_network_config_exists() -> StorageResult<()> {
    if tokio::fs::metadata("unified_network_config.json")
        .await
        .is_err()
    {
        let default_config = UnifiedNetworkConfig::default();
        save_unified_network_config(&default_config).await?;
    }
    Ok(())
}

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

    Ok(())
}

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

pub async fn is_story_read(story_id: usize, peer_id: &str) -> StorageResult<bool> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let mut stmt =
        conn.prepare("SELECT COUNT(*) FROM story_read_status WHERE story_id = ? AND peer_id = ?")?;

    let count: i64 = stmt.query_row([&story_id.to_string(), peer_id], mappers::map_row_to_i64)?;

    Ok(count > 0)
}

pub async fn search_stories(
    query: &crate::types::SearchQuery,
) -> StorageResult<crate::types::SearchResults> {
    use crate::types::{SearchResult, SearchResults};

    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let has_text_search = !query.text.trim().is_empty();
    let search_pattern = if has_text_search {
        Some(format!("%{}%", query.text.trim()))
    } else {
        None
    };

    let mut sql = r#"
        SELECT s.id, s.name, s.header, s.body, s.public, s.channel, s.created_at
        FROM stories s
        WHERE 1=1
    "#
    .to_string();

    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(pattern) = &search_pattern {
        sql.push_str(" AND (s.name LIKE ? OR s.header LIKE ? OR s.body LIKE ?)");
        params.push(Box::new(pattern.clone()));
        params.push(Box::new(pattern.clone()));
        params.push(Box::new(pattern.clone()));
    }

    if let Some(channel) = &query.channel_filter {
        sql.push_str(" AND s.channel = ?");
        params.push(Box::new(channel.clone()));
    }

    if let Some(public_only) = query.visibility_filter {
        if public_only {
            sql.push_str(" AND s.public = 1");
        } else {
            sql.push_str(" AND s.public = 0");
        }
    }

    if let Some(days) = query.date_range_days {
        let cutoff_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .saturating_sub((days as u64) * 24 * 60 * 60); // Subtract N days in seconds

        sql.push_str(" AND s.created_at >= ?");
        params.push(Box::new(cutoff_timestamp));
    }

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

fn calculate_simple_relevance(story: &crate::types::Story, search_term: &str) -> Option<f64> {
    if search_term.trim().is_empty() {
        return None;
    }

    let term_lower = search_term.trim().to_lowercase();
    let mut score = 0.0;

    if story.name.to_lowercase().contains(&term_lower) {
        score += 3.0;
        if story.name.to_lowercase() == term_lower {
            score += 2.0; // Exact match bonus
        }
    }

    if story.header.to_lowercase().contains(&term_lower) {
        score += 2.0;
    }

    if story.body.to_lowercase().contains(&term_lower) {
        score += 1.0;
    }

    if score > 0.0 { Some(score) } else { None }
}

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

pub async fn save_direct_message(message: &crate::types::DirectMessage) -> StorageResult<()> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    conn.execute(
        "INSERT INTO direct_messages (from_peer_id, from_name, to_name, message, timestamp, is_read) VALUES (?, ?, ?, ?, ?, ?)",
        [
            &message.from_peer_id,
            &message.from_name,
            &message.to_name,
            &message.message,
            &message.timestamp.to_string(),
            &utils::rust_bool_to_db(false), // New messages start as unread
        ],
    )?;

    Ok(())
}

pub async fn load_conversation_messages(
    peer_id: &str,
    local_peer_name: &str,
) -> StorageResult<Vec<crate::types::DirectMessage>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let mut stmt = conn.prepare(
        r#"
        SELECT from_peer_id, from_name, to_name, message, timestamp
        FROM direct_messages
        WHERE 
            (from_peer_id = ? AND to_name = ?) OR 
            (from_name = ? AND to_name = ?) OR
            (? LIKE 'name:%' AND (
                (from_name = ? AND to_name = SUBSTR(?, 6)) OR
                (from_name = SUBSTR(?, 6) AND to_name = ?)
            ))
        ORDER BY timestamp ASC
        "#,
    )?;

    let message_iter = stmt.query_map(
        [peer_id, local_peer_name, peer_id, local_peer_name, peer_id, local_peer_name, peer_id, peer_id, local_peer_name],
        |row| {
            Ok(crate::types::DirectMessage {
                from_peer_id: row.get(0)?,
                from_name: row.get(1)?,
                to_name: row.get(2)?,
                message: row.get(3)?,
                timestamp: row.get::<_, i64>(4)? as u64,
            })
        },
    )?;

    let mut messages = Vec::new();
    for message in message_iter {
        messages.push(message?);
    }

    Ok(messages)
}

pub async fn mark_conversation_messages_as_read(
    peer_id: &str,
    local_peer_name: &str,
) -> StorageResult<()> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    conn.execute(
        r#"
        UPDATE direct_messages 
        SET is_read = ? 
        WHERE (from_peer_id = ? AND to_name = ?) OR (from_name = ? AND to_name = ?)
        "#,
        [
            &utils::rust_bool_to_db(true),
            peer_id,
            local_peer_name,
            peer_id,
            local_peer_name,
        ],
    )?;

    Ok(())
}

pub async fn get_conversations_with_unread_counts(
    local_peer_name: &str,
) -> StorageResult<Vec<(String, String, usize, u64)>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;

    let mut stmt = conn.prepare(
        r#"
        SELECT 
            peer_name,
            peer_id,
            unread_count,
            last_activity
        FROM (
            SELECT 
                CASE 
                    WHEN from_name = ? THEN to_name 
                    ELSE from_name 
                END as peer_name,
                CASE 
                    WHEN from_name = ? THEN 
                        COALESCE(
                            (SELECT DISTINCT from_peer_id FROM direct_messages d2 
                             WHERE d2.from_name = dm.to_name AND d2.to_name = ? LIMIT 1),
                            'name:' || to_name
                        )
                    ELSE from_peer_id 
                END as peer_id,
                COUNT(CASE WHEN is_read = 0 AND from_name != ? THEN 1 END) as unread_count,
                MAX(timestamp) as last_activity
            FROM direct_messages dm
            WHERE from_name = ? OR to_name = ?
            GROUP BY 
                CASE 
                    WHEN from_name = ? THEN to_name 
                    ELSE from_name 
                END,
                CASE 
                    WHEN from_name = ? THEN 
                        COALESCE(
                            (SELECT DISTINCT from_peer_id FROM direct_messages d2 
                             WHERE d2.from_name = dm.to_name AND d2.to_name = ? LIMIT 1),
                            'name:' || to_name
                        )
                    ELSE from_peer_id 
                END
        )
        ORDER BY last_activity DESC
        "#,
    )?;

    let conversation_iter = stmt.query_map(
        [
            local_peer_name,  // First peer_name CASE condition
            local_peer_name,  // First peer_id CASE condition
            local_peer_name,  // Subquery in peer_id CASE
            local_peer_name,  // Unread count condition
            local_peer_name,  // First WHERE condition
            local_peer_name,  // Second WHERE condition
            local_peer_name,  // Second peer_name CASE condition (in GROUP BY)
            local_peer_name,  // Second peer_id CASE condition (in GROUP BY)
            local_peer_name,  // Second subquery in peer_id CASE (in GROUP BY)
        ],
        |row| {
            Ok((
                row.get::<_, String>(1)?,       // peer_id
                row.get::<_, String>(0)?,       // peer_name
                row.get::<_, i64>(2)? as usize, // unread_count
                row.get::<_, i64>(3)? as u64,   // last_activity
            ))
        },
    )?;

    let mut conversations = Vec::new();
    for conversation in conversation_iter {
        conversations.push(conversation?);
    }

    Ok(conversations)
}

pub async fn load_conversation_manager(
    local_peer_name: &str,
) -> StorageResult<crate::types::ConversationManager> {
    let mut conversation_manager = crate::types::ConversationManager::new();

    let conversations_data = get_conversations_with_unread_counts(local_peer_name).await?;

    for (peer_id, peer_name, unread_count, last_activity) in conversations_data {
        let messages = load_conversation_messages(&peer_id, local_peer_name).await?;

        if !messages.is_empty() {
            let mut conversation =
                crate::types::Conversation::new(peer_id.clone(), peer_name.clone());
            conversation.messages = messages;
            conversation.unread_count = unread_count;
            conversation.last_activity = last_activity;

            conversation_manager
                .conversations
                .insert(peer_id, conversation);
        }
    }

    Ok(conversation_manager)
}
