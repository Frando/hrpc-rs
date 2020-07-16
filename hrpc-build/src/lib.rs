use proc_macro2::TokenStream;
use prost_build::Config;
use quote::quote;
use std::collections::HashSet;
use std::fs;
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
    let protos = path_array_to_owned(protos);
    let mut includes = path_array_to_owned(includes);
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    eprintln!("build hrpc protocol from {:?} into {:?}", protos, out_dir);

    // Write hrpc.proto into a tempdir to add it to the protoc includes.
    let hrpc_proto = include_bytes!("hrpc.proto");
    let tempdir = tempfile::Builder::new().prefix("hrpc-build").tempdir()?;
    fs::write(tempdir.path().join("hrpc.proto"), hrpc_proto.to_vec())?;
    includes.push(tempdir.path().to_path_buf());

    // Setup prost config.
    let mut config = Config::new();
    config.compile_well_known_types();
    config.extern_path(".hrpc.Void", "Void");
    config.out_dir(out_dir.clone());
    config.service_generator(Box::new(ServiceGenerator::new()));

    config.compile_protos(&protos[..], &includes[..])?;

    // Optionally format the code with rustfmt.
    #[cfg(feature = "rustfmt")]
    crate::fmt::fmt(out_dir.to_str().expect("Expected utf8 out_dir"));

    Ok(())
}

pub struct ServiceGenerator {
    build_client: bool,
    build_server: bool,
    clients: TokenStream,
    servers: TokenStream,
    service_id_counter: u64,
    service_ids: HashSet<u64>,
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
            service_ids: HashSet::new(),
            services: Vec::new(),
        }
    }
}

impl prost_build::ServiceGenerator for ServiceGenerator {
    fn generate(&mut self, service: prost_build::Service, mut _buf: &mut String) {
        let service_id = if let Some(id) = service.options.id() {
            id
        } else {
            self.service_id_counter += 1;
            self.service_id_counter
        };

        if self.service_ids.get(&service_id).is_some() {
            // TODO: How to handle errors here?
            eprintln!("ERROR: Duplicate service ID {}", service_id);
            std::process::exit(1);
        }
        self.service_ids.insert(service_id);

        if self.build_client {
            let client = client::generate(&service, service_id);
            self.clients.extend(client);
        }

        if self.build_server {
            let server = server::generate(&service, service_id);
            self.servers.extend(server);
        }
        self.services.push(service);
    }

    fn finalize(&mut self, buf: &mut String) {
        let encodings = quote! {
            pub type Void = ();
        };
        buf.push_str(&encodings.to_string());

        if !self.clients.is_empty() {
            let clients = &self.clients;
            let wrapper = client::generate_wrapper(&self.services);
            let client_module = quote! {
                pub mod client {
                    use super::*;
                    use prost::Message;
                    use hrpc::RequestFuture;
                    use std::io::Result;
                    #wrapper
                    #clients
                }
            };
            buf.push_str(&client_module.to_string());
            self.clients = TokenStream::default();
        }
        if !self.servers.is_empty() {
            let servers = &self.servers;
            let server_module = quote! {
                pub mod server {
                    use super::*;
                    use async_trait::async_trait;
                    use hrpc::{Request, Response, Service};
                    use std::io::{Error, ErrorKind, Result};
                    use prost::Message;
                    #servers
                }
            };
            buf.push_str(&server_module.to_string());
            self.servers = TokenStream::default();
        }
    }
}

pub trait HrpcOptions {
    fn get_unknown_fields(&self) -> &Vec<prost::UnknownField>;
    fn id_tag(&self) -> u32;
    fn id(&self) -> Option<u64> {
        let tag = self.id_tag();
        let fields = self.get_unknown_fields();
        let field = fields.iter().filter(|f| f.tag == tag).nth(0)?;
        let mut id = 0u64;
        varinteger::decode(&field.value[..], &mut id);
        Some(id)
    }
}

impl HrpcOptions for prost_types::MethodOptions {
    fn get_unknown_fields(&self) -> &Vec<prost::UnknownField> {
        &self.protobuf_unknown_fields
    }
    fn id_tag(&self) -> u32 {
        50001
    }
}

impl HrpcOptions for prost_types::ServiceOptions {
    fn get_unknown_fields(&self) -> &Vec<prost::UnknownField> {
        &self.protobuf_unknown_fields
    }
    fn id_tag(&self) -> u32 {
        50000
    }
}

fn path_array_to_owned<P>(paths: &[P]) -> Vec<PathBuf>
where
    P: AsRef<Path>,
{
    paths.iter().map(|p| p.as_ref().to_path_buf()).collect()
}
