use crate::Decoder;
use futures::channel::mpsc;
use futures::channel::oneshot;
use futures::future::FutureExt;
use futures::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use log::*;
use prost::Message as ProstMessage;
use std::collections::HashMap;
use std::future::Future;
use std::io::{Error, ErrorKind, Result};
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::message::{EncodeBody, Message, MessageType};
use crate::sessions::Sessions;

#[macro_export]
macro_rules! error_other {
    ($message:expr) => {
        Err(Error::new(ErrorKind::Other, $message))
    };
}

struct Services {
    inner: HashMap<u64, Box<dyn Service>>,
}
impl Services {
    fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }
    fn insert(&mut self, service: Box<dyn Service>) {
        self.inner.insert(service.id(), service);
    }

    async fn handle_request(&mut self, request: Request) -> Option<Message> {
        let service = self.inner.get_mut(&request.service());
        let address = request.address();
        let id = request.id();
        let response = match service {
            Some(service) => {
                let response = service.handle_request(request).await;
                if id != 0 {
                    let response = match response {
                        Ok(response) => response,
                        Err(error) => Response::error(error),
                    };
                    Some(response)
                } else {
                    None
                }
            }
            None => {
                if id != 0 {
                    Some(Response::error("Service not implemented"))
                } else {
                    None
                }
            }
        };
        if let Some(mut response) = response {
            response.set_address(address);
            Some(response.take_message())
        } else {
            None
        }
    }
}

type ClientSessions = Sessions<oneshot::Sender<Message>>;

pub struct Rpc {
    services: Services,
    client: Client,
    out_request_rx: OutRequestReceiver,
}

impl Rpc {
    pub fn new() -> Self {
        let services = Services::new();
        let (client, out_request_rx) = Client::new();
        Self {
            services,
            client,
            out_request_rx,
        }
    }

    pub fn define_service<S>(&mut self, service: S)
    where
        S: Service + Send + 'static,
    {
        self.services.insert(Box::new(service));
    }

    pub fn client(&mut self) -> Client {
        self.client.clone()
    }

    pub async fn connect<S>(self, stream: S) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Clone + Send + Unpin + 'static,
    {
        self.connect_rw(stream.clone(), stream).await
    }

    pub async fn connect_rw<R, W>(self, reader: R, mut writer: W) -> Result<()>
    where
        R: AsyncRead + Send + Unpin + 'static,
        W: AsyncWrite + Send + Unpin + 'static,
    {
        let Self {
            out_request_rx,
            mut services,
            client: _client,
        } = self;
        enum Event {
            Incoming(Message),
            Outgoing(Message),
        }

        let sessions = ClientSessions::new();

        let (mut out_message_tx, out_message_rx) = mpsc::channel(100);

        let decoder = Decoder::decode(reader);
        let stream_incoming =
            decoder.map(|message| message.map(|message| Event::Incoming(message)));

        let sessions_clone = sessions.clone();
        let stream_outgoing = out_request_rx.map(move |out_request| {
            let (mut message, reply_tx) = out_request;
            let id = sessions_clone.insert(reply_tx);
            message.set_request(id);
            Ok(Event::Outgoing(message))
        });

        let events = futures::stream::select(stream_incoming, stream_outgoing);
        let mut events = futures::stream::select(events, out_message_rx);
        let mut send_buf = Vec::new();

        while let Some(event) = events.next().await {
            let event = event?;
            match event {
                // Incoming message.
                Event::Incoming(message) => {
                    debug!("recv {:?}", message);
                    match message.typ() {
                        // Incoming request.
                        MessageType::Request => {
                            let request = Request::from_message(message);
                            let response = services.handle_request(request).await;
                            if let Some(message) = response {
                                // TODO: Handle channel drop error?
                                out_message_tx
                                    .send(Ok(Event::Outgoing(message)))
                                    .await
                                    .unwrap();
                            };
                        }
                        // Incoming response or error.
                        MessageType::Response | MessageType::Error => {
                            let reply_tx = sessions.take(&message.id);
                            if let Some(reply_tx) = reply_tx {
                                // Ignore errors on dropped reply receivers.
                                let _ = reply_tx.send(message);
                            } else {
                                return Err(Error::new(
                                    ErrorKind::InvalidData,
                                    "Received response with invalid request ID",
                                ));
                            }
                        }
                    };
                }
                Event::Outgoing(message) => {
                    debug!("send {:?}", message);
                    send_message(&mut writer, &mut send_buf, message).await?;
                }
            }
        }

        Ok(())
    }
}

async fn send_message<W>(writer: &mut W, buf: &mut Vec<u8>, message: Message) -> Result<()>
where
    W: AsyncWrite + Send + Unpin + 'static,
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

pub struct Request {
    message: Message,
}
impl Request {
    pub fn new(service: u64, method: u64, body: impl EncodeBody) -> Self {
        Self {
            message: Message::new_request(service, method, body),
        }
    }

