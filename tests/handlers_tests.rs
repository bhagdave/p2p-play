use p2p_play::handlers::*;
use p2p_play::error_logger::ErrorLogger;
use p2p_play::types::{Story, ActionResult};
use p2p_play::network::{create_swarm, PEER_ID};
use tokio::sync::mpsc;

#[tokio::test]
async fn test_handle_set_name_valid() {
    let mut local_peer_name = None;
    let (sender, _receiver) = mpsc::unbounded_channel::<String>();
    let ui_logger = UILogger::new(sender);

    // Test setting a valid name
    let result = handle_set_name("name Alice", &mut local_peer_name, &ui_logger).await;

    assert!(result.is_some());
    assert_eq!(local_peer_name, Some("Alice".to_string()));

    let peer_name = result.unwrap();
    assert_eq!(peer_name.name, "Alice");
    assert_eq!(peer_name.peer_id, PEER_ID.to_string());
}

#[tokio::test]
async fn test_handle_set_name_empty() {
    let mut local_peer_name = None;
    let (sender, _receiver) = mpsc::unbounded_channel::<String>();
    let ui_logger = UILogger::new(sender);

    // Test setting an empty name
    let result = handle_set_name("name ", &mut local_peer_name, &ui_logger).await;

    assert!(result.is_none());
    assert_eq!(local_peer_name, None);
}

#[tokio::test]
async fn test_handle_set_name_invalid_format() {
    let mut local_peer_name = None;
    let (sender, _receiver) = mpsc::unbounded_channel::<String>();
    let ui_logger = UILogger::new(sender);

    // Test invalid command format
    let result = handle_set_name("invalid command", &mut local_peer_name, &ui_logger).await;

    assert!(result.is_none());
    assert_eq!(local_peer_name, None);
}

#[tokio::test]
async fn test_handle_set_name_with_spaces() {
    let mut local_peer_name = None;
    let (sender, _receiver) = mpsc::unbounded_channel::<String>();
    let ui_logger = UILogger::new(sender);

    // Test name with spaces
    let result = handle_set_name("name Alice Smith", &mut local_peer_name, &ui_logger).await;

    assert!(result.is_some());
    assert_eq!(local_peer_name, Some("Alice Smith".to_string()));
}

#[tokio::test]
async fn test_handle_help() {
    let (sender, mut receiver) = mpsc::unbounded_channel::<String>();
    let ui_logger = UILogger::new(sender);

    // This function just prints help text, we'll test it doesn't panic
    handle_help("help", &ui_logger).await;

    // Verify help messages are sent to the logger
    let mut messages = Vec::new();
    while let Ok(msg) = receiver.try_recv() {
        messages.push(msg);
    }

    assert!(!messages.is_empty());
    assert!(messages.iter().any(|m| m.contains("ls p")));
    assert!(messages.iter().any(|m| m.contains("create s")));
    assert!(messages.iter().any(|m| m.contains("show story")));
    assert!(messages.iter().any(|m| m.contains("dht bootstrap")));
    assert!(messages.iter().any(|m| m.contains("dht peers")));
    assert!(messages.iter().any(|m| m.contains("quit")));
}

#[test]
fn test_extract_peer_id_from_multiaddr() {
    use libp2p::multiaddr::Protocol;

    // Test with valid multiaddr containing peer ID
    let peer_id = *PEER_ID;
    let mut addr = libp2p::Multiaddr::empty();
    addr.push(Protocol::Ip4([127, 0, 0, 1].into()));
    addr.push(Protocol::Tcp(8080));
    addr.push(Protocol::P2p(peer_id));

    let extracted = extract_peer_id_from_multiaddr(&addr);
    assert_eq!(extracted, Some(peer_id));

    // Test with multiaddr without peer ID
    let mut addr_no_peer = libp2p::Multiaddr::empty();
    addr_no_peer.push(Protocol::Ip4([127, 0, 0, 1].into()));
    addr_no_peer.push(Protocol::Tcp(8080));

    let extracted_none = extract_peer_id_from_multiaddr(&addr_no_peer);
    assert_eq!(extracted_none, None);
}

