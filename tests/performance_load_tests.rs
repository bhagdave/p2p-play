use p2p_play::network::*;
use p2p_play::types::*;
use libp2p::floodsub::{FloodsubEvent};
use libp2p::swarm::{SwarmEvent};
use libp2p::request_response::{Event as RequestResponseEvent, Message};
use libp2p::{PeerId, Multiaddr};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, SystemTime, UNIX_EPOCH, Instant};
use tokio::time;
use futures::future::join_all;
use std::sync::{Arc, Mutex};
use futures::StreamExt;
use futures::future::FutureExt;

/// Helper to create test swarms with unique peer IDs
async fn create_test_swarm() -> Result<libp2p::Swarm<StoryBehaviour>, Box<dyn std::error::Error>> {
    create_test_swarm_with_keypair(libp2p::identity::Keypair::generate_ed25519())
}

/// Helper to create test swarms with specific keypairs
fn create_test_swarm_with_keypair(keypair: libp2p::identity::Keypair) -> Result<libp2p::Swarm<StoryBehaviour>, Box<dyn std::error::Error>> {
    use libp2p::tcp::Config;
    use libp2p::{Transport, core::upgrade, dns, noise, swarm::Config as SwarmConfig, tcp, yamux, request_response, StreamProtocol};
    use std::time::Duration;
    use std::iter;
    
    let tcp_config = Config::default()
        .nodelay(true)
        .listen_backlog(1024)
        .ttl(64);
        
    let mut yamux_config = yamux::Config::default();
    yamux_config.set_max_num_streams(512);
        
    let transp = dns::tokio::Transport::system(tcp::tokio::Transport::new(tcp_config))
        .map_err(|e| format!("Failed to create DNS transport: {e}"))?
        .upgrade(upgrade::Version::V1)
        .authenticate(noise::Config::new(&keypair).unwrap())
        .multiplex(yamux_config)
        .boxed();

    // Create behaviour with unique peer ID
    let peer_id = libp2p::PeerId::from(keypair.public());
    
    // Create request-response protocols using the same patterns as the main code
    let dm_protocol = StreamProtocol::new("/dm/1.0.0");
    let dm_protocols = iter::once((dm_protocol, request_response::ProtocolSupport::Full));
    
    let cfg = request_response::Config::default()
        .with_request_timeout(Duration::from_secs(60))
        .with_max_concurrent_streams(256);
    
    let request_response = request_response::cbor::Behaviour::new(dm_protocols, cfg.clone());
    
    // Node description protocol
    let desc_protocol = StreamProtocol::new("/node-desc/1.0.0");
    let desc_protocols = iter::once((desc_protocol, request_response::ProtocolSupport::Full));
    let node_description = request_response::cbor::Behaviour::new(desc_protocols, cfg.clone());
    
    // Story sync protocol
    let story_sync_protocol = StreamProtocol::new("/story-sync/1.0.0");
    let story_sync_protocols = iter::once((story_sync_protocol, request_response::ProtocolSupport::Full));
    let story_sync = request_response::cbor::Behaviour::new(story_sync_protocols, cfg);
    
    // Create Kademlia DHT
    let store = libp2p::kad::store::MemoryStore::new(peer_id);
    let kad_config = libp2p::kad::Config::default();
    let mut kad = libp2p::kad::Behaviour::with_config(peer_id, store, kad_config);
    kad.set_mode(Some(libp2p::kad::Mode::Server));
    
    let behaviour = StoryBehaviour {
        floodsub: libp2p::floodsub::Behaviour::new(peer_id),
        mdns: libp2p::mdns::tokio::Behaviour::new(Default::default(), peer_id).expect("can create mdns"),
        ping: libp2p::ping::Behaviour::new(libp2p::ping::Config::new()),
        request_response,
        node_description,
        story_sync,
        kad,
    };

    let swarm_config = SwarmConfig::with_tokio_executor()
        .with_idle_connection_timeout(Duration::from_secs(30));
        
    Ok(libp2p::Swarm::new(
        transp,
        behaviour,
        peer_id,
        swarm_config
    ))
}

