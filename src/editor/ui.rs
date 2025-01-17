use crossterm::{ queue, style, terminal };
use unicode_width::{ UnicodeWidthChar, UnicodeWidthStr };
use crate::editor;
use crate::ui::layout::{ self, Layout };
use crate::ui::render::{ Render, Fill };
use crate::util::RefWriter;
use crate::util::arena::Id;
use crate::shell::Shell;
use crate::ui::render::RenderVtable;

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
        let tips =  layout.new_node(command, layout::Style {
            justify: layout::Justify::End,
            ..Default::default()
        });

        Ok(Editor {
            layout,
            prompt,
            insert_line,
            command_line,
            tips
        })
    }
}

pub struct Prompt;

const PROMPT: &str = "> ";

impl Render for Prompt {
    type State = Shell;
    type Error = anyhow::Error;

    const VTABLE: &'static RenderVtable<Self::State, Self::Error>
        = &RenderVtable::new::<Self>();

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
        current.x += layout.size.0;
        Ok(())
    }
}

pub struct InsertLine;

impl Render for InsertLine {
    type State = Shell;
    type Error = anyhow::Error;

    const VTABLE: &'static RenderVtable<Self::State, Self::Error>
        = &RenderVtable::new::<Self>();    

    fn info(state: &Self::State, leaf_id: Id<layout::Node>) -> Option<layout::SpaceInfo> {
        assert_eq!(state.editor.ui.insert_line, leaf_id);

        let (s0, s1) = state.editor.insert.split(state.editor.insert_cursor.end);
        let s0_len = s0.width();
        let s1_len = s1.width();

        let cursor_len = state.editor.command.is_empty()
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
            colour(state, cmd, state.editor.insert.as_str(), term)?;
        } else {
            queue!(
                term,
                style::Print(state.editor.insert.as_str()),
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

    const VTABLE: &'static RenderVtable<Self::State, Self::Error>
        = &RenderVtable::new::<Self>();

    fn info(state: &Self::State, leaf_id: Id<layout::Node>) -> Option<layout::SpaceInfo> {
        assert_eq!(state.editor.ui.command_line, leaf_id);

        matches!(state.editor.mode, editor::Mode::Normal)
            .then_some(())?;

        let (s0, s1) = state.editor.command.split(state.editor.command_cursor);
        let s0_len = s0.width();
        let s1_len = s1.width();

        Some(layout::SpaceInfo {
            length: s0_len + s1_len,
            cursor: state.editor.ready.is_none()
                .then_some(s0_len)
                .filter(|_| !state.editor.command.is_empty())
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

        if matches!(state.editor.mode, editor::Mode::Normal) {
            queue!(term,
                terminal::DisableLineWrap,
                style::SetColors(style::Colors::new(style::Color::Black, style::Color::White)),
                style::Print(state.editor.command.as_str()),
                style::Print(Fill(' ', layout.padding.into())),
                style::ResetColor,
                terminal::EnableLineWrap
            )?
        }

        current.x += layout.size.0;
        Ok(())
    }
}

pub struct Tips;

impl Render for Tips {
    type State = Shell;
    type Error = anyhow::Error;

    const VTABLE: &'static RenderVtable<Self::State, Self::Error>
        = &RenderVtable::new::<Self>();    

    fn info(state: &Self::State, leaf_id: Id<layout::Node>) -> Option<layout::SpaceInfo> {
        assert_eq!(state.editor.ui.tips, leaf_id);

        state.editor.ready
            .map(|c| layout::SpaceInfo {
                length: c.width().unwrap_or_default() + 2,
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

        if let Some(ready) = state.editor.ready {
            queue!(term,
                style::SetColors(style::Colors::new(style::Color::Black, style::Color::White)),
                style::Print("<"),
                style::Print(ready),
                style::Print(">"),
                style::ResetColor,
            )?;
        }

        current.x += layout.size.0;
        Ok(())
    }
}
