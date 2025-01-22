use std::io;
use std::path::PathBuf;
use tokio::net::UnixStream;
use directories::ProjectDirs;
use super::build_ipc_path;


pub struct Connect {
    path: PathBuf,
    stream: Option<UnixStream>
}

impl Connect {
    pub fn new(projdir: &ProjectDirs) -> Self {
        let path = build_ipc_path(projdir);
        Connect { path, stream: None }
    }

    async fn reconnect(&mut self) -> io::Result<()> {
        self.stream = Some(UnixStream::connect(&self.path).await?);
        Ok(())
    }

    //
}
