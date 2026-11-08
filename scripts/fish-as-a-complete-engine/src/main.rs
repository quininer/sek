use std::io;
use std::process::Command;
use std::os::unix::process::CommandExt;
use argh::FromArgs;


/// Fish as a complete engine
///
/// The `<index>` is the index of the token the cursor is on, the tokens
/// behind it are the tail of the line and are not a part of the command
/// line `complete -C` is given.
/// The token under the cursor could be empty, which means the cursor is
/// on a new token.
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

/// escape one token for the command line which `complete -C` re-parses.
///
/// There is only one level of escaping, because the command line is given
/// to fish through `argv` (`$argv[1]`), not interpolated into a fish
/// script, so nothing eats the escapes before fish does.
fn escape_token(out: &mut String, token: &str) {
    for c in token.chars() {
        match c {
            ' ' | '\t' | '\n' | '\r'
            | '\'' | '"' | '\\' | '$' | '`' | ';' | '|' | '&' | '<' | '>'
            | '(' | ')' | '{' | '}' | '[' | ']' | '#' | '~' | '*' | '?' => {
                out.push('\\');
                out.push(match c {
                    '\t' => 't',
                    '\n' => 'n',
                    '\r' => 'r',
                    c => c,
                });
            }
            c => out.push(c),
        }
    }
}

/// the caller used a lone space argv as the "the cursor is here" marker,
/// it means the same as an empty token
fn token_of(arg: &str) -> &str {
    if arg.chars().all(char::is_whitespace) { "" } else { arg }
}

fn build(args: &[String], index: usize) -> String {
    // only the command line up to the cursor is given to fish,
    // the tokens behind the cursor are the tail of the line
    // TODO https://github.com/fish-shell/fish-shell/issues/12219
    // a cursor behind the last token is a new empty token
    let taken = (index + 1).min(args.len() + 1);
    let mut out = String::new();
    let mut cursor_on_empty = true;

    for idx in 0..taken {
        let token = args.get(idx).map(|arg| token_of(arg)).unwrap_or("");

        cursor_on_empty = token.is_empty();

        // the separator only depends on the tokens before the cursor,
        // a separator after the token under the cursor would move the
        // cursor to the next token; for an empty token the separator is
        // all it has, so it is pushed even when the token is empty
        if idx > 0 {
            out.push(' ');
        }

        if !cursor_on_empty {
            escape_token(&mut out, token);
        }
    }

    // `complete -C` takes the end of the string as the cursor, so an empty
    // token at the beginning of the line is a buffer of one space
    if cursor_on_empty && out.is_empty() {
        out.push(' ');
    }

    out
}

fn main() -> io::Result<()> {
    let options: Options = argh::from_env();

    let buffer = build(&options.commands, options.index);

    let mut cmd = sandbox();

    // TODO https://github.com/fish-shell/fish-shell/issues/6943
    cmd.args([
        "fish",
        "-P",
        "-c",
        // the buffer is passed by argv to avoid a second level of quoting
        "complete -C \"$argv[1]\"",
        buffer.as_str()
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
