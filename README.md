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
[P2P] listening on 127.0.0.1:41644

[P2P] router cmd SayHello(127.0.0.1:41644)

[P2P] Sent network msg successfully.

[P2P] router cmd AddPeer(i:877dca, 127.0.0.1:41644)

> trust i:cb6b
[P2P] router cmd Trust("i:877dca")

[P2P] Trusting actor: i:877dca

[BRB] i:877dca is forcing i:877dca to join

> peers
[P2P] router cmd ListPeers

i:877dca@127.0.0.1:41644 (voting) (self)
```

### Joining an existing Group

`Peer B` starts, adds `A` as peer, and trusts `A`,

(`trust` aka force_join is a BRB Membership local operation)

```
$ cargo run
[P2P] listening on 127.0.0.1:53261

[P2P] router cmd SayHello(127.0.0.1:53261)

[P2P] Sent network msg successfully.

[P2P] router cmd AddPeer(i:3497d6, 127.0.0.1:53261)

> peer 127.0.0.1:41644
[REPL] parsed addr 127.0.0.1:41644

[P2P] router cmd SayHello(127.0.0.1:41644)

[P2P] Sent network msg successfully.

[P2P] router cmd AddPeer(i:877dca, 127.0.0.1:41644)

[P2P] Sent network msg successfully.

> trust i:877dca
[P2P] router cmd Trust("i:877dca")

[P2P] Trusting actor: i:877dca

[BRB] i:3497d6 is forcing i:877dca to join

> peers
[P2P] router cmd ListPeers

i:3497d6@127.0.0.1:53261 (self)
i:877dca@127.0.0.1:41644 (voting)
```

`Peer A` sponsors/proposes `B` to join voting group and members {`A`} vote/agree to include `B`.

(`join` is a BRB Membership p2p voting operation)

note: `B` may have asked `A` to sponsor out of band.  ie, not part of membership protocol.


```
> join i:3497d6
[P2P] router cmd RequestJoin("i:3497d6")

[P2P] Starting join for actor: i:3497d6

[P2P] delivering packet to i:877dca at addr 127.0.0.1:41644: Packet { source: i:877dca, dest: i:877dca, payload: Membership(P(Ji:3497d6)@i:877dcaG1), sig: sig:7c6d35 }

[P2P] Sent network msg successfully.

[P2P] router cmd Apply(Packet { source: i:877dca, dest: i:877dca, payload: Membership(P(Ji:3497d6)@i:877dcaG1), sig: sig:7c6d35 })

[BRB] handling packet from i:877dca->i:877dca

[MBR] Detected super majority

[MBR] broadcasting super majority

[P2P] delivering packet to i:877dca at addr 127.0.0.1:41644: Packet { source: i:877dca, dest: i:877dca, payload: Membership(SM{P(Ji:3497d6)@i:877dcaG1}@i:877dcaG1), sig: sig:0d3413 }

[P2P] Sent network msg successfully.

[P2P] delivering Ack(packet) to i:877dca at addr 127.0.0.1:41644: Packet { source: i:877dca, dest: i:877dca, payload: Membership(P(Ji:3497d6)@i:877dcaG1), sig: sig:7c6d35 }

[P2P] Sent network msg successfully.

[P2P] router cmd Apply(Packet { source: i:877dca, dest: i:877dca, payload: Membership(SM{P(Ji:3497d6)@i:877dcaG1}@i:877dcaG1), sig: sig:0d3413 })

[BRB] handling packet from i:877dca->i:877dca

[MBR] Detected super majority over super majorities

[P2P] delivering Ack(packet) to i:877dca at addr 127.0.0.1:41644: Packet { source: i:877dca, dest: i:877dca, payload: Membership(SM{P(Ji:3497d6)@i:877dcaG1}@i:877dcaG1), sig: sig:0d3413 }

[P2P] Sent network msg successfully.

[P2P] router cmd Acked(Packet { source: i:877dca, dest: i:877dca, payload: Membership(P(Ji:3497d6)@i:877dcaG1), sig: sig:7c6d35 })

[P2P] Got ack for packet Packet { source: i:877dca, dest: i:877dca, payload: Membership(P(Ji:3497d6)@i:877dcaG1), sig: sig:7c6d35 }

[P2P] router cmd Acked(Packet { source: i:877dca, dest: i:877dca, payload: Membership(SM{P(Ji:3497d6)@i:877dcaG1}@i:877dcaG1), sig: sig:0d3413 })

[P2P] Got ack for packet Packet { source: i:877dca, dest: i:877dca, payload: Membership(SM{P(Ji:3497d6)@i:877dcaG1}@i:877dcaG1), sig: sig:0d3413 }


> peers
[P2P] router cmd ListPeers

