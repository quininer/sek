pub mod env;
pub mod syntax;
pub mod process;
pub mod execute;

use std::path::PathBuf;
use std::io::{self, Write};
use crossterm::{ queue, style, terminal };
use crate::config::{ self, Config };
use crate::ui::layout;
use crate::ui::render::Renderer;
use crate::editor::{ Editor, Action };
use crate::util::{ ScopeGuard, FmtDebug };
use crate::util::stdout::Stdout;
use env::Environment;
use process::Morgue;


pub struct Shell {
    pub config: Config,
    pub env: Environment,
    pub morgue: Morgue,
    pub editor: Editor,
    pub parser: syntax::Parser,
    pub ast: Option<syntax::Command>,
}

impl Shell {
    pub fn new(pwd: PathBuf, config_path: PathBuf) -> anyhow::Result<Self> {
        let mut env = Environment::new(pwd)?;
        let mut config = config::load(&mut env, config_path)?;
        
        let editor = Editor::new()?;
        let parser = syntax::Parser::default();

        config.alias.shrink_to_fit();
        
        Ok(Shell {
            ast: None,
            morgue: Morgue::default(),
            env, editor, parser, config
        })        
    }
    
    pub fn start(mut self) -> anyhow::Result<()> {
        let stdout = Stdout::from(io::stdout());
        let size = terminal::size()?;

        let mut renderer = Renderer::new(size, || stdout.lock());
        let error_renderer = annotate_snippets::Renderer::styled()
            .term_width(size.1.into());

        init_to(&self.editor, &mut renderer);

        let _guard = ScopeGuard(terminal::enable_raw_mode(), |_| {
            let _ = terminal::disable_raw_mode();
        });

        loop {
            self.morgue.wait()?;
            renderer.render(&self)?;
            
            let event = crossterm::event::read()?;
            let is_execute = match self.editor.step(event)? {
                Action::Continue => false,
                Action::Execute => true,
                Action::Break => break
            };

            let line = self.editor.insert.as_str();
            let result = self.parser.parse(line);
            self.ast = result.as_ref().ok().copied();

            match result {
                Ok(_) => (),
                // syntax error
                Err(err) if is_execute => {
                    let _guard = ScopeGuard(terminal::disable_raw_mode(), |_| {
                        let _ = terminal::enable_raw_mode();
                    });

                    let display = error_renderer.render(err.to_message(line));
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
                    
                    match execute::execute(&self, self.editor.insert.as_str(), cmd) {
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
                }

                self.editor.insert.clear();
                self.editor.insert_cursor = 0..0;
            }
        }

        Ok(())
    }
}

fn init_to<W>(editor: &Editor, renderer: &mut Renderer<Shell, W, anyhow::Error>) {
    use crate::editor::ui;

    renderer.insert::<ui::Prompt>(editor.ui.prompt);
    renderer.insert::<ui::InsertLine>(editor.ui.insert_line);
    renderer.insert::<ui::CommandLine>(editor.ui.command_line);
    renderer.insert::<ui::Tips>(editor.ui.tips);
}

impl AsRef<layout::Tree> for Shell {
    fn as_ref(&self) -> &layout::Tree {
        self.editor.as_ref()
    }
}
