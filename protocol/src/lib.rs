use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_bytes::Bytes;
use std::num::NonZeroU64;

pub type RequestId = NonZeroU64;

pub const FILENAME: &str = "sek.ipc";

#[derive(Serialize, Deserialize, Debug)]
pub struct ClientMessage<'a> {
    pub request_id: RequestId,
    pub time: Timestamp,
    #[serde(borrow)]
    pub data: ClientMessageData<'a>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ServerMessage<'a> {
    pub request_id: Option<RequestId>,
    pub time: Timestamp,
    #[serde(borrow)]
    pub data: ServerMessageData<'a>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ClientMessageData<'a> {
    ClientHello {
        version: &'a str,
        cwd: &'a Bytes,
        envs: Vec<(&'a Bytes, &'a Bytes)>,
    },
    ChangeDir {
        cwd: &'a Bytes,
    },
    ChangeEnv {
        key: &'a Bytes,
        value: Option<&'a Bytes>,
    },
    StartExecute {
        command: &'a str,
    },
    EndExecute,
    RequestPrompt {
        width: u16,
    },
    RequestSuggest {
        command: &'a str,
    },
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ServerMessageData<'a> {
    ServerHello {
        version: &'a str,
    },
    PushPrompt {
        left: Option<Prompt<'a>>,
        right: Option<Prompt<'a>>,
    },
    PushSuggest {
        command: &'a str,
    },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Prompt<'a> {
    #[serde(borrow)]
    pub data: &'a Bytes,
    pub size: (u16, u16),
}
