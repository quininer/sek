#![cfg_attr(not(unix), allow(unused))]

use std::{ io, mem };
use std::ffi::{OsStr, OsString};
use std::cell::RefCell;
use std::path::PathBuf;
use std::num::NonZeroU64;
use serde_bytes::Bytes;
use tokio::io::{ AsyncReadExt, AsyncWriteExt };
use crate::shell::env::Environment;
use sek_protocol::{ FILENAME, ClientMessage };
pub use sek_protocol::{ RequestId, ClientMessageData, ServerMessage, ServerMessageData };

#[cfg(unix)]
use tokio::net::UnixStream;

pub struct Client {
    #[cfg(unix)]
    socket: UnixStream,
    request_id: RequestId,
    env_changed: Vec<OsString>,
    prev_round_pwd: Option<PathBuf>,
}

#[cfg(unix)]
impl Client {
    pub async fn connect(env: &RefCell<Environment>) -> anyhow::Result<Option<Client>> {
        let env = env.borrow();

        let path = if let Some(path) = env.get(OsStr::new("SEK_IPC_PATH")) {
            path.into()
        } else {
            env.projdir.runtime_dir()
                .unwrap_or_else(|| env.projdir.data_local_dir())
                .join(FILENAME)
        };
        let socket = match UnixStream::connect(path).await {
            Ok(socket) => socket,
            Err(ref err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err.into())
        };
        let ucred = socket.peer_cred()?;
        let uid = unsafe { libc::getuid() };
        
        if ucred.uid() != uid {
            anyhow::bail!("ipc authentication failed: {} != {}", ucred.uid(), uid);
        }

        let mut client = Client {
            socket,
            request_id: NonZeroU64::new(1).unwrap(),
            env_changed: Vec::new(),
            prev_round_pwd: None,
        };
        let mut buf = Vec::new();

        // send client hello
        let hello = ClientMessageData::ClientHello {
            version: concat!(env!("CARGO_PKG_NAME"), "-", env!("CARGO_PKG_VERSION")),
            cwd: Bytes::new(env.pwd().as_os_str().as_encoded_bytes()),
            envs: env.map.iter()
                .map(|(k, v)| (Bytes::new(k.as_encoded_bytes()), Bytes::new(v.as_encoded_bytes())))
                .collect()
        };
        let request_id = client.send_msg(&mut buf, hello).await?;
        client.socket.flush().await?;

        // recv server hello
        let msg: ServerMessage<'_> = client.recv_msg(&mut buf).await?;
        let ServerMessageData::ServerHello { .. } = msg.data
            else {
                anyhow::bail!("expected server-hello but recv {:?}", msg.data);
            };

        if msg.request_id != Some(request_id) {
            anyhow::bail!("request_id mismatch: {:?} != {:?}", msg.request_id, request_id);
        }

        Ok(Some(client))
    }

    pub async fn send_msg(&mut self, buf: &mut Vec<u8>, data: ClientMessageData<'_>)
        -> anyhow::Result<RequestId>
    {
        let request_id = self.request_id;
        self.request_id = self.request_id.checked_add(1).unwrap();

        let msg = ClientMessage {
            time: jiff::Timestamp::now(),
            request_id, data,
        };

        buf.clear();
        cbor4ii::serde::to_writer(&mut *buf, &msg)?;
        drop(msg);

        let len: u32 = buf.len().try_into().unwrap();
        self.socket.write_all(&len.to_le_bytes()).await?;
        self.socket.write_all(buf).await?;

        Ok(request_id)
    }

    pub async fn recv_msg<'a>(&mut self, buf: &'a mut Vec<u8>)
        -> anyhow::Result<ServerMessage<'a>>
    {
        let mut lenbuf = [0; 4];
        self.socket.read_exact(&mut lenbuf).await?;

        let len: usize = u32::from_le_bytes(lenbuf).try_into().unwrap();
        buf.resize(len, 0);
        self.socket.read_exact(buf).await?;

        let msg = cbor4ii::serde::from_slice(buf)?;
        Ok(msg)
    }
}

