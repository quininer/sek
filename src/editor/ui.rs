use crossterm::{ queue, style };
use unicode_width::UnicodeWidthStr;
use crate::editor::Mode;
use crate::ui::layout::{ self, Layout };
use crate::ui::render::{ Render, Fill };
use crate::util::RefWriter;
use crate::util::arena::Id;
use crate::shell::Shell;

pub struct Editor {
    pub layout: layout::Tree,
    pub prompt: Id<layout::Node>,
    pub insert_line: Id<layout::Node>,
    pub command_line: Id<layout::Node>,
    pub tips: Id<layout::Node>
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
            axis: layout::Axis::Horizontal,
            justify: layout::Justify::Stretch,
            overflow: true,
        });

        // command line
        let command = layout.new_node(root, layout::Style {
            axis: layout::Axis::Horizontal,
            justify: layout::Justify::Start,
            ..Default::default()
        });
        let command_line = layout.new_node(command, layout::Style {
            axis: layout::Axis::Horizontal,
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

pub struct Prompt;

const PROMPT: &str = "> ";

impl Render for Prompt {
    type State = Shell;
    type Error = anyhow::Error;

    fn info(state: &Self::State, leaf_id: Id<layout::Node>) -> Option<layout::SpaceInfo> {
        assert_eq!(state.editor.ui.prompt, leaf_id);

        Some(layout::SpaceInfo {
            length: PROMPT.width(),
            cursor: None
        })
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
        assert_eq!(state.editor.ui.prompt, leaf_id);
        assert_eq!(layout.padding, 0);

        queue!(term, style::Print(PROMPT.get(..usize::from(layout.size.0)).unwrap_or_default()))?;

        assert_eq!(usize::from(current.x) + PROMPT.width(), usize::from(layout.range.end.x));
        current.x = layout.range.end.x;
        Ok(())
    }
}

pub struct InsertLine;

impl Render for InsertLine {
    type State = Shell;
    type Error = anyhow::Error;

    fn info(state: &Self::State, leaf_id: Id<layout::Node>) -> Option<layout::SpaceInfo> {
        assert_eq!(state.editor.ui.insert_line, leaf_id);

        let (s0, s1) = state.editor.line.split(state.editor.line_cursor.end);
        let s0_len = s0.width();
        let s1_len = s1.width();

        let cursor_len = matches!(state.editor.mode, Mode::Insert)
            .then_some(s0_len);

        Some(layout::SpaceInfo {
            length: s0_len + s1_len,
            cursor: cursor_len
        })
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
        use crate::shell::syntax::highlight::colour;
        
        assert_eq!(state.editor.ui.insert_line, leaf_id);

        if let Some(cmd) = state.ast {
            colour(state, cmd, state.editor.line.as_str(), term)?;
        } else {
            queue!(
                term,
                style::Print(state.editor.line.as_str()),
            )?;
        }

        *current = layout.range.end;
        
        Ok(())
    }
}

pub struct CommandLine;

impl Render for CommandLine {
    type State = Shell;
    type Error = anyhow::Error;

    fn info(state: &Self::State, leaf_id: Id<layout::Node>) -> Option<layout::SpaceInfo> {
        assert_eq!(state.editor.ui.command_line, leaf_id);

        Some(layout::SpaceInfo {
            length: state.editor.command.as_str().width(),
            cursor: None
        })
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
        assert_eq!(state.editor.ui.command_line, leaf_id);

        queue!(term, style::Print(state.editor.command.as_str()))?;
        queue!(term, style::Print(Fill(' ', layout.padding.into())))?;
        current.x = layout.range.end.x;
        Ok(())
    }
}

pub struct Tips;

impl Render for Tips {
    type State = Shell;
    type Error = anyhow::Error;

    fn info(state: &Self::State, leaf_id: Id<layout::Node>) -> Option<layout::SpaceInfo> {
        assert_eq!(state.editor.ui.tips, leaf_id);

        Some(layout::SpaceInfo {
            length: 2,
            cursor: None
        })
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
        assert_eq!(state.editor.ui.tips, leaf_id);

        queue!(term, style::Print("<>"))?;
        current.x = layout.range.end.x;
        Ok(())
    }
}
