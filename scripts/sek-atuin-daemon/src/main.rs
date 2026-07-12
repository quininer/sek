use std::{ io, env };
use std::sync::Arc;
use std::path::Path;
use anyhow::Context;
use tokio::runtime;
use tokio::io::{ AsyncReadExt, AsyncWriteExt };
use tokio::net::{ UnixListener, UnixStream };
use tracing::{ info, warn };
use directories::ProjectDirs;
use atuin_client::history::History;
use atuin_client::database::Context as AtuinContext;
use atuin_client::settings::{ SearchMode, FilterMode };
use atuin_client::database::{ Sqlite, Database, OptFilters };
use sek_protocol::{ FILENAME, RequestId, ClientMessage, ServerMessage, ClientMessageData, ServerMessageData };

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let rt = runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()?;

    let projdir = ProjectDirs::from("", "", "sek")
        .context("Unable to retrieve project path from system")?;

    let path = if let Some(path) = env::var_os("SEK_IPC_PATH") {
        path.into()
    } else {
        projdir.runtime_dir()
            .unwrap_or_else(|| projdir.data_local_dir())
            .join(FILENAME)
    };

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }

    std::fs::remove_file(&path)
        .or_else(|err| if err.kind() == io::ErrorKind::NotFound {
            Ok(())
        } else {
            Err(err)
        })?;

    rt.block_on(daemon(&projdir, &path))    
}

struct State {
    history: Sqlite,
}

async fn daemon(projdir: &ProjectDirs, path: &Path) -> anyhow::Result<()> {
    info!(?path, "listen atuin daemon");

    let data_dir = projdir.data_local_dir().join("atuin");
    std::fs::create_dir_all(&data_dir)?;
    let history = Sqlite::new(data_dir.join("hist.db"), 100.).await?;
    
    let listener = UnixListener::bind(path)
        .context("listen failed")?;

    let state = Arc::new(State {
        history,
    });

    loop {
        let (mut stream, _) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            let mut recvbuf = Vec::new();
            let mut sendbuf = Vec::new();
            
            let mut session = match Session::handshake(
                &mut stream,
                &mut sendbuf,
                &mut recvbuf
            ).await {
                Ok(session) => session,
                Err(err) => {
                    warn!(?err, "handshake failed");
                    return;
                }
            };
            if let Err(err) = handle(
                state,
                stream,
                &mut session,
                &mut sendbuf,
                &mut recvbuf,
            ).await {
                warn!(session_id=session.session_id, ?err, "session is down");
            }
        });
    }
}

struct Session {
    session_id: String,
    cwd: Vec<u8>,
    envs: Vec<(Vec<u8>, Vec<u8>)>,
    running: Vec<Command>,
    context: AtuinContext,
}

struct Command {
    request_id: RequestId,
    when_run: jiff::Timestamp,
    history: History,
}

impl Session {
    async fn handshake(
        stream: &mut UnixStream,
        sendbuf: &mut Vec<u8>,
        recvbuf: &mut Vec<u8>,
    ) -> anyhow::Result<Session> {
        let ucred = stream.peer_cred()?;
        let uid = unsafe { libc::getuid() };

        if ucred.uid() != uid {
            anyhow::bail!("ipc authentication failed: {} != {}", ucred.uid(), uid);
        }

        let msg = recv_msg(stream, recvbuf).await?;
        let ClientMessageData::ClientHello { version, cwd, envs } = msg.data
            else {
                anyhow::bail!("bad handshake message");
            };

        info!(?version, "handshake ok");

        send_msg(
            stream,
            sendbuf,
            Some(msg.request_id),
            ServerMessageData::ServerHello {
                version: concat!(env!("CARGO_PKG_NAME"), "-", env!("CARGO_PKG_VERSION"))
            }
        ).await?;

        let session_id = format!(
            "{}-{}",
            ucred.pid().unwrap_or_default(),
            msg.time.as_millisecond(),
        );

        let context = AtuinContext {
            session: session_id.clone(),
            cwd: String::from_utf8_lossy(cwd).into_owned(),
            hostname: hostname::get()
                .ok()
                .and_then(|s| s.into_string().ok())
                .unwrap_or_default(),
            host_id: Default::default(),
            git_root: None
        };

        Ok(Session {
            session_id, context,
            cwd: (&**cwd).into(),
            envs: envs.into_iter()
                .map(|(k ,v)| ((&**k).into(), (&**v).into()))
                .collect(),
            running: Vec::new(),
        })
    }
}

