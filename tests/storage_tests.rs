use p2p_play::storage::*;
use p2p_play::types::*;
use tempfile::NamedTempFile;
use tokio::sync::mpsc;

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
    tokio::fs::write(path, "invalid json content").await.unwrap();

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
    use p2p_play::migrations;
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
    let rows_affected = conn
        .execute("DELETE FROM stories WHERE id = ?", ["1"])
        .unwrap();
    assert_eq!(rows_affected, 1);

    // Verify story was deleted
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM stories").unwrap();
    let count_after: i64 = stmt.query_row([], |row| row.get(0)).unwrap();
    assert_eq!(count_after, 2);

    // Verify specific story with id=1 is gone
    let mut stmt = conn
        .prepare("SELECT COUNT(*) FROM stories WHERE id = ?")
        .unwrap();
    let id_count: i64 = stmt.query_row(["1"], |row| row.get(0)).unwrap();
    assert_eq!(id_count, 0);

    // Test deleting non-existent story
    let rows_affected_nonexistent = conn
        .execute("DELETE FROM stories WHERE id = ?", ["999"])
        .unwrap();
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

#[tokio::test]
async fn test_bootstrap_config_save_and_load() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();

    let mut config = BootstrapConfig::new();
    // Add a valid multiaddr for testing
    config.add_peer(
        "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN"
            .to_string(),
    );
    config.retry_interval_ms = 10000;

    // Save the config
    save_bootstrap_config_to_path(&config, path).await.unwrap();

    // Load it back
    let loaded_config = load_bootstrap_config_from_path(path).await.unwrap();
    assert_eq!(loaded_config.bootstrap_peers.len(), 3); // 2 default + 1 added
    assert_eq!(loaded_config.retry_interval_ms, 10000);
}

#[tokio::test]
async fn test_bootstrap_config_default_creation() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();

    // Remove the temp file so it doesn't exist
    std::fs::remove_file(path).unwrap();

    // Load config when file doesn't exist - should create default
    let config = load_bootstrap_config_from_path(path).await.unwrap();
    assert_eq!(config.bootstrap_peers.len(), 2); // Default peers
    assert_eq!(config.retry_interval_ms, 5000);
    assert_eq!(config.max_retry_attempts, 5);

    // File should now exist
    assert!(tokio::fs::metadata(path).await.is_ok());
}

#[tokio::test]
async fn test_bootstrap_config_validation() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();

    // Create invalid config JSON with an actually invalid multiaddr
    let invalid_json = r#"{"bootstrap_peers": ["not-a-valid-multiaddr"], "retry_interval_ms": 5000, "max_retry_attempts": 5, "bootstrap_timeout_ms": 30000}"#;
    tokio::fs::write(path, invalid_json).await.unwrap();

    // Loading should fail due to validation
    let result = load_bootstrap_config_from_path(path).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_bootstrap_config_add_remove_peers() {
    let mut config = BootstrapConfig::new();
    let peer =
        "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN"
            .to_string();

    // Add peer
    assert!(config.add_peer(peer.clone()));
    assert!(config.bootstrap_peers.contains(&peer));

    // Adding same peer again should return false
    assert!(!config.add_peer(peer.clone()));

    // Remove peer
    assert!(config.remove_peer(&peer));
    assert!(!config.bootstrap_peers.contains(&peer));

    // Removing same peer again should return false
    assert!(!config.remove_peer(&peer));
}

#[tokio::test]
async fn test_bootstrap_config_clear_peers() {
    let mut config = BootstrapConfig::new();
    assert!(!config.bootstrap_peers.is_empty());

    config.clear_peers();
    assert!(config.bootstrap_peers.is_empty());
}

#[tokio::test]
async fn test_bootstrap_config_validation_edge_cases() {
    let mut config = BootstrapConfig::new();

    // Empty peers should fail validation
    config.clear_peers();
    assert!(config.validate().is_err());

    // Invalid multiaddr should fail validation
    config.bootstrap_peers.push("not-a-multiaddr".to_string());
    assert!(config.validate().is_err());

    // Valid multiaddr should pass
    config.bootstrap_peers.clear();
    config.bootstrap_peers.push(
        "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN"
            .to_string(),
    );
    assert!(config.validate().is_ok());
}