use bstr::{ ByteSlice, BString };
use logos::Span;
use crate::shell::syntax::{ Command, Argument, ArgSlice, StrSlice };
use crate::shell::Shell;
use crate::ui::render::{ Renderer, TermTarget };
use crate::editor::{ Action, Mode };

#[derive(Debug)]
pub enum CompletionType {
    None,
    Exe(Span),
    Path(Span, BString),
    Flag(Span, BString),
    Value,
}

pub async fn complete(shell: &Shell, cmd: Command)
    -> CompletionType
{
    let point = shell.editor.insert.cursor().end;

    // check exe
    let exe = cmd.exe(&shell.parser);
    if let span = exe.span(&shell.parser)
        && (span.contains(&point) || span.end == point)
    {
        let mut iter = exe.slice(&shell.parser);

        if let Some(ArgSlice::Literal(lit)) = iter.next()
            && iter.next().is_none()
            && lit.span(&shell.parser).contains(&point)
        {
            return CompletionType::Exe(lit.span(&shell.parser));
        }
    }

    // check path
    if let Some(args) = shell.parser.iter()
        .filter_map(|node_id| Argument::new(&shell.parser, node_id))
        .find(|args| {
            let span = args.span(&shell.parser);
            span.contains(&point) || span.end == point
        })
    {
        let has_subshell = args.slice(&shell.parser)
            .any(|arg| match arg {
                ArgSlice::SubShell(_) => true,
                ArgSlice::DoubleStr(s) => s.slice(&shell.parser)
                    .any(|s| matches!(s, StrSlice::SubShell(_))),
                _ => false
            });
        if !has_subshell {
            let s = shell.editor.insert.as_str();
            let mut buf = Vec::new();
            let mut push = |osstr: &[u8]| {
                buf.extend_from_slice(osstr);
                Ok(())
            };

            if args.eval(shell, s, &mut push).await.is_ok() {
                // path check
                if buf.starts_with_str("/")
                    || buf.starts_with_str("./")
                    || buf.ends_with_str("/")
                    || buf.as_slice() == b"."
                {
                    return CompletionType::Path(args.span(&shell.parser), buf.into());
                }
            }
        }
    }

    CompletionType::None
}

impl CompletionType {
    pub async fn resolve<T: TermTarget>(
        self,
        shell: &mut Shell,
        renderer: &mut Renderer<T>,
        action: &mut anyhow::Result<Action>,
    ) -> anyhow::Result<()> {
        match self {
            CompletionType::None => (),
            CompletionType::Exe(_prefix) => (),
            CompletionType::Path(span, prefix) => {
                renderer.screen_reset()?;

                let env = shell.env.borrow();
                let path = prefix.to_path()?;
                let path = if path.is_relative() {
                    env.pwd().join(path)
                } else {
                    path.into()
                };
                
                let (dir, prefix) = if prefix.ends_with_str(b"/") || path.is_dir() {
                    (&*path, "")
                } else {
                    let dir = path.parent().unwrap_or(env.pwd());
                    let prefix = path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or_default();
                    (dir, prefix)
                };

                shell.editor.command.clear();
                shell.editor.path_selector.search = prefix.into();
                shell.editor.path_selector.set_glob(None);
                shell.editor.path_selector.set_space(renderer.size.1.into());
                match shell.editor.path_selector.cd(dir) {
                    Ok(()) => {
                        *shell.editor.insert.cursor_mut() = span;
                        shell.editor.mode = Mode::PathSelector;
                        let _ = shell.editor.path_selector.search_down();
                    },
                    Err(err) => {
                        *action = Err(err);

                        // TODO path-selector error
                    }
                }
            },
            CompletionType::Flag(..) => (),
            CompletionType::Value => (),
        }

        Ok(())
    }
}
