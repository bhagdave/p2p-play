use p2p_play::storage::*;
use p2p_play::types::*;
use std::env;
use std::sync::{Arc, Mutex};
use tempfile::NamedTempFile;
use uuid::Uuid;

// Mutex to ensure database-dependent tests don't interfere with each other
static TEST_DB_MUTEX: Mutex<()> = Mutex::new(());

/// Helper function to set up test isolation with unique database path
async fn setup_test_database() -> String {
    let unique_db_path = format!("/tmp/test_db_{}.db", Uuid::new_v4());
    unsafe {
        env::set_var("TEST_DATABASE_PATH", &unique_db_path);
    }

    // Reset database connection to use the new path
    reset_db_connection_for_testing().await.unwrap();

    // Initialize the test database
    ensure_stories_file_exists().await.unwrap();

    unique_db_path
}

/// Helper function to cleanup test database
async fn cleanup_test_database(db_path: &str) {
    // Reset the environment variable
    unsafe {
        env::remove_var("TEST_DATABASE_PATH");
    }

    // Try to remove the test database file
    if let Err(e) = std::fs::remove_file(db_path) {
        // It's ok if the file doesn't exist or can't be removed in tests
        eprintln!("Could not remove test database {db_path}: {e}");
    }
}

#[tokio::test]
async fn test_story_workflow_integration() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();

    // Create a story
    let story_id = create_new_story_in_path("Integration Test", "Test Header", "Test Body", path)
        .await
        .unwrap();

    // Verify it's created and private
    let stories = read_local_stories_from_path(path).await.unwrap();
    assert_eq!(stories.len(), 1);
    assert!(!stories[0].public);

    // Publish it
    let published = publish_story_in_path(story_id, path).await.unwrap();
    assert!(published.is_some());

    // Verify it's now public
    let stories = read_local_stories_from_path(path).await.unwrap();
    assert!(stories[0].public);

    // Simulate receiving the story on another peer
    let received_story = published.unwrap();
    let temp_file2 = NamedTempFile::new().unwrap();
    let path2 = temp_file2.path().to_str().unwrap();

    let received_id = save_received_story_to_path(received_story, path2)
        .await
        .unwrap();

    // Verify the received story
    let peer2_stories = read_local_stories_from_path(path2).await.unwrap();
    assert_eq!(peer2_stories.len(), 1);
    assert_eq!(peer2_stories[0].id, received_id);
    assert!(peer2_stories[0].public);
    assert_eq!(peer2_stories[0].name, "Integration Test");
}

#[tokio::test]
async fn test_multiple_peers_story_sharing() {
    // Simulate 3 peers sharing stories
    let peer1_file = NamedTempFile::new().unwrap();
    let peer2_file = NamedTempFile::new().unwrap();
    let peer3_file = NamedTempFile::new().unwrap();

    let peer1_path = peer1_file.path().to_str().unwrap();
    let peer2_path = peer2_file.path().to_str().unwrap();
    let peer3_path = peer3_file.path().to_str().unwrap();

    // Each peer creates a story
    let story1_id = create_new_story_in_path("Peer 1 Story", "Header 1", "Body 1", peer1_path)
        .await
        .unwrap();
    let story2_id = create_new_story_in_path("Peer 2 Story", "Header 2", "Body 2", peer2_path)
        .await
        .unwrap();
    let story3_id = create_new_story_in_path("Peer 3 Story", "Header 3", "Body 3", peer3_path)
        .await
        .unwrap();

    // Each peer publishes their story
    let published1 = publish_story_in_path(story1_id, peer1_path)
        .await
        .unwrap()
        .unwrap();
    let published2 = publish_story_in_path(story2_id, peer2_path)
        .await
        .unwrap()
        .unwrap();
    let published3 = publish_story_in_path(story3_id, peer3_path)
        .await
        .unwrap()
        .unwrap();

    // Simulate story distribution - each peer receives stories from others
    save_received_story_to_path(published2.clone(), peer1_path)
        .await
        .unwrap();
    save_received_story_to_path(published3.clone(), peer1_path)
        .await
        .unwrap();

    save_received_story_to_path(published1.clone(), peer2_path)
        .await
        .unwrap();
    save_received_story_to_path(published3.clone(), peer2_path)
        .await
        .unwrap();

    save_received_story_to_path(published1.clone(), peer3_path)
        .await
        .unwrap();
    save_received_story_to_path(published2.clone(), peer3_path)
        .await
        .unwrap();

    // Verify each peer has all 3 stories
    let peer1_stories = read_local_stories_from_path(peer1_path).await.unwrap();
    let peer2_stories = read_local_stories_from_path(peer2_path).await.unwrap();
    let peer3_stories = read_local_stories_from_path(peer3_path).await.unwrap();

    assert_eq!(peer1_stories.len(), 3);
    assert_eq!(peer2_stories.len(), 3);
    assert_eq!(peer3_stories.len(), 3);

    // Verify all stories are public
    for stories in [&peer1_stories, &peer2_stories, &peer3_stories] {
        for story in stories {
            assert!(story.public);
        }
    }
}