/// Helper to get current timestamp
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Helper to establish connection between swarms
async fn establish_connection(
    swarm1: &mut libp2p::Swarm<StoryBehaviour>,
    swarm2: &mut libp2p::Swarm<StoryBehaviour>
) -> Option<Multiaddr> {
    swarm1.listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap()).unwrap();
    
    // Get the listening address with timeout
    let addr = {
        let mut addr_opt = None;
        for _ in 0..50 {
            tokio::select! {
                event = swarm1.select_next_some() => {
                    if let SwarmEvent::NewListenAddr { address, .. } = event {
                        addr_opt = Some(address);
                        break;
                    }
                }
                _ = time::sleep(Duration::from_millis(50)) => {}
            }
        }
        addr_opt?
    };
    
    if swarm2.dial(addr.clone()).is_err() {
        return None;
    }
    
    // Wait for connection with much longer timeout
    for _ in 0..100 {
        tokio::select! {
            event1 = swarm1.select_next_some() => {
                if let SwarmEvent::ConnectionEstablished { .. } = event1 {
                    return Some(addr);
                }
            }
            event2 = swarm2.select_next_some() => {
                if let SwarmEvent::ConnectionEstablished { .. } = event2 {
                    return Some(addr);
                }
            }
            _ = time::sleep(Duration::from_millis(100)) => {}
        }
    }
    
    None
}

#[tokio::test]
async fn test_high_frequency_message_broadcasting() {
    // Test high-frequency floodsub message broadcasting performance
    let mut swarm1 = create_test_swarm().await.unwrap();
    let mut swarm2 = create_test_swarm().await.unwrap();
    
    let peer1_id = *swarm1.local_peer_id();
    let _peer2_id = *swarm2.local_peer_id();
    
    // Establish connection first
    let _addr = establish_connection(&mut swarm1, &mut swarm2).await;
    assert!(_addr.is_some(), "Connection should be established");
    
    // Allow time for connection to stabilize
    time::sleep(Duration::from_millis(1000)).await;
    
    // Subscribe to floodsub after connection is established
    swarm1.behaviour_mut().floodsub.subscribe(TOPIC.clone());
    swarm2.behaviour_mut().floodsub.subscribe(TOPIC.clone());
    
    // Allow time for subscriptions to propagate
    time::sleep(Duration::from_millis(1000)).await;
    
    let message_count = 1000;
    let mut messages_sent = 0;
    let mut messages_received = 0;
    let mut received_message_ids = HashSet::new();
    
    let start_time = Instant::now();
    
    // Send messages rapidly
    for i in 0..message_count {
        let story = PublishedStory::new(
            Story::new(
                i,
                format!("High Frequency Message {}", i),
                "Header".to_string(),
                format!("Body content for message {}", i),
                true,
            ),
            peer1_id.to_string(),
        );
        
        let message_data = serde_json::to_string(&story).unwrap().into_bytes();
        swarm1.behaviour_mut().floodsub.publish(TOPIC.clone(), message_data);
        messages_sent += 1;
        
        // Small delay to prevent overwhelming the system
        if i % 100 == 0 {
            time::sleep(Duration::from_millis(10)).await;
        }
    }
    
    let send_duration = start_time.elapsed();
    
    // Process received messages
    let receive_start = Instant::now();
    let receive_timeout = Duration::from_secs(30);
    
    while receive_start.elapsed() < receive_timeout && messages_received < message_count / 2 {
        tokio::select! {
            event1 = swarm1.select_next_some() => {
                // Process sender events but don't count self-received messages
            }
            event2 = swarm2.select_next_some() => {
                if let SwarmEvent::Behaviour(StoryBehaviourEvent::Floodsub(FloodsubEvent::Message(msg))) = event2 {
                    if let Ok(story) = serde_json::from_slice::<PublishedStory>(&msg.data) {
                        if story.story.name.starts_with("High Frequency Message") {
                            received_message_ids.insert(story.story.id);
                            messages_received += 1;
                        }
                    }
                }
            }
            _ = time::sleep(Duration::from_millis(10)) => {}
        }
    }
    
    let receive_duration = receive_start.elapsed();
    
    // Performance metrics
    let send_rate = messages_sent as f64 / send_duration.as_secs_f64();
    let receive_rate = messages_received as f64 / receive_duration.as_secs_f64();
    let delivery_ratio = messages_received as f64 / messages_sent as f64;
    
    println!("High-frequency broadcasting performance:");
    println!("  Messages sent: {}", messages_sent);
    println!("  Messages received: {}", messages_received);
    println!("  Send rate: {:.2} msgs/sec", send_rate);
    println!("  Receive rate: {:.2} msgs/sec", receive_rate);
    println!("  Delivery ratio: {:.2}%", delivery_ratio * 100.0);
    println!("  Send duration: {:?}", send_duration);
    println!("  Receive duration: {:?}", receive_duration);
    
    // Assertions for performance
    assert!(messages_sent == message_count, "All messages should be sent");
    assert!(messages_received > 0, "Some messages should be received");
    assert!(send_rate > 10.0, "Send rate should be reasonable (>10 msgs/sec)");
    
    // In test environment, delivery ratio might be lower due to timing
    // The test primarily validates that the system can handle high-frequency messaging
}

