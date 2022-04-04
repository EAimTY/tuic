use super::IsClosed;
use parking_lot::Mutex;
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
};

#[derive(Clone)]
pub struct IsAuthenticated {
    is_connection_closed: IsClosed,
    is_authenticated: Arc<AtomicBool>,
    broadcast: Arc<Mutex<Vec<Waker>>>,
}

impl IsAuthenticated {
    pub fn new(is_closed: IsClosed) -> Self {
        Self {
            is_connection_closed: is_closed,
            is_authenticated: Arc::new(AtomicBool::new(false)),
            broadcast: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn set_authenticated(&self) {
        self.is_authenticated.store(true, Ordering::Release);
    }

    pub fn wake(&self) {
        for waker in self.broadcast.lock().drain(..) {
            waker.wake();
        }
    }
}

impl Future for IsAuthenticated {
    type Output = bool;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.is_connection_closed.check() {
            Poll::Ready(false)
        } else if self.is_authenticated.load(Ordering::Relaxed) {
            Poll::Ready(true)
        } else {
            self.broadcast.lock().push(cx.waker().clone());
            Poll::Pending
        }
    }
}
