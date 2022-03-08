use anyhow::{bail, Result};
use futures_util::StreamExt;
use quinn::{
    Connection as QuinnConnection, ConnectionError, Datagrams as QuinnDatagrams, Endpoint,
    Incoming, IncomingBiStreams as QuinnBiStreams, IncomingUniStreams as QuinnUniStreams,
    NewConnection, RecvStream, SendStream, ServerConfig, VarInt,
};
use rustls::{Certificate, PrivateKey};
use std::{
    net::{SocketAddr, ToSocketAddrs},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{io, net::TcpStream, time};
use tuic_protocol::{Address, Command, Response};

pub struct Server {
    incoming: Incoming,
    expected_token_digest: [u8; 32],
}

impl Server {
    pub fn init(
        port: u16,
        expected_token_digest: [u8; 32],
        cert: Certificate,
        priv_key: PrivateKey,
    ) -> Result<Self> {
        let config = ServerConfig::with_single_cert(vec![cert], priv_key)?;
        let (_, incoming) = Endpoint::server(config, SocketAddr::from(([0, 0, 0, 0], port)))?;

        Ok(Self {
            incoming,
            expected_token_digest,
        })
    }

    pub async fn run(mut self) {
        while let Some(conn) = self.incoming.next().await {
            tokio::spawn(async move {
                match conn.await {
                    Ok(NewConnection {
                        connection: conn,
                        bi_streams,
                        uni_streams,
                        datagrams,
                        ..
                    }) => {
                        let conn = Connection::new(
                            conn,
                            bi_streams,
                            uni_streams,
                            datagrams,
                            self.expected_token_digest,
                        );
                        match conn.process().await {
                            Ok(()) => (),
                            Err(err) => eprintln!("{err}"),
                        }
                    }
                    Err(err) => eprintln!("{err}"),
                }
            });
        }
    }
}

struct Connection {
    conn: QuinnConnection,
    bi_streams: QuinnBiStreams,
    uni_streams: QuinnUniStreams,
    datagrams: QuinnDatagrams,
    expected_token_digest: [u8; 32],
    is_authenticated: Arc<AtomicBool>,
    create_time: Instant,
}

impl Connection {
    fn new(
        conn: QuinnConnection,
        bi_streams: QuinnBiStreams,
        uni_streams: QuinnUniStreams,
        datagrams: QuinnDatagrams,
        expected_token_digest: [u8; 32],
    ) -> Self {
        Self {
            conn,
            bi_streams,
            uni_streams,
            datagrams,
            expected_token_digest,
            is_authenticated: Arc::new(AtomicBool::new(false)),
            create_time: Instant::now(),
        }
    }

    async fn process(mut self) -> Result<()> {
        while let Some(stream) = self.bi_streams.next().await {
            match stream {
                Ok((send, recv)) => self.handle_bi_stream(send, recv),
                Err(err) => {
                    match err {
                        ConnectionError::ConnectionClosed(_) | ConnectionError::TimedOut => {}
                        err => eprintln!("{err}"),
                    }
                    continue;
                }
            }
        }

        Ok(())
    }

    fn handle_bi_stream(&self, send: SendStream, mut recv: RecvStream) {
        let conn = self.conn.clone();
        let expected_token_digest = self.expected_token_digest;
        let is_authenticated = self.is_authenticated.clone();
        let create_time = self.create_time;

        tokio::spawn(async move {
            let cmd = match Command::read_from(&mut recv).await {
                Ok(cmd) => cmd,
                Err(err) => {
                    eprintln!("{err}");
                    return;
                }
            };

            match cmd {
                Command::Authenticate { digest } => {
                    if digest == expected_token_digest {
                        is_authenticated.store(true, Ordering::Release);
                    } else {
                        eprintln!("Authentication failed");
                        conn.close(VarInt::MAX, b"Authentication failed");
                    }
                }
                cmd => {
                    let mut interval = time::interval(Duration::from_millis(100));

                    loop {
                        if is_authenticated.load(Ordering::Acquire) {
                            match cmd {
                                Command::Authenticate { .. } => unreachable!(),
                                Command::Connect { addr } => {
                                    match handle_connect(send, recv, addr).await {
                                        Ok(()) => {}
                                        Err(err) => eprintln!("{err}"),
                                    }
                                }
                                Command::Bind { addr } => todo!(),
                                Command::Udp { assoc_id, addr } => todo!(),
                            }
                            break;
                        } else if create_time.elapsed() > Duration::from_secs(3) {
                            eprintln!("Authentication timeout");
                            conn.close(VarInt::MAX, b"Authentication timeout");
                            break;
                        } else {
                            interval.tick().await;
                        }
                    }
                }
            }
        });
    }
}

async fn handle_connect(mut send: SendStream, mut recv: RecvStream, addr: Address) -> Result<()> {
    async fn connect_remote(addr: Address) -> Result<TcpStream> {
        let addrs = addr.to_socket_addrs()?;

        for addr in addrs {
            if let Ok(stream) = TcpStream::connect(addr).await {
                return Ok(stream);
            }
        }

        bail!("Failed to connect to remote");
    }

    let mut stream = match connect_remote(addr).await {
        Ok(stream) => stream,
        Err(err) => {
            let resp = Response::new(false);
            resp.write_to(&mut send).await?;
            return Err(err);
        }
    };

    let resp = Response::new(true);
    resp.write_to(&mut send).await?;

    let (mut target_recv, mut target_send) = stream.split();
    let target_to_tunnel = io::copy(&mut target_recv, &mut send);
    let tunnel_to_target = io::copy(&mut recv, &mut target_send);
    let _ = tokio::try_join!(target_to_tunnel, tunnel_to_target);

    Ok(())
}
