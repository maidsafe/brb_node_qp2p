# brb_node_qp2p

[MaidSafe website](http://maidsafe.net) | [Safe Network Forum](https://safenetforum.org/)
:-------------------------------------: | :---------------------------------------------:

## About

This crate implements a P2P node (CLI) for using BRB over Quic protocol via [qp2p](https://github.com/maidsafe/qp2p).

This is a very basic CLI intended for developers to interactively try out the BRB and BRB Membership protocols.  It is not an end-user tool.

Each running node process is a BRB node.  It may start a voting group or join an existing group.

For info about BRB, please see the [brb crate](https://github.com/maidsafe/brb/).

## Example Session.

We will demonstrate an interactive session with two nodes A and B.

### Starting a Voting Group

`Peer A` starts and trusts itself, which makes it a voting member of it's own group.

(`trust` aka force_join is a BRB Membership local operation)

```
$ cargo run
[P2P] listening on Ok(127.0.0.1:59232)
[P2P] router cmd SayHello(127.0.0.1:59232)
[P2P] router cmd AddPeer(i:cb6b, 127.0.0.1:59232)

> trust i:cb6b
[P2P] router cmd Trust("i:cb6b")
Trusting actor: i:cb6b
[BRB] i:cb6b is forcing i:cb6b to join

> peers
[P2P] router cmd ListPeers
i:2d63@127.0.0.1:36138
i:cb6b@127.0.0.1:59232        (voting)        (self)
```

### Joining an existing Group

`Peer B` starts, adds `A` as peer, and trusts `A`,

(`trust` aka force_join is a BRB Membership local operation)

```
$ cargo run
[P2P] listening on Ok(127.0.0.1:36138)
[P2P] router cmd SayHello(127.0.0.1:36138)
[P2P] router cmd AddPeer(i:2d63, 127.0.0.1:36138)

> peer 127.0.0.1:59232
[REPL] parsed addr 127.0.0.1:59232
[P2P] router cmd SayHello(127.0.0.1:59232)
[P2P] router cmd AddPeer(i:cb6b, 127.0.0.1:59232)

> trust i:cb6b
[P2P] router cmd Trust("i:cb6b")
Trusting actor: i:cb6b
[BRB] i:2d63 is forcing i:cb6b to join

> peers
[P2P] router cmd ListPeers
i:2d63@127.0.0.1:36138  (self)
i:cb6b@127.0.0.1:59232  (voting)
```

`Peer A` sponsors/proposes `B` to join voting group and members {`A`} vote/agree to include `B`.

(`join` is a BRB Membership p2p voting operation)

note: `B` may have asked `A` to sponsor out of band.  ie, not part of membership protocol.


```
> join i:2d63
[P2P] router cmd RequestJoin("i:2d63")
Starting join for actor: i:2d63
[P2P] delivering packet to i:cb6b at addr 127.0.0.1:59232: Packet { source: i:cb6b, dest: i:cb6b, payload: Membership(P(Ji:2d63)@i:cb6bG1), sig: sig:d59a }
Sent packet successfully.
[P2P] router cmd Apply(Packet { source: i:cb6b, dest: i:cb6b, payload: Membership(P(Ji:2d63)@i:cb6bG1), sig: sig:d59a })
[BRB] handling packet from i:cb6b->i:cb6b
[MBR] Detected super majority
[MBR] broadcasting super majority
[P2P] delivering packet to i:cb6b at addr 127.0.0.1:59232: Packet { source: i:cb6b, dest: i:cb6b, payload: Membership(SM{P(Ji:2d63)@i:cb6bG1}@i:cb6bG1), sig: sig:e89a }
Sent packet successfully.
[P2P] router cmd Apply(Packet { source: i:cb6b, dest: i:cb6b, payload: Membership(SM{P(Ji:2d63)@i:cb6bG1}@i:cb6bG1), sig: sig:e89a })
[BRB] handling packet from i:cb6b->i:cb6b
[MBR] Detected super majority over super majorities

> peers
[P2P] router cmd ListPeers
i:2d63@127.0.0.1:36138  (voting)
i:cb6b@127.0.0.1:59232  (voting)        (self)
```

`Peer B` is not yet aware of the vote, so initiates an `enti_entropy` round with `A` to obtain voting history.
After which, `B` knows self to be a voting member of the group.

(`anti_entropy` is a read-only BRB operation)

```
> peers
[P2P] router cmd ListPeers
i:2d63@127.0.0.1:36138  (self)
> i:cb6b@127.0.0.1:59232        (voting)

> anti_entropy i:cb6b
[P2P] router cmd AntiEntropy("i:cb6b")
Starting anti-entropy with actor: i:cb6b
[P2P] delivering packet to i:cb6b at addr 127.0.0.1:59232: Packet { source: i:2d63, dest: i:cb6b, payload: AntiEntropy { generation: 0, delivered: VClock { dots: {} } }, sig: sig:e301 }
Sent packet successfully.
[P2P] router cmd Apply(Packet { source: i:cb6b, dest: i:2d63, payload: Membership(SM{SM{P(Ji:2d63)@i:cb6bG1}@i:cb6bG1}@i:cb6bG1), sig: sig:9310 })
[BRB] handling packet from i:cb6b->i:2d63
[MBR] Detected super majority over super majorities
[MBR] Adding vote to history

> peers
[P2P] router cmd ListPeers
> i:2d63@127.0.0.1:36138        (voting)        (self)
i:cb6b@127.0.0.1:59232  (voting)
```

### Data Operation

`Peer B` initiates a CRDT (ORSWOT) operation to add element '3' to the data set.
(`add` is a DataType operation, wrapped by BRB)

```
> read
{}
> add 3
[BRB] i:2d63 initiating bft for msg Msg { gen: 1, op: Add(i:2d63.1, [3]), dot: i:2d63.1 }
[BRB] broadcasting i:2d63->{i:2d63, i:cb6b}
[P2P] router cmd Deliver(Packet { source: i:2d63, dest: i:2d63, payload: BRB(RequestValidation { msg: Msg { gen: 1, op: Add(i:2d63.1, [3]), dot: i:2d63.1 } }), sig: sig:75d6 })
[P2P] delivering packet to i:2d63 at addr 127.0.0.1:36138: Packet { source: i:2d63, dest: i:2d63, payload: BRB(RequestValidation { msg: Msg { gen: 1, op: Add(i:2d63.1, [3]), dot: i:2d63.1 } }), sig: sig:75d6 }
> Sent packet successfully.
[P2P] router cmd Deliver(Packet { source: i:2d63, dest: i:cb6b, payload: BRB(RequestValidation { msg: Msg { gen: 1, op: Add(i:2d63.1, [3]), dot: i:2d63.1 } }), sig: sig:75d6 })
[P2P] delivering packet to i:cb6b at addr 127.0.0.1:59232: Packet { source: i:2d63, dest: i:cb6b, payload: BRB(RequestValidation { msg: Msg { gen: 1, op: Add(i:2d63.1, [3]), dot: i:2d63.1 } }), sig: sig:75d6 }
Sent packet successfully.
[P2P] router cmd Apply(Packet { source: i:2d63, dest: i:2d63, payload: BRB(RequestValidation { msg: Msg { gen: 1, op: Add(i:2d63.1, [3]), dot: i:2d63.1 } }), sig: sig:75d6 })
[BRB] handling packet from i:2d63->i:2d63
[BRB] request for validation
[P2P] delivering packet to i:2d63 at addr 127.0.0.1:36138: Packet { source: i:2d63, dest: i:2d63, payload: BRB(SignedValidated { msg: Msg { gen: 1, op: Add(i:2d63.1, [3]), dot: i:2d63.1 }, sig: sig:084c }), sig: sig:52d8 }
Sent packet successfully.
[P2P] router cmd Apply(Packet { source: i:cb6b, dest: i:2d63, payload: BRB(SignedValidated { msg: Msg { gen: 1, op: Add(i:2d63.1, [3]), dot: i:2d63.1 }, sig: sig:fcb4 }), sig: sig:f311 })
[BRB] handling packet from i:cb6b->i:2d63
[BRB] signed validated
[P2P] router cmd Apply(Packet { source: i:2d63, dest: i:2d63, payload: BRB(SignedValidated { msg: Msg { gen: 1, op: Add(i:2d63.1, [3]), dot: i:2d63.1 }, sig: sig:084c }), sig: sig:52d8 })
[BRB] handling packet from i:2d63->i:2d63
[BRB] signed validated
[BRB] we have quorum over msg, sending proof to network
[BRB] broadcasting i:2d63->{i:2d63, i:cb6b}
[P2P] delivering packet to i:2d63 at addr 127.0.0.1:36138: Packet { source: i:2d63, dest: i:2d63, payload: BRB(ProofOfAgreement { msg: Msg { gen: 1, op: Add(i:2d63.1, [3]), dot: i:2d63.1 }, proof: {i:2d63: sig:084c, i:cb6b: sig:fcb4} }), sig: sig:c1b6 }
Sent packet successfully.
[P2P] delivering packet to i:cb6b at addr 127.0.0.1:59232: Packet { source: i:2d63, dest: i:cb6b, payload: BRB(ProofOfAgreement { msg: Msg { gen: 1, op: Add(i:2d63.1, [3]), dot: i:2d63.1 }, proof: {i:2d63: sig:084c, i:cb6b: sig:fcb4} }), sig: sig:c1b6 }
Sent packet successfully.
[P2P] router cmd Apply(Packet { source: i:2d63, dest: i:2d63, payload: BRB(ProofOfAgreement { msg: Msg { gen: 1, op: Add(i:2d63.1, [3]), dot: i:2d63.1 }, proof: {i:2d63: sig:084c, i:cb6b: sig:fcb4} }), sig: sig:c1b6 })
[BRB] handling packet from i:2d63->i:2d63
[BRB] proof of agreement: Msg { gen: 1, op: Add(i:2d63.1, [3]), dot: i:2d63.1 }

> read
{3}
```

`Peer A` also sees/agrees that element '3' has been added to the data set.

```
> read
{3}
```



## License

This Safe Network software is dual-licensed under the Modified BSD (<LICENSE-BSD> <https://opensource.org/licenses/BSD-3-Clause>) or the MIT license (<LICENSE-MIT> <https://opensource.org/licenses/MIT>) at your option.

## Contributing

Want to contribute? Great :tada:

There are many ways to give back to the project, whether it be writing new code, fixing bugs, or just reporting errors. All forms of contributions are encouraged!

For instructions on how to contribute, see our [Guide to contributing](https://github.com/maidsafe/QA/blob/master/CONTRIBUTING.md).
