use std::io;
use std::collections::HashMap;
use anyhow::Context;
use unicode_width::UnicodeWidthStr;
use crossterm::{ queue, style };
use sek::ui::layout::{ self, Layout };
use sek::ui::render::{ Renderer, Render, RefWriter, Fill };
use sek::util::arena::Id;


struct ShellUi {
    tree: layout::Tree,
    prompt: Id<layout::Node>,
    insert_line: Id<layout::Node>,
    command_line: Id<layout::Node>,
    tips: Id<layout::Node>,
    state: HashMap<Id<layout::Node>, String>,
}

impl ShellUi {
    fn new() -> anyhow::Result<ShellUi> {
        let mut tree = layout::Tree::default();
        let root = tree.root();

        // insert line
        let insert = tree.new_node(root, layout::Style {
            axis: layout::Axis::Horizontal,
            justify: layout::Justify::Start,
            ..Default::default()
        });
        let prompt = tree.new_node(insert, layout::Style {
            justify: layout::Justify::Start,
            ..Default::default()
        });
        let insert_line = tree.new_node(insert, layout::Style {
            justify: layout::Justify::Stretch,
            wrap: true,
            ..Default::default()
        });
        // command line
        let command = tree.new_node(root, layout::Style {
            axis: layout::Axis::Horizontal,
            justify: layout::Justify::Start,
            ..Default::default()
        });
        let command_line = tree.new_node(command, layout::Style {
            justify: layout::Justify::Stretch,
            ..Default::default()
        });
        let command_tips =  tree.new_node(command, layout::Style {
            justify: layout::Justify::End,
            ..Default::default()
        });

        Ok(ShellUi {
            tree, prompt,
            insert_line,
            command_line,
            tips: command_tips,
            state: Default::default()
        })
    }
}

impl AsRef<layout::Tree> for ShellUi {
    fn as_ref(&self) -> &layout::Tree {
        &self.tree
    }
}

struct Text;

impl Render for Text {
    type State = ShellUi;
    type Error = anyhow::Error;

    fn length(state: &Self::State, leaf_id: Id<layout::Node>) -> Option<usize> {
        state.state.get(&leaf_id).map(|s| s.width())
    }

    fn render(
        state: &Self::State,
        leaf_id: Id<layout::Node>,
        layout: &Layout,
        current: &mut layout::Point,
        mut term: RefWriter<'_>
    )
        -> Result<(), Self::Error>
    {
        let s = state.state.get(&leaf_id).context("not found node data")?;
        queue!(term, style::Print(s))?;

        if layout.padding != 0 {
            queue!(term, style::Print(Fill(' ', layout.padding.into())))?;
        }
        
        current.y = layout.range.end.y;
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    let mut ui = ShellUi::new()?;
    ui.state = [
            (ui.prompt, "> "),
//            (ui.insert_line, "xx"),
            (ui.insert_line, "longlonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglong"),
            (ui.command_line, ":q"),
            (ui.tips, "<>")
        ]
            .iter()
            .map(|&(id, s)| (id, s.into()))
            .collect();

    let stdout = io::stdout();

    let mut renderer: Renderer<ShellUi, _, anyhow::Error>
        = Renderer::new(crossterm::terminal::size()?, || stdout.lock());

    for (id, _) in ui.state.iter() {
        renderer.insert::<Text>(*id);
    }

    renderer.render(&ui)
}
