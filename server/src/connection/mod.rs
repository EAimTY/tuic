use parking_lot::Mutex;
use quinn::{Connecting, Connection as QuinnConnection, NewConnection};
use std::{
    collections::HashMap,
    sync::{atomic::AtomicBool, Arc},
    time::Instant,
};

mod bi_stream;
mod datagram;
mod uni_stream;

pub struct Connection {
    controller: QuinnConnection,
    assoc_map: Arc<AssociateMap>,
    is_authenticated: Arc<AtomicBool>,
    create_time: Instant,
}

impl Connection {
    pub async fn handle(conn: Connecting, expected_token_digest: [u8; 32]) {
        match conn.await {
            Ok(NewConnection {
                connection,
                uni_streams,
                bi_streams,
                datagrams,
                ..
            }) => {
                let conn = Self {
                    controller: connection,
                    assoc_map: Arc::new(AssociateMap::new()),
                    is_authenticated: Arc::new(AtomicBool::new(false)),
                    create_time: Instant::now(),
                };

                tokio::join!(
                    conn.listen_uni_streams(uni_streams, expected_token_digest),
                    conn.listen_bi_streams(bi_streams),
                    conn.listen_datagrams(datagrams)
                );
            }
            Err(err) => eprintln!("{err}"),
        }
    }
}

pub struct AssociateMap(Mutex<HashMap<(), ()>>);

impl AssociateMap {
    pub fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }
}
