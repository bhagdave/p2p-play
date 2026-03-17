# p2p-play

P2P-Play is a peer-to-peer story sharing application built with Rust using the [libp2p library (0.56.0)](https://github.com/libp2p/rust-libp2p/releases/tag/v0.56.0). The application allows users to create, publish, and share stories across a distributed network of peers using a modern terminal user interface.

## Features

### Core Functionality
- **Story Creation & Sharing**: Create and publish stories that can be shared across the peer-to-peer network
- **Peer Discovery**: Automatic peer discovery using mDNS for local networks and DHT (Kademlia) for broader connectivity
- **Direct Messaging**: Send private messages directly to other peers by their alias names
- **Channel System**: Organize stories into channels and subscribe to channels of interest
- **Local Storage**: SQLite database for persistent local story storage
- **Terminal UI**: Modern terminal user interface built with ratatui for interactive story management

### Network Features
- **Floodsub Messaging**: Reliable message broadcasting using libp2p's floodsub protocol
- **Connection Management**: Automatic connection maintenance and peer monitoring
- **Bootstrap Discovery**: Configurable bootstrap peers for network connectivity
- **Ping Keep-Alive**: Configurable ping settings to maintain stable connections
- **Direct Message Retry**: Automatic retry logic for failed direct messages

### Message Notifications
- **Visual Indicators**: Color-coded conversations show unread messages in magenta
- **Flash Notifications**: Brief visual flash in status bar when new messages arrive
- **Sound Alerts**: Optional audio notifications (configurable, disabled by default)
- **Enhanced Timestamps**: Relative time display (now, 5m ago, Mon 14:30) in conversation views
- **Delivery Status**: Visual checkmarks (✓) for delivered outgoing messages
- **Unread Counters**: Real-time unread message counts in status bar and conversation panel

### User Interface
- **Interactive Commands**: Full command-line interface for all operations
- **Real-time Updates**: Live display of connected peers, local stories, and received messages
- **Story Creation Wizard**: Step-by-step interactive story creation process
- **Multi-panel View**: Separate sections for peers, stories, logs, and input

## Terminal Requirements

P2P-Play uses [ratatui](https://github.com/ratatui-org/ratatui) for its terminal user interface. The following requirements must be met for correct rendering:

- **Minimum terminal size**: 80×24 columns×rows recommended — ratatui layouts may panic or render incorrectly in smaller terminals.
- **UTF-8 encoding**: The terminal must support UTF-8. On Windows CMD, run `chcp 65001` before launching, or switch to [Windows Terminal](https://aka.ms/terminal) which enables UTF-8 by default.
- **Colour support**: 256-colour mode is recommended for the best visual experience. Terminals limited to 16 colours will fall back gracefully but some visual indicators (e.g. unread message highlights) may appear less distinct.
- **Known working terminals**: iTerm2, GNOME Terminal, Windows Terminal, Alacritty, kitty.

## Available Commands

The application provides an interactive command-line interface with the following commands:

| Command | Description |
|---------|-------------|
| `ls p` | List all discovered peers |
| `ls c` | List currently connected peers |
| `ls s all` | Request all public stories from all peers |
| `ls s local` | Show your local stories |
| `create s` | Start interactive story creation wizard |
| `publish s <story_id>` | Publish a story to the network by ID |
| `name <alias>` | Set your peer's display name/alias |
| `msg <peer_alias> <message>` | Send a direct message to a peer |
| `connect <multiaddr>` | Connect to a specific peer using multiaddr |
| `help` | Show all available commands |
| `quit` | Exit the application |

## Getting Started

### Prerequisites
- Rust 1.88+ 
- Cargo package manager

### Building and Running
```bash
# Clone the repository
git clone https://github.com/bhagdave/p2p-play.git
cd p2p-play

# Build the project
cargo build

# Run the application
cargo run

# Run with debug logging
RUST_LOG=debug cargo run
```

### Quick Start

Once the application is running, follow these five steps to get up and running quickly:

1. **Set your peer name** — Give yourself a human-readable alias so other peers can identify you:
   ```
   name <alias>
   ```
   *Example:* `name alice`

2. **Discover peers** — On a local network, mDNS handles peer discovery automatically. You should see nearby peers appear in the peers panel within a few seconds. To connect to a peer on a different network, use a bootstrap peer address:
   ```
   connect <multiaddr>
   ```
   *Example:* `connect /ip4/1.2.3.4/tcp/4001/p2p/QmPeerID...`

3. **Check connected peers** — Verify you have peers available to share stories with:
   ```
   ls c
   ```

4. **Create a story** — Launch the interactive story creation wizard:
   ```
   create s
   ```
   Follow the prompts to enter a name, header, body, and channel for your story.

5. **Publish your story** — Share your story with all connected peers using the story ID shown after creation:
   ```
   publish s <story_id>
   ```

For a full list of commands, type `help` inside the application or see the [Available Commands](#available-commands) section.

### Testing
```bash
# Run all tests
cargo test

# Run with the test runner script
./scripts/test_runner.sh
```

## Platform Notes

### Windows

- **TCP listener binding**: On Windows, the application binds the TCP listener to `127.0.0.1` (localhost only) instead of `0.0.0.0` (all interfaces). This is intentional to avoid common port-conflict errors on Windows, but it means the node is **only reachable from the same machine**. Peers on other machines on your local network will not be able to connect to a Windows node that is listening only on `127.0.0.1`. mDNS may still allow those peers to *discover* the Windows node on the local subnet, but it will typically advertise only a loopback address, so connection attempts from other machines will fail.
- **mDNS limitations**: mDNS may be blocked by the Windows Firewall by default. If peer discovery is not working, check that your firewall allows mDNS traffic (UDP port 5353).
- **Terminal (UTF-8)**: The TUI uses UTF-8 characters (borders, icons, status indicators). Ensure your terminal emulator is set to UTF-8 encoding. Windows Terminal and recent versions of PowerShell support this out of the box; the legacy Command Prompt (`cmd.exe`) may display garbled characters.

### Linux / macOS

- The TCP listener binds to `0.0.0.0` (all interfaces), so the node is reachable from other machines on the local network as expected.
- mDNS peer discovery works out of the box on most distributions and macOS versions.

## Project Origins

This project was originally inspired by a [LogRocket blog post](https://blog.logrocket.com/libp2p-tutorial-build-a-peer-to-peer-app-in-rust/) and the linked [GitHub repository](https://github.com/zupzup/rust-peer-to-peer-example). However, the codebase has been considerably evolved and modernized since then, and may not resemble the original implementation.

I should also add that I am using this project to test different code assistants and see how they work and how to integrate them into a normal workflow. This represents an ongoing experiment in AI-assisted development and learning.

## Configuration

The application uses a unified network configuration file (`unified_network_config.json`) that consolidates all network-related settings. This file is automatically created with default values when you first run the application.

### Hot-reload vs Restart

Most settings can be updated at runtime using the `reload config` command. A small number of settings are registered with the libp2p swarm at startup and **require a full application restart** to take effect.

| Section | Fields requiring restart | Hot-reloadable fields |
|---------|-------------------------|----------------------|
| `bootstrap` | `bootstrap_peers` | `retry_interval_ms`, `max_retry_attempts`, `bootstrap_timeout_ms` |
| `network` | `max_connections_per_peer`, `max_pending_incoming`, `max_pending_outgoing`, `max_established_total`, `request_timeout_seconds`, `max_concurrent_streams`, `network_health_update_interval_seconds` | all other fields |
| `ping` | `interval_secs`, `timeout_secs` | — |
| `direct_message` | — | all fields |
| `channel_auto_subscription` | — | all fields |
| `message_notifications` | — | all fields |
| `relay` | — | all fields |
| `auto_share` | — | all fields |
| `circuit_breaker` | — | all fields |
| `wasm` | — | all fields |

### Configuration Sections

#### Bootstrap Configuration

Controls how the node connects to the DHT network at startup and how it retries failed connections.

| Field | Default | Valid range | Restart? |
|-------|---------|-------------|---------|
| `bootstrap_peers` | Public libp2p peers | Valid [multiaddr](https://multiformats.io/multiaddr/) strings | **Yes** |
| `retry_interval_ms` | `5000` | 1000–60000 ms | No |
| `max_retry_attempts` | `10` | 1–50 | No |
| `bootstrap_timeout_ms` | `60000` | 5000–120000 ms | No |

- **`bootstrap_peers`** — List of DHT bootstrap peers used to join the Kademlia network. Each entry must be a valid multiaddr including the peer ID component (e.g. `/dnsaddr/…/p2p/Qm…`). Clear this list and add your own peers for a private network.
- **`retry_interval_ms`** — Milliseconds to wait between consecutive attempts to connect to a bootstrap peer after a failure.
- **`max_retry_attempts`** — How many times to retry a failed bootstrap peer before giving up for that session.
- **`bootstrap_timeout_ms`** — Milliseconds to wait for a bootstrap connection to complete before declaring it timed out.

#### Network Configuration

Controls the connection pool, stream limits, and maintenance intervals for the libp2p swarm.

| Field | Default | Valid range | Restart? |
|-------|---------|-------------|---------|
| `connection_maintenance_interval_seconds` | `30` | 10–3600 s | No |
| `request_timeout_seconds` | `120` | 10–300 s | No |
| `max_concurrent_streams` | `100` | 1–1000 | No |
| `max_connections_per_peer` | `1` | 1–10 | **Yes** |
| `max_pending_incoming` | `10` | ≥ 1 | **Yes** |
| `max_pending_outgoing` | `10` | ≥ 1 | **Yes** |
| `max_established_total` | `100` | ≥ 1 | **Yes** |
| `connection_establishment_timeout_seconds` | `30` | 5–300 s | No |
| `network_health_update_interval_seconds` | `15` | ≥ 1 s | No |

- **`connection_maintenance_interval_seconds`** — How often (in seconds) the swarm performs housekeeping tasks such as pruning stale connections. **Note:** In the current implementation this interval is fixed at 30 seconds and the configuration value is ignored; changing it in the config file has no effect yet.
- **`request_timeout_seconds`** — Maximum time to wait for a remote peer to respond to a network request before the request is considered failed.
- **`max_concurrent_streams`** — Maximum number of simultaneous protocol streams that may be open over a single connection.
- **`max_connections_per_peer`** — Hard cap on how many connections can exist to the same remote peer. Keeping this at `1` prevents resource waste.
- **`max_pending_incoming`** — Maximum number of inbound connections that are being established but not yet fully negotiated.
- **`max_pending_outgoing`** — Maximum number of outbound connections that are being dialled but not yet fully established.
- **`max_established_total`** — Total connection pool size across all peers. Increase for high-traffic nodes; reduce on resource-constrained hardware.
- **`connection_establishment_timeout_seconds`** — Seconds to wait for a TCP/QUIC handshake to complete before aborting the attempt.
- **`network_health_update_interval_seconds`** — How often the application refreshes internal network health metrics.

#### Ping Configuration

Controls the keep-alive ping protocol that detects silent connection drops.

| Field | Default | Valid range | Restart? |
|-------|---------|-------------|---------|
| `interval_secs` | `30` | ≥ 1 s (must be > `timeout_secs`) | **Yes** |
| `timeout_secs` | `20` | ≥ 1 s (must be < `interval_secs`) | **Yes** |

- **`interval_secs`** — Seconds between successive pings to each connected peer. Lower values detect dropped connections sooner but increase bandwidth usage. Must be strictly greater than `timeout_secs`.
- **`timeout_secs`** — Seconds to wait for a ping reply before treating the peer as unreachable and closing the connection. Must be strictly less than `interval_secs`.

The defaults (30 s / 20 s) are intentionally more conservative than libp2p's built-in defaults (15 s / 10 s) to tolerate temporary network hiccups without dropping otherwise healthy connections.

#### Direct Message Configuration

Controls retry behaviour for peer-to-peer direct messages that fail to deliver.

| Field | Default | Valid range | Restart? |
|-------|---------|-------------|---------|
| `max_retry_attempts` | `3` | ≥ 1 | No |
| `retry_interval_seconds` | `30` | ≥ 1 s | No |
| `enable_connection_retries` | `true` | boolean | No |
| `enable_timed_retries` | `true` | boolean | No |

- **`max_retry_attempts`** — How many times to re-send a direct message that could not be delivered before giving up.
- **`retry_interval_seconds`** — Seconds between successive re-send attempts.
- **`enable_connection_retries`** — When `true`, the application automatically re-attempts delivery whenever the target peer reconnects.
- **`enable_timed_retries`** — When `true`, the application periodically re-attempts delivery on a timer (`retry_interval_seconds`) regardless of connection events.

#### Channel Auto-Subscription Configuration

Controls automatic subscription behaviour when new channels are discovered on the network.

| Field | Default | Valid range | Restart? |
|-------|---------|-------------|---------|
| `auto_subscribe_to_new_channels` | `false` | boolean | No |
| `notify_new_channels` | `true` | boolean | No |
| `max_auto_subscriptions` | `10` | 1–100 | No |

- **`auto_subscribe_to_new_channels`** — When `true`, the node automatically subscribes to every new channel it discovers. Keep `false` on public networks to avoid subscribing to unwanted content.
- **`notify_new_channels`** — When `true`, a log message is shown in the TUI whenever a new channel is discovered, even if auto-subscribe is disabled.
- **`max_auto_subscriptions`** — Maximum number of channels that can be auto-subscribed. Acts as a safety limit against channel spam.

#### Message Notifications Configuration

Controls visual and audio notification behaviour in the TUI.

| Field | Default | Valid range | Restart? |
|-------|---------|-------------|---------|
| `enable_color_coding` | `true` | boolean | No |
| `enable_sound_notifications` | `false` | boolean | No |
| `enable_flash_indicators` | `true` | boolean | No |
| `flash_duration_ms` | `200` | 1–5000 ms | No |
| `show_delivery_status` | `true` | boolean | No |
| `enhanced_timestamps` | `true` | boolean | No |

- **`enable_color_coding`** — When `true`, unread conversations are highlighted in the TUI using colour (magenta by default).
- **`enable_sound_notifications`** — When `true`, an audio alert is played on new incoming messages. Disabled by default; requires terminal audio support.
- **`enable_flash_indicators`** — When `true`, the status bar briefly flashes when a new message arrives.
- **`flash_duration_ms`** — How long (in milliseconds) the status bar flash lasts. Valid range: 1–5000 ms.
- **`show_delivery_status`** — When `true`, outgoing messages show a checkmark (✓) once delivery is confirmed.
- **`enhanced_timestamps`** — When `true`, message timestamps are shown as relative time (e.g. "5m ago", "Mon 14:30") instead of absolute UTC values.

#### Relay Configuration

Controls the message relay system, which forwards messages through intermediate peers to reach offline or NAT-restricted nodes.

| Field | Default | Valid range | Restart? |
|-------|---------|-------------|---------|
| `enable_relay` | `true` | boolean | No |
| `enable_forwarding` | `true` | boolean | No |
| `max_hops` | `3` | 1–10 | No |
| `relay_timeout_ms` | `5000` | ≥ 1 ms | No |
| `prefer_direct` | `true` | boolean | No |
| `rate_limit_per_peer` | `10` | ≥ 1 | No |

- **`enable_relay`** — Master switch for the relay system. When `false`, this node will neither relay messages for others nor use relays itself.
- **`enable_forwarding`** — When `true`, this node will forward relay messages on behalf of other peers. Disable if you want to receive relayed messages but not act as a relay node.
- **`max_hops`** — Maximum number of relay hops a message may traverse before being dropped. Higher values improve reachability but increase network overhead. Must be 1–10.
- **`relay_timeout_ms`** — Milliseconds to wait for a relay acknowledgement before the attempt is considered failed.
- **`prefer_direct`** — When `true`, the application always attempts a direct connection first and only falls back to relay if the direct attempt fails.
- **`rate_limit_per_peer`** — Maximum number of relay messages accepted from a single peer per time window. Prevents relay spam.

#### Auto-Share Configuration

Controls automatic story synchronisation with newly connected peers.

| Field | Default | Valid range | Restart? |
|-------|---------|-------------|---------|
| `global_auto_share` | `true` | boolean | No |
| `sync_days` | `30` | 1–365 | No |

- **`global_auto_share`** — When `true`, the node automatically shares all public stories with peers when they connect. Set to `false` to require manual publishing via `publish s <id>`.
- **`sync_days`** — How many days back to look when auto-sharing stories. Only stories created within this window are included in the automatic sync. Valid range: 1–365 days.

#### Circuit Breaker Configuration

The circuit breaker prevents repeated attempts to contact unresponsive peers, protecting the node from wasting resources on dead connections.

| Field | Default | Valid range | Restart? |
|-------|---------|-------------|---------|
| `failure_threshold` | `5` | ≥ 1 | No |
| `success_threshold` | `3` | ≥ 1 | No |
| `timeout_secs` | `60` | 1–300 s | No |
| `operation_timeout_secs` | `30` | 1–120 s | No |
| `enabled` | `true` | boolean | No |

- **`failure_threshold`** — Number of consecutive failures required to *open* the circuit breaker (stop trying the peer).
- **`success_threshold`** — Number of consecutive successes required to *close* the circuit breaker again (resume normal operation) after it has been opened.
- **`timeout_secs`** — Seconds the circuit breaker stays open before allowing a single probe attempt to test whether the peer has recovered. Valid range: 1–300 s.
- **`operation_timeout_secs`** — Maximum seconds a single network operation may take before it is counted as a failure. Valid range: 1–120 s.
- **`enabled`** — Master switch for the circuit breaker. Set to `false` to disable circuit breaking entirely (not recommended for production).

#### WASM Configuration

Controls resource limits for executing WebAssembly modules on this node.

| Field | Default | Valid range | Restart? |
|-------|---------|-------------|---------|
| `default_fuel_limit` | `10000000` | ≥ 1 | No |
| `default_memory_limit_mb` | `64` | ≥ 1 (≤ `max_memory_limit_mb`) | No |
| `max_memory_limit_mb` | `1024` | ≥ `default_memory_limit_mb` | No |
| `default_timeout_secs` | `30` | ≥ 1 s | No |

- **`default_fuel_limit`** — Computational budget (instruction count) for a single WASM execution. Execution is halted once this limit is reached, preventing runaway modules.
- **`default_memory_limit_mb`** — Default memory allocation for a WASM module (in MB).
- **`max_memory_limit_mb`** — Hard upper bound on memory any WASM module may request (in MB). A module requesting more than this is rejected.
- **`default_timeout_secs`** — Wall-clock execution timeout for a WASM module. The module is forcibly terminated if it exceeds this duration.

##### WASM Capability Sub-section (`wasm.capability`)

Controls peer-to-peer WASM capability advertisement and remote execution.

| Field | Default | Valid range | Restart? |
|-------|---------|-------------|---------|
| `advertise_capabilities` | `false` | boolean | No |
| `allow_remote_execution` | `false` | boolean | No |
| `max_offerings` | `10` | 1–100 | No |
| `cache_remote_offerings` | `true` | boolean | No |
| `discovery_interval_secs` | `300` | ≥ 60 s | No |
| `max_concurrent_executions` | `2` | 1–10 | No |

- **`advertise_capabilities`** — When `true`, this node broadcasts its WASM offerings to other peers so they can discover and invoke them.
- **`allow_remote_execution`** — When `true`, remote peers are allowed to request execution of WASM modules hosted on this node. Keep `false` unless you explicitly want to offer compute to the network.
- **`max_offerings`** — Maximum number of WASM modules this node may advertise. Valid range: 1–100.
- **`cache_remote_offerings`** — When `true`, offerings discovered from other peers are cached locally to reduce repeated discovery traffic.
- **`discovery_interval_secs`** — How often (in seconds) to re-discover WASM offerings from connected peers. Minimum 60 s.
- **`max_concurrent_executions`** — Maximum number of WASM modules that may execute simultaneously on this node. Valid range: 1–10.

### Deployment Profiles

The following example configurations are provided as starting points. Copy the relevant block into your `unified_network_config.json` and adjust as needed.

#### Local Testing

Optimised for rapid iteration on a single machine or loopback network. Uses short timeouts, disables relay and WASM execution. Bootstrap attempts will fail quickly (which is fine — mDNS handles local peer discovery automatically).

```json
{
  "bootstrap": {
    "bootstrap_peers": [
      "/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN"
    ],
    "retry_interval_ms": 1000,
    "max_retry_attempts": 3,
    "bootstrap_timeout_ms": 5000
  },
  "network": {
    "connection_maintenance_interval_seconds": 30,
    "request_timeout_seconds": 10,
    "max_concurrent_streams": 20,
    "max_connections_per_peer": 1,
    "max_pending_incoming": 5,
    "max_pending_outgoing": 5,
    "max_established_total": 20,
    "connection_establishment_timeout_seconds": 10,
    "network_health_update_interval_seconds": 5
  },
  "ping": {
    "interval_secs": 10,
    "timeout_secs": 5
  },
  "direct_message": {
    "max_retry_attempts": 1,
    "retry_interval_seconds": 5,
    "enable_connection_retries": true,
    "enable_timed_retries": false
  },
  "channel_auto_subscription": {
    "auto_subscribe_to_new_channels": true,
    "notify_new_channels": true,
    "max_auto_subscriptions": 100
  },
  "message_notifications": {
    "enable_color_coding": true,
    "enable_sound_notifications": false,
    "enable_flash_indicators": true,
    "flash_duration_ms": 100,
    "show_delivery_status": true,
    "enhanced_timestamps": true
  },
  "relay": {
    "enable_relay": false,
    "enable_forwarding": false,
    "max_hops": 1,
    "relay_timeout_ms": 1000,
    "prefer_direct": true,
    "rate_limit_per_peer": 100
  },
  "auto_share": {
    "global_auto_share": true,
    "sync_days": 1
  },
  "circuit_breaker": {
    "failure_threshold": 2,
    "success_threshold": 1,
    "timeout_secs": 10,
    "operation_timeout_secs": 5,
    "enabled": true
  },
  "wasm": {
    "default_fuel_limit": 1000000,
    "default_memory_limit_mb": 16,
    "max_memory_limit_mb": 64,
    "default_timeout_secs": 10,
    "capability": {
      "advertise_capabilities": false,
      "allow_remote_execution": false,
      "max_offerings": 5,
      "cache_remote_offerings": false,
      "discovery_interval_secs": 60,
      "max_concurrent_executions": 1
    }
  }
}
```

#### LAN Deployment

Suitable for a local area network where all peers are on the same subnet. Relies primarily on mDNS for peer discovery; the single bootstrap entry satisfies the config validator but connection failures are expected and non-fatal. Moderate timeouts suit reliable LAN speeds.

```json
{
  "bootstrap": {
    "bootstrap_peers": [
      "/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN"
    ],
    "retry_interval_ms": 3000,
    "max_retry_attempts": 5,
    "bootstrap_timeout_ms": 15000
  },
  "network": {
    "connection_maintenance_interval_seconds": 120,
    "request_timeout_seconds": 30,
    "max_concurrent_streams": 50,
    "max_connections_per_peer": 1,
    "max_pending_incoming": 10,
    "max_pending_outgoing": 10,
    "max_established_total": 50,
    "connection_establishment_timeout_seconds": 15,
    "network_health_update_interval_seconds": 10
  },
  "ping": {
    "interval_secs": 20,
    "timeout_secs": 10
  },
  "direct_message": {
    "max_retry_attempts": 3,
    "retry_interval_seconds": 15,
    "enable_connection_retries": true,
    "enable_timed_retries": true
  },
  "channel_auto_subscription": {
    "auto_subscribe_to_new_channels": false,
    "notify_new_channels": true,
    "max_auto_subscriptions": 20
  },
  "message_notifications": {
    "enable_color_coding": true,
    "enable_sound_notifications": false,
    "enable_flash_indicators": true,
    "flash_duration_ms": 200,
    "show_delivery_status": true,
    "enhanced_timestamps": true
  },
  "relay": {
    "enable_relay": true,
    "enable_forwarding": true,
    "max_hops": 2,
    "relay_timeout_ms": 3000,
    "prefer_direct": true,
    "rate_limit_per_peer": 20
  },
  "auto_share": {
    "global_auto_share": true,
    "sync_days": 7
  },
  "circuit_breaker": {
    "failure_threshold": 3,
    "success_threshold": 2,
    "timeout_secs": 30,
    "operation_timeout_secs": 15,
    "enabled": true
  },
  "wasm": {
    "default_fuel_limit": 5000000,
    "default_memory_limit_mb": 32,
    "max_memory_limit_mb": 256,
    "default_timeout_secs": 20,
    "capability": {
      "advertise_capabilities": false,
      "allow_remote_execution": false,
      "max_offerings": 10,
      "cache_remote_offerings": true,
      "discovery_interval_secs": 120,
      "max_concurrent_executions": 2
    }
  }
}
```

#### Internet-Facing Node

Configured for a publicly reachable node that connects to the global libp2p DHT. Uses longer timeouts to tolerate variable internet latency, a larger connection pool, and full relay support for peers behind NAT.

```json
{
  "bootstrap": {
    "bootstrap_peers": [
      "/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
      "/dnsaddr/bootstrap.libp2p.io/p2p/QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa"
    ],
    "retry_interval_ms": 10000,
    "max_retry_attempts": 20,
    "bootstrap_timeout_ms": 60000
  },
  "network": {
    "connection_maintenance_interval_seconds": 300,
    "request_timeout_seconds": 120,
    "max_concurrent_streams": 200,
    "max_connections_per_peer": 1,
    "max_pending_incoming": 20,
    "max_pending_outgoing": 20,
    "max_established_total": 200,
    "connection_establishment_timeout_seconds": 45,
    "network_health_update_interval_seconds": 15
  },
  "ping": {
    "interval_secs": 30,
    "timeout_secs": 20
  },
  "direct_message": {
    "max_retry_attempts": 5,
    "retry_interval_seconds": 60,
    "enable_connection_retries": true,
    "enable_timed_retries": true
  },
  "channel_auto_subscription": {
    "auto_subscribe_to_new_channels": false,
    "notify_new_channels": true,
    "max_auto_subscriptions": 10
  },
  "message_notifications": {
    "enable_color_coding": true,
    "enable_sound_notifications": false,
    "enable_flash_indicators": true,
    "flash_duration_ms": 200,
    "show_delivery_status": true,
    "enhanced_timestamps": true
  },
  "relay": {
    "enable_relay": true,
    "enable_forwarding": true,
    "max_hops": 3,
    "relay_timeout_ms": 10000,
    "prefer_direct": true,
    "rate_limit_per_peer": 10
  },
  "auto_share": {
    "global_auto_share": true,
    "sync_days": 30
  },
  "circuit_breaker": {
    "failure_threshold": 5,
    "success_threshold": 3,
    "timeout_secs": 120,
    "operation_timeout_secs": 60,
    "enabled": true
  },
  "wasm": {
    "default_fuel_limit": 10000000,
    "default_memory_limit_mb": 64,
    "max_memory_limit_mb": 1024,
    "default_timeout_secs": 30,
    "capability": {
      "advertise_capabilities": false,
      "allow_remote_execution": false,
      "max_offerings": 10,
      "cache_remote_offerings": true,
      "discovery_interval_secs": 300,
      "max_concurrent_executions": 2
    }
  }
}
```

### Legacy Configuration Files

For backward compatibility, the application still recognizes individual configuration files:
- `bootstrap_config.json`
- `network_config.json` 
- `ping_config.json`
- `direct_message_config.json`

However, the unified configuration file (`unified_network_config.json`) takes precedence and is the recommended approach.
