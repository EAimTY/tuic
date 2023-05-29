use crate::{
    config::Local,
    connection::{Connection as TuicConnection, CONNECTION_CLOSE_ERROR_CODE},
    Error,
};
use bytes::Bytes;
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use socks5_proto::{Address, Reply};
use socks5_server::{
    auth::{NoAuth, Password},
    connection::{associate, bind, connect},
    Associate, AssociatedUdpSocket, Auth, Bind, Connect, Connection, Server as Socks5Server,
};
use std::{
    collections::HashMap,
    io::{Error as IoError, ErrorKind},
    net::{IpAddr, SocketAddr, TcpListener as StdTcpListener, UdpSocket as StdUdpSocket},
    sync::{
        atomic::{AtomicU16, Ordering},
        Arc,
    },
};
use tokio::{
    io::{self, AsyncWriteExt},
    net::{TcpListener, UdpSocket},
};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tuic::Address as TuicAddress;

static SERVER: OnceCell<Server> = OnceCell::new();
pub static UDP_SESSIONS: OnceCell<Mutex<HashMap<u16, UdpSession>>> = OnceCell::new();

pub struct Server {
    inner: Socks5Server,
    dual_stack: Option<bool>,
    max_pkt_size: usize,
    next_assoc_id: AtomicU16,
}

impl Server {
    pub fn set_config(cfg: Local) -> Result<(), Error> {
        SERVER
            .set(Self::new(
                cfg.server,
                cfg.dual_stack,
                cfg.max_packet_size,
                cfg.username.map(|s| s.into_bytes()),
                cfg.password.map(|s| s.into_bytes()),
            )?)
            .map_err(|_| "failed initializing socks5 server")
            .unwrap();

        UDP_SESSIONS
            .set(Mutex::new(HashMap::new()))
            .map_err(|_| "failed initializing socks5 UDP session pool")
            .unwrap();

        Ok(())
    }

    fn new(
        addr: SocketAddr,
        dual_stack: Option<bool>,
        max_pkt_size: usize,
        username: Option<Vec<u8>>,
        password: Option<Vec<u8>>,
    ) -> Result<Self, Error> {
        let socket = {
            let domain = match addr {
                SocketAddr::V4(_) => Domain::IPV4,
                SocketAddr::V6(_) => Domain::IPV6,
            };

            let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))
                .map_err(|err| Error::Socket("failed to create socks5 server socket", err))?;

            if let Some(dual_stack) = dual_stack {
                socket.set_only_v6(!dual_stack).map_err(|err| {
                    Error::Socket("socks5 server dual-stack socket setting error", err)
                })?;
            }

            socket.set_reuse_address(true).map_err(|err| {
                Error::Socket("failed to set socks5 server socket to reuse_address", err)
            })?;

            socket.set_nonblocking(true).map_err(|err| {
                Error::Socket("failed setting socks5 server socket as non-blocking", err)
            })?;

            socket
                .bind(&SockAddr::from(addr))
                .map_err(|err| Error::Socket("failed to bind socks5 server socket", err))?;

            socket
                .listen(i32::MAX)
                .map_err(|err| Error::Socket("failed to listen on socks5 server socket", err))?;

