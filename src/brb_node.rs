use serde::{Deserialize, Serialize};

use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use cmdr::*;

use log::{debug, error, info, warn};
use std::io::Write;

use qp2p::{self, Config, Endpoint, QuicP2p};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use brb::membership::actor::ed25519::{Actor, Sig, SigningActor};
use brb::{BRBDataType, DeterministicBRB, Packet as BRBPacket};
use brb_dt_orswot::BRBOrswot;

type Value = u64;
type State = BRBOrswot<Actor, Value>;
type BRB = DeterministicBRB<Actor, SigningActor, Sig, State>;
type Packet = BRBPacket<Actor, Sig, <State as BRBDataType<Actor>>::Op>;

#[derive(Debug, Clone)]
struct SharedBRB {
    brb: Arc<Mutex<BRB>>,
}

impl SharedBRB {
    fn new() -> Self {
        Self {
            brb: Arc::new(Mutex::new(BRB::new())),
        }
    }

    fn actor(&self) -> Actor {
        self.brb.lock().unwrap().actor()
    }

    fn add(&self, v: Value) -> Vec<Packet> {
        let op = { self.brb.lock().unwrap().dt.add(v) }; // wrap to release the after op
        self.exec_op(op)
    }

    fn rm(&self, v: Value) -> Vec<Packet> {
        let op = { self.brb.lock().unwrap().dt.rm(v) }; // wrap to releease lock after op
        self.exec_op(op)
    }

    fn peers(&self) -> BTreeSet<Actor> {
        self.brb.lock().unwrap().peers().unwrap_or_else(|err| {
            error!("[CLI] Failure while reading brb peers: {:?}", err);
            Default::default()
        })
    }

    fn trust_peer(&mut self, peer: Actor) {
        self.brb.lock().unwrap().force_join(peer);
    }

    fn untrust_peer(&mut self, peer: Actor) {
        self.brb.lock().unwrap().force_leave(peer);
    }

    fn request_join(&mut self, actor: Actor) -> Vec<Packet> {
        self.brb
            .lock()
            .unwrap()
            .request_membership(actor)
            .unwrap_or_else(|err| {
                error!("[CLI] Failed to request join for {:?} : {:?}", actor, err);
                Default::default()
            })
    }

    fn request_leave(&mut self, actor: Actor) -> Vec<Packet> {
        self.brb
            .lock()
            .unwrap()
            .kill_peer(actor)
            .unwrap_or_else(|err| {
                error!("[CLI] Failed to request leave for {:?}: {:?}", actor, err);
                Default::default()
            })
    }

    fn anti_entropy(&mut self, peer: Actor) -> Option<Packet> {
        match self.brb.lock().unwrap().anti_entropy(peer) {
            Ok(packet) => Some(packet),
            Err(err) => {
                error!("[CLI] Failed initiating anti-entropy {:?}", err);
                None
            }
        }
    }

    fn exec_op(&self, op: <State as BRBDataType<Actor>>::Op) -> Vec<Packet> {
        self.brb.lock().unwrap().exec_op(op).unwrap_or_else(|err| {
            error!("[CLI] Error executing datatype op: {:?}", err);
            Default::default()
        })
    }

    fn apply(&self, packet: Packet) -> Vec<Packet> {
        match self.brb.lock().unwrap().handle_packet(packet) {
            Ok(packets) => packets,
            Err(e) => {
                error!("[CLI] dropping packet: {:?}", e);
                Default::default()
            }
        }
    }

    fn read(&self) -> HashSet<Value> {
        self.brb.lock().unwrap().dt.orswot().read().val
    }
}

#[derive(Debug)]
struct Repl {
    state: SharedBRB,
    network_tx: mpsc::Sender<RouterCmd>,
}

#[cmdr]
impl Repl {
    fn new(state: SharedBRB, network_tx: mpsc::Sender<RouterCmd>) -> Self {
        Self { state, network_tx }
    }

    #[cmd]
    fn peer(&mut self, args: &[String]) -> CommandResult {
        match args {
            [ip_port] => match ip_port.parse::<SocketAddr>() {
                Ok(addr) => {
                    info!("[REPL] parsed addr {:?}", addr);
                    self.network_tx
                        .try_send(RouterCmd::SayHello(addr))
                        .unwrap_or_else(|e| {
                            error!("[REPL] Failed to queue router command {:?}", e)
                        });
                }
                Err(e) => error!("[REPL] bad addr {:?}", e),
            },
            _ => println!("help: peer <ip>:<port>"),
        };
        Ok(Action::Done)
    }

    #[cmd]
    fn peers(&mut self, args: &[String]) -> CommandResult {
        match args {
            [] => self
                .network_tx
                .try_send(RouterCmd::ListPeers)
                .unwrap_or_else(|e| error!("[REPL] Failed to queue router command {:?}", e)),
            _ => println!("help: peers expects no arguments"),
        };
        Ok(Action::Done)
    }

