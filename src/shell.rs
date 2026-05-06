pub mod env;
pub mod syntax;
pub mod execute;

use std::cell::RefCell;
use std::path::PathBuf;
use std::io::{self, Write};
use crossterm::{ queue, style, terminal };
use directories::ProjectDirs;
use crate::config::{ self, Config };
use crate::ui::layout;
use crate::ui::render::Renderer;
use crate::editor::{ Editor, Action, Mode };
use crate::util::{ ScopeGuard, FmtDebug };
use crate::util::stdout::Stdout;
use env::Environment;
use execute::external::{ Morgue, Cause };


pub struct Shell {
    pub config: Config,
    pub env: RefCell<Environment>,
    pub morgue: Morgue,
    pub editor: Editor,
    pub parser: syntax::Parser,
    pub ast: Option<syntax::Command>,
}

impl Shell {
    pub fn new(_projdir: ProjectDirs, pwd: PathBuf, config_path: PathBuf)
        -> anyhow::Result<Self>
    {
        let mut env = Environment::new(pwd)?;
        let mut config = config::load(&mut env, config_path)?;
        let env = RefCell::new(env);
        
        let editor = Editor::new()?;
        let parser = syntax::Parser::default();

        config.alias.shrink_to_fit();
        
        Ok(Shell {
            ast: None,
            morgue: Morgue::default(),
            env, editor, parser, config
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

        loop {
            self.morgue.wait(Cause::Error).await?;

            renderer.render(&self.editor.ui.table, &self)?;
            
            let event = crossterm::event::read()?;

            if let crossterm::event::Event::Resize(x, y) = &event {
                renderer.size = (*x, *y);
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

            if let Ok(_cmd) = result
                && matches!(action, Ok(Action::Completion))
            {
                // TODO check completion type

                renderer.screen_reset()?;
                self.editor.command.clear();
                self.editor.path_selector.set_glob(None);
                self.editor.path_selector.set_space(renderer.size.1.into());
                match self.editor.path_selector.cd(self.env.borrow().pwd()) {
                    Ok(()) => {
                        self.editor.mode = Mode::PathSelector;
                    },
                    Err(err) => {
                        action = Err(err);

                        // TODO path-selector error
                    }
                }
            }

            match self.editor.mode {
                Mode::PathSelector
                    if self.editor.ui.layout[self.editor.ui.command].justify != layout::Justify::End
                    => self.editor.ui.layout[self.editor.ui.command].justify = layout::Justify::End,
                Mode::PathSelector => (),
                _
                    if self.editor.ui.layout[self.editor.ui.command].justify != layout::Justify::Stretch
                    => self.editor.ui.layout[self.editor.ui.command].justify = layout::Justify::Stretch,
                _ => (),
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
                        // TODO set prompt
                        Ok(_status) => (),
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