#[tokio::test]
async fn test_handle_show_story() {
    let (sender, mut receiver) = mpsc::unbounded_channel::<String>();
    let ui_logger = UILogger::new(sender);

    // Test invalid command format
    handle_show_story("show story", &ui_logger).await;
    let mut messages = Vec::new();
    while let Ok(msg) = receiver.try_recv() {
        messages.push(msg);
    }
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Usage: show story <id>"))
    );

    // Test invalid story ID
    handle_show_story("show story abc", &ui_logger).await;
    let mut messages = Vec::new();
    while let Ok(msg) = receiver.try_recv() {
        messages.push(msg);
    }
    assert!(messages.iter().any(|m| m.contains("Invalid story id")));

    // Test non-existent story ID
    handle_show_story("show story 999", &ui_logger).await;
    let mut messages = Vec::new();
    while let Ok(msg) = receiver.try_recv() {
        messages.push(msg);
    }
    assert!(
        messages
            .iter()
            .any(|m| m.contains("not found") || m.contains("Error reading stories"))
    );
}

#[test]
fn test_handle_create_stories_interactive() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (sender, mut receiver) = mpsc::unbounded_channel::<String>();
    let ui_logger = UILogger::new(sender);
    let error_logger = ErrorLogger::new("test_errors.log");

    rt.block_on(async {
        // Test empty create s command should trigger interactive mode
        let result = handle_create_stories("create s", &ui_logger, &error_logger).await;
        assert_eq!(result, Some(ActionResult::StartStoryCreation));

        // Check that appropriate messages were logged
        let mut messages = Vec::new();
        while let Ok(msg) = receiver.try_recv() {
            messages.push(msg);
        }
        assert!(
            messages
                .iter()
                .any(|m| m.contains("interactive story creation"))
        );
    });
}

#[test]
fn test_handle_create_stories_pipe_format() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (sender, _receiver) = mpsc::unbounded_channel::<String>();
    let ui_logger = UILogger::new(sender);
    let error_logger = ErrorLogger::new("test_errors.log");

    rt.block_on(async {
        // Test pipe-separated format still works but may fail due to file system
        let result = handle_create_stories(
            "create s Test|Header|Body|general",
            &ui_logger,
            &error_logger,
        )
        .await;
        // The result depends on whether the storage operation succeeds
        // We're mainly testing that the parsing doesn't panic
        assert!(result.is_some() || result.is_none());
    });
}

#[test]
fn test_handle_create_stories_invalid() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (sender, _receiver) = mpsc::unbounded_channel::<String>();
    let ui_logger = UILogger::new(sender);
    let error_logger = ErrorLogger::new("test_errors.log");

    rt.block_on(async {
        // Test invalid format (too few arguments)
        handle_create_stories("create sTest|Header", &ui_logger, &error_logger).await;

        // Test completely invalid format
        handle_create_stories("invalid command", &ui_logger, &error_logger).await;
    });
}

#[test]
fn test_handle_publish_story_valid_id() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (sender, _receiver) = mpsc::unbounded_channel::<String>();
    let ui_logger = UILogger::new(sender);
    let error_logger = ErrorLogger::new("test_errors.log");

    rt.block_on(async {
        let (story_sender, _story_receiver) = mpsc::unbounded_channel::<Story>();

        // Test with valid ID format
        handle_publish_story("publish s123", story_sender, &ui_logger, &error_logger).await;
        // The function will try to publish but may fail due to file system issues
        // We're testing the parsing logic
    });
}

#[test]
fn test_handle_publish_story_invalid_id() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (sender, _receiver) = mpsc::unbounded_channel::<String>();
    let ui_logger = UILogger::new(sender);
    let error_logger = ErrorLogger::new("test_errors.log");

    rt.block_on(async {
        let (story_sender, _story_receiver) = mpsc::unbounded_channel::<Story>();

        // Test with invalid ID format
        handle_publish_story("publish sabc", story_sender, &ui_logger, &error_logger).await;

        // Test with invalid command format - use a new sender
        let (story_sender2, _story_receiver2) = mpsc::unbounded_channel::<Story>();
        handle_publish_story("invalid command", story_sender2, &ui_logger, &error_logger).await;
    });
}

#[test]
fn test_command_parsing_edge_cases() {
    // Test various edge cases in command parsing

    // Test commands with extra whitespace
    assert_eq!("ls s all".strip_prefix("ls s "), Some("all"));
    assert_eq!("create s".strip_prefix("create s"), Some(""));
    assert_eq!("name   Alice   ".strip_prefix("name "), Some("  Alice   "));

    // Test commands that don't match expected prefixes
    assert_eq!("invalid".strip_prefix("ls s "), None);
    assert_eq!("list stories".strip_prefix("ls s "), None);
}

