use super::{Address, Connection};
use anyhow::Result;
use quinn::{RecvStream, SendStream};
use tokio::sync::oneshot::Sender as OneshotSender;
use tuic_protocol::{Address as TuicAddress, Command as TuicCommand, Response as TuicResponse};

impl Connection {
    pub fn handle_connect(
        &self,
        addr: Address,
        tx: OneshotSender<Option<(SendStream, RecvStream)>>,
    ) {
        let conn = self.controller.clone();

        tokio::spawn(async move {
            let res: Result<()> = try {
                let (mut send, mut recv) = conn.open_bi().await?;

                let addr = TuicAddress::from(addr);
                let cmd = TuicCommand::new_connect(addr);

                cmd.write_to(&mut send).await?;

                let resp = TuicResponse::read_from(&mut recv).await?;

                if resp.is_succeeded() {
                    let _ = tx.send(Some((send, recv)));
                    return;
                }
            };

            match res {
                Ok(()) => (),
                Err(err) => eprintln!("{err}"),
            }

            let _ = tx.send(None);
        });
    }
}
