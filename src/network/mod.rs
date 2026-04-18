//! Network layer: libp2p swarm construction and behaviour wiring.
//!
//! Protocol wire types live in the [`protocol`] submodule and are
//! re-exported here so all callers can continue to use
//! `crate::network::DirectMessageRequest`, etc. unchanged.

pub mod protocol;
pub use protocol::*;

use crate::errors::NetworkResult;
use crate::types::NetworkConfig;
use crate::types::PingConfig;
use libp2p::floodsub::{Behaviour, Event, Topic};
use libp2p::swarm::{NetworkBehaviour, Swarm};
use libp2p::{PeerId, StreamProtocol, identity, kad, mdns, ping, request_response};
use log::warn;
use once_cell::sync::Lazy;
use std::fs;
use std::iter;

// ── Transport / swarm tuning constants ───────────────────────────────────────

/// TCP listen-socket backlog (pending-connection queue depth).
const TCP_LISTEN_BACKLOG: u32 = 1024;
/// IP time-to-live set on outgoing TCP segments.
const TCP_TTL: u32 = 64;
/// Maximum concurrent multiplexed streams per yamux session.
const YAMUX_MAX_STREAMS: usize = 512;
/// Seconds an idle connection is kept open before the swarm closes it.
const SWARM_IDLE_CONNECTION_TIMEOUT_SECS: u64 = 60;
/// Fallback dial-concurrency factor used when the config value cannot be
/// represented as a `NonZeroU8`.
const SWARM_DIAL_CONCURRENCY_FALLBACK: u8 = 8;

// ── Type aliases ─────────────────────────────────────────────────────────────

/// CBOR request-response behaviour for direct peer messages.
pub type DirectMessageBehaviour =
    request_response::cbor::Behaviour<DirectMessageRequest, DirectMessageResponse>;
/// CBOR request-response behaviour for node-description queries.
pub type NodeDescriptionBehaviour =
    request_response::cbor::Behaviour<NodeDescriptionRequest, NodeDescriptionResponse>;
/// CBOR request-response behaviour for story synchronisation.
pub type StorySyncBehaviour =
    request_response::cbor::Behaviour<StorySyncRequest, StorySyncResponse>;
/// CBOR request-response behaviour for the application handshake.
pub type HandshakeBehaviour =
    request_response::cbor::Behaviour<HandshakeRequest, HandshakeResponse>;
/// CBOR request-response behaviour for WASM capability discovery.
pub type WasmCapabilitiesBehaviour =
    request_response::cbor::Behaviour<WasmCapabilitiesRequest, WasmCapabilitiesResponse>;
/// CBOR request-response behaviour for remote WASM execution.
pub type WasmExecutionBehaviour =
    request_response::cbor::Behaviour<WasmExecutionRequest, WasmExecutionResponse>;

/// Swarm event emitted by the direct-message protocol.
pub type DirectMessageEvent = request_response::Event<DirectMessageRequest, DirectMessageResponse>;
/// Swarm event emitted by the node-description protocol.
pub type NodeDescriptionEvent =
    request_response::Event<NodeDescriptionRequest, NodeDescriptionResponse>;
/// Swarm event emitted by the story-sync protocol.
pub type StorySyncEvent = request_response::Event<StorySyncRequest, StorySyncResponse>;
/// Swarm event emitted by the handshake protocol.
pub type HandshakeEvent = request_response::Event<HandshakeRequest, HandshakeResponse>;
/// Swarm event emitted by the WASM-capabilities protocol.
pub type WasmCapabilitiesEvent =
    request_response::Event<WasmCapabilitiesRequest, WasmCapabilitiesResponse>;
/// Swarm event emitted by the WASM-execution protocol.
pub type WasmExecutionEvent = request_response::Event<WasmExecutionRequest, WasmExecutionResponse>;

// ── Global statics ────────────────────────────────────────────────────────────

pub static KEYS: Lazy<identity::Keypair> = Lazy::new(|| {
    let peer_key_path = crate::data_dir::get_data_path("peer_key");
    match fs::read(&peer_key_path) {
        Ok(bytes) => match identity::Keypair::from_protobuf_encoding(&bytes) {
            Ok(keypair) => keypair,
            Err(e) => {
                warn!("Error loading keypair: {e}, generating new one");
                generate_and_save_keypair(&peer_key_path)
            }
        },
        Err(_) => generate_and_save_keypair(&peer_key_path),
    }
});

