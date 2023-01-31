use crate::{error::Error, socks5};
use bytes::Bytes;
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use quinn::{
    Connection as QuinnConnection, Endpoint as QuinnEndpoint, RecvStream, SendStream, VarInt,
};
use socks5_proto::Address as Socks5Address;
use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Weak,
    },
    time::Duration,
};
use tokio::{
    sync::{Mutex as AsyncMutex, OnceCell as AsyncOnceCell},
    time,
};
use tuic::Address;
use tuic_quinn::{side, Connect, Connection as Model, Task};

static ENDPOINT: OnceCell<Mutex<Endpoint>> = OnceCell::new();
static CONNECTION: AsyncOnceCell<AsyncMutex<Connection>> = AsyncOnceCell::const_new();

const DEFAULT_CONCURRENT_STREAMS: usize = 32;

struct Endpoint {
    ep: QuinnEndpoint,
}

impl Endpoint {
    fn new() -> Result<Self, Error> {
        let ep = QuinnEndpoint::client(SocketAddr::from(([0, 0, 0, 0], 0)))?;
        Ok(Self { ep })
    }

    async fn connect(&self) -> Result<Connection, Error> {
        let conn = self
            .ep
            .connect(SocketAddr::from(([127, 0, 0, 1], 8080)), "localhost")?
            .await
            .map(Connection::new)?;

        tokio::spawn(conn.clone().accept());

        Ok(conn)
    }
}

#[derive(Clone)]
pub struct Connection {
    conn: QuinnConnection,
    model: Model<side::Client>,
    remote_uni_stream_cnt: StreamCount,
    remote_bi_stream_cnt: StreamCount,
    max_concurrent_uni_streams: Arc<AtomicUsize>,
    max_concurrent_bi_streams: Arc<AtomicUsize>,
}

impl Connection {
    fn new(conn: QuinnConnection) -> Self {
        Self {
            conn: conn.clone(),
            model: Model::<side::Client>::new(conn),
            remote_uni_stream_cnt: StreamCount::new(),
            remote_bi_stream_cnt: StreamCount::new(),
            max_concurrent_uni_streams: Arc::new(AtomicUsize::new(DEFAULT_CONCURRENT_STREAMS)),
            max_concurrent_bi_streams: Arc::new(AtomicUsize::new(DEFAULT_CONCURRENT_STREAMS)),
        }
    }

    pub async fn get() -> Result<Connection, Error> {
        let try_init_conn = async {
            ENDPOINT
                .get_or_try_init(|| Endpoint::new().map(Mutex::new))
                .map(|ep| ep.lock())?
                .connect()
                .await
                .map(AsyncMutex::new)
        };

        let try_get_conn = async {
            let mut conn = CONNECTION
                .get_or_try_init(|| try_init_conn)
                .await?
                .lock()
                .await;

            if conn.is_closed() {
                let new_conn = ENDPOINT.get().unwrap().lock().connect().await?;
                *conn = new_conn;
            }

            Ok::<_, Error>(conn.clone())
        };

        let conn = time::timeout(Duration::from_secs(5), try_get_conn)
            .await
            .map_err(|_| Error::Timeout)??;

        Ok(conn)
    }

    pub async fn connect(&self, addr: Address) -> Result<Connect, Error> {
        Ok(self.model.connect(addr).await?)
    }

    pub async fn packet(&self, pkt: Bytes, addr: Address, assoc_id: u16) -> Result<(), Error> {
        self.model.packet_quic(pkt, addr, assoc_id).await?; // TODO
        Ok(())
    }

    pub async fn dissociate(&self, assoc_id: u16) -> Result<(), Error> {
        self.model.dissociate(assoc_id).await?;
        Ok(())
    }

    fn is_closed(&self) -> bool {
        self.conn.close_reason().is_some()
    }

    async fn accept_uni_stream(&self) -> Result<(RecvStream, StreamRegister), Error> {
        let max = self.max_concurrent_uni_streams.load(Ordering::Relaxed);

        if self.remote_uni_stream_cnt.get() == max {
            self.max_concurrent_uni_streams
                .store(max * 2, Ordering::Relaxed);

            self.conn
                .set_max_concurrent_uni_streams(VarInt::from((max * 2) as u32));
        }

        let recv = self.conn.accept_uni().await?;
        let reg = self.remote_uni_stream_cnt.register();
        Ok((recv, reg))
    }