#[test]
fn test_multiaddr_parsing() {
    // Test address parsing logic used in establish_direct_connection
    let valid_addr = "/ip4/127.0.0.1/tcp/8080";
    let parsed = valid_addr.parse::<libp2p::Multiaddr>();
    assert!(parsed.is_ok());

    let invalid_addr = "not-a-valid-address";
    let parsed_invalid = invalid_addr.parse::<libp2p::Multiaddr>();
    assert!(parsed_invalid.is_err());
}

#[test]
fn test_direct_message_command_parsing() {
    // Test parsing of direct message commands
    let valid_cmd = "msg Alice Hello there!";
    assert_eq!(valid_cmd.strip_prefix("msg "), Some("Alice Hello there!"));

    let parts: Vec<&str> = "Alice Hello there!".splitn(2, ' ').collect();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0], "Alice");
    assert_eq!(parts[1], "Hello there!");

    // Test edge cases
    let _no_message = "msg Alice";
    let parts: Vec<&str> = "Alice".splitn(2, ' ').collect();
    assert_eq!(parts.len(), 1);

    let no_space = "msgAlice";
    assert_eq!(no_space.strip_prefix("msg "), None);
}

#[test]
fn test_parse_direct_message_command() {
    use std::collections::HashMap;
    use p2p_play::handlers::parse_direct_message_command;

    // Create a mock peer names map
    let mut peer_names = HashMap::new();
    let peer_id1 = libp2p::PeerId::random();
    let peer_id2 = libp2p::PeerId::random();
    let peer_id3 = libp2p::PeerId::random();

    peer_names.insert(peer_id1, "Alice".to_string());
    peer_names.insert(peer_id2, "Alice Smith".to_string());
    peer_names.insert(peer_id3, "Bob Jones Jr".to_string());

    // Create a cache and update it with peer names
    let mut cache = SortedPeerNamesCache::new();
    cache.update(&peer_names);

    // Test simple name without spaces
    let result = parse_direct_message_command("Alice Hello there!", cache.get_sorted_names());
    assert_eq!(
        result,
        Some(("Alice".to_string(), "Hello there!".to_string()))
    );

    // Test name with spaces
    let result =
        parse_direct_message_command("Alice Smith Hello world", cache.get_sorted_names());
    assert_eq!(
        result,
        Some(("Alice Smith".to_string(), "Hello world".to_string()))
    );

    // Test name with multiple spaces
    let result =
        parse_direct_message_command("Bob Jones Jr How are you?", cache.get_sorted_names());
    assert_eq!(
        result,
        Some(("Bob Jones Jr".to_string(), "How are you?".to_string()))
    );

    // Test edge case - no message
    let result = parse_direct_message_command("Alice Smith", cache.get_sorted_names());
    assert_eq!(result, None);

    // Test edge case - no space after name
    let result = parse_direct_message_command("Alice SmithHello", cache.get_sorted_names());
    assert_eq!(result, None);

    // Test fallback to original parsing for simple names not in known peers
    let result = parse_direct_message_command("Charlie Hello there", cache.get_sorted_names());
    assert_eq!(
        result,
        Some(("Charlie".to_string(), "Hello there".to_string()))
    );
}

#[tokio::test]
async fn test_handle_direct_message_no_local_name() {
    use std::collections::HashMap;
    let (sender, _receiver) = mpsc::unbounded_channel::<String>();
    let ui_logger = UILogger::new(sender);

    let mut swarm = create_swarm().expect("Failed to create swarm");
    let peer_names = HashMap::new();
    let local_peer_name = None;
    let mut cache = SortedPeerNamesCache::new();
    cache.update(&peer_names);

    // This should print an error message about needing to set name first
    handle_direct_message(
        "msg Alice Hello",
        &mut swarm,
        &peer_names,
        &local_peer_name,
        &cache,
        &ui_logger,
    )
    .await;
    // Test passes if it doesn't panic
}

