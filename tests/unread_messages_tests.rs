use libp2p::PeerId;
use p2p_play::storage::core::*;
use p2p_play::types::DirectMessage;
use std::collections::HashMap;
use uuid::Uuid;

async fn setup_test_db() -> Result<String, Box<dyn std::error::Error>> {
    let unique_db_path = format!("/tmp/test_unread_{}.db", Uuid::new_v4());
    unsafe {
        std::env::set_var("TEST_DATABASE_PATH", &unique_db_path);
    }
    reset_db_connection_for_testing().await?;
    ensure_stories_file_exists().await?;
    Ok(unique_db_path)
}

async fn cleanup_test_db(db_path: &str) {
    let _ = std::fs::remove_file(db_path);
}

fn make_dm(from: &str, to: &str, msg: &str, ts: u64, is_outgoing: bool) -> DirectMessage {
    DirectMessage {
        from_peer_id: from.to_string(),
        from_name: from.to_string(),
        to_peer_id: to.to_string(),
        to_name: to.to_string(),
        message: msg.to_string(),
        timestamp: ts,
        is_outgoing,
    }
}

// ---------------------------------------------------------------------------
// get_unread_messages tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_unread_messages_empty_db() {
    let db_path = setup_test_db().await.unwrap();

    let result = get_unread_messages(50).await.unwrap();
    assert!(result.is_empty(), "Expected no unread messages in empty DB");

    cleanup_test_db(&db_path).await;
}

#[tokio::test]
async fn test_get_unread_messages_returns_incoming_unread() {
    let db_path = setup_test_db().await.unwrap();

    let peer_id = PeerId::random().to_string();
    let local = "local";

    let dm = make_dm(&peer_id, local, "hello!", 1000, false);
    save_direct_message(&dm, None).await.unwrap();

    let unread = get_unread_messages(50).await.unwrap();
    assert_eq!(unread.len(), 1);
    assert_eq!(unread[0].from_peer_id, peer_id);
    assert_eq!(unread[0].message, "hello!");

    cleanup_test_db(&db_path).await;
}

#[tokio::test]
async fn test_get_unread_messages_excludes_outgoing() {
    let db_path = setup_test_db().await.unwrap();

    let peer_id = PeerId::random().to_string();
    let local = "local";

    // outgoing messages must not appear as unread
    let outgoing = make_dm(local, &peer_id, "sent by me", 1000, true);
    save_direct_message(&outgoing, None).await.unwrap();

    let unread = get_unread_messages(50).await.unwrap();
    assert!(
        unread.is_empty(),
        "Outgoing messages must not appear in unread list"
    );

    cleanup_test_db(&db_path).await;
}

#[tokio::test]
async fn test_get_unread_messages_excludes_already_read() {
    let db_path = setup_test_db().await.unwrap();

    let peer_id = PeerId::random().to_string();
    let local = "local";

    let dm = make_dm(&peer_id, local, "read me", 1000, false);
    save_direct_message(&dm, None).await.unwrap();

    // mark the conversation as read
    mark_conversation_messages_as_read(&peer_id).await.unwrap();

    let unread = get_unread_messages(50).await.unwrap();
    assert!(
        unread.is_empty(),
        "Messages marked as read must not appear in unread list"
    );

    cleanup_test_db(&db_path).await;
}

#[tokio::test]
async fn test_get_unread_messages_respects_limit() {
    let db_path = setup_test_db().await.unwrap();

    let peer_id = PeerId::random().to_string();
    let local = "local";

    // Save 5 unread messages
    for i in 0..5u64 {
        let dm = make_dm(&peer_id, local, &format!("msg {i}"), 1000 + i, false);
        save_direct_message(&dm, None).await.unwrap();
    }

    let unread = get_unread_messages(3).await.unwrap();
    assert_eq!(
        unread.len(),
        3,
        "Limit of 3 must be respected; got {}",
        unread.len()
    );

    cleanup_test_db(&db_path).await;
}

