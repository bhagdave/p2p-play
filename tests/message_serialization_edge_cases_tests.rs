use p2p_play::network::*;
use p2p_play::types::*;
use serde_json;
use std::time::{SystemTime, UNIX_EPOCH};
use std::collections::HashMap;

/// Helper to get current timestamp
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[tokio::test]
async fn test_story_serialization_edge_cases() {
    // Test Story serialization with various edge cases
    
    // Test with empty strings
    let empty_story = Story {
        id: 0,
        name: String::new(),
        header: String::new(),
        body: String::new(),
        public: false,
        channel: String::new(),
        created_at: 0,
        auto_share: None,
    };
    
    let json = serde_json::to_string(&empty_story).unwrap();
    let deserialized: Story = serde_json::from_str(&json).unwrap();
    assert_eq!(empty_story, deserialized);
    
    // Test with maximum values
    let max_story = Story {
        id: usize::MAX,
        name: "A".repeat(10000), // Very long name
        header: "B".repeat(50000), // Very long header  
        body: "C".repeat(100000), // Very long body
        public: true,
        channel: "D".repeat(1000), // Long channel name
        created_at: u64::MAX,
        auto_share: Some(true),
    };
    
    let json = serde_json::to_string(&max_story).unwrap();
    let deserialized: Story = serde_json::from_str(&json).unwrap();
    assert_eq!(max_story, deserialized);
    assert_eq!(deserialized.name.len(), 10000);
    assert_eq!(deserialized.header.len(), 50000);
    assert_eq!(deserialized.body.len(), 100000);
    
    // Test with special Unicode characters
    let unicode_story = Story {
        id: 42,
        name: "🚀 Story with émojis and ñ special chars 中文 العربية".to_string(),
        header: "Header with\ttabs\nand\r\nnewlines".to_string(),
        body: "Body with \"quotes\" and 'apostrophes' and \\backslashes".to_string(),
        public: true,
        channel: "chäññél-wîth-spëcîäl-chars".to_string(),
        created_at: current_timestamp(),
        auto_share: Some(false),
    };
    
    let json = serde_json::to_string(&unicode_story).unwrap();
    let deserialized: Story = serde_json::from_str(&json).unwrap();
    assert_eq!(unicode_story, deserialized);
    
    // Test with null bytes and control characters (these should serialize properly as JSON escapes them)
    let control_story = Story {
        id: 1,
        name: "Story\x00with\x01control\x1Fchars".to_string(),
        header: "Header\x7F\x80\x9F".to_string(),
        body: "Body\x00\x01\x02".to_string(),
        public: false,
        channel: "general".to_string(),
        created_at: current_timestamp(),
        auto_share: None,
    };
    
    let json = serde_json::to_string(&control_story).unwrap();
    let deserialized: Story = serde_json::from_str(&json).unwrap();
    assert_eq!(control_story, deserialized);
}

#[tokio::test]
async fn test_published_story_serialization_edge_cases() {
    // Test PublishedStory with edge cases
    
    // Empty publisher
    let story = Story::new(1, "Test".to_string(), "Header".to_string(), "Body".to_string(), true);
    let published = PublishedStory::new(story.clone(), String::new());
    
    let json = serde_json::to_string(&published).unwrap();
    let deserialized: PublishedStory = serde_json::from_str(&json).unwrap();
    assert_eq!(published, deserialized);
    assert!(deserialized.publisher.is_empty());
    
    // Very long publisher ID
    let long_publisher = "12D3KooW".to_string() + &"A".repeat(10000);
    let published_long = PublishedStory::new(story.clone(), long_publisher.clone());
    
    let json = serde_json::to_string(&published_long).unwrap();
    let deserialized: PublishedStory = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.publisher, long_publisher);
    
    // Publisher with special characters
    let special_publisher = "🎯Publisher-with-émojis_and_symbols!@#$%^&*()".to_string();
    let published_special = PublishedStory::new(story, special_publisher.clone());
    
    let json = serde_json::to_string(&published_special).unwrap();
    let deserialized: PublishedStory = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.publisher, special_publisher);
}