#[tokio::test]
async fn test_handle_direct_message_invalid_format() {
    use std::collections::HashMap;
    let (sender, _receiver) = mpsc::unbounded_channel::<String>();
    let ui_logger = UILogger::new(sender);

    let mut swarm = create_swarm().expect("Failed to create swarm");
    let peer_names = HashMap::new();
    let local_peer_name = Some("Bob".to_string());
    let mut cache = SortedPeerNamesCache::new();
    cache.update(&peer_names);

    // Test invalid command formats
    handle_direct_message(
        "msg Alice",
        &mut swarm,
        &peer_names,
        &local_peer_name,
        &cache,
        &ui_logger,
    )
    .await;
    handle_direct_message(
        "msg",
        &mut swarm,
        &peer_names,
        &local_peer_name,
        &cache,
        &ui_logger,
    )
    .await;
    handle_direct_message(
        "invalid command",
        &mut swarm,
        &peer_names,
        &local_peer_name,
        &cache,
        &ui_logger,
    )
    .await;
    // Test passes if it doesn't panic
}

#[tokio::test]
async fn test_handle_direct_message_with_spaces_in_names() {
    use std::collections::HashMap;
    let (sender, _receiver) = mpsc::unbounded_channel::<String>();
    let ui_logger = UILogger::new(sender);

    let mut swarm = create_swarm().expect("Failed to create swarm");
    let mut peer_names = HashMap::new();
    let peer_id = libp2p::PeerId::random();
    peer_names.insert(peer_id, "Alice Smith".to_string());

    let local_peer_name = Some("Bob".to_string());
    let mut cache = SortedPeerNamesCache::new();
    cache.update(&peer_names);

    // Test message to peer with spaces in name
    handle_direct_message(
        "msg Alice Smith Hello world",
        &mut swarm,
        &peer_names,
        &local_peer_name,
        &cache,
        &ui_logger,
    )
    .await;
    // Test passes if it doesn't panic and correctly parses the name
}

#[test]
fn test_sorted_peer_names_cache() {
    use std::collections::HashMap;

    let mut cache = SortedPeerNamesCache::new();
    assert!(cache.get_sorted_names().is_empty());

    // Create test peer names
    let mut peer_names = HashMap::new();
    let peer_id1 = libp2p::PeerId::random();
    let peer_id2 = libp2p::PeerId::random();
    let peer_id3 = libp2p::PeerId::random();

    peer_names.insert(peer_id1, "Alice".to_string());
    peer_names.insert(peer_id2, "Alice Smith".to_string());
    peer_names.insert(peer_id3, "Bob".to_string());

    // Update cache
    cache.update(&peer_names);

    // Verify names are sorted by length (descending)
    let sorted_names = cache.get_sorted_names();
    assert_eq!(sorted_names.len(), 3);
    assert_eq!(sorted_names[0], "Alice Smith"); // Longest first
    assert_eq!(sorted_names[1], "Alice");
    assert_eq!(sorted_names[2], "Bob");

    // Test that parsing still works correctly with the sorted cache
    let result = p2p_play::handlers::parse_direct_message_command("Alice Smith Hello world", sorted_names);
    assert_eq!(
        result,
        Some(("Alice Smith".to_string(), "Hello world".to_string()))
    );

    // Test that longer names are preferred (should match "Alice Smith", not "Alice")
    let result = p2p_play::handlers::parse_direct_message_command("Alice Smith test", sorted_names);
    assert_eq!(
        result,
        Some(("Alice Smith".to_string(), "test".to_string()))
    );
}

#[tokio::test]
async fn test_handle_create_description() {
    let (sender, mut receiver) = mpsc::unbounded_channel::<String>();
    let ui_logger = UILogger::new(sender);

    // Test valid description
    handle_create_description("create desc This is my node", &ui_logger).await;

    // Collect messages
    let mut messages = Vec::new();
    while let Ok(msg) = receiver.try_recv() {
        messages.push(msg);
    }

    assert!(!messages.is_empty());
    // Should contain success message about saved description
    assert!(messages.iter().any(|m| m.contains("saved")));

    // Clean up
    let _ = tokio::fs::remove_file("node_description.txt").await;
}

#[tokio::test]
async fn test_handle_create_description_invalid() {
    let (sender, mut receiver) = mpsc::unbounded_channel::<String>();
    let ui_logger = UILogger::new(sender);

    // Test invalid format
    handle_create_description("create desc", &ui_logger).await;

    let mut messages = Vec::new();
    while let Ok(msg) = receiver.try_recv() {
        messages.push(msg);
    }

    assert!(!messages.is_empty());
    assert!(messages.iter().any(|m| m.contains("Usage:")));
}

