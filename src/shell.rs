pub mod parser;

use crate::editor::Editor;

pub struct Shell {
    //
}

impl Shell {
    pub fn start(self) -> anyhow::Result<()> {
        let mut editor = Editor::new()?;
        
        loop {
            let event = crossterm::event::read()?;

            // TODO render

            match editor.step(event)? {
                () => ()
            }
        }
    }
}