#[tokio::test]
async fn test_story_deduplication() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();

    let story = Story {
        id: 1,
        name: "Duplicate Test".to_string(),
        header: "Same Header".to_string(),
        body: "Same Body".to_string(),
        public: true,
        channel: "general".to_string(),
        created_at: 1234567890,
    };

    // Save the same story multiple times
    let id1 = save_received_story_to_path(story.clone(), path)
        .await
        .unwrap();
    let id2 = save_received_story_to_path(story.clone(), path)
        .await
        .unwrap();
    let id3 = save_received_story_to_path(story.clone(), path)
        .await
        .unwrap();

    // All should return the same ID
    assert_eq!(id1, id2);
    assert_eq!(id2, id3);

    // Should only have one story
    let stories = read_local_stories_from_path(path).await.unwrap();
    assert_eq!(stories.len(), 1);
}

#[tokio::test]
async fn test_list_request_response_cycle() {
    // Test the request/response message cycle
    let story1 = Story::new(
        1,
        "Public Story".to_string(),
        "Header".to_string(),
        "Body".to_string(),
        true,
    );
    let story2 = Story::new(
        2,
        "Private Story".to_string(),
        "Header".to_string(),
        "Body".to_string(),
        false,
    );

    let stories = vec![story1, story2];

    // Create a list request
    let request = ListRequest::new_all();
    let _request_json = serde_json::to_string(&request).unwrap();

    // Simulate receiving the request and creating a response
    let public_stories: Vec<Story> = stories.into_iter().filter(|s| s.public).collect();
    let response = ListResponse::new(
        ListMode::ALL,
        "test_receiver".to_string(),
        public_stories.clone(),
    );

    // Serialize and deserialize to simulate network transmission
    let response_json = serde_json::to_string(&response).unwrap();
    let received_response: ListResponse = serde_json::from_str(&response_json).unwrap();

    // Verify only public stories are included
    assert_eq!(received_response.data.len(), 1);
    assert_eq!(received_response.data[0].name, "Public Story");
    assert!(received_response.data[0].public);
}

#[tokio::test]
async fn test_published_story_message() {
    let story = Story::new(
        1,
        "Published Test".to_string(),
        "Header".to_string(),
        "Body".to_string(),
        true,
    );
    let publisher = "peer123".to_string();

    let published = PublishedStory::new(story.clone(), publisher.clone());

    // Serialize and deserialize to simulate network transmission
    let json = serde_json::to_string(&published).unwrap();
    let received: PublishedStory = serde_json::from_str(&json).unwrap();

    assert_eq!(received.story, story);
    assert_eq!(received.publisher, publisher);
}

#[tokio::test]
async fn test_sequential_story_operations() {
    let temp_file = NamedTempFile::with_suffix(".json").unwrap();
    let path = temp_file.path().to_str().unwrap();

    // Create multiple stories sequentially
    let mut story_ids = Vec::new();
    for i in 0..10 {
        let story_id = create_new_story_in_path(
            &format!("Story {i}"),
            &format!("Header {i}"),
            &format!("Body {i}"),
            path,
        )
        .await
        .unwrap();
        story_ids.push(story_id);
    }

    // Verify all stories were created with unique IDs
    let stories = read_local_stories_from_path(path).await.unwrap();
    assert_eq!(stories.len(), 10);

    // Verify IDs are sequential
    for (i, story) in stories.iter().enumerate() {
        assert_eq!(story.id, i);
        assert_eq!(story.name, format!("Story {i}"));
    }
}

