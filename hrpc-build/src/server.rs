use proc_macro2::TokenStream;
use quote::{format_ident, quote};

pub fn generate(service: &prost_build::Service, service_id: u64) -> TokenStream {
    let service_trait = quote::format_ident!("{}", service.name);
    let service_struct = quote::format_ident!("{}Server", service.name);

    let request_matches = generate_request_matches(service);
    let trait_methods = generate_trait_methods(service);

    quote! {
        #[async_trait]
        pub trait #service_trait: Send + 'static {
            #trait_methods
        }

        pub struct #service_struct {
            inner: Box<dyn #service_trait>
        }

        impl #service_struct {
            pub fn new(inner: impl #service_trait + 'static) -> Self {
                Self {
                    inner: Box::new(inner)
                }
            }
        }

        #[async_trait]
        impl Service for #service_struct {
            fn id(&self) -> u64 {
                #service_id
            }
            async fn handle_request(&mut self, request: Request) -> Result<Response> {
                match request.method() {
                    #request_matches
                    _ => Err(Error::new(ErrorKind::Other, "Invalid method ID"))
                }
            }
        }
    }
}

pub fn generate_request_matches(service: &prost_build::Service) -> TokenStream {
    let mut stream = TokenStream::new();
    for (i, method) in service.methods.iter().enumerate() {
        // TODO: Fixed method ids.
        let method_id = i as u64 + 1;
        let ident = format_ident!("{}", method.name);
        let input_type = format_ident!("{}", method.input_type);
        // let output_type = format_ident!("{}", method.input_type);
        stream.extend(quote! {
            #method_id => {
                let req = #input_type::decode(request.body())?;
                let res = self.inner.#ident(req).await?;
                Ok(res.into())
            }
        });
    }
    stream
}

pub fn generate_trait_methods(service: &prost_build::Service) -> TokenStream {
    let mut stream = TokenStream::new();

    for method in service.methods.iter() {
        let ident = format_ident!("{}", method.name);
        let input_type = format_ident!("{}", method.input_type);
        let output_type = format_ident!("{}", method.output_type);
        stream.extend(quote! {
            async fn #ident(&mut self, _req: #input_type) -> Result<#output_type> {
                Err(Error::new(ErrorKind::Other, "Method not implemented"))

            }
        });
    }
    stream
}
