use libp2p::PeerId;
use p2p_play::storage::core::*;
use p2p_play::types::DirectMessage;
use p2p_play::ui::AppEvent;
use std::collections::HashMap;
use tokio;
use uuid::Uuid;

async fn setup_integration_test_db() -> Result<String, Box<dyn std::error::Error>> {
    let unique_db_path = format!("/tmp/test_conversation_integration_{}.db", Uuid::new_v4());
    unsafe {
        std::env::set_var("TEST_DATABASE_PATH", &unique_db_path);
    }
    reset_db_connection_for_testing().await?;
    ensure_stories_file_exists().await?;
    Ok(unique_db_path)
}

async fn cleanup_integration_db(db_path: &str) {
    let _ = std::fs::remove_file(db_path);
}

async fn create_test_peer_names() -> (HashMap<PeerId, String>, PeerId, PeerId, PeerId) {
    let peer1 = PeerId::random();
    let peer2 = PeerId::random();
    let peer3 = PeerId::random();

    let mut peer_names = HashMap::new();
    peer_names.insert(peer1, "Alice".to_string());
    peer_names.insert(peer2, "Bob".to_string());
    peer_names.insert(peer3, "Charlie".to_string());

    (peer_names, peer1, peer2, peer3)
}

#[tokio::test]
async fn test_complete_conversation_flow_with_multiple_participants() {
    let db_path = setup_integration_test_db().await.unwrap();
    let (peer_names, peer1, peer2, _peer3) = create_test_peer_names().await;
    let peer1_str = peer1.to_string();
    let peer2_str = peer2.to_string();

    // Simulate receiving messages from multiple peers
    let messages = vec![
        DirectMessage {
            from_peer_id: peer1_str.clone(),
            from_name: "Alice".to_string(),
            to_peer_id: "local".to_string(),
            to_name: "Local".to_string(),
            message: "Hello from Alice!".to_string(),
            timestamp: 1000,
            is_outgoing: false,
        },
        DirectMessage {
            from_peer_id: peer2_str.clone(),
            from_name: "Bob".to_string(),
            to_peer_id: "local".to_string(),
            to_name: "Local".to_string(),
            message: "Hi there from Bob!".to_string(),
            timestamp: 2000,
            is_outgoing: false,
        },
        DirectMessage {
            from_peer_id: peer1_str.clone(),
            from_name: "Alice".to_string(),
            to_peer_id: "local".to_string(),
            to_name: "Local".to_string(),
            message: "How are you doing?".to_string(),
            timestamp: 3000,
            is_outgoing: false,
        },
        // Outgoing message to Alice
        DirectMessage {
            from_peer_id: "local".to_string(),
            from_name: "Local".to_string(),
            to_peer_id: peer1_str.clone(),
            to_name: "Alice".to_string(),
            message: "I'm doing well, thanks!".to_string(),
            timestamp: 3500,
            is_outgoing: true,
        },
    ];

    // Save all messages
    for message in &messages {
        save_direct_message(message, Some(&peer_names))
            .await
            .unwrap();
    }

    // Test conversation list
    let conversations = get_conversations_with_status().await.unwrap();
    assert_eq!(conversations.len(), 2);

    // Should be sorted by last activity (Alice conversation has latest activity)
    assert_eq!(conversations[0].peer_name, "Alice");
    assert_eq!(conversations[0].unread_count, 2); // 2 unread incoming messages
    assert_eq!(conversations[0].last_activity, 3500); // Last outgoing message

    assert_eq!(conversations[1].peer_name, "Bob");
    assert_eq!(conversations[1].unread_count, 1); // 1 unread incoming message
    assert_eq!(conversations[1].last_activity, 2000);

    // Test getting messages for Alice's conversation
    let alice_messages = get_conversation_messages(&peer1_str).await.unwrap();
    assert_eq!(alice_messages.len(), 3); // 2 incoming + 1 outgoing

    // Should be in chronological order
    assert_eq!(alice_messages[0].message, "Hello from Alice!");
    assert_eq!(alice_messages[0].timestamp, 1000);
    assert!(!alice_messages[0].is_outgoing);

    assert_eq!(alice_messages[1].message, "How are you doing?");
    assert_eq!(alice_messages[1].timestamp, 3000);
    assert!(!alice_messages[1].is_outgoing);

    assert_eq!(alice_messages[2].message, "I'm doing well, thanks!");
    assert_eq!(alice_messages[2].timestamp, 3500);
    assert!(alice_messages[2].is_outgoing);

    // Test marking conversation as read
    mark_conversation_messages_as_read(&peer1_str)
        .await
        .unwrap();

    let updated_conversations = get_conversations_with_status().await.unwrap();
    let alice_conversation = updated_conversations
        .iter()
        .find(|c| c.peer_name == "Alice")
        .unwrap();
    assert_eq!(alice_conversation.unread_count, 0);

    // Bob's conversation should still have unread messages
    let bob_conversation = updated_conversations
        .iter()
        .find(|c| c.peer_name == "Bob")
        .unwrap();
    assert_eq!(bob_conversation.unread_count, 1);

    cleanup_integration_db(&db_path).await;
}

