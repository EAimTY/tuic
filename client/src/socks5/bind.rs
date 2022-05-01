use crate::relay::Request as RelayRequest;
use socks5_proto::{Address, Reply};
use socks5_server::{connection::bind::NeedFirstReply, Bind};
use std::io::{Error as IoError, ErrorKind};
use tokio::{io::AsyncWriteExt, sync::mpsc::Sender};

pub async fn handle(
    conn: Bind<NeedFirstReply>,
    _req_tx: Sender<RelayRequest>,
    _target_addr: Address,
) -> Result<(), IoError> {
    let mut conn = conn
        .reply(Reply::CommandNotSupported, Address::unspecified())
        .await?;

    let _ = conn.shutdown().await;

    Err(IoError::new(
        ErrorKind::Unsupported,
        "BIND command is not supported",
    ))
}
