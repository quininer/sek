pub mod syntax;

use std::io;
use crossterm::terminal;
use crate::ui::render::Renderer;
use crate::editor::{ Editor, Action };
use crate::util::ScopeGuard;
use crate::util::stdout::Stdout;


#[derive(Default)]
pub struct Shell {
    //
}

impl Shell {
    pub fn start(self) -> anyhow::Result<()> {
        let stdout = Stdout::from(io::stdout());
        let size = terminal::size()?;
        
        let mut editor = Editor::new()?;
        let mut parser = syntax::Parser::default();
        let mut renderer = Renderer::new(size, || stdout.lock());

        editor.init_to(&mut renderer);

        let _guard = ScopeGuard(terminal::enable_raw_mode(), |_| {
            let _ = terminal::disable_raw_mode();
        });

        loop {
            editor.render(&mut renderer)?;
            
            let event = crossterm::event::read()?;

            match editor.step(event)? {
                Action::Continue => continue,
                Action::Execute => (),
                Action::Break => break
            }

            let _guard = ScopeGuard(terminal::disable_raw_mode(), |_| {
                let _ = terminal::enable_raw_mode();
            });

            renderer.new_line()?;

            match parser.parse(editor.line.as_str()) {
                Ok(_root) => (),
                Err(err) => {
                    dbg!(err);
                }
            }
            
            editor.line.clear(&mut editor.line_cursor);
        }

        Ok(())
    }
}
