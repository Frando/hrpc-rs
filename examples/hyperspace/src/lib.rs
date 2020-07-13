pub mod codegen {
    include!(concat!(env!("OUT_DIR"), "/codegen.rs"));
}

pub use codegen::*;
mod freemap;
mod session;
pub mod stream;

pub use session::*;
pub use stream::*;

pub async fn open_corestore() -> std::io::Result<RemoteCorestore> {
    let path = "/tmp/hyperspace.sock";
    let socket = async_std::os::unix::net::UnixStream::connect(path).await?;
    let mut rpc = hrpc::Rpc::new();
    let corestore = RemoteCorestore::new(&mut rpc);
    async_std::task::spawn(async move {
        rpc.connect(socket).await.unwrap();
    });
    Ok(corestore)
}
