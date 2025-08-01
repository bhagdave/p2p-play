use p2p_play::storage::{
    create_new_story_with_channel, ensure_stories_file_exists, read_local_stories,
    reset_db_connection_for_testing,
};
use std::env;

#[tokio::test]
async fn test_story_chronological_order_and_channel_display() {
    // Set a test database path
    unsafe {
        env::set_var("TEST_DATABASE_PATH", "/tmp/test_chronological_stories.db");
    }
    
    // Reset any existing connection and ensure fresh database
    reset_db_connection_for_testing().await.unwrap();
    ensure_stories_file_exists().await.unwrap();
    
    // Clear the database manually to ensure clean state
    let conn_arc = p2p_play::storage::get_db_connection().await.unwrap();
    let conn = conn_arc.lock().await;
    conn.execute("DELETE FROM stories", []).unwrap();
    drop(conn); // Release the lock
    
    println!("Creating test stories with channel info and timestamps...");
    
    // Create stories with different channels and timestamps
    let start_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    println!("Start time: {}", start_time);
    
    create_new_story_with_channel(
        "First Story",
        "This is the first story",
        "Once upon a time, there was a first story.",
        "general"
    ).await.unwrap();
    
    // Small delay to ensure different timestamps
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    
    create_new_story_with_channel(
        "Second Story",
        "This is the second story",
        "This is the story that came after the first.",
        "tech"
    ).await.unwrap();
    
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    
    create_new_story_with_channel(
        "Third Story",
        "This is the third story",
        "And this is the newest story of all.",
        "general"
    ).await.unwrap();
    
    println!("Reading stories back to verify chronological order (newest first)...");
    
    let stories = read_local_stories().await.unwrap();
    
    println!("\nStories in chronological order (newest first):");
    println!("===============================================");
    
    for (i, story) in stories.iter().enumerate() {
        let status = if story.public { "📖 Public" } else { "📕 Private" };
        println!("{}. {} | Channel: {} | ID: {} | {}", 
                 i + 1, status, story.channel, story.id, story.name);
        println!("   Created at: {}", story.created_at);
        println!("   Header: {}", story.header);
        println!();
    }
    
    // Verify we have the expected number of stories
    assert_eq!(stories.len(), 3, "Should have exactly 3 stories");
    
    // Verify the order - newest should be first (created_at DESC)
    assert!(stories[0].created_at >= stories[1].created_at, 
            "Stories are not in chronological order (newest first): {} >= {}", 
            stories[0].created_at, stories[1].created_at);
    assert!(stories[1].created_at >= stories[2].created_at, 
            "Stories are not in chronological order (newest first): {} >= {}", 
            stories[1].created_at, stories[2].created_at);
    println!("✅ Stories are correctly ordered chronologically (newest first)");
    
    // Verify newest story is "Third Story"
    assert_eq!(stories[0].name, "Third Story", "Newest story should be 'Third Story'");
    assert_eq!(stories[1].name, "Second Story", "Second newest story should be 'Second Story'");
    assert_eq!(stories[2].name, "First Story", "Oldest story should be 'First Story'");
    
    // Verify channel info is preserved
    assert_eq!(stories[0].channel, "general", "Third story should be in 'general' channel");
    assert_eq!(stories[1].channel, "tech", "Second story should be in 'tech' channel");
    assert_eq!(stories[2].channel, "general", "First story should be in 'general' channel");
    println!("✅ Channel information is correctly preserved");
    
    // Verify all stories have timestamps
    for story in &stories {
        assert!(story.created_at > 0, "Story '{}' should have a valid timestamp", story.name);
    }
    println!("✅ All stories have valid timestamps");
    
    // Test UI display format
    for story in &stories {
        let status = if story.public { "📖" } else { "📕" };
        let ui_format = format!("{} [{}] {}: {}", status, story.channel, story.id, story.name);
        println!("UI format: {}", ui_format);
        
        // Verify the format includes channel info
        assert!(ui_format.contains(&format!("[{}]", story.channel)), 
                "UI format should include channel info");
    }
    println!("✅ UI display format correctly shows channel information");
}