pub mod syntax;

use std::io;
use crossterm::terminal;
use crate::config::Config;
use crate::ui::render::Renderer;
use crate::editor::{ Editor, Action };
use crate::util::ScopeGuard;
use crate::util::stdout::Stdout;


pub struct Shell {
    config: Config,
    editor: Editor,
    parser: syntax::Parser
}

impl Shell {
    pub fn new() -> anyhow::Result<Self> {
        let editor = Editor::new()?;
        let parser = syntax::Parser::default();
        
        Ok(Shell {
            config: Config::default(),
            editor, parser
        })        
    }
    
    pub fn start(mut self) -> anyhow::Result<()> {
        let stdout = Stdout::from(io::stdout());
        let size = terminal::size()?;
        
        let mut renderer = Renderer::new(size, || stdout.lock());

        self.editor.init_to(&mut renderer);

        let _guard = ScopeGuard(terminal::enable_raw_mode(), |_| {
            let _ = terminal::disable_raw_mode();
        });

        loop {
            self.editor.render(&mut renderer)?;
            
            let event = crossterm::event::read()?;

            match self.editor.step(event)? {
                Action::Continue => continue,
                Action::Execute => (),
                Action::Break => break
            }

            let _guard = ScopeGuard(terminal::disable_raw_mode(), |_| {
                let _ = terminal::enable_raw_mode();
            });

            renderer.new_line()?;

            match self.parser.parse(self.editor.line.as_str()) {
                Ok(_root) => (),
                Err(err) => {
                    dbg!(err);
                }
            }
            
            self.editor.line.clear(&mut self.editor.line_cursor);
        }

        Ok(())
    }
}