    async fn accept_bi_stream(&self) -> Result<(SendStream, RecvStream, StreamRegister), Error> {
        let max = self.max_concurrent_bi_streams.load(Ordering::Relaxed);

        if self.remote_bi_stream_cnt.get() == max {
            self.max_concurrent_bi_streams
                .store(max * 2, Ordering::Relaxed);

            self.conn
                .set_max_concurrent_bi_streams(VarInt::from((max * 2) as u32));
        }

        let (send, recv) = self.conn.accept_bi().await?;
        let reg = self.remote_bi_stream_cnt.register();
        Ok((send, recv, reg))
    }

    async fn accept_datagram(&self) -> Result<Bytes, Error> {
        Ok(self.conn.read_datagram().await?)
    }

    async fn handle_uni_stream(self, recv: RecvStream, _reg: StreamRegister) {
        let res = match self.model.accept_uni_stream(recv).await {
            Err(err) => Err(Error::from(err)),
            Ok(Task::Packet(pkt)) => match pkt.accept().await {
                Ok(Some((pkt, addr, assoc_id))) => {
                    let addr = match addr {
                        Address::None => unreachable!(),
                        Address::DomainAddress(domain, port) => {
                            Socks5Address::DomainAddress(domain, port)
                        }
                        Address::SocketAddress(addr) => Socks5Address::SocketAddress(addr),
                    };
                    socks5::recv_pkt(pkt, addr, assoc_id).await
                }
                Ok(None) => Ok(()),
                Err(err) => Err(Error::from(err)),
            },
            _ => unreachable!(),
        };

        match res {
            Ok(()) => {}
            Err(err) => eprintln!("{err}"),
        }
    }

    async fn handle_bi_stream(self, send: SendStream, recv: RecvStream, _reg: StreamRegister) {
        let res = match self.model.accept_bi_stream(send, recv).await {
            Err(err) => Err(Error::from(err)),
            _ => unreachable!(),
        };

        match res {
            Ok(()) => {}
            Err(err) => eprintln!("{err}"),
        }
    }

    async fn handle_datagram(self, dg: Bytes) {
        let res = match self.model.accept_datagram(dg) {
            Err(err) => Err(Error::from(err)),
            Ok(Task::Packet(pkt)) => match pkt.accept().await {
                Ok(Some((pkt, addr, assoc_id))) => {
                    let addr = match addr {
                        Address::None => unreachable!(),
                        Address::DomainAddress(domain, port) => {
                            Socks5Address::DomainAddress(domain, port)
                        }
                        Address::SocketAddress(addr) => Socks5Address::SocketAddress(addr),
                    };
                    socks5::recv_pkt(pkt, addr, assoc_id).await
                }
                Ok(None) => Ok(()),
                Err(err) => Err(Error::from(err)),
            },
            _ => unreachable!(),
        };

        match res {
            Ok(()) => {}
            Err(err) => eprintln!("{err}"),
        }
    }

    async fn accept(self) {
        let err = loop {
            tokio::select! {
                res = self.accept_uni_stream() => match res {
                    Ok((recv, reg)) => tokio::spawn(self.clone().handle_uni_stream(recv, reg)),
                    Err(err) => break err,
                },
                res = self.accept_bi_stream() => match res {
                    Ok((send, recv, reg)) => tokio::spawn(self.clone().handle_bi_stream(send, recv, reg)),
                    Err(err) => break err,
                },
                res = self.accept_datagram() => match res {
                    Ok(dg) => tokio::spawn(self.clone().handle_datagram(dg)),
                    Err(err) => break err,
                },
            };
        };

        eprintln!("{err}");
    }
}

#[derive(Clone)]
struct StreamCount(Arc<()>);
struct StreamRegister(Weak<()>);

impl StreamCount {
    fn new() -> Self {
        Self(Arc::new(()))
    }

    fn register(&self) -> StreamRegister {
        StreamRegister(Arc::downgrade(&self.0))
    }

    fn get(&self) -> usize {
        Arc::weak_count(&self.0)
    }
}
