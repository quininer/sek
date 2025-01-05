use std::io;
use std::collections::HashMap;
use anyhow::Context;
use unicode_width::UnicodeWidthStr;
use crossterm::{ queue, style, cursor };
use sek::ui::layout::{ self, Layout };
use sek::ui::render::{ Renderer, Render, RefWriter, Fill };
use sek::util::arena::Id;


struct ShellUi {
    prompt: Id<layout::Node>,
    insert_line: Id<layout::Node>,
    command_line: Id<layout::Node>,
    tips: Id<layout::Node>,
}

impl ShellUi {
    fn new(tree: &mut layout::Tree) -> anyhow::Result<ShellUi> {
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
            prompt,
            insert_line,
            command_line,
            tips: command_tips
        })
    }
}

struct ShellData(HashMap<Id<layout::Node>, String>);

struct Text;

impl Render for Text {
    type Data = ShellData;
    type Error = anyhow::Error;

    fn length(data: &Self::Data, leaf_id: Id<layout::Node>) -> Option<usize> {
        data.0.get(&leaf_id).map(|s| s.width())
    }

    fn render(
        data: &Self::Data,
        leaf_id: Id<layout::Node>,
        layout: &Layout,
        current: &mut layout::Point,
        mut term: RefWriter<'_>
    )
        -> Result<(), Self::Error>
    {
        let s = data.0.get(&leaf_id).context("not found node data")?;
        queue!(term, style::Print(s))?;

        if layout.padding != 0 {
            queue!(term, style::Print(Fill(' ', layout.padding.into())))?;
        }
        
        current.y = layout.range.end.y;
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    let mut tree = layout::Tree::default();
    let ui = ShellUi::new(&mut tree)?;
    let data = ShellData(
        [
            (ui.prompt, "> "),
//            (ui.insert_line, "xx"),
            (ui.insert_line, "longlonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglong"),
            (ui.command_line, ":q"),
            (ui.tips, "<>")
        ]
            .iter()
            .map(|&(id, s)| (id, s.into()))
            .collect()
    );

    let mut renderer: Renderer<ShellData, anyhow::Error>
        = Renderer::new(crossterm::terminal::size()?);

    for (id, _) in data.0.iter() {
        renderer.insert::<Text>(*id);
    }
    
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    renderer.render(&tree, &data, &mut stdout)
}
