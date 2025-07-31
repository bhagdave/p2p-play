use p2p_play::types::*;

#[test]
fn test_story_creation() {
    let story = Story::new(
        1,
        "Test Story".to_string(),
        "Test Header".to_string(),
        "Test Body".to_string(),
        false,
    );

    assert_eq!(story.id, 1);
    assert_eq!(story.name, "Test Story");
    assert_eq!(story.header, "Test Header");
    assert_eq!(story.body, "Test Body");
    assert!(!story.is_public());
}

#[test]
fn test_story_publicity() {
    let mut story = Story::new(
        1,
        "Test".to_string(),
        "Header".to_string(),
        "Body".to_string(),
        false,
    );

    assert!(!story.is_public());
    story.set_public(true);
    assert!(story.is_public());
}

#[test]
fn test_story_serialization() {
    let story = Story::new(
        42,
        "Serialization Test".to_string(),
        "JSON Header".to_string(),
        "JSON Body".to_string(),
        true,
    );

    let json = serde_json::to_string(&story).unwrap();
    let deserialized: Story = serde_json::from_str(&json).unwrap();

    assert_eq!(story, deserialized);
}

#[test]
fn test_list_request_creation() {
    let req_all = ListRequest::new_all();
    assert_eq!(req_all.mode, ListMode::ALL);

    let peer_id = "12D3KooWTest".to_string();
    let req_one = ListRequest::new_one(peer_id.clone());
    assert_eq!(req_one.mode, ListMode::One(peer_id));
}

#[test]
fn test_list_response_creation() {
    let story = Story::new(
        1,
        "Test".to_string(),
        "H".to_string(),
        "B".to_string(),
        true,
    );
    let stories = vec![story];

    let response = ListResponse::new(ListMode::ALL, "receiver123".to_string(), stories.clone());

    assert_eq!(response.mode, ListMode::ALL);
    assert_eq!(response.receiver, "receiver123");
    assert_eq!(response.data, stories);
}

#[test]
fn test_published_story_creation() {
    let story = Story::new(
        1,
        "Pub".to_string(),
        "Header".to_string(),
        "Body".to_string(),
        true,
    );
    let publisher = "publisher123".to_string();

    let published = PublishedStory::new(story.clone(), publisher.clone());

    assert_eq!(published.story, story);
    assert_eq!(published.publisher, publisher);
}

#[test]
fn test_list_request_serialization() {
    let req = ListRequest::new_all();
    let json = serde_json::to_string(&req).unwrap();
    let deserialized: ListRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(req, deserialized);
}

#[test]
fn test_list_response_serialization() {
    let story = Story::new(
        1,
        "Test".to_string(),
        "H".to_string(),
        "B".to_string(),
        true,
    );
    let response = ListResponse::new(ListMode::ALL, "test_receiver".to_string(), vec![story]);

    let json = serde_json::to_string(&response).unwrap();
    let deserialized: ListResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(response, deserialized);
}

#[test]
fn test_published_story_serialization() {
    let story = Story::new(
        1,
        "Test".to_string(),
        "H".to_string(),
        "B".to_string(),
        true,
    );
    let published = PublishedStory::new(story, "publisher".to_string());

    let json = serde_json::to_string(&published).unwrap();
    let deserialized: PublishedStory = serde_json::from_str(&json).unwrap();
    assert_eq!(published, deserialized);
}

#[test]
fn test_peer_name_creation() {
    let peer_name = PeerName::new("12D3KooWTest".to_string(), "Alice".to_string());
    assert_eq!(peer_name.peer_id, "12D3KooWTest");
    assert_eq!(peer_name.name, "Alice");
}

#[test]
fn test_peer_name_serialization() {
    let peer_name = PeerName::new("12D3KooWTest".to_string(), "Bob".to_string());

    let json = serde_json::to_string(&peer_name).unwrap();
    let deserialized: PeerName = serde_json::from_str(&json).unwrap();
    assert_eq!(peer_name, deserialized);
}

#[test]
fn test_story_equality() {
    let story1 = Story::new(
        1,
        "Test".to_string(),
        "Header".to_string(),
        "Body".to_string(),
        true,
    );
    let story2 = Story::new(
        1,
        "Test".to_string(),
        "Header".to_string(),
        "Body".to_string(),
        true,
    );
    let story3 = Story::new(
        2,
        "Test".to_string(),
        "Header".to_string(),
        "Body".to_string(),
        true,
    );

    assert_eq!(story1, story2);
    assert_ne!(story1, story3);
}

#[test]
fn test_peer_name_equality() {
    let peer1 = PeerName::new("peer1".to_string(), "Alice".to_string());
    let peer2 = PeerName::new("peer1".to_string(), "Alice".to_string());
    let peer3 = PeerName::new("peer2".to_string(), "Alice".to_string());

    assert_eq!(peer1, peer2);
    assert_ne!(peer1, peer3);
}