#[tokio::test]
async fn test_conversation_view_event_processing() {
    let db_path = setup_integration_test_db().await.unwrap();
    let (peer_names, peer1, _peer2, _peer3) = create_test_peer_names().await;
    let peer1_str = peer1.to_string();

    // Set up a conversation with unread messages
    let message = DirectMessage {
        from_peer_id: peer1_str.clone(),
        from_name: "Alice".to_string(),
        to_peer_id: "local".to_string(),
        to_name: "Local".to_string(),
        message: "Test message".to_string(),
        timestamp: 1000,
        is_outgoing: false,
    };
    save_direct_message(&message, Some(&peer_names))
        .await
        .unwrap();

    // Verify conversation has unread message
    let conversations_before = get_conversations_with_status().await.unwrap();
    assert_eq!(conversations_before[0].unread_count, 1);

    // Simulate ConversationViewed event processing
    // This would typically be handled by the event processor
    let conversation_viewed_event = AppEvent::ConversationViewed {
        peer_id: peer1_str.clone(),
    };

    // Process the event (marking messages as read)
    match conversation_viewed_event {
        AppEvent::ConversationViewed { peer_id } => {
            mark_conversation_messages_as_read(&peer_id).await.unwrap();
        }
        _ => unreachable!(),
    }

    // Verify conversation is now marked as read
    let conversations_after = get_conversations_with_status().await.unwrap();
    assert_eq!(conversations_after[0].unread_count, 0);

    cleanup_integration_db(&db_path).await;
}

#[tokio::test]
async fn test_conversation_persistence_across_sessions() {
    let db_path = setup_integration_test_db().await.unwrap();
    let (peer_names, peer1, _peer2, _peer3) = create_test_peer_names().await;
    let peer1_str = peer1.to_string();

    // Simulate first session - create conversation
    let message1 = DirectMessage {
        from_peer_id: peer1_str.clone(),
        from_name: "Alice".to_string(),
        to_peer_id: "local".to_string(),
        to_name: "Local".to_string(),
        message: "Message from session 1".to_string(),
        timestamp: 1000,
        is_outgoing: false,
    };
    save_direct_message(&message1, Some(&peer_names))
        .await
        .unwrap();

    // Mark as read
    mark_conversation_messages_as_read(&peer1_str)
        .await
        .unwrap();

    // Simulate second session - add more messages
    let message2 = DirectMessage {
        from_peer_id: peer1_str.clone(),
        from_name: "Alice".to_string(),
        to_peer_id: "local".to_string(),
        to_name: "Local".to_string(),
        message: "Message from session 2".to_string(),
        timestamp: 2000,
        is_outgoing: false,
    };
    save_direct_message(&message2, Some(&peer_names))
        .await
        .unwrap();

    // Test that conversation persists with updated state
    let conversations = get_conversations_with_status().await.unwrap();
    assert_eq!(conversations.len(), 1);
    assert_eq!(conversations[0].peer_name, "Alice");
    assert_eq!(conversations[0].unread_count, 1); // Only new message is unread
    assert_eq!(conversations[0].last_activity, 2000);

    // Test that all messages are retrieved
    let all_messages = get_conversation_messages(&peer1_str).await.unwrap();
    assert_eq!(all_messages.len(), 2);

    // Messages should be in chronological order
    assert_eq!(all_messages[0].message, "Message from session 1");
    assert_eq!(all_messages[1].message, "Message from session 2");

    cleanup_integration_db(&db_path).await;
}

#[tokio::test]
async fn test_peer_name_updates_in_conversations() {
    let db_path = setup_integration_test_db().await.unwrap();
    let peer1 = PeerId::random();
    let peer1_str = peer1.to_string();

    // Initially save message without peer name
    let message1 = DirectMessage {
        from_peer_id: peer1_str.clone(),
        from_name: peer1_str.clone(),
        to_peer_id: "local".to_string(),
        to_name: "Local".to_string(),
        message: "Initial message".to_string(),
        timestamp: 1000,
        is_outgoing: false,
    };
    save_direct_message(&message1, None).await.unwrap();

    // Conversation should use peer_id as name initially
    let conversations_initial = get_conversations_with_status().await.unwrap();
    assert_eq!(conversations_initial[0].peer_name, peer1_str);

    // Add message with peer name mapping
    let mut peer_names = HashMap::new();
    peer_names.insert(peer1, "Alice".to_string());

    let message2 = DirectMessage {
        from_peer_id: peer1_str.clone(),
        from_name: "Alice".to_string(),
        to_peer_id: "local".to_string(),
        to_name: "Local".to_string(),
        message: "Message with name".to_string(),
        timestamp: 2000,
        is_outgoing: false,
    };
    save_direct_message(&message2, Some(&peer_names))
        .await
        .unwrap();

    // Conversation should now use the friendly name
    let conversations_updated = get_conversations_with_status().await.unwrap();
    assert_eq!(conversations_updated[0].peer_name, "Alice");
    assert_eq!(conversations_updated[0].unread_count, 2);

    cleanup_integration_db(&db_path).await;
}

