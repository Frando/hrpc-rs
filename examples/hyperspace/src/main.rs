// TODO: Cleanup
#![allow(dead_code)]
#![allow(unused_imports)]

use async_std::io;
use async_std::os::unix::net::UnixStream;
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
mod freemap;
mod session;

use session::RemoteCorestore;

#[async_std::main]
pub async fn main() -> Result<()> {
    env_logger::from_env(Env::default().default_filter_or("info")).init();
    let path = "/tmp/hrpc.sock";
    let socket = UnixStream::connect(path).await?;
    let mut rpc = Rpc::new();
    let mut corestore = RemoteCorestore::new(&mut rpc);
    task::spawn(async move {
        rpc.connect(socket).await.unwrap();
    });
    let mut core = corestore.open_by_name("first").await?;
    info!("core start {:?}", core.read().await);

    let core_clone = core.clone();
    task::spawn(async move {
        info!("core in task {:?}", core_clone.read().await);
    });

    core.append(vec![b"yee".to_vec(), b"yaa".to_vec()]).await?;
    info!("core end {:?}", core.read().await);
    let block = core.get(1).await?;
    info!("block {:?}", block.map(|b| String::from_utf8(b)));
    Ok(())
}

pub async fn _main_old() -> Result<()> {
    env_logger::from_env(Env::default().default_filter_or("info")).init();

    let path = "/tmp/hrpc.sock";
    let mut client = uds_client(path).await.unwrap();

    let core = client
        .corestore
        .open(OpenRequest {
            id: 1,
            key: None,
            name: Some("first".into()),
            weak: None,
        })
        .await?;
    eprintln!("core : {:?}", core);
    let res = client
        .hypercore
        .append(AppendRequest {
            id: 1,
            blocks: vec![b"foo".to_vec(), b"bar".to_vec()],
        })
        .await?;
    eprintln!("append: {:?}", res);
    let res = client
        .hypercore
        .get(GetRequest {
            id: 1,
            seq: 1,
            resource_id: 2,
            wait: None,
            if_available: None,
            on_wait_id: None,
        })
        .await?;
    eprintln!("get 1: {:?}", res);
    Ok(())
}

async fn uds_client(path: &str) -> Result<codegen::client::Client> {
    let client = hrpc::transport::uds::connect(
        path,
        Box::new(move |rpc| rpc.create_client(codegen::client::Client::new())),
    )
    .await?;
    Ok(client)
}
