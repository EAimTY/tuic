use super::Connection;
use crate::{error::Error, utils::UdpRelayMode};
use bytes::Bytes;
use quinn::{RecvStream, SendStream, VarInt};
use register_count::Register;
use std::sync::atomic::Ordering;
use tuic_quinn::Task;

impl Connection {
    pub async fn accept_uni_stream(&self) -> Result<(RecvStream, Register), Error> {
        let max = self.max_concurrent_uni_streams.load(Ordering::Relaxed);

        if self.remote_uni_stream_cnt.count() as u32 == max {
            self.max_concurrent_uni_streams
                .store(max * 2, Ordering::Relaxed);

            self.conn
                .set_max_concurrent_uni_streams(VarInt::from(max * 2));
        }

        let recv = self.conn.accept_uni().await?;
        let reg = self.remote_uni_stream_cnt.reg();
        Ok((recv, reg))
    }

    pub async fn accept_bi_stream(&self) -> Result<(SendStream, RecvStream, Register), Error> {
        let max = self.max_concurrent_bi_streams.load(Ordering::Relaxed);

        if self.remote_bi_stream_cnt.count() as u32 == max {
            self.max_concurrent_bi_streams
                .store(max * 2, Ordering::Relaxed);

            self.conn
                .set_max_concurrent_bi_streams(VarInt::from(max * 2));
        }

        let (send, recv) = self.conn.accept_bi().await?;
        let reg = self.remote_bi_stream_cnt.reg();
        Ok((send, recv, reg))
    }

    pub async fn accept_datagram(&self) -> Result<Bytes, Error> {
        Ok(self.conn.read_datagram().await?)
    }

    pub async fn handle_uni_stream(self, recv: RecvStream, _reg: Register) {
        log::debug!("[relay] incoming unidirectional stream");

        let res = match self.model.accept_uni_stream(recv).await {
            Err(err) => Err(Error::Model(err)),
            Ok(Task::Packet(pkt)) => match self.udp_relay_mode {
                UdpRelayMode::Quic => {
                    Self::handle_packet(pkt).await;
                    Ok(())
                }
                UdpRelayMode::Native => Err(Error::WrongPacketSource),
            },
            _ => unreachable!(), // already filtered in `tuic_quinn`
        };

        if let Err(err) = res {
            log::warn!("[relay] incoming unidirectional stream error: {err}");
        }
    }

    pub async fn handle_bi_stream(self, send: SendStream, recv: RecvStream, _reg: Register) {
        log::debug!("[relay] incoming bidirectional stream");

        let res = match self.model.accept_bi_stream(send, recv).await {
            Err(err) => Err::<(), _>(Error::Model(err)),
            _ => unreachable!(), // already filtered in `tuic_quinn`
        };

        if let Err(err) = res {
            log::warn!("[relay] incoming bidirectional stream error: {err}");
        }
    }

    pub async fn handle_datagram(self, dg: Bytes) {
        log::debug!("[relay] incoming datagram");

        let res = match self.model.accept_datagram(dg) {
            Err(err) => Err(Error::Model(err)),
            Ok(Task::Packet(pkt)) => match self.udp_relay_mode {
                UdpRelayMode::Native => {
                    Self::handle_packet(pkt).await;
                    Ok(())
                }
                UdpRelayMode::Quic => Err(Error::WrongPacketSource),
            },
            _ => unreachable!(), // already filtered in `tuic_quinn`
        };

        if let Err(err) = res {
            log::warn!("[relay] incoming datagram error: {err}");
        }
    }
}
