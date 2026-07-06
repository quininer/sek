use std::io;
use std::process::Command;
use std::os::unix::process::CommandExt;
use argh::FromArgs;


/// Fish as a complete engine
///
/// ```json
/// "complete": {
/// 	"exe": "fish-as-a-complete-engine",
/// 	"args": ["-i", "@index"]
/// },
/// ```
#[derive(FromArgs)]
struct Options {
    /// arg index
    #[argh(option, short = 'i')]
    index: usize,

    /// do-complete for command
    #[argh(positional, greedy)]
    commands: Vec<String>,
}

fn main() -> io::Result<()> {
    let options: Options = argh::from_env();

    let mut s = String::new();
    for (idx, arg) in options.commands
        .iter()
        // TODO https://github.com/fish-shell/fish-shell/issues/12219
        .take(options.index + 1)
        .enumerate()
    {
        if !arg.contains(&['\'', ' ']) {
            s.push_str(arg)
        } else {
            for c in arg.chars() {
                match c {
                    '\'' => s.push_str("\\'"),
                    ' ' => s.push_str("\\ "),
                    '\\' => s.push_str("\\\\"),
                    c => s.push(c),
                }
            }
        }

        if options.commands.get(idx + 1).is_some() {
            s.push(' ');
        }
    }
    let complete = format!("complete -C '{}'", s);

    let mut cmd = sandbox();

    // TODO https://github.com/fish-shell/fish-shell/issues/6943
    cmd.args([
        "fish",
        "-P",
        "-c",
        complete.as_str()
    ]);

    Err(cmd.exec())
}

#[cfg(target_os = "linux")]
fn sandbox() -> Command {
    let mut cmd = Command::new("bwrap");
    cmd.args([
        "--die-with-parent",
        "--unshare-all",
        "--cap-drop", "ALL",
        "--ro-bind", "/", "/",
        "--dev-bind", "/dev/null", "/dev/null",
        "--",
    ]);
    cmd
}

#[cfg(target_os = "macos")]
fn sandbox() -> Command {
    let mut cmd = Command::new("sandbox-exec");
    cmd.args([
        "-p",
"(version 1)\
(deny default)\
(allow file-read*)\
(allow sysctl-read)\
(allow process-fork)\
(allow process-exec)\
(allow process-info* (target same-sandbox))\
(allow file-write* (subpath \"/dev/null\"))",
    ]);
    cmd
}
