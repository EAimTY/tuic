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
                        process_conn(
                            conn,
                            bi_streams,
                            uni_streams,
                            datagrams,
                            self.expected_token_digest,
                        )
                        .await
                    }
                    Err(err) => eprintln!("{err}"),
                }
            });
        }
    }
}

async fn process_conn(
    conn: QuinnConnection,
    bi_streams: QuinnBiStreams,
    uni_streams: QuinnUniStreams,
    datagrams: QuinnDatagrams,
    expected_token_digest: [u8; 32],
) {
    let is_authenticated = Arc::new(AtomicBool::new(false));
    let create_time = Instant::now();

    let listen_uni_streams = listen_uni_streams(
        conn.clone(),
        uni_streams,
        expected_token_digest,
        is_authenticated.clone(),
        create_time,
    );

    let listen_bi_streams = listen_bi_streams(
        conn.clone(),
        bi_streams,
        is_authenticated.clone(),
        create_time,
    );

    tokio::join!(listen_uni_streams, listen_bi_streams);
}

async fn listen_uni_streams(
    conn: QuinnConnection,
    mut uni_streams: QuinnUniStreams,
    expected_token_digest: [u8; 32],
    is_authenticated: Arc<AtomicBool>,
    create_time: Instant,
) {
    while let Some(stream) = uni_streams.next().await {
        match stream {
            Ok(stream) => {
                tokio::spawn(handle_uni_stream(
                    stream,
                    conn.clone(),
                    expected_token_digest,
                    is_authenticated.clone(),
                    create_time,
                ));
            }
            Err(err) => {
                match err {
                    ConnectionError::ConnectionClosed(_) | ConnectionError::TimedOut => {}
                    err => eprintln!("{err}"),
                }
                continue;
            }
        }
    }
}

async fn listen_bi_streams(
    conn: QuinnConnection,
    mut bi_streams: QuinnBiStreams,
    is_authenticated: Arc<AtomicBool>,
    create_time: Instant,
) {
    while let Some(stream) = bi_streams.next().await {
        match stream {
            Ok((send, recv)) => {
                tokio::spawn(handle_bi_stream(
                    send,
                    recv,
                    conn.clone(),
                    is_authenticated.clone(),
                    create_time,
                ));
            }
            Err(err) => {
                match err {
                    ConnectionError::ConnectionClosed(_) | ConnectionError::TimedOut => {}
                    err => eprintln!("{err}"),
                }
                continue;
            }
        }
    }
}

async fn handle_uni_stream(
    mut stream: RecvStream,
    conn: QuinnConnection,
    expected_token_digest: [u8; 32],
    is_authenticated: Arc<AtomicBool>,
    create_time: Instant,
) {
    let cmd = match Command::read_from(&mut stream).await {
        Ok(cmd) => cmd,
        Err(err) => {
            eprintln!("{err}");
            conn.close(VarInt::MAX, b"Bad command");
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
                        Command::Connect { .. } => unreachable!(),
                        Command::Bind { .. } => unreachable!(),
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
}

async fn handle_bi_stream(
    send: SendStream,
    mut recv: RecvStream,
    conn: QuinnConnection,
    is_authenticated: Arc<AtomicBool>,
    create_time: Instant,
) {
    let cmd = match Command::read_from(&mut recv).await {
        Ok(cmd) => cmd,
        Err(err) => {
            eprintln!("{err}");
            conn.close(VarInt::MAX, b"Bad command");
            return;
        }
    };

    let mut interval = time::interval(Duration::from_millis(100));

    loop {
        if is_authenticated.load(Ordering::Acquire) {
            match cmd {
                Command::Authenticate { .. } => unreachable!(),
                Command::Connect { addr } => match handle_connect(send, recv, addr).await {
                    Ok(()) => {}
                    Err(err) => eprintln!("{err}"),
                },
                Command::Bind { addr } => todo!(),
                Command::Udp { .. } => unreachable!(),
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
