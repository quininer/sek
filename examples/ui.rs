use std::io;
use std::fmt::{ self, Write };
use std::collections::HashMap;
use anyhow::Context;
use unicode_width::UnicodeWidthStr;
use crossterm::{ queue, style, cursor };
use sek::ui::layout;
use sek::util::arena::Id;


struct ShellUi {
    layout: layout::Tree,
    prompt: Id<layout::Node>,
    insert_line: Id<layout::Node>,
    command_line: Id<layout::Node>,
    tips: Id<layout::Node>,
}

impl ShellUi {
    fn new() -> anyhow::Result<ShellUi> {
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
            overflow: false
        });
        // command line
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

    fn render<W: io::Write>(&self, data: &ShellData, mut term: W) -> anyhow::Result<()> {
        let size = crossterm::terminal::size()?;

        let mut output = Vec::new();
        self.layout.layout(data, (size.1, size.0), &mut output);

        output.sort_by_key(|(_, layout)| layout.range.start);

        let mut current = layout::Point {
            x: 0,
            y: 0
        };

        for (id, layout) in output {
            let s = data.0.get(&id).context("not found node data")?;

            if current != layout.range.start {
                let x = current.x.abs_diff(layout.range.start.x);
                if x != 0 {
                    if current.x > layout.range.start.x {
                        queue!(&mut term, cursor::MoveToPreviousLine(x))?;
                    } else {
                        for _ in 0..(layout.range.start.x - current.x) {
                            queue!(&mut term, style::Print("\r\n"))?;
                        }
                    }
                }

                if current.y != layout.range.start.y {
                    queue!(&mut term, cursor::MoveToColumn(layout.range.start.y))?
                }
            }

            queue!(&mut term, style::Print(s))?;

            if layout.padding != 0 {
                queue!(&mut term, style::Print(Fill(' ', layout.padding.into())))?;
            }

            current = layout.range.end;
        }

        term.flush()?;

        Ok(())        
    }
}

struct ShellData(HashMap<Id<layout::Node>, String>);

impl layout::Space for ShellData {
    fn length(&self, leaf: Id<layout::Node>) -> Option<usize> {
        self.0.get(&leaf).map(|s| s.width())
    }
}

struct Fill(char, usize);

impl fmt::Display for Fill {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for _ in 0..self.1 {
            f.write_char(self.0)?;
        }

        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    let ui = ShellUi::new()?;
    let data = ShellData(
        [
            (ui.prompt, "> "),
            (ui.insert_line, "longlonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglong"),
            (ui.command_line, ":q"),
            (ui.tips, "0")
        ]
            .iter()
            .map(|&(id, s)| (id, s.into()))
            .collect()
    );
    
    let stdout = io::stdout();
    let stdout = stdout.lock();

    ui.render(&data, stdout)    
}
