# p2p-play
This is a toy project working with the [libp2p library](https://github.com/libp2p/rust-libp2p) in Rust

This project was originally taken from the [log rocket blog post](https://blog.logrocket.com/libp2p-tutorial-build-a-peer-to-peer-app-in-rust/) and the linked [Github Repo](https://github.com/zupzup/rust-peer-to-peer-example).  However, I have considerably changed it since then so it may not resemble the original.

The basic functionality is to share stories amongst peers in a peer to peer network.  This has to be done via the command line running specific commands.  You can run through the code to see what commands are available.

I have also changed the code so that it works with the latest version of the [libp2p library (0.56.0)](https://github.com/libp2p/rust-libp2p/releases/tag/v0.56.0).

I should also add that I am using this project to test different code assistants and see how they work and how to integrate them into a normal workflow.

## Node Descriptions

The application now supports optional node descriptions that can be shared between peers:

- `create desc <description>` - Create a description for your node (max 1024 bytes)
- `show desc` - Display your current description 
- `get desc <peer_alias>` - Request description from a connected peer

**Note**: All nodes in a network must be running the same version to use node descriptions. If you see "UnsupportedProtocols" errors when requesting descriptions, it means the other peer is running an older version without this feature.


