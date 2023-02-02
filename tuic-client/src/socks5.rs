use crate::{config::Local, connection::Connection as TuicConnection, error::Error};
use bytes::Bytes;
use once_cell::sync::{Lazy, OnceCell};
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
static NEXT_ASSOCIATE_ID: AtomicU16 = AtomicU16::new(0);
static UDP_SESSIONS: Lazy<Mutex<HashMap<u16, Arc<AssociatedUdpSocket>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub struct Server {
    inner: Socks5Server,
    addr: SocketAddr,
    dual_stack: Option<bool>,
    max_packet_size: usize,
}

impl Server {
    pub fn set_config(cfg: Local) -> Result<(), Error> {
        let socket = {
            let domain = match cfg.server.ip() {
                IpAddr::V4(_) => Domain::IPV4,
                IpAddr::V6(_) => Domain::IPV6,
            };

            let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;

            if let Some(dual_stack) = cfg.dual_stack {
                socket.set_only_v6(!dual_stack)?;
            }

            socket.set_reuse_address(true)?;
            socket.bind(&SockAddr::from(cfg.server))?;
            TcpListener::from_std(StdTcpListener::from(socket))?
        };

        let auth: Arc<dyn Auth + Send + Sync> = match (cfg.username, cfg.password) {
            (Some(username), Some(password)) => {
                Arc::new(Password::new(username.into_bytes(), password.into_bytes()))
            }
            (None, None) => Arc::new(NoAuth),
            _ => return Err(Error::InvalidAuth),
        };

        let server = Self {
            inner: Socks5Server::new(socket, auth),
            addr: cfg.server,
            dual_stack: cfg.dual_stack,
            max_packet_size: cfg.max_packet_size,
        };

        SERVER
            .set(server)
            .map_err(|_| "socks5 server already initialized")
            .unwrap();

        Ok(())
    }

    pub async fn start() {
        let server = SERVER.get().unwrap();

        loop {
            match server.inner.accept().await {
                Ok((conn, _)) => {
                    tokio::spawn(async move {
                        let res = match conn.handshake().await {
                            Ok(Connection::Associate(associate, addr)) => {
                                Self::handle_associate(associate, addr).await
                            }
                            Ok(Connection::Bind(bind, addr)) => Self::handle_bind(bind, addr).await,
                            Ok(Connection::Connect(connect, addr)) => {
                                Self::handle_connect(connect, addr).await
                            }
                            Err(err) => Err(Error::from(err)),
                        };

                        match res {
                            Ok(_) => {}
                            Err(err) => eprintln!("{err}"),
                        }
                    });
                }
                Err(err) => eprintln!("{err}"),
            }
        }
    }

    async fn handle_associate(
        assoc: Associate<associate::NeedReply>,
        _addr: Address,
    ) -> Result<(), Error> {
        async fn get_assoc_socket() -> Result<(Arc<AssociatedUdpSocket>, SocketAddr), IoError> {
            let domain = match SERVER.get().unwrap().addr.ip() {
                IpAddr::V4(_) => Domain::IPV4,
                IpAddr::V6(_) => Domain::IPV6,
            };

            let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;

            if let Some(dual_stack) = SERVER.get().unwrap().dual_stack {
                socket.set_only_v6(!dual_stack)?;
            }

            socket.set_reuse_address(true)?;
            socket.bind(&SockAddr::from(SERVER.get().unwrap().addr))?;

            let socket = AssociatedUdpSocket::from((
                UdpSocket::from_std(StdUdpSocket::from(socket))?,
                SERVER.get().unwrap().max_packet_size,
            ));

            let addr = socket.local_addr()?;
            Ok((Arc::new(socket), addr))
        }

        match get_assoc_socket().await {
            Ok((assoc_socket, assoc_addr)) => {
                let assoc = assoc
                    .reply(Reply::Succeeded, Address::SocketAddress(assoc_addr))
                    .await?;
                Self::send_pkt(assoc, assoc_socket).await
            }
            Err(err) => {
                let mut assoc = assoc
                    .reply(Reply::GeneralFailure, Address::unspecified())
                    .await?;
                let _ = assoc.shutdown().await;
                Err(Error::from(err))
            }
        }
    }

