use p2p_play::storage::*;
use p2p_play::types::{DirectMessage};
use tokio;
use uuid::Uuid;

async fn setup_test_db() -> Result<String, Box<dyn std::error::Error>> {
    let unique_db_path = format!("/tmp/test_conversation_{}.db", Uuid::new_v4());
    unsafe {
        std::env::set_var("TEST_DATABASE_PATH", &unique_db_path);
    }

    // Reset database connection to use the new path
    reset_db_connection_for_testing().await?;

    // Initialize the test database (this creates all tables)
    ensure_stories_file_exists().await?;

    Ok(unique_db_path)
}

async fn cleanup_test_db(db_path: &str) {
    // Reset the environment variable
    unsafe {
        std::env::remove_var("TEST_DATABASE_PATH");
    }

    // Try to remove the test database file
    let _ = std::fs::remove_file(db_path);
}

fn create_test_direct_message(
    from_peer: &str,
    to_peer: &str,
    message: &str,
    timestamp: u64,
    is_outgoing: bool,
) -> DirectMessage {
    DirectMessage {
        from_peer_id: from_peer.to_string(),
        from_name: from_peer.to_string(),
        to_peer_id: to_peer.to_string(),
        to_name: to_peer.to_string(),
        message: message.to_string(),
        timestamp,
        is_outgoing,
    }
}

#[tokio::test]
async fn test_simple_conversation_creation() {
    let db_path = setup_test_db().await.unwrap();
    
    // Test empty conversations list initially
    let conversations = get_conversations_with_status().await.unwrap();
    assert!(conversations.is_empty(), "Should start with no conversations");
    
    // Create a simple message without peer names mapping (should work)
    let peer_id = "12D3KooWGHpBMeZbestVEWkfdycxMRjXGEHLVujaXp1JMSnzAADC"; // Valid peer ID
    let message = create_test_direct_message(peer_id, "local", "Hello!", 1000, false);
    
    // Save without peer names mapping (should use peer_id as name)
    save_direct_message(&message, None).await.unwrap();
    
    // Test that conversation was created
    let conversations = get_conversations_with_status().await.unwrap();
    assert_eq!(conversations.len(), 1);
    assert_eq!(conversations[0].peer_id, peer_id);
    assert_eq!(conversations[0].peer_name, peer_id); // Should use peer_id as name
    assert_eq!(conversations[0].unread_count, 1);
    assert_eq!(conversations[0].last_activity, 1000);
    
    cleanup_test_db(&db_path).await;
}

#[tokio::test]
async fn test_simple_conversation_messages() {
    let db_path = setup_test_db().await.unwrap();
    
    let peer_id = "12D3KooWGHpBMeZbestVEWkfdycxMRjXGEHLVujaXp1JMSnzAADC";
    
    // Add multiple messages
    let msg1 = create_test_direct_message(peer_id, "local", "First message", 1000, false);
    let msg2 = create_test_direct_message(peer_id, "local", "Second message", 2000, false);
    let msg3 = create_test_direct_message("local", peer_id, "Reply message", 2500, true);
    
    save_direct_message(&msg1, None).await.unwrap();
    save_direct_message(&msg2, None).await.unwrap();
    save_direct_message(&msg3, None).await.unwrap();
    
    // Test conversation list shows correct counts
    let conversations = get_conversations_with_status().await.unwrap();
    assert_eq!(conversations.len(), 1);
    assert_eq!(conversations[0].unread_count, 2); // Only incoming messages count as unread
    assert_eq!(conversations[0].last_activity, 2500); // Last message timestamp
    
    // Test getting individual conversation messages
    let messages = get_conversation_messages(peer_id).await.unwrap();
    assert_eq!(messages.len(), 3);
    
    // Should be in chronological order
    assert_eq!(messages[0].message, "First message");
    assert_eq!(messages[0].timestamp, 1000);
    assert!(!messages[0].is_outgoing);
    
    assert_eq!(messages[1].message, "Second message");
    assert_eq!(messages[1].timestamp, 2000);
    assert!(!messages[1].is_outgoing);
    
    assert_eq!(messages[2].message, "Reply message");
    assert_eq!(messages[2].timestamp, 2500);
    assert!(messages[2].is_outgoing);
    
    cleanup_test_db(&db_path).await;
}

#[tokio::test]
async fn test_mark_messages_as_read() {
    let db_path = setup_test_db().await.unwrap();
    
    let peer_id = "12D3KooWGHpBMeZbestVEWkfdycxMRjXGEHLVujaXp1JMSnzAADC";
    
    // Add unread messages
    let msg1 = create_test_direct_message(peer_id, "local", "Message 1", 1000, false);
    let msg2 = create_test_direct_message(peer_id, "local", "Message 2", 2000, false);
    
    save_direct_message(&msg1, None).await.unwrap();
    save_direct_message(&msg2, None).await.unwrap();
    
    // Verify unread count
    let conversations_before = get_conversations_with_status().await.unwrap();
    assert_eq!(conversations_before[0].unread_count, 2);
    
    // Mark as read
    mark_conversation_messages_as_read(peer_id).await.unwrap();
    
    // Verify read count
    let conversations_after = get_conversations_with_status().await.unwrap();
    assert_eq!(conversations_after[0].unread_count, 0);
    
    cleanup_test_db(&db_path).await;
}

#[tokio::test]
async fn test_multiple_conversations() {
    let db_path = setup_test_db().await.unwrap();
    
    let peer1 = "12D3KooWGHpBMeZbestVEWkfdycxMRjXGEHLVujaXp1JMSnzAADC";
    let peer2 = "12D3KooWGHpBMeZbestVEWkfdycxMRjXGEHLVujaXp1JMSnzBBCC";
    
    // Create messages from two different peers
    let msg1 = create_test_direct_message(peer1, "local", "Hello from peer1", 1000, false);
    let msg2 = create_test_direct_message(peer2, "local", "Hello from peer2", 2000, false);
    let msg3 = create_test_direct_message(peer1, "local", "Another from peer1", 3000, false);
    
    save_direct_message(&msg1, None).await.unwrap();
    save_direct_message(&msg2, None).await.unwrap();
    save_direct_message(&msg3, None).await.unwrap();
    
    // Should have two separate conversations
    let conversations = get_conversations_with_status().await.unwrap();
    assert_eq!(conversations.len(), 2);
    
    // Should be sorted by last activity (peer1 = 3000, peer2 = 2000)
    assert_eq!(conversations[0].peer_id, peer1);
    assert_eq!(conversations[0].last_activity, 3000);
    assert_eq!(conversations[0].unread_count, 2);
    
    assert_eq!(conversations[1].peer_id, peer2);
    assert_eq!(conversations[1].last_activity, 2000);
    assert_eq!(conversations[1].unread_count, 1);
    
    cleanup_test_db(&db_path).await;
}