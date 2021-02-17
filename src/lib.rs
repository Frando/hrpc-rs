//! hrpc is a bidirectional binary RPC protocol.
//!
//! It is a quite simple protocol, and this implementation supports automatic code generation
//! from protocol buffer message and service definitions.
//! See [`hrpc_build`][1] for docs for the code generation.
//!
//! [1]: /hrpc_build

#![recursion_limit = "256"]

mod decode;
mod message;
#[macro_use]
mod rpc;
pub mod sessions;

// next module - a redesign of the core API.
pub mod next;

pub use decode::Decoder;
pub use message::Message;
pub use rpc::{
    Client, RawRequestFuture, Request, RequestFuture, Response, Rpc, RpcClient, Service,
};

pub mod messages {
    include!(concat!(env!("OUT_DIR"), "/error.rs"));
}

pub mod encoding {
    pub type Void = ();
}
