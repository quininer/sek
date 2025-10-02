use std::convert::TryInto;

use bstr::ByteSlice;
use crossterm::{ queue, style, cursor, terminal };
use unicode_width::{ UnicodeWidthChar, UnicodeWidthStr };
use crate::{ editor, ui };
use crate::ui::layout::{ self, Layout };
use crate::util::RefWriter;
use crate::util::arena::Id;
use crate::shell::Shell;
use crate::ui::render::{ ElementImpl, Fill, LimitAndFill };

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
            layout::Style::default(),
            (
                ui::Elem(layout::Style::default(), PROMPT),
                ui::Elem(
                    layout::Style::default()
                        .justify(layout::Justify::Stretch)
                        .overflow(true),
                    INSERT_LINE,
                )
            )
        );

        let selector = ui::Box(
            layout::Style::default()
                .axis(layout::Axis::Vertical)
                .justify(layout::Justify::Stretch),
            (
                ui::Box(
                    layout::Style::default(),
                    ui::Elem(
                        layout::Style::default()
                            .justify(layout::Justify::Stretch)
                            .overflow(true),
                        PATH_LINE
                    ),
                ),
                ui::Box(
                    layout::Style::default()
                        .axis(layout::Axis::Horizontal)
                        .justify(layout::Justify::Stretch),
                    (
                        ui::Elem(
                            layout::Style::default()
                                .axis(layout::Axis::Vertical)
                                .justify(layout::Justify::Stretch),
                            PATH_SELECTOR.0,
                        ),
                        ui::Elem(
                            layout::Style::default()
                                .axis(layout::Axis::Vertical)
                                .justify(layout::Justify::Stretch),
                            PATH_SELECTOR.1,
                        ),
                        ui::Elem(
                            layout::Style::default()
                                .axis(layout::Axis::Vertical)
                                .justify(layout::Justify::Stretch),
                            PATH_SELECTOR.2
                        ),
                    )
                )
            )
        );

        let command = ui::Box(
            layout::Style::default().justify(layout::Justify::End),
            (
                ui::Elem(layout::Style::default(), MODE),
                ui::Elem(
                    layout::Style::default().justify(layout::Justify::Stretch),
                    COMMAND_LINE
                ),
                ui::Elem(
                    layout::Style::default().justify(layout::Justify::End),
                    TIPS
                )
            )
        );

        let mut table = ui::Table::default();

        (insert, selector, command)
            .walk(&mut layout, &mut table, root);

        Ok(Editor { layout, table })
    }
}

const PROMPT_STRING: &str = "> ";
const PROMPT: ElementImpl = ElementImpl {
    info: |_, _| Some(layout::SpaceInfo {
        length: PROMPT_STRING.width(),
        cursor: None
    }),
    render: |_, _, layout, current, mut term| {
        assert_eq!(layout.padding, 0);

        let prompt = PROMPT_STRING.get(..usize::from(layout.size.0)).unwrap_or_default();
        queue!(term, style::Print(prompt))?;

        assert_eq!(usize::from(current.x) + PROMPT_STRING.width(), usize::from(layout.range.end.x));
        current.x += layout.size.0;
        Ok(())
    }
};

const INSERT_LINE: ElementImpl = ElementImpl {
    info: |shell, _| {
        let (s0, s1) = shell.editor.insert.split(shell.editor.insert.cursor().end);
        let s0_len = s0.width();
        let s1_len = s1.width();

        let cursor_len = shell.editor.command.is_empty()
            .then_some(s0_len);

        Some(layout::SpaceInfo {
            length: s0_len + s1_len,
            cursor: cursor_len
        })
    },
    render: |shell, _, layout, current, mut term| {
        use crate::shell::syntax::highlight::colour;
        
        if let Some(cmd) = shell.ast {
            colour(shell, cmd, shell.editor.insert.as_str(), term)?;
        } else {
            queue!(
                term,
                style::Print(shell.editor.insert.as_str()),
            )?;
        }

        *current = layout.range.end;
        Ok(())
    }
};

