pub mod env;
pub mod syntax;
pub mod execute;
pub mod complete;
pub mod prompt;

use std::io;
use std::pin::Pin;
use std::cell::RefCell;
use std::path::PathBuf;
use crossterm::terminal;
use crossterm::event::{ Event, EventStream };
use crate::ipc;
use crate::cache::{ self, Cache };
use crate::config::{ self, Config };
use crate::ui::layout;
use crate::ui::render::{ Renderer, TermTarget, warn };
use crate::editor::{ Editor, Action, Mode };
use crate::util::{ Select, Either, ScopeGuard };
use crate::util::stdout::Stdout;
use env::Environment;
use execute::external::Morgue;
use complete::complete;
use prompt::Prompt;


pub struct Shell {
    pub config: RefCell<Config>,
    pub env: RefCell<Environment>,
    pub cache: RefCell<Cache>,
    pub ipc: Option<RefCell<ipc::Client>>,
    pub morgue: Morgue,
    pub prompt: Prompt,
    pub editor: Editor,
    pub parser: syntax::Parser,
    pub ast: Option<syntax::Command>,
    pub error: Option<String>,
}

impl Shell {
    pub fn new(confpath: Option<PathBuf>)
        -> anyhow::Result<Self>
    {
        let term = || io::stdout().lock();

        let mut env = Environment::new()?;
        let confpath = confpath
            .unwrap_or_else(|| env.projdir.config_dir().join("config"));
        let config = config::load(&mut env, &confpath)
            .inspect_err(|err| {
                let _ = warn(&term, "config load failed", &err);
            })
            .unwrap_or_else(|_| Config::with_confpath(confpath));
        let cache = cache::load(&config, &env)
            .inspect_err(|err| {
                let _ = warn(&term, "cache load failed", &err);
            })
            .unwrap_or_default();

        let config = RefCell::new(config);
        let env = RefCell::new(env);
        let cache = RefCell::new(cache);
        
        let editor = Editor::new()?;
        let parser = syntax::Parser::default();

        Ok(Shell {
            ast: None,
            error: None,
            ipc: None,
            morgue: Morgue::default(),
            prompt: Prompt::default(),
            env, editor, parser, config, cache,
        })        
    }

    pub async fn start(mut self) -> anyhow::Result<()> {
        let _guard = ScopeGuard(terminal::enable_raw_mode(), |_| {
            let _ = terminal::disable_raw_mode();
        });

        let stdout = Stdout::from(io::stdout());

        match ipc::Client::connect(&self.env).await {
            Ok(Some(client)) => self.ipc = Some(RefCell::new(client)),
            Ok(None) => (),
            Err(err) => warn(&|| stdout.lock(), "ipc connect failed; ", &err)?
        }        

        runloop(&mut self, &stdout).await
    }
}

impl AsRef<layout::Tree> for Shell {
    fn as_ref(&self) -> &layout::Tree {
        self.editor.as_ref()
    }
}

async fn runloop(
    shell: &mut Shell,
    stdout: &Stdout,
) -> anyhow::Result<()> {
    use crate::ipc;

    let size = terminal::size()?;
    let mut reader = EventStream::new();
    let mut reader = Pin::new(&mut reader);
    let mut ipcbuf = Vec::new();

    let mut renderer = <Renderer<_>>::new(size, || stdout.lock());
    let mut error_renderer = None;

    // TODO use ipc
    shell.prompt.update(&shell.config, &shell.env, renderer.size);

    loop {
        renderer.render(&shell.editor.ui.table, shell)?;
        
        match Select::new(
            Select::new(
                input(reader.as_mut()),
                request_debounce(shell)
            ),
            ipc::read(shell.ipc.as_ref(), &mut ipcbuf)
        ).await {
            Either::Left(Either::Left(Some(event))) => {
                if !process_input(
                    shell,
                    &mut renderer,
                    &mut error_renderer,
                    &mut ipcbuf,
                    event?
                ).await? {
                    break
                }

                let mut has_error = false;

                if let Some(ipc) = shell.ipc.as_ref()
                    && let Err(err) = ipc.borrow_mut()
                        .send_update(&shell.env, &mut ipcbuf)
                        .await
                {
                    warn(&renderer.term, "ipc error; ", &err)?;
                    has_error = true;
                }

                if has_error {
                    shell.ipc = None;
                }
            },
            Either::Left(Either::Left(None)) =>
                anyhow::bail!("terminal input stop"),
            Either::Left(Either::Right(())) => {
                match ipc::request_suggest(
                    shell.ipc.as_ref(),
                    &mut ipcbuf,
                    shell.editor.insert.as_str()
                ).await {
                    Ok(Some(request_id)) => {
                        shell.editor.suggestion.set_requested(request_id);
                    },
                    Ok(None) => (),
                    Err(err) => {
                        warn(&renderer.term, "ipc error; ", &err)?;
                        shell.ipc = None;
                    }
                }
            },
            Either::Right(msg) => {
                let err = match msg {
                    Ok(msg) => process_msg(shell, msg).await.err(),
                    Err(err) => Some(err)
                };

                if let Some(err) = err {
                    warn(&renderer.term, "ipc error; ", &err)?;
                    shell.ipc = None;
                }
            }
        }
    }

    Ok(())
}

