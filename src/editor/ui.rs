use crossterm::{ queue, style, terminal };
use unicode_width::{ UnicodeWidthChar, UnicodeWidthStr };
use crate::{ editor, ui };
use crate::ui::layout::{ self, Layout };
use crate::ui::render::{ Element, Fill };
use crate::util::RefWriter;
use crate::util::arena::Id;
use crate::shell::Shell;
use crate::ui::render::RenderVtable;

pub struct Editor {
    pub layout: layout::Tree,
    pub table: ui::Table,
}

impl Editor {
    pub fn new() -> anyhow::Result<Editor> {
        use ui::Element;
        
        let mut layout = layout::Tree::default();
        let root = layout.root();

        let insert = ui::Box(
            layout::Style::default()
                .axis(layout::Axis::Horizontal)
                .justify(layout::Justify::Start),
            (
                ui::Elem(
                    layout::Style::default()
                        .justify(layout::Justify::Start),
                    Prompt,
                ),
                ui::Elem(
                    layout::Style::default()
                        .axis(layout::Axis::Horizontal)
                        .justify(layout::Justify::Stretch)
                        .overflow(true),
                    InsertLine,
                )
            )
        );

        let command = ui::Box(
            layout::Style::default()
                .axis(layout::Axis::Horizontal)
                .justify(layout::Justify::Start),
            (
                ui::Elem(
                    layout::Style::default()
                        .axis(layout::Axis::Horizontal)
                        .justify(layout::Justify::Start),
                    Mode,
                ),
                ui::Elem(
                    layout::Style::default()
                        .axis(layout::Axis::Horizontal)
                        .justify(layout::Justify::Stretch),
                    CommandLine,
                ),
                ui::Elem(
                    layout::Style::default()
                        .justify(layout::Justify::End),
                    Tips,
                )
            )
        );

        let mut table = ui::Table::default();
        insert.walk(&mut layout, &mut table, root);
        command.walk(&mut layout, &mut table, root);

        Ok(Editor { layout, table })
    }
}

pub struct Prompt;

const PROMPT: &str = "> ";

impl Element for Prompt {
    type State = Shell;
    type Error = anyhow::Error;

    const VTABLE: &'static RenderVtable<Self::State, Self::Error>
        = &RenderVtable::new::<Self>();

    fn info(_state: &Self::State, _leaf_id: Id<layout::Node>) -> Option<layout::SpaceInfo> {
        Some(layout::SpaceInfo {
            length: PROMPT.width(),
            cursor: None
        })
    }

    fn render(
        _state: &Self::State,
        _leaf_id: Id<layout::Node>,
        layout: &Layout,
        current: &mut layout::Point,
        mut term: RefWriter<'_>
    )
        -> Result<(), Self::Error>
    {
        assert_eq!(layout.padding, 0);

        queue!(term, style::Print(PROMPT.get(..usize::from(layout.size.0)).unwrap_or_default()))?;

        assert_eq!(usize::from(current.x) + PROMPT.width(), usize::from(layout.range.end.x));
        current.x += layout.size.0;
        Ok(())
    }
}

pub struct InsertLine;

impl Element for InsertLine {
    type State = Shell;
    type Error = anyhow::Error;

    const VTABLE: &'static RenderVtable<Self::State, Self::Error>
        = &RenderVtable::new::<Self>();    

    fn info(state: &Self::State, _leaf_id: Id<layout::Node>) -> Option<layout::SpaceInfo> {
        let (s0, s1) = state.editor.insert.split(state.editor.insert.cursor().end);
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
        _leaf_id: Id<layout::Node>,
        layout: &Layout,
        current: &mut layout::Point,
        mut term: RefWriter<'_>
    )
        -> Result<(), Self::Error>
    {
        use crate::shell::syntax::highlight::colour;
        
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

pub struct Mode;

impl Element for Mode {
    type State = Shell;
    type Error = anyhow::Error;

    const VTABLE: &'static RenderVtable<Self::State, Self::Error>
        = &RenderVtable::new::<Self>();    

    fn info(state: &Self::State, _leaf_id: Id<layout::Node>) -> Option<layout::SpaceInfo> {
        state.editor.mode.str()
            .map(|s| layout::SpaceInfo {
                length: s.width() + 1,
                cursor: None
            })
    }

    fn render(
        state: &Self::State,
        _leaf_id: Id<layout::Node>,
        layout: &Layout,
        current: &mut layout::Point,
        mut term: RefWriter<'_>
    )
        -> Result<(), Self::Error>
    {
        if let Some(s) = state.editor.mode.str() {
            queue!(term,
                style::SetColors(style::Colors::new(style::Color::Black, style::Color::White)),
                style::Print(" "),
                style::Print(s),
                style::ResetColor,
            )?;
        }

        current.x += layout.size.0;
        Ok(())
    }
}

pub struct CommandLine;

impl Element for CommandLine {
    type State = Shell;
    type Error = anyhow::Error;

    const VTABLE: &'static RenderVtable<Self::State, Self::Error>
        = &RenderVtable::new::<Self>();

    fn info(state: &Self::State, _leaf_id: Id<layout::Node>) -> Option<layout::SpaceInfo> {
        matches!(state.editor.mode, editor::Mode::Normal | editor::Mode::Visual)
            .then_some(())?;

        let (s0, s1) = state.editor.command.split(state.editor.command.cursor().end);
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
        _leaf_id: Id<layout::Node>,
        layout: &Layout,
        current: &mut layout::Point,
        mut term: RefWriter<'_>
    )
        -> Result<(), Self::Error>
    {
        if matches!(state.editor.mode, editor::Mode::Normal | editor::Mode::Visual) {
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

impl Element for Tips {
    type State = Shell;
    type Error = anyhow::Error;

    const VTABLE: &'static RenderVtable<Self::State, Self::Error>
        = &RenderVtable::new::<Self>();    

    fn info(state: &Self::State, _leaf_id: Id<layout::Node>) -> Option<layout::SpaceInfo> {
        state.editor.ready
            .map(|c| layout::SpaceInfo {
                length: c.width().unwrap_or_default() + 2,
                cursor: None
            })
    }

    fn render(
        state: &Self::State,
        _leaf_id: Id<layout::Node>,
        layout: &Layout,
        current: &mut layout::Point,
        mut term: RefWriter<'_>
    )
        -> Result<(), Self::Error>
    {
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