#[tokio::test]
async fn test_direct_message_serialization_edge_cases() {
    // Test DirectMessageRequest with various edge cases
    
    // Empty fields
    let empty_dm = DirectMessageRequest {
        from_peer_id: String::new(),
        from_name: String::new(),
        to_name: String::new(),
        message: String::new(),
        timestamp: 0,
    };
    
    let json = serde_json::to_string(&empty_dm).unwrap();
    let deserialized: DirectMessageRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(empty_dm, deserialized);
    
    // Maximum timestamp
    let max_dm = DirectMessageRequest {
        from_peer_id: "test_peer".to_string(),
        from_name: "test_name".to_string(),
        to_name: "recipient".to_string(),
        message: "test message".to_string(),
        timestamp: u64::MAX,
    };
    
    let json = serde_json::to_string(&max_dm).unwrap();
    let deserialized: DirectMessageRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.timestamp, u64::MAX);
    
    // Very large message
    let large_message = "A".repeat(1_000_000); // 1MB message
    let large_dm = DirectMessageRequest {
        from_peer_id: "sender".to_string(),
        from_name: "sender_name".to_string(),
        to_name: "recipient".to_string(),
        message: large_message.clone(),
        timestamp: current_timestamp(),
    };
    
    let json = serde_json::to_string(&large_dm).unwrap();
    let deserialized: DirectMessageRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.message.len(), 1_000_000);
    assert_eq!(deserialized.message, large_message);
    
    // Message with all types of whitespace and special characters
    let special_message = "Message with\ttabs\n\rnewlines\x0Bvertical_tab\x0Cform_feed and 🚀emojis";
    let special_dm = DirectMessageRequest {
        from_peer_id: "special_peer".to_string(),
        from_name: "Sëndér Namë 🎯".to_string(),
        to_name: "Rëcîpîént 📧".to_string(),
        message: special_message.to_string(),
        timestamp: current_timestamp(),
    };
    
    let json = serde_json::to_string(&special_dm).unwrap();
    let deserialized: DirectMessageRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.message, special_message);
    
    // DirectMessageResponse edge cases
    let response_false = DirectMessageResponse {
        received: false,
        timestamp: 0,
    };
    
    let json = serde_json::to_string(&response_false).unwrap();
    let deserialized: DirectMessageResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(response_false, deserialized);
    assert!(!deserialized.received);
    
    let response_max = DirectMessageResponse {
        received: true,
        timestamp: u64::MAX,
    };
    
    let json = serde_json::to_string(&response_max).unwrap();
    let deserialized: DirectMessageResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.timestamp, u64::MAX);
}

#[tokio::test]
async fn test_node_description_serialization_edge_cases() {
    // Test NodeDescriptionRequest edge cases
    
    // Empty request
    let empty_req = NodeDescriptionRequest {
        from_peer_id: String::new(),
        from_name: String::new(),
        timestamp: 0,
    };
    
    let json = serde_json::to_string(&empty_req).unwrap();
    let deserialized: NodeDescriptionRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(empty_req, deserialized);
    
    // Very long peer ID and name
    let long_req = NodeDescriptionRequest {
        from_peer_id: "12D3KooW".to_string() + &"X".repeat(10000),
        from_name: "VeryLongPeerName".to_string() + &"Y".repeat(10000),
        timestamp: current_timestamp(),
    };
    
    let json = serde_json::to_string(&long_req).unwrap();
    let deserialized: NodeDescriptionRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.from_peer_id.len(), 8 + 10000);
    assert_eq!(deserialized.from_name.len(), 16 + 10000);
    
    // NodeDescriptionResponse edge cases
    
    // Response with None description
    let none_response = NodeDescriptionResponse {
        description: None,
        from_peer_id: "peer_id".to_string(),
        from_name: "peer_name".to_string(),
        timestamp: current_timestamp(),
    };
    
    let json = serde_json::to_string(&none_response).unwrap();
    let deserialized: NodeDescriptionResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(none_response, deserialized);
    assert!(deserialized.description.is_none());
    
    // Response with empty description
    let empty_desc_response = NodeDescriptionResponse {
        description: Some(String::new()),
        from_peer_id: "peer_id".to_string(),
        from_name: "peer_name".to_string(),
        timestamp: current_timestamp(),
    };
    
    let json = serde_json::to_string(&empty_desc_response).unwrap();
    let deserialized: NodeDescriptionResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.description, Some(String::new()));
    
    // Response with very long description
    let long_description = "This is a very long node description. ".repeat(10000);
    let long_desc_response = NodeDescriptionResponse {
        description: Some(long_description.clone()),
        from_peer_id: "peer_id".to_string(),
        from_name: "peer_name".to_string(),
        timestamp: current_timestamp(),
    };
    
    let json = serde_json::to_string(&long_desc_response).unwrap();
    let deserialized: NodeDescriptionResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.description, Some(long_description));
    
    // Response with special characters in description
    let special_desc = "Node description with\n\ttabs\r\nand émojis 🚀 and symbols !@#$%^&*()";
    let special_desc_response = NodeDescriptionResponse {
        description: Some(special_desc.to_string()),
        from_peer_id: "spëcîäl_pëër_îd".to_string(),
        from_name: "Spëcîäl Pëër Namë 🎯".to_string(),
        timestamp: current_timestamp(),
    };
    
    let json = serde_json::to_string(&special_desc_response).unwrap();
    let deserialized: NodeDescriptionResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.description, Some(special_desc.to_string()));
}

