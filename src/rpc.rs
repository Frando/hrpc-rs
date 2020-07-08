use crate::Decoder;
use futures::channel::mpsc;
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

    async fn handle_request(&mut self, message: Message) -> Result<Option<Message>> {
        let request = Request::from_message(message);
        let service = self.inner.get_mut(&request.service());
        let address = request.address();
        let response = match service {
            Some(service) => {
                let response = service.handle_request(request).await;
                let response = match response {
                    Ok(response) => response,
                    Err(error) => Response::error(error),
                };
                Some(response)
            }
            None => Some(Response::error("Service not implemented")),
        };
        if let Some(mut response) = response {
            response.set_address(address);
            Ok(Some(response.take_message()))
        } else {
            Ok(None)
        }
    }
}
pub struct Server {
    services: Services,
    outgoing_recv: Option<OutgoingRequestReceiver>,
}

async fn send_message<W>(writer: &mut W, message: Message) -> Result<()>
where
    W: AsyncWrite + Send + Unpin + 'static,
{
    debug!("send {:?}", message);
    let mut buf = vec![0u8; message.encoded_len()];
    message.encode(&mut buf)?;
    writer.write_all(&buf).await?;
    writer.flush().await
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

impl Server {
    pub fn new() -> Self {
        Self {
            services: Services::new(),
            outgoing_recv: None,
        }
    }

    pub fn define_service<S>(&mut self, service: S)
    where
        S: Service + Send + 'static,
    {
        self.services.insert(Box::new(service));
    }

    // TODO: Rethink define_client / create_client namings.
    fn define_client(&mut self, outgoing_recv: OutgoingRequestReceiver) {
        self.outgoing_recv = Some(outgoing_recv)
    }

    pub fn create_client<T>(&mut self, client: (T, ClientBuilder)) -> T
    where
        T: RpcClient,
    {
        let (app_client, rpc_client_builder) = client;
        self.define_client(rpc_client_builder.take_receiver());
        app_client
    }

    pub async fn connect<S>(&mut self, stream: S) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Clone + Send + Unpin + 'static,
    {
        let instant = std::time::Instant::now();
        let reader = stream.clone();
        let mut writer = stream;
        let mut decoder = Decoder::decode(reader);

        let mut sessions: Sessions<oneshot::Sender<Message>> = Sessions::new();
        let mut outgoing_recv = self.outgoing_recv.take().unwrap();

        loop {
            debug!("loop in {:?}", instant.elapsed());
            futures::select! {
               message = decoder.select_next_some() => {
                    let message = message?;
                    debug!("recv {:?}", message);
                    match message.typ() {
                        // Incoming request.
                        MessageType::Request => {
                            let response = self.services.handle_request(message).await?;
                            // debug!("created response {:?}", response);
                            if let Some(message) = response {
                                send_message(&mut writer, message).await?;
                            }
                        }
                        // Incoming response.
                        MessageType::Response | MessageType::Error => {
                            let mut reply_sender = sessions.take(&message.id).unwrap();
                            reply_sender.send(message).unwrap();
                        },
                    };
               },
               mut request = outgoing_recv.select_next_some() => {
                   // TODO: Free and reuse sessions.
                   // let id = (sessions.len() + 1) as u64;
                   let (mut message, reply_sender) = request;
                   let id = sessions.insert(reply_sender);
                   message.set_request(id);
                   send_message(&mut writer, message).await?;

               }
            };
        }
    }

    // TODO: Maybe spawn tasks instead of using future::select!
    // pub async fn connect<S>(self, stream: S) -> Result<()>
    // where
    //     S: AsyncRead + AsyncWrite + Clone + Send + Unpin + 'static,
    // {
    //     let Self {
    //         services,
    //         outgoing_recv,
    //     } = self;
    //     let reader = stream.clone();
    //     let writer = stream;
    //     let (outgoing_send, outgoing_recv) = mpsc::channel(100);

    //     let read_task = self.read_loop(stream.clone());
    //     let write_task = self.write_loop(stream.clone());
    //     let tasks = FuturesUnordered::new();
    //     loop {
    //         let reader = self.reader.take();
    //     }
    // }
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

pub trait RpcClient {}

pub struct ClientBuilder {
    client: Client,
    request_receiver: OutgoingRequestReceiver,
}

impl ClientBuilder {
    pub fn new() -> Self {
        let (client, request_receiver) = Client::new();
        Self {
            client,
            request_receiver,
        }
    }

    pub fn create_client(&self) -> Client {
        self.client.clone()
    }

    pub fn take_receiver(self) -> OutgoingRequestReceiver {
        self.request_receiver
    }
}

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
