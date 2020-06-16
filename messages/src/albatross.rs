use std::ops::Deref;

use beserial::{Deserialize, Serialize};
use hash::Blake2bHash;

use crate::Message;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(u8)]
pub enum Objects<T: Serialize + Deserialize> {
    #[beserial(discriminant = 0)]
    Hashes(#[beserial(len_type(u16))] Vec<Blake2bHash>),
    #[beserial(discriminant = 1)]
    Objects(#[beserial(len_type(u16))] Vec<T>),
}

impl<T: Serialize + Deserialize> Objects<T> {
    pub const MAX_HASHES: usize = 1000;
    pub const MAX_OBJECTS: usize = 1000;

    pub fn with_objects(objects: Vec<T>) -> Self {
        Objects::Objects(objects)
    }

    pub fn with_hashes(hashes: Vec<Blake2bHash>) -> Self {
        Objects::Hashes(hashes)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestResponse<T: Serialize + Deserialize> {
    pub msg: T,
    request_identifier: u32,
}

impl<T: Serialize + Deserialize> RequestResponse<T> {
    pub fn new(msg: T, request_identifier: u32) -> Self {
        RequestResponse {
            msg,
            request_identifier,
        }
    }

    pub fn request_identifier(msg: &RequestResponse<T>) -> u32 {
        msg.request_identifier
    }
}

impl<T: Serialize + Deserialize> Deref for RequestResponse<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.msg
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[repr(u8)]
pub enum RequestBlocksFilter {
    All = 1,
    MacroOnly = 2,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestBlocksMessage {
    #[beserial(len_type(u16, limit = 128))]
    pub locators: Vec<Blake2bHash>,
    pub max_blocks: u16,
    pub filter: RequestBlocksFilter,
}

impl RequestBlocksMessage {
    pub fn new(
        locators: Vec<Blake2bHash>,
        max_blocks: u16,
        filter: RequestBlocksFilter,
        request_identifier: u32,
    ) -> Message {
        Message::RequestBlocks(Box::new(RequestResponse::new(
            Self {
                locators,
                max_blocks,
                filter,
            },
            request_identifier,
        )))
    }
}
