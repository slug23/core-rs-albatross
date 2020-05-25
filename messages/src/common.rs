use beserial::{Deserialize, ReadBytesExt, Serialize, SerializingError};

use std::ops::Deref;

#[derive(Clone, Debug, Serialize)]
pub struct CompatibleResponse<T: Serialize + Deserialize> {
    response_msg: T,
    request_identifier: u32,
}

impl<T: Serialize + Deserialize> CompatibleResponse<T> {
    pub fn new(msg: T, request_identifier: u32) -> Self {
        CompatibleResponse {
            response_msg: msg,
            request_identifier,
        }
    }

    pub fn request_identifier(msg: &CompatibleResponse<T>) -> u32 {
        msg.request_identifier
    }
}

impl<T: Serialize + Deserialize> Deref for CompatibleResponse<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.response_msg
    }
}

impl<T: Serialize + Deserialize> Deserialize for CompatibleResponse<T> {
    fn deserialize<R: ReadBytesExt>(reader: &mut R) -> Result<Self, SerializingError> {
        // For compatibility with old messages, we set the id to 0 if none is given.
        let request_identifier: u32 = match Deserialize::deserialize(reader) {
            Ok(i) => i,
            Err(SerializingError::IoError(std::io::ErrorKind::UnexpectedEof, _)) => 0,
            Err(e) => return Err(e),
        };
        let msg: T = Deserialize::deserialize(reader)?;
        Ok(CompatibleResponse {
            response_msg: msg,
            request_identifier,
        })
    }
}
