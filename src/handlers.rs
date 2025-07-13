use crate::network::{PEER_ID, StoryBehaviour, TOPIC};
use crate::storage::{create_new_story, publish_story, read_local_stories, save_local_peer_name};
use crate::types::{DirectMessage, ListMode, ListRequest, PeerName, Story};
use bytes::Bytes;
use libp2p::PeerId;
use libp2p::swarm::Swarm;
use log::info;
use std::collections::{HashMap, HashSet};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

pub async fn handle_list_peers(
    swarm: &mut Swarm<StoryBehaviour>,
    peer_names: &HashMap<PeerId, String>,
) {
    println!("Discovered Peers:");
    let nodes = swarm.behaviour().mdns.discovered_nodes();
    let mut unique_peers = HashSet::new();
    for peer in nodes {
        unique_peers.insert(peer);
    }
    unique_peers.iter().for_each(|p| {
        let name = peer_names
            .get(p)
            .map(|n| format!(" ({})", n))
            .unwrap_or_default();
        println!("{}{}", p, name);
    });
}

pub async fn handle_list_connections(
    swarm: &mut Swarm<StoryBehaviour>,
    peer_names: &HashMap<PeerId, String>,
) {
    let connected_peers: Vec<_> = swarm.connected_peers().cloned().collect();
    println!("Connected Peers: {}", connected_peers.len());
    for peer in connected_peers {
        let name = peer_names
            .get(&peer)
            .map(|n| format!(" ({})", n))
            .unwrap_or_default();
        println!("Connected to: {}{}", peer, name);
    }
}

pub async fn handle_list_stories(cmd: &str, swarm: &mut Swarm<StoryBehaviour>) {
    let rest = cmd.strip_prefix("ls s ");
    match rest {
        Some("all") => {
            println!("Requesting all stories from all peers");
            let req = ListRequest {
                mode: ListMode::ALL,
            };
            let json = serde_json::to_string(&req).expect("can jsonify request");
            info!("JSON od request: {}", json);
            let json_bytes = Bytes::from(json.into_bytes());
            info!(
                "Publishing to topic: {:?} from peer:{:?}",
                TOPIC.clone(),
                PEER_ID.clone()
            );
            swarm
                .behaviour_mut()
                .floodsub
                .publish(TOPIC.clone(), json_bytes);
            info!("Published request");
        }
        Some(story_peer_id) => {
            println!("Requesting all stories from peer: {}", story_peer_id);
            let req = ListRequest {
                mode: ListMode::One(story_peer_id.to_owned()),
            };
            let json = serde_json::to_string(&req).expect("can jsonify request");
            info!("JSON od request: {}", json);
            let json_bytes = Bytes::from(json.into_bytes());
            swarm
                .behaviour_mut()
                .floodsub
                .publish(TOPIC.clone(), json_bytes);
        }
        None => {
            println!("Local stories:");
            match read_local_stories().await {
                Ok(v) => {
                    println!("Local stories ({})", v.len());
                    v.iter().for_each(|r| println!("{:?}", r));
                }
                Err(e) => eprintln!("error fetching local stories: {}", e),
            };
        }
    };
}

async fn prompt_for_input(prompt: &str) -> Result<String, Box<dyn std::error::Error>> {
    println!("{}", prompt);
    let mut stdin = BufReader::new(tokio::io::stdin());
    let mut input = String::new();
    stdin.read_line(&mut input).await?;
    Ok(input.trim().to_string())
}

pub async fn handle_create_stories(cmd: &str) {
    if let Some(rest) = cmd.strip_prefix("create s") {
        let rest = rest.trim();

        // Check if user wants interactive mode (no arguments provided)
        if rest.is_empty() {
            println!("Creating a new story interactively...");

            // Prompt for each element
            let name = match prompt_for_input("Enter story name:").await {
                Ok(input) if !input.is_empty() => input,
                Ok(_) => {
                    println!("Story name cannot be empty. Story creation cancelled.");
                    return;
                }
                Err(e) => {
                    eprintln!("Error reading input: {}. Story creation cancelled.", e);
                    return;
                }
            };

            let header = match prompt_for_input("Enter story header:").await {
                Ok(input) if !input.is_empty() => input,
                Ok(_) => {
                    println!("Story header cannot be empty. Story creation cancelled.");
                    return;
                }
                Err(e) => {
                    eprintln!("Error reading input: {}. Story creation cancelled.", e);
                    return;
                }
            };

            let body = match prompt_for_input("Enter story body:").await {
                Ok(input) if !input.is_empty() => input,
                Ok(_) => {
                    println!("Story body cannot be empty. Story creation cancelled.");
                    return;
                }
                Err(e) => {
                    eprintln!("Error reading input: {}. Story creation cancelled.", e);
                    return;
                }
            };

            if let Err(e) = create_new_story(&name, &header, &body).await {
                eprintln!("error creating story: {}", e);
            } else {
                println!("Story created successfully");
            }
        } else {
            // Legacy mode: parse pipe-separated arguments
            let elements: Vec<&str> = rest.split('|').collect();
            if elements.len() < 3 {
                println!("too few arguments - Format: name|header|body");
                println!("Alternatively, use 'create s' for interactive mode");
            } else {
                let name = elements.first().expect("name is there");
                let header = elements.get(1).expect("header is there");
                let body = elements.get(2).expect("body is there");
                if let Err(e) = create_new_story(name, header, body).await {
                    eprintln!("error creating story: {}", e);
                } else {
                    println!("Story created successfully");
                };
            }
        }
    }
}

