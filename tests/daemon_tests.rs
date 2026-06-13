#![cfg(unix)]

use p2p_play::daemon::DaemonServer;
use p2p_play::daemon::client::send_request;
use p2p_play::daemon::protocol::{
    ChannelInfo, ConversationSummary, DaemonRequest, DaemonResponse, MessageInfo, MessagesSummary,
    PeerInfo,
};
use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns a unique temporary directory for each test, avoiding socket path
/// collisions between parallel test runs.
fn tmp_dir(label: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("p2p_daemon_test_{}_{}", label, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Spawn a `DaemonServer` that answers commands with `responder`.
/// Returns the socket path, the shutdown sender, and the server task handle.
async fn spawn_server(
    dir: &PathBuf,
    responder: impl Fn(DaemonRequest) -> DaemonResponse + Send + 'static,
) -> (PathBuf, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let socket_path = dir.join("daemon.sock");
    let pid_path = dir.join("daemon.pid");

    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let server = DaemonServer::new(&socket_path, &pid_path, cmd_tx).unwrap();

    // Background task: answer every incoming command with the responder.
    tokio::spawn(async move {
        while let Some((req, reply_tx)) = cmd_rx.recv().await {
            let _ = reply_tx.send(responder(req));
        }
    });

    let handle = tokio::spawn(server.run(shutdown_rx));

    // Give the server a moment to start listening.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    (socket_path, shutdown_tx, handle)
}

// ---------------------------------------------------------------------------
// Protocol serialization tests (no I/O needed)
// ---------------------------------------------------------------------------

#[test]
fn daemon_request_peers_serializes_correctly() {
    let req = DaemonRequest::Peers;
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("\"command\":\"peers\""), "got: {json}");
}

#[test]
fn daemon_request_conversations_serializes_with_limit() {
    let req = DaemonRequest::Conversations { limit: 20 };
    let json = serde_json::to_string(&req).unwrap();
    assert!(
        json.contains("\"command\":\"conversations\""),
        "got: {json}"
    );
    assert!(json.contains("\"limit\":20"), "got: {json}");
}

#[test]
fn daemon_request_channels_serializes_correctly() {
    let req = DaemonRequest::Channels;
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("\"command\":\"channels\""), "got: {json}");
}

#[test]
fn daemon_request_unread_serializes_with_limit() {
    let req = DaemonRequest::Unread { limit: 50 };
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("\"command\":\"unread\""), "got: {json}");
    assert!(json.contains("\"limit\":50"), "got: {json}");
}

#[test]
fn daemon_response_peers_roundtrips() {
    let resp = DaemonResponse::Peers {
        peers: vec![PeerInfo {
            peer_id: "12D3KooW".to_string(),
            name: "alice".to_string(),
        }],
    };
    let json = serde_json::to_string(&resp).unwrap();
    let back: DaemonResponse = serde_json::from_str(&json).unwrap();
    match back {
        DaemonResponse::Peers { peers } => {
            assert_eq!(peers.len(), 1);
            assert_eq!(peers[0].peer_id, "12D3KooW");
            assert_eq!(peers[0].name, "alice");
        }
        other => panic!("Expected Peers, got {other:?}"),
    }
}

#[test]
fn daemon_response_conversations_roundtrips() {
    let resp = DaemonResponse::Conversations {
        conversations: vec![ConversationSummary {
            peer_id: "abc".to_string(),
            peer_name: "bob".to_string(),
            unread_count: 3,
            last_activity: 1_700_000_000,
        }],
    };
    let json = serde_json::to_string(&resp).unwrap();
    let back: DaemonResponse = serde_json::from_str(&json).unwrap();
    match back {
        DaemonResponse::Conversations { conversations } => {
            assert_eq!(conversations.len(), 1);
            assert_eq!(conversations[0].unread_count, 3);
        }
        other => panic!("Expected Conversations, got {other:?}"),
    }
}

#[test]
fn daemon_response_channels_roundtrips() {
    let resp = DaemonResponse::Channels {
        channels: vec![ChannelInfo {
            name: "general".to_string(),
            description: "Default channel".to_string(),
        }],
    };
    let json = serde_json::to_string(&resp).unwrap();
    let back: DaemonResponse = serde_json::from_str(&json).unwrap();
    match back {
        DaemonResponse::Channels { channels } => {
            assert_eq!(channels.len(), 1);
            assert_eq!(channels[0].name, "general");
            assert_eq!(channels[0].description, "Default channel");
        }
        other => panic!("Expected Channels, got {other:?}"),
    }
}

