use beserial::{Deserialize, ReadBytesExt, Serialize, SerializingError};

use std::ops::Deref;

#[derive(Clone, Debug, Serialize)]
pub struct CompatibleRequestResponse<T: Serialize + Deserialize> {
    pub msg: T,
    request_identifier: Option<u32>,
}

impl<T: Serialize + Deserialize> CompatibleRequestResponse<T> {
    pub fn new(msg: T, request_identifier: Option<u32>) -> Self {
        CompatibleRequestResponse {
            msg,
            request_identifier,
        }
    }

    pub fn request_identifier(msg: &CompatibleRequestResponse<T>) -> Option<u32> {
        msg.request_identifier
    }
}

impl<T: Serialize + Deserialize> Deref for CompatibleRequestResponse<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.msg
    }
}

impl<T: Serialize + Deserialize> Deserialize for CompatibleRequestResponse<T> {
    fn deserialize<R: ReadBytesExt>(reader: &mut R) -> Result<Self, SerializingError> {
        // For compatibility with old messages, we set the id to 0 if none is given.
        let request_identifier: Option<u32> = match Deserialize::deserialize(reader) {
            Ok(i) => Some(i),
            Err(SerializingError::IoError(std::io::ErrorKind::UnexpectedEof, _)) => None,
            Err(e) => return Err(e),
        };
        let msg: T = Deserialize::deserialize(reader)?;
        Ok(CompatibleRequestResponse {
            msg: msg,
            request_identifier,
        })
    }
}
