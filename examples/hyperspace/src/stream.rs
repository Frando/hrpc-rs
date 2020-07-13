use futures::stream::Stream;
use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::RemoteHypercore;

type GetOutput = Result<Option<Vec<u8>>>;
type GetFuture = Pin<Box<dyn Future<Output = (RemoteHypercore, GetOutput)> + Send>>;

async fn get(mut core: RemoteHypercore, seq: u64) -> (RemoteHypercore, GetOutput) {
    let block = core.get(seq).await;
    (core, block)
}

fn get_future(core: RemoteHypercore, seq: u64) -> GetFuture {
    Box::pin(get(core, seq))
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
        Self {
            future: get_future(core, start),
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
            Ok(Some(block)) => (false, Some(Ok(block))),
            // TODO: If in live mode, don't set finish but instead register a waker
            // on the append event.
            Ok(None) => (true, None),
            Err(err) => (true, Some(Err(err))),
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
            self.future = get_future(core, self.index)
        }
        Poll::Ready(result)
    }
}
