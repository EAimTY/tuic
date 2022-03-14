use super::Connection;
use anyhow::{bail, Result};
use futures_util::StreamExt;
use quinn::{
    Connection as QuinnConnection, ConnectionError, IncomingBiStreams, IncomingUniStreams,
    RecvStream, SendStream, VarInt,
};
use std::{
    net::ToSocketAddrs,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{io, net::TcpStream, time};
use tuic_protocol::{Address, Command, Response};

impl Connection {
    pub async fn listen_bi_streams(&self, mut bi_streams: IncomingBiStreams) {
        while let Some(stream) = bi_streams.next().await {
            match stream {
                Ok((send, recv)) => {
                    tokio::spawn(handle_bi_stream(
                        send,
                        recv,
                        self.controller.clone(),
                        self.is_authenticated.clone(),
                        self.create_time,
                    ));
                }
                Err(err) => {
                    match err {
                        ConnectionError::ConnectionClosed(_) | ConnectionError::TimedOut => {}
                        err => eprintln!("{err}"),
                    }
                    break;
                }
            }
        }
    }
}

pub async fn handle_bi_stream(
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
                Command::Authenticate { .. } => conn.close(VarInt::MAX, b"Bad command"),
                Command::Connect { addr } => match handle_connect(send, recv, addr).await {
                    Ok(()) => {}
                    Err(err) => eprintln!("{err}"),
                },
                Command::Bind { addr } => todo!(),
                Command::Packet { .. } => conn.close(VarInt::MAX, b"Bad command"),
                Command::Dissociate { .. } => conn.close(VarInt::MAX, b"Bad command"),
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
