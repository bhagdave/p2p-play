use p2p_play::network::{StorySyncRequest, StorySyncResponse};
use p2p_play::storage::{ensure_stories_file_exists, read_local_stories, save_received_story};
use p2p_play::types::{Icons, Story};
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::test]
async fn test_story_sync_request_creation() {
    let request = StorySyncRequest {
        from_peer_id: "test_peer".to_string(),
        from_name: "TestPeer".to_string(),
        last_sync_timestamp: 1000,
        subscribed_channels: vec!["general".to_string(), "tech".to_string()],
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };

    assert_eq!(request.from_peer_id, "test_peer");
    assert_eq!(request.from_name, "TestPeer");
    assert_eq!(request.last_sync_timestamp, 1000);
    assert_eq!(request.subscribed_channels.len(), 2);
    assert!(request.subscribed_channels.contains(&"general".to_string()));
    assert!(request.subscribed_channels.contains(&"tech".to_string()));
}

#[tokio::test]
async fn test_story_sync_response_creation() {
    let story1 = Story {
        id: 1,
        name: "Test Story 1".to_string(),
        header: "Header 1".to_string(),
        body: "Body 1".to_string(),
        public: true,
        channel: "general".to_string(),
        created_at: 2000,
        auto_share: None,
    };

    let story2 = Story {
        id: 2,
        name: "Test Story 2".to_string(),
        header: "Header 2".to_string(),
        body: "Body 2".to_string(),
        public: true,
        channel: "tech".to_string(),
        created_at: 3000,
        auto_share: None,
    };

    let response = StorySyncResponse {
        stories: vec![story1.clone(), story2.clone()],
        channels: vec![],
        from_peer_id: "peer_123".to_string(),
        from_name: "Peer123".to_string(),
        sync_timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };

    assert_eq!(response.stories.len(), 2);
    assert_eq!(response.from_peer_id, "peer_123");
    assert_eq!(response.from_name, "Peer123");
    assert_eq!(response.stories[0], story1);
    assert_eq!(response.stories[1], story2);
}

#[tokio::test]
async fn test_story_sync_filtering_by_timestamp() {
    // Use a unique database path for this test
    unsafe {
        std::env::set_var("TEST_DATABASE_PATH", "/tmp/story_sync_test.db");
    }

    // Clean up and initialize database
    let _ = std::fs::remove_file("/tmp/story_sync_test.db");
    ensure_stories_file_exists().await.unwrap();

    // Create stories with different timestamps
    let old_story = Story {
        id: 101,
        name: "Old Story".to_string(),
        header: "Old Header".to_string(),
        body: "Old Body".to_string(),
        public: true,
        channel: "general".to_string(),
        created_at: 1000,
        auto_share: None, // Old timestamp
    };

    let new_story = Story {
        id: 102,
        name: "New Story".to_string(),
        header: "New Header".to_string(),
        body: "New Body".to_string(),
        public: true,
        channel: "general".to_string(),
        created_at: 2000,
        auto_share: None, // New timestamp
    };

    // Save the stories
    save_received_story(old_story.clone()).await.unwrap();
    save_received_story(new_story.clone()).await.unwrap();

    // Read all stories
    let all_stories = read_local_stories().await.unwrap();
    assert!(all_stories.len() >= 2);

    // Test filtering: stories newer than timestamp 1500 should only include new_story
    let filtered_stories: Vec<_> = all_stories
        .into_iter()
        .filter(|story| story.created_at > 1500 && story.public)
        .collect();

    // Should contain at least the new story (may contain other test stories)
    let new_story_found = filtered_stories
        .iter()
        .any(|s| s.name == "New Story" && s.created_at == 2000);
    assert!(new_story_found);

    // Should NOT contain the old story
    let old_story_found = filtered_stories
        .iter()
        .any(|s| s.name == "Old Story" && s.created_at == 1000);
    assert!(!old_story_found);

    // Cleanup
    let _ = std::fs::remove_file("/tmp/story_sync_test.db");
    unsafe {
        std::env::remove_var("TEST_DATABASE_PATH");
    }
}

