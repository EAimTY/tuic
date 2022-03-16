use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct IsAuthenticated {
    is_authenticated: Arc<AtomicBool>,
    create_time: Instant,
    timeout: Duration,
}

impl IsAuthenticated {
    pub fn new(timeout: Duration) -> Self {
        Self {
            is_authenticated: Arc::new(AtomicBool::new(false)),
            create_time: Instant::now(),
            timeout,
        }
    }

    pub fn set_authenticated(&self) {
        self.is_authenticated.store(true, Ordering::Release);
    }
}

impl Future for IsAuthenticated {
    type Output = bool;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.is_authenticated.load(Ordering::Relaxed) {
            Poll::Ready(true)
        } else if self.create_time.elapsed() > self.timeout {
            Poll::Ready(false)
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}
