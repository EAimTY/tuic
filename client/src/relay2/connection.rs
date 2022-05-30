use super::Address;
use bytes::Bytes;
use parking_lot::Mutex;
use std::collections::HashMap;
use tokio::sync::mpsc::Sender;

pub type UdpSessionMap = Mutex<HashMap<u32, Sender<(Bytes, Address)>>>;

pub struct Connection;
