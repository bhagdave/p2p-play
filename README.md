# p2p-play
This is a toy project working with the [libp2p library](https://github.com/libp2p/rust-libp2p) in Rust

This project was originally taken from the [log rocket blog post](https://blog.logrocket.com/libp2p-tutorial-build-a-peer-to-peer-app-in-rust/) and the linked [Github Repo](https://github.com/zupzup/rust-peer-to-peer-example).  However, I have considerably changed it since then so it may not resemble the original.

The basic functionality is to share stories amongst peers in a peer to peer network.  This has to be done via the command line running specific commands.  You can run through the code to see what commands are available.

I have also changed the code so that it works with the latest version of the [libp2p library (0.56.0)](https://github.com/libp2p/rust-libp2p/releases/tag/v0.56.0).

I should also add that I am using this project to test different code assistants and see how they work and how to integrate them into a normal workflow.

## Configuration

The application supports several configuration files to customize network behavior:

### Ping Configuration (`ping_config.json`)

Configure ping keep-alive settings to improve connection stability:

```json
{
  "interval_secs": 30,
  "timeout_secs": 20
}
```

- `interval_secs`: How often to send ping messages (default: 30 seconds)
- `timeout_secs`: How long to wait for ping responses (default: 20 seconds)

If the file doesn't exist, the application uses the default lenient settings shown above. These are more conservative than libp2p's defaults (15s interval, 10s timeout) to reduce connection drops on temporary network hiccups.

### Bootstrap Configuration (`bootstrap_config.json`)

Configure DHT bootstrap peers and retry behavior (existing functionality).

### Direct Message Configuration (`direct_message_config.json`)

Configure direct message retry logic and delivery behavior (existing functionality).