#[test]
fn test_list_mode_equality() {
    let mode1 = ListMode::ALL;
    let mode2 = ListMode::ALL;
    let mode3 = ListMode::One("peer123".to_string());
    let mode4 = ListMode::One("peer123".to_string());
    let mode5 = ListMode::One("peer456".to_string());

    assert_eq!(mode1, mode2);
    assert_eq!(mode3, mode4);
    assert_ne!(mode1, mode3);
    assert_ne!(mode3, mode5);
}

#[test]
fn test_published_story_fields() {
    let story = Story::new(
        1,
        "Test".to_string(),
        "Header".to_string(),
        "Body".to_string(),
        true,
    );
    let published = PublishedStory::new(story.clone(), "publisher123".to_string());

    assert_eq!(published.story, story);
    assert_eq!(published.publisher, "publisher123");
}

#[test]
fn test_list_response_fields() {
    let stories = vec![
        Story::new(
            1,
            "Story1".to_string(),
            "H1".to_string(),
            "B1".to_string(),
            true,
        ),
        Story::new(
            2,
            "Story2".to_string(),
            "H2".to_string(),
            "B2".to_string(),
            true,
        ),
    ];
    let response = ListResponse::new(ListMode::ALL, "receiver".to_string(), stories.clone());

    assert_eq!(response.mode, ListMode::ALL);
    assert_eq!(response.receiver, "receiver");
    assert_eq!(response.data, stories);
}

#[test]
fn test_empty_story_collections() {
    let empty_stories: Stories = vec![];
    let response = ListResponse::new(ListMode::ALL, "receiver".to_string(), empty_stories.clone());

    assert_eq!(response.data.len(), 0);
    assert!(response.data.is_empty());
}

#[test]
fn test_story_with_empty_strings() {
    let story = Story::new(0, "".to_string(), "".to_string(), "".to_string(), false);

    assert_eq!(story.id, 0);
    assert_eq!(story.name, "");
    assert_eq!(story.header, "");
    assert_eq!(story.body, "");
    assert!(!story.public);
}

#[test]
fn test_story_with_large_id() {
    let large_id = usize::MAX;
    let story = Story::new(
        large_id,
        "Test".to_string(),
        "Header".to_string(),
        "Body".to_string(),
        true,
    );

    assert_eq!(story.id, large_id);
}

#[test]
fn test_story_clone() {
    let story1 = Story::new(
        1,
        "Test".to_string(),
        "Header".to_string(),
        "Body".to_string(),
        true,
    );
    let story2 = story1.clone();

    assert_eq!(story1, story2);
    // Ensure they are separate instances
    assert_eq!(story1.id, story2.id);
    assert_eq!(story1.name, story2.name);
}

#[test]
fn test_direct_message_creation() {
    let dm = DirectMessage::new(
        "peer123".to_string(),
        "Alice".to_string(),
        "Bob".to_string(),
        "Hello Bob!".to_string(),
    );

    assert_eq!(dm.from_peer_id, "peer123");
    assert_eq!(dm.from_name, "Alice");
    assert_eq!(dm.to_name, "Bob");
    assert_eq!(dm.message, "Hello Bob!");
    assert!(dm.timestamp > 0);
}

#[test]
fn test_direct_message_serialization() {
    let dm = DirectMessage::new(
        "peer456".to_string(),
        "Charlie".to_string(),
        "David".to_string(),
        "Test message with special chars: 🌍!".to_string(),
    );

    let json = serde_json::to_string(&dm).unwrap();
    let deserialized: DirectMessage = serde_json::from_str(&json).unwrap();

    assert_eq!(dm, deserialized);
}

#[test]
fn test_direct_message_equality() {
    let dm1 = DirectMessage {
        from_peer_id: "peer1".to_string(),
        from_name: "Alice".to_string(),
        to_name: "Bob".to_string(),
        message: "Hello".to_string(),
        timestamp: 1234567890,
    };
    let dm2 = DirectMessage {
        from_peer_id: "peer1".to_string(),
        from_name: "Alice".to_string(),
        to_name: "Bob".to_string(),
        message: "Hello".to_string(),
        timestamp: 1234567890,
    };
    let dm3 = DirectMessage {
        from_peer_id: "peer1".to_string(),
        from_name: "Alice".to_string(),
        to_name: "Bob".to_string(),
        message: "Different message".to_string(),
        timestamp: 1234567890,
    };

    assert_eq!(dm1, dm2);
    assert_ne!(dm1, dm3);
}

