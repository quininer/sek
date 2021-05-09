pub mod env;
pub mod parser;
pub mod process;

use crate::shell::env::Env;
use crate::shell::process::Morgue;

pub struct Shell {
    env: Env,
    morgue: Morgue
}
