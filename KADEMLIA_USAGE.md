# Kademlia DHT Usage Guide

This guide explains how to use the new Kademlia DHT functionality in P2P-Play to discover peers over the internet.

## Overview

The Kademlia DHT implementation allows P2P-Play nodes to discover and connect to peers across the internet, not just on the local network (which is what mDNS provides). This enables true peer-to-peer story sharing across geographical boundaries.

## New Commands

### `dht bootstrap <multiaddr>`

Bootstrap the DHT by connecting to a known peer. This establishes your node's presence in the DHT network.

**Usage:**
```
dht bootstrap /ip4/172.105.162.14/tcp/4001/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN
```

**What happens:**
1. The node attempts to dial the specified peer
2. Adds the peer to the DHT routing table
3. Starts the bootstrap process to discover more peers
4. The DHT switches to server mode to accept queries from other peers

### `dht peers`

Find the closest peers to your node in the DHT network.

**Usage:**
```
dht peers
```

**What happens:**
1. Initiates a search for peers closest to your peer ID
2. Results are displayed in the log output as they are discovered
3. Found peers may be automatically added to your routing table

## DHT Event Messages

When using the DHT, you'll see various status messages in the output:

- `DHT bootstrap started successfully` - Bootstrap process initiated
- `DHT bootstrap successful with peer: <peer_id>` - Successfully connected to a bootstrap peer
- `New peer added to DHT: <peer_id>` - New peer discovered and added to routing table
- `DHT mode changed to: Server` - Node is now accepting DHT queries
- `DHT peer search started` - Closest peer search initiated

## Example Workflow

1. **Start the application:**
   ```bash
   cargo run
   ```

2. **Set your peer name (optional but recommended):**
   ```
   name YourAlias
   ```

3. **Bootstrap with a known peer:**
   ```
   dht bootstrap /ip4/KNOWN_PEER_IP/tcp/PORT/p2p/PEER_ID
   ```

4. **Search for nearby peers:**
   ```
   dht peers
   ```

5. **Check discovered peers:**
   ```
   ls p
   ```

6. **Check connected peers:**
   ```
   ls c
   ```

## Integration with Existing Features

- **Story Sharing**: Once DHT peers are discovered and connected, they automatically participate in story sharing via floodsub
- **Direct Messaging**: You can send direct messages to DHT-discovered peers using their peer aliases
- **Automatic Peer Management**: DHT-discovered peers are automatically added to the floodsub network for story broadcasting

## Technical Details

- **Storage**: Uses in-memory storage for DHT records (restarts fresh each time)
- **Mode**: Automatically runs in server mode to accept queries and provide records
- **Routing**: Maintains a k-bucket routing table for efficient peer discovery
- **Bootstrap**: Supports bootstrapping from any libp2p-compatible peer with Kademlia enabled

## Compatibility

The DHT implementation is fully compatible with:
- Existing mDNS local peer discovery
- Floodsub story broadcasting
- Direct messaging via request-response protocol
- All existing P2P-Play features

Both local (mDNS) and internet (DHT) peer discovery work simultaneously, providing seamless connectivity across local networks and the broader internet.