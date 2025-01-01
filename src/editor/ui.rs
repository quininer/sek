use crate::ui::layout;
use crate::util::arena::Id;

pub struct ShellUi {
    layout: layout::Tree,
    prompt: Id<layout::Node>,
    insert_line: Id<layout::Node>,
    command_line: Id<layout::Node>,
    tips: Id<layout::Node>
}

impl ShellUi {
    pub fn new() -> anyhow::Result<ShellUi> {
        let mut layout = layout::Tree::default();
        let root = layout.root();

        // insert line
        let insert = layout.new_node(root, layout::Style {
            axis: layout::Axis::Horizontal,
            justify: layout::Justify::Start,
            overflow: false
        });
        let prompt = layout.new_node(insert, layout::Style {
            axis: layout::Axis::Horizontal,
            justify: layout::Justify::Start,
            overflow: false
        });
        let insert_line = layout.new_node(insert, layout::Style {
            axis: layout::Axis::Horizontal,
            justify: layout::Justify::Stretch,
            overflow: true
        });
        let command = layout.new_node(root, layout::Style {
            axis: layout::Axis::Horizontal,
            justify: layout::Justify::Start,
            overflow: false
        });
        let command_line = layout.new_node(command, layout::Style {
            axis: layout::Axis::Horizontal,
            justify: layout::Justify::Stretch,
            overflow: false
        });
        let command_tips =  layout.new_node(command, layout::Style {
            axis: layout::Axis::Horizontal,
            justify: layout::Justify::End,
            overflow: false
        });

        Ok(ShellUi {
            layout,
            prompt,
            insert_line,
            command_line,
            tips: command_tips
        })
    }
}