#[tokio::test]
async fn test_error_handling() {
    // Test various error conditions

    // 1. Reading from non-existent file
    let result = read_local_stories_from_path("/definitely/does/not/exist").await;
    assert!(result.is_err());

    // 2. Publishing non-existent story in empty file
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();

    // Create an empty stories file first
    write_local_stories_to_path(&Vec::new(), path)
        .await
        .unwrap();

    let result = publish_story_in_path(999, path).await.unwrap();
    assert!(result.is_none());

    // 3. Reading corrupted JSON file
    tokio::fs::write(path, "not valid json").await.unwrap();
    let result = read_local_stories_from_path(path).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_story_filtering() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();

    // Create mix of public and private stories
    create_new_story_in_path("Private 1", "H1", "B1", path)
        .await
        .unwrap();
    let public_id = create_new_story_in_path("Public 1", "H2", "B2", path)
        .await
        .unwrap();
    create_new_story_in_path("Private 2", "H3", "B3", path)
        .await
        .unwrap();

    // Publish one story
    publish_story_in_path(public_id, path).await.unwrap();

    // Read all stories and filter public ones
    let all_stories = read_local_stories_from_path(path).await.unwrap();
    let public_stories: Vec<_> = all_stories.into_iter().filter(|s| s.public).collect();

    assert_eq!(public_stories.len(), 1);
    assert_eq!(public_stories[0].name, "Public 1");
}

#[tokio::test]
async fn test_peer_name_functionality() {
    // Test PeerName struct creation and serialization
    let peer_id = "12D3KooWTestPeer123456789".to_string();
    let name = "Alice's Node".to_string();

    let peer_name = PeerName::new(peer_id.clone(), name.clone());

    // Verify creation
    assert_eq!(peer_name.peer_id, peer_id);
    assert_eq!(peer_name.name, name);

    // Test serialization/deserialization (simulating network transmission)
    let json = serde_json::to_string(&peer_name).unwrap();
    let deserialized: PeerName = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.peer_id, peer_id);
    assert_eq!(deserialized.name, name);

    // Test that different peer names are not equal
    let different_peer_name = PeerName::new("DifferentPeer".to_string(), "Bob".to_string());
    assert_ne!(peer_name, different_peer_name);
}

#[tokio::test]
async fn test_name_command_shows_current_alias() {
    use p2p_play::error_logger::ErrorLogger;
    use p2p_play::event_handlers::handle_input_event;
    use p2p_play::handlers::{SortedPeerNamesCache, UILogger};
    use p2p_play::network::create_swarm;
    use std::collections::HashMap;
    use tokio::sync::mpsc;

    // Setup test components
    let ping_config = p2p_play::types::PingConfig::new();
    let mut swarm = create_swarm(&ping_config).expect("Failed to create swarm");
    let peer_names = HashMap::new();
    let (story_sender, _story_receiver) = mpsc::unbounded_channel();
    let mut sorted_peer_names_cache = SortedPeerNamesCache::new();
    sorted_peer_names_cache.update(&peer_names);
    let (log_sender, mut log_receiver) = mpsc::unbounded_channel::<String>();
    let ui_logger = UILogger::new(log_sender);
    let error_logger = ErrorLogger::new("test_integration_errors.log");

    // Setup direct message config and pending messages for the function
    let dm_config = DirectMessageConfig::new();
    let pending_messages: Arc<Mutex<Vec<PendingDirectMessage>>> = Arc::new(Mutex::new(Vec::new()));
    let mut relay_service = None;

    // Test case 1: No alias set
    let mut local_peer_name = None;
    handle_input_event(
        "name".to_string(),
        &mut swarm,
        &peer_names,
        story_sender.clone(),
        &mut local_peer_name,
        &sorted_peer_names_cache,
        &ui_logger,
        &error_logger,
        &dm_config,
        &pending_messages,
        &mut relay_service,
    )
    .await;

    // Verify the correct message was logged
    let message = log_receiver.try_recv().unwrap();
    assert_eq!(message, "No alias set. Use 'name <alias>' to set one.");

    // Test case 2: Alias is set
    local_peer_name = Some("TestAlias".to_string());
    handle_input_event(
        "name".to_string(),
        &mut swarm,
        &peer_names,
        story_sender.clone(),
        &mut local_peer_name,
        &sorted_peer_names_cache,
        &ui_logger,
        &error_logger,
        &dm_config,
        &pending_messages,
        &mut relay_service,
    )
    .await;

    // Verify the correct message was logged
    let message = log_receiver.try_recv().unwrap();
    assert_eq!(message, "Current alias: TestAlias");

    // Ensure no more messages were logged
    assert!(log_receiver.try_recv().is_err());
}

