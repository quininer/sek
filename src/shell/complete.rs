use std::ffi::OsStr;
use bstr::{ ByteSlice, BString };
use logos::Span;
use crate::shell::syntax::{ Command, Argument, ArgSlice, StrSlice, Variable };
use crate::shell::Shell;
use crate::ui::render::{ Renderer, TermTarget };
use crate::editor::Mode;

#[derive(Debug)]
pub enum CompletionType {
    None,
    Exe(Span),
    Env(Span),
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
        {
            return CompletionType::Exe(lit.span(&shell.parser));
        }
    }

    // check env
    if let Some(span) = shell.parser.iter()
        .filter_map(|node_id| Variable::new(&shell.parser, node_id))
        .find_map(|var| {
            let span = var.span(&shell.parser);
            (span.contains(&point) || span.end == point).then_some(span)
        })
    {
        return CompletionType::Env(span);
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
    ) -> anyhow::Result<()> {
        match self {
            CompletionType::None => (),
            CompletionType::Exe(span) => {
                let prefix = &shell.editor.insert.as_str()[span.clone()];
                let cache = shell.cache.borrow();
                let list = cache
                    .exe_set
                    .search(prefix)
                    .filter_map(|buf| String::from_utf8(buf.into()).ok());
                shell.editor.complete_selector.list.clear();
                shell.editor.complete_selector.desc.clear();
                shell.editor.complete_selector.list.extend(list);

                if !shell.editor.complete_selector.list.is_empty() {
                    shell.editor.complete_selector.cur = 0;
                    shell.editor.complete_selector.set_space(renderer.size);
                    shell.editor.complete_selector.update();
                    *shell.editor.insert.cursor_mut() = span;
                    shell.editor.mode = Mode::CompleteSelector;
                }
            },
            CompletionType::Env(span) => {
                let span = (span.start + 1)..span.end;
                let prefix = &shell.editor.insert.as_str()[span.clone()];
                let env = shell.env.borrow();
                let list = env
                    .search(OsStr::new(prefix))
                    .filter_map(|(k, _)| k.to_str().map(Into::into));
                shell.editor.complete_selector.list.clear();
                shell.editor.complete_selector.desc.clear();
                shell.editor.complete_selector.list.extend(list);
                if !shell.editor.complete_selector.list.is_empty() {
                    shell.editor.complete_selector.cur = 0;
                    shell.editor.complete_selector.set_space(renderer.size);
                    shell.editor.complete_selector.update();
                    *shell.editor.insert.cursor_mut() = span;
                    shell.editor.mode = Mode::CompleteSelector;
                }
            },
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

                if prefix.starts_with('.') {
                    shell.editor.path_selector.set_hidden_file(false);
                }

                shell.editor.command.clear();
                shell.editor.path_selector.search = prefix.into();
                shell.editor.path_selector.set_glob(None);
                shell.editor.path_selector.set_space(renderer.size.1.into());
                shell.editor.path_selector.cd(dir)?;
                *shell.editor.insert.cursor_mut() = span;
                shell.editor.mode = Mode::PathSelector;
                shell.editor.path_selector.search_down()?;
            },
            CompletionType::Flag(..) => (),
            CompletionType::Value => (),
        }

        Ok(())
    }
}
