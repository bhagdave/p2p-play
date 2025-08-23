/// Integration test to validate the channel synchronization fix
/// This simulates the scenario reported by the user where channels
/// are discovered but don't appear in the available channels list
use p2p_play::storage::{
    create_channel, get_channels_for_stories, process_discovered_channels, read_channels,
    save_received_story,
};
use p2p_play::types::{Channel, Story};
use std::collections::HashSet;

async fn setup_clean_test_environment(test_name: &str) {
    // Create unique database path for each test
    let test_db_path = format!("./test_integration_channel_sync_{}.db", test_name);

    // Clean up any existing test database first
    let _ = std::fs::remove_file(&test_db_path);

    unsafe {
        std::env::set_var("TEST_DATABASE_PATH", &test_db_path);
    }

    // Reset any cached database connections
    p2p_play::storage::reset_db_connection_for_testing()
        .await
        .unwrap();

    // Initialize the database using the proper initialization function
    p2p_play::storage::ensure_stories_file_exists()
        .await
        .unwrap();
}

fn cleanup_test_db(test_name: &str) {
    let test_db_path = format!("./test_integration_channel_sync_{}.db", test_name);
    let _ = std::fs::remove_file(&test_db_path);
}

/// Test the complete channel discovery workflow that was failing
#[tokio::test]
async fn test_channel_discovery_integration_workflow() {
    setup_clean_test_environment("workflow").await;

    println!("=== Testing Channel Discovery Integration Workflow ===");

    // === Scenario 1: Node A creates a channel and stories ===
    println!("1. Node A creates channel 'technology'");
    create_channel("technology", "Technology discussions", "nodeA")
        .await
        .unwrap();

    // Node A creates stories in that channel
    println!("2. Node A creates stories in 'technology' channel");
    let story_a1 = Story {
        id: 1,
        name: "AI Breakthrough".to_string(),
        header: "New AI discovery".to_string(),
        body: "Scientists have made a breakthrough in AI research.".to_string(),
        public: true,
        channel: "technology".to_string(),
        created_at: 1000,
        auto_share: None,
    };

    let story_a2 = Story {
        id: 2,
        name: "Quantum Computing".to_string(),
        header: "Quantum advancement".to_string(),
        body: "New quantum computing milestone achieved.".to_string(),
        public: true,
        channel: "technology".to_string(),
        created_at: 2000,
        auto_share: None,
    };

    // Save the stories locally (this simulates Node A creating them)
    save_received_story(story_a1.clone()).await.unwrap();
    save_received_story(story_a2.clone()).await.unwrap();

    // === Scenario 2: Node A prepares to sync with Node B ===
    println!("3. Node A prepares channel metadata for story sync");
    let stories_to_sync = vec![story_a1.clone(), story_a2.clone()];
    let channel_metadata = get_channels_for_stories(&stories_to_sync).await.unwrap();

    println!(
        "4. Channel metadata found: {:?}",
        channel_metadata.iter().map(|c| &c.name).collect::<Vec<_>>()
    );

    // Verify that channel metadata was found
    assert_eq!(channel_metadata.len(), 1);
    assert_eq!(channel_metadata[0].name, "technology");
    assert_eq!(channel_metadata[0].description, "Technology discussions");

    // === Scenario 3: Simulate Node B (fresh node) receiving the sync ===
    println!("5. Simulating Node B (fresh node) receiving sync data");

    // Clear the database to simulate Node B having no prior knowledge
    setup_clean_test_environment("workflow_node_b").await;

    // Verify Node B starts with only the general channel
    let initial_channels = read_channels().await.unwrap();
    println!(
        "6. Node B initial channels: {:?}",
        initial_channels.iter().map(|c| &c.name).collect::<Vec<_>>()
    );
    assert_eq!(initial_channels.len(), 1);
    assert_eq!(initial_channels[0].name, "general");

    // === Scenario 4: Node B receives stories and channel metadata ===
    println!("7. Node B receives stories from Node A");
    save_received_story(story_a1.clone()).await.unwrap();
    save_received_story(story_a2.clone()).await.unwrap();

    println!("8. Node B processes discovered channel metadata");
    let discovered_count = process_discovered_channels(&channel_metadata, "nodeA")
        .await
        .unwrap();
    println!("9. Node B discovered {} new channels", discovered_count);

    // This should be 1 because the technology channel is new to Node B
    assert_eq!(discovered_count, 1);

    // === Scenario 5: Verify Node B can now see the discovered channel ===
    println!("10. Verifying Node B can see discovered channels");
    let final_channels = read_channels().await.unwrap();
    println!(
        "11. Node B final channels: {:?}",
        final_channels.iter().map(|c| &c.name).collect::<Vec<_>>()
    );

    // Should now have both general and technology channels
    assert_eq!(final_channels.len(), 2);
    let channel_names: HashSet<String> = final_channels.iter().map(|c| c.name.clone()).collect();
    assert!(channel_names.contains("general"));
    assert!(channel_names.contains("technology"));

    // Verify the technology channel has correct metadata
    let tech_channel = final_channels
        .iter()
        .find(|c| c.name == "technology")
        .unwrap();
    assert_eq!(tech_channel.description, "Technology discussions");
    assert_eq!(tech_channel.created_by, "nodeA");

    println!("=== Integration test completed successfully! ===");
    cleanup_test_db("workflow");
}

