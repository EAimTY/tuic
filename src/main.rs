use futures_util::StreamExt;
use quinn::{ClientConfig, Endpoint, Incoming, ServerConfig};
use rustls::{Certificate, PrivateKey, RootCertStore};
use std::net::SocketAddr;

async fn handle_server(mut incoming: Incoming) {
    while let Some(conn) = incoming.next().await {
        let mut conn = conn.await.unwrap();

        while let Some(Ok((_, recv))) = conn.bi_streams.next().await {
            println!("Server received a msg!");
            let msg = recv.read_to_end(10).await.unwrap();
            println!("Server received: {:?}", std::str::from_utf8(&msg).unwrap());
        }
    }
}

async fn handle_client(client: Endpoint, server_addr: SocketAddr, server_name: &str) {
    let conn = client
        .connect(server_addr, server_name)
        .unwrap()
        .await
        .unwrap();

    let (mut send, _) = conn.connection.open_bi().await.unwrap();

    println!("Sending msg to server...");
    send.write_all(b"a msg").await.unwrap();
    send.finish().await.unwrap();
    println!("msg was sent!");
}

#[tokio::main]
async fn main() {
    let (cert, private_key) = load_cert();
    let (server_config, client_config) = load_cert_config(cert, private_key);

    let (_, incoming) = Endpoint::server(server_config, ([127, 0, 0, 1], 5000).into()).unwrap();

    tokio::spawn(handle_server(incoming));

    let client = {
        let mut endpoint = Endpoint::client(([127, 0, 0, 1], 5001).into()).unwrap();
        endpoint.set_default_client_config(client_config);
        endpoint
    };

    handle_client(client, ([127, 0, 0, 1], 5000).into(), "localhost").await;
}

const CERT: &[u8] = include_bytes!("../cert.der");
const KEY: &[u8] = include_bytes!("../key.der");

fn load_cert() -> (Certificate, PrivateKey) {
    let cert = Certificate(CERT.to_vec());
    let private_key = PrivateKey(KEY.to_vec());

    (cert, private_key)
}

fn load_cert_config(cert: Certificate, private_key: PrivateKey) -> (ServerConfig, ClientConfig) {
    let mut root_cert_store = RootCertStore::empty();
    root_cert_store.add(&cert).unwrap();

    let server_config = ServerConfig::with_single_cert(vec![cert], private_key).unwrap();
    let client_config = ClientConfig::with_root_certificates(root_cert_store);

    (server_config, client_config)
}