pub async fn handle_publish_story(cmd: &str, story_sender: mpsc::UnboundedSender<Story>) {
    if let Some(rest) = cmd.strip_prefix("publish s") {
        match rest.trim().parse::<usize>() {
            Ok(id) => {
                if let Err(e) = publish_story(id, story_sender).await {
                    eprintln!("error publishing story with id {}, {}", id, e)
                } else {
                    println!("Published story with id: {}", id);
                }
            }
            Err(e) => eprintln!("invalid id: {}, {}", rest.trim(), e),
        };
    }
}

pub async fn handle_help(_cmd: &str) {
    println!("ls p to list discovered peers");
    println!("ls c to list connected peers");
    println!("ls s to list stories");
    println!("create s to create story (interactive mode)");
    println!("create s name|header|body to create story (quick mode)");
    println!("publish s to publish story");
    println!("name <alias> to set your peer name");
    println!("msg <peer_alias> <message> to send direct message");
    println!("quit to quit");
}

pub async fn handle_set_name(cmd: &str, local_peer_name: &mut Option<String>) -> Option<PeerName> {
    if let Some(name) = cmd.strip_prefix("name ") {
        let name = name.trim();
        if name.is_empty() {
            eprintln!("Name cannot be empty");
            return None;
        }

        *local_peer_name = Some(name.to_string());
        println!("Set local peer name to: {}", name);

        // Save the peer name to storage for persistence across restarts
        if let Err(e) = save_local_peer_name(name).await {
            eprintln!("Warning: Failed to save peer name: {}", e);
        }

        // Return a PeerName message to broadcast to connected peers
        Some(PeerName::new(PEER_ID.to_string(), name.to_string()))
    } else {
        eprintln!("Usage: name <alias>");
        None
    }
}

/// Parse a direct message command that may contain peer names with spaces
fn parse_direct_message_command(
    rest: &str,
    known_peer_names: &HashMap<PeerId, String>,
) -> Option<(String, String)> {
    // Try to match against known peer names first (handles names with spaces)
    // Sort by length in descending order to prioritize longer names
    let mut peer_names: Vec<&String> = known_peer_names.values().collect();
    peer_names.sort_by(|a, b| b.len().cmp(&a.len()));
    
    for peer_name in peer_names {
        // Check if the rest starts with this peer name
        if rest.starts_with(peer_name) {
            let remaining = &rest[peer_name.len()..];
            
            // If we have an exact match (peer name with no message)
            if remaining.is_empty() {
                return None; // No message provided
            }
            
            // If the peer name is followed by a space
            if remaining.starts_with(' ') {
                let message = remaining[1..].trim();
                if !message.is_empty() {
                    return Some((peer_name.clone(), message.to_string()));
                } else {
                    return None; // Empty message after space
                }
            }
            
            // If it's not followed by a space, this is an invalid command
            // because the peer name should be followed by a space and then a message
            return None;
        }
    }
    
    // Fallback to original parsing for backward compatibility
    // This handles simple names without spaces that are not in the known peer list
    let parts: Vec<&str> = rest.splitn(2, ' ').collect();
    if parts.len() >= 2 {
        let to_name = parts[0].trim();
        let message = parts[1].trim();
        
        if !to_name.is_empty() && !message.is_empty() {
            return Some((to_name.to_string(), message.to_string()));
        }
    }
    
    None
}

