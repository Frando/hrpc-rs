use crate::Decoder;
use async_std::sync::{Arc, Mutex};
use async_std::task;
use futures::channel::mpsc;
// use async_std::channel as mpsc;
use futures::channel::oneshot;
use futures::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use log::*;
use prost::Message as ProstMessage;
use std::collections::{HashMap, VecDeque};
use std::io::{Error, ErrorKind, Result};

use crate::message::{EncodeBody, Message, MessageType};

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
struct Sessions<T> {
    inner: HashMap<u64, T>,
    free: VecDeque<u64>,
}

impl<T> Sessions<T> {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
            free: VecDeque::new(),
        }
    }

    // pub fn get_mut(&mut self, id: &u64) -> Option<&mut T> {
    //     self.inner.get_mut(id)
    // }

    pub fn insert(&mut self, item: T) -> u64 {
        let id = if let Some(id) = self.free.pop_front() {
            id
        } else {
            self.inner.len() as u64 + 1
        };
        self.inner.insert(id, item);
        id
    }
    pub fn take(&mut self, id: &u64) -> Option<T> {
        if let Some(item) = self.inner.remove(id) {
            self.free.push_back(*id);
            Some(item)
        } else {
            None
        }
    }
}

type ClientSessions = Sessions<oneshot::Sender<Message>>;

pub struct Rpc {
    services: Services,
    client: Client,
    outgoing_requests_receiver: OutgoingRequestReceiver,
}

impl Rpc {
    pub fn new() -> Self {
        let services = Services::new();
        let (client, outgoing_requests_receiver) = Client::new();
        Self {
            services,
            client,
            outgoing_requests_receiver,
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

    pub async fn connect_rw<R, W>(self, reader: R, writer: W) -> Result<()>
    where
        R: AsyncRead + Send + Unpin + 'static,
        W: AsyncWrite + Send + Unpin + 'static,
    {
        let Self {
            outgoing_requests_receiver,
            services,
            client: _client,
        } = self;

        let services = Arc::new(Mutex::new(services));

        let sessions: ClientSessions = Sessions::new();
        let sessions = Arc::new(Mutex::new(sessions));

        let (outgoing_messages_sender, outgoing_messages_receiver) = mpsc::channel(100);

        let mut tasks = vec![];

        // Task to handle incoming request.
        tasks.push(task::spawn(incoming(
            reader,
            services.clone(),
            sessions.clone(),
            outgoing_messages_sender.clone(),
        )));

        // Task to send outgoing messages.
        tasks.push(task::spawn(outgoing_send(
            writer,
            outgoing_messages_receiver,
        )));

        // Task to forward outgoing client requests to the send task.
        tasks.push(task::spawn(outgoing_requests(
            sessions,
            outgoing_requests_receiver,
            outgoing_messages_sender.clone(),
        )));

        futures::future::join_all(tasks).await;

        Ok(())
    }
}

async fn incoming<R>(
    reader: R,
    services: Arc<Mutex<Services>>,
    sessions: Arc<Mutex<ClientSessions>>,
    mut outgoing_messages_sender: mpsc::Sender<Message>,
) -> Result<()>
where
    R: AsyncRead + Send + Unpin + 'static,
{
    let mut decoder = Decoder::decode(reader);
    while let Some(message) = decoder.next().await {
        let message = message?;
        debug!("recv {:?}", message);
        match message.typ() {
            // Incoming request.
            MessageType::Request => {
                let request = Request::from_message(message);
                let response = {
                    let mut services = services.lock().await;
                    services.handle_request(request).await
                };
                if let Some(message) = response {
                    // TODO: Handle channel drop error?
                    outgoing_messages_sender.send(message).await.unwrap();
                };
            }
            // Incoming response or error.
            MessageType::Response | MessageType::Error => {
                let mut sessions = sessions.lock().await;
                let reply_sender = sessions.take(&message.id);
                if let Some(reply_sender) = reply_sender {
                    reply_sender.send(message).unwrap();
                } else {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        "Received response with invalid request ID",
                    ));
                }
            }
        };
    }
    Ok(())
}

async fn outgoing_requests(
    sessions: Arc<Mutex<ClientSessions>>,
    mut outgoing_requests_receiver: OutgoingRequestReceiver,
    mut outgoing_messages_sender: mpsc::Sender<Message>,
) -> Result<()> {
    while let Some(outgoing_request) = outgoing_requests_receiver.next().await {
        let (mut message, reply_sender) = outgoing_request;
        let id = {
            let mut sessions = sessions.lock().await;
            sessions.insert(reply_sender)
        };
        message.set_request(id);
        // TODO: Handle dropped channel / map err?
        outgoing_messages_sender.send(message).await.unwrap();
    }
    Ok(())
}

async fn outgoing_send<W>(
    mut writer: W,
    mut outgoing_messages_receiver: mpsc::Receiver<Message>,
) -> Result<()>
where
    W: AsyncWrite + Send + Unpin + 'static,
{
    // TODO: Don't allocate for each message.
    // let buf = vec![0u8; MAX_MESSAGE_SIZE];
    while let Some(message) = outgoing_messages_receiver.next().await {
        debug!("send {:?}", message);
        let mut buf = vec![0u8; message.encoded_len()];
        // let len = message.encoded_len();
        message.encode(&mut buf[..])?;
        writer.write_all(&buf[..]).await?;
        writer.flush().await?;
    }
    Ok(())
}

pub struct Request {
    message: Message,
}
impl Request {
    pub fn new(service: u64, method: u64, body: impl EncodeBody) -> Self {
        Request::from_message(Message::request(service, method, body))
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
    // pub(crate) fn set_address(&mut self, address: Address) {
    //     self.message.set_address(address)
    // }
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
    request_sender: OutgoingRequestSender,
}
pub type OutgoingRequestSender = mpsc::Sender<(Message, oneshot::Sender<Message>)>;
pub type OutgoingRequestReceiver = mpsc::Receiver<(Message, oneshot::Sender<Message>)>;

impl Client {
    pub fn new() -> (Self, OutgoingRequestReceiver) {
        let (sender, receiver) = mpsc::channel(100);
        let client = Self {
            request_sender: sender,
        };
        (client, receiver)
    }
    pub async fn request(
        &mut self,
        service: u64,
        method: u64,
        body: impl EncodeBody,
    ) -> Result<Vec<u8>> {
        let request = Request::new(service, method, body);
        let (reply_sender, onreply_recv) = oneshot::channel();

        let message = request.take_message();
        self.request_sender
            .send((message, reply_sender))
            .await
            .unwrap();
        let res = onreply_recv.await;
        let message = res.map_err(|_| Error::new(ErrorKind::Other, "Channel dropped"))?;
        if message.is_error() {
            let error_message = String::from_utf8(message.body)
                .unwrap_or("Error: Cannot decode error message".into());
            Err(Error::new(ErrorKind::Other, error_message))
        } else {
            Ok(message.body)
        }
    }

    pub async fn request_into<T>(
        &mut self,
        service: u64,
        method: u64,
        body: impl EncodeBody,
    ) -> Result<T>
    where
        T: ProstMessage + Default,
    {
        let response_body = self.request(service, method, body).await?;
        let response_body = T::decode(&response_body[..])?;
        Ok(response_body)
    }
}

// pub type IncomingRequestSender = mpsc::Sender<Message>;
// pub type IncomingRequestReceiver = mpsc::Receiver<Message>;