    pub fn from_message(message: Message) -> Self {
        Self { message }
    }
    pub fn method(&self) -> u64 {
        self.message.method
    }
    pub fn service(&self) -> u64 {
        self.message.service
    }
    pub fn body(&self) -> &[u8] {
        &self.message.body
    }
    pub fn id(&self) -> u64 {
        self.message.id
    }
    pub(crate) fn address(&self) -> Address {
        self.message.address()
    }
    pub(crate) fn take_message(self) -> Message {
        self.message
    }
}

pub struct Response {
    message: Message,
}

/// Service, Method, Id
type Address = (u64, u64, u64);

impl Response {
    pub fn from_message(message: Message) -> Self {
        Self { message }
    }
    pub fn from_request(request: Request, body: impl EncodeBody) -> Self {
        Response::from_message(request.take_message().into_response(Ok(body)))
    }
    pub fn error(error: impl ToString) -> Self {
        Response::from_message(Message::prepare_error(error))
    }
    // pub(crate) fn address(&self) -> Address {
    //     self.message.address()
    // }
    pub(crate) fn set_address(&mut self, address: Address) {
        self.message.set_address(address)
    }
    pub(crate) fn take_message(self) -> Message {
        self.message
    }
}
impl<T> From<T> for Response
where
    T: EncodeBody,
{
    fn from(body: T) -> Self {
        Response::from_message(Message::prepare_response(body))
    }
}
impl From<Message> for Response {
    fn from(message: Message) -> Self {
        Response::from_message(message)
    }
}

#[async_trait::async_trait]
pub trait Service: Send {
    async fn handle_request(&mut self, request: Request) -> Result<Response>;
    fn id(&self) -> u64;
}

// Marker trait for Codegen clients.
pub trait RpcClient {}

#[derive(Clone)]
pub struct Client {
    request_tx: OutRequestSender,
}
pub type OutRequestSender = mpsc::Sender<(Message, oneshot::Sender<Message>)>;
pub type OutRequestReceiver = mpsc::Receiver<(Message, oneshot::Sender<Message>)>;

pub struct RawRequestFuture<'a> {
    state: RequestState<'a>,
    reply_rx: oneshot::Receiver<Message>,
}
pub type SendFuture<'a> = futures::sink::Send<
    'a,
    mpsc::Sender<(Message, oneshot::Sender<Message>)>,
    (Message, oneshot::Sender<Message>),
>;

pub enum RequestState<'a> {
    Sending(SendFuture<'a>),
    Receiving,
}

pub struct RequestFuture<'a, T> {
    inner: RawRequestFuture<'a>,
    output: PhantomData<T>,
}
impl<'a, T> RequestFuture<'a, T> {
    pub fn new(client: &'a mut Client, request: Request) -> Self {
        Self {
            inner: RawRequestFuture::new(client, request),
            output: PhantomData,
        }
    }
}

// TODO: I don't know if I'm allowed to do this..
impl<'a, T> Unpin for RequestFuture<'a, T> {}

impl<'a, T> Future for RequestFuture<'a, T>
where
    T: ProstMessage + Default,
{
    type Output = Result<T>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let result = futures::ready!(self.inner.poll_unpin(cx));
        let result: Result<T> = match result {
            Err(err) => Err(err),
            Ok(message) => message.body(),
        };
        Poll::Ready(result)
    }
}

impl<'a> RawRequestFuture<'a> {
    pub fn new(client: &'a mut Client, request: Request) -> Self {
        let (reply_tx, reply_rx) = oneshot::channel();
        let message = request.take_message();
        let send_future = client.request_tx.send((message, reply_tx));
        Self {
            state: RequestState::Sending(send_future),
            reply_rx,
        }
    }
}

impl<'a> Unpin for RawRequestFuture<'a> {}

impl<'a> Future for RawRequestFuture<'a> {
    type Output = Result<Message>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        loop {
            self.state = match &mut self.state {
                RequestState::Sending(ref mut future) => {
                    let res = futures::ready!(Pin::new(future).poll(cx));
                    let _ = res.unwrap();
                    RequestState::Receiving
                }
                RequestState::Receiving => {
                    let res = futures::ready!(Pin::new(&mut self.reply_rx).poll(cx));
                    let message = res.map_err(|_| Error::new(ErrorKind::Other, "Channel dropped"));
                    // TODO: Remove unwrap.
                    let message = message.unwrap();
                    return Poll::Ready(Ok(message));
                }
            };
        }
    }
}

impl Client {
    pub fn new() -> (Self, OutRequestReceiver) {
        let (request_tx, request_rx) = mpsc::channel(100);
        let client = Self { request_tx };
        (client, request_rx)
    }
    pub fn request_raw<'a>(
        &'a mut self,
        service: u64,
        method: u64,
        body: impl EncodeBody,
    ) -> RawRequestFuture<'a> {
        let request = Request::new(service, method, body);
        RawRequestFuture::new(self, request)
    }

    pub fn request<'a, T>(
        &'a mut self,
        service: u64,
        method: u64,
        body: impl EncodeBody,
    ) -> RequestFuture<'a, T>
    where
        T: ProstMessage + Default,
    {
        let request = Request::new(service, method, body);
        RequestFuture::new(self, request)
    }
}

// pub type IncomingRequestSender = mpsc::Sender<Message>;
// pub type IncomingRequestReceiver = mpsc::Receiver<Message>;
