use prost::Message as ProstMessage;
use std::fmt;
use std::io::{Error, ErrorKind, Result};

/// Encode a value into a byte vector.
pub trait EncodeBody: Sized {
    fn encode_body(&self) -> Vec<u8>;
}
impl<T> EncodeBody for T
where
    T: ProstMessage,
{
    fn encode_body(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.encoded_len());
        // Should be safe because only error is insufficient capacity,
        // and we just created the buf with the correct len.
        self.encode(&mut buf).unwrap();
        // encode_body(self)
        buf
    }
}

/// Message type
#[derive(Debug, Clone)]
pub enum MessageType {
    Request,
    Response,
    Error,
}

type Address = (u64, u64, u64);

/// HRPC wire message
pub struct Message {
    pub typ: MessageType,
    pub service: u64,
    pub method: u64,
    pub id: u64,
    pub body: Vec<u8>,
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let body = match self.typ {
            MessageType::Error => String::from_utf8(self.body.clone()).unwrap(),
            _ => format!("[{}]", self.body.len()),
        };
        write!(
            f,
            "Message {{ {:?}, s {} m {} id {}, body {} }}",
            self.typ, self.service, self.method, self.id, body
        )
    }
}

impl Message {
    pub fn new_request(service: u64, method: u64, body: impl EncodeBody) -> Self {
        Self {
            typ: MessageType::Request,
            service,
            method,
            id: 0,
            body: body.encode_body(),
        }
    }

    pub fn new_response(request: Message, body: impl EncodeBody) -> Self {
        Self {
            typ: MessageType::Response,
            id: request.id,
            service: request.service,
            method: request.method,
            body: body.encode_body(),
        }
    }

    pub fn new_error(request: Message, body: impl ToString) -> Self {
        Self {
            typ: MessageType::Error,
            id: request.id,
            service: request.service,
            method: request.method,
            body: body.to_string().into(),
        }
    }

    pub fn into_response(self, body: Result<impl EncodeBody>) -> Self {
        match body {
            Err(error) => Message::new_error(self, error),
            Ok(body) => Message::new_response(self, body.encode_body()),
        }
    }

    pub fn prepare_response(body: impl EncodeBody) -> Self {
        Self {
            typ: MessageType::Response,
            body: body.encode_body(),
            service: 0,
            method: 0,
            id: 0,
        }
    }

    pub fn prepare_error(body: impl ToString) -> Self {
        Self {
            typ: MessageType::Error,
            body: body.to_string().as_bytes().to_vec(),
            service: 0,
            method: 0,
            id: 0,
        }
    }

    pub fn address(&self) -> Address {
        (self.service, self.method, self.id)
    }

    pub fn set_address(&mut self, address: Address) {
        self.service = address.0;
        self.method = address.1;
        self.id = address.2;
    }

    pub fn from_raw(header: u64, method: u64, id: u64, body: Vec<u8>) -> Result<Self> {
        let typ = header & 3;
        let service = header >> 2;
        let typ = match typ {
            0 => MessageType::Request,
            1 => MessageType::Response,
            2 => MessageType::Error,
            _ => {
                return Err(Error::new(ErrorKind::Other, "Invalid message type"));
            }
        };
        Ok(Message {
            typ,
            service,
            method,
            id,
            body,
        })
    }

    pub fn is_request(&self) -> bool {
        match self.typ {
            MessageType::Request => true,
            _ => false,
        }
    }

    pub fn is_response(&self) -> bool {
        match self.typ {
            MessageType::Response => true,
            _ => false,
        }
    }

    pub fn is_error(&self) -> bool {
        match self.typ {
            MessageType::Error => true,
            _ => false,
        }
    }

    pub fn method(&self) -> u64 {
        self.method
    }