#[cfg(not(unix))]
impl Client {
    pub async fn connect(env: &RefCell<Environment>) -> anyhow::Result<Option<Client>> {
        Ok(None)
    }

    pub async fn send_msg(&mut self, buf: &mut Vec<u8>, data: ClientMessageData<'_>)
        -> anyhow::Result<RequestId>
    {
        anyhow::bail!("unimplemented")
    }

    pub async fn recv_msg<'a>(&mut self, buf: &'a mut Vec<u8>)
        -> anyhow::Result<ServerMessage<'a>>
    {
        todo!()
    }
}

impl Client {
    pub fn env_changed(&mut self, env: &OsStr) {
        self.env_changed.push(env.into());
    }

    pub async fn send_update(
        &mut self,
        env: &RefCell<Environment>,
        ipcbuf: &mut Vec<u8>
    )
        -> anyhow::Result<()>
    {
        let env = env.borrow();

        if self.prev_round_pwd.as_deref() != Some(env.pwd()) {
            self.prev_round_pwd = Some(env.pwd().into());

            self.send_msg(ipcbuf, ClientMessageData::ChangeDir {
                cwd: Bytes::new(env.pwd().as_os_str().as_encoded_bytes())
            }).await?;
        }        

        for k in mem::take(&mut self.env_changed) {
            self.send_msg(ipcbuf, ClientMessageData::ChangeEnv {
                key: Bytes::new(k.as_encoded_bytes()),
                value: env.get(&k).map(|v| Bytes::new(v.as_encoded_bytes()))
            }).await?;
        }

        self.socket.flush().await?;
        
        Ok(())
    }
}

#[cfg(unix)]
pub async fn readable(client: Option<&RefCell<Client>>) -> anyhow::Result<()> {
    use std::task::Poll;
    use std::future::poll_fn;

    poll_fn(|cx| match client {
        Some(client) => client.borrow().socket.poll_read_ready(cx),
        None => Poll::Pending
    }).await?;

    Ok(())
}

#[cfg(unix)]
pub async fn read<'a>(client: Option<&RefCell<Client>>, ipcbuf: &'a mut Vec<u8>)
    -> anyhow::Result<ServerMessage<'a>>
{
    match client {
        Some(client) => client.borrow_mut().recv_msg(ipcbuf).await,
        None => std::future::pending().await
    }
}

#[cfg(not(unix))]
pub async fn readable(client: Option<&Client>) -> anyhow::Result<()> {
    use std::future;

    future::pending::<()>().await;

    Ok(())
}

pub async fn start_execute(
    client: Option<&RefCell<Client>>,
    ipcbuf: &mut Vec<u8>,
    command: &str
)
    -> anyhow::Result<Option<RequestId>>
{
    let Some(client) = client
        else {
            return Ok(None)
        };
    if command.starts_with(' ') {
        return Ok(None);
    }

    let request_id = client.borrow_mut()
        .send_msg(ipcbuf, ClientMessageData::StartExecute { command })
        .await?;
    Ok(Some(request_id))
}

pub async fn end_execute(
    client: Option<&RefCell<Client>>,
    ipcbuf: &mut Vec<u8>,
    request_id: Option<RequestId>,
    code: i32
)
    -> anyhow::Result<()>
{
    let Some(client) = client
        else {
            return Ok(())
        };
    let Some(request_id) = request_id
        else {
            return Ok(());
        };

    client.borrow_mut()
        .send_msg(ipcbuf, ClientMessageData::EndExecute {
            request_id,
            code
        })
        .await?;
    Ok(())
}

pub async fn request_suggest(
    client: Option<&RefCell<Client>>,
    ipcbuf: &mut Vec<u8>,
    command: &str
)
    -> anyhow::Result<Option<RequestId>>
{
    let Some(client) = client
        else {
            return Ok(None)
        };
    client.borrow_mut()
        .send_msg(ipcbuf, ClientMessageData::RequestSuggest {
            command
        })
        .await
        .map(Some)
}
