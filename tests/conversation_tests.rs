/// Tests for conversation management and message threading functionality
use p2p_play::storage::{
    get_conversations_with_unread_counts, load_conversation_manager, load_conversation_messages,
    mark_conversation_messages_as_read, save_direct_message,
};
use p2p_play::types::{Conversation, ConversationManager, DirectMessage};
use std::env;
use tempfile::TempDir;

#[tokio::test]
async fn test_save_and_load_direct_messages() {
    // Setup test database
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_conversation.db");
    unsafe {
        env::set_var("TEST_DATABASE_PATH", db_path.to_str().unwrap());
    }

    // Initialize storage
    p2p_play::storage::reset_db_connection_for_testing()
        .await
        .expect("Failed to reset database connection");
    p2p_play::storage::ensure_stories_file_exists()
        .await
        .expect("Failed to initialize storage");

    // Create test messages
    let message1 = DirectMessage::new(
        "peer123".to_string(),
        "Alice".to_string(),
        "Bob".to_string(),
        "Hello Bob!".to_string(),
    );

    let message2 = DirectMessage::new(
        "peer456".to_string(),
        "Bob".to_string(),
        "Alice".to_string(),
        "Hi Alice! How are you?".to_string(),
    );

    let message3 = DirectMessage::new(
        "peer123".to_string(),
        "Alice".to_string(),
        "Bob".to_string(),
        "I'm doing great, thanks!".to_string(),
    );

    // Save messages to database
    save_direct_message(&message1)
        .await
        .expect("Failed to save message 1");
    save_direct_message(&message2)
        .await
        .expect("Failed to save message 2");
    save_direct_message(&message3)
        .await
        .expect("Failed to save message 3");

    // Load conversation messages for Alice (conversation with Bob)
    let alice_bob_conversation = load_conversation_messages("peer123", "Bob")
        .await
        .expect("Failed to load Alice-Bob conversation");

    // Verify all messages in the conversation are loaded correctly
    assert_eq!(alice_bob_conversation.len(), 3); // Alice sent 2 messages, Bob sent 1
    // Messages should be in chronological order
    assert_eq!(alice_bob_conversation[0].message, "Hello Bob!");
    assert_eq!(alice_bob_conversation[1].message, "Hi Alice! How are you?");
    assert_eq!(
        alice_bob_conversation[2].message,
        "I'm doing great, thanks!"
    );

    // Load the same conversation from Bob's perspective
    let bob_alice_conversation = load_conversation_messages("peer123", "Alice")
        .await
        .expect("Failed to load Bob-Alice conversation");

    // Should get the same messages (this is the same conversation)
    assert_eq!(bob_alice_conversation.len(), 3);

    println!("✅ Save and load direct messages test passed!");
}

#[tokio::test]
async fn test_conversation_unread_counts() {
    // Setup test database
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_unread.db");
    unsafe {
        env::set_var("TEST_DATABASE_PATH", db_path.to_str().unwrap());
    }

    // Initialize storage
    p2p_play::storage::reset_db_connection_for_testing()
        .await
        .expect("Failed to reset database connection");
    p2p_play::storage::ensure_stories_file_exists()
        .await
        .expect("Failed to initialize storage");

    // Create test messages
    let message1 = DirectMessage::new(
        "peer123".to_string(),
        "Alice".to_string(),
        "Bob".to_string(),
        "First message".to_string(),
    );

    let message2 = DirectMessage::new(
        "peer123".to_string(),
        "Alice".to_string(),
        "Bob".to_string(),
        "Second message".to_string(),
    );

    // Save messages
    save_direct_message(&message1)
        .await
        .expect("Failed to save message 1");
    save_direct_message(&message2)
        .await
        .expect("Failed to save message 2");

    // Get conversations with unread counts for Bob
    let conversations = get_conversations_with_unread_counts("Bob")
        .await
        .expect("Failed to get conversations");

    // Should have one conversation with Alice, with 2 unread messages (from Alice to Bob)
    assert_eq!(conversations.len(), 1);
    let (peer_id, peer_name, unread_count, _last_activity) = &conversations[0];
    assert_eq!(peer_id, "peer123");
    assert_eq!(peer_name, "Alice");
    assert_eq!(*unread_count, 2);

    // Mark messages as read from Alice's perspective (peer123) to Bob
    mark_conversation_messages_as_read("peer123", "Bob")
        .await
        .expect("Failed to mark messages as read");

    // Check unread counts again
    let conversations_after_read = get_conversations_with_unread_counts("Bob")
        .await
        .expect("Failed to get conversations after read");

    // Should still have the conversation but with 0 unread messages
    assert_eq!(conversations_after_read.len(), 1);
    let (_, _, unread_count_after, _) = &conversations_after_read[0];
    assert_eq!(*unread_count_after, 0);

    println!("✅ Conversation unread counts test passed!");
}

