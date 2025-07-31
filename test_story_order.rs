use p2p_play::storage::{
    create_new_story_with_channel, ensure_stories_file_exists, read_local_stories,
};
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set a test database path
    env::set_var("TEST_DATABASE_PATH", "/tmp/test_stories.db");
    
    // Initialize the database
    ensure_stories_file_exists().await?;
    
    println!("Creating test stories with channel info and timestamps...");
    
    // Create stories with different channels
    create_new_story_with_channel(
        "First Story",
        "This is the first story",
        "Once upon a time, there was a first story.",
        "general"
    ).await?;
    
    // Small delay to ensure different timestamps
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    
    create_new_story_with_channel(
        "Second Story",
        "This is the second story",
        "This is the story that came after the first.",
        "tech"
    ).await?;
    
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    
    create_new_story_with_channel(
        "Third Story",
        "This is the third story",
        "And this is the newest story of all.",
        "general"
    ).await?;
    
    println!("Reading stories back to verify chronological order (newest first)...");
    
    let stories = read_local_stories().await?;
    
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
    
    // Verify the order - newest should be first
    if stories.len() >= 3 {
        assert!(stories[0].created_at >= stories[1].created_at, 
                "Stories are not in chronological order (newest first)");
        assert!(stories[1].created_at >= stories[2].created_at, 
                "Stories are not in chronological order (newest first)");
        println!("✅ Stories are correctly ordered chronologically (newest first)");
        
        // Verify channel info is displayed
        assert!(stories.iter().any(|s| s.channel == "general"), "General channel stories missing");
        assert!(stories.iter().any(|s| s.channel == "tech"), "Tech channel stories missing");
        println!("✅ Channel information is correctly preserved and displayed");
    } else {
        println!("❌ Not enough stories created for verification");
    }
    
    Ok(())
}