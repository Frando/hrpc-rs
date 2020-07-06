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
use log::*;
use std::collections::HashMap;
use std::env;

use hrpc::codegen::*;
use hrpc::{Client, Decoder, Server};

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

async fn onconnection(stream: TcpStream, _is_initiator: bool, _ctx: ()) -> Result<()> {
    let instant = std::time::Instant::now();
    let (mut rpc_server, mut rpc_client) = create_server();
    info!("server and client constructed, {:?}", instant.elapsed());

    let client_task = task::spawn(async move {
        // TODO: Await connection.
        timeout(100).await;
        let instant = std::time::Instant::now();
        info!("request start");
        let res = rpc_client
            .shouter
            .shout(ShoutRequest {
                message: "hi from rust client".into(),
            })
            .await;
        info!("received response after {:?}: {:?}", instant.elapsed(), res);
    });

    // would be much nicer to not await forever here but return once connected.
    rpc_server.connect(stream).await?;
    client_task.await;
    info!("connection closed, elapsed {:?}", instant.elapsed());
    Ok(())
}

pub fn create_server() -> (Server, hrpc::codegen::client::Client) {
    use hrpc::codegen::server::{Shouter, ShouterServer};
    use std::io::Result;

    struct App;

    #[async_trait]
    impl Shouter for App {
        async fn shout(&mut self, req: ShoutRequest) -> Result<ShoutResponse> {
            Ok(ShoutResponse {
                message: req.message.to_uppercase(),
                loudness: 15,
            })
        }
    }

    let app = App {};
    let mut server = Server::new();
    server.define_service(ShouterServer::new(app));
    let client = server.create_client(hrpc::codegen::client::Client::new());
    (server, client)
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