async fn handle(
    state: Arc<State>,
    mut stream: UnixStream,
    session: &mut Session,
    sendbuf: &mut Vec<u8>,
    recvbuf: &mut Vec<u8>,
) -> anyhow::Result<()> {
    loop {
        let msg = match recv_msg(&mut stream, recvbuf).await {
            Ok(msg) => msg,
            Err(ref err) if err.downcast_ref::<io::Error>()
                .is_some_and(|err| err.kind() == io::ErrorKind::UnexpectedEof)
                => break,
            Err(err) => return Err(err)
        };

        match msg.data {
            ClientMessageData::ChangeDir { cwd } => session.cwd = (&**cwd).into(),
            ClientMessageData::ChangeEnv { key, value } => {
                match session.envs.binary_search_by_key(&&**key, |(k, _)| k.as_slice()) {
                    Ok(idx) => match value {
                        Some(value) => session.envs[idx].1 = (&**value).into(),
                        None => {
                            session.envs.remove(idx);
                        }
                    },
                    Err(idx) => if let Some(value) = value {
                        session.envs.insert(idx, ((&**key).into(), (&**value).into()));
                    }
                }
            },
            ClientMessageData::StartExecute { command } => {
                let time = time::OffsetDateTime::from_unix_timestamp_nanos(msg.time.as_nanosecond())
                    .ok()
                    .unwrap_or_else(time::OffsetDateTime::now_utc);
                let mut item: History = History::capture()
                    .timestamp(time)
                    .command(command)
                    .cwd(String::from_utf8_lossy(&session.cwd))
                    .build()
                    .into();
                item.id = format!("{}-{}", session.session_id, msg.request_id).into();
                state.history.save(&item).await?;

                session.running.push(Command {
                    request_id: msg.request_id,
                    when_run: msg.time,
                    history: item,
                });
                session.running.sort_by_key(|cmd| cmd.request_id);
            },
            ClientMessageData::EndExecute { request_id, code } => {
                if let Ok(idx) = session.running
                    .binary_search_by_key(&request_id, |cmd| cmd.request_id)
                {
                    info!(?request_id, "execute command");
                    
                    let mut cmd = session.running.remove(idx);
                    let duration = msg.time.as_duration() - cmd.when_run.as_duration();

                    cmd.history.exit = code.into();
                    cmd.history.duration = duration.as_millis().try_into().unwrap_or_default();

                    state.history.update(&cmd.history).await?;
                }
            },
            ClientMessageData::RequestSuggest { command } => {
                let commands = state.history.search(
                    SearchMode::Prefix,
                    FilterMode::Global,
                    &session.context,
                    command,
                    OptFilters {
                        exit: Some(0),
                        limit: Some(1),
                        ..Default::default()
                    }
                ).await?;

                if let Some(command) = commands.first() {
                    let command = command.command.as_str();
                    send_msg(
                        &mut stream,
                        sendbuf,
                        Some(msg.request_id),
                        ServerMessageData::PushSuggest { command }
                    ).await?;
                }
            },
            ClientMessageData::RequestHistory { command, num } => {
                let commands = state.history.search(
                    SearchMode::Fuzzy,
                    FilterMode::Global,
                    &session.context,
                    command,
                    OptFilters {
                        limit: Some(num.into()),
                        cwd: Some(session.context.cwd.clone()),
                        ..Default::default()
                    }
                ).await?;

                info!(request=?msg.request_id, len=?commands.len(), "request history");
                
                let commands = commands.iter()
                    .map(|cmd| cmd.command.as_str())
                    .collect();
                send_msg(
                    &mut stream,
                    sendbuf,
                    Some(msg.request_id),
                    ServerMessageData::PushHistory { commands }
                ).await?;
            },
            ClientMessageData::RequestPrompt { .. } => (),
            ClientMessageData::ClientHello { .. } => anyhow::bail!("bad message")
        }
    }

    Ok(())
}

async fn recv_msg<'a>(stream: &mut UnixStream, buf: &'a mut Vec<u8>)
    -> anyhow::Result<ClientMessage<'a>>
{
    let mut lenbuf = [0; 4];

    stream.read_exact(&mut lenbuf).await?;
    buf.resize(u32::from_le_bytes(lenbuf) as usize, 0);
    stream.read_exact(buf).await?;

    Ok(cbor4ii::serde::from_slice(buf)?)
}

async fn send_msg(
    stream: &mut UnixStream,
    buf: &mut Vec<u8>,
    request_id: Option<RequestId>,
    data: ServerMessageData<'_>
)
    -> anyhow::Result<()>
{
    let msg = ServerMessage {
        request_id, data,
        time: jiff::Timestamp::now(),
    };
    
    buf.clear();
    cbor4ii::serde::to_writer(&mut *buf, &msg)?;
    drop(msg);

    let len: u32 = buf.len().try_into().unwrap();
    stream.write_all(&len.to_le_bytes()).await?;
    stream.write_all(buf).await?;

    Ok(())
}
