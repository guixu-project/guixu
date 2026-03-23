use anyhow::Result;
use libp2p::{
    futures::StreamExt,
    gossipsub, identify, kad,
    mdns::tokio::Behaviour as Mdns,
    noise, tcp, yamux,
    swarm::SwarmEvent,
    Multiaddr, PeerId, StreamProtocol, Swarm,
};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use data_core::config::NodeConfig;

/// Events emitted by the P2P network layer to upper layers.
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    NewMetadata(Vec<u8>),
    PeerConnected(PeerId),
}

/// Commands sent from upper layers to the network task.
#[derive(Debug)]
pub enum NetworkCommand {
    DhtPut { key: Vec<u8>, value: Vec<u8> },
    DhtGet { key: Vec<u8>, reply: tokio::sync::oneshot::Sender<Option<Vec<u8>>> },
    GossipPublish { topic: String, data: Vec<u8> },
}

#[derive(libp2p::swarm::NetworkBehaviour)]
struct Behaviour {
    kademlia: kad::Behaviour<kad::store::MemoryStore>,
    gossipsub: gossipsub::Behaviour,
    mdns: Mdns,
    identify: identify::Behaviour,
}

pub const DATASETS_TOPIC: &str = "datasets";

/// Handle to the running P2P network. Send commands via `cmd_tx`.
pub struct NetworkHandle {
    pub cmd_tx: mpsc::Sender<NetworkCommand>,
    pub local_peer_id: PeerId,
}

impl NetworkHandle {
    pub async fn dht_put(&self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.cmd_tx.send(NetworkCommand::DhtPut { key, value }).await?;
        Ok(())
    }

    pub async fn dht_get(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.cmd_tx.send(NetworkCommand::DhtGet { key, reply: tx }).await?;
        Ok(rx.await?)
    }

    pub async fn gossip_publish(&self, topic: String, data: Vec<u8>) -> Result<()> {
        self.cmd_tx.send(NetworkCommand::GossipPublish { topic, data }).await?;
        Ok(())
    }
}

/// Start the P2P network. Returns a handle and spawns the swarm event loop.
pub async fn start(
    config: &NodeConfig,
    keypair_seed: &[u8; 32],
    event_tx: mpsc::Sender<NetworkEvent>,
) -> Result<NetworkHandle> {
    // Build identity from seed (deterministic)
    let mut id_bytes = [0u8; 64];
    // ed25519 in libp2p uses the full 64-byte expanded key, but we can derive from seed
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
    let mdns = Mdns::new(Default::default(), local_peer_id)?;

    // Identify
    let identify = identify::Behaviour::new(identify::Config::new(
        "/data-protocol/id/1.0.0".into(),
        id_keypair.public(),
    ));

    let behaviour = Behaviour { kademlia, gossipsub, mdns, identify };

    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(id_keypair)
        .with_tokio()
        .with_tcp(tcp::Config::default(), noise::Config::new, yamux::Config::default)?
        .with_behaviour(|_| Ok(behaviour))
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

    let (cmd_tx, mut cmd_rx) = mpsc::channel::<NetworkCommand>(256);

    // Pending DHT GET replies
    let mut pending_gets: std::collections::HashMap<
        kad::QueryId,
        tokio::sync::oneshot::Sender<Option<Vec<u8>>>,
    > = std::collections::HashMap::new();

    // Spawn swarm event loop
    tokio::spawn(async move {
        let topic = gossipsub::IdentTopic::new(DATASETS_TOPIC);
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
                        SwarmEvent::NewListenAddr { address, .. } => {
                            info!(%address, "listening on");
                        }
                        _ => {}
                    }
                }
            }
        }
    });

    Ok(NetworkHandle { cmd_tx, local_peer_id })
}