#[tokio::test]
async fn test_story_sync_filtering_by_channel() {
    // Use a unique database path for this test
    unsafe {
        std::env::set_var("TEST_DATABASE_PATH", "/tmp/story_sync_channel_test.db");
    }

    // Clean up and initialize database
    let _ = std::fs::remove_file("/tmp/story_sync_channel_test.db");
    ensure_stories_file_exists().await.unwrap();

    // Create stories in different channels
    let general_story = Story {
        id: 201,
        name: "General Story".to_string(),
        header: "General Header".to_string(),
        body: "General Body".to_string(),
        public: true,
        channel: "general".to_string(),
        created_at: 1000,
        auto_share: None,
    };

    let tech_story = Story {
        id: 202,
        name: "Tech Story".to_string(),
        header: "Tech Header".to_string(),
        body: "Tech Body".to_string(),
        public: true,
        channel: "tech".to_string(),
        created_at: 1000,
        auto_share: None,
    };

    let private_story = Story {
        id: 203,
        name: "Private Story".to_string(),
        header: "Private Header".to_string(),
        body: "Private Body".to_string(),
        public: false, // Not public
        channel: "general".to_string(),
        created_at: 1000,
        auto_share: None,
    };

    // Save the stories
    save_received_story(general_story.clone()).await.unwrap();
    save_received_story(tech_story.clone()).await.unwrap();
    save_received_story(private_story.clone()).await.unwrap();

    // Read all stories
    let all_stories = read_local_stories().await.unwrap();

    // Test filtering: only public stories in subscribed channels
    let subscribed_channels : [String; 1] = ["general".to_string()];
    let filtered_stories: Vec<_> = all_stories
        .into_iter()
        .filter(|story| {
            story.public
                && (subscribed_channels.is_empty() || subscribed_channels.contains(&story.channel))
        })
        .collect();

    // Should contain the general story
    let general_found = filtered_stories
        .iter()
        .any(|s| s.name == "General Story" && s.channel == "general");
    assert!(general_found);

    // Should NOT contain the tech story (not subscribed to tech channel)
    let tech_found = filtered_stories
        .iter()
        .any(|s| s.name == "Tech Story" && s.channel == "tech");
    assert!(!tech_found);

    // Should NOT contain the private story (not public)
    let private_found = filtered_stories
        .iter()
        .any(|s| s.name == "Private Story" && !s.public);
    assert!(!private_found);

    // Cleanup
    let _ = std::fs::remove_file("/tmp/story_sync_channel_test.db");
    unsafe {
        std::env::remove_var("TEST_DATABASE_PATH");
    }
}

#[test]
fn test_story_sync_serialization() {
    let request = StorySyncRequest {
        from_peer_id: "test_peer".to_string(),
        from_name: "TestPeer".to_string(),
        last_sync_timestamp: 1234567890,
        subscribed_channels: vec!["general".to_string(), "tech".to_string()],
        timestamp: 1234567899,
    };

    // Test JSON serialization/deserialization
    let json = serde_json::to_string(&request).unwrap();
    let deserialized: StorySyncRequest = serde_json::from_str(&json).unwrap();

    assert_eq!(request.from_peer_id, deserialized.from_peer_id);
    assert_eq!(request.from_name, deserialized.from_name);
    assert_eq!(
        request.last_sync_timestamp,
        deserialized.last_sync_timestamp
    );
    assert_eq!(
        request.subscribed_channels,
        deserialized.subscribed_channels
    );
    assert_eq!(request.timestamp, deserialized.timestamp);
}

#[test]
fn test_sync_icons() {
    // Test that the new sync and checkmark icons work
    assert!(!Icons::sync().is_empty());
    assert!(!Icons::checkmark().is_empty());

    // Windows vs non-Windows behavior
    #[cfg(windows)]
    {
        assert_eq!(Icons::sync(), "[SYNC]");
        assert_eq!(Icons::checkmark(), "[✓]");
    }

    #[cfg(not(windows))]
    {
        assert_eq!(Icons::sync(), "🔄");
        assert_eq!(Icons::checkmark(), "✓");
    }
}

#[tokio::test]
async fn test_story_deduplication() {
    // Use a unique database path for this test
    unsafe {
        std::env::set_var("TEST_DATABASE_PATH", "/tmp/story_dedup_test.db");
    }

    // Clean up and initialize database
    let _ = std::fs::remove_file("/tmp/story_dedup_test.db");
    ensure_stories_file_exists().await.unwrap();

    let story = Story {
        id: 301,
        name: "Duplicate Test Story".to_string(),
        header: "Duplicate Header".to_string(),
        body: "Duplicate Body".to_string(),
        public: true,
        channel: "general".to_string(),
        created_at: 1000,
        auto_share: None,
    };

    // Save the same story twice - should succeed both times due to silent deduplication
    save_received_story(story.clone()).await.unwrap();
    let result = save_received_story(story.clone()).await;

    // Second save should succeed (silent deduplication)
    assert!(result.is_ok());

    // Verify only one copy exists
    let stories = read_local_stories().await.unwrap();
    let duplicate_count = stories
        .iter()
        .filter(|s| s.name == "Duplicate Test Story")
        .count();
    assert_eq!(duplicate_count, 1);

    // Cleanup
    let _ = std::fs::remove_file("/tmp/story_dedup_test.db");
    unsafe {
        std::env::remove_var("TEST_DATABASE_PATH");
    }
}
