use beserial::{Deserialize, Serialize};
use block_albatross::{BlockExtrinsics, BlockHeader, BlockJustification};
use hash::Blake2bHash;

use bitflags::bitflags;

use std::ops::Deref;

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
pub struct Response<T: Serialize + Deserialize> {
    response_msg: T,
    request_identifier: u32,
}

impl<T: Serialize + Deserialize> Response<T> {
    pub fn new(msg: T, request_identifier: u32) -> Self {
        Response {
            response_msg: msg,
            request_identifier,
        }
    }

    pub fn request_identifier(msg: &Response<T>) -> u32 {
        msg.request_identifier
    }
}

impl<T: Serialize + Deserialize> Deref for Response<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.response_msg
    }
}

bitflags! {
    #[derive(Default, Serialize, Deserialize)]
    pub struct BlockComponentFlags: u8 {
        const HEADER  = 0b0000_0001;
        const JUSTIFICATION = 0b0000_0010;
        const EXTRINSICS = 0b0000_0100;
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockComponents {
    header: Option<BlockHeader>,
    justification: Option<BlockJustification>,
    extrinsics: Option<BlockExtrinsics>,
}
