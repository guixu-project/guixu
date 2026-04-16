// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use libp2p::{
    autonat, dcutr,
    futures::StreamExt,
    gossipsub, identify, kad,
    mdns::tokio::Behaviour as Mdns,
    noise, relay,
    request_response::{self, ProtocolSupport},
    swarm::{behaviour::toggle::Toggle, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, StreamProtocol,
};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use data_core::config::NodeConfig;
use data_core::types::{SampleRequest, SampleResponse};

pub mod codec;

/// Events emitted by the P2P network layer to upper layers.
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    NewMetadata(Vec<u8>),
    PeerConnected(PeerId),
    SampleResponse {
        request_id: String,
        response: SampleResponse,
    },
    IncomingSampleRequest {
        peer: PeerId,
        channel_id: u64,
        request: SampleRequest,
    },
    IncomingAccessRequest {
        peer: PeerId,
        channel_id: u64,
        request: data_core::types::AccessRequest,
    },
    NatStatusChanged {
        is_public: bool,
    },
}

/// Commands sent from upper layers to the network task.
#[derive(Debug)]
pub enum NetworkCommand {
    DhtPut {
        key: Vec<u8>,
        value: Vec<u8>,
    },
    DhtGet {
        key: Vec<u8>,
        reply: tokio::sync::oneshot::Sender<Option<Vec<u8>>>,
    },
    GossipPublish {
        topic: String,
        data: Vec<u8>,
    },
    Ping {
        reply: tokio::sync::oneshot::Sender<()>,
    },
    SampleRequest {
        peer: PeerId,
        request: SampleRequest,
        reply: tokio::sync::oneshot::Sender<Option<SampleResponse>>,
    },
    SampleResponse {
        channel_id: u64,
        response: SampleResponse,
    },
    AccessRequest {
        peer: PeerId,
        request: data_core::types::AccessRequest,
        reply: tokio::sync::oneshot::Sender<Option<data_core::types::AccessGrant>>,
    },
    AccessResponse {
        channel_id: u64,
        response: data_core::types::AccessGrant,
    },
}

#[derive(libp2p::swarm::NetworkBehaviour)]
struct Behaviour {
    kademlia: kad::Behaviour<kad::store::MemoryStore>,
    gossipsub: gossipsub::Behaviour,
    mdns: Toggle<Mdns>,
    identify: identify::Behaviour,
    relay_client: relay::client::Behaviour,
    autonat: autonat::v2::client::Behaviour,
    dcutr: dcutr::Behaviour,
    sample_protocol: request_response::Behaviour<codec::JsonCodec>,
    access_protocol: request_response::Behaviour<codec::JsonCodec>,
}

pub const DATASETS_TOPIC: &str = "datasets";
pub const SAMPLE_PROTOCOL: &str = "/guixu/sample/1.0.0";
pub const ACCESS_PROTOCOL: &str = "/guixu/access/1.0.0";

/// Handle to the running P2P network. Send commands via `cmd_tx`.
pub struct NetworkHandle {
    pub cmd_tx: mpsc::Sender<NetworkCommand>,
    pub local_peer_id: PeerId,
}

impl NetworkHandle {
    pub async fn dht_put(&self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.cmd_tx
            .send(NetworkCommand::DhtPut { key, value })
            .await?;
        Ok(())
    }

    pub async fn dht_get(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.cmd_tx
            .send(NetworkCommand::DhtGet { key, reply: tx })
            .await?;
        Ok(rx.await?)
    }

    pub async fn gossip_publish(&self, topic: String, data: Vec<u8>) -> Result<()> {
        self.cmd_tx
            .send(NetworkCommand::GossipPublish { topic, data })
            .await?;
        Ok(())
    }

    pub async fn send_sample_request(
        &self,
        peer: PeerId,
        request: SampleRequest,
    ) -> Result<Option<SampleResponse>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.cmd_tx
            .send(NetworkCommand::SampleRequest {
                peer,
                request,
                reply: tx,
            })
            .await?;
        match tokio::time::timeout(Duration::from_secs(15), rx).await {
            Ok(Ok(resp)) => Ok(resp),
            _ => Ok(None),
        }
    }

    pub async fn send_sample_response(
        &self,
        channel_id: u64,
        response: SampleResponse,
    ) -> Result<()> {
        self.cmd_tx
            .send(NetworkCommand::SampleResponse {
                channel_id,
                response,
            })
            .await?;
        Ok(())
    }

    pub async fn send_access_response(
        &self,
        channel_id: u64,
        response: data_core::types::AccessGrant,
    ) -> Result<()> {
        self.cmd_tx
            .send(NetworkCommand::AccessResponse {
                channel_id,
                response,
            })
            .await?;
        Ok(())
    }
}

