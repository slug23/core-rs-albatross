use crate::Message;
use beserial::{Deserialize, Serialize};
use hash::Blake2bHash;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[repr(u8)]
pub enum GetBlocksDirection {
    Forward = 1,
    Backward = 2,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetBlocksMessage {
    #[beserial(len_type(u16))]
    pub locators: Vec<Blake2bHash>,
    pub max_inv_size: u16,
    pub direction: GetBlocksDirection,
}
impl GetBlocksMessage {
    pub const LOCATORS_MAX_COUNT: usize = 128;

    pub fn new(
        locators: Vec<Blake2bHash>,
        max_inv_size: u16,
        direction: GetBlocksDirection,
    ) -> Message {
        Message::GetBlocks(Box::new(Self {
            locators,
            max_inv_size,
            direction,
        }))
    }
}
