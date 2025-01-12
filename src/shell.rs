pub mod environment;
pub mod syntax;
pub mod process;
pub mod execute;

use std::io;
use std::cell::RefCell;
use crossterm::terminal;
use crate::config::{ Config, default_theme };
use crate::ui::layout;
use crate::ui::render::Renderer;
use crate::editor::{ Editor, Action };
use crate::util::ScopeGuard;
use crate::util::stdout::Stdout;
use environment::Environment;
use process::Morgue;


pub struct Shell {
    pub config: Config,
    pub env: Environment,
    pub morgue: Morgue,
    pub editor: Editor,
    pub parser: syntax::Parser,
    pub ast: Option<syntax::Command>,
    pub tmpbuf: RefCell<Box<[u8]>>,
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
            morgue: Morgue::default(),
            tmpbuf: RefCell::new(vec![0; 1024].into_boxed_slice()),
            editor, parser, config
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
            renderer.render(&self)?;
            
            let event = crossterm::event::read()?;
            let is_execute = match self.editor.step(event)? {
                Action::Continue => false,
                Action::Execute => true,
                Action::Break => break
            };

            let line = self.editor.line.as_str();

            let result = self.parser.parse(line);
            self.ast = result.as_ref().ok().copied();

            match result {
                Ok(_) => (),
                Err(err) if is_execute => {
                    let _guard = ScopeGuard(terminal::disable_raw_mode(), |_| {
                        let _ = terminal::enable_raw_mode();
                    });


                    let display = error_renderer.render(err.to_message(line));
                    renderer.new_line(&display)?;
                    
                    // TODO render error
                },
                Err(_) => ()
            }

            if is_execute {
                renderer.new_line(&"")?;
                
                if let Some(cmd) = self.ast {
                    let _guard = ScopeGuard(terminal::disable_raw_mode(), |_| {
                        let _ = terminal::enable_raw_mode();
                    });
                    
                    execute::execute(&self, self.editor.line.as_str(), cmd)?;
                }
                
                self.ast = None;
                self.editor.line.clear(&mut self.editor.line_cursor);
            }

            self.morgue.wait()?;
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
