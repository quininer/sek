pub mod env;
pub mod syntax;
pub mod execute;
pub mod complete;
pub mod prompt;

use std::cell::RefCell;
use std::path::PathBuf;
use std::io::{self, Write};
use crossterm::{ queue, style, terminal };
use crate::cache::{ self, Cache };
use crate::config::{ self, Config };
use crate::ui::layout;
use crate::ui::render::Renderer;
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
            morgue: Morgue::default(),
            prompt: Prompt::default(),
            env, editor, parser, config, cache,
        })        
    }
    
    pub async fn start(mut self) -> anyhow::Result<()> {
        let stdout = Stdout::from(io::stdout());
        let mut size = terminal::size()?;

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
            
            let event = read()?;

            // update resize
            if let crossterm::event::Event::Resize(x, y) = &event
                && renderer.size != (*x, *y)
            {
                renderer.size = (*x, *y);
                self.prompt.set_width(*x);

                if let Some(render) = error_renderer.as_mut() {
                    *render = annotate_snippets::Renderer::styled()
                        .term_width(renderer.size.1.into());
                }

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

            let mode = self.editor.mode;
            let mut action = self.editor.step(&self.env, event);
            let is_execute = matches!(action, Ok(Action::Execute));

            match action {
                Ok(Action::Break) => break,
                Ok(Action::Reload) => {
                    let mut env = self.env.borrow_mut();
                    let mut config = self.config.borrow_mut();
                    let mut cache = self.cache.borrow_mut();
                    match config::reload(&mut env, &mut config, &mut cache) {
                        Ok(()) => continue,
                        Err(err) => action = Err(err)
                    }
                },
                _ => (),
            }

            let line = self.editor.insert.as_str();
            let result = if is_execute {
                self.parser.parse(line)
            } else {
                self.parser.parse_incomplete(line)
            };
            self.ast = result.as_ref().ok().copied();

            if self.editor.insert.is_empty() || !self.editor.insert.is_editing() {
                self.editor.suggestion.clear();
            }

            if let Ok(cmd) = result {
                if matches!(action, Ok(Action::Completion)) {
                    // complete resolve
                    if let Err(err) = complete(&self, cmd).await
                        .resolve(&mut self, &mut renderer).await
                    {
                        action = Err(err);
                    }
                } else if self.editor.insert.is_editing()
                    && !self.editor.insert.is_empty()
                    && self.editor.insert.is_point_end()
                {
                    // autosuggestion
                    if let Err(err) = complete(&self, cmd).await
                        .suggest(&mut self).await
                    {
                        action = Err(err);
                    }
                }
            }

            self.editor.mode_switch(mode, &mut renderer)?;
            self.error = action.err().map(|err| err.to_string());

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
                                style::Print(&err),
                                style::Print("\r\n")
                            )?;
                            term.flush()?;
                        }
                    }

                    self.editor.insert.clear();
                    self.editor.suggestion.clear();
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

fn read() -> io::Result<crossterm::event::Event> {
    loop {
        let ev = crossterm::event::read()?;
        if !ev.is_key_release() {
            return Ok(ev);
        }
    }
}
