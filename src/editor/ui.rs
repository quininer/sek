use std::convert::TryInto;
use crossterm::{ queue, style, terminal };
use unicode_width::UnicodeWidthStr;
use crate::editor::Mode;
use crate::ui::layout::{ self, Layout };
use crate::ui::render::{ Render, RefWriter, Fill };
use crate::util::arena::Id;
use super::Editor as ShellEditor;

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
            ..Default::default()
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
    type State = ShellEditor;
    type Error = anyhow::Error;

    fn length_and_cursor(state: &Self::State, leaf_id: Id<layout::Node>) -> (Option<usize>, Option<usize>) {
        assert_eq!(state.ui.prompt, leaf_id);

        (Some(PROMPT.width()), None)
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
        assert_eq!(state.ui.prompt, leaf_id);
        assert_eq!(layout.padding, 0);

        queue!(term, style::Print(PROMPT.get(..usize::from(layout.size.0)).unwrap_or_default()))?;

        assert_eq!(usize::from(current.x) + PROMPT.width(), usize::from(layout.range.end.x));
        current.x = layout.range.end.x;
        Ok(())
    }
}

pub struct InsertLine;

impl Render for InsertLine {
    type State = ShellEditor;
    type Error = anyhow::Error;

    fn length_and_cursor(state: &Self::State, leaf_id: Id<layout::Node>) -> (Option<usize>, Option<usize>) {
        assert_eq!(state.ui.insert_line, leaf_id);

        let (s0, s1) = state.line.split(state.line_cursor.end);
        let s0_len = s0.width();
        let s1_len = s1.width();

        let cursor_len = matches!(state.mode, Mode::Insert)
            .then_some(s0_len);

        (Some(s0_len + s1_len), cursor_len)
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
        assert_eq!(state.ui.insert_line, leaf_id);

        queue!(
            term,
            style::Print(state.line.as_str()),
        )?;

        *current = layout.range.end;
        
        Ok(())
    }
}

pub struct CommandLine;

impl Render for CommandLine {
    type State = ShellEditor;
    type Error = anyhow::Error;

    fn length_and_cursor(state: &Self::State, leaf_id: Id<layout::Node>) -> (Option<usize>, Option<usize>) {
        assert_eq!(state.ui.command_line, leaf_id);

        (Some(state.command.as_str().width()), None)
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
        assert_eq!(state.ui.command_line, leaf_id);

        queue!(term, style::Print(state.command.as_str()))?;
        queue!(term, style::Print(Fill(' ', layout.padding.into())))?;
        current.x = layout.range.end.x;
        Ok(())
    }
}

pub struct Tips;

impl Render for Tips {
    type State = ShellEditor;
    type Error = anyhow::Error;

    fn length_and_cursor(state: &Self::State, leaf_id: Id<layout::Node>) -> (Option<usize>, Option<usize>) {
        assert_eq!(state.ui.tips, leaf_id);

        (Some(2), None)
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
        assert_eq!(state.ui.tips, leaf_id);

        queue!(term, style::Print("<>"))?;
        current.x = layout.range.end.x;
        Ok(())
    }
}
