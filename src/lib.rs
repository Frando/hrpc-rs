#![recursion_limit = "256"]

mod decode;
mod message;
#[macro_use]
mod rpc;
mod sessions;
pub mod transport;

pub use decode::Decoder;
pub use message::Message;
pub use rpc::{
    Client, RawRequestFuture, Request, RequestFuture, Response, Rpc, RpcClient, Service,
};

pub mod messages {
    include!(concat!(env!("OUT_DIR"), "/hrpc.rs"));
}

pub mod encoding {
    pub type Void = ();
}