pub async fn handle_direct_message(
    cmd: &str,
    swarm: &mut Swarm<StoryBehaviour>,
    peer_names: &HashMap<PeerId, String>,
    local_peer_name: &Option<String>,
) {
    if let Some(rest) = cmd.strip_prefix("msg ") {
        let (to_name, message) = match parse_direct_message_command(rest, peer_names) {
            Some((name, msg)) => (name, msg),
            None => {
                eprintln!("Usage: msg <peer_alias> <message>");
                return;
            }
        };

        if to_name.is_empty() || message.is_empty() {
            eprintln!("Both peer alias and message must be non-empty");
            return;
        }

        let from_name = match local_peer_name {
            Some(name) => name.clone(),
            None => {
                eprintln!("You must set your name first using 'name <alias>'");
                return;
            }
        };

        // Check if the target peer exists
        let peer_exists = peer_names.values().any(|name| name == &to_name);
        if !peer_exists {
            eprintln!(
                "Peer '{}' not found. Use 'ls p' to see available peers.",
                to_name
            );
            return;
        }

        let direct_msg = DirectMessage::new(
            PEER_ID.to_string(),
            from_name,
            to_name.to_string(),
            message.to_string(),
        );

        let json = serde_json::to_string(&direct_msg).expect("can jsonify direct message");
        let json_bytes = Bytes::from(json.into_bytes());

        swarm
            .behaviour_mut()
            .floodsub
            .publish(TOPIC.clone(), json_bytes);

        println!("Direct message sent to {}: {}", to_name, message);
        info!(
            "Sent direct message to {} from {}",
            to_name, direct_msg.from_name
        );
    } else {
        eprintln!("Usage: msg <peer_alias> <message>");
    }
}