/// Start the P2P network. Returns a handle and spawns the swarm event loop.
pub async fn start(
    config: &NodeConfig,
    keypair_seed: &[u8; 32],
    event_tx: mpsc::Sender<NetworkEvent>,
) -> Result<NetworkHandle> {
    let id_keypair = libp2p::identity::Keypair::ed25519_from_bytes(keypair_seed.to_vec())
        .map_err(|e| anyhow::anyhow!("keypair from seed: {e}"))?;
    let local_peer_id = id_keypair.public().to_peer_id();
    info!(%local_peer_id, "starting P2P node");

    // Kademlia
    let store = kad::store::MemoryStore::new(local_peer_id);
    let mut kad_config = kad::Config::new(StreamProtocol::new("/data-protocol/kad/1.0.0"));
    kad_config.set_query_timeout(Duration::from_secs(30));
    let kademlia = kad::Behaviour::with_config(local_peer_id, store, kad_config);

    // GossipSub
    let message_id_fn = |message: &gossipsub::Message| {
        let mut s = DefaultHasher::new();
        message.data.hash(&mut s);
        gossipsub::MessageId::from(s.finish().to_string())
    };
    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_secs(10))
        .validation_mode(gossipsub::ValidationMode::Strict)
        .message_id_fn(message_id_fn)
        .build()
        .map_err(|e| anyhow::anyhow!("gossipsub config: {e}"))?;
    let gossipsub = gossipsub::Behaviour::new(
        gossipsub::MessageAuthenticity::Signed(id_keypair.clone()),
        gossipsub_config,
    )
    .map_err(|e| anyhow::anyhow!("gossipsub: {e}"))?;

    // mDNS
    let mdns = if config.disable_mdns {
        info!("mDNS disabled (privacy mode)");
        Toggle::from(None)
    } else {
        Toggle::from(Some(Mdns::new(Default::default(), local_peer_id)?))
    };

    // Identify
    let identify = identify::Behaviour::new(identify::Config::new(
        "/data-protocol/id/1.0.0".into(),
        id_keypair.public(),
    ));

    // Relay client (NAT traversal)
    let (_relay_transport, relay_client) = relay::client::new(local_peer_id);

    // AutoNAT v2 client
    let autonat = autonat::v2::client::Behaviour::new(
        rand::rngs::OsRng,
        autonat::v2::client::Config::default(),
    );

    // DCUtR (Direct Connection Upgrade through Relay)
    let dcutr = dcutr::Behaviour::new(local_peer_id);

    // Sample protocol (/guixu/sample/1.0.0)
    let sample_protocol = request_response::Behaviour::new(
        [(StreamProtocol::new(SAMPLE_PROTOCOL), ProtocolSupport::Full)],
        request_response::Config::default().with_request_timeout(Duration::from_secs(15)),
    );

    // Access protocol (/guixu/access/1.0.0)
    let access_protocol = request_response::Behaviour::new(
        [(StreamProtocol::new(ACCESS_PROTOCOL), ProtocolSupport::Full)],
        request_response::Config::default().with_request_timeout(Duration::from_secs(30)),
    );

    let behaviour = Behaviour {
        kademlia,
        gossipsub,
        mdns,
        identify,
        relay_client,
        autonat,
        dcutr,
        sample_protocol,
        access_protocol,
    };

    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(id_keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_relay_client(noise::Config::new, yamux::Config::default)?
        .with_behaviour(|_key, relay_behaviour| {
            // Replace the relay_client with the one from the transport
            let mut b = behaviour;
            b.relay_client = relay_behaviour;
            Ok(b)
        })
        .map_err(|e| anyhow::anyhow!("behaviour: {e}"))?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    // Subscribe to datasets topic
    let topic = gossipsub::IdentTopic::new(DATASETS_TOPIC);
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

    // Listen
    let listen_addr: Multiaddr = format!("/ip4/0.0.0.0/tcp/{}", config.listen_port).parse()?;
    swarm.listen_on(listen_addr)?;

    // Connect to bootstrap peers
    for addr_str in &config.bootstrap_peers {
        if let Ok(addr) = addr_str.parse::<Multiaddr>() {
            info!(%addr, "dialing bootstrap peer");
            let _ = swarm.dial(addr);
        }
    }

    // Connect to relay servers for NAT traversal
    if config.network.relay_enabled {
        for relay_addr_str in &config.network.relay_servers {
            if let Ok(addr) = relay_addr_str.parse::<Multiaddr>() {
                info!(%addr, "dialing relay server");
                let _ = swarm.dial(addr);
            }
        }
    }

    let (cmd_tx, mut cmd_rx) = mpsc::channel::<NetworkCommand>(256);

    // Pending DHT GET replies
    let mut pending_gets: std::collections::HashMap<
        kad::QueryId,
        tokio::sync::oneshot::Sender<Option<Vec<u8>>>,
    > = std::collections::HashMap::new();

    // Pending sample request replies
    let mut pending_sample_requests: std::collections::HashMap<
        request_response::OutboundRequestId,
        tokio::sync::oneshot::Sender<Option<SampleResponse>>,
    > = std::collections::HashMap::new();

    // Pending access request replies
    let mut pending_access_requests: std::collections::HashMap<
        request_response::OutboundRequestId,
        tokio::sync::oneshot::Sender<Option<data_core::types::AccessGrant>>,
    > = std::collections::HashMap::new();

    // Pending inbound response channels (keyed by monotonic ID)
    let mut next_channel_id: u64 = 0;
    let mut pending_sample_channels: std::collections::HashMap<
        u64,
        request_response::ResponseChannel<Vec<u8>>,
    > = std::collections::HashMap::new();
    let mut pending_access_channels: std::collections::HashMap<
        u64,
        request_response::ResponseChannel<Vec<u8>>,
    > = std::collections::HashMap::new();

    // Spawn swarm event loop
    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(cmd) = cmd_rx.recv() => {
                    match cmd {
                        NetworkCommand::DhtPut { key, value } => {
                            let record = kad::Record {
                                key: kad::RecordKey::new(&key),
                                value,
                                publisher: None,
                                expires: None,
                            };
                            if let Err(e) = swarm.behaviour_mut().kademlia.put_record(record, kad::Quorum::One) {
                                warn!("DHT PUT failed: {e:?}");
                            }
                        }
                        NetworkCommand::DhtGet { key, reply } => {
                            let qid = swarm.behaviour_mut().kademlia.get_record(kad::RecordKey::new(&key));
                            pending_gets.insert(qid, reply);
                        }
                        NetworkCommand::GossipPublish { topic: t, data } => {
                            let tp = gossipsub::IdentTopic::new(t);
                            if let Err(e) = swarm.behaviour_mut().gossipsub.publish(tp, data) {
                                warn!("gossip publish failed: {e:?}");
                            }
                        }
                        NetworkCommand::Ping { reply } => {
                            let _ = reply.send(());
                        }
                        NetworkCommand::SampleRequest { peer, request, reply } => {
                            if let Ok(data) = serde_json::to_vec(&request) {
                                let req_id = swarm.behaviour_mut().sample_protocol.send_request(&peer, data);
                                pending_sample_requests.insert(req_id, reply);
                            } else {
                                let _ = reply.send(None);
                            }
                        }
                        NetworkCommand::SampleResponse { channel_id, response } => {
                            if let Some(channel) = pending_sample_channels.remove(&channel_id) {
                                if let Ok(data) = serde_json::to_vec(&response) {
                                    let _ = swarm.behaviour_mut().sample_protocol.send_response(channel, data);
                                }
                            } else {
                                warn!(channel_id, "sample response channel not found");
                            }
                        }
                        NetworkCommand::AccessRequest { peer, request, reply } => {
                            if let Ok(data) = serde_json::to_vec(&request) {
                                let req_id = swarm.behaviour_mut().access_protocol.send_request(&peer, data);
                                pending_access_requests.insert(req_id, reply);
                            } else {
                                let _ = reply.send(None);
                            }
                        }
                        NetworkCommand::AccessResponse { channel_id, response } => {
                            if let Some(channel) = pending_access_channels.remove(&channel_id) {
                                if let Ok(data) = serde_json::to_vec(&response) {
                                    let _ = swarm.behaviour_mut().access_protocol.send_response(channel, data);
                                }
                            } else {
                                warn!(channel_id, "access response channel not found");
                            }
                        }
                    }
                }
                event = swarm.select_next_some() => {
                    match event {
                        SwarmEvent::Behaviour(BehaviourEvent::Mdns(mdns_event)) => {
                            match mdns_event {
                                libp2p::mdns::Event::Discovered(peers) => {
                                    for (peer_id, addr) in peers {
                                        info!(%peer_id, %addr, "mDNS discovered peer");
                                        swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                                        let _ = event_tx.send(NetworkEvent::PeerConnected(peer_id)).await;
                                    }
                                }
                                libp2p::mdns::Event::Expired(peers) => {
                                    for (peer_id, _) in peers {
                                        debug!(%peer_id, "mDNS peer expired");
                                    }
                                }
                            }
                        }
                        SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(gossipsub::Event::Message {
                            message, ..
                        })) => {
                            debug!(len = message.data.len(), "received gossipsub message");
                            let _ = event_tx.send(NetworkEvent::NewMetadata(message.data)).await;
                        }
                        SwarmEvent::Behaviour(BehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed {
                            id, result, ..
                        })) => {
                            match result {
                                kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(
                                    kad::PeerRecord { record, .. },
                                ))) => {
                                    if let Some(reply) = pending_gets.remove(&id) {
                                        let _ = reply.send(Some(record.value));
                                    }
                                }
                                kad::QueryResult::GetRecord(Err(_)) => {
                                    if let Some(reply) = pending_gets.remove(&id) {
                                        let _ = reply.send(None);
                                    }
                                }
                                _ => {}
                            }
                        }
                        // Sample protocol events
                        SwarmEvent::Behaviour(BehaviourEvent::SampleProtocol(
                            request_response::Event::Message { peer, message, .. }
                        )) => {
                            match message {
                                request_response::Message::Request { request, channel, .. } => {
                                    if let Ok(req) = serde_json::from_slice::<SampleRequest>(&request) {
                                        let ch_id = next_channel_id;
                                        next_channel_id += 1;
                                        pending_sample_channels.insert(ch_id, channel);
                                        let _ = event_tx.send(NetworkEvent::IncomingSampleRequest {
                                            peer,
                                            channel_id: ch_id,
                                            request: req,
                                        }).await;
                                    }
                                }
                                request_response::Message::Response { request_id, response } => {
                                    if let Some(reply) = pending_sample_requests.remove(&request_id) {
                                        let resp = serde_json::from_slice::<SampleResponse>(&response).ok();
                                        let _ = reply.send(resp);
                                    }
                                }
                            }
                        }
                        // Access protocol events
                        SwarmEvent::Behaviour(BehaviourEvent::AccessProtocol(
                            request_response::Event::Message { peer, message, .. }
                        )) => {
                            match message {
                                request_response::Message::Request { request, channel, .. } => {
                                    if let Ok(req) = serde_json::from_slice::<data_core::types::AccessRequest>(&request) {
                                        let ch_id = next_channel_id;
                                        next_channel_id += 1;
                                        pending_access_channels.insert(ch_id, channel);
                                        let _ = event_tx.send(NetworkEvent::IncomingAccessRequest {
                                            peer,
                                            channel_id: ch_id,
                                            request: req,
                                        }).await;
                                    }
                                }
                                request_response::Message::Response { request_id, response } => {
                                    if let Some(reply) = pending_access_requests.remove(&request_id) {
                                        let grant = serde_json::from_slice::<data_core::types::AccessGrant>(&response).ok();
                                        let _ = reply.send(grant);
                                    }
                                }
                            }
                        }
                        // DCUtR events
                        SwarmEvent::Behaviour(BehaviourEvent::Dcutr(
                            dcutr::Event { remote_peer_id, result }
                        )) => {
                            match result {
                                Ok(_) => info!(%remote_peer_id, "DCUtR direct connection established"),
                                Err(e) => debug!(%remote_peer_id, error = %e, "DCUtR upgrade failed"),
                            }
                        }
                        SwarmEvent::NewListenAddr { address, .. } => {
                            info!(%address, "listening on");
                        }
                        _ => {}
                    }
                }
            }
        }
    });

    Ok(NetworkHandle {
        cmd_tx,
        local_peer_id,
    })
}