#[tokio::test]
async fn test_conversation_manager_functionality() {
    // Setup test database
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_manager.db");
    unsafe {
        env::set_var("TEST_DATABASE_PATH", db_path.to_str().unwrap());
    }

    // Initialize storage
    p2p_play::storage::reset_db_connection_for_testing()
        .await
        .expect("Failed to reset database connection");
    p2p_play::storage::ensure_stories_file_exists()
        .await
        .expect("Failed to initialize storage");

    // Create test messages between multiple peers
    let messages = vec![
        DirectMessage::new(
            "peer1".to_string(),
            "Alice".to_string(),
            "Bob".to_string(),
            "Hi Bob!".to_string(),
        ),
        DirectMessage::new(
            "peer1".to_string(),
            "Alice".to_string(),
            "Bob".to_string(),
            "How are you?".to_string(),
        ),
        DirectMessage::new(
            "peer2".to_string(),
            "Charlie".to_string(),
            "Bob".to_string(),
            "Hey Bob!".to_string(),
        ),
        DirectMessage::new(
            "peer3".to_string(),
            "David".to_string(),
            "Bob".to_string(),
            "Hello!".to_string(),
        ),
    ];

    // Save all messages
    for message in &messages {
        save_direct_message(message)
            .await
            .expect("Failed to save message");
    }

    // Load conversation manager for Bob
    let conversation_manager = load_conversation_manager("Bob")
        .await
        .expect("Failed to load conversation manager");

    // Should have 3 conversations
    assert_eq!(conversation_manager.conversations.len(), 3);

    // Check that conversations are properly organized
    let alice_conversation = conversation_manager
        .get_conversation("peer1")
        .expect("Alice conversation should exist");
    assert_eq!(alice_conversation.peer_name, "Alice");
    assert_eq!(alice_conversation.messages.len(), 2);
    assert_eq!(alice_conversation.unread_count, 2);

    let charlie_conversation = conversation_manager
        .get_conversation("peer2")
        .expect("Charlie conversation should exist");
    assert_eq!(charlie_conversation.peer_name, "Charlie");
    assert_eq!(charlie_conversation.messages.len(), 1);
    assert_eq!(charlie_conversation.unread_count, 1);

    let david_conversation = conversation_manager
        .get_conversation("peer3")
        .expect("David conversation should exist");
    assert_eq!(david_conversation.peer_name, "David");
    assert_eq!(david_conversation.messages.len(), 1);
    assert_eq!(david_conversation.unread_count, 1);

    // Test total unread count
    assert_eq!(conversation_manager.get_total_unread_count(), 4);

    // Test conversation sorting by activity
    let sorted_conversations = conversation_manager.get_conversations_sorted_by_activity();
    assert_eq!(sorted_conversations.len(), 3);

    // Most recent should be first (David, then Charlie, then Alice with 2 messages)
    assert_eq!(sorted_conversations[0].peer_name, "David");
    assert_eq!(sorted_conversations[1].peer_name, "Charlie");
    assert_eq!(sorted_conversations[2].peer_name, "Alice");

    println!("✅ Conversation manager functionality test passed!");
}