async fn request_debounce(shell: &Shell) {
    use std::time::Duration;
    
    if shell.ipc.is_some()
        && !shell.editor.insert.as_str().is_empty()
        && !shell.editor.insert.as_str().starts_with(' ')
        && shell.editor.insert.is_editing()
        && shell.editor.insert.is_point_end()
        && !shell.editor.suggestion.has_suggest()
    {
        tokio::time::sleep(Duration::from_millis(300)).await
    } else {
        std::future::pending().await
    }
}

async fn input(mut reader: Pin<&mut EventStream>) -> Option<io::Result<Event>> {
    use std::task::Poll;
    use std::future::poll_fn;
    use futures_core::Stream;

    poll_fn(|cx| loop {
        match reader.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(ev))) if ev.is_key_release() => (),
            ret => break ret
        }
    }).await
}

async fn process_input<T: TermTarget>(
    shell: &mut Shell,
    renderer: &mut Renderer<T>,
    error_renderer: &mut Option<annotate_snippets::Renderer>,
    ipcbuf: &mut Vec<u8>,
    event: Event
) -> anyhow::Result<bool> {
    if let Event::Resize(x, y) = &event
        && renderer.size != (*x, *y)
    {
        renderer.size = (*x, *y);

        if let Some(render) = error_renderer.as_mut() {
            *render = annotate_snippets::Renderer::styled()
                .term_width(renderer.size.0.into());
        }

        match shell.editor.mode {
            Mode::Path => {
                shell.editor.path_selector.set_space(renderer.size.1.into());
            },
            Mode::Complete => {
                shell.editor.complete_selector.set_space(renderer.size);
                shell.editor.complete_selector.update();
            },
            _ => ()
        }

        return Ok(true);
    }

    let mode = shell.editor.mode;
    let mut action = {
        let action = shell.editor.step(event);
        shell.editor.apply(&shell.env, action)
    };
    let is_execute = matches!(action, Ok(Action::Execute));

    if matches!(action, Ok(Action::Quit)) {
        return Ok(false);
    }

    if matches!(action, Ok(Action::Reload)) {
        let mut result = config::reload(&shell.env, &shell.config, &shell.cache);

        match ipc::Client::connect(&shell.env).await {
            Ok(Some(client)) => shell.ipc = Some(RefCell::new(client)),
            Ok(None) => (),
            Err(err) if result.is_ok() => result = Err(err),
            Err(_) => (),
        }
        
        match result {
            Ok(()) => return Ok(true),
            Err(err) => action = Err(err)
        }
    }

    if matches!(action, Ok(Action::QueryHistory)) {
        let command = shell.editor.insert.as_str();

        match ipc::request_history(shell.ipc.as_ref(), ipcbuf, command).await {
            Ok(Some(request_id)) => match wait_history(
                    shell.ipc.as_ref(),
                    |cmds| shell.editor.insert.set_history(cmds),
                    ipcbuf,
                    request_id
                ).await
            {
                Ok(Some(())) => {
                    shell.editor.insert.up();
                    shell.editor.suggestion.clear();
                },
                Ok(None) => (),
                Err(err) => {
                    action = Err(err);
                    shell.ipc = None;                        
                }
            },
            Ok(None) => return Ok(true),
            Err(err) => {
                action = Err(err);
                shell.ipc = None;
            }
        }
    }    

    let line = shell.editor.insert.as_str();
    let result = if is_execute {
        shell.parser.parse(line)
    } else {
        shell.parser.parse_incomplete(line)
    };
    shell.ast = result.as_ref().ok().copied();

    if let Ok(cmd) = result {
        if matches!(action, Ok(Action::Completion)) {
            // complete resolve
            if let Err(err) = complete(shell, cmd).await
                .resolve(shell, renderer).await
            {
                action = Err(err);
            }
        } else if shell.editor.insert.is_editing()
            && shell.editor.insert.is_point_end()
        {
            // autosuggestion
            if let Err(err) = complete(shell, cmd).await
                .suggest(shell).await
            {
                action = Err(err);
            }
        }
    }

    shell.editor.mode_switch(mode, renderer)?;
    shell.error = action.err().map(|err| err.to_string());

    match result {
        Ok(_) => (),
        Err(_) if shell.editor.insert.is_empty() => (),
        // syntax error
        Err(err) if is_execute => {
            let _guard = ScopeGuard(terminal::disable_raw_mode(), |_| {
                let _ = terminal::enable_raw_mode();
            });

            let error_renderer = error_renderer
                .get_or_insert_with(|| annotate_snippets::Renderer::styled()
                    .term_width(renderer.size.0.into())
                );

            let line = shell.editor.insert.as_str();
            let display = error_renderer.render(&[err.to_message(line)]);
            renderer.new_line(&display)?;
        },
        Err(_) => return Ok(true)
    }

    if is_execute {
        renderer.new_line(&"")?;
        
        if let Some(cmd) = shell.ast.take() {
            let input = shell.editor.insert.as_str();
            let request_id = ipc::start_execute(shell.ipc.as_ref(), ipcbuf, input)
                .await
                .ok()
                .flatten();

            let result = {
                let _guard = ScopeGuard(terminal::disable_raw_mode(), |_| {
                    let _ = terminal::enable_raw_mode();
                });

                execute::execute(shell, input, cmd).await
            };

            let code = result.as_ref().ok().map(|status| status.code()).unwrap_or(-1);
            let _ = ipc::end_execute(shell.ipc.as_ref(), ipcbuf, request_id, code).await;

            match result {
                Ok(status) => shell.prompt.set_status(status),
                Err(err) => warn(&renderer.term, "", &err)?,
            }

            shell.editor.insert.clear();
            shell.editor.suggestion.clear();
        }

        shell.prompt.update(&shell.config, &shell.env, renderer.size);
    }    

    Ok(true)
}