const MODE: ElementImpl = ElementImpl {
    info: |shell, _| shell.editor.mode.str()
        .map(|s| layout::SpaceInfo {
            length: s.width() + 1,
            cursor: None
        }),
    render: |shell, _, layout, current, mut term| {
        if let Some(s) = shell.editor.mode.str() {
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
};

const COMMAND_LINE: ElementImpl = ElementImpl {
    info: |shell, _| {
        matches!(
            shell.editor.mode,
            editor::Mode::Normal | editor::Mode::Visual | editor::Mode::PathSelector
        )
            .then_some(())?;

        let (s0, s1) = shell.editor.command.split(shell.editor.command.cursor().end);
        let s0_len = s0.width();
        let s1_len = s1.width();

        Some(layout::SpaceInfo {
            length: s0_len + s1_len,
            cursor: shell.editor.ready.is_none()
                .then_some(s0_len)
                .filter(|_| !shell.editor.command.is_empty())
        })
    },
    render: |shell, _, layout, current, mut term| {
        if matches!(
            shell.editor.mode,
            editor::Mode::Normal | editor::Mode::Visual | editor::Mode::PathSelector
        ) {
            queue!(term,
                terminal::DisableLineWrap,
                style::SetColors(style::Colors::new(style::Color::Black, style::Color::White)),
                style::Print(shell.editor.command.as_str()),
                style::Print(Fill(' ', layout.padding.into())),
                style::ResetColor,
                terminal::EnableLineWrap
            )?
        }

        current.x += layout.size.0;
        Ok(())
    }
};

const TIPS: ElementImpl = ElementImpl {
    info: |shell, _| shell.editor.ready
        .map(|c| layout::SpaceInfo {
            length: c.width().unwrap_or_default() + 2,
            cursor: None
        }),
    render: |shell, _, layout, current, mut term| {
        if let Some(ready) = shell.editor.ready {
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
};

const PATH_LINE: ElementImpl = ElementImpl {
    info: |shell, _| {
        matches!(shell.editor.mode, editor::Mode::PathSelector)
            .then_some(())?;

        let length = shell.editor.path_selector.path()
            .as_os_str()
            .as_encoded_bytes()
            .chars()
            .filter_map(|c| c.width())
            .sum();
        Some(layout::SpaceInfo {
            length, cursor: None
        })
    },
    render: |shell, _, layout, current, mut term| {
        let path = shell.editor.path_selector.path();
        queue!(term, style::Print(path.display()))?;
        *current = layout.range.end;
        Ok(())
    }
};

const PATH_SELECTOR: (ElementImpl, ElementImpl, ElementImpl) = {
    fn info<const N: usize>(shell: &Shell, _leaf_id: Id<layout::Node>) -> Option<layout::SpaceInfo> {
        matches!(shell.editor.mode, editor::Mode::PathSelector)
            .then_some(())?;
        let length = match N {
            0 => shell.editor.path_selector.parent.len(),
            1 => shell.editor.path_selector.current.len(),
            2 => shell.editor.path_selector.children.len(),
            _ => unreachable!()
        };
        Some(layout::SpaceInfo { length, cursor: None })        
    }

    fn render<const N: usize>(
        shell: &Shell,
        _leaf_id: Id<layout::Node>,
        layout: &Layout,
        current: &mut layout::Point,
        mut term: RefWriter<'_>
    )
        -> anyhow::Result<()>
    {
        let list = match N {
            0 => &shell.editor.path_selector.parent,
            1 => &shell.editor.path_selector.current,
            2 => &shell.editor.path_selector.children,
            _ => unreachable!()
        };

        for (n, (hint, entry)) in list
            .take(layout.size.1.into())
            .enumerate()
        {
            let n: u16 = n.try_into().unwrap();
            let name = entry.name();

            queue!(term, cursor::MoveTo(layout.range.start.x, layout.range.start.y + n))?;

            if hint {
                queue!(term, style::SetColors(style::Colors::new(
                    style::Color::Black,
                    style::Color::White
                )))?;
            }
            
            queue!(term,
                style::Print(LimitAndFill(
                    name.as_encoded_bytes().chars(),
                    ' ',
                    layout.size.0.saturating_sub(1).into()
                )),
                style::ResetColor,
            )?;
        }

        queue!(term, cursor::MoveTo(layout.range.end.x, layout.range.end.y))?;

        *current = layout.range.end;
        Ok(())      
    }

    const fn imp<const N: usize>() -> ElementImpl {
        ElementImpl {
            info: info::<N>,
            render: render::<N>
        }
    }

    (imp::<0>(), imp::<1>(), imp::<2>())
};