pub async fn establish_direct_connection(swarm: &mut Swarm<StoryBehaviour>, addr_str: &str) {
    match addr_str.parse::<libp2p::Multiaddr>() {
        Ok(addr) => {
            println!("Manually dialing address: {}", addr);
            match swarm.dial(addr) {
                Ok(_) => {
                    println!("Dialing initiated successfully");

                    let connected_peers: Vec<_> = swarm.connected_peers().cloned().collect();
                    info!("Number of connected peers: {}", connected_peers.len());

                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    let connected_peers_after: Vec<_> = swarm.connected_peers().cloned().collect();
                    info!(
                        "Number of connected peers after 2 seconds: {}",
                        connected_peers_after.len()
                    );
                    for peer in connected_peers {
                        info!("Connected to peer: {}", peer);

                        info!("Adding peer to floodsub: {}", peer);
                        swarm
                            .behaviour_mut()
                            .floodsub
                            .add_node_to_partial_view(peer);
                    }
                }
                Err(e) => eprintln!("Failed to dial: {}", e),
            }
        }
        Err(e) => eprintln!("Failed to parse address: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Story;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_handle_set_name_valid() {
        let mut local_peer_name = None;

        // Test setting a valid name
        let result = handle_set_name("name Alice", &mut local_peer_name).await;

        assert!(result.is_some());
        assert_eq!(local_peer_name, Some("Alice".to_string()));

        let peer_name = result.unwrap();
        assert_eq!(peer_name.name, "Alice");
        assert_eq!(peer_name.peer_id, PEER_ID.to_string());
    }

    #[tokio::test]
    async fn test_handle_set_name_empty() {
        let mut local_peer_name = None;

        // Test setting an empty name
        let result = handle_set_name("name ", &mut local_peer_name).await;

        assert!(result.is_none());
        assert_eq!(local_peer_name, None);
    }

    #[tokio::test]
    async fn test_handle_set_name_invalid_format() {
        let mut local_peer_name = None;

        // Test invalid command format
        let result = handle_set_name("invalid command", &mut local_peer_name).await;

        assert!(result.is_none());
        assert_eq!(local_peer_name, None);
    }

    #[tokio::test]
    async fn test_handle_set_name_with_spaces() {
        let mut local_peer_name = None;

        // Test name with spaces
        let result = handle_set_name("name Alice Smith", &mut local_peer_name).await;

        assert!(result.is_some());
        assert_eq!(local_peer_name, Some("Alice Smith".to_string()));
    }

    #[tokio::test]
    async fn test_handle_help() {
        // This function just prints help text, we'll test it doesn't panic
        handle_help("help").await;
        // If we get here without panicking, the test passes
    }

    #[test]
    fn test_handle_create_stories_valid() {
        let rt = tokio::runtime::Runtime::new().unwrap();

        // Note: This will try to create actual files, but we're testing the parsing logic
        rt.block_on(async {
            // Test valid create story command format
            handle_create_stories("create sTest Story|Test Header|Test Body").await;
            // The function will try to create a story but may fail due to file system issues
            // We're mainly testing that the parsing doesn't panic
        });
    }

    #[test]
    fn test_handle_create_stories_invalid() {
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            // Test invalid format (too few arguments)
            handle_create_stories("create sTest|Header").await;

            // Test completely invalid format
            handle_create_stories("invalid command").await;
        });
    }

    #[test]
    fn test_handle_publish_story_valid_id() {
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            let (sender, _receiver) = mpsc::unbounded_channel::<Story>();

            // Test with valid ID format
            handle_publish_story("publish s123", sender).await;
            // The function will try to publish but may fail due to file system issues
            // We're testing the parsing logic
        });
    }

    #[test]
    fn test_handle_publish_story_invalid_id() {
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            let (sender, _receiver) = mpsc::unbounded_channel::<Story>();

            // Test with invalid ID format
            handle_publish_story("publish sabc", sender).await;

            // Test with invalid command format - use a new sender
            let (sender2, _receiver2) = mpsc::unbounded_channel::<Story>();
            handle_publish_story("invalid command", sender2).await;
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
        use libp2p::PeerId;
        
        // Create a mock peer names map
        let mut peer_names = HashMap::new();
        let peer_id1 = PeerId::random();
        let peer_id2 = PeerId::random();
        let peer_id3 = PeerId::random();
        
        peer_names.insert(peer_id1, "Alice".to_string());
        peer_names.insert(peer_id2, "Alice Smith".to_string());
        peer_names.insert(peer_id3, "Bob Jones Jr".to_string());
        
        // Test simple name without spaces
        let result = parse_direct_message_command("Alice Hello there!", &peer_names);
        assert_eq!(result, Some(("Alice".to_string(), "Hello there!".to_string())));
        
        // Test name with spaces
        let result = parse_direct_message_command("Alice Smith Hello world", &peer_names);
        assert_eq!(result, Some(("Alice Smith".to_string(), "Hello world".to_string())));
        
        // Test name with multiple spaces
        let result = parse_direct_message_command("Bob Jones Jr How are you?", &peer_names);
        assert_eq!(result, Some(("Bob Jones Jr".to_string(), "How are you?".to_string())));
        
        // Test edge case - no message
        let result = parse_direct_message_command("Alice Smith", &peer_names);
        assert_eq!(result, None);
        
        // Test edge case - no space after name
        let result = parse_direct_message_command("Alice SmithHello", &peer_names);
        assert_eq!(result, None);
        
        // Test fallback to original parsing for simple names not in known peers
        let result = parse_direct_message_command("Charlie Hello there", &peer_names);
        assert_eq!(result, Some(("Charlie".to_string(), "Hello there".to_string())));
    }

    #[tokio::test]
    async fn test_handle_direct_message_no_local_name() {
        use crate::network::create_swarm;
        use std::collections::HashMap;

        let mut swarm = create_swarm().expect("Failed to create swarm");
        let peer_names = HashMap::new();
        let local_peer_name = None;

        // This should print an error message about needing to set name first
        handle_direct_message("msg Alice Hello", &mut swarm, &peer_names, &local_peer_name).await;
        // Test passes if it doesn't panic
    }

    #[tokio::test]
    async fn test_handle_direct_message_invalid_format() {
        use crate::network::create_swarm;
        use std::collections::HashMap;

        let mut swarm = create_swarm().expect("Failed to create swarm");
        let peer_names = HashMap::new();
        let local_peer_name = Some("Bob".to_string());

        // Test invalid command formats
        handle_direct_message("msg Alice", &mut swarm, &peer_names, &local_peer_name).await;
        handle_direct_message("msg", &mut swarm, &peer_names, &local_peer_name).await;
        handle_direct_message("invalid command", &mut swarm, &peer_names, &local_peer_name).await;
        // Test passes if it doesn't panic
    }

    #[tokio::test]
    async fn test_handle_direct_message_with_spaces_in_names() {
        use crate::network::create_swarm;
        use std::collections::HashMap;
        use libp2p::PeerId;

        let mut swarm = create_swarm().expect("Failed to create swarm");
        let mut peer_names = HashMap::new();
        let peer_id = PeerId::random();
        peer_names.insert(peer_id, "Alice Smith".to_string());
        
        let local_peer_name = Some("Bob".to_string());

        // Test message to peer with spaces in name
        handle_direct_message("msg Alice Smith Hello world", &mut swarm, &peer_names, &local_peer_name).await;
        // Test passes if it doesn't panic and correctly parses the name
    }
}