#[tokio::test]
async fn test_get_unread_messages_ordered_by_timestamp_ascending() {
    let db_path = setup_test_db().await.unwrap();

    let peer_id = PeerId::random().to_string();
    let local = "local";
    let mut peer_names = HashMap::new();
    peer_names.insert(
        peer_id.parse::<libp2p::PeerId>().unwrap(),
        "alice".to_string(),
    );

    // Insert in reverse timestamp order
    let dm3 = make_dm(&peer_id, local, "third", 3000, false);
    let dm1 = make_dm(&peer_id, local, "first", 1000, false);
    let dm2 = make_dm(&peer_id, local, "second", 2000, false);

    save_direct_message(&dm3, None).await.unwrap();
    save_direct_message(&dm1, None).await.unwrap();
    save_direct_message(&dm2, None).await.unwrap();

    let unread = get_unread_messages(50).await.unwrap();
    assert_eq!(unread.len(), 3);
    assert_eq!(unread[0].message, "first");
    assert_eq!(unread[1].message, "second");
    assert_eq!(unread[2].message, "third");

    cleanup_test_db(&db_path).await;
}

#[tokio::test]
async fn test_get_unread_messages_multiple_peers() {
    let db_path = setup_test_db().await.unwrap();

    let peer1 = PeerId::random().to_string();
    let peer2 = PeerId::random().to_string();
    let local = "local";

    save_direct_message(&make_dm(&peer1, local, "from p1 a", 1000, false), None)
        .await
        .unwrap();
    save_direct_message(&make_dm(&peer2, local, "from p2 a", 2000, false), None)
        .await
        .unwrap();
    save_direct_message(&make_dm(&peer1, local, "from p1 b", 3000, false), None)
        .await
        .unwrap();

    let unread = get_unread_messages(50).await.unwrap();
    assert_eq!(unread.len(), 3);

    // Confirm both peers appear
    let peer_ids: Vec<&str> = unread.iter().map(|m| m.from_peer_id.as_str()).collect();
    assert!(peer_ids.contains(&peer1.as_str()));
    assert!(peer_ids.contains(&peer2.as_str()));

    cleanup_test_db(&db_path).await;
}

#[tokio::test]
async fn test_get_messages_from_peer_alias_returns_only_matching_incoming_messages() {
    let db_path = setup_test_db().await.unwrap();

    let peer1 = PeerId::random();
    let peer2 = PeerId::random();
    let local = "local";
    let mut peer_names = HashMap::new();
    peer_names.insert(peer1, "Alice".to_string());
    peer_names.insert(peer2, "Bob".to_string());
    let peer1_id = peer1.to_string();
    let peer2_id = peer2.to_string();

    save_direct_message(
        &make_dm(&peer1_id, local, "from alice 1", 1000, false),
        Some(&peer_names),
    )
    .await
    .unwrap();
    save_direct_message(
        &make_dm(local, &peer1_id, "to alice", 2000, true),
        Some(&peer_names),
    )
    .await
    .unwrap();
    save_direct_message(
        &make_dm(&peer1_id, local, "from alice 2", 3000, false),
        Some(&peer_names),
    )
    .await
    .unwrap();
    save_direct_message(
        &make_dm(&peer2_id, local, "from bob", 4000, false),
        Some(&peer_names),
    )
    .await
    .unwrap();

    let messages = get_incoming_messages_from_peer_alias("alice")
        .await
        .unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].message, "from alice 1");
    assert_eq!(messages[1].message, "from alice 2");
    assert!(messages.iter().all(|m| !m.is_outgoing));

    cleanup_test_db(&db_path).await;
}

#[tokio::test]
async fn test_get_messages_from_peer_alias_empty_when_alias_missing() {
    let db_path = setup_test_db().await.unwrap();

    let messages = get_incoming_messages_from_peer_alias("nobody")
        .await
        .unwrap();
    assert!(messages.is_empty());

    cleanup_test_db(&db_path).await;
}
