#![allow(dead_code)]

pub mod schema {
    use super::*;
    use async_trait::async_trait;

    pub use demo_server::*;

    #[derive(Clone, Debug)]
    pub enum DemoServices {
        Calc(CalcService),
        Text(TextService),
    }

    #[derive(Clone, Debug)]
    pub enum CalcService {
        Add(Message<AddRequest, CalcResponse>),
        Square(Message<SquareRequest, CalcResponse>),
    }

    #[derive(Clone, Debug)]
    pub enum TextService {
        Upper(Message<UpperRequest, UpperResponse>),
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct AddRequest {
        #[prost(uint64, tag = "1")]
        pub a: u64,
        #[prost(uint64, tag = "2")]
        pub b: u64,
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct SquareRequest {
        #[prost(uint64, tag = "1")]
        pub num: u64,
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct CalcResponse {
        #[prost(uint64, tag = "1")]
        pub result: u64,
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct UpperRequest {
        #[prost(string, tag = "1")]
        pub text: String,
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct UpperResponse {
        #[prost(string, tag = "1")]
        pub text: String,
    }

    impl Services for DemoServices {
        fn from_parts(
            typ: u8,
            service_id: u64,
            method_id: u64,
            request_id: u64,
            body: &[u8],
        ) -> Result<Self, prost::DecodeError> {
            match service_id {
                0 => Ok(Self::Calc(CalcService::from_parts(
                    typ, method_id, request_id, body,
                )?)),
                1 => Ok(Self::Text(TextService::from_parts(
                    typ, method_id, request_id, body,
                )?)),
                _ => Err(prost::DecodeError::new(INVALID_SERVICE_ID)),
            }
        }

        fn service(&self) -> &dyn Service {
            match self {
                Self::Calc(service) => service,
                Self::Text(service) => service,
            }
        }
        fn service_mut(&mut self) -> &mut dyn Service {
            match self {
                Self::Calc(service) => service,
                Self::Text(service) => service,
            }
        }
    }

    impl Service for CalcService {
        fn service_id(&self) -> u64 {
            0
        }
        fn method_id(&self) -> u64 {
            match self {
                Self::Add(_) => 0,
                Self::Square(_) => 1,
            }
        }

        fn from_parts(
            typ: u8,
            method_id: u64,
            request_id: u64,
            body: &[u8],
        ) -> Result<Self, prost::DecodeError>
        where
            Self: Sized,
        {
            match method_id {
                0 => Ok(Self::Add(Message::<AddRequest, CalcResponse>::from_parts(
                    typ, request_id, body,
                )?)),
                1 => Ok(Self::Square(
                    Message::<SquareRequest, CalcResponse>::from_parts(typ, request_id, body)?,
                )),
                _ => Err(prost::DecodeError::new(INVALID_METHOD_ID)),
            }
        }

        fn inner_mut(&mut self) -> &mut dyn MessageT {
            match self {
                Self::Add(message) => message,
                Self::Square(message) => message,
            }
        }
        fn inner(&self) -> &dyn MessageT {
            match self {
                Self::Add(message) => message,
                Self::Square(message) => message,
            }
        }
    }

    impl Service for TextService {
        fn service_id(&self) -> u64 {
            1
        }
        fn method_id(&self) -> u64 {
            match self {
                Self::Upper(_) => 0,
            }
        }
        fn from_parts(
            typ: u8,
            method_id: u64,
            request_id: u64,
            body: &[u8],
        ) -> Result<Self, prost::DecodeError> {
            match method_id {
                0 => Ok(Self::Upper(
                    Message::<UpperRequest, UpperResponse>::from_parts(typ, request_id, body)?,
                )),
                _ => Err(prost::DecodeError::new(INVALID_METHOD_ID)),
            }
        }

        fn inner_mut(&mut self) -> &mut dyn MessageT {
            match self {
                Self::Upper(message) => message,
            }
        }
        fn inner(&self) -> &dyn MessageT {
            match self {
                Self::Upper(message) => message,
            }
        }
    }

    pub mod demo_client {
        use super::*;
        pub mod calc {
            use super::*;
            pub async fn add(
                handle: &mut RpcHandle<DemoServices>,
                req: AddRequest,
            ) -> Result<CalcResponse, RpcError> {
                let req = DemoServices::Calc(CalcService::Add(Message::request(req)));
                let res = handle.request(req).await?;
                match res {
                    DemoServices::Calc(CalcService::Add(message)) if message.is_response() => {
                        Ok(message.into_response().unwrap())
                    }
                    _ => Err(RpcError::MessageMismatch),
                }
            }
        }
    }

    pub mod demo_server {
        use super::*;

        #[async_trait]
        pub trait CalcServer {
            async fn handle_request(
                &mut self,
                handle: &mut RpcHandle<DemoServices>,
                req: DemoServices,
            ) -> Result<(), RpcError> {
                match req {
                    DemoServices::Calc(CalcService::Add(message)) => {
                        let req = message.as_request().unwrap();
                        let res = self.on_add(req).await?;
                        let res = DemoServices::Calc(CalcService::Add(message.reply(res)?));
                        handle.request_no_reply(res).await?;
                        Ok(())
                    }
                    _ => Err(RpcError::MethodNotImplemented),
                }
            }
            async fn on_add(&mut self, _req: &AddRequest) -> Result<CalcResponse, RpcError> {
                Err(RpcError::MethodNotImplemented)
            }
            async fn on_square(&mut self, _req: &SquareRequest) -> Result<CalcResponse, RpcError> {
                Err(RpcError::MethodNotImplemented)
            }
        }
    }
}

use crate::{sessions::Sessions, Decoder, Message as RawMessage};
use async_std::channel::{Receiver, Sender};
use async_std::stream::StreamExt;
use async_std::task::{self, JoinHandle};
// use futures::channel::oneshot;
use futures::{channel::oneshot, stream::Stream};
// use futures::{AsyncRead, AsyncWrite, AsyncWriteExt, Future, FutureExt};
use futures::{AsyncRead, AsyncWrite, AsyncWriteExt, Future};
// use oneshot::Canceled;
use std::pin::Pin;
use std::sync::Arc;
use std::{
    io,
    task::{Context, Poll},
};
use thiserror::Error;
const INVALID_SERVICE_ID: &'static str = "Invalid service ID";
const INVALID_METHOD_ID: &'static str = "Invalid method ID";

#[derive(Error, Clone, Debug)]
pub enum RpcError {
    #[error("IO error")]
    Io(#[from] Arc<io::Error>),
    #[error("Decode error")]
    Decode(#[from] prost::DecodeError),
    #[error("Encode error")]
    Encode(#[from] prost::EncodeError),
    #[error("Message does not match")]
    MessageMismatch,
    #[error("Method not implemented")]
    MethodNotImplemented,
    #[error("Remote error: {message}")]
    Remote { message: String },
}

impl From<io::Error> for RpcError {
    fn from(e: io::Error) -> Self {
        Arc::new(e).into()
    }
}

pub trait RpcMessageBody: prost::Message + Default {}

pub trait Services: std::fmt::Debug + Sized + Send + 'static {
    fn from_parts(
        typ: u8,
        service_id: u64,
        method_id: u64,
        request_id: u64,
        body: &[u8],
    ) -> Result<Self, prost::DecodeError>;
    fn service(&self) -> &dyn Service;
    fn service_mut(&mut self) -> &mut dyn Service;

    fn encode(&self, buf: &mut [u8]) -> Result<(), prost::EncodeError> {
        self.service().encode(buf)
    }
    fn encoded_len(&self) -> usize {
        self.service().encoded_len()
    }
    fn inner_mut(&mut self) -> &mut dyn MessageT {
        self.service_mut().inner_mut()
    }
    fn inner(&self) -> &dyn MessageT {
        self.service().inner()
    }
}

pub trait Service: std::fmt::Debug + Send + 'static {
    fn method_id(&self) -> u64;
    fn service_id(&self) -> u64;
    fn from_parts(
        typ: u8,
        method_id: u64,
        request_id: u64,
        body: &[u8],
    ) -> Result<Self, prost::DecodeError>
    where
        Self: Sized;
    fn inner_mut(&mut self) -> &mut dyn MessageT;
    fn inner(&self) -> &dyn MessageT;
    fn encode(&self, buf: &mut [u8]) -> Result<(), prost::EncodeError> {
        let service_id = self.service_id();
        let method_id = self.method_id();
        self.inner().encode(buf, service_id, method_id)
    }
    fn encoded_len(&self) -> usize {
        let service_id = self.service_id();
        let method_id = self.method_id();
        self.inner().encoded_len(service_id, method_id)
    }
}

pub trait MessageT: std::fmt::Debug + Send + 'static {
    fn request_id(&self) -> u64;
    fn set_request_id(&mut self, request_id: u64);
    fn encode(
        &self,
        buf: &mut [u8],
        service_id: u64,
        method_id: u64,
    ) -> Result<(), prost::EncodeError>;
    fn encoded_len(&self, service_id: u64, method_id: u64) -> usize;
    fn is_response(&self) -> bool;
    fn is_request(&self) -> bool;
}

impl<Req, Res> MessageT for Message<Req, Res>
where
    Req: prost::Message + Default + Sized + 'static,
    Res: prost::Message + Default + Sized + 'static,
{
    fn request_id(&self) -> u64 {
        self.request_id
    }
    fn set_request_id(&mut self, request_id: u64) {
        self.request_id = request_id
    }

    fn encode(
        &self,
        buf: &mut [u8],
        service_id: u64,
        method_id: u64,
    ) -> Result<(), prost::EncodeError> {
        let mut offset = 0;
        offset += varinteger::encode(
            self.message_len(service_id, method_id) as u64,
            &mut buf[offset..],
        );
        offset += varinteger::encode(self.header(service_id), &mut buf[offset..]);
        offset += varinteger::encode(method_id, &mut buf[offset..]);
        offset += varinteger::encode(self.request_id, &mut buf[offset..]);
        self.encode_body(&mut buf[offset..])?;
        Ok(())
    }

    fn encoded_len(&self, service_id: u64, method_id: u64) -> usize {
        let message_len = self.message_len(service_id, method_id);
        message_len + varinteger::length(message_len as u64)
    }
    fn is_response(&self) -> bool {
        matches!(self.body, Body::Response(_))
    }

    fn is_request(&self) -> bool {
        matches!(self.body, Body::Request(_))
    }
}

#[derive(Clone, Debug)]
pub struct Message<Req, Res> {
    body: Body<Req, Res>,
    request_id: u64,
}

#[derive(Clone, Debug)]
pub enum Body<Req, Res> {
    Request(Req),
    Response(Res),
    Error(RpcError),
}

impl<Req, Res> Message<Req, Res>
where
    Req: prost::Message + Default + Sized,
    Res: prost::Message + Default + Sized,
{
    fn request(request: Req) -> Self {
        Self {
            body: Body::Request(request),
            request_id: 0,
        }
    }
    fn response(response: Res) -> Self {
        Self {
            body: Body::Response(response),
            request_id: 0,
        }
    }

    fn from_parts(typ: u8, request_id: u64, body: &[u8]) -> Result<Self, prost::DecodeError> {
        let body = match typ {
            0 => Body::Request(Req::decode(body)?),
            1 => Body::Response(Res::decode(body)?),
            _ => return Err(prost::DecodeError::new("Invalid message type")),
        };
        Ok(Self { request_id, body })
    }

    fn body(&self) -> &Body<Req, Res> {
        &self.body
    }

    pub fn as_request(&self) -> Option<&Req> {
        match &self.body {
            Body::Request(req) => Some(req),
            _ => None,
        }
    }

    pub fn as_response(&self) -> Option<&Res> {
        match &self.body {
            Body::Response(res) => Some(res),
            _ => None,
        }
    }

    pub fn into_response(self) -> Option<Res> {
        match self.body {
            Body::Response(res) => Some(res),
            _ => None,
        }
    }

    pub fn into_request(self) -> Option<Req> {
        match self.body {
            Body::Request(req) => Some(req),
            _ => None,
        }
    }

    pub fn convert_to_response(&mut self, response: Res) {
        self.body = Body::Response(response)
    }

    pub fn replace_body(&mut self, body: Body<Req, Res>) {
        self.body = body
    }

    pub fn reply(&self, response: Res) -> Result<Message<Req, Res>, RpcError>
// where
    //     F: Fn(Message<Req, Res>) -> S,
    {
        if !matches!(self.body, Body::Request(_)) {
            Err(RpcError::MessageMismatch)
        } else {
            let message = Self {
                request_id: self.request_id,
                body: Body::Response(response),
            };
            // let message = map(message);
            Ok(message)
        }
    }

    fn typ(&self) -> u8 {
        match self.body {
            Body::Request(_) => 0,
            Body::Response(_) => 1,
            Body::Error(_) => 2,
        }
    }

    fn message_len(&self, service_id: u64, method_id: u64) -> usize {
        varinteger::length(self.header(service_id))
            + varinteger::length(method_id)
            + varinteger::length(self.request_id)
            + self.encoded_body_len()
    }

    fn header(&self, service_id: u64) -> u64 {
        (service_id << 2) | self.typ() as u64
    }

    // fn header_len(&self, ) -> u64 {
    //     varinteger::length(self.header())
    //         + varinteger::length(self.method)
    //         + varinteger::length(self.id)
    // }

    pub fn encode_body(&self, mut buf: &mut [u8]) -> Result<(), prost::EncodeError> {
        match &self.body {
            Body::Request(req) => req.encode(&mut buf),
            Body::Response(res) => res.encode(&mut buf),
            Body::Error(_e) => panic!("not yet supported"),
        }
    }

    pub fn encoded_body_len(&self) -> usize {
        match &self.body {
            Body::Request(req) => req.encoded_len(),
            Body::Response(res) => res.encoded_len(),
            Body::Error(_e) => panic!("not yet supported"),
        }
    }
}

pub type OutboundRequest<S> = (S, Option<oneshot::Sender<S>>);

pub struct RpcHandle<S> {
    pub task: JoinHandle<Result<(), RpcError>>,
    outbound_tx: Sender<OutboundRequest<S>>,
    inbound_rx: Receiver<S>,
    // cancel_tx: oneshot::Sender<oneshot::Sender<()>>,
}

impl<S> Future for RpcHandle<S> {
    type Output = Result<(), RpcError>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.task).poll(cx)
    }
}

impl<S> RpcHandle<S>
where
    S: Services,
{
    pub async fn request(&self, request: S) -> Result<S, RpcError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.outbound_tx
            .send((request, Some(reply_tx)))
            .await
            .unwrap();
        Ok(reply_rx.await.unwrap())
    }

    pub async fn request_no_reply(&self, request: S) -> Result<(), RpcError> {
        self.outbound_tx.send((request, None)).await.unwrap();
        Ok(())
    }

    // pub async fn cancel(self) {
    //     let (oncancel_tx, oncancel_rx) = oneshot::channel();
    //     self.cancel_tx.send(oncancel_tx).unwrap();
    //     oncancel_rx.await.unwrap();
    //     // self.task.cancel().await
    // }

    // pub async fn reply<Req,Res>(&self, request: Message<Req,Res>, mut response: Res) -> Result<(), RpcError> {
    //     let response = request.inner().reply(response);
    //     // let request_id = request.inner().request_id();
    //     // response.inner_mut().set_request_id(request_id);
    //     self.outbound_tx.send((response, None)).await.unwrap();
    //     Ok(())
    // }
}

impl<S> Stream for RpcHandle<S> {
    type Item = S;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inbound_rx).poll_next(cx)
    }
}

