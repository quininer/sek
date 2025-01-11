pub mod environment;
pub mod syntax;
pub mod process;
pub mod execute;

use std::io;
use crossterm::terminal;
use crate::config::{ Config, default_theme };
use crate::ui::layout;
use crate::ui::render::Renderer;
use crate::editor::{ Editor, Action };
use crate::util::ScopeGuard;
use crate::util::stdout::Stdout;
use environment::Environment;


pub struct Shell {
    pub config: Config,
    pub env: Environment,
    pub editor: Editor,
    pub parser: syntax::Parser,
    pub ast: Option<syntax::Command>,
}

impl Shell {
    pub fn new() -> anyhow::Result<Self> {
        let mut config = Config::default();
        config.theme = default_theme();
        let editor = Editor::new()?;
        let parser = syntax::Parser::default();
        
        Ok(Shell {
            ast: None,
            env: Environment::new()?,
            editor, parser, config
        })        
    }
    
    pub fn start(mut self) -> anyhow::Result<()> {
        let stdout = Stdout::from(io::stdout());
        let size = terminal::size()?;
        
        let mut renderer = Renderer::new(size, || stdout.lock());

        init_to(&self.editor, &mut renderer);

        let _guard = ScopeGuard(terminal::enable_raw_mode(), |_| {
            let _ = terminal::disable_raw_mode();
        });

        loop {
            renderer.render(&self)?;
            
            let event = crossterm::event::read()?;
            let is_execute = match self.editor.step(event)? {
                Action::Continue => false,
                Action::Execute => true,
                Action::Break => break
            };

            let result = self.parser.parse(self.editor.line.as_str());
            self.ast = result.as_ref().ok().copied();

            match result {
                Ok(_) => (),
                Err(err) if is_execute => {
                    dbg!(err);

                    // TODO render error
                },
                Err(_) => ()
            }

            if is_execute {
                renderer.new_line()?;
                
                if let Some(cmd) = self.ast {
                    let _guard = ScopeGuard(terminal::disable_raw_mode(), |_| {
                        let _ = terminal::enable_raw_mode();
                    });
                    
                    execute::execute(&self, self.editor.line.as_str(), cmd)?;
                }
                
                self.ast = None;
                self.editor.line.clear(&mut self.editor.line_cursor);
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
