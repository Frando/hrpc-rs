use async_std::os::unix::net::{UnixListener, UnixStream};
use async_std::prelude::*;
use async_std::task;
use futures::channel::mpsc;
use futures::sink::SinkExt;
use log::*;
use std::io::Result;
use std::path::{Path, PathBuf};

use crate::{Rpc, RpcClient};

// type Callback = Box<dyn FnMut(&mut Rpc)>;
type ClientCallback<T> = dyn Fn(&mut Rpc) -> T + Send;

struct SockRemover {
    path: PathBuf,
}
impl Drop for SockRemover {
    fn drop(&mut self) {
        if std::fs::remove_file(&self.path).is_err() {
            error!("Could not remove socket {}", self.path.to_str().unwrap());
        } else {
            debug!("Removed socket {}", self.path.to_str().unwrap());
        }
    }
}

pub mod uds {
    use super::*;

    pub async fn accept<P, C>(
        path: P,
        define_rpc: Box<ClientCallback<C>>,
    ) -> Result<mpsc::Receiver<C>>
    where
        P: AsRef<Path>,
        C: RpcClient + Send + 'static,
    {
        let (send, recv) = mpsc::channel(100);
        let path = path.as_ref().to_path_buf();

        task::spawn(incoming_loop(path, send, define_rpc));
        Ok(recv)
    }

    async fn incoming_loop<C>(
        path: PathBuf,
        mut sender: mpsc::Sender<C>,
        define_rpc: Box<ClientCallback<C>>,
    ) -> Result<()>
    where
        C: RpcClient + Send,
    {
        let sock_remover = SockRemover { path: path.clone() };
        let listener = UnixListener::bind(&path).await?;
        info!("Listening on {}", path.to_str().unwrap());
        let mut incoming = listener.incoming();
        while let Some(stream) = incoming.next().await {
            let stream = stream?;
            let mut rpc = Rpc::new();
            let client = define_rpc(&mut rpc);
            task::spawn(async move {
                rpc.connect(stream).await.unwrap();
            });
            sender.send(client).await.unwrap();
        }
        let _ = sock_remover;
        Ok(())
    }

    pub async fn connect<P, C>(path: P, define_rpc: Box<ClientCallback<C>>) -> Result<C>
    where
        P: AsRef<Path>,
        C: RpcClient,
    {
        let socket = UnixStream::connect(path.as_ref()).await?;
        let mut rpc = Rpc::new();
        let client = define_rpc(&mut rpc);
        task::spawn(async move {
            rpc.connect(socket).await.unwrap();
        });
        Ok(client)
    }
}
