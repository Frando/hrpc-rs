use async_std::os::unix::net::{UnixListener, UnixStream};
use async_std::sync::Arc;
use async_std::task;
use async_trait::async_trait;
use env_logger::Env;
use futures::stream::StreamExt;
use hrpc::Rpc;
use log::*;
use std::env;
use std::io::Result;
use std::sync::Mutex;

pub mod codegen {
    include!(concat!(env!("OUT_DIR"), "/codegen.rs"));
}
use codegen::*;

#[async_std::main]
pub async fn main() -> Result<()> {
    env_logger::from_env(Env::default().default_filter_or("info")).init();

    let mode = env::args().nth(1).unwrap_or("server".into());
    let path = "/tmp/hrpc.sock";
    let app = App::default();

    match mode.as_ref() {
        "client" => {
            let socket = UnixStream::connect(path).await?;
            info!("connected to {}", path);
            let mut rpc = Rpc::new();
            rpc.define_service(codegen::server::CalcService::new(app.clone()));
            let mut client: codegen::client::Client = rpc.client().into();
            task::spawn(async move {
                rpc.connect(socket).await.unwrap();
            });
            info!("I ask the server: hello, world");
            let res = client
                .shouter
                .shout(ShoutRequest {
                    message: "hello, world".into(),
                })
                .await?;
            info!("got response: {:?}", res);
            Ok(())
        }
        "server" => {
            let listener = UnixListener::bind(&path).await?;
            info!("listening on {}", path);
            let mut incoming = listener.incoming();
            let app = app.clone();
            while let Some(stream) = incoming.next().await {
                let stream = stream?;
                info!("new client connected");
                let mut rpc = Rpc::new();
                rpc.define_service(codegen::server::ShouterService::new(app.clone()));
                let mut client: codegen::client::Client = rpc.client().into();
                task::spawn(async move {
                    rpc.connect(stream).await.unwrap();
                });
                info!("I ask the client: what is 3 + 5.3?");
                let res = client.calc.add(AddRequest { a: 3f32, b: 5.3f32 }).await?;
                info!("got response: {:?}", res);
            }
            Ok(())
        }
        _ => {
            eprintln!("usage: cargo run --example basic -- [client|server]");
            Ok(())
        }
    }
}

#[derive(Default, Clone)]
struct App {
    loudness: Arc<Mutex<u64>>,
}

#[async_trait]
impl codegen::server::Shouter for App {
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
impl codegen::server::Calc for App {
    async fn add(&mut self, req: AddRequest) -> std::io::Result<CalcResponse> {
        Ok(CalcResponse {
            result: req.a + req.b,
        })
    }
}
