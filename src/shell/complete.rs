use std::ffi::OsStr;
use std::ops::Range;
use bstr::{ ByteSlice, BString };
use logos::Span;
use crate::shell::syntax::{ Command, SubShell, Argument, ArgSlice, StrSlice, Variable };
use crate::shell::Shell;
use crate::ui::render::{ Renderer, TermTarget };
use crate::editor::{ Editor, Mode };

#[derive(Debug)]
pub enum CompletionType {
    Exe(Span),
    Env(Span),
    Path {
        select: Span,
        prefix: BString,
    },
    Item {
        command: Command,
        select: Span,
    },
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
                    return CompletionType::Path {
                        select: args.span(&shell.parser),
                        prefix: buf.into()
                    };
                }
            }
        }
    }

    let command = shell.parser.iter()
        .filter_map(|node_id| SubShell::new(&shell.parser, node_id))
        .map(|subshell| {
            let start = subshell.start(&shell.parser).start;
            let end = subshell
                .end(&shell.parser)
                .map(|span| span.end)
                .unwrap_or(shell.editor.insert.as_str().len());
            (subshell.command(&shell.parser), start..end)
        })
        .filter(|(_, span)| span.contains(&point) || point == span.end)
        .min_by_key(|(_, span)| span.len())
        .map(|(cmd, _)| cmd)
        .unwrap_or(cmd);
    let select = shell.parser.iter()
        .filter_map(|node_id| Argument::new(&shell.parser, node_id))
        .find_map(|args| {
            let span = args.span(&shell.parser);
            (span.contains(&point) || span.end == point).then_some(span)
        })
        .unwrap_or(point..point);

    CompletionType::Item { command, select }
}

impl CompletionType {
    pub async fn resolve<T: TermTarget>(
        self,
        shell: &mut Shell,
        renderer: &mut Renderer<T>,
    ) -> anyhow::Result<()> {
        match self {
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

                if make_complete_selector(
                    &mut shell.editor,
                    span,
                    renderer.size,
                ) {
                    let input = shell.editor.insert.as_str();
                    shell.ast = shell.parser.parse_incomplete(input).ok();
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

                if make_complete_selector(
                    &mut shell.editor,
                    span,
                    renderer.size,
                ) {
                    let input = shell.editor.insert.as_str();
                    shell.ast = shell.parser.parse_incomplete(input).ok();
                }
            },
            CompletionType::Path { select, prefix } => {
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
                shell.editor.insert.select_span(select);
                shell.editor.mode = Mode::Path;
                shell.editor.path_selector.search_down()?;
            },
            CompletionType::Item { command, select } =>
                do_complete(shell, renderer, command, select).await?,
        }

        Ok(())
    }

    pub async fn suggest(self, shell: &mut Shell) -> anyhow::Result<()> {
        shell.editor.suggestion.clear();
        
        match self {
            CompletionType::Exe(span) => {
                let prefix = &shell.editor.insert.as_str()[span.clone()];
                let cache = shell.cache.borrow();
                let mut iter = cache
                    .exe_set
                    .search(prefix)
                    .filter_map(|buf| String::from_utf8(buf.into()).ok());
                if let Some(item) = iter.next()
                    && iter.next().is_none()
                    && let Some(item) = item.strip_prefix(prefix)
                {
                    shell.editor.suggestion.set_value(item);
                }
            },
            CompletionType::Env(span) => {
                let span = (span.start + 1)..span.end;
                let prefix = &shell.editor.insert.as_str()[span.clone()];
                let env = shell.env.borrow();
                let mut iter = env
                    .search(OsStr::new(prefix))
                    .filter_map(|(k, _)| k.to_str());
                if let Some(item) = iter.next()
                    && iter.next().is_none()
                    && let Some(item) = item.strip_prefix(prefix)
                {
                    shell.editor.suggestion.set_value(item);
                }
            },
            CompletionType::Path { .. } => (),
            CompletionType::Item { .. } => (),
        }

        if shell.editor.suggestion.as_str(&shell.editor.insert).is_empty()
            && shell.ipc.is_none()
        {
            shell.editor.suggestion.set_history(&shell.editor.insert);
        }

        Ok(())
    }
}

