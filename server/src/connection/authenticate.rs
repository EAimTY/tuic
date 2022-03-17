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
    is_authenticated: Arc<AtomicBool>,
    authenticate_broadcast: Arc<AuthenticateBroadcast>,
    is_connection_closed: Arc<AtomicBool>,
}

impl IsAuthenticated {
    pub fn new(is_closed: Arc<AtomicBool>) -> (Self, Arc<AuthenticateBroadcast>) {
        let auth_bcast = Arc::new(AuthenticateBroadcast::new());

        (
            Self {
                is_authenticated: Arc::new(AtomicBool::new(false)),
                authenticate_broadcast: auth_bcast.clone(),
                is_connection_closed: is_closed,
            },
            auth_bcast,
        )
    }

    pub fn set_authenticated(&self) {
        self.is_authenticated.store(true, Ordering::Release);
    }

    pub fn check(&self) -> bool {
        self.is_authenticated.load(Ordering::Acquire)
    }
}

impl Future for IsAuthenticated {
    type Output = bool;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.is_connection_closed.load(Ordering::Acquire) {
            Poll::Ready(false)
        } else if self.is_authenticated.load(Ordering::Relaxed) {
            Poll::Ready(true)
        } else {
            self.authenticate_broadcast.register(cx.waker().clone());
            Poll::Pending
        }
    }
}

pub struct AuthenticateBroadcast(Mutex<Vec<Waker>>);

impl AuthenticateBroadcast {
    pub fn register(&self, waker: Waker) {
        let mut bcast = self.0.lock();
        bcast.push(waker);
    }

    pub fn wake(&self) {
        let mut bcast = self.0.lock();

        for waker in bcast.drain(..) {
            waker.wake();
        }
    }

    fn new() -> Self {
        Self(Mutex::new(Vec::new()))
    }
}