#[tokio::test]
async fn test_conversation_ordering_with_mixed_message_types() {
    let db_path = setup_integration_test_db().await.unwrap();
    let (peer_names, peer1, peer2, _peer3) = create_test_peer_names().await;
    let peer1_str = peer1.to_string();
    let peer2_str = peer2.to_string();

    // Create conversations with different types of last activity

    // Conversation 1: Last activity is incoming message
    let msg1 = DirectMessage {
        from_peer_id: peer1_str.clone(),
        from_name: "Alice".to_string(),
        to_peer_id: "local".to_string(),
        to_name: "Local".to_string(),
        message: "Latest incoming".to_string(),
        timestamp: 3000,
        is_outgoing: false,
    };

    // Conversation 2: Last activity is outgoing message
    let msg2_in = DirectMessage {
        from_peer_id: peer2_str.clone(),
        from_name: "Bob".to_string(),
        to_peer_id: "local".to_string(),
        to_name: "Local".to_string(),
        message: "Earlier incoming".to_string(),
        timestamp: 1000,
        is_outgoing: false,
    };
    let msg2_out = DirectMessage {
        from_peer_id: "local".to_string(),
        from_name: "Local".to_string(),
        to_peer_id: peer2_str.clone(),
        to_name: "Bob".to_string(),
        message: "Later outgoing".to_string(),
        timestamp: 2000,
        is_outgoing: true,
    };

    // Save messages
    save_direct_message(&msg2_in, Some(&peer_names))
        .await
        .unwrap();
    save_direct_message(&msg2_out, Some(&peer_names))
        .await
        .unwrap();
    save_direct_message(&msg1, Some(&peer_names)).await.unwrap();

    let conversations = get_conversations_with_status().await.unwrap();
    assert_eq!(conversations.len(), 2);

    // Should be ordered by last activity (timestamp 3000 > 2000)
    assert_eq!(conversations[0].peer_name, "Alice"); // timestamp 3000
    assert_eq!(conversations[0].last_activity, 3000);
    assert_eq!(conversations[0].unread_count, 1); // One unread incoming

    assert_eq!(conversations[1].peer_name, "Bob"); // timestamp 2000
    assert_eq!(conversations[1].last_activity, 2000);
    assert_eq!(conversations[1].unread_count, 1); // One unread incoming (outgoing doesn't count)

    cleanup_integration_db(&db_path).await;
}

#[tokio::test]
async fn test_large_conversation_performance() {
    let db_path = setup_integration_test_db().await.unwrap();
    let (peer_names, peer1, _peer2, _peer3) = create_test_peer_names().await;
    let peer1_str = peer1.to_string();

    // Create a conversation with many messages
    for i in 0..100 {
        let message = DirectMessage {
            from_peer_id: peer1_str.clone(),
            from_name: "Alice".to_string(),
            to_peer_id: "local".to_string(),
            to_name: "Local".to_string(),
            message: format!("Message {}", i),
            timestamp: 1000 + i,
            is_outgoing: false,
        };
        save_direct_message(&message, Some(&peer_names))
            .await
            .unwrap();
    }

    // Test conversation list performance
    let start = std::time::Instant::now();
    let conversations = get_conversations_with_status().await.unwrap();
    let list_duration = start.elapsed();

    assert_eq!(conversations.len(), 1);
    assert_eq!(conversations[0].unread_count, 100);
    assert_eq!(conversations[0].last_activity, 1099);

    // Should complete quickly (less than 100ms)
    assert!(list_duration.as_millis() < 100);

    // Test individual conversation retrieval performance
    let start = std::time::Instant::now();
    let messages = get_conversation_messages(&peer1_str).await.unwrap();
    let messages_duration = start.elapsed();

    assert_eq!(messages.len(), 100);
    // Should be in chronological order
    assert_eq!(messages[0].message, "Message 0");
    assert_eq!(messages[99].message, "Message 99");

    // Should complete quickly (less than 100ms)
    assert!(messages_duration.as_millis() < 100);

    cleanup_integration_db(&db_path).await;
}

#[tokio::test]
async fn test_conversation_with_no_peer_names_mapping() {
    let db_path = setup_integration_test_db().await.unwrap();
    let peer_id = PeerId::random();
    let peer_str = peer_id.to_string();

    // Save messages without any peer names mapping
    let message = DirectMessage {
        from_peer_id: peer_str.clone(),
        from_name: peer_str.clone(),
        to_peer_id: "local".to_string(),
        to_name: "Local".to_string(),
        message: "Anonymous message".to_string(),
        timestamp: 1000,
        is_outgoing: false,
    };
    save_direct_message(&message, None).await.unwrap();

    let conversations = get_conversations_with_status().await.unwrap();
    assert_eq!(conversations.len(), 1);

    // Should use the full peer_id as the display name
    assert_eq!(conversations[0].peer_name, peer_str);
    assert_eq!(conversations[0].peer_id, peer_str);
    assert_eq!(conversations[0].unread_count, 1);

    let messages = get_conversation_messages(&peer_str).await.unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].message, "Anonymous message");

    cleanup_integration_db(&db_path).await;
}