    #[cmd]
    fn trust(&mut self, args: &[String]) -> CommandResult {
        match args {
            [actor_id] => {
                self.network_tx
                    .try_send(RouterCmd::Trust(actor_id.to_string()))
                    .unwrap_or_else(|e| error!("[REPL] Failed to queue router command {:?}", e));
            }
            _ => println!("help: trust id:8sdkgalsd"),
        };
        Ok(Action::Done)
    }

    #[cmd]
    fn untrust(&mut self, args: &[String]) -> CommandResult {
        match args {
            [actor_id] => {
                self.network_tx
                    .try_send(RouterCmd::Untrust(actor_id.to_string()))
                    .unwrap_or_else(|e| error!("[REPL] Failed to queue router command {:?}", e));
            }
            _ => println!("help: untrust id:8f4e"),
        };
        Ok(Action::Done)
    }

    #[cmd]
    fn join(&mut self, args: &[String]) -> CommandResult {
        match args {
            [actor_id] => {
                self.network_tx
                    .try_send(RouterCmd::RequestJoin(actor_id.to_string()))
                    .unwrap_or_else(|e| error!("[REPL] Failed to queue router command {:?}", e));
            }
            _ => println!("help: join takes one arguments, the actor to add to the network"),
        };
        Ok(Action::Done)
    }

    #[cmd]
    fn leave(&mut self, args: &[String]) -> CommandResult {
        match args {
            [actor_id] => {
                self.network_tx
                    .try_send(RouterCmd::RequestLeave(actor_id.to_string()))
                    .unwrap_or_else(|e| error!("[REPL] Failed to queue router command {:?}", e));
            }
            _ => println!("help: leave takes one arguments, the actor to leave the network"),
        };
        Ok(Action::Done)
    }

    #[cmd]
    fn anti_entropy(&mut self, args: &[String]) -> CommandResult {
        match args {
            [actor_id] => {
                self.network_tx
                    .try_send(RouterCmd::AntiEntropy(actor_id.to_string()))
                    .unwrap_or_else(|e| error!("[REPL] Failed to queue router command {:?}", e));
            }
            _ => println!("help: anti_entropy takes one arguments, the actor to request data from"),
        };

        Ok(Action::Done)
    }

    #[cmd]
    fn retry(&mut self, _args: &[String]) -> CommandResult {
        self.network_tx
            .try_send(RouterCmd::Retry)
            .expect("Failed to queue router cmd");
        Ok(Action::Done)
    }

    #[cmd]
    fn add(&mut self, args: &[String]) -> CommandResult {
        match args {
            [arg] => match arg.parse::<Value>() {
                Ok(v) => {
                    for packet in self.state.add(v) {
                        self.network_tx
                            .try_send(RouterCmd::Deliver(packet))
                            .expect("Failed to queue packet");
                    }
                }
                Err(_) => error!("[REPL] bad arg: '{}'", arg),
            },
            _ => println!("help: add <v>"),
        }
        Ok(Action::Done)
    }

    #[cmd]
    fn remove(&mut self, args: &[String]) -> CommandResult {
        match args {
            [arg] => match arg.parse::<Value>() {
                Ok(v) => {
                    for packet in self.state.rm(v) {
                        self.network_tx
                            .try_send(RouterCmd::Deliver(packet))
                            .expect("Failed to queue packet");
                    }
                }
                Err(_) => error!("[REPL] bad arg: '{}'", arg),
            },
            _ => println!("help: remove <v>"),
        }
        Ok(Action::Done)
    }

    #[cmd]
    fn read(&mut self, _args: &[String]) -> CommandResult {
        println!("{:?}", self.state.read());
        Ok(Action::Done)
    }

    #[cmd]
    fn dbg(&mut self, _args: &[String]) -> CommandResult {
        self.network_tx
            .try_send(RouterCmd::Debug)
            .expect("Failed to queue router cmd");
        Ok(Action::Done)
    }
}

#[derive(Debug)]
struct Router {
    state: SharedBRB,
    qp2p: QuicP2p,
    addr: SocketAddr,
    peers: HashMap<Actor, SocketAddr>,
    unacked_packets: VecDeque<Packet>,
}

#[derive(Debug)]
enum RouterCmd {
    Retry,
    Debug,
    AntiEntropy(String),
    ListPeers,
    RequestJoin(String),
    RequestLeave(String),
    Trust(String),
    Untrust(String),
    SayHello(SocketAddr),
    AddPeer(Actor, SocketAddr),
    Deliver(Packet),
    Apply(Packet),
    Acked(Packet),
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Serialize, Deserialize)]
enum NetworkMsg {
    Peer(Actor, SocketAddr),
    Packet(Packet),
    Ack(Packet),
}

