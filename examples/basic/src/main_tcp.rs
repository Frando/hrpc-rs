// TODO: Cleanup
#![allow(dead_code)]
#![allow(unused_imports)]

use anyhow::{Error, Result};
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

pub mod codegen {
    include!(concat!(env!("OUT_DIR"), "/codegen.rs"));
}

use codegen::*;

/// Print usage and exit.
fn usage() {
    println!("usage: cargo run --example basic -- [client|server] [port]");
    std::process::exit(1);
}

pub fn main() -> Result<()> {
    env_logger::from_env(Env::default().default_filter_or("info")).init();

    let mode = env::args().nth(1).unwrap_or("server".into());
    let port = env::args().nth(2).unwrap_or(8080.to_string());

    let address = format!("127.0.0.1:{}", port);

    task::block_on(async move {
        match mode.as_ref() {
            "server" => tcp_server(address, onconnection, ()).await,
            "client" => tcp_client(address, onconnection, ()).await,
            _ => panic!(usage()),
        }
    })
}

async fn onconnection(stream: TcpStream, is_initiator: bool, _ctx: ()) -> Result<()> {
    let instant = std::time::Instant::now();
    let (rpc_server, mut rpc_client) = create_server();
    info!("server and client constructed, {:?}", instant.elapsed());

    let client_task = task::spawn(async move {
        if !is_initiator {
            return;
        }
        let instant = std::time::Instant::now();
        let mut shouter = rpc_client.shouter;
        let res = shouter
            .shout(ShoutRequest {
                message: "hello world".into(),
            })
            .await;
        info!("received response after {:?}: {:?}", instant.elapsed(), res);
        let res = shouter
            .shout(ShoutRequest {
                message: "hi moon".into(),
            })
            .await;
        info!("received response after {:?}: {:?}", instant.elapsed(), res);

        let instant = std::time::Instant::now();
        let res = rpc_client.calc.add(AddRequest { a: 0.7, b: 1.3 }).await;
        info!("received response after {:?}: {:?}", instant.elapsed(), res);
    });

    // would be much nicer to not await forever here but return once connected.
    rpc_server.connect(stream).await?;
    client_task.await;
    info!("connection closed, elapsed {:?}", instant.elapsed());
    Ok(())
}

pub fn create_server() -> (Rpc, codegen::client::Client) {
    use std::io::Result;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct App {
        loudness: Mutex<u64>,
    };

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
        async fn add(&mut self, req: AddRequest) -> Result<CalcResponse> {
            Ok(CalcResponse {
                result: req.a + req.b,
            })
        }
    }

    let app = Arc::new(App::default());
    let mut rpc = Rpc::new();
    rpc.define_service(codegen::server::ShouterService::new(app.clone()));
    rpc.define_service(codegen::server::CalcService::new(app));
    let client = rpc.create_client(codegen::client::Client::new());
    (rpc, client)
}

/// A simple async TCP server that calls an async function for each incoming connection.
pub async fn tcp_server<F, C>(
    address: String,
    onconnection: impl Fn(TcpStream, bool, C) -> F + Send + Sync + Copy + 'static,
    context: C,
) -> Result<()>
where
    F: Future<Output = Result<()>> + Send,
    C: Clone + Send + 'static,
{
    let listener = TcpListener::bind(&address).await?;
    log::info!("listening on {}", listener.local_addr()?);
    let mut incoming = listener.incoming();
    while let Some(Ok(stream)) = incoming.next().await {
        let context = context.clone();
        let peer_addr = stream.peer_addr().unwrap();
        log::info!("new connection from {}", peer_addr);
        task::spawn(async move {
            let result = onconnection(stream, false, context).await;
            if let Err(e) = result {
                log::error!("error: {}", e);
            };
            log::info!("connection closed from {}", peer_addr);
        });
    }
    log::info!("server closed");
    Ok(())
}

/// A simple async TCP client that calls an async function when connected.
pub async fn tcp_client<F, C>(
    address: String,
    onconnection: impl Fn(TcpStream, bool, C) -> F + Send + Sync + Copy + 'static,
    context: C,
) -> Result<()>
where
    F: Future<Output = Result<()>> + Send,
    C: Clone + Send + 'static,
{
    let stream = TcpStream::connect(&address).await?;
    log::info!("connected to {}", &address);
    onconnection(stream, true, context).await
}

async fn timeout(ms: u64) {
    let _ = async_std::future::timeout(
        std::time::Duration::from_millis(ms),
        futures::future::pending::<()>(),
    )
    .await;
}
