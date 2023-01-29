use crate::{connection::Connection as TuicConnection, error::Error};
use bytes::Bytes;
use socks5_proto::{Address, Reply};
use socks5_server::{
    auth::NoAuth,
    connection::{associate, bind, connect},
    Associate, Bind, Connect, Connection, Server,
};
use std::sync::Arc;
use tokio::io::{self, AsyncWriteExt};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tuic::Address as TuicAddress;

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
    addr: Address,
) -> Result<(), Error> {
    todo!()
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

pub async fn recv_pkt(pkt: Bytes, addr: TuicAddress, assoc_id: u16) -> Result<(), Error> {
    todo!()
}