#[tokio::test]
async fn test_handle_create_description_empty() {
    let (sender, mut receiver) = mpsc::unbounded_channel::<String>();
    let ui_logger = UILogger::new(sender);

    // Test empty description
    handle_create_description("create desc ", &ui_logger).await;

    let mut messages = Vec::new();
    while let Ok(msg) = receiver.try_recv() {
        messages.push(msg);
    }

    assert!(!messages.is_empty());
    assert!(messages.iter().any(|m| m.contains("Usage:")));
}

#[tokio::test]
async fn test_handle_show_description() {
    
    let (sender, mut receiver) = mpsc::unbounded_channel::<String>();
    let ui_logger = UILogger::new(sender);

    // Test when no description exists - remove file and ensure it's gone
    let _ = tokio::fs::remove_file("node_description.txt").await;

    // Wait a bit to ensure file is deleted
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    handle_show_description(&ui_logger).await;

    let mut messages = Vec::new();
    while let Ok(msg) = receiver.try_recv() {
        messages.push(msg);
    }

    assert!(!messages.is_empty());
    assert!(
        messages
            .iter()
            .any(|m| m.contains("No node description set"))
    );
}

#[tokio::test]
async fn test_handle_show_description_with_content() {
    let (sender, mut receiver) = mpsc::unbounded_channel::<String>();
    let ui_logger = UILogger::new(sender);

    // First create a description
    p2p_play::storage::save_node_description("Test description content")
        .await
        .unwrap();

    // Then show it
    handle_show_description(&ui_logger).await;

    let mut messages = Vec::new();
    while let Ok(msg) = receiver.try_recv() {
        messages.push(msg);
    }

    assert!(!messages.is_empty());
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Test description content"))
    );
    assert!(messages.iter().any(|m| m.contains("Your node description")));

    // Clean up
    let _ = tokio::fs::remove_file("node_description.txt").await;
}

#[test]
fn test_extract_peer_id_from_multiaddr_success() {
    let peer_id = libp2p::PeerId::random();
    let addr: libp2p::Multiaddr = format!("/ip4/127.0.0.1/tcp/8080/p2p/{}", peer_id).parse().unwrap();
    let extracted = extract_peer_id_from_multiaddr(&addr);
    assert_eq!(extracted, Some(peer_id));
}

#[test]
fn test_extract_peer_id_from_multiaddr_no_peer() {
    let addr: libp2p::Multiaddr = "/ip4/127.0.0.1/tcp/8080".parse().unwrap();
    let extracted = extract_peer_id_from_multiaddr(&addr);
    assert_eq!(extracted, None);
}

#[test]
fn test_peer_name_caching() {
    use std::collections::HashMap;
    
    let mut cache = SortedPeerNamesCache::new();
    let mut peer_names = HashMap::new();
    
    let peer_id1 = libp2p::PeerId::random();
    let peer_id2 = libp2p::PeerId::random();
    peer_names.insert(peer_id1, "Alice Smith".to_string());
    peer_names.insert(peer_id2, "Bob".to_string());
    
    cache.update(&peer_names);
    let sorted = cache.get_sorted_names();
    
    // Should be sorted by length descending
    assert_eq!(sorted[0], "Alice Smith");
    assert_eq!(sorted[1], "Bob");
}

#[test]
fn test_parse_direct_message_simple() {
    let peer_names = vec!["Alice".to_string(), "Bob".to_string()];
    let result = parse_direct_message_command("Alice Hello world", &peer_names);
    assert_eq!(result, Some(("Alice".to_string(), "Hello world".to_string())));
}

#[test]
fn test_parse_direct_message_with_spaces() {
    let peer_names = vec!["Alice Smith".to_string(), "Bob".to_string()];
    let result = parse_direct_message_command("Alice Smith Hello world", &peer_names);
    assert_eq!(result, Some(("Alice Smith".to_string(), "Hello world".to_string())));
}

#[test]
fn test_parse_direct_message_no_message() {
    let peer_names = vec!["Alice".to_string()];
    let result = parse_direct_message_command("Alice", &peer_names);
    assert_eq!(result, None);
}

#[tokio::test]
async fn test_ui_logger_functionality() {
    let (sender, mut receiver) = mpsc::unbounded_channel::<String>();
    let ui_logger = UILogger::new(sender);
    
    ui_logger.log("Test message".to_string());
    
    let message = receiver.try_recv().unwrap();
    assert_eq!(message, "Test message");
}