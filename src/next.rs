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
            use CalcService::*;
            use DemoServices::*;

            pub async fn add(
                handle: &mut RpcHandle<DemoServices>,
                req: AddRequest,
            ) -> Result<CalcResponse, RpcError> {
                match handle.request(Calc, Add, req).await? {
                    Calc(Add(message)) => message.into_response(),
                    _ => Err(RpcError::MessageMismatch),
                }
            }

            pub async fn square(
                handle: &mut RpcHandle<DemoServices>,
                req: SquareRequest,
            ) -> Result<CalcResponse, RpcError> {
                match handle.request(Calc, Square, req).await? {
                    Calc(Square(message)) => message.into_response(),
                    _ => Err(RpcError::MessageMismatch),
                }
            }
        }
    }

    pub mod demo_server {
        use super::*;
        use CalcService::*;
        use DemoServices::*;

        #[async_trait]
        pub trait CalcServer {
            async fn handle_request(
                &mut self,
                handle: &mut RpcHandle<DemoServices>,
                req: DemoServices,
            ) -> Result<(), RpcError> {
                match req {
                    Calc(Add(message)) => {
                        let req = message.as_request()?;
                        let res = self.on_add(req).await;
                        handle.reply(message, Calc, Add, res).await
                    }
                    Calc(Square(message)) => {
                        let req = message.as_request()?;
                        let res = self.on_square(req).await;
                        handle.reply(message, Calc, Square, res).await
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

use crate::ErrorMessage;
use crate::{sessions::Sessions, Decoder, Message as RawMessage};
use async_std::channel::{Receiver, Sender};
use async_std::stream::StreamExt;
use async_std::task::{self, JoinHandle};
use prost::Message as ProstMessage;
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
    #[error("Remote error: {}", .0.message)]
    Remote(ErrorMessage),
}

impl RpcError {
    fn to_message(&self) -> ErrorMessage {
        match self {
            RpcError::Remote(message) => message.clone(),
            _ => ErrorMessage {
                message: format!("{}", self),
                ..Default::default()
            },
        }
    }
}

impl From<io::Error> for RpcError {
    fn from(e: io::Error) -> Self {
        Arc::new(e).into()
    }
}

impl From<ErrorMessage> for RpcError {
    fn from(e: ErrorMessage) -> Self {
        Self::Remote(e)
    }
}

pub trait RpcMessageBody: prost::Message + Default + Sized + 'static {}

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

    fn encode(&self, buf: &mut [u8]) -> Result<(), RpcError> {
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
    fn encode(&self, buf: &mut [u8]) -> Result<(), RpcError> {
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
    fn encode(&self, buf: &mut [u8], service_id: u64, method_id: u64) -> Result<(), RpcError>;
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

    fn encode(&self, buf: &mut [u8], service_id: u64, method_id: u64) -> Result<(), RpcError> {
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
        matches!(self.body, Body::Response(_) | Body::Error(_))
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
    pub fn request(request: Req) -> Self {
        Self {
            body: Body::Request(request),
            request_id: 0,
        }
    }

    pub fn response(response: Res) -> Self {
        Self {
            body: Body::Response(response),
            request_id: 0,
        }
    }

    pub fn from_parts(typ: u8, request_id: u64, body: &[u8]) -> Result<Self, prost::DecodeError> {
        use prost::Message;
        let body = match typ {
            0 => Body::Request(Req::decode(body)?),
            1 => Body::Response(Res::decode(body)?),
            2 => Body::Error(ErrorMessage::decode(body)?.into()),
            _ => return Err(prost::DecodeError::new("Invalid message type")),
        };
        Ok(Self { request_id, body })
    }

    pub fn body(&self) -> &Body<Req, Res> {
        &self.body
    }

    pub fn as_request(&self) -> Result<&Req, RpcError> {
        match &self.body {
            Body::Request(req) => Ok(req),
            _ => Err(RpcError::MessageMismatch),
        }
    }

    pub fn as_response(&self) -> Result<&Res, RpcError> {
        match &self.body {
            Body::Response(res) => Ok(res),
            _ => Err(RpcError::MessageMismatch),
        }
    }

    pub fn into_response(self) -> Result<Res, RpcError> {
        match self.body {
            Body::Response(res) => Ok(res),
            Body::Error(err) => Err(err),
            Body::Request(_) => Err(RpcError::MessageMismatch),
        }
    }

    pub fn convert_to_response(&mut self, response: Res) {
        self.body = Body::Response(response)
    }

    pub fn replace_body(&mut self, body: Body<Req, Res>) {
        self.body = body
    }

    pub fn reply(&self, response: Result<Res, RpcError>) -> Result<Message<Req, Res>, RpcError> {
        if !matches!(self.body, Body::Request(_)) {
            Err(RpcError::MessageMismatch)
        } else {
            let body = match response {
                Ok(res) => Body::Response(res),
                Err(e) => Body::Error(e),
            };
            let message = Self {
                request_id: self.request_id,
                body,
            };
            Ok(message)
        }
    }

    pub fn encode_body(&self, mut buf: &mut [u8]) -> Result<(), RpcError> {
        match &self.body {
            Body::Request(req) => req.encode(&mut buf)?,
            Body::Response(res) => res.encode(&mut buf)?,
            Body::Error(e) => e.to_message().encode(&mut buf)?,
        };
        Ok(())
    }

    pub fn encoded_body_len(&self) -> usize {
        match &self.body {
            Body::Request(req) => req.encoded_len(),
            Body::Response(res) => res.encoded_len(),
            Body::Error(e) => e.to_message().encoded_len(),
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
}

pub type OutboundRequest<S> = (S, Option<oneshot::Sender<S>>);

pub struct RpcHandle<S> {
    pub task: JoinHandle<Result<(), RpcError>>,
    outbound_tx: Sender<OutboundRequest<S>>,
    inbound_rx: Option<Receiver<S>>,
}

impl<S> Future for RpcHandle<S> {
    type Output = Result<(), RpcError>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.task).poll(cx)
    }
}
fn upcast<Co, Ci, S, Si, Req, Res>(outer_c: Co, inner_c: Ci, message: Message<Req, Res>) -> S
where
    Co: Fn(Si) -> S,
    Ci: Fn(Message<Req, Res>) -> Si,
{
    let service = (inner_c)(message);
    let services = (outer_c)(service);
    services
}

impl<S> RpcHandle<S>
where
    S: Services,
{
    pub async fn send_request(&self, message: S) -> Result<S, RpcError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.outbound_tx
            .send((message, Some(reply_tx)))
            .await
            .unwrap();
        Ok(reply_rx.await.unwrap())
    }

    pub async fn send_no_reply(&self, message: S) -> Result<(), RpcError> {
        self.outbound_tx.send((message, None)).await.unwrap();
        Ok(())
    }

    pub async fn request<CS, CM, Me, Req, Res>(
        &self,
        service: CS,
        method: CM,
        request: Req,
    ) -> Result<S, RpcError>
    where
        CS: Fn(Me) -> S,
        CM: Fn(Message<Req, Res>) -> Me,
        Req: prost::Message + Default + Sized + 'static,
        Res: prost::Message + Default + Sized + 'static,
    {
        let message = Message::request(request);
        let wrapped = (service)((method)(message));
        let reply = self.send_request(wrapped).await;
        reply
    }

    pub async fn request_no_reply<CS, CM, Me, Req, Res>(
        &self,
        service: CS,
        method: CM,
        request: Req,
    ) -> Result<(), RpcError>
    where
        CS: Fn(Me) -> S,
        CM: Fn(Message<Req, Res>) -> Me,
        Req: prost::Message + Default + Sized + 'static,
        Res: prost::Message + Default + Sized + 'static,
    {
        let message = Message::request(request);
        let wrapped = (service)((method)(message));
        self.send_no_reply(wrapped).await
    }

    pub async fn reply<CS, CM, Me, Req, Res>(
        &self,
        request_message: Message<Req, Res>,
        service: CS,
        method: CM,
        response: Result<Res, RpcError>,
    ) -> Result<(), RpcError>
    where
        CS: Fn(Me) -> S,
        CM: Fn(Message<Req, Res>) -> Me,
        Req: prost::Message + Default + Sized + 'static,
        Res: prost::Message + Default + Sized + 'static,
    {
        let message = request_message.reply(response)?;
        let wrapped = (service)((method)(message));
        self.send_no_reply(wrapped).await?;
        Ok(())
    }

    pub fn take_receiver(&mut self) -> Option<Receiver<S>> {
        self.inbound_rx.take()
    }
}

impl<S> Stream for RpcHandle<S> {
    type Item = S;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(inbound_rx) = self.inbound_rx.as_mut() {
            Pin::new(inbound_rx).poll_next(cx)
        } else {
            Poll::Ready(None)
        }
    }
}

#[derive(Debug)]
pub enum Event<S> {
    Inbound(Result<S, RpcError>),
    Outbound(OutboundRequest<S>),
}

pub fn run_rpc<S, R, W>(reader: R, writer: W) -> Result<RpcHandle<S>, RpcError>
where
    S: Services + Send + Unpin + 'static,
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    let (outbound_tx, outbound_rx) = async_channel::unbounded();
    let (inbound_tx, inbound_rx) = async_channel::unbounded();
    let task = task::spawn(run_loop::<S, R, W>(reader, writer, outbound_rx, inbound_tx));
    let handle = RpcHandle {
        task,
        outbound_tx,
        inbound_rx: Some(inbound_rx),
    };
    Ok(handle)
}

async fn run_loop<S, R, W>(
    reader: R,
    mut writer: W,
    outbound_rx: Receiver<OutboundRequest<S>>,
    inbound_tx: Sender<S>,
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
                        // We received a reply or an error with a request_id
                        // that we did not prepare.
                        // TODO: What to do here - return with error, ignore?
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
            }
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
    use async_std::prelude::*;
    use async_std::task;
    use async_trait::async_trait;
    use futures::future::FutureExt as FuturesFutureExt;

    // "Server"
    struct MyState;

    #[async_trait]
    impl demo_server::CalcServer for MyState {
        async fn on_add(&mut self, req: &AddRequest) -> Result<CalcResponse, RpcError> {
            Ok(CalcResponse {
                result: req.a + req.b,
            })
        }
        async fn on_square(&mut self, req: &SquareRequest) -> Result<CalcResponse, RpcError> {
            if req.num == 0 {
                Err(RpcError::Remote(ErrorMessage::new("dont square the zero")))
            } else {
                let result = req.num * req.num;
                Ok(CalcResponse { result })
            }
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

        let (close_tx, close_rx) = oneshot::channel::<()>();

        // "Client"
        let task_a: TaskHandle = task::spawn(async move {
            let res = demo_client::calc::add(&mut handle_a, AddRequest { a: 2, b: 13 }).await?;
            assert_eq!(res, CalcResponse { result: 15 });
            let res = demo_client::calc::square(&mut handle_a, SquareRequest { num: 0 }).await;
            match res {
                Err(RpcError::Remote(e)) => assert_eq!(&e.message, "dont square the zero"),
                _ => panic!("expected error, got ok"),
            }
            close_tx.send(()).unwrap();
            Ok(())
        });

        // "Server"
        let task_b: TaskHandle = task::spawn(async move {
            enum Event {
                Request(DemoServices),
                Cancel,
            }
            let mut events = StreamExt::map(handle_b.take_receiver().unwrap(), Event::Request)
                .merge(close_rx.into_stream().map(|_e| Event::Cancel));
            let mut state = MyState {};
            while let Some(event) = events.next().await {
                match event {
                    Event::Request(request) => {
                        state.handle_request(&mut handle_b, request).await?;
                    }
                    Event::Cancel => return Ok(()),
                }
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

// fn encode(&self, buf: &mut [u8]) -> Result<(), RpcError> {
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
//
//
// VARIANTS FOR CLIENT CODEGEN
//
// macro_rules! matches {
//     ($expression:expr, $( $pattern:pat )|+ $( if $guard: expr )? $(,)?) => {
//         match $expression {
//             $( $pattern )|+ $( if $guard )? => true,
//             _ => false
//         }
//     }
// }
// let req = Calc(Add(Message::request(req)));
// let res = handle.request(req).await?;
// let req = upcast(DemoServices::Calc, CalcService::Add, Message::request(req));
// let req = upcast(Calc, Add, Message::request(req));
// let req = DemoServices::Calc(CalcService::Add(Message::request(req)));
// match_response!(message, Calc(Add(message)));
// match res {
//     DemoServices::Calc(CalcService::Add(message)) if message.is_response() => {
//         message.into_response()
//     }
//     _ => Err(RpcError::MessageMismatch),
// }
//
//
//
//
// OLD SERVER handle_request
// let res = DemoServices::Calc(CalcService::Add(message.reply(res)?));
// handle.request_no_reply(res).await?;
// Ok(())

// pub async fn request<Req>(&self, request: Req) -> S {
// }

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