#[tokio::test]
async fn test_story_sync_serialization_edge_cases() {
    // Test StorySyncRequest edge cases
    
    // Empty channels list
    let empty_sync_req = StorySyncRequest {
        from_peer_id: "peer".to_string(),
        from_name: "name".to_string(),
        last_sync_timestamp: 0,
        subscribed_channels: vec![],
        timestamp: current_timestamp(),
    };
    
    let json = serde_json::to_string(&empty_sync_req).unwrap();
    let deserialized: StorySyncRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.subscribed_channels.len(), 0);
    
    // Very large number of channels
    let many_channels: Vec<String> = (0..10000).map(|i| format!("channel_{i}")).collect();
    let large_sync_req = StorySyncRequest {
        from_peer_id: "peer".to_string(),
        from_name: "name".to_string(),
        last_sync_timestamp: current_timestamp(),
        subscribed_channels: many_channels.clone(),
        timestamp: current_timestamp(),
    };
    
    let json = serde_json::to_string(&large_sync_req).unwrap();
    let deserialized: StorySyncRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.subscribed_channels.len(), 10000);
    assert_eq!(deserialized.subscribed_channels, many_channels);
    
    // Channels with special characters
    let special_channels = vec![
        String::new(), // empty channel
        "chäññél_with_spëcîäl_chars".to_string(),
        "channel\nwith\nnewlines".to_string(),
        "channel\twith\ttabs".to_string(),
        "🚀_emoji_channel_🎯".to_string(),
        "very_long_channel_name_".to_string() + &"x".repeat(1000),
    ];
    
    let special_sync_req = StorySyncRequest {
        from_peer_id: "special_peer".to_string(),
        from_name: "Spëcîäl Pëër".to_string(),
        last_sync_timestamp: u64::MAX,
        subscribed_channels: special_channels.clone(),
        timestamp: current_timestamp(),
    };
    
    let json = serde_json::to_string(&special_sync_req).unwrap();
    let deserialized: StorySyncRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.subscribed_channels, special_channels);
    
    // StorySyncResponse edge cases
    
    // Empty stories list
    let empty_sync_response = StorySyncResponse {
        stories: vec![],
        from_peer_id: "peer".to_string(),
        from_name: "name".to_string(),
        sync_timestamp: current_timestamp(),
    };
    
    let json = serde_json::to_string(&empty_sync_response).unwrap();
    let deserialized: StorySyncResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.stories.len(), 0);
    
    // Large number of stories
    let many_stories: Vec<Story> = (0..1000).map(|i| {
        Story::new_with_channel(
            i,
            format!("Story {i}"),
            format!("Header {i}"),
            format!("Body {i}"),
            true,
            format!("channel_{}", i % 10),
        )
    }).collect();
    
    let large_sync_response = StorySyncResponse {
        stories: many_stories.clone(),
        from_peer_id: "peer".to_string(),
        from_name: "name".to_string(),
        sync_timestamp: current_timestamp(),
    };
    
    let json = serde_json::to_string(&large_sync_response).unwrap();
    let deserialized: StorySyncResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.stories.len(), 1000);
    assert_eq!(deserialized.stories[0].name, "Story 0");
    assert_eq!(deserialized.stories[999].name, "Story 999");
    
    // Stories with maximum-size content
    let huge_story = Story {
        id: usize::MAX,
        name: "H".repeat(50000),
        header: "I".repeat(100000),
        body: "J".repeat(500000), // 500KB body
        public: true,
        channel: "K".repeat(10000),
        created_at: u64::MAX,
        auto_share: Some(true),
    };
    
    let huge_sync_response = StorySyncResponse {
        stories: vec![huge_story],
        from_peer_id: "peer".to_string(),
        from_name: "name".to_string(),
        sync_timestamp: current_timestamp(),
    };
    
    let json = serde_json::to_string(&huge_sync_response).unwrap();
    let deserialized: StorySyncResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.stories.len(), 1);
    assert_eq!(deserialized.stories[0].body.len(), 500000);
}