            TcpListener::from_std(StdTcpListener::from(socket))
                .map_err(|err| Error::Socket("failed to create socks5 server socket", err))?
        };

        let auth: Arc<dyn Auth + Send + Sync> = match (username, password) {
            (Some(username), Some(password)) => Arc::new(Password::new(username, password)),
            (None, None) => Arc::new(NoAuth),
            _ => return Err(Error::InvalidSocks5Auth),
        };

        Ok(Self {
            inner: Socks5Server::new(socket, auth),
            dual_stack,
            max_pkt_size,
            next_assoc_id: AtomicU16::new(0),
        })
    }

    pub async fn start() {
        let server = SERVER.get().unwrap();

        log::warn!(
            "[socks5] server started, listening on {}",
            server.inner.local_addr().unwrap()
        );

        loop {
            match server.inner.accept().await {
                Ok((conn, addr)) => {
                    log::debug!("[socks5] [{addr}] connection established");

                    tokio::spawn(async move {
                        match conn.handshake().await {
                            Ok(Connection::Associate(associate, _)) => {
                                let assoc_id = server.next_assoc_id.fetch_add(1, Ordering::Relaxed);
                                log::info!("[socks5] [{addr}] [associate] [{assoc_id:#06x}]");
                                Self::handle_associate(
                                    associate,
                                    assoc_id,
                                    server.dual_stack,
                                    server.max_pkt_size,
                                )
                                .await;
                            }
                            Ok(Connection::Bind(bind, _)) => {
                                log::info!("[socks5] [{addr}] [bind]");
                                Self::handle_bind(bind).await;
                            }
                            Ok(Connection::Connect(connect, target_addr)) => {
                                log::info!("[socks5] [{addr}] [connect] [{target_addr}]");
                                Self::handle_connect(connect, target_addr).await;
                            }
                            Err(err) => log::warn!("[socks5] [{addr}] handshake error: {err}"),
                        };

                        log::debug!("[socks5] [{addr}] connection closed");
                    });
                }
                Err(err) => log::warn!("[socks5] failed to establish connection: {err}"),
            }
        }
    }

    async fn handle_associate(
        assoc: Associate<associate::NeedReply>,
        assoc_id: u16,
        dual_stack: Option<bool>,
        max_pkt_size: usize,
    ) {
        let peer_addr = assoc.peer_addr().unwrap();
        let local_ip = assoc.local_addr().unwrap().ip();

        match UdpSession::new(assoc_id, peer_addr, local_ip, dual_stack, max_pkt_size) {
            Ok(session) => {
                let local_addr = session.local_addr().unwrap();
                log::debug!(
                    "[socks5] [{peer_addr}] [associate] [{assoc_id:#06x}] bound to {local_addr}"
                );

                let mut assoc = match assoc
                    .reply(Reply::Succeeded, Address::SocketAddress(local_addr))
                    .await
                {
                    Ok(assoc) => assoc,
                    Err(err) => {
                        log::warn!("[socks5] [{peer_addr}] [associate] [{assoc_id:#06x}] command reply error: {err}");
                        return;
                    }
                };

                UDP_SESSIONS
                    .get()
                    .unwrap()
                    .lock()
                    .insert(assoc_id, session.clone());

                let handle_local_incoming_pkt = async move {
                    loop {
                        let (pkt, target_addr) = match session.recv().await {
                            Ok(res) => res,
                            Err(err) => {
                                log::warn!("[socks5] [{peer_addr}] [associate] [{assoc_id:#06x}] failed to receive UDP packet: {err}");
                                continue;
                            }
                        };

                        let forward = async move {
                            let target_addr = match target_addr {
                                Address::DomainAddress(domain, port) => {
                                    TuicAddress::DomainAddress(domain, port)
                                }
                                Address::SocketAddress(addr) => TuicAddress::SocketAddress(addr),
                            };

                            match TuicConnection::get().await {
                                Ok(conn) => conn.packet(pkt, target_addr, assoc_id).await,
                                Err(err) => Err(err),
                            }
                        };

                        tokio::spawn(async move {
                            match forward.await {
                                Ok(()) => {}
                                Err(err) => {
                                    log::warn!("[socks5] [{peer_addr}] [associate] [{assoc_id:#06x}] failed relaying UDP packet: {err}");
                                }
                            }
                        });
                    }
                };

                match tokio::select! {
                    res = assoc.wait_until_closed() => res,
                    _ = handle_local_incoming_pkt => unreachable!(),
                } {
                    Ok(()) => {}
                    Err(err) => {
                        log::warn!("[socks5] [{peer_addr}] [associate] [{assoc_id:#06x}] associate connection error: {err}")
                    }
                }

                log::debug!(
                    "[socks5] [{peer_addr}] [associate] [{assoc_id:#06x}] stopped associating"
                );

                UDP_SESSIONS
                    .get()
                    .unwrap()
                    .lock()
                    .remove(&assoc_id)
                    .unwrap();

                let res = match TuicConnection::get().await {
                    Ok(conn) => conn.dissociate(assoc_id).await,
                    Err(err) => Err(err),
                };

                match res {
                    Ok(()) => {}
                    Err(err) => log::warn!("[socks5] [{peer_addr}] [associate] [{assoc_id:#06x}] failed stoping UDP relaying session: {err}"),
                }
            }
            Err(err) => {
                log::warn!("[socks5] [{peer_addr}] [associate] [{assoc_id:#06x}] failed setting up UDP associate session: {err}");

                match assoc
                    .reply(Reply::GeneralFailure, Address::unspecified())
                    .await
                {
                    Ok(mut assoc) => {
                        let _ = assoc.shutdown().await;
                    }
                    Err(err) => {
                        log::warn!("[socks5] [{peer_addr}] [associate] [{assoc_id:#06x}] command reply error: {err}")
                    }
                }
            }
        }
    }

    async fn handle_bind(bind: Bind<bind::NeedFirstReply>) {
        let peer_addr = bind.peer_addr().unwrap();
        log::warn!("[socks5] [{peer_addr}] [bind] command not supported");

        match bind
            .reply(Reply::CommandNotSupported, Address::unspecified())
            .await
        {
            Ok(mut bind) => {
                let _ = bind.shutdown().await;
            }
            Err(err) => log::warn!("[socks5] [{peer_addr}] [bind] command reply error: {err}"),
        }
    }

    async fn handle_connect(conn: Connect<connect::NeedReply>, addr: Address) {
        let peer_addr = conn.peer_addr().unwrap();
        let target_addr = match addr {
            Address::DomainAddress(domain, port) => TuicAddress::DomainAddress(domain, port),
            Address::SocketAddress(addr) => TuicAddress::SocketAddress(addr),
        };

        let relay = match TuicConnection::get().await {
            Ok(conn) => conn.connect(target_addr.clone()).await,
            Err(err) => Err(err),
        };

        match relay {
            Ok(relay) => {
                let mut relay = relay.compat();

                match conn.reply(Reply::Succeeded, Address::unspecified()).await {
                    Ok(mut conn) => match io::copy_bidirectional(&mut conn, &mut relay).await {
                        Ok(_) => {}
                        Err(err) => {
                            let _ = conn.shutdown().await;
                            let _ = relay.get_mut().reset(CONNECTION_CLOSE_ERROR_CODE);
                            log::warn!("[socks5] [{peer_addr}] [connect] [{target_addr}] TCP stream relaying error: {err}");
                        }
                    },
                    Err(err) => {
                        let _ = relay.shutdown().await;
                        log::warn!("[socks5] [{peer_addr}] [connect] [{target_addr}] command reply error: {err}");
                    }
                }
            }
            Err(err) => {
                log::warn!("[socks5] [{peer_addr}] [connect] [{target_addr}] unable to relay TCP stream: {err}");

                match conn
                    .reply(Reply::GeneralFailure, Address::unspecified())
                    .await
                {
                    Ok(mut conn) => {
                        let _ = conn.shutdown().await;
                    }
                    Err(err) => {
                        log::warn!("[socks5] [{peer_addr}] [connect] [{target_addr}] command reply error: {err}")
                    }
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct UdpSession {
    socket: Arc<AssociatedUdpSocket>,
    assoc_id: u16,
    ctrl_addr: SocketAddr,
}

impl UdpSession {
    fn new(
        assoc_id: u16,
        ctrl_addr: SocketAddr,
        local_ip: IpAddr,
        dual_stack: Option<bool>,
        max_pkt_size: usize,
    ) -> Result<Self, Error> {
        let domain = match local_ip {
            IpAddr::V4(_) => Domain::IPV4,
            IpAddr::V6(_) => Domain::IPV6,
        };

        let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP)).map_err(|err| {
            Error::Socket("failed to create socks5 server UDP associate socket", err)
        })?;

        if let Some(dual_stack) = dual_stack {
            socket.set_only_v6(!dual_stack).map_err(|err| {
                Error::Socket(
                    "socks5 server UDP associate dual-stack socket setting error",
                    err,
                )
            })?;
        }

        socket.set_nonblocking(true).map_err(|err| {
            Error::Socket(
                "failed setting socks5 server UDP associate socket as non-blocking",
                err,
            )
        })?;

        socket
            .bind(&SockAddr::from(SocketAddr::from((local_ip, 0))))
            .map_err(|err| {
                Error::Socket("failed to bind socks5 server UDP associate socket", err)
            })?;

        let socket = UdpSocket::from_std(StdUdpSocket::from(socket)).map_err(|err| {
            Error::Socket("failed to create socks5 server UDP associate socket", err)
        })?;

        Ok(Self {
            socket: Arc::new(AssociatedUdpSocket::from((socket, max_pkt_size))),
            assoc_id,
            ctrl_addr,
        })
    }

    pub async fn send(&self, pkt: Bytes, src_addr: Address) -> Result<(), Error> {
        let src_addr_display = src_addr.to_string();

        log::debug!(
            "[socks5] [{ctrl_addr}] [associate] [{assoc_id:#06x}] send packet from {src_addr_display} to {dst_addr}",
            ctrl_addr = self.ctrl_addr,
            assoc_id = self.assoc_id,
            dst_addr = self.socket.peer_addr().unwrap(),
        );

        if let Err(err) = self.socket.send(pkt, 0, src_addr).await {
            log::warn!(
                "[socks5] [{ctrl_addr}] [associate] [{assoc_id:#06x}] send packet from {src_addr_display} to {dst_addr} error: {err}",
                ctrl_addr = self.ctrl_addr,
                assoc_id = self.assoc_id,
                dst_addr = self.socket.peer_addr().unwrap(),
            );

            return Err(Error::Io(err));
        }

        Ok(())
    }

    pub async fn recv(&self) -> Result<(Bytes, Address), Error> {
        let (pkt, frag, dst_addr, src_addr) = self.socket.recv_from().await?;

        if let Ok(connected_addr) = self.socket.peer_addr() {
            if src_addr != connected_addr {
                Err(IoError::new(
                    ErrorKind::Other,
                    format!("invalid source address: {src_addr}"),
                ))?;
            }
        } else {
            self.socket.connect(src_addr).await?;
        }

        if frag != 0 {
            Err(IoError::new(
                ErrorKind::Other,
                "fragmented packet is not supported",
            ))?;
        }

        log::debug!(
            "[socks5] [{ctrl_addr}] [associate] [{assoc_id:#06x}] receive packet from {src_addr} to {dst_addr}",
            ctrl_addr = self.ctrl_addr,
            assoc_id = self.assoc_id
        );

        Ok((pkt, dst_addr))
    }

    fn local_addr(&self) -> Result<SocketAddr, IoError> {
        self.socket.local_addr()
    }
}
