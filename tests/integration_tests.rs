use p2p_play::storage::*;
use p2p_play::types::*;
use tempfile::NamedTempFile;

#[tokio::test]
async fn test_story_workflow_integration() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();

    // Create a story
    let story_id = create_new_story_in_path("Integration Test", "Test Header", "Test Body", path)
        .await
        .unwrap();

    // Verify it's created and private
    let stories = read_local_stories_from_path(path).await.unwrap();
    assert_eq!(stories.len(), 1);
    assert!(!stories[0].public);

    // Publish it
    let published = publish_story_in_path(story_id, path).await.unwrap();
    assert!(published.is_some());

    // Verify it's now public
    let stories = read_local_stories_from_path(path).await.unwrap();
    assert!(stories[0].public);

    // Simulate receiving the story on another peer
    let received_story = published.unwrap();
    let temp_file2 = NamedTempFile::new().unwrap();
    let path2 = temp_file2.path().to_str().unwrap();

    let received_id = save_received_story_to_path(received_story, path2)
        .await
        .unwrap();

    // Verify the received story
    let peer2_stories = read_local_stories_from_path(path2).await.unwrap();
    assert_eq!(peer2_stories.len(), 1);
    assert_eq!(peer2_stories[0].id, received_id);
    assert!(peer2_stories[0].public);
    assert_eq!(peer2_stories[0].name, "Integration Test");
}

#[tokio::test]
async fn test_multiple_peers_story_sharing() {
    // Simulate 3 peers sharing stories
    let peer1_file = NamedTempFile::new().unwrap();
    let peer2_file = NamedTempFile::new().unwrap();
    let peer3_file = NamedTempFile::new().unwrap();

    let peer1_path = peer1_file.path().to_str().unwrap();
    let peer2_path = peer2_file.path().to_str().unwrap();
    let peer3_path = peer3_file.path().to_str().unwrap();

    // Each peer creates a story
    let story1_id = create_new_story_in_path("Peer 1 Story", "Header 1", "Body 1", peer1_path)
        .await
        .unwrap();
    let story2_id = create_new_story_in_path("Peer 2 Story", "Header 2", "Body 2", peer2_path)
        .await
        .unwrap();
    let story3_id = create_new_story_in_path("Peer 3 Story", "Header 3", "Body 3", peer3_path)
        .await
        .unwrap();

    // Each peer publishes their story
    let published1 = publish_story_in_path(story1_id, peer1_path)
        .await
        .unwrap()
        .unwrap();
    let published2 = publish_story_in_path(story2_id, peer2_path)
        .await
        .unwrap()
        .unwrap();
    let published3 = publish_story_in_path(story3_id, peer3_path)
        .await
        .unwrap()
        .unwrap();

    // Simulate story distribution - each peer receives stories from others
    save_received_story_to_path(published2.clone(), peer1_path)
        .await
        .unwrap();
    save_received_story_to_path(published3.clone(), peer1_path)
        .await
        .unwrap();

    save_received_story_to_path(published1.clone(), peer2_path)
        .await
        .unwrap();
    save_received_story_to_path(published3.clone(), peer2_path)
        .await
        .unwrap();

    save_received_story_to_path(published1.clone(), peer3_path)
        .await
        .unwrap();
    save_received_story_to_path(published2.clone(), peer3_path)
        .await
        .unwrap();

    // Verify each peer has all 3 stories
    let peer1_stories = read_local_stories_from_path(peer1_path).await.unwrap();
    let peer2_stories = read_local_stories_from_path(peer2_path).await.unwrap();
    let peer3_stories = read_local_stories_from_path(peer3_path).await.unwrap();

    assert_eq!(peer1_stories.len(), 3);
    assert_eq!(peer2_stories.len(), 3);
    assert_eq!(peer3_stories.len(), 3);

    // Verify all stories are public
    for stories in [&peer1_stories, &peer2_stories, &peer3_stories] {
        for story in stories {
            assert!(story.public);
        }
    }
}

#[tokio::test]
async fn test_story_deduplication() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();

    let story = Story {
        id: 1,
        name: "Duplicate Test".to_string(),
        header: "Same Header".to_string(),
        body: "Same Body".to_string(),
        public: true,
    };

    // Save the same story multiple times
    let id1 = save_received_story_to_path(story.clone(), path)
        .await
        .unwrap();
    let id2 = save_received_story_to_path(story.clone(), path)
        .await
        .unwrap();
    let id3 = save_received_story_to_path(story.clone(), path)
        .await
        .unwrap();

    // All should return the same ID
    assert_eq!(id1, id2);
    assert_eq!(id2, id3);

    // Should only have one story
    let stories = read_local_stories_from_path(path).await.unwrap();
    assert_eq!(stories.len(), 1);
}