// Channel functionality tests
#[tokio::test]
async fn test_channel_creation_and_management() {
    let _lock = TEST_DB_MUTEX.lock().unwrap(); // Ensure test isolation

    use p2p_play::storage::*;

    // Set up isolated test database
    let db_path = setup_test_database().await;

    // Test channel creation
    let channel_name = "tech";
    let description = "Technology discussions";
    let creator = "alice_peer";

    create_channel(channel_name, description, creator)
        .await
        .unwrap();

    // Read channels and verify creation
    let channels = read_channels().await.unwrap();
    let tech_channel = channels.iter().find(|c| c.name == channel_name).unwrap();

    assert_eq!(tech_channel.name, channel_name);
    assert_eq!(tech_channel.description, description);
    assert_eq!(tech_channel.created_by, creator);
    assert!(tech_channel.created_at > 0);

    // Test duplicate channel creation (should be ignored)
    create_channel(channel_name, "Different description", "different_peer")
        .await
        .unwrap();
    let channels_after = read_channels().await.unwrap();
    assert_eq!(channels.len(), channels_after.len()); // Should be same count

    // Cleanup
    cleanup_test_database(&db_path).await;
}

#[tokio::test]
async fn test_channel_subscriptions() {
    let _lock = TEST_DB_MUTEX.lock().unwrap(); // Ensure test isolation

    use p2p_play::storage::*;

    // Set up isolated test database
    let db_path = setup_test_database().await;

    let peer_id = "test_peer_123";
    let channel1 = "tech";
    let channel2 = "news";

    // Create channels
    create_channel(channel1, "Technology channel", "system")
        .await
        .unwrap();
    create_channel(channel2, "News channel", "system")
        .await
        .unwrap();

    // Subscribe to channels
    subscribe_to_channel(peer_id, channel1).await.unwrap();
    subscribe_to_channel(peer_id, channel2).await.unwrap();

    // Read subscriptions
    let subscriptions = read_subscribed_channels(peer_id).await.unwrap();
    assert_eq!(subscriptions.len(), 2);
    assert!(subscriptions.contains(&channel1.to_string()));
    assert!(subscriptions.contains(&channel2.to_string()));

    // Unsubscribe from one channel
    unsubscribe_from_channel(peer_id, channel1).await.unwrap();
    let subscriptions_after = read_subscribed_channels(peer_id).await.unwrap();
    assert_eq!(subscriptions_after.len(), 1);
    assert!(subscriptions_after.contains(&channel2.to_string()));
    assert!(!subscriptions_after.contains(&channel1.to_string()));

    // Test duplicate subscription (should not create duplicates)
    subscribe_to_channel(peer_id, channel2).await.unwrap();
    let final_subscriptions = read_subscribed_channels(peer_id).await.unwrap();
    assert_eq!(final_subscriptions.len(), 1);

    // Cleanup
    cleanup_test_database(&db_path).await;
}

#[tokio::test]
async fn test_stories_with_channels() {
    let _lock = TEST_DB_MUTEX.lock().unwrap(); // Ensure test isolation

    use p2p_play::storage::*;

    // Initialize database
    ensure_stories_file_exists().await.unwrap();
    clear_database_for_testing().await.unwrap();

    // Create stories in different channels using database functions
    create_new_story_with_channel("Tech Story", "Header", "Body", "general")
        .await
        .unwrap();
    create_new_story_with_channel("Gaming Story", "Game Header", "Game Body", "gaming")
        .await
        .unwrap();
    create_new_story_with_channel("News Story", "News Header", "News Body", "news")
        .await
        .unwrap();

    // Read stories and verify channels
    let stories = read_local_stories().await.unwrap();

    // Find our test stories
    let tech_story = stories.iter().find(|s| s.name == "Tech Story").unwrap();
    let gaming_story = stories.iter().find(|s| s.name == "Gaming Story").unwrap();
    let news_story = stories.iter().find(|s| s.name == "News Story").unwrap();

    // The first story should default to "general" channel
    assert_eq!(tech_story.channel, "general");
    assert_eq!(gaming_story.channel, "gaming");
    assert_eq!(news_story.channel, "news");
}