#[test]
fn daemon_response_unread_roundtrips() {
    let resp = DaemonResponse::Unread {
        messages: vec![MessagesSummary {
            peer_id: "peer1".to_string(),
            peer_name: "alice".to_string(),
            messages: vec![
                MessageInfo {
                    content: "hello".to_string(),
                    timestamp: 1_700_000_001,
                },
                MessageInfo {
                    content: "world".to_string(),
                    timestamp: 1_700_000_002,
                },
            ],
        }],
    };
    let json = serde_json::to_string(&resp).unwrap();
    let back: DaemonResponse = serde_json::from_str(&json).unwrap();
    match back {
        DaemonResponse::Unread { messages } => {
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0].peer_id, "peer1");
            assert_eq!(messages[0].peer_name, "alice");
            assert_eq!(messages[0].messages.len(), 2);
            assert_eq!(messages[0].messages[0].content, "hello");
            assert_eq!(messages[0].messages[1].timestamp, 1_700_000_002);
        }
        other => panic!("Expected Unread, got {other:?}"),
    }
}

#[test]
fn daemon_response_error_roundtrips() {
    let resp = DaemonResponse::Error {
        message: "something went wrong".to_string(),
    };
    let json = serde_json::to_string(&resp).unwrap();
    let back: DaemonResponse = serde_json::from_str(&json).unwrap();
    match back {
        DaemonResponse::Error { message } => assert_eq!(message, "something went wrong"),
        other => panic!("Expected Error, got {other:?}"),
    }
}

#[test]
fn daemon_request_deserializes_peers_from_json() {
    let json = r#"{"command":"peers"}"#;
    let req: DaemonRequest = serde_json::from_str(json).unwrap();
    assert!(matches!(req, DaemonRequest::Peers));
}

#[test]
fn daemon_request_deserializes_conversations_from_json() {
    let json = r#"{"command":"conversations","limit":5}"#;
    let req: DaemonRequest = serde_json::from_str(json).unwrap();
    assert!(matches!(req, DaemonRequest::Conversations { limit: 5 }));
}

#[test]
fn daemon_request_deserializes_channels_from_json() {
    let json = r#"{"command":"channels"}"#;
    let req: DaemonRequest = serde_json::from_str(json).unwrap();
    assert!(matches!(req, DaemonRequest::Channels));
}

#[test]
fn daemon_request_deserializes_unread_from_json() {
    let json = r#"{"command":"unread","limit":10}"#;
    let req: DaemonRequest = serde_json::from_str(json).unwrap();
    assert!(matches!(req, DaemonRequest::Unread { limit: 10 }));
}

// ---------------------------------------------------------------------------
// DaemonServer lifecycle tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn server_creates_socket_and_pid_file_on_startup() {
    let dir = tmp_dir("lifecycle");
    let socket_path = dir.join("daemon.sock");
    let pid_path = dir.join("daemon.pid");

    let (cmd_tx, _cmd_rx) = mpsc::unbounded_channel();
    let (_shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let server = DaemonServer::new(&socket_path, &pid_path, cmd_tx).unwrap();

    assert!(
        socket_path.exists(),
        "socket file must exist after DaemonServer::new"
    );
    assert!(
        pid_path.exists(),
        "PID file must exist after DaemonServer::new"
    );

    let pid_content = std::fs::read_to_string(&pid_path).unwrap();
    let written_pid: u32 = pid_content.trim().parse().unwrap();
    assert_eq!(written_pid, std::process::id());

    // Cleanly shut down.
    drop(shutdown_rx); // receiver dropped → select fires
    drop(server);
}

#[tokio::test]
async fn server_removes_socket_and_pid_file_on_shutdown() {
    let dir = tmp_dir("shutdown");
    let socket_path = dir.join("daemon.sock");
    let pid_path = dir.join("daemon.pid");

    let (cmd_tx, _cmd_rx) = mpsc::unbounded_channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let server = DaemonServer::new(&socket_path, &pid_path, cmd_tx).unwrap();
    let handle = tokio::spawn(server.run(shutdown_rx));

    // Files should exist while server is running.
    assert!(socket_path.exists());
    assert!(pid_path.exists());

    // Signal shutdown and wait.
    let _ = shutdown_tx.send(());
    handle.await.unwrap();

    assert!(
        !socket_path.exists(),
        "socket must be removed after shutdown"
    );
    assert!(
        !pid_path.exists(),
        "PID file must be removed after shutdown"
    );
}

