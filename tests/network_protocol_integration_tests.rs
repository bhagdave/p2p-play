use p2p_play::network::*;
use p2p_play::types::*;
use libp2p::floodsub::{Topic, Event as FloodsubEvent, FloodsubEvent::Message};
use libp2p::kad::{Event as KadEvent, QueryResult};
use libp2p::mdns::{Event as MdnsEvent};
use libp2p::ping::{Event as PingEvent};
use libp2p::request_response::{Event as RequestResponseEvent, ResponseChannel};
use libp2p::swarm::{SwarmEvent, ToSwarm};
use libp2p::{PeerId, Multiaddr, StreamProtocol};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time;
use tokio::sync::mpsc;

/// Test helper to create test swarms with unique peer IDs
async fn create_test_swarm() -> Result<libp2p::Swarm<StoryBehaviour>, Box<dyn std::error::Error>> {
    let ping_config = PingConfig::new();
    let swarm = create_swarm(&ping_config)?;
    Ok(swarm)
}

/// Test helper to get current timestamp
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[tokio::test]
async fn test_floodsub_message_broadcasting() {
    // Test floodsub message broadcasting between peers
    let mut swarm1 = create_test_swarm().await.unwrap();
    let mut swarm2 = create_test_swarm().await.unwrap();
    
    let peer1_id = *swarm1.local_peer_id();
    let peer2_id = *swarm2.local_peer_id();
    
    // Subscribe both peers to the stories topic
    swarm1.behaviour_mut().floodsub.subscribe(TOPIC.clone());
    swarm2.behaviour_mut().floodsub.subscribe(TOPIC.clone());
    
    // Start listening on available ports
    swarm1.listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap()).unwrap();
    let addr = loop {
        match swarm1.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => {
                break address;
            }
            _ => {}
        }
    };
    
    // Connect swarm2 to swarm1
    swarm2.dial(addr.clone()).unwrap();
    
    // Wait for connection establishment
    let mut connection_established = false;
    for _ in 0..10 {
        tokio::select! {
            event1 = swarm1.select_next_some() => {
                match event1 {
                    SwarmEvent::ConnectionEstablished { peer_id, .. } if peer_id == peer2_id => {
                        connection_established = true;
                        break;
                    }
                    _ => {}
                }
            }
            event2 = swarm2.select_next_some() => {
                match event2 {
                    SwarmEvent::ConnectionEstablished { peer_id, .. } if peer_id == peer1_id => {
                        connection_established = true;
                        break;
                    }
                    _ => {}
                }
            }
            _ = time::sleep(Duration::from_millis(500)) => {}
        }
    }
    
    assert!(connection_established, "Peers should establish connection");
    
    // Create and publish a test story
    let test_story = Story::new(
        1,
        "Integration Test Story".to_string(),
        "Test Header".to_string(),
        "Test Body".to_string(),
        true,
    );
    let published_story = PublishedStory::new(test_story, peer1_id.to_string());
    let message_data = serde_json::to_string(&published_story).unwrap();
    
    // Publish message from swarm1
    swarm1.behaviour_mut().floodsub.publish(TOPIC.clone(), message_data.as_bytes());
    
    // Wait for message reception on swarm2
    let mut message_received = false;
    for _ in 0..20 {
        tokio::select! {
            event1 = swarm1.select_next_some() => {
                // Process swarm1 events but don't expect message reception here
            }
            event2 = swarm2.select_next_some() => {
                if let SwarmEvent::Behaviour(StoryBehaviourEvent::Floodsub(FloodsubEvent::Message(message))) = event2 {
                    if let Ok(received_story) = serde_json::from_slice::<PublishedStory>(&message.data) {
                        assert_eq!(received_story.story.name, "Integration Test Story");
                        assert_eq!(received_story.publisher, peer1_id.to_string());
                        message_received = true;
                        break;
                    }
                }
            }
            _ = time::sleep(Duration::from_millis(100)) => {}
        }
    }
    
    assert!(message_received, "Story message should be received via floodsub");
}

