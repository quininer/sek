use bstr::ByteSlice;
use crossterm::{ queue, style, cursor, terminal };
use unicode_width::{ UnicodeWidthChar, UnicodeWidthStr };
use anstream::adapter::strip_str;
use crate::{ editor, ui };
use crate::ui::layout::{ self, Layout };
use crate::util::RefWriter;
use crate::util::arena::Id;
use crate::shell::Shell;
use crate::ui::render::{ ElementImpl, Fill, LimitAndFill };

pub struct Editor {
    pub layout: layout::Tree,
    pub table: ui::Table,

    pub complete_selector: Id<layout::Node>,
    pub path_selector: Id<layout::Node>,
    pub command: Id<layout::Node>,
}

impl Editor {
    pub fn new() -> anyhow::Result<Editor> {
        use ui::Element;

        const TAG_COMMAND: u32 = 1;
        const TAG_PATH_SELECTOR: u32 = 2;
        const TAG_COMPLETE_SELECTOR: u32 = 3;
        
        let mut layout = layout::Tree::default();
        let root = layout.root();

        let insert = ui::Box(
            None, layout::Style::default(),
            (
                ui::Elem(None, layout::Style::default(), PROMPT),
                ui::Elem(
                    None,
                    layout::Style::default()
                        .justify(layout::Justify::Stretch)
                        .overflow(true),
                    INSERT_LINE,
                )
            )
        );

        let complete_values = ui::Box(
            Some(TAG_COMPLETE_SELECTOR),
            layout::Style::default()
                .axis(layout::Axis::Vertical)
                .justify(layout::Justify::Start),
            ui::Elem(
                None,
                layout::Style::default()
                    .axis(layout::Axis::Vertical)
                    .justify(layout::Justify::Start),
                COMPLETE_SELECTOR
            )
        );

        let path_selector = ui::Box(
            Some(TAG_PATH_SELECTOR),
            layout::Style::default()
                .axis(layout::Axis::Vertical)
                .justify(layout::Justify::Stretch),
            (
                ui::Box(
                    None, layout::Style::default(),
                    ui::Elem(
                        None,
                        layout::Style::default()
                            .justify(layout::Justify::Stretch)
                            .overflow(true),
                        PATH_LINE
                    ),
                ),
                ui::Box(
                    None,
                    layout::Style::default()
                        .axis(layout::Axis::Horizontal)
                        .justify(layout::Justify::Stretch),
                    (
                        ui::Elem(
                            None,
                            layout::Style::default()
                                .axis(layout::Axis::Vertical)
                                .justify(layout::Justify::Stretch),
                            PATH_SELECTOR.0,
                        ),
                        ui::Elem(
                            None,
                            layout::Style::default()
                                .axis(layout::Axis::Vertical)
                                .justify(layout::Justify::Stretch),
                            PATH_SELECTOR.1,
                        ),
                        ui::Elem(
                            None,
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
            Some(TAG_COMMAND),
            layout::Style::default().justify(layout::Justify::Stretch),
            (
                ui::Elem(None,layout::Style::default(), MODE),
                ui::Elem(
                    None,
                    layout::Style::default().justify(layout::Justify::Stretch),
                    COMMAND_LINE
                ),
                ui::Elem(
                    None,
                    layout::Style::default().justify(layout::Justify::End),
                    TIPS
                )
            )
        );

        let mut table = ui::Table::default();
        let mut map = Vec::new();

        (insert, complete_values, path_selector, command)
            .walk(&mut layout, &mut table, &mut map, root);

        let mut editor = Editor {
            layout, table,
            complete_selector: Id::default(),
            path_selector: Id::default(),
            command: Id::default(),
        };

        for (tag, id) in map {
            match tag {
                TAG_COMPLETE_SELECTOR => editor.complete_selector = id,
                TAG_PATH_SELECTOR => editor.path_selector = id,
                TAG_COMMAND => editor.command = id,
                _ => unreachable!()
            }
        }

        Ok(editor)
    }
}

const PROMPT: ElementImpl = ElementImpl {
    info: |shell, _| Some(layout::SpaceInfo {
        length: strip_str(shell.prompt.as_str()).map(|s| s.width()).sum(),
        cursor: None
    }),
    render: |shell, _, layout, current, mut term| {
        debug_assert_eq!(layout.padding, 0);

        queue!(term, style::Print(shell.prompt.as_str()))?;

        debug_assert_eq!(
            usize::from(current.x) + strip_str(shell.prompt.as_str()).map(|s| s.width()).sum::<usize>(),
            usize::from(layout.range.end.x)
        );

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
            editor::Mode::Normal
            | editor::Mode::Visual
            | editor::Mode::PathSelector
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
            editor::Mode::Normal
            | editor::Mode::Visual
            | editor::Mode::PathSelector
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
        use crate::editor::path_selector::EntryType;
    
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

            let color = match entry.type_() {
                EntryType::Dir => shell.config.theme.variable.color(),
                EntryType::File => Some(style::Color::White),
                EntryType::Other => shell.config.theme.single_str.color()
            };

            let color = if hint {
                style::Colors::new(
                    style::Color::Black,
                    color.unwrap_or(style::Color::White)
                )
            } else {
                style::Colors::new(
                    color.unwrap_or(style::Color::White),
                    style::Color::Reset,
                )
            };

            queue!(term,
                cursor::MoveTo(layout.range.start.x, layout.range.start.y + n),
                style::SetColors(color),
                style::Print(LimitAndFill(
                    name.as_encoded_bytes().chars(),
                    Some(' '),
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

const COMPLETE_SELECTOR: ElementImpl = ElementImpl {
    info: |shell, _| {
        matches!(shell.editor.mode, editor::Mode::CompleteSelector)
            .then_some(())?;

        Some(layout::SpaceInfo {
            length: shell.editor.complete_selector.window.len(),
            cursor: None
        })
    },
    render: |shell, _, layout, current, mut term| {
        let selector = &shell.editor.complete_selector;

        for (row, chunk) in selector.list
            .chunks(selector.column)
            .enumerate()
            .skip(selector.window.start)
            .take(selector.window.len())
        {
            for (column, comp) in chunk.iter().enumerate() {
                let idx = (row * selector.column) + column;
                let hint = selector.cur == idx;
                let desc = selector.desc.get(idx).map(String::as_str).unwrap_or_default();
                let comp_width = comp.width();
                let desc_width = desc.width();

                let color = if hint {
                    style::Colors::new(style::Color::Black, style::Color::White)
                } else {
                    style::Colors::new(style::Color::White, style::Color::Reset)
                };

                // 'comp (desc)'
                #[allow(clippy::obfuscated_if_else)]
                let prepad = (desc_width != 0).then_some(3).unwrap_or_default();
                let comp_limit = if comp_width + prepad + 1 > selector.width {
                    // 'com… '
                    Some(selector.width.saturating_sub(prepad + 2))
                } else {
                    None
                };
                let rem = selector
                    .width
                    .saturating_sub(comp_limit.unwrap_or(comp_width) + 1);
                let (pad, desc_limit) = rem.checked_sub(desc_width)
                    .map(|pad| (pad, None))
                    .unwrap_or((0, Some(rem)));

                queue!(term, style::SetColors(color))?;

                if let Some(limit) = comp_limit {
                    queue!(term,
                        style::Print(LimitAndFill(comp.chars(), None, limit)),
                        style::Print("…"),
                    )?;
                } else {
                    queue!(term, style::Print(comp), style::Print(Fill(' ', pad)))?;
                }

                if let Some(limit) = desc_limit {
                    queue!(term,
                        style::Print('('),
                        style::Print(LimitAndFill(desc.chars(), None, limit)),
                        style::Print('…'),
                        style::Print(')'),
                    )?;
                } else if !desc.is_empty() {
                    queue!(term, style::Print('('), style::Print(desc), style::Print(')'))?
                }

                queue!(term, style::ResetColor, style::Print(' '))?;
            }

            queue!(term, style::Print("\r\n"))?;
        }

        debug_assert_eq!(
            current.y + selector.window.len() as u16,
            layout.range.end.y
        );

        *current = layout.range.end;
        
        Ok(())        
    }
};
