use crate::{config::Config, utils::UdpRelayMode, Error};
use bytes::Bytes;
use crossbeam_utils::atomic::AtomicCell;
use parking_lot::Mutex;
use quinn::{Connecting, Connection as QuinnConnection, Endpoint, RecvStream, SendStream, VarInt};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
};
use tuic_quinn::{side, Connection as Model, Task};

pub struct Server {
    ep: Endpoint,
    token: Arc<[u8]>,
    zero_rtt_handshake: bool,
}

impl Server {
    pub fn init(cfg: Config) -> Result<Self, Error> {
        todo!()
    }

    pub async fn start(&self) {
        loop {
            let conn = self.ep.accept().await.unwrap();
            tokio::spawn(Connection::init(
                conn,
                self.token.clone(),
                self.zero_rtt_handshake,
            ));
        }
    }
}

#[derive(Clone)]
struct Connection {
    inner: QuinnConnection,
    model: Model<side::Server>,
    token: Arc<[u8]>,
    is_authed: IsAuthed,
    udp_relay_mode: Arc<AtomicCell<Option<UdpRelayMode>>>,
}

impl Connection {
    pub async fn init(conn: Connecting, token: Arc<[u8]>, zero_rtt_handshake: bool) {
        match Self::handshake(conn, token, zero_rtt_handshake).await {
            Ok(conn) => loop {
                if conn.is_closed() {
                    break;
                }

                match conn.accept().await {
                    Ok(()) => {}
                    Err(err) => eprintln!("{err}"),
                }
            },
            Err(err) => eprintln!("{err}"),
        }
    }

    async fn handshake(
        conn: Connecting,
        token: Arc<[u8]>,
        zero_rtt_handshake: bool,
    ) -> Result<Self, Error> {
        let conn = if zero_rtt_handshake {
            match conn.into_0rtt() {
                Ok((conn, _)) => conn,
                Err(conn) => {
                    eprintln!("0-RTT handshake failed, fallback to 1-RTT handshake");
                    conn.await?
                }
            }
        } else {
            conn.await?
        };

        Ok(Self {
            inner: conn.clone(),
            model: Model::<side::Server>::new(conn),
            token,
            is_authed: IsAuthed::new(),
            udp_relay_mode: Arc::new(AtomicCell::new(None)),
        })
    }

    async fn accept(&self) -> Result<(), Error> {
        tokio::select! {
            res = self.inner.accept_uni() => tokio::spawn(self.clone().handle_uni_stream(res?)),
            res = self.inner.accept_bi() => tokio::spawn(self.clone().handle_bi_stream(res?)),
            res = self.inner.read_datagram() => tokio::spawn(self.clone().handle_datagram(res?)),
        };

        Ok(())
    }

    async fn handle_uni_stream(self, recv: RecvStream) {
        async fn pre_process(conn: &Connection, recv: RecvStream) -> Result<Task, Error> {
            let task = conn.model.accept_uni_stream(recv).await?;

            if let Task::Authenticate(token) = &task {
                if conn.is_authed() {
                    return Err(Error::DuplicatedAuth);
                } else {
                    let mut buf = [0; 32];
                    conn.inner
                        .export_keying_material(&mut buf, &conn.token, &conn.token)
                        .map_err(|_| Error::ExportKeyingMaterial)?;

                    if token == &buf {
                        conn.set_authed();
                    } else {
                        return Err(Error::AuthFailed);
                    }
                }
            }

            tokio::select! {
                () = conn.authed() => {}
                err = conn.inner.closed() => Err(err)?,
            };

            let same_pkt_src = matches!(task, Task::Packet(_))
                && matches!(conn.get_udp_relay_mode(), Some(UdpRelayMode::Native));
            if same_pkt_src {
                return Err(Error::UnexpectedPacketSource);
            }

            Ok(task)
        }

        match pre_process(&self, recv).await {
            Ok(Task::Packet(pkt)) => todo!(),
            Ok(Task::Dissociate(assoc_id)) => todo!(),
            Ok(_) => unreachable!(),
            Err(err) => {
                eprintln!("{err}");
                self.inner.close(VarInt::from_u32(0), b"");
                return;
            }
        }
    }

    async fn handle_bi_stream(self, (send, recv): (SendStream, RecvStream)) {
        async fn pre_process(
            conn: &Connection,
            send: SendStream,
            recv: RecvStream,
        ) -> Result<Task, Error> {
            let task = conn.model.accept_bi_stream(send, recv).await?;

            tokio::select! {
                () = conn.authed() => {}
                err = conn.inner.closed() => Err(err)?,
            };

            Ok(task)
        }

        match pre_process(&self, send, recv).await {
            Ok(Task::Connect(conn)) => todo!(),
            Ok(_) => unreachable!(),
            Err(err) => {
                eprintln!("{err}");
                self.inner.close(VarInt::from_u32(0), b"");
                return;
            }
        }
    }

    async fn handle_datagram(self, dg: Bytes) {
        async fn pre_process(conn: &Connection, dg: Bytes) -> Result<Task, Error> {
            let task = conn.model.accept_datagram(dg)?;

            tokio::select! {
                () = conn.authed() => {}
                err = conn.inner.closed() => Err(err)?,
            };

            let same_pkt_src = matches!(task, Task::Packet(_))
                && matches!(conn.get_udp_relay_mode(), Some(UdpRelayMode::Quic));
            if same_pkt_src {
                return Err(Error::UnexpectedPacketSource);
            }

            Ok(task)
        }

        match pre_process(&self, dg).await {
            Ok(Task::Packet(pkt)) => todo!(),
            Ok(Task::Heartbeat) => todo!(),
            Ok(_) => unreachable!(),
            Err(err) => {
                eprintln!("{err}");
                self.inner.close(VarInt::from_u32(0), b"");
                return;
            }
        }
    }

    fn set_authed(&self) {
        self.is_authed.set_authed();
    }

    fn is_authed(&self) -> bool {
        self.is_authed.is_authed()
    }

    fn authed(&self) -> IsAuthed {
        self.is_authed.clone()
    }

    fn set_udp_relay_mode(&self, mode: UdpRelayMode) {
        self.udp_relay_mode.store(Some(mode));
    }

    fn get_udp_relay_mode(&self) -> Option<UdpRelayMode> {
        self.udp_relay_mode.load()
    }

    fn is_closed(&self) -> bool {
        self.inner.close_reason().is_some()
    }
}

#[derive(Clone)]
struct IsAuthed {
    is_authed: Arc<AtomicBool>,
    broadcast: Arc<Mutex<Vec<Waker>>>,
}

impl IsAuthed {
    fn new() -> Self {
        Self {
            is_authed: Arc::new(AtomicBool::new(false)),
            broadcast: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn set_authed(&self) {
        self.is_authed.store(true, Ordering::Release);

        for waker in self.broadcast.lock().drain(..) {
            waker.wake();
        }
    }

    fn is_authed(&self) -> bool {
        self.is_authed.load(Ordering::Relaxed)
    }
}

impl Future for IsAuthed {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.is_authed.load(Ordering::Relaxed) {
            Poll::Ready(())
        } else {
            self.broadcast.lock().push(cx.waker().clone());
            Poll::Pending
        }
    }
}