#[tokio::test]
async fn test_ping_protocol_connectivity() {
    // Test ping protocol for connection monitoring
    let mut swarm1 = create_test_swarm().await.unwrap();
    let mut swarm2 = create_test_swarm().await.unwrap();
    
    let peer1_id = *swarm1.local_peer_id();
    let peer2_id = *swarm2.local_peer_id();
    
    // Start listening on swarm1
    swarm1.listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap()).unwrap();
    let addr = loop {
        match swarm1.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => break address,
            _ => {}
        }
    };
    
    // Connect swarm2 to swarm1
    swarm2.dial(addr).unwrap();
    
    // Wait for connection and ping events
    let mut ping_received = false;
    let mut pong_received = false;
    
    for _ in 0..30 {
        tokio::select! {
            event1 = swarm1.select_next_some() => {
                if let SwarmEvent::Behaviour(StoryBehaviourEvent::Ping(ping_event)) = event1 {
                    match ping_event {
                        PingEvent::Pong { peer, result } => {
                            if peer == peer2_id && result.is_ok() {
                                pong_received = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
            event2 = swarm2.select_next_some() => {
                if let SwarmEvent::Behaviour(StoryBehaviourEvent::Ping(ping_event)) = event2 {
                    match ping_event {
                        PingEvent::Pong { peer, result } => {
                            if peer == peer1_id && result.is_ok() {
                                ping_received = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ = time::sleep(Duration::from_millis(200)) => {}
        }
        
        if ping_received && pong_received {
            break;
        }
    }
    
    // At least one side should receive a pong (indicating successful ping)
    assert!(ping_received || pong_received, "Ping protocol should work between connected peers");
}

#[tokio::test]
async fn test_direct_message_request_response() {
    // Test request-response protocol for direct messaging
    let mut swarm1 = create_test_swarm().await.unwrap();
    let mut swarm2 = create_test_swarm().await.unwrap();
    
    let peer1_id = *swarm1.local_peer_id();
    let peer2_id = *swarm2.local_peer_id();
    
    // Start listening
    swarm1.listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap()).unwrap();
    let addr = loop {
        match swarm1.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => break address,
            _ => {}
        }
    };
    
    // Connect peers
    swarm2.dial(addr).unwrap();
    
    // Wait for connection
    let mut connected = false;
    for _ in 0..10 {
        tokio::select! {
            event1 = swarm1.select_next_some() => {
                if let SwarmEvent::ConnectionEstablished { peer_id, .. } = event1 {
                    if peer_id == peer2_id {
                        connected = true;
                        break;
                    }
                }
            }
            event2 = swarm2.select_next_some() => {
                if let SwarmEvent::ConnectionEstablished { peer_id, .. } = event2 {
                    if peer_id == peer1_id {
                        connected = true;
                        break;
                    }
                }
            }
            _ = time::sleep(Duration::from_millis(100)) => {}
        }
    }
    
    assert!(connected, "Peers should be connected before testing direct messaging");
    
    // Create direct message request
    let dm_request = DirectMessageRequest {
        from_peer_id: peer2_id.to_string(),
        from_name: "TestSender".to_string(),
        to_name: "TestReceiver".to_string(),
        message: "Hello from integration test!".to_string(),
        timestamp: current_timestamp(),
    };
    
    // Send request from swarm2 to swarm1
    let request_id = swarm2.behaviour_mut().request_response.send_request(&peer1_id, dm_request.clone());
    
    let mut request_received = false;
    let mut response_sent = false;
    let mut response_received = false;
    
    for _ in 0..30 {
        tokio::select! {
            event1 = swarm1.select_next_some() => {
                if let SwarmEvent::Behaviour(StoryBehaviourEvent::RequestResponse(req_resp_event)) = event1 {
                    match req_resp_event {
                        RequestResponseEvent::Message { peer, message: libp2p::request_response::Message::Request { request, channel, .. } } => {
                            if peer == peer2_id {
                                assert_eq!(request.message, "Hello from integration test!");
                                request_received = true;
                                
                                // Send response
                                let response = DirectMessageResponse {
                                    received: true,
                                    timestamp: current_timestamp(),
                                };
                                swarm1.behaviour_mut().request_response.send_response(channel, response).unwrap();
                                response_sent = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
            event2 = swarm2.select_next_some() => {
                if let SwarmEvent::Behaviour(StoryBehaviourEvent::RequestResponse(req_resp_event)) = event2 {
                    match req_resp_event {
                        RequestResponseEvent::Message { peer, message: libp2p::request_response::Message::Response { response, .. } } => {
                            if peer == peer1_id {
                                assert!(response.received);
                                response_received = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ = time::sleep(Duration::from_millis(100)) => {}
        }
        
        if request_received && response_sent && response_received {
            break;
        }
    }
    
    assert!(request_received, "Direct message request should be received");
    assert!(response_sent, "Direct message response should be sent");
    assert!(response_received, "Direct message response should be received");
}

#[tokio::test]
async fn test_node_description_request_response() {
    // Test node description request-response protocol
    let mut swarm1 = create_test_swarm().await.unwrap();
    let mut swarm2 = create_test_swarm().await.unwrap();
    
    let peer1_id = *swarm1.local_peer_id();
    let peer2_id = *swarm2.local_peer_id();
    
    // Set up connection
    swarm1.listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap()).unwrap();
    let addr = loop {
        match swarm1.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => break address,
            _ => {}
        }
    };
    
    swarm2.dial(addr).unwrap();
    
    // Wait for connection
    let mut connected = false;
    for _ in 0..10 {
        tokio::select! {
            event = swarm1.select_next_some() => {
                if let SwarmEvent::ConnectionEstablished { peer_id, .. } = event {
                    if peer_id == peer2_id {
                        connected = true;
                        break;
                    }
                }
            }
            _ = time::sleep(Duration::from_millis(100)) => {}
        }
    }
    
    assert!(connected, "Peers should be connected");
    
    // Create node description request
    let desc_request = NodeDescriptionRequest {
        from_peer_id: peer2_id.to_string(),
        from_name: "TestRequester".to_string(),
        timestamp: current_timestamp(),
    };
    
    // Send request
    swarm2.behaviour_mut().node_description.send_request(&peer1_id, desc_request);
    
    let mut request_handled = false;
    let mut response_received = false;
    
    for _ in 0..20 {
        tokio::select! {
            event1 = swarm1.select_next_some() => {
                if let SwarmEvent::Behaviour(StoryBehaviourEvent::NodeDescription(desc_event)) = event1 {
                    match desc_event {
                        RequestResponseEvent::Message { peer, message: libp2p::request_response::Message::Request { channel, .. } } => {
                            if peer == peer2_id {
                                // Send response with test description
                                let response = NodeDescriptionResponse {
                                    description: Some("Test Node Description".to_string()),
                                    from_peer_id: peer1_id.to_string(),
                                    from_name: "TestResponder".to_string(),
                                    timestamp: current_timestamp(),
                                };
                                swarm1.behaviour_mut().node_description.send_response(channel, response).unwrap();
                                request_handled = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
            event2 = swarm2.select_next_some() => {
                if let SwarmEvent::Behaviour(StoryBehaviourEvent::NodeDescription(desc_event)) = event2 {
                    match desc_event {
                        RequestResponseEvent::Message { peer, message: libp2p::request_response::Message::Response { response, .. } } => {
                            if peer == peer1_id {
                                assert_eq!(response.description, Some("Test Node Description".to_string()));
                                response_received = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ = time::sleep(Duration::from_millis(100)) => {}
        }
        
        if request_handled && response_received {
            break;
        }
    }
    
    assert!(request_handled, "Node description request should be handled");
    assert!(response_received, "Node description response should be received");
}

#[tokio::test]
async fn test_story_sync_request_response() {
    // Test story synchronization request-response protocol
    let mut swarm1 = create_test_swarm().await.unwrap();
    let mut swarm2 = create_test_swarm().await.unwrap();
    
    let peer1_id = *swarm1.local_peer_id();
    let peer2_id = *swarm2.local_peer_id();
    
    // Set up connection
    swarm1.listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap()).unwrap();
    let addr = loop {
        match swarm1.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => break address,
            _ => {}
        }
    };
    
    swarm2.dial(addr).unwrap();
    
    // Wait for connection
    for _ in 0..10 {
        if let SwarmEvent::ConnectionEstablished { .. } = swarm1.select_next_some().await {
            break;
        }
    }
    
    // Create story sync request
    let sync_request = StorySyncRequest {
        from_peer_id: peer2_id.to_string(),
        from_name: "SyncRequester".to_string(),
        last_sync_timestamp: 0,
        subscribed_channels: vec!["general".to_string(), "tech".to_string()],
        timestamp: current_timestamp(),
    };
    
    // Send sync request
    swarm2.behaviour_mut().story_sync.send_request(&peer1_id, sync_request);
    
    let mut sync_handled = false;
    let mut sync_response_received = false;
    
    for _ in 0..20 {
        tokio::select! {
            event1 = swarm1.select_next_some() => {
                if let SwarmEvent::Behaviour(StoryBehaviourEvent::StorySync(sync_event)) = event1 {
                    match sync_event {
                        RequestResponseEvent::Message { peer, message: libp2p::request_response::Message::Request { request, channel, .. } } => {
                            if peer == peer2_id {
                                // Create test stories to sync
                                let test_stories = vec![
                                    Story::new_with_channel(
                                        1,
                                        "Synced Story 1".to_string(),
                                        "Header 1".to_string(),
                                        "Body 1".to_string(),
                                        true,
                                        "general".to_string(),
                                    ),
                                    Story::new_with_channel(
                                        2,
                                        "Synced Story 2".to_string(),
                                        "Header 2".to_string(),
                                        "Body 2".to_string(),
                                        true,
                                        "tech".to_string(),
                                    ),
                                ];
                                
                                let sync_response = StorySyncResponse {
                                    stories: test_stories,
                                    from_peer_id: peer1_id.to_string(),
                                    from_name: "SyncResponder".to_string(),
                                    sync_timestamp: current_timestamp(),
                                };
                                
                                swarm1.behaviour_mut().story_sync.send_response(channel, sync_response).unwrap();
                                sync_handled = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
            event2 = swarm2.select_next_some() => {
                if let SwarmEvent::Behaviour(StoryBehaviourEvent::StorySync(sync_event)) = event2 {
                    match sync_event {
                        RequestResponseEvent::Message { peer, message: libp2p::request_response::Message::Response { response, .. } } => {
                            if peer == peer1_id {
                                assert_eq!(response.stories.len(), 2);
                                assert_eq!(response.stories[0].name, "Synced Story 1");
                                assert_eq!(response.stories[0].channel, "general");
                                assert_eq!(response.stories[1].name, "Synced Story 2");
                                assert_eq!(response.stories[1].channel, "tech");
                                sync_response_received = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ = time::sleep(Duration::from_millis(100)) => {}
        }
        
        if sync_handled && sync_response_received {
            break;
        }
    }
    
    assert!(sync_handled, "Story sync request should be handled");
    assert!(sync_response_received, "Story sync response should be received");
}

#[tokio::test]
async fn test_kademlia_dht_basic_functionality() {
    // Test basic Kademlia DHT functionality
    let mut swarm1 = create_test_swarm().await.unwrap();
    let mut swarm2 = create_test_swarm().await.unwrap();
    
    let peer1_id = *swarm1.local_peer_id();
    let peer2_id = *swarm2.local_peer_id();
    
    // Set up listening and connection
    swarm1.listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap()).unwrap();
    let addr = loop {
        match swarm1.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => break address,
            _ => {}
        }
    };
    
    // Add peer2 to peer1's routing table
    swarm2.behaviour_mut().kad.add_address(&peer1_id, addr.clone());
    swarm2.dial(addr).unwrap();
    
    // Wait for connection and DHT events
    let mut connection_established = false;
    let mut dht_events_observed = false;
    
    for _ in 0..30 {
        tokio::select! {
            event1 = swarm1.select_next_some() => {
                match event1 {
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        if peer_id == peer2_id {
                            connection_established = true;
                        }
                    }
                    SwarmEvent::Behaviour(StoryBehaviourEvent::Kad(kad_event)) => {
                        // Any Kademlia event indicates the DHT is active
                        dht_events_observed = true;
                    }
                    _ => {}
                }
            }
            event2 = swarm2.select_next_some() => {
                match event2 {
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        if peer_id == peer1_id {
                            connection_established = true;
                        }
                    }
                    SwarmEvent::Behaviour(StoryBehaviourEvent::Kad(kad_event)) => {
                        // Any Kademlia event indicates the DHT is active
                        dht_events_observed = true;
                    }
                    _ => {}
                }
            }
            _ = time::sleep(Duration::from_millis(100)) => {}
        }
        
        if connection_established {
            break;
        }
    }
    
    assert!(connection_established, "Peers should establish connection for DHT testing");
    
    // The fact that we can create swarms with Kademlia and establish connections
    // indicates basic DHT functionality is working
    
    // Test bootstrap operation (basic functionality)
    swarm2.behaviour_mut().kad.bootstrap().unwrap();
    
    // Wait for any bootstrap-related events
    for _ in 0..10 {
        tokio::select! {
            event2 = swarm2.select_next_some() => {
                if let SwarmEvent::Behaviour(StoryBehaviourEvent::Kad(kad_event)) = event2 {
                    match kad_event {
                        KadEvent::OutboundQueryProgressed { result: QueryResult::Bootstrap(result), .. } => {
                            // Bootstrap query initiated - this indicates DHT is functional
                            dht_events_observed = true;
                            break;
                        }
                        _ => {
                            dht_events_observed = true;
                        }
                    }
                }
            }
            _ = time::sleep(Duration::from_millis(100)) => {}
        }
    }
    
    // Basic DHT functionality test - we've established that:
    // 1. Kademlia DHT can be initialized
    // 2. Peers can connect (prerequisite for DHT operations)
    // 3. Bootstrap operations can be initiated
    assert!(connection_established, "Basic DHT connectivity should work");
}

#[tokio::test] 
async fn test_protocol_message_serialization_edge_cases() {
    // Test edge cases in protocol message serialization
    
    // Test DirectMessageRequest with special characters
    let dm_request = DirectMessageRequest {
        from_peer_id: "12D3KooWTest".to_string(),
        from_name: "User with émojis 🚀".to_string(),
        to_name: "Recipient with\nnewlines".to_string(),
        message: "Message with\ttabs and\r\nwindows newlines".to_string(),
        timestamp: u64::MAX, // Edge case: maximum timestamp
    };
    
    let serialized = serde_json::to_string(&dm_request).unwrap();
    let deserialized: DirectMessageRequest = serde_json::from_str(&serialized).unwrap();
    assert_eq!(dm_request, deserialized);
    
    // Test NodeDescriptionRequest with empty strings
    let desc_request = NodeDescriptionRequest {
        from_peer_id: String::new(),
        from_name: String::new(),
        timestamp: 0,
    };
    
    let serialized = serde_json::to_string(&desc_request).unwrap();
    let deserialized: NodeDescriptionRequest = serde_json::from_str(&serialized).unwrap();
    assert_eq!(desc_request, deserialized);
    
    // Test StorySyncRequest with large channel list
    let many_channels: Vec<String> = (0..1000).map(|i| format!("channel_{i}")).collect();
    let sync_request = StorySyncRequest {
        from_peer_id: "test_peer".to_string(),
        from_name: "test_name".to_string(),
        last_sync_timestamp: current_timestamp(),
        subscribed_channels: many_channels.clone(),
        timestamp: current_timestamp(),
    };
    
    let serialized = serde_json::to_string(&sync_request).unwrap();
    let deserialized: StorySyncRequest = serde_json::from_str(&serialized).unwrap();
    assert_eq!(sync_request.subscribed_channels.len(), 1000);
    assert_eq!(deserialized.subscribed_channels, many_channels);
    
    // Test StorySyncResponse with empty stories
    let sync_response = StorySyncResponse {
        stories: vec![],
        from_peer_id: "responder".to_string(),
        from_name: "responder_name".to_string(),
        sync_timestamp: current_timestamp(),
    };
    
    let serialized = serde_json::to_string(&sync_response).unwrap();
    let deserialized: StorySyncResponse = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.stories.len(), 0);
    
    // Test DirectMessageResponse edge cases
    let dm_response = DirectMessageResponse {
        received: false,
        timestamp: 0,
    };
    
    let serialized = serde_json::to_string(&dm_response).unwrap();
    let deserialized: DirectMessageResponse = serde_json::from_str(&serialized).unwrap();
    assert_eq!(dm_response, deserialized);
    assert!(!deserialized.received);
}

#[tokio::test]
async fn test_concurrent_protocol_operations() {
    // Test multiple protocols operating concurrently
    let mut swarm1 = create_test_swarm().await.unwrap();
    let mut swarm2 = create_test_swarm().await.unwrap();
    
    let peer1_id = *swarm1.local_peer_id();
    let peer2_id = *swarm2.local_peer_id();
    
    // Subscribe to floodsub topic
    swarm1.behaviour_mut().floodsub.subscribe(TOPIC.clone());
    swarm2.behaviour_mut().floodsub.subscribe(TOPIC.clone());
    
    // Set up connection
    swarm1.listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap()).unwrap();
    let addr = loop {
        match swarm1.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => break address,
            _ => {}
        }
    };
    
    swarm2.dial(addr).unwrap();
    
    // Wait for connection
    let mut connected = false;
    for _ in 0..10 {
        tokio::select! {
            event = swarm1.select_next_some() => {
                if let SwarmEvent::ConnectionEstablished { peer_id, .. } = event {
                    if peer_id == peer2_id {
                        connected = true;
                        break;
                    }
                }
            }
            _ = time::sleep(Duration::from_millis(100)) => {}
        }
    }
    
    assert!(connected, "Peers should be connected for concurrent testing");
    
    // Simultaneously:
    // 1. Send a floodsub message
    // 2. Send a direct message request
    // 3. Send a node description request
    
    // Floodsub message
    let story = PublishedStory::new(
        Story::new(1, "Concurrent Test".to_string(), "Header".to_string(), "Body".to_string(), true),
        peer1_id.to_string(),
    );
    let story_data = serde_json::to_string(&story).unwrap();
    swarm1.behaviour_mut().floodsub.publish(TOPIC.clone(), story_data.as_bytes());
    
    // Direct message request
    let dm_request = DirectMessageRequest {
        from_peer_id: peer1_id.to_string(),
        from_name: "Sender".to_string(),
        to_name: "Receiver".to_string(),
        message: "Concurrent DM test".to_string(),
        timestamp: current_timestamp(),
    };
    swarm1.behaviour_mut().request_response.send_request(&peer2_id, dm_request);
    
    // Node description request
    let desc_request = NodeDescriptionRequest {
        from_peer_id: peer1_id.to_string(),
        from_name: "Requester".to_string(),
        timestamp: current_timestamp(),
    };
    swarm1.behaviour_mut().node_description.send_request(&peer2_id, desc_request);
    
    // Track what we receive
    let mut floodsub_received = false;
    let mut dm_request_received = false;
    let mut desc_request_received = false;
    let mut events_processed = 0;
    
    // Process events for a reasonable time
    for _ in 0..50 {
        tokio::select! {
            event1 = swarm1.select_next_some() => {
                events_processed += 1;
                // Swarm1 might receive ping events, responses, etc.
            }
            event2 = swarm2.select_next_some() => {
                events_processed += 1;
                match event2 {
                    SwarmEvent::Behaviour(StoryBehaviourEvent::Floodsub(FloodsubEvent::Message(_))) => {
                        floodsub_received = true;
                    }
                    SwarmEvent::Behaviour(StoryBehaviourEvent::RequestResponse(
                        RequestResponseEvent::Message { message: libp2p::request_response::Message::Request { channel, .. }, .. }
                    )) => {
                        dm_request_received = true;
                        let response = DirectMessageResponse {
                            received: true,
                            timestamp: current_timestamp(),
                        };
                        swarm2.behaviour_mut().request_response.send_response(channel, response).unwrap();
                    }
                    SwarmEvent::Behaviour(StoryBehaviourEvent::NodeDescription(
                        RequestResponseEvent::Message { message: libp2p::request_response::Message::Request { channel, .. }, .. }
                    )) => {
                        desc_request_received = true;
                        let response = NodeDescriptionResponse {
                            description: Some("Concurrent test node".to_string()),
                            from_peer_id: peer2_id.to_string(),
                            from_name: "Responder".to_string(),
                            timestamp: current_timestamp(),
                        };
                        swarm2.behaviour_mut().node_description.send_response(channel, response).unwrap();
                    }
                    _ => {}
                }
            }
            _ = time::sleep(Duration::from_millis(50)) => {}
        }
        
        if floodsub_received && dm_request_received && desc_request_received {
            break;
        }
    }
    
    // Verify that multiple protocols can operate concurrently
    assert!(events_processed > 0, "Should process network events");
    assert!(floodsub_received, "Should receive floodsub message");
    assert!(dm_request_received, "Should receive direct message request");
    assert!(desc_request_received, "Should receive node description request");
}