use futures::future::FutureExt;
use futures::future::Map;
use futures::io::AsyncRead;
use futures::stream::{FusedStream, Stream};
use log::*;
use std::collections::VecDeque;
use std::future::Future;
use std::io::{Error, ErrorKind, Result};
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::RemoteHypercore;

type GetOutput = Result<Option<Vec<u8>>>;

async fn get(mut core: RemoteHypercore, seq: u64) -> (RemoteHypercore, GetOutput) {
    let block = core.get(seq).await;
    (core, block)
}

pub struct ReadStream {
    future: Pin<Box<dyn Future<Output = (RemoteHypercore, GetOutput)> + Send>>,
    index: u64,
    end: Option<u64>,
    live: bool,
    finished: bool,
}

impl ReadStream {
    pub fn new(core: RemoteHypercore, start: u64, end: Option<u64>, live: bool) -> Self {
        let future = get(core, start);
        let future = Box::pin(future);
        Self {
            future,
            index: start,
            end,
            live,
            finished: false,
        }
    }
}

impl Stream for ReadStream {
    type Item = Result<Vec<u8>>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.finished {
            return Poll::Ready(None);
        }

        let poll_result = Pin::as_mut(&mut self.future).poll(cx);
        let result = futures::ready!(poll_result);
        let (core, result) = result;
        let (mut finished, result) = match result {
            Err(err) => (true, Some(Err(err))),
            Ok(None) => (true, None),
            Ok(Some(block)) => (false, Some(Ok(block))),
        };
        self.index += 1;
        if let Some(end) = self.end {
            if end == self.index {
                finished = true;
            }
        }
        if finished {
            self.finished = true;
        } else {
            self.future = Box::pin(get(core, self.index));
        }
        Poll::Ready(result)
    }
}

// fn get(core: RemoteHypercore, seq: u64) -> impl Future<Output = MappedOutput>
// // where
// //     Fut: Future<Output = GetOutput>,
// {
//     let core_clone = core.clone();
//     let future = core.get(seq);
//     let future = Box::pin(future);
//     let mapped = future.map(move |result| (core_clone, result));
//     Box::pin(mapped)
// }

// // pub struct ReadStream<Fut>
// // where
// //     Fut: Future<Output = GetOutput>,
// pub struct ReadStream<Fut> {
//     future: Fut,
//     // core: Option<&'a RemoteHypercore>,
//     // future: Option<GetFuture>,
//     start: u64,
//     end: Option<u64>,
//     live: bool,
//     index: u64,
// }

// // impl<Fut> ReadStream<Fut>
// // where
// // Fut: Future<Output = GetOutput>,
// // impl<Fut> ReadStream<Fut> {
// //     pub fn new(core: RemoteHypercore, start: u64, end: Option<u64>, live: bool) -> Self {
// //         let future = get(core, start);

// //         Self {
// //             future,
// //             start,
// //             end,
// //             live,
// //             index: start,
// //         }
// //     }
// // }
// // pub fn create_read_stream(
// //     core: RemoteHypercore,
// //     start: u64,
// //     end: Option<u64>,
// //     live: bool,
// // ) -> impl Stream<Item = Result<Vec<u8>>> {
// //     let stream: ReadStream<dyn Future<Output = MappedOutput>> =
// //         create_read_stream_inner(core, start, end, live);
// //     stream
// // }

// // fn create_read_stream_inner<Fut>(
// //     core: RemoteHypercore,
// //     start: u64,
// //     end: Option<u64>,
// //     live: bool,
// // ) -> ReadStream<Fut>
// // where
// //     Fut: Future<Output = MappedOutput>,
// // {
// //     let future = get(core, start);

// //     ReadStream {
// //         future,
// //         start,
// //         end,
// //         live,
// //         index: start,
// //     }
// // }

// impl<Fut> Stream for ReadStream<Fut> {
//     type Item = Result<Vec<u8>>;
//     fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
//         return Poll::Pending;
//         // let future = &mut self.future;
//         // let mut core = self.core.clone();
//         // // let core = &mut self.core;
//         // if future.is_none() {
//         //     let future = core.get(self.index);
//         //     self.future = Some(Box::pin(future));
//         // }
//         //
//         // let future = self.future.take().unwrap();
//         // let core = self.core.take().unwrap();
//         // let result = if let Some(future) = self.future {
//         //     let result = Pin::as_mut(&mut future).poll(cx);
//         //     let result = match result {
//         //         Poll::Pending => return Poll::Pending,
//         //         Poll::Ready(result) => match result {
//         //             Err(err) => Some(Err(err)),
//         //             Ok(None) => None,
//         //             Ok(Some(block)) => Some(Ok(block)),
//         //         },
//         //     };
//         //     result
//         // } else {
//         //     None
//         // };

//         // Re-init the future for the next poll.
//         // {
//         //     let future = core.get(self.index);
//         //     self.future = Some(Box::pin(future));
//         // }

//         // return Poll::Ready(result);
//     }
// }
