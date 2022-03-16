use super::{IsAuthenticated, UdpSessionMap};
use anyhow::{bail, Result};
use quinn::{Connection as QuinnConnection, RecvStream, SendStream, VarInt};
use std::{hint::unreachable_unchecked, net::ToSocketAddrs, sync::Arc};
use tokio::{io, net::TcpStream};
use tuic_protocol::{Address, Command, Response};

mod bind;
mod connect;
mod dissociate;
mod packet;

pub async fn handle_uni_stream(
    mut stream: RecvStream,
    conn: QuinnConnection,
    assoc_map: Arc<UdpSessionMap>,
    expected_token_digest: [u8; 32],
    is_authenticated: IsAuthenticated,
) {
    let cmd = match Command::read_from(&mut stream).await {
        Ok(cmd) => cmd,
        Err(err) => {
            eprintln!("{err}");
            conn.close(VarInt::MAX, b"Bad command");
            return;
        }
    };

    if let Command::Authenticate { digest } = cmd {
        if digest == expected_token_digest {
            is_authenticated.set_authenticated();
        } else {
            eprintln!("Authentication failed");
            conn.close(VarInt::MAX, b"Authentication failed");
        }

        return;
    }

    if is_authenticated.await {
        match cmd {
            Command::Authenticate { .. } => unsafe { unreachable_unchecked() },
            Command::Connect { .. } => conn.close(VarInt::MAX, b"Bad command"),
            Command::Bind { .. } => conn.close(VarInt::MAX, b"Bad command"),
            Command::Packet {
                assoc_id,
                len,
                addr,
            } => {
                async fn handle_packet(
                    mut stream: RecvStream,
                    assoc_map: Arc<UdpSessionMap>,
                    assoc_id: u32,
                    len: u16,
                    addr: Address,
                ) {
                    let mut buf = vec![0; len as usize];

                    match stream.read_exact(&mut buf).await {
                        Ok(()) => assoc_map.send(assoc_id, buf, addr).await,
                        Err(err) => eprintln!("{err}"),
                    }
                }

                tokio::spawn(handle_packet(
                    stream,
                    assoc_map.clone(),
                    assoc_id,
                    len,
                    addr,
                ));
            }
            Command::Dissociate { assoc_id } => assoc_map.dissociate(assoc_id),
        }
    } else {
        eprintln!("Authentication timeout");
        conn.close(VarInt::MAX, b"Authentication timeout");
    }
}

pub async fn handle_bi_stream(
    send: SendStream,
    mut recv: RecvStream,
    conn: QuinnConnection,
    is_authenticated: IsAuthenticated,
) {
    let cmd = match Command::read_from(&mut recv).await {
        Ok(cmd) => cmd,
        Err(err) => {
            eprintln!("{err}");
            conn.close(VarInt::MAX, b"Bad command");
            return;
        }
    };

    if is_authenticated.await {
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
    } else {
        eprintln!("Authentication timeout");
        conn.close(VarInt::MAX, b"Authentication timeout");
    }

    async fn handle_connect(
        mut send: SendStream,
        mut recv: RecvStream,
        addr: Address,
    ) -> Result<()> {
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
}

pub async fn handle_received_udp_packet(
    conn: QuinnConnection,
    assoc_id: u32,
    packet: Vec<u8>,
    addr: Address,
) {
    let res: Result<()> = try {
        let mut stream = conn.open_uni().await?;
        let cmd = Command::new_packet(assoc_id, packet.len() as u16, addr);
        cmd.write_to(&mut stream).await?;
        stream.write_all(&packet).await?;
    };

    match res {
        Ok(()) => {}
        Err(err) => eprintln!("{err}"),
    }
}
