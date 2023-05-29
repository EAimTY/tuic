use super::Connection;
use crate::{Error, UdpRelayMode};
use bytes::Bytes;
use quinn::{RecvStream, SendStream, VarInt};
use register_count::Register;
use std::sync::atomic::Ordering;
use tokio::time;
use tuic_quinn::Task;

impl Connection {
    pub(crate) async fn handle_uni_stream(self, recv: RecvStream, _reg: Register) {
        let addr = self.inner.remote_address();
        log::debug!("[{addr}] incoming unidirectional stream");

        let max = self.max_concurrent_uni_streams.load(Ordering::Relaxed);

        if self.remote_uni_stream_cnt.count() as u32 == max {
            self.max_concurrent_uni_streams
                .store(max * 2, Ordering::Relaxed);

            self.inner
                .set_max_concurrent_uni_streams(VarInt::from(max * 2));
        }

        let pre_process = async {
            let task = time::timeout(
                self.task_negotiation_timeout,
                self.model.accept_uni_stream(recv),
            )
            .await
            .map_err(|_| Error::TaskNegotiationTimeout)??;

            if let Task::Authenticate(auth) = &task {
                self.authenticate(auth)?;
            }

            tokio::select! {
                () = self.auth.clone() => {}
                err = self.inner.closed() => return Err(Error::Connection(err)),
            };

            let same_pkt_src = matches!(task, Task::Packet(_))
                && matches!(self.udp_relay_mode.load(), Some(UdpRelayMode::Native));
            if same_pkt_src {
                return Err(Error::UnexpectedPacketSource);
            }

            Ok(task)
        };

        match pre_process.await {
            Ok(Task::Authenticate(auth)) => self.handle_authenticate(auth).await,
            Ok(Task::Packet(pkt)) => self.handle_packet(pkt, UdpRelayMode::Quic).await,
            Ok(Task::Dissociate(assoc_id)) => self.handle_dissociate(assoc_id).await,
            Ok(_) => unreachable!(), // already filtered in `tuic_quinn`
            Err(err) => {
                log::warn!("[{addr}] handle unidirection stream error: {err}");
                self.close();
            }
        }
    }

    pub(crate) async fn handle_bi_stream(
        self,
        (send, recv): (SendStream, RecvStream),
        _reg: Register,
    ) {
        let addr = self.inner.remote_address();
        log::debug!("[{addr}] incoming bidirectional stream");

        let max = self.max_concurrent_bi_streams.load(Ordering::Relaxed);

        if self.remote_bi_stream_cnt.count() as u32 == max {
            self.max_concurrent_bi_streams
                .store(max * 2, Ordering::Relaxed);

            self.inner
                .set_max_concurrent_bi_streams(VarInt::from(max * 2));
        }

        let pre_process = async {
            let task = time::timeout(
                self.task_negotiation_timeout,
                self.model.accept_bi_stream(send, recv),
            )
            .await
            .map_err(|_| Error::TaskNegotiationTimeout)??;

            tokio::select! {
                () = self.auth.clone() => {}
                err = self.inner.closed() => return Err(Error::Connection(err)),
            };

            Ok(task)
        };

        match pre_process.await {
            Ok(Task::Connect(conn)) => self.handle_connect(conn).await,
            Ok(_) => unreachable!(), // already filtered in `tuic_quinn`
            Err(err) => {
                log::warn!("[{addr}] handle bidirection stream error: {err}");
                self.close();
            }
        }
    }

    pub(crate) async fn handle_datagram(self, dg: Bytes) {
        let addr = self.inner.remote_address();
        log::debug!("[{addr}] incoming datagram");

        let pre_process = async {
            let task = self.model.accept_datagram(dg)?;

            tokio::select! {
                () = self.auth.clone() => {}
                err = self.inner.closed() => return Err(Error::Connection(err)),
            };

            let same_pkt_src = matches!(task, Task::Packet(_))
                && matches!(self.udp_relay_mode.load(), Some(UdpRelayMode::Quic));
            if same_pkt_src {
                return Err(Error::UnexpectedPacketSource);
            }

            Ok(task)
        };

        match pre_process.await {
            Ok(Task::Packet(pkt)) => self.handle_packet(pkt, UdpRelayMode::Native).await,
            Ok(Task::Heartbeat) => self.handle_heartbeat().await,
            Ok(_) => unreachable!(),
            Err(err) => {
                log::warn!("[{addr}] handle datagram error: {err}");
                self.close();
            }
        }
    }
}
