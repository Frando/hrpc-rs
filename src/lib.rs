#![recursion_limit = "256"]

pub mod codegen;
mod decode;
mod message;
#[macro_use]
mod rpc;

pub use decode::Decoder;
pub use message::Message;
pub use rpc::{Client, Server};
