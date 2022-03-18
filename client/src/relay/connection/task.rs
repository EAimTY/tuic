use super::{Address, Error, UdpSessionMap};
use bytes::{Bytes, BytesMut};
use quinn::{Connection as QuinnConnection, RecvStream, SendStream};
use std::sync::Arc;
use tokio::sync::oneshot::Sender as OneshotSender;
use tuic_protocol::{Address as TuicAddress, Command as TuicCommand, Response as TuicResponse};

pub async fn connect(
    conn: QuinnConnection,
    addr: Address,
    tx: OneshotSender<Option<(SendStream, RecvStream)>>,
) -> Result<(), Error> {
    async fn get_streams(
        conn: QuinnConnection,
        addr: Address,
    ) -> Result<Option<(SendStream, RecvStream)>, Error> {
        let (mut send, mut recv) = conn.open_bi().await?;

        let addr = TuicAddress::from(addr);
        let cmd = TuicCommand::new_connect(addr);

        cmd.write_to(&mut send).await?;

        let resp = TuicResponse::read_from(&mut recv).await?;

        if resp.is_succeeded() {
            Ok(Some((send, recv)))
        } else {
            Ok(None)
        }
    }

    match get_streams(conn, addr).await {
        Ok(res) => {
            let _ = tx.send(res);
            Ok(())
        }
        Err(err) => {
            let _ = tx.send(None);
            Err(err)
        }
    }
}

pub async fn packet_to_uni_stream(
    conn: QuinnConnection,
    assoc_id: u32,
    pkt: Bytes,
    addr: Address,
) -> Result<(), Error> {
    let mut stream = conn.open_uni().await?;

    let addr = TuicAddress::from(addr);
    let cmd = TuicCommand::new_packet(assoc_id, pkt.len() as u16, addr);

    cmd.write_to(&mut stream).await?;
    stream.write_all(&pkt).await?;

    Ok(())
}

pub async fn packet_to_datagram(
    conn: QuinnConnection,
    assoc_id: u32,
    pkt: Bytes,
    addr: Address,
) -> Result<(), Error> {
    let addr = TuicAddress::from(addr);
    let cmd = TuicCommand::new_packet(assoc_id, pkt.len() as u16, addr);

    let mut buf = BytesMut::with_capacity(cmd.serialized_len());
    cmd.write_to_buf(&mut buf);
    buf.extend_from_slice(&pkt);

    let pkt = buf.freeze();
    conn.send_datagram(pkt)?;

    Ok(())
}

pub async fn packet_from_server(
    pkt: Bytes,
    udp_sessions: Arc<UdpSessionMap>,
    assoc_id: u32,
    addr: Address,
) -> Result<(), Error> {
    let recv_pkt_tx = udp_sessions
        .lock()
        .get(&assoc_id)
        .cloned()
        .ok_or(Error::UdpSessionNotFound(assoc_id))?;

    let _ = recv_pkt_tx.send((pkt, addr)).await;

    Ok(())
}

pub async fn dissociate(conn: QuinnConnection, assoc_id: u32) -> Result<(), Error> {
    let mut stream = conn.open_uni().await?;
    let cmd = TuicCommand::new_dissociate(assoc_id);
    cmd.write_to(&mut stream).await?;

    Ok(())
}
