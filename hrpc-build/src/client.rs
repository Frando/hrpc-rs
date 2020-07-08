use heck::SnakeCase;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

pub fn generate(service: &prost_build::Service, service_id: u64) -> TokenStream {
    let service_ident = quote::format_ident!("{}", service.name);
    let methods = generate_methods(service);

    quote! {
        pub struct #service_ident(hrpc::Client);
        impl #service_ident {
            const ID: u64 = #service_id;
            pub fn new(client: hrpc::Client) -> Self {
                Self(client)
            }
            #methods
        }
    }
}

pub fn generate_methods(service: &prost_build::Service) -> TokenStream {
    let mut stream = TokenStream::new();

    for (i, method) in service.methods.iter().enumerate() {
        // TODO: Support statically tagged methods.
        let method_id = i as u64 + 1;
        let ident = format_ident!("{}", method.name);
        let input_type = format_ident!("{}", method.input_type);
        let output_type = format_ident!("{}", method.output_type);
        stream.extend(quote! {
            pub async fn #ident(&mut self, req: #input_type) -> Result<#output_type> {
                self.0.request_into(Self::ID, #method_id, req).await
            }
        });
    }
    stream
}

pub fn generate_wrapper(services: &Vec<prost_build::Service>) -> TokenStream {
    let mut fields = TokenStream::new();
    let mut build = TokenStream::new();
    let mut fieldlist = TokenStream::new();
    for service in services.iter() {
        let field_ident = format_ident!("{}", service.name.to_snake_case());
        let struct_ident = format_ident!("{}", service.name);
        fields.extend(quote! {
            pub #field_ident: #struct_ident,
        });
        build.extend(quote! {
            let #field_ident = #struct_ident::new(builder.create_client());
        });
        fieldlist.extend(quote! {
            #field_ident,
        });
    }

    quote! {
        pub struct Client {
            #fields
        }

        impl Client {
            pub fn new() -> (Self, hrpc::ClientBuilder) {
                let builder = hrpc::ClientBuilder::new();

                #build

                let app_client = Self { #fieldlist };
                (app_client, builder)
            }
        }

        impl hrpc::RpcClient for Client {}
    }
}
