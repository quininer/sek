use crate::ui::layout;
use crate::util::arena::Id;

pub struct Editor {
    layout: layout::Tree,
    prompt: Id<layout::Node>,
    insert_line: Id<layout::Node>,
    command_line: Id<layout::Node>,
    tips: Id<layout::Node>
}

impl Editor {
    pub fn new() -> anyhow::Result<Editor> {
        let mut layout = layout::Tree::default();
        let root = layout.root();

        // insert line
        let insert = layout.new_node(root, layout::Style {
            axis: layout::Axis::Horizontal,
            justify: layout::Justify::Start,
            ..Default::default()
        });
        let prompt = layout.new_node(insert, layout::Style {
            justify: layout::Justify::Start,
            ..Default::default()
        });
        let insert_line = layout.new_node(insert, layout::Style {
            justify: layout::Justify::Stretch,
            wrap: true,
            ..Default::default()
        });

        // command line
        let command = layout.new_node(root, layout::Style {
            axis: layout::Axis::Horizontal,
            justify: layout::Justify::Start,
            ..Default::default()
        });
        let command_line = layout.new_node(command, layout::Style {
            justify: layout::Justify::Stretch,
            ..Default::default()
        });
        let command_tips =  layout.new_node(command, layout::Style {
            justify: layout::Justify::End,
            ..Default::default()
        });

        Ok(Editor {
            layout,
            prompt,
            insert_line,
            command_line,
            tips: command_tips
        })
    }
}