    async fn handle_bind(bind: Bind<bind::NeedFirstReply>, _addr: Address) -> Result<(), Error> {
        let mut conn = bind
            .reply(Reply::CommandNotSupported, Address::unspecified())
            .await?;
        let _ = conn.shutdown().await;
        Ok(())
    }

    async fn handle_connect(conn: Connect<connect::NeedReply>, addr: Address) -> Result<(), Error> {
        let target_addr = match addr {
            Address::DomainAddress(domain, port) => TuicAddress::DomainAddress(domain, port),
            Address::SocketAddress(addr) => TuicAddress::SocketAddress(addr),
        };

        let relay = match TuicConnection::get().await {
            Ok(conn) => conn.connect(target_addr).await,
            Err(err) => Err(err),
        };

        match relay {
            Ok(relay) => {
                let mut relay = relay.compat();
                let conn = conn.reply(Reply::Succeeded, Address::unspecified()).await;

                match conn {
                    Ok(mut conn) => match io::copy_bidirectional(&mut conn, &mut relay).await {
                        Ok(_) => Ok(()),
                        Err(err) => {
                            let _ = conn.shutdown().await;
                            let _ = relay.shutdown().await;
                            Err(Error::from(err))
                        }
                    },
                    Err(err) => {
                        let _ = relay.shutdown().await;
                        Err(Error::from(err))
                    }
                }
            }
            Err(err) => {
                let mut conn = conn
                    .reply(Reply::GeneralFailure, Address::unspecified())
                    .await?;
                let _ = conn.shutdown().await;
                Err(err)
            }
        }
    }

    async fn send_pkt(
        mut assoc: Associate<associate::Ready>,
        assoc_socket: Arc<AssociatedUdpSocket>,
    ) -> Result<(), Error> {
        let assoc_id = NEXT_ASSOCIATE_ID.fetch_add(1, Ordering::AcqRel);
        UDP_SESSIONS.lock().insert(assoc_id, assoc_socket.clone());
        let mut connected = None;

        async fn accept_pkt(
            assoc_socket: &AssociatedUdpSocket,
            connected: &mut Option<SocketAddr>,
            assoc_id: u16,
        ) -> Result<(), Error> {
            let (pkt, frag, dst_addr, src_addr) = assoc_socket.recv_from().await?;

            if let Some(connected) = connected {
                if connected != &src_addr {
                    Err(IoError::new(
                        ErrorKind::Other,
                        format!("invalid source address: {src_addr}"),
                    ))?;
                }
            } else {
                assoc_socket.connect(src_addr).await?;
                *connected = Some(src_addr);
            }

            if frag != 0 {
                Err(IoError::new(
                    ErrorKind::Other,
                    format!("fragmented packet is not supported"),
                ))?;
            }

            let target_addr = match dst_addr {
                Address::DomainAddress(domain, port) => TuicAddress::DomainAddress(domain, port),
                Address::SocketAddress(addr) => TuicAddress::SocketAddress(addr),
            };

            TuicConnection::get()
                .await?
                .packet(pkt, target_addr, assoc_id)
                .await
        }

        let res = tokio::select! {
            res = assoc.wait_until_closed() => res,
            _ = async { loop {
                if let Err(err) = accept_pkt(&assoc_socket, &mut connected, assoc_id).await {
                    eprintln!("{err}");
                }
            }} => unreachable!(),
        };

        let _ = assoc.shutdown().await;
        UDP_SESSIONS.lock().remove(&assoc_id);

        match TuicConnection::get().await {
            Ok(conn) => match conn.dissociate(assoc_id).await {
                Ok(_) => {}
                Err(err) => eprintln!("{err}"),
            },
            Err(err) => eprintln!("{err}"),
        }

        Ok(res?)
    }

    pub async fn recv_pkt(pkt: Bytes, addr: Address, assoc_id: u16) -> Result<(), Error> {
        let sessions = UDP_SESSIONS.lock();
        let Some(assoc_socket) = sessions.get(&assoc_id) else { unreachable!() };
        assoc_socket.send(pkt, 0, addr).await?;
        Ok(())
    }
}
