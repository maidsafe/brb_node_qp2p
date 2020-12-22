use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use cmdr::*;

use qp2p::{self, Config, Endpoint, QuicP2p};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use brb::{Actor, BRBAlgorithm, DeterministicBRB, Packet as BRBPacket};
use brb_algo_orswot::BRBOrswot;

type Value = u64;
type State = BRBOrswot<Value>;
type BRB = DeterministicBRB<State>;
type Packet = BRBPacket<<State as BRBAlgorithm>::Op>;

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

    fn peers(&self) -> BTreeSet<Actor> {
        self.brb.lock().unwrap().peers().unwrap()
    }

    fn trust_peer(&mut self, peer: Actor) {
        self.brb.lock().unwrap().trust_peer(peer);
    }

    fn request_membership(&mut self, actor: Actor) -> Vec<Packet> {
        self.brb.lock().unwrap().request_membership(actor).unwrap()
    }

    fn exec_algo_op(
        &self,
        f: impl FnOnce(&State) -> Option<<State as BRBAlgorithm>::Op>,
    ) -> Vec<Packet> {
        self.brb.lock().unwrap().exec_algo_op(f).unwrap()
    }

    fn apply(&self, packet: Packet) -> Vec<Packet> {
        self.brb.lock().unwrap().handle_packet(packet).unwrap()
    }

    fn read(&self) -> HashSet<Value> {
        self.brb
            .lock()
            .unwrap()
            .read_state(|orswot| orswot.state().read().val)
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
                    println!("[REPL] parsed addr {:?}", addr);
                    self.network_tx.try_send(RouterCmd::SayHello(addr)).unwrap();
                }
                Err(e) => println!("[REPL] bad addr {:?}", e),
            },
            _ => println!("help: peer <ip>:<port>"),
        };
        Ok(Action::Done)
    }

    #[cmd]
    fn peers(&mut self, args: &[String]) -> CommandResult {
        match args {
            [] => self.network_tx.try_send(RouterCmd::ListPeers).unwrap(),
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
                    .unwrap();
            }
            _ => println!("help: trust id:8sdkgalsd"),
        };
        Ok(Action::Done)
    }

    #[cmd]
    fn join(&mut self, args: &[String]) -> CommandResult {
        match args {
            [actor_id] => {
                self.network_tx
                    .try_send(RouterCmd::RequestMembership(actor_id.to_string()))
                    .unwrap();
            }
            _ => println!("help: join takes one arguments, the actor to add to the network"),
        };
        Ok(Action::Done)
    }

    #[cmd]
    fn add(&mut self, args: &[String]) -> CommandResult {
        match args {
            [arg] => match arg.parse::<Value>() {
                Ok(v) => {
                    for packet in self.state.exec_algo_op(|orswot| Some(orswot.add(v))) {
                        self.network_tx
                            .try_send(RouterCmd::Deliver(packet))
                            .expect("Failed to queue packet");
                    }
                }
                Err(_) => println!("[REPL] bad arg: '{}'", arg),
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
                    for packet in self.state.exec_algo_op(|orswot| orswot.rm(v)) {
                        self.network_tx
                            .try_send(RouterCmd::Deliver(packet))
                            .expect("Failed to queue packet");
                    }
                }
                Err(_) => println!("[REPL] bad arg: '{}'", arg),
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
        println!("{:#?}", self);
        Ok(Action::Done)
    }
}

#[derive(Debug)]
struct Router {
    state: SharedBRB,
    qp2p: QuicP2p,
    addr: SocketAddr,
    peers: HashMap<Actor, (Endpoint, SocketAddr)>,
}

#[derive(Debug)]
enum RouterCmd {
    ListPeers,
    RequestMembership(String),
    Trust(String),
    SayHello(SocketAddr),
    AddPeer(Actor, SocketAddr),
    Deliver(Packet),
    Apply(Packet),
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Serialize, Deserialize)]
enum NetworkMsg {
    HelloMyNameIs(Actor, SocketAddr),
    Packet(Packet),
}

impl Router {
    fn new(state: SharedBRB) -> (Self, Endpoint) {
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
            .our_addr()
            .expect("Failed to read our addr from endpoint");

        let router = Self {
            state,
            qp2p,
            addr,
            peers: Default::default(),
        };

        (router, endpoint)
    }

    fn new_endpoint(&self) -> Endpoint {
        self.qp2p.new_endpoint().expect("Failed to create endpoint")
    }

    async fn listen_for_cmds(mut self, mut net_rx: mpsc::Receiver<RouterCmd>) {
        while let Some(net_cmd) = net_rx.recv().await {
            self.apply(net_cmd).await;
        }
    }