pub static PEER_ID: Lazy<PeerId> = Lazy::new(|| PeerId::from(KEYS.public()));
pub static TOPIC: Lazy<Topic> = Lazy::new(|| Topic::new("stories"));
pub static RELAY_TOPIC: Lazy<Topic> = Lazy::new(|| Topic::new("relay"));

// ── Keypair helpers ───────────────────────────────────────────────────────────

/// Log a keypair persistence error to the errors log file.
fn log_keypair_error(message: &str) {
    let errors_log_path = crate::data_dir::get_data_path("errors.log");
    let error_logger = crate::error_logger::ErrorLogger::new(&errors_log_path);
    error_logger.log_error(message);
}

fn generate_and_save_keypair(peer_key_path: &str) -> identity::Keypair {
    let keypair = identity::Keypair::generate_ed25519();
    match keypair.to_protobuf_encoding() {
        Ok(bytes) => {
            if let Err(e) = fs::write(peer_key_path, bytes) {
                log_keypair_error(&format!("Failed to save keypair: {e}"));
            }
        }
        Err(e) => log_keypair_error(&format!("Failed to encode keypair: {e}")),
    }
    keypair
}

// ── NetworkBehaviour ──────────────────────────────────────────────────────────

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "StoryBehaviourEvent")]
pub struct StoryBehaviour {
    pub floodsub: Behaviour,
    pub mdns: mdns::tokio::Behaviour,
    pub ping: ping::Behaviour,
    pub request_response: DirectMessageBehaviour,
    pub node_description: NodeDescriptionBehaviour,
    pub story_sync: StorySyncBehaviour,
    pub handshake: HandshakeBehaviour,
    pub wasm_capabilities: WasmCapabilitiesBehaviour,
    pub wasm_execution: WasmExecutionBehaviour,
    pub kad: kad::Behaviour<kad::store::MemoryStore>,
}

// ── Unified swarm event ───────────────────────────────────────────────────────

#[derive(Debug)]
pub enum StoryBehaviourEvent {
    Floodsub(Event),
    Mdns(mdns::Event),
    Ping(ping::Event),
    RequestResponse(DirectMessageEvent),
    NodeDescription(NodeDescriptionEvent),
    StorySync(StorySyncEvent),
    Handshake(HandshakeEvent),
    WasmCapabilities(WasmCapabilitiesEvent),
    WasmExecution(WasmExecutionEvent),
    Kad(kad::Event),
}

/// Generate `From<SourceEvent> for StoryBehaviourEvent` for every inner event
/// type, eliminating repetitive hand-written boilerplate.
macro_rules! impl_story_event_from {
    ($source:ty => $variant:ident) => {
        impl From<$source> for StoryBehaviourEvent {
            fn from(event: $source) -> Self {
                StoryBehaviourEvent::$variant(event)
            }
        }
    };
}

impl_story_event_from!(Event => Floodsub);
impl_story_event_from!(mdns::Event => Mdns);
impl_story_event_from!(ping::Event => Ping);
impl_story_event_from!(DirectMessageEvent => RequestResponse);
impl_story_event_from!(NodeDescriptionEvent => NodeDescription);
impl_story_event_from!(StorySyncEvent => StorySync);
impl_story_event_from!(HandshakeEvent => Handshake);
impl_story_event_from!(WasmCapabilitiesEvent => WasmCapabilities);
impl_story_event_from!(WasmExecutionEvent => WasmExecution);
impl_story_event_from!(kad::Event => Kad);

// ── Swarm construction ────────────────────────────────────────────────────────

/// Build a CBOR request-response [`Behaviour`][request_response::cbor::Behaviour]
/// for the given `protocol_path`, sharing the common `cfg`.
fn make_cbor_behaviour<Req, Resp>(
    protocol_path: &'static str,
    cfg: request_response::Config,
) -> request_response::cbor::Behaviour<Req, Resp>
where
    Req: serde::Serialize + for<'de> serde::Deserialize<'de> + Send + Clone + 'static,
    Resp: serde::Serialize + for<'de> serde::Deserialize<'de> + Send + Clone + 'static,
{
    request_response::cbor::Behaviour::new(
        iter::once((
            StreamProtocol::new(protocol_path),
            request_response::ProtocolSupport::Full,
        )),
        cfg,
    )
}

