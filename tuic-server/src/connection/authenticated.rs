use crossbeam_utils::atomic::AtomicCell;
use parking_lot::Mutex;
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, Waker},
};
use uuid::Uuid;

#[derive(Clone)]
pub(super) struct Authenticated(Arc<AuthenticatedInner>);

struct AuthenticatedInner {
    uuid: AtomicCell<Option<Uuid>>,
    broadcast: Mutex<Vec<Waker>>,
}

impl Authenticated {
    pub(super) fn new() -> Self {
        Self(Arc::new(AuthenticatedInner {
            uuid: AtomicCell::new(None),
            broadcast: Mutex::new(Vec::new()),
        }))
    }

    pub(super) fn set(&self, uuid: Uuid) {
        self.0.uuid.store(Some(uuid));

        for waker in self.0.broadcast.lock().drain(..) {
            waker.wake();
        }
    }

    pub(super) fn get(&self) -> Option<Uuid> {
        self.0.uuid.load()
    }
}

impl Future for Authenticated {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.get().is_some() {
            Poll::Ready(())
        } else {
            self.0.broadcast.lock().push(cx.waker().clone());
            Poll::Pending
        }
    }
}