impl Router {
    async fn new(state: SharedBRB) -> (Self, Endpoint) {
        let qp2p = QuicP2p::with_config(
            Some(Config {
                port: Some(0),
                ip: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
                ..Default::default()
            }),
            Default::default(),
            true,
        )
        .expect("Error creating QuicP2p object");

        let endpoint = qp2p.new_endpoint().expect("Failed to create endpoint");
        let addr = endpoint
            .socket_addr()
            .await
            .expect("Failed to read our addr from endpoint");

        let router = Self {
            state,
            qp2p,
            addr,
            peers: Default::default(),
            unacked_packets: Default::default(),
        };

        (router, endpoint)
    }

    fn new_endpoint(&self) -> Endpoint {
        self.qp2p.new_endpoint().expect("Failed to create endpoint")
    }

    fn resolve_actor(&self, actor_id: &str) -> Option<Actor> {
        let matching_actors: Vec<Actor> = self
            .peers
            .iter()
            .map(|(actor, _)| actor)
            .cloned()
            .filter(|actor| format!("{:?}", actor).starts_with(&actor_id))
            .collect();

        if matching_actors.len() > 1 {
            println!("Ambiguous actor id, more than one actor matches:");
            for actor in matching_actors {
                println!("{:?}", actor);
            }
            None
        } else if matching_actors.is_empty() {
            println!("No actors with that actor id");
            None
        } else {
            Some(matching_actors[0])
        }
    }

    async fn listen_for_cmds(mut self, mut net_rx: mpsc::Receiver<RouterCmd>) {
        while let Some(net_cmd) = net_rx.recv().await {
            self.apply(net_cmd).await;
        }
    }

    async fn deliver_network_msg(&self, network_msg: &NetworkMsg, dest_addr: &SocketAddr) {
        let msg = bincode::serialize(&network_msg).unwrap();
        let endpoint = self.new_endpoint();
        match endpoint.connect_to(&dest_addr).await {
            Ok((conn, _)) => {
                match conn.send_uni(msg.clone().into()).await {
                    Ok(_) => info!("[P2P] Sent network msg successfully."),
                    Err(e) => error!("[P2P] Failed to send network msg: {:?}", e),
                }
                conn.close();
            }
            Err(err) => {
                error!(
                    "[P2P] Failed to connect to destination {:?}: {:?}",
                    dest_addr, err
                );
            }
        }
    }

    async fn deliver_packet(&mut self, packet: Packet) {
        match self.peers.get(&packet.dest) {
            Some(peer_addr) => {
                info!(
                    "[P2P] delivering packet to {:?} at addr {:?}: {:?}",
                    packet.dest, peer_addr, packet
                );
                self.unacked_packets.push_back(packet.clone());
                self.deliver_network_msg(&NetworkMsg::Packet(packet), &peer_addr)
                    .await;
            }
            None => warn!(
                "[P2P] we don't have a peer matching the destination for packet {:?}",
                packet
            ),
        }
    }

