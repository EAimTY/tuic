use crate::client::{Address as RelayAddress, Request as RelayRequest};
use anyhow::{bail, Result};
use bytes::Bytes;
use std::{net::SocketAddr, sync::Arc};
use tokio::{
    io,
    net::{TcpListener, TcpStream, UdpSocket},
    sync::mpsc::Sender as MpscSender,
    task::JoinHandle,
};

pub use self::auth::Authentication;
use self::protocol::{
    handshake::password::{Request as PasswordAuthRequest, Response as PasswordAuthResponse},
    Address, HandshakeMethod, HandshakeRequest, HandshakeResponse, Reply, Request, Response,
    UdpHeader,
};

mod auth;
mod protocol;

pub async fn init(
    local_addr: SocketAddr,
    auth: Authentication,
    req_tx: MpscSender<RelayRequest>,
) -> Result<JoinHandle<()>> {
    let listener = TcpListener::bind(local_addr).await?;
    let auth = Arc::new(auth);

    Ok(tokio::spawn(listen_socks5_request(listener, auth, req_tx)))
}

async fn listen_socks5_request(
    listener: TcpListener,
    auth: Arc<Authentication>,
    req_tx: MpscSender<RelayRequest>,
) {
    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(Connection::new(stream, auth.clone(), req_tx.clone()));
    }
}

struct Connection {
    stream: TcpStream,
    auth: Arc<Authentication>,
    req_tx: MpscSender<RelayRequest>,
}

impl Connection {
    async fn new(stream: TcpStream, auth: Arc<Authentication>, req_tx: MpscSender<RelayRequest>) {
        let conn = Connection {
            stream,
            auth,
            req_tx,
        };

        match conn.process().await {
            Ok(()) => (),
            Err(err) => eprintln!("{err}"),
        }
    }

    async fn process(mut self) -> Result<()> {
        self.handshake().await?;

        let req = Request::read_from(&mut self.stream).await?;

        match req.command {
            protocol::Command::Connect => self.handle_connect(req.address).await?,
            protocol::Command::Bind => self.handle_bind(req.address).await?,
            protocol::Command::Associate => self.handle_associate(req.address).await?,
        }

        Ok(())
    }

    async fn handshake(&mut self) -> Result<()> {
        let method = self.auth.as_handshake_method();
        let req = HandshakeRequest::read_from(&mut self.stream).await?;

        if req.methods.contains(&method) {
            let resp = HandshakeResponse::new(method);
            resp.write_to(&mut self.stream).await?;

            match self.auth.as_ref() {
                Authentication::None => {}
                Authentication::Gssapi => unimplemented!(),
                Authentication::Password { username, password } => {
                    let req = PasswordAuthRequest::read_from(&mut self.stream).await?;

                    if (&req.username, &req.password) == (username, password) {
                        let resp = PasswordAuthResponse::new(true);
                        resp.write_to(&mut self.stream).await?;
                    } else {
                        let resp = PasswordAuthResponse::new(false);
                        resp.write_to(&mut self.stream).await?;
                        bail!("Password authentication failed");
                    }
                }
            }
        } else {
            let resp = HandshakeResponse::new(HandshakeMethod::Unacceptable);
            resp.write_to(&mut self.stream).await?;
            bail!("No acceptable authentication method");
        }

        Ok(())
    }

    async fn handle_connect(mut self, addr: Address) -> Result<()> {
        let addr = match addr {
            Address::SocketAddress(addr) => RelayAddress::SocketAddress(addr),
            Address::HostnameAddress(hostname, port) => {
                RelayAddress::HostnameAddress(hostname, port)
            }
        };

        let (relay_req, relay_resp_rx) = RelayRequest::new_connect(addr);
        let _ = self.req_tx.send(relay_req).await;

        let relay_resp = relay_resp_rx.await?;

        if let Some((mut remote_send, mut remote_recv)) = relay_resp {
            let resp = Response::new(
                Reply::Succeeded,
                Address::SocketAddress(SocketAddr::from(([0, 0, 0, 0], 0))),
            );

            resp.write_to(&mut self.stream).await?;

            let (mut local_recv, mut local_send) = self.stream.split();
            let remote_to_local = io::copy(&mut remote_recv, &mut local_send);
            let local_to_remote = io::copy(&mut local_recv, &mut remote_send);
            let _ = tokio::try_join!(remote_to_local, local_to_remote);
        } else {
            let resp = Response::new(
                Reply::GeneralFailure,
                Address::SocketAddress(SocketAddr::from(([0, 0, 0, 0], 0))),
            );

            resp.write_to(&mut self.stream).await?;
        }

        Ok(())
    }

    async fn handle_bind(self, _addr: Address) -> Result<()> {
        Ok(())
    }

    async fn handle_associate(mut self, addr: Address) -> Result<()> {
        let mut src_addr = match addr {
            Address::SocketAddress(addr) => {
                if addr.ip().is_unspecified() && addr.port() == 0 {
                    None
                } else {
                    Some(addr)
                }
            }
            Address::HostnameAddress(hostname, port) => {
                if hostname.is_empty() && port == 0 {
                    None
                } else {
                    bail!("Connot associate FQDN address")
                }
            }
        };

        let socket = UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0))).await?;
        let socket_addr = socket.local_addr()?;

        let resp = Response::new(Reply::Succeeded, Address::SocketAddress(socket_addr));
        resp.write_to(&mut self.stream).await?;

        let (relay_req, pkt_send_tx, pkt_receive_rx) = RelayRequest::new_associate();
        let _ = self.req_tx.send(relay_req).await;

        if src_addr.is_none() {
            let mut buf = vec![0; 1536];
            let (len, addr) = socket.recv_from(&mut buf).await?;
            buf.truncate(len);

            src_addr = Some(addr);

            match send_packet(buf, &pkt_send_tx).await {
                Ok(()) => (),
                Err(err) => eprintln!("{err}"),
            }
        }

        socket.connect(src_addr.unwrap()).await?;

        loop {
            let mut buf = vec![0; 1536];
            let len = socket.recv(&mut buf).await?;
            buf.truncate(len);

            match send_packet(buf, &pkt_send_tx).await {
                Ok(()) => (),
                Err(err) => eprintln!("{err}"),
            }
        }

        Ok(())
    }
}

async fn send_packet(buf: Vec<u8>, pkt_send_tx: &MpscSender<(Bytes, RelayAddress)>) -> Result<()> {
    let udp_header = UdpHeader::read_from(&mut buf.as_slice()).await?;

    if udp_header.frag != 0 {
        bail!("Fragmented UDP packet is not supported");
    }

    let bytes = Bytes::from(buf).slice(udp_header.serialized_len()..);

    let dst_addr = match udp_header.address {
        Address::SocketAddress(addr) => RelayAddress::SocketAddress(addr),
        Address::HostnameAddress(hostname, port) => RelayAddress::HostnameAddress(hostname, port),
    };

    let _ = pkt_send_tx.send((bytes, dst_addr)).await;

    Ok(())
}
