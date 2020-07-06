use prost::Message as ProstMessage;
use std::io::{Error, ErrorKind, Result};

pub trait EncodeBody: ProstMessage + Sized {
    fn encode_body(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.encoded_len());
        // Should be safe because only error is insufficient capacity,
        // and we just created the buf with the correct len.
        self.encode(&mut buf).unwrap();
        // encode_body(self)
        buf
    }
}
impl<T> EncodeBody for T where T: ProstMessage {}
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

pub fn encode_body<T>(body: T) -> Vec<u8>
where
    T: EncodeBody,
{
    body.encode_body()
    // let mut buf = Vec::with_capacity(body.encoded_len());
    // Should be safe because only error is insufficient capacity,
    // and we just created the buf with the correct len.
    // body.encode(&mut buf).unwrap();
    // buf
}

#[derive(Debug, Clone)]
pub enum MessageType {
    Request,
    Response,
    Error,
}

#[derive(Debug)]
pub struct Message {
    pub typ: MessageType,
    pub service: u64,
    pub method: u64,
    pub id: u64,
    pub body: Vec<u8>,
}

impl Message {
    pub fn request(service: u64, method: u64, body: Vec<u8>) -> Self {
        Self {
            typ: MessageType::Request,
            service,
            method,
            id: 0,
            body,
        }
    }

    pub fn error(request: Message, body: impl ToString) -> Self {
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
            Err(error) => Message::error(self, error),
            Ok(body) => Message::response(self, encode_body(body)),
        }
    }

    pub fn response(request: Message, body: Vec<u8>) -> Self {
        Self {
            typ: MessageType::Response,
            id: request.id,
            service: request.service,
            method: request.method,
            body,
        }
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
    // pub fn set_typ(&mut self, typ: MessageType) -> Result<()> {
    //     self.typ = match typ {
    //         MessageType::Request => 0,
    //         MessageType::Response => 1,
    //         MessageType::Error => 2,
    //         _ => {
    //             return Err(Error::new(ErrorKind::Other, "Invalid message type"));
    //         }
    //     };
    //     Ok(())
    // }

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

    pub fn set_request(&mut self, id: u64) {
        self.id = id;
        self.typ = MessageType::Request;
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

    fn header(&self) -> u64 {
        (self.service << 2) | self.typ_u8() as u64
    }

    fn body_len(&self) -> usize {
        varinteger::length(self.header())
            + varinteger::length(self.method)
            + varinteger::length(self.id)
            + self.body.len()
    }
    pub fn encoded_len(&self) -> usize {
        let body_len = self.body_len();
        let len = body_len + varinteger::length(body_len as u64);
        len
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
}
