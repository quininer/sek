pub mod external;

use std::process::ExitStatus;
use super::{ syntax, Shell };


pub fn execute(shell: &Shell, input: &str, cmd: syntax::Command)
    -> anyhow::Result<ExitStatus>
{
    external::execute(shell, input, cmd)
}