/// Test the scenario where stories exist but no channel metadata is available
/// This tests the enhanced get_channels_for_stories function
#[tokio::test]
async fn test_channel_discovery_with_missing_metadata() {
    setup_clean_test_environment("missing_metadata").await;

    println!("=== Testing Channel Discovery with Missing Metadata ===");

    // Create a story that references a channel that doesn't exist in the database
    // This simulates a scenario where stories were created but channel metadata wasn't stored
    let orphan_story = Story {
        id: 3,
        name: "Orphan Story".to_string(),
        header: "Story without channel metadata".to_string(),
        body: "This story references a channel that doesn't exist in our database.".to_string(),
        public: true,
        channel: "missing_channel".to_string(),
        created_at: 3000,
        auto_share: None,
    };

    save_received_story(orphan_story.clone()).await.unwrap();

    // Test that get_channels_for_stories creates default metadata for unknown channels
    println!("1. Getting channel metadata for story with unknown channel");
    let stories_to_sync = vec![orphan_story];
    let channel_metadata = get_channels_for_stories(&stories_to_sync).await.unwrap();

    println!(
        "2. Channel metadata created: {:?}",
        channel_metadata
            .iter()
            .map(|c| (&c.name, &c.description))
            .collect::<Vec<_>>()
    );

    // Should create default metadata for the missing channel
    assert_eq!(channel_metadata.len(), 1);
    assert_eq!(channel_metadata[0].name, "missing_channel");
    assert_eq!(channel_metadata[0].description, "Channel: missing_channel");
    assert_eq!(channel_metadata[0].created_by, "unknown");

    // Test that this metadata can be processed by another node
    println!("3. Simulating another node processing this default metadata");
    let discovered_count = process_discovered_channels(&channel_metadata, "unknown_peer")
        .await
        .unwrap();
    assert_eq!(discovered_count, 1);

    // Verify the channel was saved with default metadata
    let final_channels = read_channels().await.unwrap();
    let channel_names: HashSet<String> = final_channels.iter().map(|c| c.name.clone()).collect();
    assert!(channel_names.contains("missing_channel"));

    println!("=== Missing metadata test completed successfully! ===");
    cleanup_test_db("missing_metadata");
}

/// Test backward compatibility - ensure empty channel lists don't break anything
#[tokio::test]
async fn test_backward_compatibility() {
    setup_clean_test_environment("backward_compat").await;

    println!("=== Testing Backward Compatibility ===");

    // Test processing empty channel list (from older nodes)
    let empty_channels: Vec<Channel> = vec![];
    let discovered_count = process_discovered_channels(&empty_channels, "old_node")
        .await
        .unwrap();
    assert_eq!(discovered_count, 0);

    // Test getting channels for empty story list
    let empty_stories: Vec<Story> = vec![];
    let channel_metadata = get_channels_for_stories(&empty_stories).await.unwrap();
    assert_eq!(channel_metadata.len(), 0);

    println!("=== Backward compatibility test completed successfully! ===");
    cleanup_test_db("backward_compat");
}

/// Test the specific bug where existing channels were incorrectly counted as "discovered"
#[tokio::test]
async fn test_existing_channel_not_counted_as_discovered() {
    setup_clean_test_environment("existing_channel_bug").await;

    println!("=== Testing Existing Channel Bug Fix ===");

    // === Step 1: Create a channel that already exists ===
    println!("1. Creating existing channel 'technology' in database");
    create_channel("technology", "Technology discussions", "original_creator")
        .await
        .unwrap();

    // Verify it exists
    let initial_channels = read_channels().await.unwrap();
    println!(
        "Initial channels after creating 'technology': {:?}",
        initial_channels.iter().map(|c| &c.name).collect::<Vec<_>>()
    );

    // Find the technology channel
    let tech_channels: Vec<_> = initial_channels
        .iter()
        .filter(|c| c.name == "technology")
        .collect();
    assert_eq!(
        tech_channels.len(),
        1,
        "Should have exactly one technology channel"
    );
    assert_eq!(tech_channels[0].created_by, "original_creator");

    // === Step 2: Simulate receiving the same channel during sync ===
    println!("2. Simulating discovery of existing channel during sync");
    let duplicate_channel = Channel::new(
        "technology".to_string(),
        "Technology discussions from peer".to_string(),
        "peer_creator".to_string(),
    );

    // This should return 0 because the channel already exists
    let discovered_count = process_discovered_channels(&[duplicate_channel], "test_peer")
        .await
        .unwrap();

    println!(
        "3. Discovered count for existing channel: {}",
        discovered_count
    );
    assert_eq!(
        discovered_count, 0,
        "Existing channels should not be counted as discovered"
    );

    // Verify the channel still exists and wasn't duplicated
    let final_channels = read_channels().await.unwrap();
    println!(
        "Final channels: {:?}",
        final_channels
            .iter()
            .map(|c| (&c.name, &c.created_by))
            .collect::<Vec<_>>()
    );

    // Check that we still have the technology channel with original metadata
    let tech_channels: Vec<_> = final_channels
        .iter()
        .filter(|c| c.name == "technology")
        .collect();
    assert_eq!(
        tech_channels.len(),
        1,
        "Should have exactly one technology channel"
    );
    assert_eq!(
        tech_channels[0].created_by, "original_creator",
        "Original metadata should be preserved"
    );

    println!("=== Existing channel bug fix test completed successfully! ===");
    cleanup_test_db("existing_channel_bug");
}