i:3497d6@127.0.0.1:53261 (voting)
i:877dca@127.0.0.1:41644 (voting) (self)
```

`Peer B` is not yet aware of the vote, so initiates an `enti_entropy` round with `A` to obtain voting history.
After which, `B` knows self to be a voting member of the group.

(`anti_entropy` is a read-only BRB operation)

```
> peers
[P2P] router cmd ListPeers

i:3497d6@127.0.0.1:53261 (self)
i:877dca@127.0.0.1:41644 (voting)

> anti_entropy i:877dca
[P2P] router cmd AntiEntropy("i:877dca")

[P2P] Starting anti-entropy with actor: i:877dca

[P2P] delivering packet to i:877dca at addr 127.0.0.1:41644: Packet { source: i:3497d6, dest: i:877dca, payload: AntiEntropy { generation: 0, delivered: VClock { dots: {} } }, sig: sig:dc0b5e }

[P2P] Sent network msg successfully.

[P2P] router cmd Apply(Packet { source: i:877dca, dest: i:3497d6, payload: Membership(SM{SM{P(Ji:3497d6)@i:877dcaG1}@i:877dcaG1}@i:877dcaG1), sig: sig:4b70ba })

[BRB] handling packet from i:877dca->i:3497d6

[MBR] Detected super majority over super majorities

[MBR] Adding vote to history

[P2P] delivering Ack(packet) to i:3497d6 at addr 127.0.0.1:41644: Packet { source: i:877dca, dest: i:3497d6, payload: Membership(SM{SM{P(Ji:3497d6)@i:877dcaG1}@i:877dcaG1}@i:877dcaG1), sig: sig:4b70ba }

[P2P] Sent network msg successfully.

[P2P] router cmd Acked(Packet { source: i:3497d6, dest: i:877dca, payload: AntiEntropy { generation: 0, delivered: VClock { dots: {} } }, sig: sig:dc0b5e })

[P2P] Got ack for packet Packet { source: i:3497d6, dest: i:877dca, payload: AntiEntropy { generation: 0, delivered: VClock { dots: {} } }, sig: sig:dc0b5e }

> peers
[P2P] router cmd ListPeers

i:3497d6@127.0.0.1:53261 (voting) (self)
i:877dca@127.0.0.1:41644 (voting)
```

### Data Operation

`Peer B` initiates a CRDT (ORSWOT) operation to add element '3' to the data set.
(`add` is a DataType operation, wrapped by BRB)

```
> read
{}

> add 3
[BRB] i:3497d6 initiating bft for msg Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 }

[BRB] broadcasting i:3497d6->{i:3497d6, i:877dca}

[P2P] router cmd Deliver(Packet { source: i:3497d6, dest: i:3497d6, payload: BRB(RequestValidation { msg: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 } }), sig: sig:aaad75 })

[P2P] delivering packet to i:3497d6 at addr 127.0.0.1:53261: Packet { source: i:3497d6, dest: i:3497d6, payload: BRB(RequestValidation { msg: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 } }), sig: sig:aaad75 }

[P2P] Sent network msg successfully.

[P2P] router cmd Deliver(Packet { source: i:3497d6, dest: i:877dca, payload: BRB(RequestValidation { msg: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 } }), sig: sig:aaad75 })

[P2P] delivering packet to i:877dca at addr 127.0.0.1:41644: Packet { source: i:3497d6, dest: i:877dca, payload: BRB(RequestValidation { msg: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 } }), sig: sig:aaad75 }

[P2P] Sent network msg successfully.

[P2P] router cmd Apply(Packet { source: i:3497d6, dest: i:3497d6, payload: BRB(RequestValidation { msg: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 } }), sig: sig:aaad75 })

[BRB] handling packet from i:3497d6->i:3497d6

[BRB] request for validation

[P2P] delivering packet to i:3497d6 at addr 127.0.0.1:53261: Packet { source: i:3497d6, dest: i:3497d6, payload: BRB(SignedValidated { msg: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 }, sig: sig:f1c841 }), sig: sig:e1095c }

[P2P] Sent network msg successfully.

[P2P] delivering Ack(packet) to i:3497d6 at addr 127.0.0.1:53261: Packet { source: i:3497d6, dest: i:3497d6, payload: BRB(RequestValidation { msg: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 } }), sig: sig:aaad75 }

[P2P] Sent network msg successfully.

[P2P] router cmd Apply(Packet { source: i:3497d6, dest: i:3497d6, payload: BRB(SignedValidated { msg: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 }, sig: sig:f1c841 }), sig: sig:e1095c })

[BRB] handling packet from i:3497d6->i:3497d6

[BRB] signed validated

