use serde::{Deserialize, Serialize};

use bytes::Bytes;
use std::sync::{Arc, Mutex};
use tokio::sync;
use tokio::sync::mpsc;

use cmdr::*;

use log::{debug, error, info, trace, warn};
use std::io::Write;

use qp2p::{
    self, Config, DisconnectionEvents, Endpoint, IncomingConnections, IncomingMessages, QuicP2p,
};
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

#[derive(Debug, Clone)]
struct SharedEndpoint {
    endpoint: Arc<sync::Mutex<Endpoint>>,
}

impl SharedEndpoint {
    fn new(e: Endpoint) -> Self {
        Self {
            endpoint: Arc::new(sync::Mutex::new(e)),
        }
    }

    pub async fn socket_addr(&self) -> SocketAddr {
        self.endpoint.lock().await.socket_addr()
    }

    pub async fn connect_to(&self, node_addr: &SocketAddr) -> qp2p::Result<()> {
        self.endpoint.lock().await.connect_to(node_addr).await
    }

    #[allow(dead_code)]
    pub async fn close(&self) {
        self.endpoint.lock().await.close()
    }

    pub async fn send_message(&self, msg: Bytes, dest: &SocketAddr) -> qp2p::Result<()> {
        self.endpoint.lock().await.send_message(msg, dest).await
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

    // cmdr::Scope hook that is called after command execution is finished.  overriding.
    fn after_command(&mut self, _line: &Line, result: CommandResult) -> CommandResult {
        // Delay writing prompt by 1 second to reduce chance that P2P log output in
        // other thread overwrites it.
        std::thread::sleep(std::time::Duration::from_secs(1));
        result
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
            _ => println!("help: trust id:8sdkgalsd | trust me"),
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
    addr: SocketAddr,
    endpoint: SharedEndpoint,
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

#[allow(dead_code)]
struct EndpointInfo {
    shared_endpoint: SharedEndpoint,
    incoming_connections: IncomingConnections,
    incoming_messages: IncomingMessages,
    disconnection_events: DisconnectionEvents,
}

impl Router {
    async fn new(state: SharedBRB) -> (Self, EndpointInfo) {
        let qp2p = QuicP2p::with_config(
            Some(Config {
                local_port: None,
                local_ip: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
                idle_timeout_msec: Some(1000 * 86400 * 365), // 1 year idle timeout.
                ..Default::default()
            }),
            Default::default(),
            true,
        )
        .expect("Error creating QuicP2p object");

        let epmeta = qp2p.new_endpoint().await.unwrap();
        let endpoint_info = EndpointInfo {
            shared_endpoint: SharedEndpoint::new(epmeta.0),
            incoming_connections: epmeta.1,
            incoming_messages: epmeta.2,
            disconnection_events: epmeta.3,
        };

        let addr = endpoint_info.shared_endpoint.socket_addr().await;

        let router = Self {
            state,
            addr,
            endpoint: endpoint_info.shared_endpoint.clone(),
            peers: Default::default(),
            unacked_packets: Default::default(),
        };

        (router, endpoint_info)
    }

    fn resolve_actor(&self, actor_id: &str) -> Option<Actor> {
        if actor_id == "me" {
            return Some(self.state.actor());
        }

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

    async fn deliver_network_msg(&mut self, network_msg: &NetworkMsg, dest_addr: &SocketAddr) {
        let msg = bincode::serialize(&network_msg).unwrap();

        if let Err(e) = self.endpoint.connect_to(&dest_addr).await {
            error!("[P2P] Failed to connect. {:?}", e);
            return;
        }

        let logmsg = format!(
            "[P2P] Sending message to {:?} --> {:?}",
            dest_addr, network_msg
        );
        match network_msg {
            NetworkMsg::Ack(_) => trace!("{}", logmsg),
            _ => debug!("{}", logmsg),
        }

        match self.endpoint.send_message(msg.into(), &dest_addr).await {
            Ok(()) => trace!("[P2P] Sent network msg successfully."),
            Err(e) => error!("[P2P] Failed to send network msg: {:?}", e),
        }
    }

    async fn deliver_packet(&mut self, packet: Packet) {
        match self.peers.clone().get(&packet.dest) {
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
        let logmsg = format!("[P2P] router cmd {:?}", cmd);
        match cmd {
            RouterCmd::Acked(_) => trace!("{}", logmsg),
            _ => debug!("{}", logmsg),
        }

        match cmd {
            RouterCmd::Retry => {
                let packets_to_retry = self.unacked_packets.clone();
                self.unacked_packets = Default::default();
                for packet in packets_to_retry {
                    info!("Retrying packet: {:#?}", packet);
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
                    for (peer_actor, peer_addr) in self.peers.clone().iter() {
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

                if let Some(peer_addr) = self.peers.clone().get(&op_packet.source) {
                    trace!(
                        "[P2P] delivering Ack(packet) to {:?} at addr {:?}: {:?}",
                        op_packet.dest,
                        peer_addr,
                        op_packet
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
                        trace!("[P2P] Got ack for packet {:?}", packet);
                        self.unacked_packets.remove(idx)
                    });
            }
        }
    }
}

async fn listen_for_network_msgs(
    mut endpoint_info: EndpointInfo,
    mut router_tx: mpsc::Sender<RouterCmd>,
) {
    let listen_addr = endpoint_info.shared_endpoint.socket_addr().await;
    info!("[P2P] listening on {:?}", listen_addr);

    router_tx
        .send(RouterCmd::SayHello(listen_addr))
        .await
        .expect("Failed to send command to add self as peer");

    while let Some((socket_addr, bytes)) = endpoint_info.incoming_messages.next().await {
        let net_msg: NetworkMsg = bincode::deserialize(&bytes).unwrap();

        let msg = format!("[P2P] received from {:?} --> {:?}", socket_addr, net_msg);
        match net_msg {
            NetworkMsg::Ack(_) => trace!("{}", msg),
            _ => debug!("{}", msg),
        }

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

    info!("[P2P] Finished listening for incoming messages");
}

#[tokio::main]
async fn main() {
    // Customize logger to:
    //  1. display messages from brb crates only.  (filter)
    //  2. omit timestamp, etc.  display each log message string + newline.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(
        "brb=debug,brb_membership=debug,brb_dt_orswot=debug,brb_node=debug,qp2p=warn,quinn=warn",
    ))
    .format(|buf, record| writeln!(buf, "{}\n", record.args()))
    .init();

    let state = SharedBRB::new();
    let (router, endpoint_info) = Router::new(state.clone()).await;
    let (router_tx, router_rx) = mpsc::channel(100);

    tokio::spawn(listen_for_network_msgs(endpoint_info, router_tx.clone()));
    tokio::spawn(router.listen_for_cmds(router_rx));

    // Delay by 1 second to prevent P2P startup from overwriting user prompt.
    std::thread::sleep(std::time::Duration::from_secs(1));
    cmd_loop(&mut Repl::new(state, router_tx)).expect("Failure in REPL");
}