#[derive(Debug)]
pub enum Event<S> {
    Inbound(Result<S, RpcError>),
    Outbound(OutboundRequest<S>),
    // Cancel(Result<oneshot::Sender<()>, Canceled>),
}

pub fn run_rpc<S, R, W>(
    // rpc: Rpc<Req, Res>,
    reader: R,
    writer: W,
) -> Result<RpcHandle<S>, RpcError>
where
    S: Services + Send + Unpin + 'static,
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    let (outbound_tx, outbound_rx) = async_channel::unbounded();
    let (inbound_tx, inbound_rx) = async_channel::unbounded();
    // let (cancel_tx, cancel_rx) = oneshot::channel();
    let task = task::spawn(run_loop::<S, R, W>(
        reader,
        writer,
        outbound_rx,
        inbound_tx,
        // cancel_rx,
    ));
    let handle = RpcHandle {
        task,
        outbound_tx,
        inbound_rx,
        // cancel_tx,
    };
    Ok(handle)
}

async fn run_loop<S, R, W>(
    // rpc: Rpc<Req, Res>,
    reader: R,
    mut writer: W,
    outbound_rx: Receiver<OutboundRequest<S>>,
    inbound_tx: Sender<S>,
    // cancel_rx: oneshot::Receiver<oneshot::Sender<()>>,
) -> Result<(), RpcError>
where
    S: Services,
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    let inbound = Decoder::decode(reader)
        .map(upcast_message::<S>)
        .map(Event::Inbound);
    let outbound_rx = outbound_rx.map(Event::Outbound);
    // let cancel_rx = cancel_rx.into_stream();
    // let cancel_rx = cancel_rx.map(|oncancel_tx| Event::Cancel(oncancel_tx));
    // let mut events = inbound.merge(outbound_rx).merge(cancel_rx);
    let mut events = inbound.merge(outbound_rx);

    let sessions: Sessions<oneshot::Sender<S>> = Sessions::new();

    let mut write_buf = vec![0u8; 1024];
    while let Some(event) = events.next().await {
        match event {
            Event::Inbound(message) => {
                let message = message?;
                log::debug!("incoming {:?}", message);
                let (is_response, request_id) = {
                    let inner = message.inner();
                    (inner.is_response(), inner.request_id())
                };
                if is_response {
                    if let Some(reply_tx) = sessions.take(&request_id) {
                        reply_tx.send(message).unwrap();
                    } else {
                        // TODO: Report error?
                    }
                } else {
                    inbound_tx.send(message).await.unwrap();
                }
            }
            Event::Outbound((mut message, reply_tx)) => {
                log::debug!("outgoing {:?}", message);
                let is_request = message.inner().is_request();
                if is_request {
                    if let Some(reply_tx) = reply_tx {
                        let request_id = sessions.insert(reply_tx);
                        message.inner_mut().set_request_id(request_id);
                    }
                }
                send_message(&mut writer, message, &mut write_buf).await?;
            } // Event::Cancel(oncancel_tx) => {
              //     log::debug!("cancel");
              //     if let Ok(oncancel_tx) = oncancel_tx {
              //         oncancel_tx.send(()).unwrap();
              //     }
              //     return Ok(());
              // }
        }
    }
    Ok(())
}

