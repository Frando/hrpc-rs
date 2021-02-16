use crate::Message;
use async_std::io::BufReader;
use futures::io::AsyncRead;
use futures::stream::{FusedStream, Stream};
use log::*;
use std::collections::VecDeque;
use std::io::{Error, ErrorKind, Result};
use std::pin::Pin;
use std::task::{Context, Poll};

/// Parses the wire protocol of arpeecee.
///
/// The wire protocol is:
/// <len><header><method><id><message>
/// len header method id are varints
/// message is bytes
/// len is total len of header + method + id + message
/// typ is header & 3
/// service is header >> 2

// 4MB is the max wire message size (will be much smaller usually).
pub const MAX_MESSAGE_SIZE: u64 = 1024 * 1024 * 4;

#[derive(Debug)]
pub enum State {
    Length,
    Header,
    Method,
    Id,
    Message,
}

#[derive(Debug)]
pub struct Decoder<R>
where
    R: AsyncRead + Send + Unpin + 'static,
{
    reader: Option<BufReader<R>>,
    parser: Option<Parser>,
    buf: Vec<u8>,
    messages: VecDeque<Message>,
}

impl<R> Decoder<R>
where
    R: AsyncRead + Send + Unpin + 'static,
{
    pub fn decode(reader: R) -> Self {
        Self {
            reader: Some(BufReader::new(reader)),
            parser: Some(Parser::new()),
            buf: vec![0u8; MAX_MESSAGE_SIZE as usize * 2],
            messages: VecDeque::new(),
        }
    }
}

impl<R> Stream for Decoder<R>
where
    R: AsyncRead + Send + Unpin + 'static,
{
    type Item = Result<Message>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            if let Some(message) = self.messages.pop_front() {
                return Poll::Ready(Some(Ok(message)));
            }
            // Try to read from the underlying reader.
            let mut reader = self.reader.take().unwrap();
            let result = Pin::new(&mut reader).poll_read(cx, &mut self.buf[..]);
            self.reader = Some(reader);

            // If we didn't read anything, return Pending.
            let n = futures::ready!(result)?;

            // Consume all the data we read.
            let mut parser = self.parser.take().unwrap();
            let mut ptr = 0;
            while ptr < n {
                let (consumed, message) = parser.recv(&self.buf[ptr..n])?;
                ptr += consumed;
                if let Some(message) = message {
                    self.messages.push_back(message);
                }
                if consumed == 0 {
                    // TODO: See if we can enforce this statically.
                    unreachable!("Parser is broken: did not consume any bytes");
                }
            }
            self.parser = Some(parser);
        }
    }
}

impl<R> FusedStream for Decoder<R>
where
    R: AsyncRead + Send + Unpin + 'static,
{
    fn is_terminated(&self) -> bool {
        false
    }
}

#[derive(Debug)]
pub struct Parser {
    state: State,
    varint: VarintParser,
    consumed: usize,
    length: u64,
    header: u64,
    method: u64,
    id: u64,
    body: Vec<u8>,
    body_len: usize,
}

impl Parser {
    pub fn new() -> Self {
        Self {
            consumed: 0,
            state: State::Length,
            varint: VarintParser::default(),
            length: 0,
            header: 0,
            method: 0,
            id: 0,
            body: Vec::with_capacity(MAX_MESSAGE_SIZE as usize),
            body_len: 0,
        }
    }
    pub fn recv(&mut self, buf: &[u8]) -> Result<(usize, Option<Message>)> {
        trace!("RECV {:?} {:?}", self.state, buf);
        match self.state {
            State::Length | State::Header | State::Method | State::Id => {
                match self.varint.recv(buf)? {
                    (consumed, None) => Ok((consumed, None)),
                    (consumed, Some(varint)) => {
                        match self.state {
                            State::Length => {
                                if varint > MAX_MESSAGE_SIZE {
                                    return Err(Error::new(
                                        ErrorKind::InvalidInput,
                                        "Message too long",
                                    ));
                                }
                                self.length = varint;
                            }
                            State::Header => {
                                self.consumed += consumed;
                                self.header = varint;
                            }
                            State::Method => {
                                self.consumed += consumed;
                                self.method = varint;
                            }
                            State::Id => {
                                self.consumed += consumed;
                                self.id = varint;
                                if self.length < self.body_len as u64 {
                                    return Err(Error::new(
                                        ErrorKind::InvalidInput,
                                        "Invalid message",
                                    ));
                                }
                                self.body_len = self.length as usize - self.consumed;
                            }
                            _ => unreachable!(),
                        };
                        self.advance_state();
                        if matches!(self.state, State::Message) && self.body_len == 0 {
                            let message =
                                Message::from_raw(self.header, self.method, self.id, vec![0u8; 0])?;
                            self.reset();
                            self.advance_state();
                            Ok((consumed, Some(message)))
                        } else {
                            Ok((consumed, None))
                        }
                    }
                }
            }
            State::Message => {
                trace!("read message, state {:?}", self);
                let remaining = self.length as usize - self.consumed;
                let consumed = std::cmp::min(remaining, buf.len());
                self.body.extend_from_slice(&buf[..consumed]);
                self.consumed += consumed;
                if consumed < remaining {
                    return Ok((consumed, None));
                }

                let message = Message::from_raw(
                    self.header,
                    self.method,
                    self.id,
                    self.body[..self.body_len].to_vec(),
                )?;

                self.reset();
                self.advance_state();
                Ok((consumed as usize, Some(message)))
            }
        }
    }

    fn reset(&mut self) {
        self.header = 0;
        self.method = 0;
        self.id = 0;
        self.length = 0;
        self.consumed = 0;
        self.body_len = 0;
        self.body.clear();
    }

    fn advance_state(&mut self) {
        self.state = match self.state {
            State::Length => State::Header,
            State::Header => State::Method,
            State::Method => State::Id,
            State::Id => State::Message,
            State::Message => State::Length,
        }
    }
}

#[derive(Debug)]
struct VarintParser {
    varint: u64,
    factor: u64,
}

impl Default for VarintParser {
    fn default() -> Self {
        Self {
            varint: 0,
            factor: 1,
        }
    }
}
impl VarintParser {
    fn reset(&mut self) {
        self.varint = 0;
        self.factor = 1;
    }
    fn recv(&mut self, buf: &[u8]) -> Result<(usize, Option<u64>)> {
        for (i, byte) in buf[..].iter().enumerate() {
            self.varint += (*byte as u64 & 127) * self.factor;
            if self.varint > u64::max_value() {
                return Err(Error::new(ErrorKind::InvalidInput, "Invalid varint"));
            }
            if byte < &128 {
                let result = (i + 1, Some(self.varint));
                self.reset();
                return Ok(result);
            }
            self.factor *= 128;
        }
        return Ok((buf.len(), None));
    }
}