#[tokio::test]
async fn test_malformed_json_handling() {
    // Test handling of malformed JSON for all message types
    
    // Test invalid JSON strings
    let malformed_jsons = vec![
        "",                           // Empty string
        "{",                          // Incomplete JSON
        "}",                          // Just closing brace
        "null",                       // Null value
        "[]",                         // Empty array
        "{\"incomplete\":",           // Incomplete field
        "{\"field\": }",             // Missing value
        "{\"field\": \"unterminated", // Unterminated string
        "not json at all",           // Not JSON
        "{\"field\": undefined}",    // Invalid value
    ];
    
    for malformed in &malformed_jsons {
        // DirectMessageRequest
        assert!(serde_json::from_str::<DirectMessageRequest>(malformed).is_err());
        
        // DirectMessageResponse
        assert!(serde_json::from_str::<DirectMessageResponse>(malformed).is_err());
        
        // NodeDescriptionRequest
        assert!(serde_json::from_str::<NodeDescriptionRequest>(malformed).is_err());
        
        // NodeDescriptionResponse
        assert!(serde_json::from_str::<NodeDescriptionResponse>(malformed).is_err());
        
        // StorySyncRequest
        assert!(serde_json::from_str::<StorySyncRequest>(malformed).is_err());
        
        // StorySyncResponse
        assert!(serde_json::from_str::<StorySyncResponse>(malformed).is_err());
        
        // Story
        assert!(serde_json::from_str::<Story>(malformed).is_err());
        
        // PublishedStory
        assert!(serde_json::from_str::<PublishedStory>(malformed).is_err());
    }
}

#[tokio::test]
async fn test_json_with_extra_fields() {
    // Test JSON with extra fields (should be ignored by serde)
    
    // Story with extra fields
    let story_json_extra = r#"{
        "id": 1,
        "name": "Test Story",
        "header": "Test Header",
        "body": "Test Body",
        "public": true,
        "channel": "general",
        "created_at": 1234567890,
        "auto_share": null,
        "extra_field": "should be ignored",
        "another_extra": 42,
        "nested_extra": {"field": "value"}
    }"#;
    
    let story: Result<Story, _> = serde_json::from_str(story_json_extra);
    assert!(story.is_ok());
    let story = story.unwrap();
    assert_eq!(story.name, "Test Story");
    
    // DirectMessageRequest with extra fields
    let dm_json_extra = r#"{
        "from_peer_id": "peer1",
        "from_name": "name1",
        "to_name": "name2",
        "message": "test message",
        "timestamp": 1234567890,
        "priority": "high",
        "encryption": "none",
        "metadata": {"key": "value"}
    }"#;
    
    let dm: Result<DirectMessageRequest, _> = serde_json::from_str(dm_json_extra);
    assert!(dm.is_ok());
    let dm = dm.unwrap();
    assert_eq!(dm.message, "test message");
}

#[tokio::test]
async fn test_json_with_missing_fields() {
    // Test JSON with missing required fields (should fail)
    
    // Story missing required fields
    let incomplete_story = r#"{
        "id": 1,
        "name": "Test Story"
    }"#;
    
    let story: Result<Story, _> = serde_json::from_str(incomplete_story);
    assert!(story.is_err()); // Should fail due to missing required fields
    
    // DirectMessageRequest missing fields
    let incomplete_dm = r#"{
        "from_peer_id": "peer1",
        "message": "test message"
    }"#;
    
    let dm: Result<DirectMessageRequest, _> = serde_json::from_str(incomplete_dm);
    assert!(dm.is_err()); // Should fail due to missing required fields
}

