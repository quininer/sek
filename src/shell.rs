pub mod env;
pub mod syntax;
pub mod execute;
pub mod complete;
pub mod prompt;

use std::cell::RefCell;
use std::path::PathBuf;
use std::io::{self, Write};
use crossterm::{ queue, style, terminal };
use directories::ProjectDirs;
use crate::cache::{ self, Cache };
use crate::config::{ self, Config };
use crate::ui::layout;
use crate::ui::render::Renderer;
use crate::editor::{ Editor, Action, Mode };
use crate::util::{ ScopeGuard, FmtDebug };
use crate::util::stdout::Stdout;
use env::Environment;
use execute::external::{ Morgue, Cause };
use complete::complete;
use prompt::Prompt;


pub struct Shell {
    pub config: Config,
    pub env: RefCell<Environment>,
    pub cache: RefCell<Cache>,
    pub morgue: Morgue,
    pub prompt: Prompt,
    pub editor: Editor,
    pub parser: syntax::Parser,
    pub ast: Option<syntax::Command>,
}

impl Shell {
    pub fn new(projdir: ProjectDirs, pwd: PathBuf, config_path: PathBuf)
        -> anyhow::Result<Self>
    {
        let mut env = Environment::new(pwd)?;
        let config = config::load(&mut env, config_path)?;
        let cache = cache::load(&config, &env, projdir.cache_dir())?;
        let env = RefCell::new(env);
        let cache = RefCell::new(cache);
        
        let editor = Editor::new()?;
        let parser = syntax::Parser::default();

        Ok(Shell {
            ast: None,
            morgue: Morgue::default(),
            prompt: Prompt::default(),
            env, editor, parser, config, cache,
        })        
    }
    
    pub async fn start(mut self) -> anyhow::Result<()> {
        let stdout = Stdout::from(io::stdout());
        let mut size = terminal::size()?;

        #[cfg(unix)] unsafe {
            let mut act: libc::sigaction = std::mem::zeroed();
            act.sa_flags = 0;
            libc::sigemptyset(&mut act.sa_mask);

            // ignore
            act.sa_sigaction = libc::SIG_IGN;

            let nullptr = std::ptr::null_mut();
            libc::sigaction(libc::SIGTSTP, &act, nullptr);
            libc::sigaction(libc::SIGTTOU, &act, nullptr);
        }

        let mut renderer = <Renderer<_>>::new(size, || stdout.lock());
        let mut error_renderer = None;

        let _guard = ScopeGuard(terminal::enable_raw_mode(), |_| {
            let _ = terminal::disable_raw_mode();
        });

        self.prompt.set_width(renderer.size.0);
        self.prompt.update(&self.config, &self.env);

        loop {
            self.morgue.wait(Cause::Error).await?;

            renderer.render(&self.editor.ui.table, &self)?;
            
            let event = crossterm::event::read()?;

            if let crossterm::event::Event::Resize(x, y) = &event
                && renderer.size != (*x, *y)
            {
                renderer.size = (*x, *y);
                self.prompt.set_width(*x);

                match self.editor.mode {
                    Mode::PathSelector => {
                        self.editor.path_selector.set_space(renderer.size.1.into());
                    },
                    Mode::CompleteSelector => {
                        self.editor.complete_selector.set_space(renderer.size);
                        self.editor.complete_selector.update();
                    },
                    _ => ()
                }
            }

            // TODO render error
            let mut action = self.editor.step(&self.env.borrow(), event);
            let is_execute = matches!(action, Ok(Action::Execute));

            if matches!(action, Ok(Action::Break)) {
                break
            }

            let line = self.editor.insert.as_str();
            let result = if is_execute {
                self.parser.parse(line)
            } else {
                self.parser.parse_incomplete(line)
            };
            self.ast = result.as_ref().ok().copied();

            if let Ok(cmd) = result
                && matches!(action, Ok(Action::Completion))
            {
                let ty = complete(&self, cmd).await;
                ty.resolve(&mut self, &mut renderer, &mut action).await?;
            }

            match self.editor.mode {
                Mode::PathSelector => {
                    if self.editor.ui.layout[self.editor.ui.command].justify != layout::Justify::End {
                        self.editor.ui.layout[self.editor.ui.command].justify = layout::Justify::End;
                    }

                    if self.editor.ui.layout[self.editor.ui.path_selector].hidden {
                        self.editor.ui.layout[self.editor.ui.path_selector].hidden = false;
                    }
                }
                Mode::CompleteSelector => {
                    if self.editor.ui.layout[self.editor.ui.command].justify != layout::Justify::Start {
                        self.editor.ui.layout[self.editor.ui.command].justify = layout::Justify::Start;
                    }
                    
                    if self.editor.ui.layout[self.editor.ui.complete_selector].hidden {
                        self.editor.ui.layout[self.editor.ui.complete_selector].hidden = false;
                    }
                }
                _ => {
                    if self.editor.ui.layout[self.editor.ui.command].justify != layout::Justify::Start {
                        self.editor.ui.layout[self.editor.ui.command].justify = layout::Justify::Start;
                    }

                    if !self.editor.ui.layout[self.editor.ui.path_selector].hidden {
                        self.editor.ui.layout[self.editor.ui.path_selector].hidden = true;
                    }

                    if !self.editor.ui.layout[self.editor.ui.complete_selector].hidden {
                        self.editor.ui.layout[self.editor.ui.complete_selector].hidden = true;
                    }
                }
            }

            match result {
                Ok(_) => (),
                // syntax error
                Err(err) if is_execute => {
                    let _guard = ScopeGuard(terminal::disable_raw_mode(), |_| {
                        let _ = terminal::enable_raw_mode();
                    });

                    if size != renderer.size {
                        error_renderer = None;
                        size = renderer.size;
                    }

                    let error_renderer = error_renderer
                        .get_or_insert_with(|| annotate_snippets::Renderer::styled()
                            .term_width(renderer.size.1.into())
                        );

                    let line = self.editor.insert.as_str();
                    let display = error_renderer.render(&[err.to_message(line)]);
                    renderer.new_line(&display)?;
                },
                Err(_) => continue
            }

            if is_execute {
                renderer.new_line(&"")?;
                
                if let Some(cmd) = self.ast.take() {
                    let _guard = ScopeGuard(terminal::disable_raw_mode(), |_| {
                        let _ = terminal::enable_raw_mode();
                    });
                    
                    match execute::execute(&self, self.editor.insert.as_str(), cmd).await {
                        Ok(status) => self.prompt.set_status(status),
                        Err(err) => {
                            let mut term = (renderer.term)();
                            queue!(
                                term,
                                style::Print(concat!(env!("CARGO_PKG_NAME"), ": ")),
                                style::Print(FmtDebug(&err)),
                                style::Print("\r\n")
                            )?;
                            term.flush()?;
                        }
                    }

                    self.editor.insert.clear();
                }

                self.prompt.update(&self.config, &self.env);                
            }
        }

        Ok(())
    }
}

impl AsRef<layout::Tree> for Shell {
    fn as_ref(&self) -> &layout::Tree {
        self.editor.as_ref()
    }
}
