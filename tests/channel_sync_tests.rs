use p2p_play::storage::{
    create_channel, get_channels_for_stories, process_discovered_channels, read_channels,
};
use p2p_play::types::{Channel, Story};
use std::collections::HashSet;

const TEST_DB_PATH: &str = "./test_channel_sync.db";

async fn setup_test_environment() {
    unsafe {
        std::env::set_var("TEST_DATABASE_PATH", TEST_DB_PATH);
    }
    // Initialize the database by ensuring we have a connection and creating tables
    let conn_arc = p2p_play::storage::get_db_connection().await.unwrap();
    let conn = conn_arc.lock().await;
    p2p_play::migrations::create_tables(&conn).unwrap();
}

fn cleanup_test_db() {
    let _ = std::fs::remove_file(TEST_DB_PATH);
}

#[tokio::test]
async fn test_get_channels_for_stories_empty_list() {
    setup_test_environment().await;
    cleanup_test_db();

    let stories: Vec<Story> = vec![];
    let channels = get_channels_for_stories(&stories).await.unwrap();
    assert!(channels.is_empty());

    cleanup_test_db();
}

#[tokio::test]
async fn test_get_channels_for_stories_with_existing_channels() {
    setup_test_environment().await;
    cleanup_test_db();

    // Create some test channels first
    create_channel("tech", "Technology discussions", "creator1")
        .await
        .unwrap();
    create_channel("science", "Science topics", "creator2")
        .await
        .unwrap();

    // Create stories that reference these channels
    let stories = vec![
        Story {
            id: 1,
            name: "Tech Story 1".to_string(),
            header: "Header 1".to_string(),
            body: "Body 1".to_string(),
            public: true,
            channel: "tech".to_string(),
            created_at: 1000,
            auto_share: None,
        },
        Story {
            id: 2,
            name: "Science Story 1".to_string(),
            header: "Header 2".to_string(),
            body: "Body 2".to_string(),
            public: true,
            channel: "science".to_string(),
            created_at: 2000,
            auto_share: None,
        },
        Story {
            id: 3,
            name: "Tech Story 2".to_string(),
            header: "Header 3".to_string(),
            body: "Body 3".to_string(),
            public: true,
            channel: "tech".to_string(),
            created_at: 3000,
            auto_share: None,
        },
    ];

    let channels = get_channels_for_stories(&stories).await.unwrap();

    // Should return 2 unique channels
    assert_eq!(channels.len(), 2);

    let channel_names: HashSet<String> = channels.iter().map(|c| c.name.clone()).collect();
    assert!(channel_names.contains("tech"));
    assert!(channel_names.contains("science"));

    cleanup_test_db();
}

#[tokio::test]
async fn test_get_channels_for_stories_with_non_existent_channels() {
    setup_test_environment().await;
    cleanup_test_db();

    // Create stories that reference channels that don't exist in our database
    let stories = vec![Story {
        id: 1,
        name: "Story 1".to_string(),
        header: "Header 1".to_string(),
        body: "Body 1".to_string(),
        public: true,
        channel: "nonexistent".to_string(),
        created_at: 1000,
        auto_share: None,
    }];

    let channels = get_channels_for_stories(&stories).await.unwrap();

    // Should return empty since the channel doesn't exist in our database
    assert!(channels.is_empty());

    cleanup_test_db();
}

#[tokio::test]
async fn test_process_discovered_channels() {
    setup_test_environment().await;
    cleanup_test_db();

    let discovered_channels = vec![
        Channel {
            name: "newchannel1".to_string(),
            description: "New Channel 1".to_string(),
            created_by: "peer1".to_string(),
            created_at: 1000,
        },
        Channel {
            name: "newchannel2".to_string(),
            description: "New Channel 2".to_string(),
            created_by: "peer2".to_string(),
            created_at: 2000,
        },
    ];

    let saved_count = process_discovered_channels(&discovered_channels, "testpeer")
        .await
        .unwrap();
    assert_eq!(saved_count, 2);

    // Verify channels were saved
    let all_channels = read_channels().await.unwrap();
    let channel_names: HashSet<String> = all_channels.iter().map(|c| c.name.clone()).collect();
    assert!(channel_names.contains("newchannel1"));
    assert!(channel_names.contains("newchannel2"));

    cleanup_test_db();
}

#[tokio::test]
async fn test_process_discovered_channels_with_duplicates() {
    setup_test_environment().await;
    cleanup_test_db();

    // Create an existing channel
    create_channel("existing", "Existing Channel", "original_creator")
        .await
        .unwrap();

    let discovered_channels = vec![
        Channel {
            name: "existing".to_string(),
            description: "Existing Channel (duplicate)".to_string(),
            created_by: "peer1".to_string(),
            created_at: 1000,
        },
        Channel {
            name: "newchannel".to_string(),
            description: "New Channel".to_string(),
            created_by: "peer2".to_string(),
            created_at: 2000,
        },
    ];

    let saved_count = process_discovered_channels(&discovered_channels, "testpeer")
        .await
        .unwrap();

    // Should only save 1 (the new one, existing should be skipped)
    assert_eq!(saved_count, 1);

    // Verify both channels exist but the existing one wasn't modified
    let all_channels = read_channels().await.unwrap();
    let channel_names: HashSet<String> = all_channels.iter().map(|c| c.name.clone()).collect();
    assert!(channel_names.contains("existing"));
    assert!(channel_names.contains("newchannel"));

    cleanup_test_db();
}

#[tokio::test]
async fn test_process_discovered_channels_with_invalid_data() {
    setup_test_environment().await;
    cleanup_test_db();

    let discovered_channels = vec![
        Channel {
            name: "".to_string(), // Invalid: empty name
            description: "Valid Description".to_string(),
            created_by: "peer1".to_string(),
            created_at: 1000,
        },
        Channel {
            name: "validname".to_string(),
            description: "".to_string(), // Invalid: empty description
            created_by: "peer2".to_string(),
            created_at: 2000,
        },
        Channel {
            name: "goodchannel".to_string(),
            description: "Good Channel".to_string(),
            created_by: "peer3".to_string(),
            created_at: 3000,
        },
    ];

    let saved_count = process_discovered_channels(&discovered_channels, "testpeer")
        .await
        .unwrap();

    // Should only save 1 (the valid one)
    assert_eq!(saved_count, 1);

    // Verify only the valid channel was saved
    let all_channels = read_channels().await.unwrap();
    assert_eq!(all_channels.len(), 1);
    assert_eq!(all_channels[0].name, "goodchannel");

    cleanup_test_db();
}

#[tokio::test]
async fn test_process_discovered_channels_empty_list() {
    setup_test_environment().await;
    cleanup_test_db();

    let discovered_channels: Vec<Channel> = vec![];
    let saved_count = process_discovered_channels(&discovered_channels, "testpeer")
        .await
        .unwrap();
    assert_eq!(saved_count, 0);

    cleanup_test_db();
}
