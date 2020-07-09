// TODO: Cleanup
#![allow(dead_code)]
#![allow(unused_imports)]

use async_std::io;
use async_std::net::{TcpListener, TcpStream};
use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task;
use async_trait::async_trait;
use env_logger::Env;
use futures::stream::StreamExt;
use hrpc::{Client, Decoder, Rpc};
use log::*;
use std::collections::HashMap;
use std::env;
use std::io::{Error, Result};
use std::sync::Mutex;

pub mod codegen {
    include!(concat!(env!("OUT_DIR"), "/codegen.rs"));
}

use codegen::*;

/// Print usage and exit.
fn usage() {
    println!("usage: cargo run --example basic -- [client|server] [port]");
    std::process::exit(1);
}

#[async_std::main]
pub async fn main() -> Result<()> {
    env_logger::from_env(Env::default().default_filter_or("info")).init();

    let mode = env::args().nth(1).unwrap_or("server".into());
    let path = "/tmp/hrpc.sock";

    match mode.as_ref() {
        "server" => uds_server(path).await,
        "client" => {
            let mut client = uds_client(path).await.unwrap();
            let res = client
                .shouter
                .shout(ShoutRequest {
                    message: "hello uds".into(),
                })
                .await?;
            info!("got response: {:?}", res);
            Ok(())
        }
        _ => panic!(usage()),
    }
}

#[derive(Default)]
struct App {
    loudness: Mutex<u64>,
}

#[async_trait]
impl codegen::server::Shouter for Arc<App> {
    async fn shout(&mut self, req: ShoutRequest) -> Result<ShoutResponse> {
        info!("Shouter::shout {:?}", req);
        let mut loudness = self.loudness.lock().unwrap();
        *loudness += 1;
        Ok(ShoutResponse {
            message: req.message.to_uppercase(),
            loudness: *loudness,
        })
    }
}

#[async_trait]
impl codegen::server::Calc for Arc<App> {
    async fn add(&mut self, req: AddRequest) -> std::io::Result<CalcResponse> {
        Ok(CalcResponse {
            result: req.a + req.b,
        })
    }
}

fn define_rpc(rpc: &mut Rpc, app: Arc<App>) -> codegen::client::Client {
    rpc.define_service(codegen::server::ShouterServer::new(app.clone()));
    rpc.define_service(codegen::server::CalcServer::new(app));
    let client = rpc.create_client(codegen::client::Client::new());
    client
}

async fn uds_server(path: &str) -> Result<()> {
    let app = Arc::new(App::default());
    let mut incoming =
        hrpc::transport::uds::accept(path, Box::new(move |rpc| define_rpc(rpc, app.clone())))
            .await?;
    while let Some(_client) = incoming.next().await {
        eprintln!("got new connection");
    }
    Ok(())
}

async fn uds_client(path: &str) -> Result<codegen::client::Client> {
    let app = Arc::new(App::default());
    let client =
        hrpc::transport::uds::connect(path, Box::new(move |rpc| define_rpc(rpc, app.clone())))
            .await?;
    Ok(client)
}
