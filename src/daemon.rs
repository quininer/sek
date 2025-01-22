mod connect;
mod cmd;

use std::sync::Arc;
use std::path::PathBuf;
use argh::FromArgs;
use directories::ProjectDirs;
use serde::{ Serialize, Deserialize };
use tokio::{io::AsyncReadExt, net::{ UnixListener, UnixStream }};
pub use connect::Connect;


/// The Sek Daemon
#[derive(FromArgs)]
#[argh(subcommand, name = "daemon")]
pub struct Options {
    //
}

struct Daemon {
    uid: libc::uid_t,
    projdir: ProjectDirs,
    confpath: PathBuf,
    cmd: Command
}

#[derive(Default)]
struct Command {
    exe: cmd::Command,
}

#[derive(Serialize, Deserialize)]
pub enum Request<'a> {
    #[serde(borrow)]
    Command(cmd::Request<'a>)
}

#[derive(Serialize, Deserialize)]
pub enum Response<'a> {
    #[serde(borrow)]
    Command(cmd::Response<'a>)
}

impl Options {
    pub async fn exec(self, projdir: ProjectDirs, confpath: PathBuf) -> anyhow::Result<()> {
        let listener = UnixListener::bind(build_ipc_path(&projdir))?;

        let daemon = Arc::new(Daemon {
            uid: unsafe {
                libc::getuid()
            },
            cmd: Default::default(),
            projdir, confpath
        });

        loop {
            let (stream, _) = listener.accept().await?;

            let daemon = daemon.clone();
            tokio::spawn(async move {
                if let Err(err) = execute(&daemon, stream).await {
                    eprintln!("ipc execute failed: {:?}", err);
                }
            });
        }
    }
}

async fn execute(daemon: &Daemon, mut stream: UnixStream) -> anyhow::Result<()> {
    let ucred = stream.peer_cred()?;
    anyhow::ensure!(ucred.uid() == daemon.uid, "uid is not consistent");
    let mut buf = Vec::new();

    loop {
        let len = stream.read_u16_le().await?;
        buf.resize(len.into(), 0);
        stream.read_exact(&mut buf).await?;

        let req: Request<'_> = cbor4ii::serde::from_slice(&buf)?;

        let result = match req {
            Request::Command(req) => daemon.cmd.exe.execute(daemon, req).await,
        };

        //
    }
}

fn build_ipc_path(projdir: &ProjectDirs) -> PathBuf {
    projdir.runtime_dir()
        .unwrap_or_else(|| projdir.cache_dir())
        .join("sek.socket")
}
