use std::ops::Range;
use serde::{ Serialize, Deserialize };
use tokio::sync::RwLock;
use super::Daemon;


#[derive(Default)]
pub struct Command {
    cmds: RwLock<Commands>
}

#[derive(Serialize, Deserialize)]
pub enum Request<'a> {
    Reload,
    Complete(&'a str),
    FilterData
}

#[derive(Serialize, Deserialize)]
pub enum Response<'a> {
    ReloadOk,
    Complete(Vec<&'a str>),
    FilterData(&'a [u8])
}

#[derive(Default)]
struct Commands {
    string: String,
    set: Vec<Range<usize>>,
    filter: ()
}

impl Command {
    pub async fn execute(&self, daemon: &Daemon, req: Request<'_>)
        -> anyhow::Result<()>
    {
        match req {
            Request::Reload => self.reload().await,
            Request::Complete(prefix) => self.complete(&prefix).await,
            Request::FilterData => self.get_filter().await
        }
    }

    async fn reload(&self) -> anyhow::Result<()> {
        todo!()
    }

    async fn complete(&self, prefix: &str) -> anyhow::Result<()> {
        todo!()
    }

    async fn get_filter(&self) -> anyhow::Result<()> {
        todo!()
    }
}
