use futures_util::StreamExt;
use quinn::{
    Connecting, ConnectionError as QuinnConnectionError, IncomingBiStreams, NewConnection,
    RecvStream, SendStream,
};
use std::{
    io::{Error as IoError, ErrorKind},
    net::{SocketAddr, ToSocketAddrs},
};
use thiserror::Error;
use tokio::{io, net::TcpStream};
use tuic_protocol::{Address, Error as TuicError, Reply, Request, Response};

pub struct Connection {
    bi_streams: IncomingBiStreams,
    remote_addr: SocketAddr,
}

impl Connection {
    pub async fn new(conn: Connecting) -> Result<Self, ConnectionError> {
        let NewConnection {
            bi_streams,
            connection,
            ..
        } = conn.await?;
        let remote_addr = connection.remote_address();

        Ok(Self {
            bi_streams,
            remote_addr,
        })
    }

    pub async fn process(mut self, token: u64) {
        while let Some(stream) = self.bi_streams.next().await {
            match stream {
                Ok((send, recv)) => {
                    tokio::spawn(async move {
                        let stream = Stream::new(send, recv, self.remote_addr);
                        match stream.handle(token).await {
                            Ok(()) => {}
                            Err(err) => log::debug!("{err}"),
                        }
                    });
                }
                Err(QuinnConnectionError::ApplicationClosed { .. }) => break,
                Err(err) => {
                    log::debug!("{err}");
                    break;
                }
            }
        }
    }
}

struct Stream {
    send: SendStream,
    recv: RecvStream,
    remote_addr: SocketAddr,
}

impl Stream {
    fn new(send: SendStream, recv: RecvStream, remote_addr: SocketAddr) -> Self {
        Self {
            send,
            recv,
            remote_addr,
        }
    }

    async fn handle(mut self, token: u64) -> Result<(), ConnectionError> {
        let req = Request::read_from(&mut self.recv).await?;

        if req.token != token {
            log::info!("[denied] {} {:?}", self.remote_addr, &req);

            let res = Response::new(Reply::AuthenticationFailed);
            res.write_to(&mut self.send).await?;

            return Ok(());
        }

        log::info!("[accepted] {} {:?}", self.remote_addr, &req);

        async fn connect_remote(addr: &Address) -> Result<TcpStream, Option<IoError>> {
            let target_addrs = addr.to_socket_addrs()?;

            let mut last_err = None;

            for target_addr in target_addrs {
                match TcpStream::connect(target_addr).await {
                    Ok(stream) => return Ok(stream),
                    Err(err) => last_err = Some(err),
                }
            }

            Err(last_err)
        }

        match connect_remote(&req.address).await {
            Ok(target_stream) => {
                let res = Response::new(Reply::Succeeded);
                res.write_to(&mut self.send).await?;
                self.forward(target_stream).await;
            }
            Err(err) => {
                let reply = err.map_or(Reply::HostUnreachable, |err| match err.kind() {
                    ErrorKind::ConnectionRefused => Reply::ConnectionRefused,
                    _ => Reply::GeneralFailure,
                });
                let res = Response::new(reply);
                res.write_to(&mut self.send).await?;
            }
        }

        Ok(())
    }

    async fn forward(&mut self, mut target_stream: TcpStream) {
        let (mut target_recv, mut target_send) = target_stream.split();
        let target_to_tunnel = io::copy(&mut target_recv, &mut self.send);
        let tunnel_to_target = io::copy(&mut self.recv, &mut target_send);
        let _ = tokio::try_join!(target_to_tunnel, tunnel_to_target);
    }
}

#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error(transparent)]
    Quinn(#[from] QuinnConnectionError),
    #[error(transparent)]
    Tuic(#[from] TuicError),
    #[error(transparent)]
    Io(#[from] IoError),
}
