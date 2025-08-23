use p2p_play::storage::{
    create_channel, get_channels_for_stories, process_discovered_channels, channel_exists, read_channels,
};
use p2p_play::types::{Channel, Story};

const TEST_DB_PATH: &str = "./debug_channel_discovery_test.db";

async fn setup_test_environment(db_suffix: &str) {
    let db_path = format!("./debug_channel_discovery_test_{}.db", db_suffix);
    
    // Clean up any existing test database first
    let _ = std::fs::remove_file(&db_path);
    
    unsafe {
        std::env::set_var("TEST_DATABASE_PATH", &db_path);
    }
    
    // Reset any cached database connections since we changed the environment variable
    p2p_play::storage::reset_db_connection_for_testing().await.unwrap();
    
    // Initialize the database by ensuring we have a connection and creating tables
    let conn_arc = p2p_play::storage::get_db_connection().await.unwrap();
    let conn = conn_arc.lock().await;
    p2p_play::migrations::create_tables(&conn).unwrap();
    drop(conn); // Ensure connection is released
}

fn cleanup_test_db(db_suffix: &str) {
    let db_path = format!("./debug_channel_discovery_test_{}.db", db_suffix);
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn debug_complete_channel_discovery_workflow() {
    println!("=== Testing complete channel discovery workflow ===");
    
    // Step 1: Simulate pi5 node creating Technology channel and story
    setup_test_environment("pi5").await;
    
    println!("\n1. Simulating pi5 node - creating Technology channel and story");
    create_channel("Technology", "Technology discussions", "pi5_user").await.unwrap();
    
    let tech_story = Story {
        id: 1,
        name: "Cool Tech".to_string(),
        header: "Header".to_string(),
        body: "Body".to_string(),
        public: true,
        channel: "Technology".to_string(),
        created_at: 1000,
        auto_share: None,
    };
    
    // Get channels for this story (what pi5 would send)
    let stories = vec![tech_story];
    let channels_to_send = get_channels_for_stories(&stories).await.unwrap();
    println!("   pi5 found {} channels to send: {:?}", 
        channels_to_send.len(), 
        channels_to_send.iter().map(|c| &c.name).collect::<Vec<_>>());
    
    // Verify pi5 found the Technology channel to send
    assert_eq!(channels_to_send.len(), 1, "pi5 should find 1 channel to send");
    assert_eq!(channels_to_send[0].name, "Technology", "Should find Technology channel");
    
    // Step 2: Simulate meerkat node (clean database)
    setup_test_environment("meerkat").await;
    
    println!("\n2. Simulating meerkat node (clean database)");
    
    // Check if Technology channel exists on meerkat (should not)
    let exists_before = channel_exists("Technology").await.unwrap();
    println!("   Technology channel exists on meerkat before sync: {}", exists_before);
    assert!(!exists_before, "meerkat should not have Technology channel initially");
    
    let channels_before = read_channels().await.unwrap();
    println!("   Channels on meerkat before sync: {:?}", 
        channels_before.iter().map(|c| &c.name).collect::<Vec<_>>());
    
    // Check that Technology specifically doesn't exist (there might be default channels)
    let tech_channels_before: Vec<_> = channels_before.iter().filter(|c| c.name == "Technology").collect();
    assert_eq!(tech_channels_before.len(), 0, "meerkat should not have Technology channel initially");
    
    // Step 3: Process discovered channels (what happens when meerkat receives sync response)
    println!("\n3. Processing discovered channels on meerkat");
    let discovered_count = process_discovered_channels(&channels_to_send, "pi5").await.unwrap();
    println!("   Discovered count: {}", discovered_count);
    
    // Check final state
    let exists_after = channel_exists("Technology").await.unwrap();
    println!("   Technology channel exists on meerkat after sync: {}", exists_after);
    
    let channels_after = read_channels().await.unwrap();
    println!("   Channels on meerkat after sync: {:?}", 
        channels_after.iter().map(|c| &c.name).collect::<Vec<_>>());
    
    // Check that Technology specifically exists now
    let tech_channels_after: Vec<_> = channels_after.iter().filter(|c| c.name == "Technology").collect();
    assert_eq!(tech_channels_after.len(), 1, "meerkat should have Technology channel after sync");
    assert_eq!(tech_channels_after[0].name, "Technology", "Should have Technology channel");
    
    // Verify the workflow worked correctly
    println!("\n4. Verifying results");
    assert_eq!(discovered_count, 1, "Should discover exactly 1 channel");
    assert!(exists_after, "Technology channel should exist after sync");
    
    println!("\n✅ SUCCESS: Channel discovery workflow works correctly!");
    
    // Clean up
    cleanup_test_db("pi5");
    cleanup_test_db("meerkat");
}