    async fn deliver_packet(&self, packet: Packet) {
        if let Some((peer, addr)) = self.peers.get(&packet.dest) {
            println!(
                "[P2P] delivering packet to {:?} at addr {:?}",
                packet.dest, addr
            );
            let msg = bincode::serialize(&NetworkMsg::Packet(packet)).unwrap();
            let conn = peer.connect_to(&addr).await.unwrap();
            let _ = conn.send_uni(msg.clone().into()).await.unwrap();
            conn.close()
        } else {
            println!(
                "[P2P] we don't have a peer matching the destination for packet {:?}",
                packet
            );
        }
    }

    async fn apply(&mut self, cmd: RouterCmd) {
        println!("[P2P] router cmd {:?}", cmd);
        match cmd {
            RouterCmd::ListPeers => {
                let voting_peers = self.state.peers();
                for (actor, (_, addr)) in self.peers.iter() {
                    if voting_peers.contains(actor) {
                        println!("{:?}@{:?}\t(voting)", actor, addr);
                    } else {
                        println!("{:?}@{:?}", actor, addr);
                    }
                }
            }
            RouterCmd::RequestMembership(actor_id) => {
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
                } else if matching_actors.is_empty() {
                    println!("No actors with that actor id");
                } else {
                    let actor = matching_actors[0];
                    println!("Starting join for actor: {:?}", actor);
                    for packet in self.state.request_membership(actor) {
                        self.deliver_packet(packet).await;
                    }
                }
            }
            RouterCmd::Trust(actor_id) => {
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
                } else if matching_actors.is_empty() {
                    println!("No actors with that actor id");
                } else {
                    let actor = matching_actors[0];
                    println!("Trusting actor: {:?}", actor);
                    self.state.trust_peer(actor); // TODO: rename state to brb
                }
            }
            RouterCmd::SayHello(addr) => {
                let peer = self.new_endpoint();
                let conn = peer.connect_to(&addr).await.unwrap();
                // self.peers.insert(actor, (peer, addr));
                let msg =
                    bincode::serialize(&NetworkMsg::HelloMyNameIs(self.state.actor(), self.addr))
                        .unwrap();
                let _ = conn.send_uni(msg.into()).await.unwrap();
                conn.close();
            }
            RouterCmd::AddPeer(actor, addr) => {
                #[allow(clippy::map_entry)]
                if !self.peers.contains_key(&actor) {
                    let peer = self.new_endpoint();
                    let conn = peer.connect_to(&addr).await.unwrap();
                    self.peers.insert(actor, (peer, addr));
                    let msg = bincode::serialize(&NetworkMsg::HelloMyNameIs(
                        self.state.actor(),
                        self.addr,
                    ))
                    .unwrap();
                    let _ = conn.send_uni(msg.into()).await.unwrap();
                    conn.close();
                }
            }
            RouterCmd::Deliver(packet) => {
                self.deliver_packet(packet).await;
            }
            RouterCmd::Apply(op_packet) => {
                for packet in self.state.apply(op_packet) {
                    self.deliver_packet(packet).await;
                }
            }
        }
    }
}

async fn listen_for_network_msgs(endpoint: Endpoint, mut router_tx: mpsc::Sender<RouterCmd>) {
    println!("[P2P] listening on {:?}", endpoint.our_addr());

    router_tx
        .send(RouterCmd::SayHello(endpoint.our_addr().unwrap()))
        .await
        .expect("Failed to send command to add self as peer");

    match endpoint.listen() {
        Ok(mut conns) => {
            while let Some(mut msgs) = conns.next().await {
                while let Some(msg) = msgs.next().await {
                    let net_msg: NetworkMsg =
                        bincode::deserialize(&msg.get_message_data()).unwrap();
                    let cmd = match net_msg {
                        NetworkMsg::HelloMyNameIs(actor, addr) => RouterCmd::AddPeer(actor, addr),
                        NetworkMsg::Packet(packet) => RouterCmd::Apply(packet),
                    };

                    router_tx
                        .send(cmd)
                        .await
                        .expect("Failed to send router command");
                }
            }
        }
        Err(e) => println!("[P2P/ERROR] failed to start listening: {:?}", e),
    }
}

#[tokio::main]
async fn main() {
    let state = SharedBRB::new();
    let (router, endpoint) = Router::new(state.clone());
    let (router_tx, router_rx) = mpsc::channel(100);

    tokio::spawn(listen_for_network_msgs(endpoint, router_tx.clone()));
    tokio::spawn(router.listen_for_cmds(router_rx));
    cmd_loop(&mut Repl::new(state, router_tx)).expect("Failure in REPL");
}
