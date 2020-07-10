// TODO: Cleanup
#![allow(dead_code)]
#![allow(unused_imports)]

use async_std::io;
use async_std::io::BufReader;
use async_std::os::unix::net::UnixStream;
use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task;
use async_trait::async_trait;
use env_logger::Env;
use futures::io::{AsyncRead, AsyncWrite};
use futures::stream::{StreamExt, TryStreamExt};
use hrpc::{Client, Decoder, Rpc};
use log::*;
use std::collections::HashMap;
use std::env;
use std::io::{Error, Result};
use std::sync::Mutex;

use hyperspace_client::codegen;
use hyperspace_client::{RemoteCorestore, RemoteHypercore};

#[async_std::main]
pub async fn main() -> Result<()> {
    env_logger::from_env(Env::default().default_filter_or("info")).init();
    let name = env::args().nth(1).unwrap_or("first".into());
    let mode = env::args().nth(2).unwrap_or("".into());
    let path = "/tmp/hrpc.sock";
    let socket = UnixStream::connect(path).await?;
    let mut rpc = Rpc::new();
    let mut corestore = RemoteCorestore::new(&mut rpc);
    task::spawn(async move {
        rpc.connect(socket).await.unwrap();
    });
    let core = corestore.open_by_name(name).await?;

    let mut tasks = vec![];
    match mode.as_str() {
        "read" => {
            let read_task = task::spawn(read(core.clone(), io::stdout()));
            tasks.push(read_task);
        }
        "write" => {
            let write_task = task::spawn(write(core.clone(), BufReader::new(io::stdin())));
            tasks.push(write_task)
        }
        _ => panic!("usage: hyperspace-client <name> [read|write]"),
    }

    futures::future::join_all(tasks).await;

    Ok(())
    // let read_task = task::spawn(read(core.clone(), io::stdout()));
    // let write_task = task::spawn(write(core.clone(), BufReader::new(io::stdin())));
    // let mut read_core = core.clone();
    // let read_task = task::spawn(async move {
    //     let stream = read_core.create_read_stream(None, None);
    //     let mut async_read = stream.into_async_read();
    //     let mut stdout = async_std::io::stdout();
    //     async_std::io::copy(&mut async_read, &mut stdout).await
    // });
    // let mut write_core = core.clone();
    // let write_task = task::spawn(async move {
    //     let mut stdin = BufReader::new(async_std::io::stdin());
    //     let mut buf = vec![0u8; 1024 * 1024];
    //     loop {
    //         let n = stdin.read(&mut buf).await.unwrap();
    //         if n > 0 {
    //             let vec = (&buf[..n]).to_vec();
    //             write_core.append(vec![vec]).await.unwrap();
    //             eprintln!("written n {}", n);
    //         }
    //     }
    // });

    // write_task.await?;
    // read_task.await?;

    // info!("core start {:?}", core.read().await);

    // let core_clone = core.clone();
    // task::spawn(async move {
    //     info!("core in task {:?}", core_clone.read().await);
    // });

    // core.append(vec![b"yee".to_vec(), b"yaa".to_vec()]).await?;
    // info!("core end {:?}", core.read().await);
    // let block = core.get(1).await?;
    // info!("block {:?}", block.map(|b| String::from_utf8(b)));
    // while let Some(message) = stream.next().await {
    //     info!(
    //         "read: {:?}",
    //         message.and_then(|m| Ok(String::from_utf8(m).unwrap()))
    //     );
    // }
}
async fn read(mut core: RemoteHypercore, mut writer: impl AsyncWrite + Send + Unpin) -> Result<()> {
    let stream = core.create_read_stream(None, None);
    let mut async_read = stream.into_async_read();
    async_std::io::copy(&mut async_read, &mut writer).await?;
    eprintln!("finished");
    Ok(())
}

async fn write(mut core: RemoteHypercore, mut reader: impl AsyncRead + Send + Unpin) -> Result<()> {
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = reader.read(&mut buf).await?;
        if n > 0 {
            let vec = (&buf[..n]).to_vec();
            core.append(vec![vec]).await?;
            info!("written n {}", n);
        }
    }
}
