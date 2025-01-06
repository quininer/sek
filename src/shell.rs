pub mod parser;

use std::io;
use std::ops::ControlFlow;
use crossterm::terminal;
use crate::ui::render::Renderer;
use crate::editor::Editor;
use crate::util::ScopeGuard;


#[derive(Default)]
pub struct Shell {
    //
}

impl Shell {
    pub fn start(self) -> anyhow::Result<()> {
        let stdout = io::stdout();
        let size = terminal::size()?;
        
        let mut editor = Editor::new()?;
        let mut renderer = Renderer::new(size, || stdout.lock());

        editor.init_to(&mut renderer);

        terminal::enable_raw_mode()?;
        let _guard = ScopeGuard((), |_| {
            let _ = terminal::disable_raw_mode();
        });

        loop {
            editor.render(&mut renderer)?;
            
            let event = crossterm::event::read()?;

            match editor.step(event)? {
                ControlFlow::Continue(()) => (),
                ControlFlow::Break(()) => break
            }
        }

        Ok(())
    }
}