#[tokio::test]
async fn test_channel_story_filtering() {
    let _lock = TEST_DB_MUTEX.lock().unwrap(); // Ensure test isolation

    use p2p_play::storage::*;

    // Set up isolated test database
    let db_path = setup_test_database().await;

    let peer_id = "filter_test_peer";

    // Create channels
    create_channel("tech", "Technology", "system")
        .await
        .unwrap();
    create_channel("gaming", "Gaming", "system").await.unwrap();
    create_channel("news", "News", "system").await.unwrap();

    // Subscribe peer to only tech and gaming
    subscribe_to_channel(peer_id, "tech").await.unwrap();
    subscribe_to_channel(peer_id, "gaming").await.unwrap();

    // Create and publish stories in different channels
    create_new_story_with_channel("Tech Article", "Tech Header", "Tech Body", "tech")
        .await
        .unwrap();
    create_new_story_with_channel("Game Review", "Game Header", "Game Body", "gaming")
        .await
        .unwrap();
    create_new_story_with_channel("Breaking News", "News Header", "News Body", "news")
        .await
        .unwrap();
    create_new_story_with_channel("General Post", "General Header", "General Body", "general")
        .await
        .unwrap();

    // Mark all stories as public
    let all_stories = read_local_stories().await.unwrap();
    for story in &all_stories {
        let (story_sender, _) = tokio::sync::mpsc::unbounded_channel();
        publish_story(story.id, story_sender).await.unwrap();
    }

    // Test story filtering by channel
    let tech_stories = get_stories_by_channel("tech").await.unwrap();
    let gaming_stories = get_stories_by_channel("gaming").await.unwrap();
    let news_stories = get_stories_by_channel("news").await.unwrap();
    let general_stories = get_stories_by_channel("general").await.unwrap();

    assert_eq!(tech_stories.len(), 1);
    assert_eq!(tech_stories[0].name, "Tech Article");

    assert_eq!(gaming_stories.len(), 1);
    assert_eq!(gaming_stories[0].name, "Game Review");

    assert_eq!(news_stories.len(), 1);
    assert_eq!(news_stories[0].name, "Breaking News");

    assert_eq!(general_stories.len(), 1);
    assert_eq!(general_stories[0].name, "General Post");

    // Cleanup
    cleanup_test_database(&db_path).await;
}

#[tokio::test]
async fn test_channel_workflow_integration() {
    let _lock = TEST_DB_MUTEX.lock().unwrap(); // Ensure test isolation

    use p2p_play::storage::*;

    // Set up isolated test database
    let db_path = setup_test_database().await;

    // Simulate two peers with different channel subscriptions
    let peer1_id = "peer1";
    let peer2_id = "peer2";

    // Create channels
    create_channel("tech", "Technology discussions", peer1_id)
        .await
        .unwrap();
    create_channel("art", "Art and creativity", peer2_id)
        .await
        .unwrap();

    // Peer1 subscribes to tech, Peer2 subscribes to art
    subscribe_to_channel(peer1_id, "tech").await.unwrap();
    subscribe_to_channel(peer2_id, "art").await.unwrap();
    // Both subscribe to general
    subscribe_to_channel(peer1_id, "general").await.unwrap();
    subscribe_to_channel(peer2_id, "general").await.unwrap();

    // Create stories in different channels
    create_new_story_with_channel("Rust Tutorial", "Tech Header", "Tech Body", "tech")
        .await
        .unwrap();
    create_new_story_with_channel("Digital Painting", "Art Header", "Art Body", "art")
        .await
        .unwrap();
    create_new_story_with_channel("Welcome Post", "General Header", "General Body", "general")
        .await
        .unwrap();

    // Publish all stories
    let all_stories = read_local_stories().await.unwrap();
    for story in &all_stories {
        let (story_sender, _) = tokio::sync::mpsc::unbounded_channel();
        publish_story(story.id, story_sender).await.unwrap();
    }

    // Test subscriptions
    let peer1_subscriptions = read_subscribed_channels(peer1_id).await.unwrap();
    let peer2_subscriptions = read_subscribed_channels(peer2_id).await.unwrap();

    assert!(peer1_subscriptions.contains(&"tech".to_string()));
    assert!(peer1_subscriptions.contains(&"general".to_string()));
    assert!(!peer1_subscriptions.contains(&"art".to_string()));

    assert!(peer2_subscriptions.contains(&"art".to_string()));
    assert!(peer2_subscriptions.contains(&"general".to_string()));
    assert!(!peer2_subscriptions.contains(&"tech".to_string()));

    // Verify channel content
    let all_channels = read_channels().await.unwrap();
    assert!(all_channels.len() >= 3); // At least general + tech + art

    let tech_channel = all_channels.iter().find(|c| c.name == "tech").unwrap();
    let art_channel = all_channels.iter().find(|c| c.name == "art").unwrap();

    assert_eq!(tech_channel.created_by, peer1_id);
    assert_eq!(art_channel.created_by, peer2_id);

    // Cleanup
    cleanup_test_database(&db_path).await;
}

