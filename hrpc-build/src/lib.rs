use proc_macro2::TokenStream;
use prost_build::Config;
use std::io::Result;
use std::path::{Path, PathBuf};

mod client;
#[cfg(feature = "rustfmt")]
mod fmt;
mod server;

pub fn compile_protos<P>(protos: &[P], includes: &[P]) -> Result<()>
where
    P: AsRef<Path>,
{
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    eprintln!(
        "build! protos {:?} into {:?}",
        protos.iter().map(|p| p.as_ref()).collect::<Vec<&Path>>(),
        out_dir
    );
    let mut config = Config::new();
    config.out_dir(out_dir.clone());
    config.service_generator(Box::new(ServiceGenerator::new()));

    config.compile_protos(protos, includes)?;

    #[cfg(feature = "rustfmt")]
    {
        crate::fmt::fmt(out_dir.to_str().expect("Expected utf8 out_dir"));
    }
    Ok(())
}

pub struct ServiceGenerator {
    build_client: bool,
    build_server: bool,
    clients: TokenStream,
    servers: TokenStream,
    service_id_counter: u64,
    services: Vec<prost_build::Service>,
}

impl ServiceGenerator {
    pub fn new() -> Self {
        Self {
            build_client: true,
            build_server: true,
            clients: TokenStream::default(),
            servers: TokenStream::default(),
            service_id_counter: 0,
            services: Vec::new(),
        }
    }
}

impl prost_build::ServiceGenerator for ServiceGenerator {
    fn generate(&mut self, service: prost_build::Service, mut _buf: &mut String) {
        self.service_id_counter += 1;
        if self.build_client {
            let client = client::generate(&service, self.service_id_counter);
            self.clients.extend(client);
        }

        if self.build_server {
            let server = server::generate(&service, self.service_id_counter);
            self.servers.extend(server);
        }
        self.services.push(service);
    }

    fn finalize(&mut self, buf: &mut String) {
        if !self.clients.is_empty() {
            let clients = &self.clients;
            let wrapper = client::generate_wrapper(&self.services);
            let client_service = quote::quote! {
                pub mod client {
                    use super::*;
                    use prost::Message;
                    use std::io::Result;
                    #wrapper
                    #clients
                }
            };
            let code = format!("{}", client_service);
            buf.push_str(&code);
            self.clients = TokenStream::default();
        }
        if !self.servers.is_empty() {
            let servers = &self.servers;
            let client_service = quote::quote! {
                pub mod server {
                    use super::*;
                    use async_trait::async_trait;
                    use hrpc::{Request, Response, Service};
                    use std::io::{Error, ErrorKind, Result};
                    use prost::Message;
                    #servers
                }
            };
            let code = format!("{}", client_service);
            buf.push_str(&code);
            self.servers = TokenStream::default();
        }
    }
}