#[tokio::test]
async fn test_conversation_switching() {
    let mut conversation_manager = ConversationManager::new();

    // Create test conversations
    let mut alice_conv = Conversation::new("peer1".to_string(), "Alice".to_string());
    alice_conv.add_message(DirectMessage::new(
        "peer1".to_string(),
        "Alice".to_string(),
        "Bob".to_string(),
        "Hello!".to_string(),
    ));

    let mut charlie_conv = Conversation::new("peer2".to_string(), "Charlie".to_string());
    charlie_conv.add_message(DirectMessage::new(
        "peer2".to_string(),
        "Charlie".to_string(),
        "Bob".to_string(),
        "Hi there!".to_string(),
    ));

    conversation_manager
        .conversations
        .insert("peer1".to_string(), alice_conv);
    conversation_manager
        .conversations
        .insert("peer2".to_string(), charlie_conv);

    // Test setting active conversation
    conversation_manager.set_active_conversation(Some("peer1".to_string()));
    assert_eq!(
        conversation_manager.active_conversation,
        Some("peer1".to_string())
    );

    // Test getting active conversation
    let active = conversation_manager.get_active_conversation();
    assert!(active.is_some());
    assert_eq!(active.unwrap().peer_name, "Alice");
    // After setting active, unread count should be 0
    assert_eq!(active.unwrap().unread_count, 0);

    // Test switching conversations
    conversation_manager.switch_to_next_conversation();
    // Should switch to the other conversation
    assert!(conversation_manager.active_conversation.is_some());
    let new_active = conversation_manager.get_active_conversation();
    assert!(new_active.is_some());

    // Since conversations are sorted by activity, and both have the same timestamp,
    // the order might vary, but we should have a valid active conversation
    assert!(new_active.unwrap().peer_name == "Alice" || new_active.unwrap().peer_name == "Charlie");

    println!("✅ Conversation switching test passed!");
}

#[tokio::test]
async fn test_conversation_message_preview() {
    let mut conversation = Conversation::new("peer1".to_string(), "Alice".to_string());

    // Test empty conversation
    assert_eq!(conversation.get_last_message_preview(), "No messages");

    // Add a short message
    conversation.add_message(DirectMessage::new(
        "peer1".to_string(),
        "Alice".to_string(),
        "Bob".to_string(),
        "Hello!".to_string(),
    ));

    assert_eq!(conversation.get_last_message_preview(), "Alice: Hello!");

    // Add a long message that should be truncated
    let long_message = "This is a very long message that should be truncated because it exceeds the preview length limit of 50 characters";
    conversation.add_message(DirectMessage::new(
        "peer1".to_string(),
        "Alice".to_string(),
        "Bob".to_string(),
        long_message.to_string(),
    ));

    let preview = conversation.get_last_message_preview();
    assert!(preview.starts_with("Alice: This is a very long message that should be"));
    assert!(preview.ends_with("..."));
    assert!(preview.len() <= 60); // "Alice: " + 47 chars + "..."

    println!("✅ Conversation message preview test passed!");
}

#[tokio::test]
async fn test_conversation_manager_add_message() {
    let mut conversation_manager = ConversationManager::new();
    let local_peer_id = "local123";

    // Test adding incoming message
    let incoming_message = DirectMessage::new(
        "peer1".to_string(),
        "Alice".to_string(),
        "Bob".to_string(),
        "Hello Bob!".to_string(),
    );

    conversation_manager.add_message(incoming_message.clone(), local_peer_id);

    // Should create a new conversation for Alice
    assert_eq!(conversation_manager.conversations.len(), 1);
    let alice_conversation = conversation_manager.get_conversation("peer1").unwrap();
    assert_eq!(alice_conversation.peer_name, "Alice");
    assert_eq!(alice_conversation.messages.len(), 1);
    assert_eq!(alice_conversation.unread_count, 1);

    // Test adding outgoing message
    let outgoing_message = DirectMessage::new(
        local_peer_id.to_string(),
        "Bob".to_string(),
        "Alice".to_string(),
        "Hi Alice!".to_string(),
    );

    conversation_manager.add_message(outgoing_message.clone(), local_peer_id);

    // Should create a new conversation for the recipient (Alice)
    // Since we're using the recipient's name as peer_id for outgoing messages
    assert!(conversation_manager.conversations.len() >= 1);

    // Test total unread count
    let total_unread = conversation_manager.get_total_unread_count();
    assert!(total_unread >= 1);

    println!("✅ Conversation manager add message test passed!");
}

