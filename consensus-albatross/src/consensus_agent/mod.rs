use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::Duration;

use beserial::Serialize;
use block_albatross::BlockComponents;
use hash::Blake2bHash;
use network::Peer;
use network_messages::{
    Message, MessageDistributor, MessageType, Objects, RequestBlocksFilter, RequestBlocksMessage,
    RequestResponse,
};
use network_primitives::subscription_albatross::Subscription;
use utils::mutable_once::MutableOnce;
use utils::observer::weak_passthru_listener;
use utils::rate_limit::RateLimit;
use utils::timers::Timers;

use crate::consensus_agent::response_future::Response;

pub mod response_future;

pub struct ConsensusAgentState {
    current_request_identifier: u32,

    /// Responses to `Blocks` requests.
    block_responses: HashMap<u32, Response<Objects<BlockComponents>>>,

    local_subscription: Subscription,
}

impl ConsensusAgentState {
    fn get_request_identifier(&mut self) -> u32 {
        let id = self.current_request_identifier;
        self.current_request_identifier += 1;
        id
    }
}

#[derive(Ord, PartialOrd, PartialEq, Eq, Hash, Clone, Copy, Debug)]
enum ConsensusAgentTimer {
    Mempool,
    ResyncThrottle,
    RequestTimeout(u32),
}

pub struct ConsensusAgent {
    pub peer: Arc<Peer>,

    pub(crate) state: RwLock<ConsensusAgentState>,

    timers: Timers<ConsensusAgentTimer>,
    msg_distributor: Mutex<MessageDistributor>,
    self_weak: MutableOnce<Weak<ConsensusAgent>>,
}

impl ConsensusAgent {
    pub fn new(peer: Arc<Peer>) -> Arc<Self> {
        let agent = Arc::new(ConsensusAgent {
            peer,
            state: RwLock::new(ConsensusAgentState {
                current_request_identifier: 0,
                block_responses: Default::default(),
                local_subscription: Default::default(),
            }),
            timers: Timers::new(),
            msg_distributor: Default::default(),
            self_weak: MutableOnce::new(Weak::new()),
        });
        ConsensusAgent::init_listeners(&agent);
        agent
    }

    fn init_listeners(this: &Arc<ConsensusAgent>) {
        unsafe { this.self_weak.replace(Arc::downgrade(this)) };

        this.peer
            .channel
            .msg_notifier
            .bypass_notifier
            .write()
            .register(weak_passthru_listener(Arc::downgrade(this), |this, msg| {
                this.on_message(msg)
            }));
    }

    fn on_message(&self, msg: Message) {
        match msg {
            Message::Blocks(blocks) => self.on_blocks(blocks),
            msg => self.msg_distributor.lock().notify_and_cleanup(msg),
        }
    }

    pub fn request_blocks(
        &self,
        locators: Vec<Blake2bHash>,
        max_blocks: u16,
        filter: RequestBlocksFilter,
    ) -> Response<Objects<BlockComponents>> {
        let mut state = self.state.write();
        let request_identifier = state.get_request_identifier();

        self.peer.channel.send_or_close(RequestBlocksMessage::new(
            locators,
            max_blocks,
            filter,
            request_identifier,
        ));

        let (r1, r2) = Response::new();
        state.block_responses.insert(request_identifier, r2);
        self.timers.set_delay(
            ConsensusAgentTimer::RequestTimeout(request_identifier),
            || {},
            Duration::new(1, 0),
        ); // TODO: Set duration

        r1
    }

    fn on_blocks(&self, msg: Box<RequestResponse<Objects<BlockComponents>>>) {
        let request_identifier = RequestResponse::request_identifier(&msg);
        self.timers
            .clear_delay(&ConsensusAgentTimer::RequestTimeout(request_identifier));

        let mut state = self.state.write();
        let response = state.block_responses.remove(&request_identifier);

        if let Some(response) = response {
            response.set(Ok(msg.msg));
        } else {
            // TODO
        }
    }
}