pub async fn do_complete<T: TermTarget>(
    shell: &mut Shell,
    renderer: &mut Renderer<T>,
    command: Command,
    select: Span
) -> anyhow::Result<()> {
    use std::iter;
    use std::process::{ Command, Stdio };

    let config = shell.config.borrow();

    let Some(complete) = config.complete.as_ref()
        else {
            return Ok(())
        };

    let has_subshell = iter::once(command.exe(&shell.parser))
        .chain(command.args(&shell.parser))
        .flat_map(|arg| arg.slice(&shell.parser))
        .any(|arg| match arg {
            ArgSlice::SubShell(_) => true,
            ArgSlice::DoubleStr(s) => s.slice(&shell.parser)
                .any(|s| matches!(s, StrSlice::SubShell(_))),
            _ => false
        });
    if has_subshell {
        return Ok(());
    }

    let input = shell.editor.insert.as_str();
    let mut buf = Vec::new();
    let mut list = Vec::new();
    let mut index = None;

    for arg in iter::once(command.exe(&shell.parser))
        .chain(command.args(&shell.parser))
    {
        let span = arg.span(&shell.parser);

        if span.start >= select.end {
            index = Some(list.len());
            list.push(buf.len()..buf.len());
        }

        let start = buf.len();
        
        arg.eval(shell, input, &mut |osstr| {
            buf.extend_from_slice(osstr);
            Ok(())
        }).await?;

        if span == select {
            index = Some(list.len());
        }        

        let end = buf.len();
        list.push(start..end);
    }

    let env = shell.env.borrow();
    let mut cmd = Command::new(&complete.exe);

    cmd
        .current_dir(env.pwd())
        .envs(env.map.iter().map(|(k, v)| (k.as_os_str(), v.as_os_str())))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    for arg in &complete.args {
        match arg.as_str() {
            "@index" => {
                let index = index.unwrap_or(list.len() + 1);
                cmd.arg(index.to_string());
            },
            _ => {
                cmd.arg(arg);
            }
        }
    }

    cmd.arg("--");

    let buf = buf.to_str()?;

    for span in list {
        let arg = &buf[span];
        cmd.arg(arg);
    }

    if index.is_none() {
        cmd.arg(" ");
    }

    let output = cmd.output()?;

    if !output.status.success() {
        anyhow::bail!("complete engine execute failed: {:?}", output.status);
    }

    let stdout = output.stdout.to_str()?;
    let items = stdout.lines().count();
    let has_desc = stdout.lines()
        .next()
        .is_some_and(|line| line.contains('\t'));

    shell.editor.complete_selector.list.clear();
    shell.editor.complete_selector.desc.clear();    
    shell.editor.complete_selector.list.reserve(items);
    if has_desc {
        shell.editor.complete_selector.desc.reserve(items);
    }

    let input = stdout
        .lines()
        .map(|line| line.split_once('\t').unwrap_or((line, "")))
        .map(|(item, desc)| (item.into(), desc.into()));

    shell.editor.complete_selector.list.clear();
    shell.editor.complete_selector.desc.clear();

    for (item, desc) in input {
        shell.editor.complete_selector.list.push(item);
        shell.editor.complete_selector.desc.push(desc);
    }
    
    if make_complete_selector(
        &mut shell.editor,
        select,
        renderer.size,
    ) {
        let input = shell.editor.insert.as_str();
        shell.ast = shell.parser.parse_incomplete(input).ok();
    }

    Ok(())
}

fn make_complete_selector(
    editor: &mut Editor,
    select: Range<usize>,
    size: (u16, u16),
) -> bool {
    match editor.complete_selector.list.len() {
        0 => false,
        1 => {
            let s = &editor.complete_selector.list[0];
            editor.insert.select_span(select);
            editor.insert.replace_str_inclusive(s, None);
            editor.insert.push(' ');
            true
        },
        _ => {
            editor.complete_selector.cur = 0;
            editor.complete_selector.set_space(size);
            editor.complete_selector.update();
            editor.insert.select_span(select);
            editor.mode = Mode::Complete;
            false
        }
    }
}
