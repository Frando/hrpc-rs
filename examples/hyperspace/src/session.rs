use crate::codegen;
use crate::codegen::client::Client;
use crate::codegen::*;
use crate::freemap::NamedMap;
use async_std::sync::{RwLock, RwLockReadGuard};
use async_trait::async_trait;
use futures::stream::Stream;
use hrpc::Rpc;
use std::fmt;
use std::future::Future;
use std::io::Result;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::ReadStream;

type Key = Vec<u8>;
type Sessions = NamedMap<String, RemoteHypercore>;

#[derive(Clone)]
pub struct RemoteCorestore {
    client: Client,
    sessions: Sessions,
    resource_counter: Arc<AtomicU64>,
}

impl RemoteCorestore {
    pub fn new(rpc: &mut Rpc) -> Self {
        let client = Client::new(rpc.client());
        let corestore = Self {
            client,
            sessions: NamedMap::new(),
            resource_counter: Arc::new(AtomicU64::new(1)),
        };
        rpc.define_service(codegen::server::HypercoreServer::new(corestore.clone()));
        corestore
    }

    pub async fn open_by_key(&mut self, key: Key) -> Result<RemoteHypercore> {
        let hkey = hex::encode(&key);
        if let Some(mut core) = self.sessions.get_by_name(&hkey) {
            core.open().await?;
            return Ok(core);
        }
        let mut core = RemoteHypercore::new(&self, Some(key), None);
        let id = self.sessions.insert_with_name(core.clone(), hkey);
        core.set_id(id).await;
        core.open().await?;
        Ok(core)
    }

    pub async fn open_by_name(&mut self, name: impl ToString) -> Result<RemoteHypercore> {
        let name = name.to_string();
        if let Some(mut core) = self.sessions.get_by_name(&name) {
            core.open().await?;
            return Ok(core);
        }
        let mut core = RemoteHypercore::new(&self, None, Some(name.clone()));
        let id = self.sessions.insert_with_name(core.clone(), name);
        core.set_id(id).await;
        core.open().await?;

        let key = core.inner.read().await.key.clone();
        if let Some(key) = key {
            let hkey = hex::encode(key);
            self.sessions.set_name(id, hkey);
        }
        Ok(core)
    }
}

#[async_trait]
impl codegen::server::Hypercore for RemoteCorestore {
    async fn on_append(&mut self, req: AppendEvent) -> Result<Void> {
        if let Some(core) = self.sessions.get(req.id) {
            core.on_append(req.length, req.byte_length).await
        }
        Ok(Void {})
    }
}

#[derive(Clone)]
pub struct RemoteHypercore {
    inner: Arc<RwLock<InnerHypercore>>,
    client: Client,
    resource_counter: Arc<AtomicU64>,
}

impl RemoteHypercore {
    pub(crate) fn new(corestore: &RemoteCorestore, key: Option<Key>, name: Option<String>) -> Self {
        let inner = InnerHypercore {
            key,
            name,
            ..Default::default()
        };
        let inner = Arc::new(RwLock::new(inner));
        Self {
            inner,
            client: corestore.client.clone(),
            resource_counter: corestore.resource_counter.clone(),
        }
    }

    async fn set_id(&mut self, id: u64) {
        self.inner.write().await.id = id;
    }

    async fn id(&self) -> u64 {
        self.inner.read().await.id
    }

    pub async fn read(&self) -> RwLockReadGuard<'_, InnerHypercore> {
        self.inner.read().await
    }

    pub async fn append(&mut self, blocks: Vec<Vec<u8>>) -> Result<()> {
        let mut inner = self.inner.write().await;
        let id = inner.id;
        let res = self
            .client
            .hypercore
            .append(AppendRequest {
                id: id as u32,
                blocks,
            })
            .await?;
        inner.length = res.length;
        inner.byte_length = res.byte_length;
        Ok(())
    }

    pub async fn get(&mut self, seq: u64) -> Result<Option<Vec<u8>>> {
        let id = self.inner.read().await.id;
        let resource_id = self.resource_counter.fetch_add(1, Ordering::SeqCst);
        let res = self
            .client
            .hypercore
            .get(GetRequest {
                id: id as u32,
                seq,
                resource_id,
                wait: None,
                if_available: None,
                on_wait_id: None,
            })
            .await?;

        Ok(res.block)
    }

    pub fn create_read_stream(&mut self, start: Option<u64>, end: Option<u64>) -> ReadStream {
        ReadStream::new(self.clone(), start.unwrap_or_default(), end, false)
        // crate::stream::create_read_stream(self.clone(), start.unwrap_or_default(), end, false)
    }

    async fn open(&mut self) -> Result<()> {
        let open = self.inner.read().await.open;
        if open {
            return Ok(());
        }
        let req = {
            let inner = self.inner.read().await;
            OpenRequest {
                id: inner.id as u32,
                key: inner.key.clone(),
                name: inner.name.clone(),
                weak: None,
            }
        };
        let res = self.client.corestore.open(req).await?;
        let mut inner = self.inner.write().await;
        inner.key = Some(res.key);
        inner.open = true;
        Ok(())
    }

    pub(crate) async fn on_append(&self, length: u64, byte_length: u64) {
        let mut inner = self.inner.write().await;
        inner.length = length;
        inner.byte_length = byte_length;
    }
}

#[derive(Default)]
pub struct InnerHypercore {
    pub id: u64,
    pub name: Option<String>,
    pub key: Option<Key>,
    pub length: u64,
    pub byte_length: u64,
    pub writable: bool,
    pub open: bool,
}

impl fmt::Debug for InnerHypercore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RemoteHypercore {{ key {:?}, name {:?}, id {}, length {}, byte_length {}, writable {}, open {} }}",
            self.key.as_ref().and_then(|key| Some(hex::encode(key))),
            self.name,
            self.id,
            self.length,
            self.byte_length,
            self.writable,
            self.open
        )
    }
}