    async fn apply(&mut self, cmd: RouterCmd) {
        debug!("[P2P] router cmd {:?}", cmd);
        match cmd {
            RouterCmd::Retry => {
                let packets_to_retry = self.unacked_packets.clone();
                self.unacked_packets = Default::default();
                for packet in packets_to_retry {
                    println!("Retrying packet: {:#?}", packet);
                    self.deliver_packet(packet).await
                }
            }
            RouterCmd::Debug => {
                debug!("{:#?}", self);
            }
            RouterCmd::AntiEntropy(actor_id) => {
                if let Some(actor) = self.resolve_actor(&actor_id) {
                    info!("[P2P] Starting anti-entropy with actor: {:?}", actor);
                    if let Some(packet) = self.state.anti_entropy(actor) {
                        self.deliver_packet(packet).await;
                    }
                }
            }
            RouterCmd::ListPeers => {
                let voting_peers = self.state.peers();

                let peer_addrs: BTreeMap<_, _> =
                    self.peers.iter().map(|(p, addr)| (*p, *addr)).collect();

                let identities: BTreeSet<_> = voting_peers
                    .iter()
                    .cloned()
                    .chain(peer_addrs.keys().cloned())
                    .collect();

                for id in identities {
                    let mut line = format!("{:?}", id);
                    line.push('@');
                    match peer_addrs.get(&id) {
                        Some(addr) => line.push_str(&format!("{:?}", addr)),
                        None => line.push_str("<unknown>"),
                    };
                    if voting_peers.contains(&id) {
                        line.push_str("\t(voting)");
                    }
                    if id == self.state.actor() {
                        line.push_str("\t(self)");
                    }
                    println!("{}", line);
                }
            }
            RouterCmd::RequestJoin(actor_id) => {
                if let Some(actor) = self.resolve_actor(&actor_id) {
                    info!("[P2P] Starting join for actor: {:?}", actor);
                    for packet in self.state.request_join(actor) {
                        self.deliver_packet(packet).await;
                    }
                }
            }
            RouterCmd::RequestLeave(actor_id) => {
                if let Some(actor) = self.resolve_actor(&actor_id) {
                    info!("[P2P] Starting leave for actor: {:?}", actor);
                    for packet in self.state.request_leave(actor) {
                        self.deliver_packet(packet).await;
                    }
                }
            }
            RouterCmd::Trust(actor_id) => {
                if let Some(actor) = self.resolve_actor(&actor_id) {
                    info!("[P2P] Trusting actor: {:?}", actor);
                    self.state.trust_peer(actor);
                }
            }
            RouterCmd::Untrust(actor_id) => {
                if let Some(actor) = self.resolve_actor(&actor_id) {
                    info!("[P2P] Trusting actor: {:?}", actor);
                    self.state.untrust_peer(actor);
                }
            }
            RouterCmd::SayHello(addr) => {
                self.deliver_network_msg(&NetworkMsg::Peer(self.state.actor(), self.addr), &addr)
                    .await
            }
            RouterCmd::AddPeer(actor, addr) =>
            {
                #[allow(clippy::map_entry)]
                if !self.peers.contains_key(&actor) {
                    for (peer_actor, peer_addr) in self.peers.iter() {
                        self.deliver_network_msg(&NetworkMsg::Peer(*peer_actor, *peer_addr), &addr)
                            .await;
                    }
                    self.peers.insert(actor, addr);
                }
            }
            RouterCmd::Deliver(packet) => {
                self.deliver_packet(packet).await;
            }
            RouterCmd::Apply(op_packet) => {
                for packet in self.state.apply(op_packet.clone()) {
                    self.deliver_packet(packet).await;
                }

                if let Some(peer_addr) = self.peers.get(&op_packet.source) {
                    info!(
                        "[P2P] delivering Ack(packet) to {:?} at addr {:?}: {:?}",
                        op_packet.dest, peer_addr, op_packet
                    );
                    self.deliver_network_msg(&NetworkMsg::Ack(op_packet), &peer_addr)
                        .await;
                } else {
                    warn!(
                        "[P2P] we don't have a peer matching the destination for packet {:?}",
                        op_packet
                    );
                }
            }
            RouterCmd::Acked(packet) => {
                self.unacked_packets
                    .iter()
                    .position(|p| p == &packet)
                    .map(|idx| {
                        info!("[P2P] Got ack for packet {:?}", packet);
                        self.unacked_packets.remove(idx)
                    });
            }
        }
    }
}

async fn listen_for_network_msgs(endpoint: Endpoint, mut router_tx: mpsc::Sender<RouterCmd>) {
    let listen_addr = endpoint
        .socket_addr()
        .await
        .expect("Failed to read listening socket addr");
    info!("[P2P] listening on {:?}", listen_addr);

    router_tx
        .send(RouterCmd::SayHello(listen_addr))
        .await
        .expect("Failed to send command to add self as peer");

    let mut conns = endpoint.listen();
    while let Some(mut msgs) = conns.next().await {
        while let Some(msg) = msgs.next().await {
            let net_msg: NetworkMsg = bincode::deserialize(&msg.get_message_data()).unwrap();
            let cmd = match net_msg {
                NetworkMsg::Peer(actor, addr) => RouterCmd::AddPeer(actor, addr),
                NetworkMsg::Packet(packet) => RouterCmd::Apply(packet),
                NetworkMsg::Ack(packet) => RouterCmd::Acked(packet),
            };

            router_tx
                .send(cmd)
                .await
                .expect("Failed to send router command");
        }
    }

    info!("[P2P] Finished listening for connections");
}

#[tokio::main]
async fn main() {
    // Customize logger to:
    //  1. display messages from brb crates only.  (filter)
    //  2. omit timestamp, etc.  display each log message string + newline.
    env_logger::Builder::from_env(
        env_logger::Env::default()
            .default_filter_or("brb=debug,brb_membership=debug,brb_dt_orswot=debug,brb_node=debug"),
    )
    .format(|buf, record| writeln!(buf, "{}\n", record.args()))
    .init();

    let state = SharedBRB::new();
    let (router, endpoint) = Router::new(state.clone()).await;
    let (router_tx, router_rx) = mpsc::channel(100);

    tokio::spawn(listen_for_network_msgs(endpoint, router_tx.clone()));
    tokio::spawn(router.listen_for_cmds(router_rx));
    cmd_loop(&mut Repl::new(state, router_tx)).expect("Failure in REPL");
}
