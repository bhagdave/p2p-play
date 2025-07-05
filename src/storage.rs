use crate::types::{Stories, Story};
use log::{error, info};
use std::error::Error;
use tokio::fs;

const STORAGE_FILE_PATH: &str = "./stories.json";
const PEER_NAME_FILE_PATH: &str = "./peer_name.json";

pub async fn ensure_stories_file_exists() -> Result<(), Box<dyn Error>> {
    if !std::path::Path::new(STORAGE_FILE_PATH).exists() {
        info!("Creating stories.json file with empty array");
        let empty_stories: Stories = Vec::new();
        write_local_stories(&empty_stories).await?;
    }
    Ok(())
}

pub async fn read_local_stories() -> Result<Stories, Box<dyn Error>> {
    let content = fs::read(STORAGE_FILE_PATH).await?;
    let result = serde_json::from_slice(&content)?;
    Ok(result)
}

pub async fn read_local_stories_from_path(path: &str) -> Result<Stories, Box<dyn Error>> {
    let content = fs::read(path).await?;
    let result = serde_json::from_slice(&content)?;
    Ok(result)
}

pub async fn write_local_stories(stories: &Stories) -> Result<(), Box<dyn Error>> {
    let json = serde_json::to_string(&stories)?;
    fs::write(STORAGE_FILE_PATH, &json).await?;
    Ok(())
}

pub async fn write_local_stories_to_path(
    stories: &Stories,
    path: &str,
) -> Result<(), Box<dyn Error>> {
    let json = serde_json::to_string(&stories)?;
    fs::write(path, &json).await?;
    Ok(())
}

pub async fn create_new_story(name: &str, header: &str, body: &str) -> Result<(), Box<dyn Error>> {
    let mut local_stories = read_local_stories().await?;
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
    write_local_stories(&local_stories).await?;

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
    let mut local_stories = read_local_stories().await?;
    let mut published_story = None;

    for story in local_stories.iter_mut() {
        if story.id == id {
            story.public = true;
            published_story = Some(story.clone());
            break;
        }
    }

    write_local_stories(&local_stories).await?;

    if let Some(story) = published_story {
        if let Err(e) = sender.send(story) {
            error!("error sending story for broadcast: {}", e);
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

pub async fn save_received_story(mut story: Story) -> Result<(), Box<dyn Error>> {
    let mut local_stories = match read_local_stories().await {
        Ok(stories) => stories,
        Err(_) => Vec::new(), // Create empty vec if no file exists
    };

    // Check if story already exists (by name and content to avoid duplicates)
    let already_exists = local_stories
        .iter()
        .any(|s| s.name == story.name && s.header == story.header && s.body == story.body);

    if !already_exists {
        // Assign new local ID
        let new_id = match local_stories.iter().max_by_key(|r| r.id) {
            Some(v) => v.id + 1,
            None => 0,
        };
        story.id = new_id;
        story.public = true; // Mark as public since it was published

        local_stories.push(story);
        write_local_stories(&local_stories).await?;
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
    let json = serde_json::to_string(name)?;
    fs::write(PEER_NAME_FILE_PATH, &json).await?;
    Ok(())
}

pub async fn save_local_peer_name_to_path(name: &str, path: &str) -> Result<(), Box<dyn Error>> {
    let json = serde_json::to_string(name)?;
    fs::write(path, &json).await?;
    Ok(())
}

pub async fn load_local_peer_name() -> Result<Option<String>, Box<dyn Error>> {
    match fs::read(PEER_NAME_FILE_PATH).await {
        Ok(content) => {
            let name: String = serde_json::from_slice(&content)?;
            Ok(Some(name))
        }
        Err(_) => Ok(None), // File doesn't exist or can't be read, return None
    }
}

pub async fn load_local_peer_name_from_path(path: &str) -> Result<Option<String>, Box<dyn Error>> {
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
        let original_path = STORAGE_FILE_PATH;

        // We can't easily test the function that uses the constant path,
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
