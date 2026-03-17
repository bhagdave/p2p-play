# Common Workflows

This guide covers the most common ways to use P2P-Play, from local testing on a single machine through to connecting peers across the internet.

## Contents

1. [Local Testing — Two Instances on One Machine](#1-local-testing--two-instances-on-one-machine)
2. [LAN Deployment — Two Machines on the Same Network](#2-lan-deployment--two-machines-on-the-same-network)
3. [Internet Connectivity — Bootstrap Peers and DHT](#3-internet-connectivity--bootstrap-peers-and-dht)
4. [Channel Workflow — Create, Subscribe, and Publish](#4-channel-workflow--create-subscribe-and-publish)

---

## 1. Local Testing — Two Instances on One Machine

This is the easiest way to verify that p2p-play is working. Two application instances run in separate terminals on the same machine and discover each other automatically using **mDNS** (multicast DNS). No configuration is required.

### How mDNS works here

mDNS broadcasts a discovery announcement over the local loopback/LAN interface. Both instances will see each other's announcements and automatically establish a connection. This only works when both instances are on the same host or within the same broadcast domain (local network segment).

### Steps

**Terminal 1 — Instance A**

```bash
# Run from the project directory
cargo run
```

Once the TUI loads, set a name so the other instance can identify this peer:

```
name Alice
```

You should see log output similar to:

```
Local node is listening on /ip4/127.0.0.1/tcp/<PORT>
Local node is listening on /ip4/<LAN_IP>/tcp/<PORT>
```

**Terminal 2 — Instance B**

Open a second terminal in the same directory, then run the application. The two instances must use separate working directories so their configuration files and databases do not conflict:

```bash
# From the project directory (where Cargo.toml lives)
# Create a separate directory for the second instance next to the project
mkdir -p ../p2p-instance-b
cp unified_network_config.json.example ../p2p-instance-b/unified_network_config.json
cd ../p2p-instance-b
cargo run --manifest-path ../p2p-play/Cargo.toml
```

Or, on Windows using PowerShell:

```powershell
# From the project directory (where Cargo.toml lives)
# Create a separate directory for the second instance next to the project
New-Item -ItemType Directory -Path ..\p2p-instance-b -Force | Out-Null
Copy-Item -Path .\unified_network_config.json.example -Destination ..\p2p-instance-b\unified_network_config.json -Force
Set-Location ..\p2p-instance-b
cargo run --manifest-path ..\p2p-play\Cargo.toml
```

Set a name for this instance:

```
name Bob
```

### Confirming Discovery

After a few seconds, both instances should discover each other via mDNS. To confirm:

```
ls p
```

Expected output includes Alice's or Bob's peer ID in the discovered-peers list.

```
ls c
```

Expected output shows the other peer under connected peers.

### Sharing a Story

On **Instance A** (Alice), create a story:

```
create s
```

Follow the interactive prompts:

```
Story name: My First Story
Story header: Hello from Alice
Story body: This story was shared over P2P!
Channel (leave blank for general): general
```

After creation, publish the story to the network:

```
ls s
```

Note the story ID shown, then publish:

```
publish s <story_id>
```

On **Instance B** (Bob), request all available stories:

```
ls s all
```

Bob's instance should display the story published by Alice.

---

## 2. LAN Deployment — Two Machines on the Same Network

When running on two separate machines connected to the same LAN, mDNS peer discovery still works automatically as long as both machines are on the same network segment (subnet) and mDNS/multicast traffic is not blocked by a firewall or router.

### Steps

Follow the same steps as [Workflow 1](#1-local-testing--two-instances-on-one-machine), but:

- Run `cargo run` on **Machine A** and on **Machine B** independently.
- Each machine has its own working directory, configuration files, and `stories.db`.

### Manual Connection (fallback)

If mDNS is blocked (e.g., corporate networks, some Wi-Fi configurations), note the listening address logged by Instance A:

```
Local node is listening on /ip4/192.168.1.10/tcp/PORT
```

On **Machine B**, use the `connect` command to dial Machine A directly:

```
connect /ip4/192.168.1.10/tcp/PORT
```

> **Optional peer ID suffix**: The `connect` command accepts both plain transport addresses like `/ip4/192.168.1.10/tcp/PORT` and full peer multiaddrs like `/ip4/192.168.1.10/tcp/PORT/p2p/<PEER_ID_OF_A>`. The `/p2p/<PEER_ID_OF_A>` suffix is optional.
> 
> **Finding the Peer ID**: In the TUI, your local peer ID is shown in the status bar. You can also run the `peer id` command from the input prompt to print it. The peer ID looks like `12D3KooW...`.

After connecting, proceed with `ls p`, `ls c`, `create s`, `publish s <id>`, and `ls s all` as described in Workflow 1.

### Confirming Cross-Machine Discovery

```
ls c
```

Both peers should appear as connected. Stories published by one peer will be visible on the other after `ls s all`.

---

## 3. Internet Connectivity — Bootstrap Peers and DHT

Connecting across the internet requires peers to be reachable at a public IP address and to use the **Kademlia DHT** for peer discovery. mDNS does not work across the internet.

### Prerequisites

- At least one peer (the *bootstrap peer*) must have a publicly reachable IP address and an open TCP port.
- All nodes should have the bootstrap peer's address in their `unified_network_config.json`.

### Step 1 — Identify the Bootstrap Peer's Address

On the machine that will act as the bootstrap peer, start p2p-play and look at the listen address it prints on startup. It will typically look like this on non-Windows systems:

```
Local node is listening on /ip4/0.0.0.0/tcp/4001
```

(On Windows you might see `127.0.0.1` instead of `0.0.0.0`.) This is the **bind address**, not your public internet address, and it is usually **not** directly reachable from other machines on the internet.

To act as a public bootstrap peer:

- Determine your machine's **public IP address or DNS name** (for example from your hosting provider or router configuration).
- Ensure the TCP port used by p2p-play (e.g. `4001`) is **forwarded/open** on your router and firewall.
- Combine that public IP/DNS and port with your peer ID to form the public bootstrap multiaddr.

The bootstrap peer's full public multiaddr then looks like:

```
/ip4/203.0.113.42/tcp/4001/p2p/12D3KooWExamplePeerID...
```

### Step 2 — Configure Bootstrap Peers

On **each** other machine, after you have run `cargo run` once (so that `unified_network_config.json` is generated), edit the existing `unified_network_config.json` in the working directory and add the bootstrap peer's address to the `bootstrap.bootstrap_peers` list.

In the `bootstrap` section of that file, you should have something like:

```json
{
  "bootstrap_peers": [
    "/ip4/203.0.113.42/tcp/4001/p2p/12D3KooWExamplePeerID..."
  ],
  "retry_interval_ms": 5000,
  "max_retry_attempts": 5,
  "bootstrap_timeout_ms": 30000
}
```

See `unified_network_config.json.example` for a complete example with all configuration sections. Do **not** remove other top-level sections (`network`, `ping`, `direct_message`, etc.) from your config file.

Alternatively, you can add a bootstrap peer without manually editing the file by running:

```text
dht bootstrap add /ip4/203.0.113.42/tcp/4001/p2p/12D3KooWExamplePeerID...
```

This command updates `unified_network_config.json` for you.

### Step 3 — Start and Bootstrap

Start each instance with `cargo run`. The application will automatically attempt to connect to the configured bootstrap peers on startup and log status to `bootstrap.log`:

```
[BOOTSTRAP_ATTEMPT] Connecting to bootstrap peer: /ip4/...
[BOOTSTRAP_STATUS] DHT connected peers: 1
```

You can also manually trigger bootstrapping at runtime with the `dht bootstrap` command:

```
dht bootstrap /ip4/203.0.113.42/tcp/4001/p2p/12D3KooWExamplePeerID...
```

### Step 4 — Discover Peers via DHT

Once connected to the bootstrap peer, search the DHT for other nearby nodes:

```
dht peers
```

Then check your connected peers:

```
ls c
```

### Step 5 — Share Stories Across the Internet

Once peers are connected, story sharing works the same as in the local workflows:

```
create s
publish s <story_id>
```

Other connected peers can request stories with:

```
ls s all
```

### Firewall Notes

- Open the TCP port that p2p-play listens on (shown at startup) in the machine's firewall and any NAT/port-forwarding rules on the router.
- Connection failures to bootstrap peers are logged to `bootstrap.log` and `errors.log` — check these files for troubleshooting.
- In sandboxed or restricted network environments, connections to public bootstrap peers may fail. This is normal; use a known reachable peer instead.

---

## 4. Channel Workflow — Create, Subscribe, and Publish

Channels let you organize stories by topic. Peers can subscribe to specific channels and only receive stories published there.

All peers are automatically subscribed to the **general** channel when they start.

### Creating a Channel

```
create ch my-channel|A channel for sharing short stories
```

The format is `<channel-name>|<description>`. The channel is broadcast to other connected peers.

### Listing Available Channels

```
ls ch
```

To see channels you have not yet subscribed to:

```
ls ch unsubscribed
```

### Subscribing to a Channel

```
sub my-channel
```

You will now receive stories published to `my-channel` from any peer.

### Publishing a Story to a Channel

When creating a story, specify the channel name at the prompt:

```
create s
```

```
Story name: Weekend Trip
Story header: A great adventure
Story body: We hiked ten miles and saw amazing views.
Channel (leave blank for general): my-channel
```

After creation, publish the story:

```
publish s <story_id>
```

### Unsubscribing from a Channel

```
unsub my-channel
```

You will no longer receive new stories published to that channel.

### Expected Output

After another peer subscribes to `my-channel` and you run `ls s all`, you should see their stories appear under the `my-channel` heading. Stories from channels you are not subscribed to will not be shown.

---

## Quick Reference

| Goal | Command |
|---|---|
| Set your display name | `name <alias>` |
| List discovered peers | `ls p` |
| List connected peers | `ls c` |
| Create a story | `create s` |
| List your local stories | `ls s local` |
| Publish a story | `publish s <story_id>` |
| Request stories from peers | `ls s all` |
| Connect to a specific peer | `connect <multiaddr>` |
| Bootstrap DHT from a peer | `dht bootstrap <multiaddr>` |
| Create a channel | `create ch <name>\|<description>` |
| List channels | `ls ch` |
| Subscribe to a channel | `sub <channel>` |
| Unsubscribe from a channel | `unsub <channel>` |
| Send a direct message | `msg <peer_alias> <message>` |
| Show all commands | `help` |

## Log Files

| File | Contents |
|---|---|
| `bootstrap.log` | DHT bootstrap connection attempts and status |
| `errors.log` | Network errors and connection issues |
| `stories.db` | SQLite database of local stories |
| `unified_network_config.json` | All network settings |
| `peer_key` | Persistent peer identity (keep private) |