#[test]
fn test_action_result_variants() {
    let refresh = ActionResult::RefreshStories;
    let start_creation = ActionResult::StartStoryCreation;

    assert_eq!(refresh, ActionResult::RefreshStories);
    assert_eq!(start_creation, ActionResult::StartStoryCreation);
    assert_ne!(refresh, start_creation);
}

#[test]
fn test_story_new_with_channel() {
    let story = Story::new_with_channel(
        1,
        "Test Story".to_string(),
        "Test Header".to_string(),
        "Test Body".to_string(),
        true,
        "custom_channel".to_string(),
    );

    assert_eq!(story.id, 1);
    assert_eq!(story.name, "Test Story");
    assert_eq!(story.header, "Test Header");
    assert_eq!(story.body, "Test Body");
    assert!(story.public);
    assert_eq!(story.channel, "custom_channel");
}

#[test]
fn test_story_is_public() {
    let mut public_story = Story::new(
        1,
        "Public Story".to_string(),
        "Header".to_string(),
        "Body".to_string(),
        true,
    );
    let private_story = Story::new(
        2,
        "Private Story".to_string(),
        "Header".to_string(),
        "Body".to_string(),
        false,
    );

    assert!(public_story.is_public());
    assert!(!private_story.is_public());

    // Test set_public
    public_story.set_public(false);
    assert!(!public_story.is_public());
}

#[test]
fn test_story_set_public() {
    let mut story = Story::new(
        1,
        "Story".to_string(),
        "Header".to_string(),
        "Body".to_string(),
        false,
    );

    assert!(!story.public);
    story.set_public(true);
    assert!(story.public);
    story.set_public(false);
    assert!(!story.public);
}

#[test]
fn test_list_request_new_all() {
    let request = ListRequest::new_all();

    match request.mode {
        ListMode::ALL => assert!(true),
        ListMode::One(_) => panic!("Expected ListMode::ALL"),
    }
}

#[test]
fn test_list_request_new_one() {
    let peer_id = "peer123".to_string();
    let request = ListRequest::new_one(peer_id.clone());

    match request.mode {
        ListMode::One(id) => assert_eq!(id, peer_id),
        ListMode::ALL => panic!("Expected ListMode::One"),
    }
}

#[test]
fn test_list_response_new() {
    let stories = vec![Story::new(
        1,
        "Test".to_string(),
        "Header".to_string(),
        "Body".to_string(),
        true,
    )];

    let response = ListResponse::new(ListMode::ALL, "receiver123".to_string(), stories.clone());

    assert_eq!(response.mode, ListMode::ALL);
    assert_eq!(response.receiver, "receiver123");
    assert_eq!(response.data, stories);
}

#[test]
fn test_published_story_new() {
    let story = Story::new(
        1,
        "Test Story".to_string(),
        "Header".to_string(),
        "Body".to_string(),
        true,
    );
    let publisher = "publisher123".to_string();

    let published = PublishedStory::new(story.clone(), publisher.clone());

    assert_eq!(published.story, story);
    assert_eq!(published.publisher, publisher);
}

#[test]
fn test_peer_name_new() {
    let peer_id = "peer456".to_string();
    let name = "Alice".to_string();

    let peer_name = PeerName::new(peer_id.clone(), name.clone());

    assert_eq!(peer_name.peer_id, peer_id);
    assert_eq!(peer_name.name, name);
}

#[test]
fn test_direct_message_new() {
    let from_peer_id = "sender123".to_string();
    let from_name = "Alice".to_string();
    let to_name = "Bob".to_string();
    let message = "Hello Bob!".to_string();

    let dm = DirectMessage::new(
        from_peer_id.clone(),
        from_name.clone(),
        to_name.clone(),
        message.clone(),
    );

    assert_eq!(dm.from_peer_id, from_peer_id);
    assert_eq!(dm.from_name, from_name);
    assert_eq!(dm.to_name, to_name);
    assert_eq!(dm.message, message);
    assert!(dm.timestamp > 0); // Should have a valid timestamp
}

#[test]
fn test_channel_new() {
    let name = "test_channel".to_string();
    let description = "Test channel description".to_string();
    let created_by = "creator123".to_string();

    let channel = Channel::new(name.clone(), description.clone(), created_by.clone());

    assert_eq!(channel.name, name);
    assert_eq!(channel.description, description);
    assert_eq!(channel.created_by, created_by);
    assert!(channel.created_at > 0); // Should have a valid timestamp
}

#[test]
fn test_channel_subscription_new() {
    let peer_id = "peer789".to_string();
    let channel_name = "general".to_string();

    let subscription = ChannelSubscription::new(peer_id.clone(), channel_name.clone());

    assert_eq!(subscription.peer_id, peer_id);
    assert_eq!(subscription.channel_name, channel_name);
    assert!(subscription.subscribed_at > 0); // Should have a valid timestamp
}

