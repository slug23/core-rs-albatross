use futures::future::{err, ok};
use futures::Future;
use parking_lot::RwLock;
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::convert::TryFrom;
use std::sync::Arc;

use crate::consensus::{Consensus, ConsensusEvent};
use crate::consensus_agent::ConsensusAgent;
use crate::error::SyncError;
use crate::sync::SyncProtocol;
use block_albatross::Block;
use blockchain_base::AbstractBlockchain;
use network::Peer;
use network_messages::{Objects, RequestBlocksFilter};

#[derive(Default)]
pub struct HistorySyncState {
    sync_peer: Option<Arc<Peer>>,
    established: bool,
}

#[derive(Default)]
pub struct HistorySync {
    state: RwLock<HistorySyncState>,
}

impl HistorySync {
    fn sync_with_peer(
        &self,
        consensus: Arc<Consensus>,
        agent: Arc<ConsensusAgent>,
    ) -> Box<dyn Future<Item = (), Error = SyncError> + 'static + Send> {
        let locators = consensus.blockchain.get_block_locators(100);
        Box::new(
            agent
                .request_blocks(locators, 100, RequestBlocksFilter::All)
                .map_err(|e| SyncError::Other) // TODO: Better error
                .and_then(|blocks| {
                    if let Objects::Objects(blocks) = blocks {
                        for block in blocks {
                            let block = match Block::try_from(block) {
                                Ok(block) => block,
                                Err(_e) => return err(SyncError::Other), // TODO: Better error
                            };
                        }
                    }
                    ok(())
                }),
        )
    }
}

impl SyncProtocol for HistorySync {
    fn perform_sync(
        &self,
        consensus: Arc<Consensus>,
    ) -> Box<dyn Future<Item = (), Error = SyncError> + 'static + Send> {
        let mut state = self.state.write();
        // Wait for ongoing sync to finish.
        if state.sync_peer.is_some() {
            return Box::new(ok(()));
        }

        let mut num_synced_full_nodes: usize = 0;
        let consensus_state = consensus.state.read();
        let candidates: Vec<&Arc<ConsensusAgent>> = consensus_state
            .agents
            .values()
            .filter(|&agent| {
                let is_synced = agent.synced();
                if is_synced && agent.peer.peer_address().services.is_full_node() {
                    num_synced_full_nodes += 1;
                }
                !is_synced
            })
            .collect();

        // Choose a random peer which we aren't sync'd with yet.
        let mut rng = thread_rng();
        let agent = candidates.choose(&mut rng).map(|&agent| agent.clone());
        drop(consensus_state);

        // Report consensus-lost if we are synced with less than the minimum number of full nodes.
        if state.established && num_synced_full_nodes < Consensus::MIN_FULL_NODES {
            state.established = false;
            info!("Consensus lost");
            // FIXME we're still holding state write lock when notifying here.
            consensus.notifier.read().notify(ConsensusEvent::Lost);
        }

        // Do the actual sync.
        if let Some(agent) = agent {
            state.sync_peer = Some(agent.peer.clone());
            let established = state.established;
            drop(state);

            // Notify listeners when we start syncing and have not established consensus yet.
            if !established {
                consensus.notifier.read().notify(ConsensusEvent::Syncing);
            }

            debug!("Syncing blockchain with peer {}", agent.peer.peer_address());
            return self.sync_with_peer(consensus, agent);
        } else {
            // We are synced with all connected peers.
            // Report consensus-established if we are connected to the minimum number of full nodes.
            if num_synced_full_nodes >= Consensus::MIN_FULL_NODES {
                if !state.established {
                    info!(
                        "Synced with all connected peers ({}), consensus established",
                        consensus.state.read().agents.len()
                    );
                    info!(
                        "Blockchain at block #{} [{}]",
                        consensus.blockchain.head_height(),
                        consensus.blockchain.head_hash()
                    );

                    state.established = true;
                    drop(state);

                    // Report consensus-established.
                    consensus
                        .notifier
                        .read()
                        .notify(ConsensusEvent::Established);

                    // Allow inbound network connections after establishing consensus.
                    consensus.network.set_allow_inbound_connections(true);
                }
            } else {
                info!("Waiting for more peer connections...");
                drop(state);

                // Otherwise, wait until more peer connections are established.
                consensus.notifier.read().notify(ConsensusEvent::Waiting);
            }
        }

        Box::new(ok(()))
    }

    fn is_established(&self) -> bool {
        self.state.read().established
    }
}
