use quinn::{RecvStream, SendStream};
use std::{
    io::{Error as IoError, IoSlice},
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub struct BiStream(SendStream, RecvStream);

impl BiStream {
    pub fn new(send: SendStream, recv: RecvStream) -> Self {
        Self(send, recv)
    }
}

impl AsyncRead for BiStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<(), IoError>> {
        Pin::new(&mut self.1).poll_read(cx, buf)
    }
}

impl AsyncWrite for BiStream {
    #[inline]
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, IoError>> {
        Pin::new(&mut self.0).poll_write(cx, buf)
    }

    #[inline]
    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<Result<usize, IoError>> {
        Pin::new(&mut self.0).poll_write_vectored(cx, bufs)
    }

    #[inline]
    fn is_write_vectored(&self) -> bool {
        self.0.is_write_vectored()
    }

    #[inline]
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        Pin::new(&mut self.0).poll_flush(cx)
    }

    #[inline]
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        Pin::new(&mut self.0).poll_shutdown(cx)
    }
}