async fn send_message<W, S>(writer: &mut W, message: S, buf: &mut Vec<u8>) -> Result<(), RpcError>
where
    W: AsyncWrite + Send + Unpin + 'static,
    S: Services,
{
    let len = message.encoded_len();
    if len > buf.len() {
        buf.resize(len, 0u8);
    }
    message.encode(&mut buf[..len])?;
    writer.write_all(&buf[..len]).await?;
    writer.flush().await?;
    Ok(())
}

fn upcast_message<S>(message: io::Result<RawMessage>) -> Result<S, RpcError>
where
    S: Services,
{
    match message {
        Ok(message) => S::from_parts(
            message.typ_u8(),
            message.service(),
            message.method(),
            message.id(),
            &message.body,
        )
        .map_err(|e| e.into()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
pub mod tests {
    use super::schema::*;
    use super::*;
    use async_std::prelude::FutureExt;
    use async_std::task;
    use async_trait::async_trait;

    // "Server"
    struct MyState;

    #[async_trait]
    impl demo_server::CalcServer for MyState {
        async fn on_add(&mut self, req: &AddRequest) -> Result<CalcResponse, RpcError> {
            Ok(CalcResponse {
                result: req.a + req.b,
            })
        }
    }

    #[async_std::test]
    async fn demo() -> Result<(), RpcError> {
        env_logger::init();
        type TaskHandle = JoinHandle<Result<(), RpcError>>;
        let (ra, wb) = sluice::pipe::pipe();
        let (rb, wa) = sluice::pipe::pipe();

        let mut handle_a = run_rpc::<DemoServices, _, _>(ra, wa)?;
        let mut handle_b = run_rpc::<DemoServices, _, _>(rb, wb)?;

        // "Client"
        let task_a: TaskHandle = task::spawn(async move {
            let res = demo_client::calc::add(&mut handle_a, AddRequest { a: 2, b: 13 }).await?;
            assert_eq!(res, CalcResponse { result: 15 });
            Ok(())
        });

        // "Server"
        let task_b: TaskHandle = task::spawn(async move {
            let mut state = MyState {};
            if let Some(incoming) = handle_b.next().await {
                state.handle_request(&mut handle_b, incoming).await?;
            }
            Ok(())
        });

        task_a.try_join(task_b).await?;
        Ok(())
    }
}
// async fn timeout(ms: u64) {
//     let _ = async_std::future::timeout(
//         std::time::Duration::from_millis(ms),
//         futures::future::pending::<()>(),
//     )
//     .await;
// }
// let req = DemoServices::Calc(CalcService::Add(Message::request(AddRequest {
//     a: 5,
//     b: 2,
// })));
// let res = handle_a.request(req).await;
// eprintln!("TASK A GOT RES 1: {:?}", res);

// match incoming {
//     DemoServices::Calc(CalcService::Add(message)) => {
//         let request = message.as_request().unwrap();
//         let result = request.a + request.b;
//         eprintln!("RECV REQ: {:?}", request);
//         let response = CalcResponse { result };
//         let response = message.reply(response)?;
//         let response = DemoServices::Calc(CalcService::Add(response));
//         handle_b.request_no_reply(response).await?;
//     }
//     _ => {}
// }

// fn encode(&self, buf: &mut [u8]) -> Result<(), prost::EncodeError> {
//     match self {
//         Self::Calc(service) => service.encode(buf),
//         Self::Text(service) => service.encode(buf),
//     }
// }

// fn encoded_len(&self) -> usize {
//     match self {
//         Self::Calc(service) => service.encoded_len(),
//         Self::Text(service) => service.encoded_len(),
//     }
// }

// fn inner_mut(&mut self) -> &mut dyn MessageT {
//     match self {
//         Self::Calc(service) => service.inner_mut(),
//         Self::Text(service) => service.inner_mut(),
//     }
// }
// fn inner(&self) -> &dyn MessageT {
//     match self {
//         Self::Calc(service) => service.inner(),
//         Self::Text(service) => service.inner(),
//     }
// }
// impl Service for DemoServices {
//     fn service_id(&self) -> u64 {
//         match self {
//             Self::Calc(method) => method.service_id(),
//             Self::Text(method) => method.service_id(),
//         }
//     }

//     fn method_id(&self) -> u64 {
//         match self {
//             Self::Calc(method) => method.method_id(),
//             Self::Text(method) => method.method_id(),
//         }
//     }
// }
