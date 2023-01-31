use crate::{connection::Connection as TuicConnection, error::Error};
use bytes::Bytes;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use socks5_proto::{Address, Reply};
use socks5_server::{
    auth::NoAuth,
    connection::{associate, bind, connect},
    Associate, AssociatedUdpSocket, Bind, Connect, Connection, Server,
};
use std::{
    collections::HashMap,
    io::{Error as IoError, ErrorKind},
    net::SocketAddr,
    sync::{
        atomic::{AtomicU16, Ordering},
        Arc,
    },
};
use tokio::{
    io::{self, AsyncWriteExt},
    net::UdpSocket,
};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tuic::Address as TuicAddress;

static NEXT_ASSOCIATE_ID: AtomicU16 = AtomicU16::new(0);
static UDP_SESSIONS: Lazy<Mutex<HashMap<u16, Arc<AssociatedUdpSocket>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub async fn start() -> Result<(), Error> {
    let server = Server::bind("127.0.0.1:5000", Arc::new(NoAuth)).await?;

    while let Ok((conn, _)) = server.accept().await {
        tokio::spawn(async move {
            let res = match conn.handshake().await {
                Ok(Connection::Associate(associate, addr)) => {
                    handle_associate(associate, addr).await
                }
                Ok(Connection::Bind(bind, addr)) => handle_bind(bind, addr).await,
                Ok(Connection::Connect(connect, addr)) => handle_connect(connect, addr).await,
                Err(err) => Err(Error::from(err)),
            };

            match res {
                Ok(_) => {}
                Err(err) => eprintln!("{err}"),
            }
        });
    }

    Ok(())
}

async fn handle_associate(
    assoc: Associate<associate::NeedReply>,
    _addr: Address,
) -> Result<(), Error> {
    let assoc_socket = UdpSocket::bind(SocketAddr::from((assoc.local_addr()?.ip(), 0)))
        .await
        .and_then(|socket| {
            socket
                .local_addr()
                .map(|addr| (Arc::new(AssociatedUdpSocket::from((socket, 1500))), addr))
        });

    match assoc_socket {
        Ok((assoc_socket, assoc_addr)) => {
            let assoc = assoc
                .reply(Reply::Succeeded, Address::SocketAddress(assoc_addr))
                .await?;
            send_pkt(assoc, assoc_socket).await
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
