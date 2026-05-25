use std::io;
use std::ffi::OsStr;
use std::num::NonZeroU64;
use std::cell::RefCell;
use serde_bytes::Bytes;
use tokio::io::{ AsyncReadExt, AsyncWriteExt };
use tokio::net::UnixStream;
use crate::shell::env::Environment;
use sek_protocol::{ FILENAME, ClientMessage };
pub use sek_protocol::{ RequestId, ClientMessageData, ServerMessage, ServerMessageData };

pub struct Client {
    socket: UnixStream,
    request_id: RequestId,
}

impl Client {
    #[allow(clippy::await_holding_refcell_ref)]
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

        // recv server hello
        let msg: ServerMessage<'_> = client.recv_msg(&mut buf).await?;
        let ServerMessageData::ServerHello { version } = msg.data
            else {
                anyhow::bail!("expected server-hello but recv {:?}", msg.data);
            };

        let _version = version;

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

pub async fn readable(client: Option<&Client>) -> anyhow::Result<()> {
    use std::task::Poll;
    use std::future::poll_fn;

    poll_fn(|cx| match client {
        Some(client) => client.socket.poll_read_ready(cx),
        None => Poll::Pending
    }).await?;

    Ok(())
}
