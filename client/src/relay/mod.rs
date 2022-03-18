use self::connection::Connection;
use crate::config::{CongestionController, ServerAddr, UdpMode};
use anyhow::Result;
use quinn::{
    congestion::{BbrConfig, CubicConfig, NewRenoConfig},
    ClientConfig, ConnectionError, Endpoint, SendDatagramError, TransportConfig, WriteError,
};
use rustls::{Certificate, RootCertStore};
use std::{
    io::Error as IoError,
    mem::{self, MaybeUninit},
    net::{SocketAddr, ToSocketAddrs},
    sync::Arc,
    vec::IntoIter,
};
use thiserror::Error;
use tokio::sync::mpsc::{self, Receiver as MpscReceiver, Sender as MpscSender};
use tuic_protocol::Error as ProtocolError;

pub use self::{address::Address, request::Request};

mod address;
mod connection;
mod request;

pub struct Relay {
    req_rx: MpscReceiver<Request>,
    endpoint: Endpoint,
    server_addr: ServerAddr,
    token_digest: [u8; 32],
    udp_mode: UdpMode,
    reduce_rtt: bool,
}

impl Relay {
    pub fn init(
        server_addr: ServerAddr,
        certificate: Option<Certificate>,
        token_digest: [u8; 32],
        udp_mode: UdpMode,
        congestion_controller: CongestionController,
        reduce_rtt: bool,
    ) -> Result<(Self, MpscSender<Request>)> {
        let config = {
            let mut config = if let Some(cert) = certificate {
                let mut root_cert_store = RootCertStore::empty();
                root_cert_store.add(&cert)?;
                ClientConfig::with_root_certificates(root_cert_store)
            } else {
                ClientConfig::with_native_roots()
            };

            let mut transport = TransportConfig::default();

            match congestion_controller {
                CongestionController::Cubic => {
                    transport.congestion_controller_factory(Arc::new(CubicConfig::default()))
                }
                CongestionController::NewReno => {
                    transport.congestion_controller_factory(Arc::new(NewRenoConfig::default()))
                }
                CongestionController::Bbr => {
                    transport.congestion_controller_factory(Arc::new(BbrConfig::default()))
                }
            };

            config.transport = Arc::new(transport);

            config
        };

        let mut endpoint = Endpoint::client(SocketAddr::from(([0, 0, 0, 0], 0)))?;
        endpoint.set_default_client_config(config);

        let (req_tx, req_rx) = mpsc::channel(1);

        let relay = Self {
            req_rx,
            endpoint,
            server_addr,
            token_digest,
            udp_mode,
            reduce_rtt,
        };

        Ok((relay, req_tx))
    }

    pub async fn run(mut self) {
        let mut conn = self.establish_connection().await;

        while let Some(req) = self.req_rx.recv().await {
            if conn.is_closed() {
                conn = self.establish_connection().await;
            }

            let conn_cloned = conn.clone();

            tokio::spawn(async move {
                match conn_cloned.process_request(req).await {
                    Ok(()) => (),
                    Err(err) => {
                        eprintln!("{err}");
                    }
                }
            });
        }
    }

    async fn establish_connection(&self) -> Connection {
        let (mut addrs, server_name) = match &self.server_addr {
            ServerAddr::HostnameAddr { hostname, .. } => (
                unsafe { mem::transmute(MaybeUninit::<IntoIter<SocketAddr>>::uninit()) },
                hostname,
            ),
            ServerAddr::SocketAddr {
                server_addr,
                server_name,
            } => (vec![*server_addr].into_iter(), server_name),
        };

        loop {
            if let ServerAddr::HostnameAddr {
                hostname,
                server_port,
            } = &self.server_addr
            {
                match (hostname.as_str(), *server_port).to_socket_addrs() {
                    Ok(resolved) => addrs = resolved,
                    Err(err) => {
                        eprintln!("{err}");
                        continue;
                    }
                }
            }

            for addr in addrs.as_ref() {
                match self.endpoint.connect(*addr, server_name) {
                    Ok(conn) => {
                        match Connection::init(
                            conn,
                            self.token_digest,
                            self.udp_mode,
                            self.reduce_rtt,
                        )
                        .await
                        {
                            Ok(conn) => return conn,
                            Err(err) => eprintln!("{err}"),
                        }
                    }
                    Err(err) => eprintln!("{err}"),
                }
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum RelayError {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    Connection(#[from] ConnectionError),
    #[error(transparent)]
    WriteStream(#[from] WriteError),
    #[error(transparent)]
    SendDatagram(#[from] SendDatagramError),
    #[error("UDP session not found: {0}")]
    UdpSessionNotFound(u32),
    #[error("bad command")]
    BadCommand,
}
