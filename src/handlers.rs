use crate::network::{PEER_ID, StoryBehaviour, TOPIC};
use crate::storage::{create_new_story, publish_story, read_local_stories, save_local_peer_name};
use crate::types::{ListMode, ListRequest, PeerName, Story};
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
}
