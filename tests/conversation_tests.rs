use p2p_play::storage::core::*;
use p2p_play::types::{DirectMessage, Conversation};
use std::collections::HashMap;
use tokio;
use uuid::Uuid;
use libp2p::PeerId;

async fn setup_test_db() -> Result<String, Box<dyn std::error::Error>> {
    let unique_db_path = format!("/tmp/test_conversation_{}.db", Uuid::new_v4());
    unsafe {
        std::env::set_var("TEST_DATABASE_PATH", &unique_db_path);
    }
    reset_db_connection_for_testing().await?;
    ensure_stories_file_exists().await?;
    Ok(unique_db_path)
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

fn create_test_peer_id() -> PeerId {
    PeerId::random()
}

async fn cleanup_test_db(db_path: &str) {
    let _ = std::fs::remove_file(db_path);
}

#[tokio::test]
async fn test_empty_conversations_list() {
    let db_path = setup_test_db().await.unwrap();
    
    let conversations = get_conversations_with_status().await.unwrap();
    assert!(conversations.is_empty(), "Should return empty conversations list when no messages exist");
    
    cleanup_test_db(&db_path).await;
}

#[tokio::test]
async fn test_single_conversation_with_unread_messages() {
    let db_path = setup_test_db().await.unwrap();
    
    // Create test peers
    let peer1_id = create_test_peer_id();
    let peer2_id = create_test_peer_id();
    let peer1_str = peer1_id.to_string();
    let peer2_str = peer2_id.to_string();
    let mut peer_names = HashMap::new();
    peer_names.insert(
        peer1_id,
        "Alice".to_string()
    );
    
    // Save incoming message from Alice
    let message = create_test_direct_message(&peer1_str, &peer2_str, "Hello!", 1000, false);
    save_direct_message(&message, Some(&peer_names)).await.unwrap();
    
    let conversations = get_conversations_with_status().await.unwrap();
    assert_eq!(conversations.len(), 1);
    
    let conversation = &conversations[0];
    assert_eq!(conversation.peer_id, peer1_str);
    assert_eq!(conversation.peer_name, "Alice");
    assert_eq!(conversation.unread_count, 1);
    assert_eq!(conversation.last_activity, 1000);
    
    cleanup_test_db(&db_path).await;
}

#[tokio::test]
async fn test_multiple_conversations_sorted_by_activity() {
    let db_path = setup_test_db().await.unwrap();
    
    let peer1_id = create_test_peer_id();
    let peer2_id = create_test_peer_id();
    let peer1_str = peer1_id.to_string();
    let peer2_str = peer2_id.to_string();
    let local_str = "local";
    
    let mut peer_names = HashMap::new();
    peer_names.insert(peer1_id, "Alice".to_string());
    peer_names.insert(peer2_id, "Bob".to_string());
    
    // Create messages with different timestamps
    let msg1 = create_test_direct_message(&peer1_str, local_str, "First message", 1000, false);
    let msg2 = create_test_direct_message(&peer2_str, local_str, "Later message", 2000, false);
    let msg3 = create_test_direct_message(&peer1_str, local_str, "Latest from Alice", 3000, false);
    
    save_direct_message(&msg1, Some(&peer_names)).await.unwrap();
    save_direct_message(&msg2, Some(&peer_names)).await.unwrap();
    save_direct_message(&msg3, Some(&peer_names)).await.unwrap();
    
    let conversations = get_conversations_with_status().await.unwrap();
    assert_eq!(conversations.len(), 2);
    
    // Should be sorted by last activity (most recent first)
    assert_eq!(conversations[0].peer_name, "Alice");
    assert_eq!(conversations[0].last_activity, 3000);
    assert_eq!(conversations[0].unread_count, 2);
    
    assert_eq!(conversations[1].peer_name, "Bob");
    assert_eq!(conversations[1].last_activity, 2000);
    assert_eq!(conversations[1].unread_count, 1);
    
    cleanup_test_db(&db_path).await;
}

#[tokio::test]
async fn test_conversation_with_read_and_unread_messages() {
    let db_path = setup_test_db().await.unwrap();
    
    let peer1_id = create_test_peer_id();
    let peer1_str = peer1_id.to_string();
    let local_str = "local";
    
    let mut peer_names = HashMap::new();
    peer_names.insert(peer1_id, "Alice".to_string());
    
    // Save multiple messages - some read, some unread
    let msg1 = create_test_direct_message(&peer1_str, local_str, "Message 1", 1000, false);
    let msg2 = create_test_direct_message(&peer1_str, local_str, "Message 2", 2000, false);
    let msg3 = DirectMessage {
        from_peer_id: local_str.to_string(),
        from_name: local_str.to_string(),
        to_peer_id: peer1_str.clone(),
        to_name: "Alice".to_string(),
        message: "Outgoing message".to_string(),
        timestamp: 2500,
        is_outgoing: true,
    };
    
    save_direct_message(&msg1, Some(&peer_names)).await.unwrap();
    save_direct_message(&msg2, Some(&peer_names)).await.unwrap();
    save_direct_message(&msg3, Some(&peer_names)).await.unwrap();
    
    // Mark first message as read
    mark_conversation_messages_as_read(&peer1_str).await.unwrap();
    
    // Add one more unread message
    let msg4 = create_test_direct_message(&peer1_str, local_str, "New unread", 3000, false);
    save_direct_message(&msg4, Some(&peer_names)).await.unwrap();
    
    let conversations = get_conversations_with_status().await.unwrap();
    assert_eq!(conversations.len(), 1);
    
    let conversation = &conversations[0];
    assert_eq!(conversation.unread_count, 1); // Only the last message should be unread
    
    cleanup_test_db(&db_path).await;
}

#[tokio::test]
async fn test_get_conversation_messages_chronological_order() {
    let db_path = setup_test_db().await.unwrap();
    
    let peer1_id = create_test_peer_id();
    let peer1_str = peer1_id.to_string();
    let local_str = "local";
    
    let mut peer_names = HashMap::new();
    peer_names.insert(peer1_id, "Alice".to_string());
    
    // Create messages in non-chronological order
    let msg2 = create_test_direct_message(&peer1_str, local_str, "Second", 2000, false);
    let msg1 = create_test_direct_message(&peer1_str, local_str, "First", 1000, false);
    let msg3 = DirectMessage {
        from_peer_id: local_str.to_string(),
        from_name: local_str.to_string(),
        to_peer_id: peer1_str.clone(),
        to_name: "Alice".to_string(),
        message: "Third (outgoing)".to_string(),
        timestamp: 3000,
        is_outgoing: true,
    };
    
    // Save in non-chronological order
    save_direct_message(&msg2, Some(&peer_names)).await.unwrap();
    save_direct_message(&msg1, Some(&peer_names)).await.unwrap();
    save_direct_message(&msg3, Some(&peer_names)).await.unwrap();
    
    let messages = get_conversation_messages(&peer1_str).await.unwrap();
    assert_eq!(messages.len(), 3);
    
    // Should be in chronological order
    assert_eq!(messages[0].message, "First");
    assert_eq!(messages[0].timestamp, 1000);
    assert!(!messages[0].is_outgoing);
    
    assert_eq!(messages[1].message, "Second");
    assert_eq!(messages[1].timestamp, 2000);
    assert!(!messages[1].is_outgoing);
    
    assert_eq!(messages[2].message, "Third (outgoing)");
    assert_eq!(messages[2].timestamp, 3000);
    assert!(messages[2].is_outgoing);
    
    cleanup_test_db(&db_path).await;
}

#[tokio::test]
async fn test_get_conversation_messages_empty() {
    let db_path = setup_test_db().await.unwrap();
    
    let messages = get_conversation_messages("nonexistent_peer").await.unwrap();
    assert!(messages.is_empty(), "Should return empty vector for nonexistent conversation");
    
    cleanup_test_db(&db_path).await;
}

#[tokio::test]
async fn test_mark_conversation_messages_as_read() {
    let db_path = setup_test_db().await.unwrap();
    
    let peer1_id = create_test_peer_id();
    let peer1_str = peer1_id.to_string();
    let local_str = "local";
    
    let mut peer_names = HashMap::new();
    peer_names.insert(peer1_id, "Alice".to_string());
    
    // Create several unread incoming messages
    for i in 1..=3 {
        let msg = create_test_direct_message(&peer1_str, local_str, &format!("Message {}", i), 1000 + i, false);
        save_direct_message(&msg, Some(&peer_names)).await.unwrap();
    }
    
    // Add an outgoing message (should not be affected)
    let outgoing = DirectMessage {
        from_peer_id: local_str.to_string(),
        from_name: local_str.to_string(),
        to_peer_id: peer1_str.clone(),
        to_name: "Alice".to_string(),
        message: "Outgoing".to_string(),
        timestamp: 2000,
        is_outgoing: true,
    };
    save_direct_message(&outgoing, Some(&peer_names)).await.unwrap();
    
    // Verify unread count before marking as read
    let conversations_before = get_conversations_with_status().await.unwrap();
    assert_eq!(conversations_before[0].unread_count, 3);
    
    // Mark conversation messages as read
    mark_conversation_messages_as_read(&peer1_str).await.unwrap();
    
    // Verify unread count after marking as read
    let conversations_after = get_conversations_with_status().await.unwrap();
    assert_eq!(conversations_after[0].unread_count, 0);
    
    // Note: is_read is tracked in the database, not in the DirectMessage struct
    // The fact that unread_count is 0 confirms messages were marked as read
    
    cleanup_test_db(&db_path).await;
}

#[tokio::test]
async fn test_conversation_peer_name_update() {
    let db_path = setup_test_db().await.unwrap();
    
    // Start with default peer ID as name
    let peer_id = create_test_peer_id();
    let peer_str = peer_id.to_string();
    let local_str = "local";
    let msg1 = create_test_direct_message(&peer_str, local_str, "First message", 1000, false);
    save_direct_message(&msg1, None).await.unwrap();
    
    let conversations = get_conversations_with_status().await.unwrap();
    assert_eq!(conversations[0].peer_name, peer_str);
    
    // Add a message with peer names mapping
    let mut peer_names = HashMap::new();
    peer_names.insert(peer_id, "Alice".to_string());
    
    let msg2 = create_test_direct_message(&peer_str, local_str, "Second message", 2000, false);
    save_direct_message(&msg2, Some(&peer_names)).await.unwrap();
    
    let conversations_updated = get_conversations_with_status().await.unwrap();
    assert_eq!(conversations_updated[0].peer_name, "Alice");
    
    cleanup_test_db(&db_path).await;
}

#[tokio::test]
async fn test_conversation_with_no_messages_last_activity() {
    let db_path = setup_test_db().await.unwrap();
    
    // Create a conversation by saving a message, then delete all messages
    // This tests the edge case where a conversation exists but has no messages
    let peer1_id = create_test_peer_id();
    let peer1_str = peer1_id.to_string();
    let local_str = "local";
    
    let mut peer_names = HashMap::new();
    peer_names.insert(peer1_id, "Alice".to_string());
    
    let msg = create_test_direct_message(&peer1_str, local_str, "Test", 1000, false);
    save_direct_message(&msg, Some(&peer_names)).await.unwrap();
    
    // Manually delete the message to test the NULLS LAST behavior
    let conn_arc = get_db_connection().await.unwrap();
    let conn = conn_arc.lock().await;
    conn.execute("DELETE FROM direct_messages", []).unwrap();
    drop(conn);
    drop(conn_arc);
    
    let conversations = get_conversations_with_status().await.unwrap();
    // The conversation should still exist but with no last activity
    if !conversations.is_empty() {
        assert_eq!(conversations[0].last_activity, 0);
        assert_eq!(conversations[0].unread_count, 0);
    }
    
    cleanup_test_db(&db_path).await;
}