pub mod external;

use std::process::ExitStatus;
use tokio::signal::ctrl_c;
use crate::util::{ Either, Select };
use super::{ syntax, Shell };


pub async fn execute(shell: &Shell, input: &str, cmd: syntax::Command)
    -> anyhow::Result<Option<ExitStatus>>
{
    match Select::new(
        external::execute(shell, input, cmd),   
        ctrl_c(),
    ).await {
        Either::Left(result) => result.map(Some),
        Either::Right(Ok(())) => Ok(None),
        Either::Right(Err(err)) => Err(err.into())
    }
}