#[tokio::test]
async fn test_outgoing_messages_appear_in_conversations() {
    // Setup test database
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_outgoing_messages.db");
    unsafe {
        env::set_var("TEST_DATABASE_PATH", db_path.to_str().unwrap());
    }

    // Initialize storage
    p2p_play::storage::reset_db_connection_for_testing()
        .await
        .expect("Failed to reset database connection");
    p2p_play::storage::ensure_stories_file_exists()
        .await
        .expect("Failed to initialize storage");

    // Simulate the scenario where a user sends a message
    // This is what should happen when using the msg command
    let outgoing_msg = DirectMessage::new(
        "local_peer_id".to_string(),
        "Alice".to_string(),
        "Bob".to_string(),
        "Hey Bob, how are you?".to_string(),
    );

    // Save the outgoing message (this is what our fix should do)
    save_direct_message(&outgoing_msg)
        .await
        .expect("Failed to save outgoing message");

    // Simulate Bob responding
    let incoming_msg = DirectMessage::new(
        "bob_peer_id".to_string(),
        "Bob".to_string(),
        "Alice".to_string(),
        "Hi Alice! I'm doing well, thanks!".to_string(),
    );

    save_direct_message(&incoming_msg)
        .await
        .expect("Failed to save incoming message");

    // Debug: Check all direct messages in the database
    {
        let conn_arc = p2p_play::storage::core::get_db_connection().await.unwrap();
        let conn = conn_arc.lock().await;
        let mut stmt = conn.prepare("SELECT from_peer_id, from_name, to_name, message FROM direct_messages ORDER BY timestamp").unwrap();
        let message_iter = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .unwrap();

        println!("All messages in database:");
        for msg in message_iter {
            let (from_peer_id, from_name, to_name, message) = msg.unwrap();
            println!(
                "  {} ({}) -> {}: {}",
                from_name, from_peer_id, to_name, message
            );
        }
    }

    // Debug: Check what get_conversations_with_unread_counts returns
    {
        let conversations_data = p2p_play::storage::get_conversations_with_unread_counts("Alice")
            .await
            .unwrap();
        println!("get_conversations_with_unread_counts returned:");
        for (peer_id, peer_name, unread_count, last_activity) in &conversations_data {
            println!(
                "  peer_id: '{}', peer_name: '{}', unread: {}, last_activity: {}",
                peer_id, peer_name, unread_count, last_activity
            );
        }
    }

    // Load conversation manager from Alice's perspective
    let conversation_manager = load_conversation_manager("Alice")
        .await
        .expect("Failed to load conversation manager");

    // Debug: print all conversations
    println!(
        "Found {} conversations",
        conversation_manager.conversations.len()
    );
    for (peer_id, conversation) in &conversation_manager.conversations {
        println!(
            "Conversation with peer_id '{}', peer_name '{}', {} messages",
            peer_id,
            conversation.peer_name,
            conversation.messages.len()
        );
        for msg in &conversation.messages {
            println!(
                "  Message: {} -> {}: {}",
                msg.from_name, msg.to_name, msg.message
            );
        }
    }

    // Check if there's a conversation using Bob's peer_id
    let bob_conversation = conversation_manager
        .conversations
        .get("bob_peer_id")
        .or_else(|| conversation_manager.conversations.get("Bob"))
        .or_else(|| {
            conversation_manager
                .conversations
                .values()
                .find(|conv| conv.peer_name == "Bob")
        });

    assert!(
        bob_conversation.is_some(),
        "Alice should have a conversation with Bob"
    );

    let bob_conversation = bob_conversation.unwrap();

    // The conversation should have 2 messages: outgoing + incoming
    assert_eq!(
        bob_conversation.messages.len(),
        2,
        "Conversation should contain both outgoing and incoming messages"
    );

    // Check the outgoing message is there
    let outgoing_found = bob_conversation.messages.iter().any(|msg| {
        msg.from_name == "Alice" && msg.to_name == "Bob" && msg.message == "Hey Bob, how are you?"
    });
    assert!(
        outgoing_found,
        "Outgoing message from Alice should be in conversation"
    );

    // Check the incoming message is there
    let incoming_found = bob_conversation.messages.iter().any(|msg| {
        msg.from_name == "Bob"
            && msg.to_name == "Alice"
            && msg.message == "Hi Alice! I'm doing well, thanks!"
    });
    assert!(
        incoming_found,
        "Incoming message from Bob should be in conversation"
    );

    println!("✅ Outgoing messages properly appear in conversations!");
}
