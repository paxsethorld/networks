//based on https://github.com/libp2p/rust-libp2p/blob/master/examples/distributed-key-value-store.rs

//! A basic key value store demonstrating libp2p and the mDNS and Kademlia protocols.
//!
//! 1. Using two terminal windows, start two instances. If you local network
//!    allows mDNS, they will automatically connect.
//!
//! 2. Type `PUT my-key my-value` in terminal one and hit return.
//!
//! 3. Type `GET my-key` in terminal two and hit return.
//!
//! 4. Close with Ctrl-c.
//!
//! You can also store provider records instead of key value records.
//!
//! 1. Using two terminal windows, start two instances. If you local network
//!    allows mDNS, they will automatically connect.
//!
//! 2. Type `PUT_PROVIDER my-key` in terminal one and hit return.
//!
//! 3. Type `GET_PROVIDERS my-key` in terminal two and hit return.
//!
//! 4. Close with Ctrl-c.

use async_std::{io, task};
use futures::{prelude::*, select};
use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{
    record::Key, AddProviderOk, Kademlia, KademliaEvent, PeerRecord, PutRecordOk, QueryResult,
    Quorum, Record,
};
use libp2p::{
    development_transport, identity,
    mdns::{Mdns, MdnsConfig, MdnsEvent},
    swarm::{NetworkBehaviourEventProcess, SwarmEvent},
    NetworkBehaviour, PeerId, Swarm,
};
use std::error::Error;

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    // Create a random key for ourselves.
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("key and id");

    // Set up a an encrypted DNS-enabled TCP Transport over the Mplex protocol.
    let transport = development_transport(local_key).await?;
    println!("transport");

    // We create a custom network behaviour that combines Kademlia and mDNS.
    #[derive(NetworkBehaviour)]
    #[behaviour(event_process = true)]
    struct MyBehaviour {
        kademlia: Kademlia<MemoryStore>,
        mdns: Mdns,
    }

    impl NetworkBehaviourEventProcess<MdnsEvent> for MyBehaviour {
        // Called when `mdns` produces an event.
        fn inject_event(&mut self, event: MdnsEvent) {
            if let MdnsEvent::Discovered(list) = event {
                for (peer_id, multiaddr) in list {
                    self.kademlia.add_address(&peer_id, multiaddr);
                }
            }
        }
    }

    impl NetworkBehaviourEventProcess<KademliaEvent> for MyBehaviour {
        // Called when `kademlia` produces an event.
        fn inject_event(&mut self, message: KademliaEvent) {
            match message {
                KademliaEvent::OutboundQueryCompleted { result, .. } => match result {
                    QueryResult::GetProviders(Ok(ok)) => {
                        for peer in ok.providers {
                            println!(
                                "Peer {:?} provides key {:?}",
                                peer,
                                std::str::from_utf8(ok.key.as_ref()).unwrap()
                            );
                        }
                    }
                    QueryResult::GetProviders(Err(err)) => {
                        eprintln!("Failed to get providers: {:?}", err);
                    }
                    QueryResult::GetRecord(Ok(ok)) => {
                        for PeerRecord {
                            record: Record { key, value, .. },
                            ..
                        } in ok.records
                        {
                            println!(
                                "Got record {:?} {:?}",
                                std::str::from_utf8(key.as_ref()).unwrap(),
                                std::str::from_utf8(&value).unwrap(),
                            );
                        }
                    }
                    QueryResult::GetRecord(Err(err)) => {
                        eprintln!("Failed to get record: {:?}", err);
                    }
                    QueryResult::PutRecord(Ok(PutRecordOk { key })) => {
                        println!(
                            "Successfully put record {:?}",
                            std::str::from_utf8(key.as_ref()).unwrap()
                        );
                    }
                    QueryResult::PutRecord(Err(err)) => {
                        eprintln!("Failed to put record: {:?}", err);
                    }
                    QueryResult::StartProviding(Ok(AddProviderOk { key })) => {
                        println!(
                            "Successfully put provider record {:?}",
                            std::str::from_utf8(key.as_ref()).unwrap()
                        );
                    }
                    QueryResult::StartProviding(Err(err)) => {
                        eprintln!("Failed to put provider record: {:?}", err);
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }
    println!("behaviour");

    // Create a swarm to manage peers and events.
    let mut swarm = {
        // Create a Kademlia behaviour.
        let store = MemoryStore::new(local_peer_id);
        let kademlia = Kademlia::new(local_peer_id, store);
        let mdns = task::block_on(Mdns::new(MdnsConfig::default()))?;
        let behaviour = MyBehaviour { kademlia, mdns };
        Swarm::new(transport, behaviour, local_peer_id)
    };
    println!("swarm");

    // Read full lines from stdin
    let mut stdin = io::BufReader::new(io::stdin()).lines().fuse();
    println!("reading input lines");

    // Listen on all interfaces and whatever port the OS assigns.
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;
    println!("listening on ports");

    // Kick it off.
    loop {
        select! {
            line = stdin.select_next_some() => handle_input_line(&mut swarm.behaviour_mut().kademlia, line.expect("Stdin not to close")),
            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Listening in {:?}", address);
                },
                _ => {}
            }
        }
        println!("looping");
    }
}

fn handle_input_line(kademlia: &mut Kademlia<MemoryStore>, line: String) {

    println!("handle_input_line");
    let mut args = line.split(' ');

    match args.next() {
        Some("GET") => {
            let key = {
                match args.next() {
                    Some(key) => Key::new(&key),
                    None => {
                        eprintln!("Expected key");
                        return;
                    }
                }
            };
            kademlia.get_record(key, Quorum::One);
        }
        Some("GET_PROVIDERS") => {
            let key = {
                match args.next() {
                    Some(key) => Key::new(&key),
                    None => {
                        eprintln!("Expected key");
                        return;
                    }
                }
            };
            kademlia.get_providers(key);
        }
        Some("PUT") => {
            let key = {
                match args.next() {
                    Some(key) => Key::new(&key),
                    None => {
                        eprintln!("Expected key");
                        return;
                    }
                }
            };
            let value = {
                match args.next() {
                    Some(value) => value.as_bytes().to_vec(),
                    None => {
                        eprintln!("Expected value");
                        return;
                    }
                }
            };
            let record = Record {
                key,
                value,
                publisher: None,
                expires: None,
            };
            kademlia
                .put_record(record, Quorum::One)
                .expect("Failed to store record locally.");
        }
        Some("PUT_PROVIDER") => {
            let key = {
                match args.next() {
                    Some(key) => Key::new(&key),
                    None => {
                        eprintln!("Expected key");
                        return;
                    }
                }
            };

            kademlia
                .start_providing(key)
                .expect("Failed to start providing key");
        }
        _ => {
            eprintln!("expected GET, GET_PROVIDERS, PUT or PUT_PROVIDER");
        }
    }
}