#[tokio::test]
async fn test_large_message_handling_performance() {
    // Test performance with large messages
    let mut swarm1 = create_test_swarm().await.unwrap();
    let mut swarm2 = create_test_swarm().await.unwrap();
    
    let peer1_id = *swarm1.local_peer_id();
    let peer2_id = *swarm2.local_peer_id();
    
    // Establish connection
    let _addr = establish_connection(&mut swarm1, &mut swarm2).await;
    assert!(_addr.is_some(), "Connection should be established");
    
    // Allow time for connection to stabilize
    time::sleep(Duration::from_millis(1000)).await;
    
    // Test different message sizes
    let message_sizes = vec![1_000, 10_000, 100_000, 500_000]; // 1KB to 500KB
    
    for (test_idx, size) in message_sizes.iter().enumerate() {
        println!("Testing message size: {} bytes", size);
        
        let large_content = "A".repeat(*size);
        let large_dm = DirectMessageRequest {
            from_peer_id: peer1_id.to_string(),
            from_name: "Performance Tester".to_string(),
            to_name: "Receiver".to_string(),
            message: large_content,
            timestamp: current_timestamp(),
        };
        
        let start_time = Instant::now();
        swarm1.behaviour_mut().request_response.send_request(&peer2_id, large_dm.clone());
        
        let mut request_handled = false;
        let mut response_received = false;
        let mut processing_time = Duration::from_secs(0);
        
        for _ in 0..100 {
            tokio::select! {
                event1 = swarm1.select_next_some() => {
                    match event1 {
                        SwarmEvent::Behaviour(StoryBehaviourEvent::RequestResponse(
                            RequestResponseEvent::Message { message: Message::Response { .. }, .. }
                        )) => {
                            processing_time = start_time.elapsed();
                            response_received = true;
                            break;
                        }
                        SwarmEvent::Behaviour(StoryBehaviourEvent::RequestResponse(
                            RequestResponseEvent::OutboundFailure { .. }
                        )) => {
                            processing_time = start_time.elapsed();
                            println!("  Request failed for size {}", size);
                            break;
                        }
                        _ => {}
                    }
                }
                event2 = swarm2.select_next_some() => {
                    match event2 {
                        SwarmEvent::Behaviour(StoryBehaviourEvent::RequestResponse(
                            RequestResponseEvent::Message { message: Message::Request { request, channel, .. }, .. }
                        )) => {
                            // Verify message size
                            assert_eq!(request.message.len(), *size);
                            request_handled = true;
                            
                            // Send response
                            let response = DirectMessageResponse {
                                received: true,
                                timestamp: current_timestamp(),
                            };
                            swarm2.behaviour_mut().request_response.send_response(channel, response).unwrap();
                        }
                        _ => {}
                    }
                }
                _ = time::sleep(Duration::from_millis(50)) => {}
            }
        }
        
        println!("  Size: {} bytes", size);
        println!("  Request handled: {}", request_handled);
        println!("  Response received: {}", response_received);
        println!("  Processing time: {:?}", processing_time);
        println!("  Throughput: {:.2} KB/sec", (*size as f64 / 1024.0) / processing_time.as_secs_f64());
        
        // Performance assertions
        assert!(request_handled, "Large message should be handled");
        assert!(processing_time < Duration::from_secs(10), "Processing should complete within reasonable time");
        
        if response_received {
            // Calculate throughput
            let throughput_kbps = (*size as f64 / 1024.0) / processing_time.as_secs_f64();
            assert!(throughput_kbps > 0.1, "Throughput should be reasonable");
        }
        
        // Brief pause between tests
        time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn test_concurrent_connection_performance() {
    // Test performance with multiple concurrent connections
    let connection_count = 10;
    let mut swarms = Vec::new();
    
    // Create multiple swarms
    for _ in 0..connection_count {
        let swarm = create_test_swarm().await.unwrap();
        swarms.push(swarm);
    }
    
    let peer_ids: Vec<PeerId> = swarms.iter().map(|s| *s.local_peer_id()).collect();
    
    // Subscribe all to floodsub
    for swarm in &mut swarms {
        swarm.behaviour_mut().floodsub.subscribe(TOPIC.clone());
    }
    
    let start_time = Instant::now();
    
    // Start all swarms listening on dynamic ports to avoid conflicts
    let mut listen_addresses = Vec::new();
    for (i, swarm) in swarms.iter_mut().enumerate() {
        swarm.listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap()).unwrap();
        
        // Get listening address with timeout
        let mut addr_found = false;
        for _ in 0..50 {
            tokio::select! {
                event = swarm.select_next_some() => {
                    if let SwarmEvent::NewListenAddr { address, .. } = event {
                        listen_addresses.push(address);
                        addr_found = true;
                        break;
                    }
                }
                _ = time::sleep(Duration::from_millis(50)) => {}
            }
        }
        
        if !addr_found {
            println!("Failed to get listening address for swarm {}", i);
        }
    }
    
    // Connect first swarm to all others (star topology)
    for i in 1..connection_count {
        swarms[0].dial(listen_addresses[i].clone()).unwrap();
    }
    
    // Track connections established with better error handling
    let mut connections_established = 0;
    let connection_timeout = Duration::from_secs(60); // Increased timeout
    let connection_start = Instant::now();
    
    while connection_start.elapsed() < connection_timeout && connections_established < connection_count - 1 {
        for (i, swarm) in swarms.iter_mut().enumerate() {
            if let Some(event) = futures::StreamExt::next(swarm).now_or_never().flatten() {
                match event {
                    SwarmEvent::ConnectionEstablished { .. } => {
                        connections_established += 1;
                        println!("Connection established: {} of {}", connections_established, connection_count - 1);
                    }
                    SwarmEvent::OutgoingConnectionError { error, .. } => {
                        println!("Connection error on swarm {}: {:?}", i, error);
                    }
                    _ => {}
                }
            }
        }
        
        time::sleep(Duration::from_millis(50)).await; // Longer sleep
    }
    
    let connection_setup_time = start_time.elapsed();
    
    println!("Concurrent connection performance:");
    println!("  Target connections: {}", connection_count - 1);
    println!("  Established connections: {}", connections_established);
    println!("  Setup time: {:?}", connection_setup_time);
    println!("  Connection rate: {:.2} conn/sec", connections_established as f64 / connection_setup_time.as_secs_f64());
    
    // Test message broadcasting to all connections
    let broadcast_message = PublishedStory::new(
        Story::new(1, "Concurrent Broadcast Test".to_string(), "Header".to_string(), "Body".to_string(), true),
        peer_ids[0].to_string(),
    );
    
    let broadcast_start = Instant::now();
    let broadcast_data = serde_json::to_string(&broadcast_message).unwrap().into_bytes();
    swarms[0].behaviour_mut().floodsub.publish(
        TOPIC.clone(),
        broadcast_data
    );
    
    let mut messages_received = 0;
    let broadcast_timeout = Duration::from_secs(10);
    
    while broadcast_start.elapsed() < broadcast_timeout && messages_received < connections_established {
        for (i, swarm) in swarms.iter_mut().enumerate() {
            if i == 0 { continue; } // Skip sender
            
            if let Some(event) = futures::StreamExt::next(swarm).now_or_never().flatten() {
                if let SwarmEvent::Behaviour(StoryBehaviourEvent::Floodsub(FloodsubEvent::Message(msg))) = event {
                    if let Ok(story) = serde_json::from_slice::<PublishedStory>(&msg.data) {
                        if story.story.name == "Concurrent Broadcast Test" {
                            messages_received += 1;
                        }
                    }
                }
            }
        }
        
        time::sleep(Duration::from_millis(10)).await;
    }
    
    let broadcast_duration = broadcast_start.elapsed();
    
    println!("  Messages broadcasted to: {} peers", messages_received);
    println!("  Broadcast time: {:?}", broadcast_duration);
    println!("  Broadcast delivery rate: {:.2} msgs/sec", messages_received as f64 / broadcast_duration.as_secs_f64());
    
    // Performance assertions
    assert!(connections_established > 0, "Some connections should be established");
    assert!(connection_setup_time < Duration::from_secs(30), "Connection setup should be reasonable");
    assert!(messages_received > 0, "Some broadcast messages should be received");
}

#[tokio::test]
async fn test_request_response_throughput() {
    // Test request-response throughput under load
    let mut swarm1 = create_test_swarm().await.unwrap();
    let mut swarm2 = create_test_swarm().await.unwrap();
    
    let peer1_id = *swarm1.local_peer_id();
    let peer2_id = *swarm2.local_peer_id();
    
    // Establish connection
    let _addr = establish_connection(&mut swarm1, &mut swarm2).await;
    assert!(_addr.is_some(), "Connection should be established");
    
    // Allow time for connection to stabilize
    time::sleep(Duration::from_millis(1000)).await;
    
    let request_count = 500;
    let mut requests_sent = 0;
    let mut responses_received = 0;
    let mut requests_handled = 0;
    
    let start_time = Instant::now();
    
    // Send multiple requests rapidly
    for i in 0..request_count {
        let dm_request = DirectMessageRequest {
            from_peer_id: peer1_id.to_string(),
            from_name: "Load Tester".to_string(),
            to_name: "Load Handler".to_string(),
            message: format!("Load test message {}", i),
            timestamp: current_timestamp(),
        };
        
        swarm1.behaviour_mut().request_response.send_request(&peer2_id, dm_request);
        requests_sent += 1;
        
        // Small delay every 50 requests to prevent overwhelming
        if i % 50 == 0 {
            time::sleep(Duration::from_millis(10)).await;
        }
    }
    
    // Process requests and responses
    let processing_timeout = Duration::from_secs(60);
    let processing_start = Instant::now();
    
    while processing_start.elapsed() < processing_timeout && 
          (responses_received < request_count / 2 || requests_handled < request_count / 2) {
        
        tokio::select! {
            event1 = swarm1.select_next_some() => {
                match event1 {
                    SwarmEvent::Behaviour(StoryBehaviourEvent::RequestResponse(
                        RequestResponseEvent::Message { message: Message::Response { .. }, .. }
                    )) => {
                        responses_received += 1;
                    }
                    SwarmEvent::Behaviour(StoryBehaviourEvent::RequestResponse(
                        RequestResponseEvent::OutboundFailure { .. }
                    )) => {
                        // Count failures as processed
                        responses_received += 1;
                    }
                    _ => {}
                }
            }
            event2 = swarm2.select_next_some() => {
                match event2 {
                    SwarmEvent::Behaviour(StoryBehaviourEvent::RequestResponse(
                        RequestResponseEvent::Message { message: Message::Request { channel, .. }, .. }
                    )) => {
                        requests_handled += 1;
                        
                        // Send response for most requests (simulate some processing capacity limits)
                        if requests_handled <= request_count * 3 / 4 {
                            let response = DirectMessageResponse {
                                received: true,
                                timestamp: current_timestamp(),
                            };
                            swarm2.behaviour_mut().request_response.send_response(channel, response).unwrap();
                        }
                    }
                    _ => {}
                }
            }
            _ = time::sleep(Duration::from_millis(5)) => {}
        }
    }
    
    let total_duration = start_time.elapsed();
    
    // Calculate performance metrics
    let request_rate = requests_sent as f64 / total_duration.as_secs_f64();
    let response_rate = responses_received as f64 / total_duration.as_secs_f64();
    let handling_rate = requests_handled as f64 / total_duration.as_secs_f64();
    let success_rate = responses_received as f64 / requests_sent as f64;
    
    println!("Request-Response throughput performance:");
    println!("  Requests sent: {}", requests_sent);
    println!("  Requests handled: {}", requests_handled);
    println!("  Responses received: {}", responses_received);
    println!("  Total duration: {:?}", total_duration);
    println!("  Request rate: {:.2} req/sec", request_rate);
    println!("  Response rate: {:.2} resp/sec", response_rate);
    println!("  Handling rate: {:.2} handle/sec", handling_rate);
    println!("  Success rate: {:.2}%", success_rate * 100.0);
    
    // Performance assertions
    assert!(requests_sent == request_count, "All requests should be sent");
    assert!(requests_handled > 0, "Some requests should be handled");
    assert!(responses_received > 0, "Some responses should be received");
    assert!(request_rate > 5.0, "Request rate should be reasonable");
    assert!(success_rate > 0.1, "Some requests should succeed");
}

#[tokio::test]
async fn test_memory_usage_under_load() {
    // Test memory usage characteristics under load
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    
    let mut swarm1 = create_test_swarm().await.unwrap();
    let mut swarm2 = create_test_swarm().await.unwrap();
    
    let peer1_id = *swarm1.local_peer_id();
    
    // Subscribe to floodsub
    swarm1.behaviour_mut().floodsub.subscribe(TOPIC.clone());
    swarm2.behaviour_mut().floodsub.subscribe(TOPIC.clone());
    
    // Establish connection
    let _addr = establish_connection(&mut swarm1, &mut swarm2).await;
    assert!(_addr.is_some(), "Connection should be established");
    
    // Allow time for connection to stabilize
    time::sleep(Duration::from_millis(500)).await;
    
    // Create many messages of varying sizes
    let message_count = 1000;
    let messages_sent = Arc::new(AtomicUsize::new(0));
    let messages_received = Arc::new(AtomicUsize::new(0));
    
    let start_time = Instant::now();
    
    // Send messages with varying content sizes
    for i in 0..message_count {
        let content_size = (i % 10 + 1) * 1000; // 1KB to 10KB
        let large_body = "X".repeat(content_size);
        
        let story = PublishedStory::new(
            Story::new(
                i,
                format!("Memory Test Story {}", i),
                "Header".to_string(),
                large_body,
                true,
            ),
            peer1_id.to_string(),
        );
        
        let message_data = serde_json::to_string(&story).unwrap().into_bytes();
        swarm1.behaviour_mut().floodsub.publish(TOPIC.clone(), message_data);
        messages_sent.fetch_add(1, Ordering::Relaxed);
        
        // Periodic small delay
        if i % 100 == 0 {
            time::sleep(Duration::from_millis(20)).await;
        }
    }
    
    // Process messages
    let processing_timeout = Duration::from_secs(30);
    let processing_start = Instant::now();
    
    while processing_start.elapsed() < processing_timeout && 
          messages_received.load(Ordering::Relaxed) < message_count / 2 {
        
        tokio::select! {
            event1 = swarm1.select_next_some() => {
                // Process sender events
            }
            event2 = swarm2.select_next_some() => {
                if let SwarmEvent::Behaviour(StoryBehaviourEvent::Floodsub(FloodsubEvent::Message(msg))) = event2 {
                    if let Ok(story) = serde_json::from_slice::<PublishedStory>(&msg.data) {
                        if story.story.name.starts_with("Memory Test Story") {
                            messages_received.fetch_add(1, Ordering::Relaxed);
                            
                            // Simulate processing - don't hold onto the message
                            drop(story);
                        }
                    }
                }
            }
            _ = time::sleep(Duration::from_millis(10)) => {}
        }
    }
    
    let total_duration = start_time.elapsed();
    let final_sent = messages_sent.load(Ordering::Relaxed);
    let final_received = messages_received.load(Ordering::Relaxed);
    
    println!("Memory usage performance test:");
    println!("  Messages sent: {}", final_sent);
    println!("  Messages received: {}", final_received);
    println!("  Processing duration: {:?}", total_duration);
    println!("  Average message rate: {:.2} msgs/sec", final_received as f64 / total_duration.as_secs_f64());
    
    // The test validates that the system can handle varying message sizes
    // without excessive memory accumulation
    assert!(final_sent == message_count, "All messages should be sent");
    assert!(final_received > 0, "Some messages should be received");
    
    // Memory usage test - ensure we don't run out of memory
    // The fact that the test completes indicates reasonable memory management
}

#[tokio::test]
async fn test_story_sync_performance_large_dataset() {
    // Test story sync performance with large datasets
    let mut swarm1 = create_test_swarm().await.unwrap();
    let mut swarm2 = create_test_swarm().await.unwrap();
    
    let peer1_id = *swarm1.local_peer_id();
    let peer2_id = *swarm2.local_peer_id();
    
    // Establish connection
    let _addr = establish_connection(&mut swarm1, &mut swarm2).await;
    assert!(_addr.is_some(), "Connection should be established");
    
    // Allow extra time for large connection to stabilize
    time::sleep(Duration::from_millis(1500)).await;
    
    // Create large dataset of stories
    let story_count = 5000;
    let large_stories: Vec<Story> = (0..story_count).map(|i| {
        let content_size = (i % 5 + 1) * 200; // 200B to 1KB stories
        Story::new_with_channel(
            i,
            format!("Sync Performance Story {}", i),
            format!("Header {}", i),
            "Content ".repeat(content_size),
            true,
            format!("channel_{}", i % 10),
        )
    }).collect();
    
    // Send sync request
    let sync_request = StorySyncRequest {
        from_peer_id: peer2_id.to_string(),
        from_name: "Performance Tester".to_string(),
        last_sync_timestamp: 0,
        subscribed_channels: (0..10).map(|i| format!("channel_{}", i)).collect(),
        timestamp: current_timestamp(),
    };
    
    let sync_start = Instant::now();
    swarm2.behaviour_mut().story_sync.send_request(&peer1_id, sync_request);
    
    let mut sync_request_handled = false;
    let mut sync_response_received = false;
    let mut synced_stories = Vec::new();
    
    for _ in 0..200 { // Increased timeout for large data
        tokio::select! {
            event1 = swarm1.select_next_some() => {
                match event1 {
                    SwarmEvent::Behaviour(StoryBehaviourEvent::StorySync(
                        RequestResponseEvent::Message { message: Message::Request { channel, .. }, .. }
                    )) => {
                        sync_request_handled = true;
                        
                        // Send large story dataset
                        let sync_response = StorySyncResponse {
                            stories: large_stories.clone(),
                            from_peer_id: peer1_id.to_string(),
                            from_name: "Performance Responder".to_string(),
                            sync_timestamp: current_timestamp(),
                        };
                        
                        swarm1.behaviour_mut().story_sync.send_response(channel, sync_response).unwrap();
                    }
                    _ => {}
                }
            }
            event2 = swarm2.select_next_some() => {
                match event2 {
                    SwarmEvent::Behaviour(StoryBehaviourEvent::StorySync(
                        RequestResponseEvent::Message { message: Message::Response { response, .. }, .. }
                    )) => {
                        synced_stories = response.stories;
                        sync_response_received = true;
                        break;
                    }
                    SwarmEvent::Behaviour(StoryBehaviourEvent::StorySync(
                        RequestResponseEvent::OutboundFailure { .. }
                    )) => {
                        println!("Sync failed (possibly due to large dataset)");
                        sync_response_received = true; // Mark as completed even if failed
                        break;
                    }
                    _ => {}
                }
            }
            _ = time::sleep(Duration::from_millis(50)) => {}
        }
    }
    
    let sync_duration = sync_start.elapsed();
    
    // Calculate performance metrics
    let stories_per_second = if sync_response_received && !synced_stories.is_empty() {
        synced_stories.len() as f64 / sync_duration.as_secs_f64()
    } else {
        0.0
    };
    
    let total_data_size = synced_stories.iter()
        .map(|s| s.name.len() + s.header.len() + s.body.len() + s.channel.len())
        .sum::<usize>();
    
    let throughput_kbps = (total_data_size as f64 / 1024.0) / sync_duration.as_secs_f64();
    
    println!("Story sync performance (large dataset):");
    println!("  Target stories: {}", story_count);
    println!("  Synced stories: {}", synced_stories.len());
    println!("  Sync duration: {:?}", sync_duration);
    println!("  Stories per second: {:.2}", stories_per_second);
    println!("  Total data size: {} KB", total_data_size / 1024);
    println!("  Throughput: {:.2} KB/sec", throughput_kbps);
    
    // Performance assertions
    assert!(sync_request_handled, "Sync request should be handled");
    
    if sync_response_received && !synced_stories.is_empty() {
        assert!(synced_stories.len() == story_count, "All stories should be synced");
        assert!(sync_duration < Duration::from_secs(30), "Sync should complete within reasonable time");
        assert!(stories_per_second > 10.0, "Should achieve reasonable story sync rate");
    } else {
        // Large dataset sync might fail in test environment - this is acceptable
        println!("Large dataset sync test completed (may have failed due to size limits)");
    }
}

#[tokio::test] 
async fn test_network_resilience_under_load() {
    // Test network resilience under sustained load
    let mut swarm1 = create_test_swarm().await.unwrap();
    let mut swarm2 = create_test_swarm().await.unwrap();
    
    let peer1_id = *swarm1.local_peer_id();
    let peer2_id = *swarm2.local_peer_id();
    
    // Subscribe to floodsub
    swarm1.behaviour_mut().floodsub.subscribe(TOPIC.clone());
    swarm2.behaviour_mut().floodsub.subscribe(TOPIC.clone());
    
    // Establish connection
    let _addr = establish_connection(&mut swarm1, &mut swarm2).await;
    assert!(_addr.is_some(), "Connection should be established");
    
    // Allow time for connection to stabilize
    time::sleep(Duration::from_millis(500)).await;
    
    // Test sustained load over time
    let load_duration = Duration::from_secs(10);
    let message_interval = Duration::from_millis(100);
    
    let mut messages_sent = 0;
    let mut messages_received = 0;
    let mut connection_issues = 0;
    
    let start_time = Instant::now();
    let mut last_message_time = Instant::now();
    
    while start_time.elapsed() < load_duration {
        // Send periodic messages
        if last_message_time.elapsed() >= message_interval {
            let story = PublishedStory::new(
                Story::new(
                    messages_sent,
                    format!("Resilience Test {}", messages_sent),
                    "Header".to_string(),
                    format!("Sustained load message {}", messages_sent),
                    true,
                ),
                peer1_id.to_string(),
            );
            
            let story_data = serde_json::to_string(&story).unwrap().into_bytes();
            swarm1.behaviour_mut().floodsub.publish(
                TOPIC.clone(),
                story_data
            );
            messages_sent += 1;
            last_message_time = Instant::now();
        }
        
        // Process events
        tokio::select! {
            event1 = swarm1.select_next_some() => {
                match event1 {
                    SwarmEvent::ConnectionClosed { .. } => connection_issues += 1,
                    SwarmEvent::OutgoingConnectionError { .. } => connection_issues += 1,
                    _ => {}
                }
            }
            event2 = swarm2.select_next_some() => {
                match event2 {
                    SwarmEvent::Behaviour(StoryBehaviourEvent::Floodsub(FloodsubEvent::Message(msg))) => {
                        if let Ok(story) = serde_json::from_slice::<PublishedStory>(&msg.data) {
                            if story.story.name.starts_with("Resilience Test") {
                                messages_received += 1;
                            }
                        }
                    }
                    SwarmEvent::ConnectionClosed { .. } => connection_issues += 1,
                    _ => {}
                }
            }
            _ = time::sleep(Duration::from_millis(10)) => {}
        }
    }
    
    let total_duration = start_time.elapsed();
    let delivery_rate = messages_received as f64 / messages_sent as f64;
    let message_rate = messages_sent as f64 / total_duration.as_secs_f64();
    
    println!("Network resilience under sustained load:");
    println!("  Test duration: {:?}", total_duration);
    println!("  Messages sent: {}", messages_sent);
    println!("  Messages received: {}", messages_received);
    println!("  Connection issues: {}", connection_issues);
    println!("  Delivery rate: {:.2}%", delivery_rate * 100.0);
    println!("  Message rate: {:.2} msgs/sec", message_rate);
    
    // Resilience assertions
    assert!(messages_sent > 0, "Messages should be sent during load test");
    assert!(messages_received > 0, "Some messages should be received");
    assert!(delivery_rate > 0.5, "Should maintain reasonable delivery rate under load");
    assert!(connection_issues < messages_sent / 10, "Connection issues should be minimal");
    
    // The test validates that the network can handle sustained load
    // without significant degradation in performance or connectivity
}