#[tokio::test]
async fn server_replaces_stale_socket_on_startup() {
    let dir = tmp_dir("stale_socket");
    let socket_path = dir.join("daemon.sock");
    let pid_path = dir.join("daemon.pid");

    // Create a stale socket file (simulates a previous crashed daemon).
    std::fs::write(&socket_path, b"stale").unwrap();

    let (cmd_tx, _cmd_rx) = mpsc::unbounded_channel();
    let (_shutdown_tx, _shutdown_rx) = oneshot::channel::<()>();

    // Should succeed even though socket file already exists.
    let result = DaemonServer::new(&socket_path, &pid_path, cmd_tx);
    assert!(
        result.is_ok(),
        "DaemonServer::new must replace a stale socket file"
    );
}

// ---------------------------------------------------------------------------
// Client ↔ server round-trip tests (real Unix socket IPC)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn client_receives_peers_response_from_server() {
    let dir = tmp_dir("client_peers");

    let (socket_path, shutdown_tx, handle) = spawn_server(&dir, |req| match req {
        DaemonRequest::Peers => DaemonResponse::Peers {
            peers: vec![
                PeerInfo {
                    peer_id: "peer1".to_string(),
                    name: "alice".to_string(),
                },
                PeerInfo {
                    peer_id: "peer2".to_string(),
                    name: "bob".to_string(),
                },
            ],
        },
        _ => DaemonResponse::Error {
            message: "unexpected".to_string(),
        },
    })
    .await;

    let resp = send_request(&socket_path, &DaemonRequest::Peers)
        .await
        .unwrap();

    match resp {
        DaemonResponse::Peers { peers } => {
            assert_eq!(peers.len(), 2);
            assert_eq!(peers[0].name, "alice");
            assert_eq!(peers[1].name, "bob");
        }
        other => panic!("Expected Peers response, got {other:?}"),
    }

    let _ = shutdown_tx.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn client_receives_conversations_response_from_server() {
    let dir = tmp_dir("client_conversations");

    let (socket_path, shutdown_tx, handle) = spawn_server(&dir, |req| match req {
        DaemonRequest::Conversations { .. } => DaemonResponse::Conversations {
            conversations: vec![ConversationSummary {
                peer_id: "p1".to_string(),
                peer_name: "carol".to_string(),
                unread_count: 7,
                last_activity: 1_700_000_000,
            }],
        },
        _ => DaemonResponse::Error {
            message: "unexpected".to_string(),
        },
    })
    .await;

    let resp = send_request(&socket_path, &DaemonRequest::Conversations { limit: 10 })
        .await
        .unwrap();

    match resp {
        DaemonResponse::Conversations { conversations } => {
            assert_eq!(conversations.len(), 1);
            assert_eq!(conversations[0].peer_name, "carol");
            assert_eq!(conversations[0].unread_count, 7);
        }
        other => panic!("Expected Conversations response, got {other:?}"),
    }

    let _ = shutdown_tx.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn client_receives_channels_response_from_server() {
    let dir = tmp_dir("client_channels");

    let (socket_path, shutdown_tx, handle) = spawn_server(&dir, |req| match req {
        DaemonRequest::Channels => DaemonResponse::Channels {
            channels: vec![ChannelInfo {
                name: "general".to_string(),
                description: "Default channel".to_string(),
            }],
        },
        _ => DaemonResponse::Error {
            message: "unexpected".to_string(),
        },
    })
    .await;

    let resp = send_request(&socket_path, &DaemonRequest::Channels)
        .await
        .unwrap();

    match resp {
        DaemonResponse::Channels { channels } => {
            assert_eq!(channels.len(), 1);
            assert_eq!(channels[0].name, "general");
            assert_eq!(channels[0].description, "Default channel");
        }
        other => panic!("Expected Channels response, got {other:?}"),
    }

    let _ = shutdown_tx.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn client_receives_unread_response_from_server() {
    let dir = tmp_dir("client_unread");

    let (socket_path, shutdown_tx, handle) = spawn_server(&dir, |req| match req {
        DaemonRequest::Unread { .. } => DaemonResponse::Unread {
            messages: vec![MessagesSummary {
                peer_id: "peerX".to_string(),
                peer_name: "dave".to_string(),
                messages: vec![
                    MessageInfo {
                        content: "hey there".to_string(),
                        timestamp: 1_700_000_010,
                    },
                    MessageInfo {
                        content: "ping?".to_string(),
                        timestamp: 1_700_000_020,
                    },
                ],
            }],
        },
        _ => DaemonResponse::Error {
            message: "unexpected".to_string(),
        },
    })
    .await;

    let resp = send_request(&socket_path, &DaemonRequest::Unread { limit: 50 })
        .await
        .unwrap();

    match resp {
        DaemonResponse::Unread { messages } => {
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0].peer_id, "peerX");
            assert_eq!(messages[0].peer_name, "dave");
            assert_eq!(messages[0].messages.len(), 2);
            assert_eq!(messages[0].messages[0].content, "hey there");
        }
        other => panic!("Expected Unread response, got {other:?}"),
    }

    let _ = shutdown_tx.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn client_receives_empty_unread_when_no_messages() {
    let dir = tmp_dir("client_unread_empty");

    let (socket_path, shutdown_tx, handle) = spawn_server(&dir, |req| match req {
        DaemonRequest::Unread { .. } => DaemonResponse::Unread { messages: vec![] },
        _ => DaemonResponse::Error {
            message: "unexpected".to_string(),
        },
    })
    .await;

    let resp = send_request(&socket_path, &DaemonRequest::Unread { limit: 50 })
        .await
        .unwrap();

    match resp {
        DaemonResponse::Unread { messages } => assert!(messages.is_empty()),
        other => panic!("Expected Unread response, got {other:?}"),
    }

    let _ = shutdown_tx.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn client_receives_error_on_invalid_json_request() {
    // Send raw malformed JSON directly to the socket to verify the server's
    // error-handling path.
    let dir = tmp_dir("invalid_json");
    let socket_path = dir.join("daemon.sock");
    let pid_path = dir.join("daemon.pid");

    let (cmd_tx, _cmd_rx) = mpsc::unbounded_channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let server = DaemonServer::new(&socket_path, &pid_path, cmd_tx).unwrap();
    let handle = tokio::spawn(server.run(shutdown_rx));
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixStream;

    let stream = UnixStream::connect(&socket_path).await.unwrap();
    let (read_half, mut write_half) = stream.into_split();
    write_half.write_all(b"not valid json\n").await.unwrap();

    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();

    let resp: DaemonResponse = serde_json::from_str(line.trim()).unwrap();
    assert!(
        matches!(resp, DaemonResponse::Error { .. }),
        "Expected Error response for malformed JSON, got: {resp:?}"
    );

    let _ = shutdown_tx.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn client_gets_error_when_daemon_not_running() {
    let socket_path = std::env::temp_dir().join("p2p_play_nonexistent.sock");
    // Ensure it doesn't exist.
    let _ = std::fs::remove_file(&socket_path);

    let result = send_request(&socket_path, &DaemonRequest::Peers).await;
    assert!(
        result.is_err(),
        "send_request must fail when socket does not exist"
    );
    let err = result.unwrap_err();
    assert!(
        err.contains("Cannot connect to daemon"),
        "Error message must be user-friendly, got: {err}"
    );
}

#[tokio::test]
async fn server_handles_multiple_sequential_requests() {
    let dir = tmp_dir("multi_req");

    let (socket_path, shutdown_tx, handle) = spawn_server(&dir, |req| match req {
        DaemonRequest::Peers => DaemonResponse::Peers { peers: vec![] },
        DaemonRequest::Channels => DaemonResponse::Channels { channels: vec![] },
        DaemonRequest::Conversations { .. } => DaemonResponse::Conversations {
            conversations: vec![],
        },
        DaemonRequest::Unread { .. } => DaemonResponse::Unread { messages: vec![] },
    })
    .await;

    for _ in 0..5 {
        let resp = send_request(&socket_path, &DaemonRequest::Peers)
            .await
            .unwrap();
        assert!(matches!(resp, DaemonResponse::Peers { .. }));
    }

    let _ = shutdown_tx.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn server_handles_empty_peers_list() {
    let dir = tmp_dir("empty_peers");

    let (socket_path, shutdown_tx, handle) =
        spawn_server(&dir, |_| DaemonResponse::Peers { peers: vec![] }).await;

    let resp = send_request(&socket_path, &DaemonRequest::Peers)
        .await
        .unwrap();
    match resp {
        DaemonResponse::Peers { peers } => assert!(peers.is_empty()),
        other => panic!("Expected Peers, got {other:?}"),
    }

    let _ = shutdown_tx.send(());
    handle.await.unwrap();
}