#[tokio::test]
async fn test_story_serialization_with_channel() {
    use p2p_play::types::*;

    // Test story with channel field
    let story = Story::new_with_channel(
        1,
        "Channel Test".to_string(),
        "Header".to_string(),
        "Body".to_string(),
        true,
        "tech".to_string(),
    );

    // Test serialization/deserialization
    let json = serde_json::to_string(&story).unwrap();
    let deserialized: Story = serde_json::from_str(&json).unwrap();

    assert_eq!(story, deserialized);
    assert_eq!(deserialized.channel, "tech");

    // Test default channel story
    let default_story = Story::new(
        2,
        "Default Channel".to_string(),
        "Header".to_string(),
        "Body".to_string(),
        false,
    );

    assert_eq!(default_story.channel, "general");

    // Test published story with channel
    let published = PublishedStory::new(story.clone(), "publisher_peer".to_string());
    let published_json = serde_json::to_string(&published).unwrap();
    let published_deserialized: PublishedStory = serde_json::from_str(&published_json).unwrap();

    assert_eq!(published, published_deserialized);
    assert_eq!(published_deserialized.story.channel, "tech");
}

#[tokio::test]
async fn test_channel_subscription_data_structures() {
    let _lock = TEST_DB_MUTEX.lock().unwrap(); // Ensure test isolation

    use p2p_play::storage::*;
    use p2p_play::types::*;

    // Initialize and clear database to be safe
    ensure_stories_file_exists().await.unwrap();
    clear_database_for_testing().await.unwrap();

    // Test Channel creation
    let channel = Channel::new(
        "test_channel".to_string(),
        "Test channel description".to_string(),
        "creator_peer".to_string(),
    );

    assert_eq!(channel.name, "test_channel");
    assert_eq!(channel.description, "Test channel description");
    assert_eq!(channel.created_by, "creator_peer");
    assert!(channel.created_at > 0);

    // Test ChannelSubscription creation
    let subscription =
        ChannelSubscription::new("subscriber_peer".to_string(), "test_channel".to_string());

    assert_eq!(subscription.peer_id, "subscriber_peer");
    assert_eq!(subscription.channel_name, "test_channel");
    assert!(subscription.subscribed_at > 0);

    // Test serialization
    let channel_json = serde_json::to_string(&channel).unwrap();
    let subscription_json = serde_json::to_string(&subscription).unwrap();

    let channel_deserialized: Channel = serde_json::from_str(&channel_json).unwrap();
    let subscription_deserialized: ChannelSubscription =
        serde_json::from_str(&subscription_json).unwrap();

    assert_eq!(channel, channel_deserialized);
    assert_eq!(subscription, subscription_deserialized);
}

#[tokio::test]
async fn test_bootstrap_status_creation() {
    use p2p_play::bootstrap::AutoBootstrap;

    let auto_bootstrap = AutoBootstrap::new();

    // Test that we can create a bootstrap instance
    assert!(
        auto_bootstrap.get_status_string().contains("Not started")
            || auto_bootstrap.get_status_string().contains("status")
    );
}

#[tokio::test]
async fn test_swarm_creation() {
    use p2p_play::network::create_swarm;

    let ping_config = p2p_play::types::PingConfig::new();
    let result = create_swarm(&ping_config);

    // Test that we can create a swarm
    assert!(result.is_ok());
}
