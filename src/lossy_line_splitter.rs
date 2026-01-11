use crate::push_line_splitter::PushLineSplitter;
use futures::io::AsyncRead;
use futures::stream::Stream;
use futures::task::{Context, Poll};
use std::io;
use std::pin::Pin;

/// Wraps `PushLineSplitter` over an `AsyncRead`.
pub struct LossyLineSplitter<R> {
    reader: R,
    splitter: PushLineSplitter,
    buf: Box<[u8]>,
    eof: bool,
}

impl<R: AsyncRead> LossyLineSplitter<R> {
    /// Constructs a `LossyLineSplitter` over the supplied `AsyncRead`.
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            splitter: PushLineSplitter::default(),
            buf: (vec![0u8; 4096]).into_boxed_slice(),
            eof: false,
        }
    }
}

impl<R: AsyncRead + Unpin> Stream for LossyLineSplitter<R> {
    type Item = Result<String, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<String, std::io::Error>>> {
        loop {
            let self_mut = self.as_mut().get_mut();
            if let Some(line) = self_mut.splitter.next_line() {
                return Poll::Ready(Some(Ok(line.into_owned())));
            } else if self_mut.eof {
                return Poll::Ready(None);
            }

            match Pin::new(&mut self_mut.reader).poll_read(cx, &mut self_mut.buf) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => return Poll::Ready(Some(Err(e))),
                Poll::Ready(Ok(0)) => {
                    self_mut.eof = true;
                    self_mut.splitter.close();
                }
                Poll::Ready(Ok(n)) => {
                    self_mut.splitter.write(&self_mut.buf[..n]);
                }
            }
        }
    }
}
