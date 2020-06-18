// If we don't allow absurd comparisons, clippy fails because `MIN_FULL_NODES` can be 0.
#![allow(clippy::absurd_extreme_comparisons)]

use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::Duration;

use parking_lot::RwLock;

use block_albatross::Block;
use blockchain_albatross::Blockchain;
use blockchain_base::{AbstractBlockchain, BlockchainEvent};
use database::Environment;
use macros::upgrade_weak;
use mempool::{Mempool, MempoolConfig, MempoolEvent};
use network::{Network, NetworkConfig, NetworkEvent, Peer};
use network_primitives::networks::NetworkId;
use network_primitives::time::NetworkTime;
use transaction::Transaction;
use utils::mutable_once::MutableOnce;
use utils::observer::Notifier;
use utils::timers::Timers;

use crate::consensus_agent::ConsensusAgent;
use crate::error::{Error, SyncError};
use crate::sync::SyncProtocol;
use futures::Future;

pub struct Consensus {
    pub blockchain: Arc<Blockchain>,
    pub mempool: Arc<Mempool<Blockchain>>,
    pub network: Arc<Network<Blockchain>>,
    pub env: Environment,

    timers: Timers<ConsensusTimer>,

    pub(crate) state: RwLock<ConsensusState>,

    self_weak: MutableOnce<Weak<Consensus>>,
    pub notifier: RwLock<Notifier<'static, ConsensusEvent>>,

    sync_protocol: Box<dyn SyncProtocol>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConsensusEvent {
    Established,
    Lost,
    Syncing,
    Waiting,
    SyncFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum ConsensusTimer {
    Sync,
}

type ConsensusAgentMap = HashMap<Arc<Peer>, Arc<ConsensusAgent>>;

pub(crate) struct ConsensusState {
    pub(crate) agents: ConsensusAgentMap,
}

impl Consensus {
    pub const MIN_FULL_NODES: usize = 0;
    const SYNC_THROTTLE: Duration = Duration::from_millis(1500);

    pub fn new<S: SyncProtocol>(
        env: Environment,
        network_id: NetworkId,
        network_config: NetworkConfig,
        mempool_config: MempoolConfig,
        sync_protocol: S,
    ) -> Result<Arc<Self>, Error> {
        let network_time = Arc::new(NetworkTime::new());
        let blockchain = Arc::new(Blockchain::new(env.clone(), network_id)?);
        let mempool = Mempool::new(Arc::clone(&blockchain), mempool_config);
        let network = Network::new(
            Arc::clone(&blockchain),
            network_config,
            network_time,
            network_id,
        )?;

        let this = Arc::new(Consensus {
            blockchain,
            mempool,
            network,
            env,

            timers: Timers::new(),

            state: RwLock::new(ConsensusState {
                agents: HashMap::new(),
            }),

            self_weak: MutableOnce::new(Weak::new()),
            notifier: RwLock::new(Notifier::new()),
            sync_protocol: Box::new(sync_protocol),
        });
        Consensus::init_listeners(&this);
        Ok(this)
    }

    pub fn init_listeners(this: &Arc<Consensus>) {
        unsafe { this.self_weak.replace(Arc::downgrade(this)) };

        let weak = Arc::downgrade(this);
        this.network
            .notifier
            .write()
            .register(move |e: &NetworkEvent| {
                let this = upgrade_weak!(weak);
                match e {
                    NetworkEvent::PeerJoined(peer) => this.on_peer_joined(Arc::clone(peer)),
                    NetworkEvent::PeerLeft(peer) => this.on_peer_left(Arc::clone(peer)),
                    _ => {}
                }
            });

        // Relay new (verified) transactions to peers.
        let weak = Arc::downgrade(this);
        this.mempool
            .notifier
            .write()
            .register(move |e: &MempoolEvent| {
                let this = upgrade_weak!(weak);
                match e {
                    MempoolEvent::TransactionAdded(_, transaction) => {
                        this.on_transaction_added(transaction)
                    }
                    // TODO: Relay on restore?
                    MempoolEvent::TransactionRestored(transaction) => {
                        this.on_transaction_added(transaction)
                    }
                    MempoolEvent::TransactionEvicted(transaction) => {
                        this.on_transaction_removed(transaction)
                    }
                    MempoolEvent::TransactionMined(transaction) => {
                        this.on_transaction_removed(transaction)
                    }
                }
            });

        // Notify peers when our blockchain head changes.
        let weak = Arc::downgrade(this);
        this.blockchain
            .register_listener(move |e: &BlockchainEvent<Block>| {
                let this = upgrade_weak!(weak);
                this.on_blockchain_event(e);
            });
    }

    fn on_peer_joined(&self, peer: Arc<Peer>) {
        info!("Connected to {}", peer.peer_address());
        let agent = ConsensusAgent::new(Arc::clone(&peer));

        // If no more peers connect within the specified timeout, start syncing.
        let weak = Weak::clone(&self.self_weak);
        self.timers.reset_delay(
            ConsensusTimer::Sync,
            move || {
                let this = upgrade_weak!(weak);
                tokio::spawn(this.sync_blockchain().map_err(|_| ())); // TODO: Error handling
            },
            Self::SYNC_THROTTLE,
        );

        self.state.write().agents.insert(peer, agent);
    }

    fn on_peer_left(&self, peer: Arc<Peer>) {
        info!("Disconnected from {}", peer.peer_address());
        {
            let mut state = self.state.write();

            state.agents.remove(&peer);
        }

        tokio::spawn(self.sync_blockchain().map_err(|_| ())); // TODO: Error handling
    }

    fn on_blockchain_event(&self, event: &BlockchainEvent<Block>) {
        let state = self.state.read();

        let blocks: Vec<&Block>;
        let block;
        match event {
            BlockchainEvent::Extended(_) | BlockchainEvent::Finalized(_) => {
                // This implicitly takes the lock on the blockchain state.
                block = self.blockchain.head_block();
                blocks = vec![&block];
            }
            BlockchainEvent::Rebranched(_, ref adopted_blocks) => {
                blocks = adopted_blocks.iter().map(|(_, block)| block).collect();
            }
        }

        // print block height
        let height = self.blockchain.head_height();
        if height % 100 == 0 {
            info!("Now at block #{}", height);
        } else {
            trace!("Now at block #{}", height);
        }

        // Only relay blocks if we are synced up.
        if self.sync_protocol.is_established() {
            for agent in state.agents.values() {
                for &block in blocks.iter() {
                    agent.relay_block(block);
                }
            }
        }
    }

    fn on_transaction_added(&self, transaction: &Arc<Transaction>) {
        let state = self.state.read();

        // Don't relay transactions if we are not synced yet.
        if !self.sync_protocol.is_established() {
            return;
        }

        for agent in state.agents.values() {
            agent.relay_transaction(transaction.as_ref());
        }
    }

    fn on_transaction_removed(&self, transaction: &Arc<Transaction>) {
        let state = self.state.read();
        for agent in state.agents.values() {
            agent.remove_transaction(transaction.as_ref());
        }
    }

    fn sync_blockchain(&self) -> impl Future<Item = (), Error = SyncError> {
        self.sync_protocol
            .perform_sync(self.self_weak.upgrade().unwrap())
    }

    pub fn established(&self) -> bool {
        self.sync_protocol.is_established()
    }
}