[P2P] delivering Ack(packet) to i:3497d6 at addr 127.0.0.1:53261: Packet { source: i:3497d6, dest: i:3497d6, payload: BRB(SignedValidated { msg: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 }, sig: sig:f1c841 }), sig: sig:e1095c }

[P2P] Sent network msg successfully.

[P2P] router cmd Apply(Packet { source: i:877dca, dest: i:3497d6, payload: BRB(SignedValidated { msg: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 }, sig: sig:fe9a31 }), sig: sig:289701 })

[BRB] handling packet from i:877dca->i:3497d6

[BRB] signed validated

[BRB] we have quorum over msg, sending proof to network

[BRB] broadcasting i:3497d6->{i:3497d6, i:877dca}

[P2P] delivering packet to i:3497d6 at addr 127.0.0.1:53261: Packet { source: i:3497d6, dest: i:3497d6, payload: BRB(ProofOfAgreement { msg: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 }, proof: {i:3497d6: sig:f1c841, i:877dca: sig:fe9a31} }), sig: sig:e8c238 }

[P2P] Sent network msg successfully.

[P2P] delivering packet to i:877dca at addr 127.0.0.1:41644: Packet { source: i:3497d6, dest: i:877dca, payload: BRB(ProofOfAgreement { msg: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 }, proof: {i:3497d6: sig:f1c841, i:877dca: sig:fe9a31} }), sig: sig:e8c238 }

[P2P] Sent network msg successfully.

[P2P] delivering Ack(packet) to i:3497d6 at addr 127.0.0.1:41644: Packet { source: i:877dca, dest: i:3497d6, payload: BRB(SignedValidated { msg: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 }, sig: sig:fe9a31 }), sig: sig:289701 }

[P2P] Sent network msg successfully.

[P2P] router cmd Acked(Packet { source: i:3497d6, dest: i:3497d6, payload: BRB(RequestValidation { msg: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 } }), sig: sig:aaad75 })

[P2P] Got ack for packet Packet { source: i:3497d6, dest: i:3497d6, payload: BRB(RequestValidation { msg: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 } }), sig: sig:aaad75 }

[P2P] router cmd Acked(Packet { source: i:3497d6, dest: i:877dca, payload: BRB(RequestValidation { msg: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 } }), sig: sig:aaad75 })

[P2P] Got ack for packet Packet { source: i:3497d6, dest: i:877dca, payload: BRB(RequestValidation { msg: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 } }), sig: sig:aaad75 }

[P2P] router cmd Acked(Packet { source: i:3497d6, dest: i:3497d6, payload: BRB(SignedValidated { msg: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 }, sig: sig:f1c841 }), sig: sig:e1095c })

[P2P] Got ack for packet Packet { source: i:3497d6, dest: i:3497d6, payload: BRB(SignedValidated { msg: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 }, sig: sig:f1c841 }), sig: sig:e1095c }

[P2P] router cmd Apply(Packet { source: i:3497d6, dest: i:3497d6, payload: BRB(ProofOfAgreement { msg: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 }, proof: {i:3497d6: sig:f1c841, i:877dca: sig:fe9a31} }), sig: sig:e8c238 })

[BRB] handling packet from i:3497d6->i:3497d6

[BRB] proof of agreement: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 }

[P2P] delivering Ack(packet) to i:3497d6 at addr 127.0.0.1:53261: Packet { source: i:3497d6, dest: i:3497d6, payload: BRB(ProofOfAgreement { msg: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 }, proof: {i:3497d6: sig:f1c841, i:877dca: sig:fe9a31} }), sig: sig:e8c238 }

[P2P] Sent network msg successfully.

[P2P] router cmd Acked(Packet { source: i:3497d6, dest: i:877dca, payload: BRB(ProofOfAgreement { msg: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 }, proof: {i:3497d6: sig:f1c841, i:877dca: sig:fe9a31} }), sig: sig:e8c238 })

[P2P] Got ack for packet Packet { source: i:3497d6, dest: i:877dca, payload: BRB(ProofOfAgreement { msg: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 }, proof: {i:3497d6: sig:f1c841, i:877dca: sig:fe9a31} }), sig: sig:e8c238 }

[P2P] router cmd Acked(Packet { source: i:3497d6, dest: i:3497d6, payload: BRB(ProofOfAgreement { msg: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 }, proof: {i:3497d6: sig:f1c841, i:877dca: sig:fe9a31} }), sig: sig:e8c238 })

[P2P] Got ack for packet Packet { source: i:3497d6, dest: i:3497d6, payload: BRB(ProofOfAgreement { msg: Msg { gen: 1, op: Add(i:3497d6.1, [3]), dot: i:3497d6.1 }, proof: {i:3497d6: sig:f1c841, i:877dca: sig:fe9a31} }), sig: sig:e8c238 }

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
