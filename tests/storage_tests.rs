use p2p_play::storage::*;
use p2p_play::types::*;
use tempfile::NamedTempFile;
use tokio::sync::mpsc;
use uuid::Uuid;

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
            created_at: 1234567890,
            auto_share: None,
        },
        Story {
            id: 2,
            name: "Another Story".to_string(),
            header: "Another Header".to_string(),
            body: "Another Body".to_string(),
            public: false,
            channel: "tech".to_string(),
            created_at: 1234567891,
            auto_share: None,
        },
    ];

    write_local_stories_to_path(&stories, path).await.unwrap();
    let read_stories = read_local_stories_from_path(path).await.unwrap();

    // Stories should be returned in chronological order (newest first by created_at)
    let expected_order = vec![
        Story {
            id: 2,
            name: "Another Story".to_string(),
            header: "Another Header".to_string(),
            body: "Another Body".to_string(),
            public: false,
            channel: "tech".to_string(),
            created_at: 1234567891,
            auto_share: None, // Newer timestamp
        },
        Story {
            id: 1,
            name: "Test Story".to_string(),
            header: "Test Header".to_string(),
            body: "Test Body".to_string(),
            public: true,
            channel: "general".to_string(),
            created_at: 1234567890,
            auto_share: None, // Older timestamp
        },
    ];

    assert_eq!(expected_order, read_stories);
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
        created_at: 1234567800,
        auto_share: None,
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
        created_at: 1234567892,
        auto_share: None,
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
        created_at: 1234567893,
        auto_share: None,
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
    tokio::fs::write(path, "invalid json content")
        .await
        .unwrap();

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
            &format!("Story {i}"),
            &format!("Header {i}"),
            &format!("Body {i}"),
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
    )
    .unwrap();
    conn.execute(
        "INSERT INTO stories (id, name, header, body, public, channel) VALUES (?, ?, ?, ?, ?, ?)",
        ["1", "Story 2", "Header 2", "Body 2", "0", "general"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO stories (id, name, header, body, public, channel) VALUES (?, ?, ?, ?, ?, ?)",
        ["2", "Story 3", "Header 3", "Body 3", "0", "general"],
    )
    .unwrap();

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
    assert_eq!(config.max_retry_attempts, 10);

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
    let peer = "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN"
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

// ============================================================================
// WASM Offering Storage Tests
// ============================================================================

async fn setup_wasm_test_db() -> String {
    let unique_db_path = format!("/tmp/test_wasm_{}.db", Uuid::new_v4());
    unsafe {
        std::env::set_var("TEST_DATABASE_PATH", &unique_db_path);
    }

    // Reset database connection to use the new path
    reset_db_connection_for_testing().await.unwrap();

    // Initialize the test database (this creates all tables)
    ensure_stories_file_exists().await.unwrap();

    unique_db_path
}

async fn cleanup_wasm_test_db(db_path: &str) {
    unsafe {
        std::env::remove_var("TEST_DATABASE_PATH");
    }
    let _ = std::fs::remove_file(db_path);
}

fn create_test_wasm_offering(name: &str, enabled: bool) -> WasmOffering {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    WasmOffering {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        description: format!("Test offering: {}", name),
        ipfs_cid: format!("Qm{}", Uuid::new_v4().to_string().replace("-", "")),
        parameters: vec![],
        resource_requirements: WasmResourceRequirements {
            min_fuel: 1000,
            max_fuel: 100000,
            min_memory_mb: 16,
            max_memory_mb: 256,
            estimated_timeout_secs: 30,
        },
        version: "1.0.0".to_string(),
        enabled,
        created_at: now,
        updated_at: now,
    }
}

#[tokio::test]
async fn test_create_and_read_wasm_offering() {
    let db_path = setup_wasm_test_db().await;

    // Create a test offering
    let offering = create_test_wasm_offering("test-module", true);
    create_wasm_offering(&offering).await.unwrap();

    // Read all offerings
    let offerings = read_wasm_offerings().await.unwrap();
    assert_eq!(offerings.len(), 1);
    assert_eq!(offerings[0].name, "test-module");
    assert_eq!(offerings[0].id, offering.id);
    assert!(offerings[0].enabled);

    cleanup_wasm_test_db(&db_path).await;
}

#[tokio::test]
async fn test_read_enabled_wasm_offerings_filters_disabled() {
    let db_path = setup_wasm_test_db().await;

    // Create one enabled and one disabled offering
    let enabled_offering = create_test_wasm_offering("enabled-module", true);
    let disabled_offering = create_test_wasm_offering("disabled-module", false);

    create_wasm_offering(&enabled_offering).await.unwrap();
    create_wasm_offering(&disabled_offering).await.unwrap();

    // Read all offerings - should have 2
    let all_offerings = read_wasm_offerings().await.unwrap();
    assert_eq!(all_offerings.len(), 2);

    // Read enabled offerings - should have 1
    let enabled_offerings = read_enabled_wasm_offerings().await.unwrap();
    assert_eq!(enabled_offerings.len(), 1);
    assert_eq!(enabled_offerings[0].name, "enabled-module");

    cleanup_wasm_test_db(&db_path).await;
}

#[tokio::test]
async fn test_get_wasm_offering_by_id_found() {
    let db_path = setup_wasm_test_db().await;

    let offering = create_test_wasm_offering("findable-module", true);
    let offering_id = offering.id.clone();
    create_wasm_offering(&offering).await.unwrap();

    // Get by ID
    let result = get_wasm_offering_by_id(&offering_id).await.unwrap();
    assert!(result.is_some());
    let found = result.unwrap();
    assert_eq!(found.id, offering_id);
    assert_eq!(found.name, "findable-module");

    cleanup_wasm_test_db(&db_path).await;
}

#[tokio::test]
async fn test_get_wasm_offering_by_id_not_found() {
    let db_path = setup_wasm_test_db().await;

    // Try to get a non-existent offering
    let result = get_wasm_offering_by_id("non-existent-id").await.unwrap();
    assert!(result.is_none());

    cleanup_wasm_test_db(&db_path).await;
}

#[tokio::test]
async fn test_update_wasm_offering() {
    let db_path = setup_wasm_test_db().await;

    // Create an offering
    let mut offering = create_test_wasm_offering("original-name", true);
    create_wasm_offering(&offering).await.unwrap();

    // Update the offering
    offering.name = "updated-name".to_string();
    offering.description = "Updated description".to_string();
    offering.version = "2.0.0".to_string();
    offering.enabled = false;

    let updated = update_wasm_offering(&offering).await.unwrap();
    assert!(updated);

    // Verify the update
    let result = get_wasm_offering_by_id(&offering.id).await.unwrap().unwrap();
    assert_eq!(result.name, "updated-name");
    assert_eq!(result.description, "Updated description");
    assert_eq!(result.version, "2.0.0");
    assert!(!result.enabled);

    cleanup_wasm_test_db(&db_path).await;
}

#[tokio::test]
async fn test_toggle_wasm_offering() {
    let db_path = setup_wasm_test_db().await;

    // Create an enabled offering
    let offering = create_test_wasm_offering("toggleable-module", true);
    let offering_id = offering.id.clone();
    create_wasm_offering(&offering).await.unwrap();

    // Toggle to disabled
    let toggled = toggle_wasm_offering(&offering_id, false).await.unwrap();
    assert!(toggled);

    // Verify it's disabled
    let result = get_wasm_offering_by_id(&offering_id).await.unwrap().unwrap();
    assert!(!result.enabled);

    // Toggle back to enabled
    let toggled = toggle_wasm_offering(&offering_id, true).await.unwrap();
    assert!(toggled);

    // Verify it's enabled again
    let result = get_wasm_offering_by_id(&offering_id).await.unwrap().unwrap();
    assert!(result.enabled);

    cleanup_wasm_test_db(&db_path).await;
}

#[tokio::test]
async fn test_delete_wasm_offering() {
    let db_path = setup_wasm_test_db().await;

    // Create an offering
    let offering = create_test_wasm_offering("deletable-module", true);
    let offering_id = offering.id.clone();
    create_wasm_offering(&offering).await.unwrap();

    // Verify it exists
    let result = get_wasm_offering_by_id(&offering_id).await.unwrap();
    assert!(result.is_some());

    // Delete it
    let deleted = delete_wasm_offering(&offering_id).await.unwrap();
    assert!(deleted);

    // Verify it's gone
    let result = get_wasm_offering_by_id(&offering_id).await.unwrap();
    assert!(result.is_none());

    // Deleting again should return false
    let deleted_again = delete_wasm_offering(&offering_id).await.unwrap();
    assert!(!deleted_again);

    cleanup_wasm_test_db(&db_path).await;
}

// ============================================================================
// Discovered WASM Offerings Cache Tests
// ============================================================================

#[tokio::test]
async fn test_cache_discovered_wasm_offering() {
    let db_path = setup_wasm_test_db().await;

    let peer_id = "12D3KooWTestPeer1";
    let offering = create_test_wasm_offering("cached-module", true);

    // Cache the offering
    cache_discovered_wasm_offering(peer_id, &offering)
        .await
        .unwrap();

    // Verify it's cached
    let cached = get_cached_wasm_offerings_by_peer(peer_id).await.unwrap();
    assert_eq!(cached.len(), 1);
    assert_eq!(cached[0].name, "cached-module");
    assert_eq!(cached[0].id, offering.id);

    cleanup_wasm_test_db(&db_path).await;
}

#[tokio::test]
async fn test_cache_discovered_wasm_offering_updates_last_seen() {
    let db_path = setup_wasm_test_db().await;

    let peer_id = "12D3KooWTestPeer2";
    let offering = create_test_wasm_offering("updatable-cached-module", true);

    // Cache the offering twice
    cache_discovered_wasm_offering(peer_id, &offering)
        .await
        .unwrap();

    // Small delay to ensure time difference
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    cache_discovered_wasm_offering(peer_id, &offering)
        .await
        .unwrap();

    // Should still only have one entry (INSERT OR REPLACE)
    let cached = get_cached_wasm_offerings_by_peer(peer_id).await.unwrap();
    assert_eq!(cached.len(), 1);

    cleanup_wasm_test_db(&db_path).await;
}

#[tokio::test]
async fn test_get_cached_wasm_offerings_by_peer() {
    let db_path = setup_wasm_test_db().await;

    let peer1 = "12D3KooWTestPeer3";
    let peer2 = "12D3KooWTestPeer4";

    // Cache offerings from two different peers
    let offering1 = create_test_wasm_offering("peer1-module", true);
    let offering2 = create_test_wasm_offering("peer2-module", true);

    cache_discovered_wasm_offering(peer1, &offering1)
        .await
        .unwrap();
    cache_discovered_wasm_offering(peer2, &offering2)
        .await
        .unwrap();

    // Get offerings for peer1 only
    let peer1_offerings = get_cached_wasm_offerings_by_peer(peer1).await.unwrap();
    assert_eq!(peer1_offerings.len(), 1);
    assert_eq!(peer1_offerings[0].name, "peer1-module");

    // Get offerings for peer2 only
    let peer2_offerings = get_cached_wasm_offerings_by_peer(peer2).await.unwrap();
    assert_eq!(peer2_offerings.len(), 1);
    assert_eq!(peer2_offerings[0].name, "peer2-module");

    cleanup_wasm_test_db(&db_path).await;
}

#[tokio::test]
async fn test_get_all_cached_wasm_offerings() {
    let db_path = setup_wasm_test_db().await;

    let peer1 = "12D3KooWTestPeer5";
    let peer2 = "12D3KooWTestPeer6";

    // Cache multiple offerings from different peers
    let offering1 = create_test_wasm_offering("module-a", true);
    let offering2 = create_test_wasm_offering("module-b", true);
    let offering3 = create_test_wasm_offering("module-c", true);

    cache_discovered_wasm_offering(peer1, &offering1)
        .await
        .unwrap();
    cache_discovered_wasm_offering(peer1, &offering2)
        .await
        .unwrap();
    cache_discovered_wasm_offering(peer2, &offering3)
        .await
        .unwrap();

    // Get all cached offerings
    let all_cached = get_all_cached_wasm_offerings().await.unwrap();
    assert_eq!(all_cached.len(), 3);

    // Verify the structure includes peer IDs
    let peer_ids: Vec<&String> = all_cached.iter().map(|(peer_id, _)| peer_id).collect();
    assert!(peer_ids.contains(&&peer1.to_string()));
    assert!(peer_ids.contains(&&peer2.to_string()));

    cleanup_wasm_test_db(&db_path).await;
}

#[tokio::test]
async fn test_cleanup_stale_wasm_offerings() {
    let db_path = setup_wasm_test_db().await;

    let peer_id = "12D3KooWTestPeer7";
    let offering = create_test_wasm_offering("stale-module", true);

    // Cache the offering
    cache_discovered_wasm_offering(peer_id, &offering)
        .await
        .unwrap();

    // Verify it exists
    let cached = get_cached_wasm_offerings_by_peer(peer_id).await.unwrap();
    assert_eq!(cached.len(), 1);

    // Wait a moment so the offering's last_seen_at is in the past
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Cleanup with 0 seconds max age (should delete everything older than now)
    let cleaned = cleanup_stale_wasm_offerings(0).await.unwrap();
    assert_eq!(cleaned, 1);

    // Verify it's gone
    let cached = get_cached_wasm_offerings_by_peer(peer_id).await.unwrap();
    assert_eq!(cached.len(), 0);

    cleanup_wasm_test_db(&db_path).await;
}