#[tokio::test]
async fn test_list_request_response_cycle() {
    // Test the request/response message cycle
    let story1 = Story::new(
        1,
        "Public Story".to_string(),
        "Header".to_string(),
        "Body".to_string(),
        true,
    );
    let story2 = Story::new(
        2,
        "Private Story".to_string(),
        "Header".to_string(),
        "Body".to_string(),
        false,
    );

    let stories = vec![story1, story2];

    // Create a list request
    let request = ListRequest::new_all();
    let _request_json = serde_json::to_string(&request).unwrap();

    // Simulate receiving the request and creating a response
    let public_stories: Vec<Story> = stories.into_iter().filter(|s| s.public).collect();
    let response = ListResponse::new(
        ListMode::ALL,
        "test_receiver".to_string(),
        public_stories.clone(),
    );

    // Serialize and deserialize to simulate network transmission
    let response_json = serde_json::to_string(&response).unwrap();
    let received_response: ListResponse = serde_json::from_str(&response_json).unwrap();

    // Verify only public stories are included
    assert_eq!(received_response.data.len(), 1);
    assert_eq!(received_response.data[0].name, "Public Story");
    assert!(received_response.data[0].public);
}

#[tokio::test]
async fn test_published_story_message() {
    let story = Story::new(
        1,
        "Published Test".to_string(),
        "Header".to_string(),
        "Body".to_string(),
        true,
    );
    let publisher = "peer123".to_string();

    let published = PublishedStory::new(story.clone(), publisher.clone());

    // Serialize and deserialize to simulate network transmission
    let json = serde_json::to_string(&published).unwrap();
    let received: PublishedStory = serde_json::from_str(&json).unwrap();

    assert_eq!(received.story, story);
    assert_eq!(received.publisher, publisher);
}

#[tokio::test]
async fn test_sequential_story_operations() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();

    // Create multiple stories sequentially
    let mut story_ids = Vec::new();
    for i in 0..10 {
        let story_id = create_new_story_in_path(
            &format!("Story {}", i),
            &format!("Header {}", i),
            &format!("Body {}", i),
            path,
        )
        .await
        .unwrap();
        story_ids.push(story_id);
    }

    // Verify all stories were created with unique IDs
    let stories = read_local_stories_from_path(path).await.unwrap();
    assert_eq!(stories.len(), 10);

    // Verify IDs are sequential
    for (i, story) in stories.iter().enumerate() {
        assert_eq!(story.id, i);
        assert_eq!(story.name, format!("Story {}", i));
    }
}

#[tokio::test]
async fn test_error_handling() {
    // Test various error conditions

    // 1. Reading from non-existent file
    let result = read_local_stories_from_path("/definitely/does/not/exist").await;
    assert!(result.is_err());

    // 2. Publishing non-existent story in empty file
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();

    // Create an empty stories file first
    write_local_stories_to_path(&Vec::new(), path)
        .await
        .unwrap();

    let result = publish_story_in_path(999, path).await.unwrap();
    assert!(result.is_none());

    // 3. Reading corrupted JSON file
    tokio::fs::write(path, "not valid json").await.unwrap();
    let result = read_local_stories_from_path(path).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_story_filtering() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();

    // Create mix of public and private stories
    create_new_story_in_path("Private 1", "H1", "B1", path)
        .await
        .unwrap();
    let public_id = create_new_story_in_path("Public 1", "H2", "B2", path)
        .await
        .unwrap();
    create_new_story_in_path("Private 2", "H3", "B3", path)
        .await
        .unwrap();

    // Publish one story
    publish_story_in_path(public_id, path).await.unwrap();

    // Read all stories and filter public ones
    let all_stories = read_local_stories_from_path(path).await.unwrap();
    let public_stories: Vec<_> = all_stories.into_iter().filter(|s| s.public).collect();

    assert_eq!(public_stories.len(), 1);
    assert_eq!(public_stories[0].name, "Public 1");
}

#[tokio::test]
async fn test_peer_name_functionality() {
    // Test PeerName struct creation and serialization
    let peer_id = "12D3KooWTestPeer123456789".to_string();
    let name = "Alice's Node".to_string();

    let peer_name = PeerName::new(peer_id.clone(), name.clone());

    // Verify creation
    assert_eq!(peer_name.peer_id, peer_id);
    assert_eq!(peer_name.name, name);

    // Test serialization/deserialization (simulating network transmission)
    let json = serde_json::to_string(&peer_name).unwrap();
    let deserialized: PeerName = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.peer_id, peer_id);
    assert_eq!(deserialized.name, name);

    // Test that different peer names are not equal
    let different_peer_name = PeerName::new("DifferentPeer".to_string(), "Bob".to_string());
    assert_ne!(peer_name, different_peer_name);
}
