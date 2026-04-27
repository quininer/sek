pub mod external;

use std::process::ExitStatus;
use super::{ syntax, Shell };


pub async fn execute(shell: &Shell, input: &str, cmd: syntax::Command)
    -> anyhow::Result<Option<ExitStatus>>
{
    // TODO builtin command
    external::execute(shell, input, cmd).await.map(Some)
}
