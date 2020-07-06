// TODO: Autogenerate this from a .proto file.

use async_trait::async_trait;
use prost::Message as ProstMessage;
use std::io::{Error, ErrorKind, Result};

use crate::error_other;
use crate::message::encode_body;
use crate::rpc::{RpcClient, Service};
pub use schema::*;

pub mod schema {
    include!(concat!(env!("OUT_DIR"), "/codegen.rs"));
}

pub mod server {
    use super::*;
    #[async_trait]
    pub trait Shouter: Send + 'static {
        async fn shout(&mut self, _req: ShoutRequest) -> Result<ShoutResponse> {
            error_other!("Method not implemented")
        }
    }

    pub struct ShouterServer {
        inner: Box<dyn Shouter>,
    }

    impl ShouterServer {
        pub fn new(inner: impl Shouter + 'static) -> Self {
            Self {
                inner: Box::new(inner),
            }
        }
    }

    #[async_trait]
    impl Service for ShouterServer {
        fn id(&self) -> u64 {
            1
        }
        async fn handle_request(&mut self, method: u64, body: &Vec<u8>) -> Result<Vec<u8>> {
            match method {
                1 => {
                    let req = ShoutRequest::decode(&body[..])?;
                    let res = self.inner.shout(req).await?;
                    Ok(encode_body(res))
                }
                _ => error_other!("Invalid method ID"),
            }
        }
    }

    // #[async_trait]
    // pub trait CalcService {
    //     const ID: u64 = 2;
    //     fn __id(&self) -> u64 {
    //         Self::ID
    //     }
    //     async fn add(&mut self, _req: AddRequest) -> Result<CalcResponse> {
    //         error_other!("Method not implemented")
    //     }
    //     async fn square(&mut self, _req: SquareRequest) -> Result<CalcResponse> {
    //         error_other!("Method not implemented")
    //     }
    // }
}

pub mod client {
    use super::*;
    use crate::rpc;

    pub struct Client {
        pub shouter: Shouter,
    }
    impl Client {
        pub fn new() -> (Self, rpc::ClientBuilder) {
            let builder = rpc::ClientBuilder::new();
            let shouter = Shouter {
                client: builder.create_client(),
            };
            let app_client = Self { shouter };
            (app_client, builder)
        }
    }

    impl RpcClient for Client {}

    #[derive(Clone)]
    pub struct Shouter {
        client: rpc::Client,
    }
    impl Shouter {
        const ID: u64 = 1;
        pub async fn shout(&mut self, req: ShoutRequest) -> Result<ShoutResponse> {
            self.client
                .request(Self::ID, 1, req)
                .await
                .and_then(|message| ShoutResponse::decode(&message[..]).map_err(|e| e.into()))
        }
    }
}