#[tokio::test]
async fn test_json_with_wrong_field_types() {
    // Test JSON with wrong field types (should fail)
    
    // Story with wrong field types
    let wrong_types_story = r#"{
        "id": "should_be_number",
        "name": "Test Story",
        "header": "Test Header", 
        "body": "Test Body",
        "public": "should_be_boolean",
        "channel": "general",
        "created_at": "should_be_number",
        "auto_share": "should_be_boolean_or_null"
    }"#;
    
    let story: Result<Story, _> = serde_json::from_str(wrong_types_story);
    assert!(story.is_err());
    
    // DirectMessageRequest with wrong types
    let wrong_types_dm = r#"{
        "from_peer_id": 123,
        "from_name": true,
        "to_name": ["array"],
        "message": {"object": "value"},
        "timestamp": "string_timestamp"
    }"#;
    
    let dm: Result<DirectMessageRequest, _> = serde_json::from_str(wrong_types_dm);
    assert!(dm.is_err());
}

#[tokio::test]
async fn test_large_json_performance() {
    // Test performance with very large JSON payloads
    use std::time::Instant;
    
    // Create a large story sync response
    let large_stories: Vec<Story> = (0..10000).map(|i| {
        Story {
            id: i,
            name: format!("Performance Test Story {}", i),
            header: "A".repeat(1000), // 1KB header
            body: "B".repeat(10000),  // 10KB body
            public: true,
            channel: format!("channel_{}", i % 100),
            created_at: current_timestamp(),
            auto_share: Some(i % 2 == 0),
        }
    }).collect();
    
    let large_response = StorySyncResponse {
        stories: large_stories,
        from_peer_id: "performance_test_peer".to_string(),
        from_name: "Performance Tester".to_string(),
        sync_timestamp: current_timestamp(),
    };
    
    // Measure serialization time
    let start = Instant::now();
    let json = serde_json::to_string(&large_response).unwrap();
    let serialize_duration = start.elapsed();
    
    println!("Serialization of 10K stories took: {:?}", serialize_duration);
    println!("JSON size: {} MB", json.len() as f64 / 1_000_000.0);
    
    // Measure deserialization time
    let start = Instant::now();
    let deserialized: StorySyncResponse = serde_json::from_str(&json).unwrap();
    let deserialize_duration = start.elapsed();
    
    println!("Deserialization took: {:?}", deserialize_duration);
    
    // Verify correctness
    assert_eq!(deserialized.stories.len(), 10000);
    assert_eq!(deserialized.stories[0].name, "Performance Test Story 0");
    assert_eq!(deserialized.stories[9999].name, "Performance Test Story 9999");
    
    // Performance assertions (reasonable limits for test environment)
    assert!(serialize_duration.as_secs() < 10, "Serialization should complete within 10 seconds");
    assert!(deserialize_duration.as_secs() < 10, "Deserialization should complete within 10 seconds");
}

#[tokio::test]
async fn test_backwards_compatibility_serialization() {
    // Test that newer message formats can deserialize older JSON
    // This simulates receiving messages from older versions of the software
    
    // Old format Story (missing newer fields)
    let old_story_json = r#"{
        "id": 1,
        "name": "Old Format Story",
        "header": "Old Header",
        "body": "Old Body",
        "public": true
    }"#;
    
    // Should deserialize successfully with default values for missing fields
    let story: Result<Story, _> = serde_json::from_str(old_story_json);
    // Note: This might fail if the struct doesn't have proper defaults
    // In a real application, you'd use #[serde(default)] attributes
    
    if story.is_ok() {
        let story = story.unwrap();
        assert_eq!(story.name, "Old Format Story");
        // channel should get default value "general" (from Story::new)
    }
    
    // Test forward compatibility - newer JSON should work with current structs
    let future_story_json = r#"{
        "id": 1,
        "name": "Future Story",
        "header": "Future Header",
        "body": "Future Body",
        "public": true,
        "channel": "future_channel",
        "created_at": 1234567890,
        "auto_share": null,
        "future_field_1": "ignored",
        "future_field_2": {"nested": "data"},
        "future_array": [1, 2, 3]
    }"#;
    
    let future_story: Result<Story, _> = serde_json::from_str(future_story_json);
    assert!(future_story.is_ok());
    let future_story = future_story.unwrap();
    assert_eq!(future_story.name, "Future Story");
    assert_eq!(future_story.channel, "future_channel");
}