/// Construct the authenticated, multiplexed transport stack.
fn build_transport(
    keys: &identity::Keypair,
) -> NetworkResult<libp2p::core::transport::Boxed<(PeerId, libp2p::core::muxing::StreamMuxerBox)>> {
    use libp2p::{Transport, core::upgrade, dns, noise, tcp, yamux};

    #[cfg(windows)]
    let tcp_config = libp2p::tcp::Config::default()
        .nodelay(true)
        .port_reuse(false)
        .listen_backlog(TCP_LISTEN_BACKLOG)
        .ttl(TCP_TTL);

    #[cfg(not(windows))]
    let tcp_config = libp2p::tcp::Config::default()
        .nodelay(true)
        .listen_backlog(TCP_LISTEN_BACKLOG)
        .ttl(TCP_TTL);

    let mut yamux_config = yamux::Config::default();
    yamux_config.set_max_num_streams(YAMUX_MAX_STREAMS);

    let noise_config =
        noise::Config::new(keys).map_err(|e| format!("Failed to create noise config: {e}"))?;

    let transport = dns::tokio::Transport::system(tcp::tokio::Transport::new(tcp_config))
        .map_err(|e| format!("Failed to create DNS transport: {e}"))?
        .upgrade(upgrade::Version::V1)
        .authenticate(noise_config)
        .multiplex(yamux_config)
        .boxed();

    Ok(transport)
}

/// Construct the composite [`StoryBehaviour`] from individual sub-behaviours.
fn build_behaviour(
    ping_config: &PingConfig,
    rr_config: request_response::Config,
) -> NetworkResult<StoryBehaviour> {
    let mdns = mdns::tokio::Behaviour::new(Default::default(), *PEER_ID)
        .map_err(|e| format!("Failed to create mDNS behaviour: {e}"))?;

    let store = kad::store::MemoryStore::new(*PEER_ID);
    let mut kad = kad::Behaviour::with_config(*PEER_ID, store, kad::Config::default());
    kad.set_mode(Some(kad::Mode::Server));

    let mut behaviour = StoryBehaviour {
        floodsub: Behaviour::new(*PEER_ID),
        mdns,
        ping: ping::Behaviour::new(
            ping::Config::new()
                .with_interval(ping_config.interval_duration())
                .with_timeout(ping_config.timeout_duration()),
        ),
        request_response: make_cbor_behaviour("/dm/1.0.0", rr_config.clone()),
        node_description: make_cbor_behaviour("/node-desc/1.0.0", rr_config.clone()),
        story_sync: make_cbor_behaviour("/story-sync/1.0.0", rr_config.clone()),
        handshake: make_cbor_behaviour(APP_PROTOCOL, rr_config.clone()),
        wasm_capabilities: make_cbor_behaviour("/wasm-caps/1.0.0", rr_config.clone()),
        wasm_execution: make_cbor_behaviour("/wasm-exec/1.0.0", rr_config),
        kad,
    };

    behaviour.floodsub.subscribe(TOPIC.clone());
    behaviour.floodsub.subscribe(RELAY_TOPIC.clone());

    Ok(behaviour)
}

/// Construct a [`SwarmConfig`][libp2p::swarm::Config] with sensible settings
/// derived from `max_pending_outgoing`.
fn build_swarm_config(max_pending_outgoing: u32) -> libp2p::swarm::Config {
    use std::num::NonZeroU8;
    use std::time::Duration;

    // Clamp to `u8` range before constructing `NonZeroU8`. Values above 255 are
    // saturated to 255 (the maximum `NonZeroU8` accepts) rather than wrapping to 0
    // and silently falling back to the default.
    let clamped = max_pending_outgoing.min(u8::MAX as u32) as u8;
    let dial_concurrency =
        NonZeroU8::new(clamped).unwrap_or(NonZeroU8::new(SWARM_DIAL_CONCURRENCY_FALLBACK).unwrap());

    libp2p::swarm::Config::with_tokio_executor()
        .with_dial_concurrency_factor(dial_concurrency)
        .with_idle_connection_timeout(Duration::from_secs(SWARM_IDLE_CONNECTION_TIMEOUT_SECS))
}

/// Create a fully configured [`Swarm<StoryBehaviour>`] ready to listen and dial.
pub fn create_swarm(
    ping_config: &PingConfig,
    network_config: &NetworkConfig,
) -> NetworkResult<Swarm<StoryBehaviour>> {
    let transport = build_transport(&KEYS)?;

    let rr_config = request_response::Config::default()
        .with_request_timeout(std::time::Duration::from_secs(
            network_config.request_timeout_seconds,
        ))
        .with_max_concurrent_streams(network_config.max_concurrent_streams);

    let behaviour = build_behaviour(ping_config, rr_config)?;
    let swarm_config = build_swarm_config(network_config.max_pending_outgoing);

    Ok(Swarm::<StoryBehaviour>::new(
        transport,
        behaviour,
        *PEER_ID,
        swarm_config,
    ))
}