    pub fn service(&self) -> u64 {
        self.service
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn typ(&self) -> MessageType {
        self.typ.clone()
    }

    pub fn typ_u8(&self) -> u8 {
        match self.typ {
            MessageType::Request => 0,
            MessageType::Response => 1,
            MessageType::Error => 2,
        }
    }

    pub fn error(&self) -> Option<Error> {
        if !self.is_error() {
            return None;
        }
        // TODO: Error decoding.
        let error_message = String::from_utf8(self.body.clone())
            .unwrap_or("Error: Cannot decode error message".into());
        Some(Error::new(ErrorKind::Other, error_message))
    }

    pub fn body<T>(&self) -> Result<T>
    where
        T: ProstMessage + Default,
    {
        if let Some(error) = self.error() {
            Err(error)
        } else {
            T::decode(&self.body[..])
                .map_err(|e| Error::new(ErrorKind::InvalidData, format!("Decode error: {}", e)))
        }
    }

    pub(crate) fn set_request(&mut self, id: u64) {
        self.id = id;
        self.typ = MessageType::Request;
    }

    pub fn encode(&self, buf: &mut [u8]) -> Result<()> {
        let mut offset = 0;
        offset += varinteger::encode(self.body_len() as u64, &mut buf[offset..]);
        offset += varinteger::encode(self.header(), &mut buf[offset..]);
        offset += varinteger::encode(self.method, &mut buf[offset..]);
        offset += varinteger::encode(self.id, &mut buf[offset..]);
        &mut buf[offset..].copy_from_slice(&self.body);
        Ok(())
    }

    pub(crate) fn header(&self) -> u64 {
        (self.service << 2) | self.typ_u8() as u64
    }

    pub fn encoded_len(&self) -> usize {
        let body_len = self.body_len();
        let len = body_len + varinteger::length(body_len as u64);
        len
    }

    fn body_len(&self) -> usize {
        varinteger::length(self.header())
            + varinteger::length(self.method)
            + varinteger::length(self.id)
            + self.body.len()
    }
}

// impl<T> std::convert::TryFrom<Message> for T
// where
//     T: ProstMessage,
// {
//     type Error = prost::DecodeError;
//     fn try_from(message: Message) -> std::result::Result<T, Self::Error> {
//         T::decode(&message.body)
//     }
// }

// pub trait DecodeBody: Sized {
//     fn decode_body(body: &[u8]) -> Result<Self>;
// }

// impl<T> DecodeBody for T
// where
//     T: ProstMessage + Default,
// {
//     fn decode_body(body: &[u8]) -> Result<Self> {
//         let body = T::decode(body)?;
//         Ok(body)
//     }
// }

// impl<T> EncodeBody for T
// where
//     T: From<Vec<u8>>,
// {
//     fn encode_body(&self) -> Vec<u8> {
//         self.into()
//     }
// }
// impl EncodeBody for &[u8] {
//     fn encode_body(&self) -> Vec<u8> {
//         self.to_vec()
//     }
// }

// pub type HrpcResult<T> = Result<dyn EncodeBody, dyn std::error::Error>;

// pub trait DecodeBody {
//     fn decode_body<T>(&self, buf: &[u8]) -> Result<T>
//     where
//         T: EncodeBody;
// }

// impl DecodeBody for T
// where
//     T: ProstMessage,
// {
//     fn decode_body(&self, buf: &[u8]) -> Result<T> {
//         T::decode(buf)
//     }
// }
// impl EncodeBody for T where T: ProstMessage {}

// pub trait EncodeToVec: ProstMessage {
//     fn encode_body(&self) -> Result<Vec<u8>>;
// }
// impl<T> EncodeToVec for T
// where
//     T: ProstMessage,
// {
//     fn encode_body(&self) -> Result<Vec<u8>> {
//         let mut buf = Vec::with_capacity(self.encoded_len());
//         self.encode(&mut buf)?;
//         Ok(buf)
//     }
// }

// pub fn encode_body<T>(body: T) -> Vec<u8>
// where
//     T: EncodeBody,
// {
//     body.encode_body()
//     // let mut buf = Vec::with_capacity(body.encoded_len());
//     // Should be safe because only error is insufficient capacity,
//     // and we just created the buf with the correct len.
//     // body.encode(&mut buf).unwrap();
//     // buf
// }
