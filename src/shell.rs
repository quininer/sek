pub mod env;
pub mod syntax;
pub mod execute;
pub mod complete;
pub mod prompt;

use std::io;
use std::cell::RefCell;
use std::path::PathBuf;
use crossterm::terminal;
use crossterm::event::Event;
use crate::ipc;
use crate::cache::{ self, Cache };
use crate::config::{ self, Config };
use crate::ui::layout;
use crate::ui::render::{ Renderer, TermTarget, warn };
use crate::editor::{ Editor, Action, Mode };
use crate::util::ScopeGuard;
use crate::util::stdout::Stdout;
use env::Environment;
use execute::external::{ Morgue, Cause };
use complete::complete;
use prompt::Prompt;


pub struct Shell {
    pub config: RefCell<Config>,
    pub env: RefCell<Environment>,
    pub cache: RefCell<Cache>,
    pub ipc: Option<ipc::Client>,
    pub morgue: Morgue,
    pub prompt: Prompt,
    pub editor: Editor,
    pub parser: syntax::Parser,
    pub ast: Option<syntax::Command>,
    pub error: Option<String>,
}

impl Shell {
    pub fn new(config_path: Option<PathBuf>)
        -> anyhow::Result<Self>
    {
        let mut env = Environment::new()?;
        let config = config::load(&mut env, config_path)?;
        let cache = cache::load(&config, &env)?;
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
            Ok(Some(client)) => self.ipc = Some(client),
            Ok(None) => (),
            Err(err) => warn(&|| stdout.lock(), ": ipc connect failed; ", &err)?
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
    use std::pin::Pin;
    use std::task::Poll;
    use std::future::poll_fn;
    use futures_core::Stream;
    use crossterm::event::EventStream;
    use crate::util::{ Select, Either };
    use crate::ipc;

    let size = terminal::size()?;
    let mut reader = EventStream::new();
    let mut reader = Pin::new(&mut reader);
    let mut ipcbuf = Vec::new();

    let mut renderer = <Renderer<_>>::new(size, || stdout.lock());
    let mut error_renderer = None;

    // TODO use ipc
    shell.prompt.set_width(renderer.size.0);
    shell.prompt.update(&shell.config, &shell.env);

    loop {
        shell.morgue.wait(Cause::Error).await?;
        renderer.render(&shell.editor.ui.table, shell)?;
        
        match Select::new(
            poll_fn(|cx| loop {
                match reader.as_mut().poll_next(cx) {
                    Poll::Ready(Some(Ok(ev))) if ev.is_key_release() => (),
                    ret => break ret
                }
            }),
            ipc::readable(shell.ipc.as_ref())
        ).await {
            Either::Left(Some(event)) => {
                if !process_input(
                    shell,
                    &mut renderer,
                    &mut error_renderer,
                    event?
                ).await? {
                    break
                }
            },
            Either::Left(None) =>
                anyhow::bail!("terminal input stop"),
            Either::Right(ready) => {
                let mut err = ready.err();

                if err.is_none() {
                    err = process_msg(shell, &mut ipcbuf).await.err();
                }
                
                if let Some(err) = err {
                    warn(&renderer.term, ": ipc error; ", &err)?;
                    shell.ipc = None;
                }
            }
        }
    }

    Ok(())
}

async fn process_input<T: TermTarget>(
    shell: &mut Shell,
    renderer: &mut Renderer<T>,
    error_renderer: &mut Option<annotate_snippets::Renderer>,
    event: Event
) -> anyhow::Result<bool> {
    if let Event::Resize(x, y) = &event
        && renderer.size != (*x, *y)
    {
        renderer.size = (*x, *y);
        shell.prompt.set_width(*x);

        if let Some(render) = error_renderer.as_mut() {
            *render = annotate_snippets::Renderer::styled()
                .term_width(renderer.size.0.into());
        }

        match shell.editor.mode {
            Mode::PathSelector => {
                shell.editor.path_selector.set_space(renderer.size.1.into());
            },
            Mode::CompleteSelector => {
                shell.editor.complete_selector.set_space(renderer.size);
                shell.editor.complete_selector.update();
            },
            _ => ()
        }        
    }

    let mode = shell.editor.mode;
    let mut action = shell.editor.step(&shell.env, event);
    let is_execute = matches!(action, Ok(Action::Execute));

    match action {
        Ok(Action::Break) => return Ok(false),
        Ok(Action::Reload) => {
            match config::reload(&shell.env, &shell.config, &shell.cache) {
                Ok(()) => return Ok(true),
                Err(err) => action = Err(err)
            }
        },
        _ => (),
    }

    let line = shell.editor.insert.as_str();
    let result = if is_execute {
        shell.parser.parse(line)
    } else {
        shell.parser.parse_incomplete(line)
    };
    shell.ast = result.as_ref().ok().copied();

    if shell.editor.insert.is_empty() || !shell.editor.insert.is_editing() {
        shell.editor.suggestion.clear();
    }

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
            let _guard = ScopeGuard(terminal::disable_raw_mode(), |_| {
                let _ = terminal::enable_raw_mode();
            });
            
            match execute::execute(shell, shell.editor.insert.as_str(), cmd).await {
                Ok(status) => shell.prompt.set_status(status),
                Err(err) => warn(&renderer.term, "", &err)?,
            }

            shell.editor.insert.clear();
            shell.editor.suggestion.clear();
        }

        shell.prompt.update(&shell.config, &shell.env);
    }    

    Ok(true)
}

async fn process_msg(shell: &mut Shell, ipcbuf: &mut Vec<u8>) -> anyhow::Result<()> {
    let Some(ipc) = shell.ipc.as_mut()
        else {
            return Ok(());
        };
    
    let _msg = ipc.recv_msg(ipcbuf).await?;

    // TODO impl

    Ok(())
}