#[test]
fn test_event_type_variants_construction() {
    // Test that all EventType variants can be constructed with the unused types
    let peer_name = PeerName::new("peer123".to_string(), "Alice".to_string());
    let _peer_name_event = EventType::PeerName(peer_name);

    let direct_msg = DirectMessage::new(
        "peer123".to_string(),
        "Alice".to_string(),
        "Bob".to_string(),
        "Hello".to_string(),
    );
    let _direct_msg_event = EventType::DirectMessage(direct_msg);

    let channel = Channel::new(
        "test".to_string(),
        "Test channel".to_string(),
        "creator".to_string(),
    );
    let _channel_event = EventType::Channel(channel);

    let subscription = ChannelSubscription::new("peer123".to_string(), "general".to_string());
    let _subscription_event = EventType::ChannelSubscription(subscription);

    // This test mainly ensures the variants can be constructed without panic
}

#[test]
fn test_bootstrap_config_new() {
    let config = BootstrapConfig::new();
    assert_eq!(config.bootstrap_peers.len(), 2);
    assert_eq!(config.retry_interval_ms, 5000);
    assert_eq!(config.max_retry_attempts, 5);
    assert_eq!(config.bootstrap_timeout_ms, 30000);
    assert!(
        config.bootstrap_peers.contains(
            &"/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN"
                .to_string()
        )
    );
}

#[test]
fn test_bootstrap_config_default() {
    let config = BootstrapConfig::default();
    let new_config = BootstrapConfig::new();
    assert_eq!(config.bootstrap_peers, new_config.bootstrap_peers);
    assert_eq!(config.retry_interval_ms, new_config.retry_interval_ms);
}

#[test]
fn test_bootstrap_config_add_peer() {
    let mut config = BootstrapConfig::new();
    let test_peer =
        "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN"
            .to_string();

    let initial_len = config.bootstrap_peers.len();
    assert!(config.add_peer(test_peer.clone()));
    assert_eq!(config.bootstrap_peers.len(), initial_len + 1);
    assert!(config.bootstrap_peers.contains(&test_peer));

    // Adding duplicate should return false and not increase length
    assert!(!config.add_peer(test_peer.clone()));
    assert_eq!(config.bootstrap_peers.len(), initial_len + 1);
}

#[test]
fn test_bootstrap_config_remove_peer() {
    let mut config = BootstrapConfig::new();
    let test_peer =
        "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN"
            .to_string();

    // Add peer first
    config.add_peer(test_peer.clone());
    let len_after_add = config.bootstrap_peers.len();

    // Remove peer
    assert!(config.remove_peer(&test_peer));
    assert_eq!(config.bootstrap_peers.len(), len_after_add - 1);
    assert!(!config.bootstrap_peers.contains(&test_peer));

    // Removing non-existent peer should return false
    assert!(!config.remove_peer(&test_peer));
    assert_eq!(config.bootstrap_peers.len(), len_after_add - 1);
}

#[test]
fn test_bootstrap_config_clear_peers() {
    let mut config = BootstrapConfig::new();
    assert!(!config.bootstrap_peers.is_empty());

    config.clear_peers();
    assert!(config.bootstrap_peers.is_empty());
}

#[test]
fn test_bootstrap_config_validate() {
    let mut config = BootstrapConfig::new();

    // Valid config should pass
    assert!(config.validate().is_ok());

    // Empty peers should fail
    config.clear_peers();
    assert!(config.validate().is_err());
    assert!(
        config
            .validate()
            .unwrap_err()
            .contains("No bootstrap peers")
    );

    // Invalid multiaddr should fail
    config.bootstrap_peers.push("invalid-multiaddr".to_string());
    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid multiaddr"));

    // Valid multiaddr should pass
    config.bootstrap_peers.clear();
    config.bootstrap_peers.push(
        "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN"
            .to_string(),
    );
    assert!(config.validate().is_ok());
}

#[test]
fn test_bootstrap_config_serialization() {
    let config = BootstrapConfig::new();

    let json = serde_json::to_string(&config).unwrap();
    let deserialized: BootstrapConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(config, deserialized);
}

#[test]
fn test_channel_subscriptions_type_alias() {
    // Test the ChannelSubscriptions type alias
    let subscription1 = ChannelSubscription::new("peer1".to_string(), "general".to_string());
    let subscription2 = ChannelSubscription::new("peer2".to_string(), "tech".to_string());

    let subscriptions: ChannelSubscriptions = vec![subscription1, subscription2];

    assert_eq!(subscriptions.len(), 2);
    assert_eq!(subscriptions[0].peer_id, "peer1");
    assert_eq!(subscriptions[0].channel_name, "general");
    assert_eq!(subscriptions[1].peer_id, "peer2");
    assert_eq!(subscriptions[1].channel_name, "tech");
}