async fn process_msg(
    shell: &mut Shell,
    msg: ipc::ServerMessage<'_>,
) -> anyhow::Result<()> {
    match msg.data {
        ipc::ServerMessageData::PushSuggest { command } => {
            if shell.editor.suggestion.requested() == msg.request_id
                && msg.request_id.is_some()
                && let Some(value) = command.strip_prefix(shell.editor.insert.as_str())
            {
                shell.editor.suggestion.set_suffix(value);
            }
        },
        ipc::ServerMessageData::PushPrompt { .. } => {
            // TODO
        },
        _ => ()
    }

    Ok(())
}

async fn wait_history<F>(
    client: Option<&RefCell<ipc::Client>>,
    mut push_history: F,
    ipcbuf: &mut Vec<u8>,
    request_id: ipc::RequestId,
)
    -> anyhow::Result<Option<()>>
where
    F: FnMut(Vec<&str>)
{
    use tokio::signal::ctrl_c;
    
    let Some(client) = client
        else {
            return Ok(None);
        };
    let mut client = client.borrow_mut();
    loop {
        let msg = Select::new(
            client.recv_msg(&mut *ipcbuf),
            ctrl_c()
        ).await;

        let msg = match msg {
            Either::Left(msg) => msg?,
            Either::Right(Err(err)) => return Err(err.into()),
            Either::Right(Ok(())) => return Ok(None),
        };

        if msg.request_id != Some(request_id) {
            continue
        }

        if let ipc::ServerMessageData::PushHistory { commands } = msg.data {
            push_history(commands);
            break
        } else {
            anyhow::bail!("bad message data");
        }
    }

    Ok(Some(()))
}
