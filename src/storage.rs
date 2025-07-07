use crate::types::{Stories, Story};
use log::{error, info};
use rusqlite::Connection;
use std::error::Error;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::Mutex;

use crate::migrations;

const DATABASE_PATH: &str = "./stories.db";
const PEER_NAME_FILE_PATH: &str = "./peer_name.json"; // Keep for backward compatibility

// Thread-safe database connection
static DB_CONN: once_cell::sync::OnceCell<Arc<Mutex<Connection>>> = once_cell::sync::OnceCell::new();

async fn get_db_connection() -> Result<Arc<Mutex<Connection>>, Box<dyn Error>> {
    if let Some(conn) = DB_CONN.get() {
        Ok(conn.clone())
    } else {
        info!("Creating new SQLite database connection: {}", DATABASE_PATH);
        
        // Create the database file and connection
        let conn = Connection::open(DATABASE_PATH)?;
        let conn_arc = Arc::new(Mutex::new(conn));
        
        // Initialize the connection in the static variable
        DB_CONN.set(conn_arc.clone()).map_err(|_| "Failed to initialize database connection")?;
        
        info!("Successfully connected to SQLite database");
        Ok(conn_arc)
    }
}

async fn create_tables() -> Result<(), Box<dyn Error>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;
    
    migrations::create_tables(&conn)?;
    Ok(())
}

pub async fn ensure_stories_file_exists() -> Result<(), Box<dyn Error>> {
    info!("Initializing SQLite database at: {}", DATABASE_PATH);
    
    // Ensure the directory exists for the database file
    if let Some(parent) = std::path::Path::new(DATABASE_PATH).parent() {
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
    
    let mut stmt = conn.prepare("SELECT id, name, header, body, public FROM stories ORDER BY id")?;
    let story_iter = stmt.query_map([], |row| {
        Ok(Story {
            id: row.get::<_, i64>(0)? as usize,
            name: row.get(1)?,
            header: row.get(2)?,
            body: row.get(3)?,
            public: row.get::<_, i64>(4)? != 0, // Convert integer to boolean
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
        
        let mut stmt = conn.prepare("SELECT id, name, header, body, public FROM stories ORDER BY id")?;
        let story_iter = stmt.query_map([], |row| {
            Ok(Story {
                id: row.get::<_, i64>(0)? as usize,
                name: row.get(1)?,
                header: row.get(2)?,
                body: row.get(3)?,
                public: row.get::<_, i64>(4)? != 0, // Convert integer to boolean
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
            "INSERT INTO stories (id, name, header, body, public) VALUES (?, ?, ?, ?, ?)",
            [
                &story.id.to_string(),
                &story.name,
                &story.header,
                &story.body,
                &(if story.public { "1".to_string() } else { "0".to_string() }),
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
                "INSERT INTO stories (id, name, header, body, public) VALUES (?, ?, ?, ?, ?)",
                [
                    &story.id.to_string(),
                    &story.name,
                    &story.header,
                    &story.body,
                    &(if story.public { "1".to_string() } else { "0".to_string() }),
                ],
            )?;
        }
        
        Ok(())
    }
}

pub async fn create_new_story(name: &str, header: &str, body: &str) -> Result<(), Box<dyn Error>> {
    let conn_arc = get_db_connection().await?;
    let conn = conn_arc.lock().await;
    
    // Get the next ID
    let mut stmt = conn.prepare("SELECT COALESCE(MAX(id), -1) + 1 as next_id FROM stories")?;
    let next_id: i64 = stmt.query_row([], |row| row.get(0))?;
    
    // Insert the new story
    conn.execute(
        "INSERT INTO stories (id, name, header, body, public) VALUES (?, ?, ?, ?, ?)",
        [
            &next_id.to_string(),
            name,
            header,
            body,
            "0", // New stories start as private (0 = false)
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
        let mut stmt = conn.prepare("SELECT id, name, header, body, public FROM stories WHERE id = ?")?;
        let story_result = stmt.query_row([&id.to_string()], |row| {
            Ok(Story {
                id: row.get::<_, i64>(0)? as usize,
                name: row.get(1)?,
                header: row.get(2)?,
                body: row.get(3)?,
                public: row.get::<_, i64>(4)? != 0, // Convert integer to boolean
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
    let mut stmt = conn.prepare(
        "SELECT id FROM stories WHERE name = ? AND header = ? AND body = ?"
    )?;
    let existing = stmt.query_row([&story.name, &story.header, &story.body], |row| {
        Ok(row.get::<_, i64>(0)?)
    });
    
    if existing.is_err() { // Story doesn't exist
        // Get the next ID
        let mut stmt = conn.prepare("SELECT COALESCE(MAX(id), -1) + 1 as next_id FROM stories")?;
        let new_id: i64 = stmt.query_row([], |row| row.get(0))?;
        
        // Insert the story with the new ID and mark as public
        conn.execute(
            "INSERT INTO stories (id, name, header, body, public) VALUES (?, ?, ?, ?, ?)",
            [
                &new_id.to_string(),
                &story.name,
                &story.header,
                &story.body,
                "1", // Mark as public since it was published (1 = true)
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
    conn.execute("INSERT OR REPLACE INTO peer_name (id, name) VALUES (1, ?)", [name])?;
    
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
    let result = stmt.query_row([], |row| {
        Ok(row.get::<_, String>(0)?)
    });
    
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
            },
            Story {
                id: 2,
                name: "Another Story".to_string(),
                header: "Another Header".to_string(),
                body: "Another Body".to_string(),
                public: false,
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
}
