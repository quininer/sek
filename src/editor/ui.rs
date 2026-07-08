use bstr::ByteSlice;
use crossterm::{ queue, style, cursor, terminal };
use unicode_width::{ UnicodeWidthChar, UnicodeWidthStr };
use anstream::adapter::strip_str;
use crate::{ editor, ui };
use crate::ui::layout::{ self, Layout, Axis, Justify };
use crate::util::RefWriter;
use crate::util::arena::Id;
use crate::shell::Shell;
use crate::ui::render::{ ElementImpl, Fill, LimitAndFill };

pub struct Editor {
    pub layout: layout::Tree,
    pub table: ui::Table,

    pub complete_selector: Id<layout::Node>,
    pub path_selector: Id<layout::Node>,
    pub tail: Id<layout::Node>,
}

impl Default for Editor {
    fn default() -> Self {
        use ui::Element;

        const TAG_PATH_SELECTOR: u32 = 1;
        const TAG_COMPLETE_SELECTOR: u32 = 2;
        const TAG_TAIL: u32 = 3;
        
        let mut layout = layout::Tree::default();
        let root = layout.root();

        let insert = ui::Box::new((
            ui::Elem::new(PROMPT),
            ui::Elem::new(INSERT_LINE)
                .style(|style| style
                    .justify(Justify::Stretch)
                    .overflow(true)
                )
        ));

        let complete_selector = ui::Box::new(
            ui::Elem::new(COMPLETE_SELECTOR)
                .style(|style| style.axis(Axis::Vertical))
        )
            .tag(TAG_COMPLETE_SELECTOR)
            .style(|style| style
                .axis(Axis::Vertical)
                .hidden(true)
            );

        let path_selector = ui::Box::new((
            ui::Box::new(ui::Elem::new(PATH_LINE)
                .style(|style| style
                    .justify(Justify::Stretch)
                    .overflow(true)
                )
            ),
            ui::Box::new((
                ui::Elem::new(PATH_SELECTOR.0)
                    .style(|style| style
                        .axis(Axis::Vertical)
                        .justify(Justify::Stretch)
                    ),
                ui::Elem::new(PATH_SELECTOR.1)
                    .style(|style| style
                        .axis(Axis::Vertical)
                        .justify(Justify::Stretch)
                    ),
                ui::Elem::new(PATH_SELECTOR.2)
                    .style(|style| style
                        .axis(Axis::Vertical)
                        .justify(Justify::Stretch)
                    ),
            ))
                .style(|style| style.justify(Justify::Stretch)),
        ))
            .tag(TAG_PATH_SELECTOR)
            .style(|style| style
                .axis(Axis::Vertical)
                .justify(Justify::Stretch)
                .hidden(true)
            );

        let command = ui::Box::new((
            ui::Elem::new(MODE),
            ui::Elem::new(COMMAND_LINE).style(|style| style.justify(Justify::Stretch)),
            ui::Elem::new(TIPS).style(|style| style.justify(Justify::End)),
        ));

        let tail = ui::Box::new((
            command,
            ui::Box::new(ui::Elem::new(ERROR))
        ))
            .tag(TAG_TAIL)
            .style(|style| style.axis(Axis::Vertical));

        let mut table = ui::Table::default();
        let mut map = Vec::new();

        (insert, complete_selector, path_selector, tail)
            .walk(&mut layout, &mut table, &mut map, root);

        let mut editor = Editor {
            layout, table,
            complete_selector: Id::default(),
            path_selector: Id::default(),
            tail: Id::default(),
        };

        for (tag, id) in map {
            match tag {
                TAG_COMPLETE_SELECTOR => editor.complete_selector = id,
                TAG_PATH_SELECTOR => editor.path_selector = id,
                TAG_TAIL => editor.tail = id,
                _ => unreachable!()
            }
        }

        editor
    }
}

const PROMPT: ElementImpl = ElementImpl {
    info: |shell| Some(layout::SpaceInfo {
        length: strip_str(shell.prompt.as_str()).map(|s| s.width()).sum(),
        cursor: None
    }),
    render: |shell, layout, current, mut term| {
        debug_assert_eq!(layout.padding, 0);

        // FIXME limit width ?
        queue!(term, style::Print(shell.prompt.as_str()))?;

        debug_assert!(
            usize::from(current.x) + strip_str(shell.prompt.as_str()).map(|s| s.width()).sum::<usize>()
            <=
            usize::from(layout.range.end.x)
        );

        current.x += layout.size.0;
        Ok(())
    }
};

const INSERT_LINE: ElementImpl = ElementImpl {
    info: |shell| {
        let (s0, s1) = shell.editor.insert.split(shell.editor.insert.cursor().end);
        let s0_len = s0.width();
        let s1_len = s1.width();
        let s2_len = shell.editor.suggestion.as_str(&shell.editor.insert).width();

        let cursor_len = shell.editor.command.is_empty()
            .then_some(s0_len);

        Some(layout::SpaceInfo {
            length: s0_len + s1_len + s2_len,
            cursor: cursor_len
        })
    },
    render: |shell, layout, current, mut term| {
        use crate::shell::syntax::highlight::colour;
        
        if let Some(cmd) = shell.ast {
            colour(shell, cmd, shell.editor.insert.as_str(), term.reborrow())?;
        } else {
            queue!(term,
                style::Print(shell.editor.insert.as_str()),
            )?;
        }

        let suggest = shell.editor.suggestion.as_str(&shell.editor.insert);
        if !suggest.is_empty() {
            let config = shell.config.borrow();
            let color = config.theme.suggest.color().unwrap_or(style::Color::DarkGrey);
            queue!(term,
                style::SetForegroundColor(color),
                style::Print(suggest),
                style::ResetColor,
            )?;
        }

        *current = layout.range.end;
        Ok(())
    }
};

const MODE: ElementImpl = ElementImpl {
    info: |shell| shell.editor.mode.str()
        .map(|s| layout::SpaceInfo {
            length: s.width() + 1,
            cursor: None
        }),
    render: |shell, layout, current, mut term| {
        if let Some(s) = shell.editor.mode.str() {
            queue!(term,
                style::SetColors(
                    style::Colors::new(style::Color::Black, style::Color::White)
                ),
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
    info: |shell| {
        match shell.editor.mode {
            editor::Mode::Normal
            | editor::Mode::Visual
            | editor::Mode::PathSelector => (),
            editor::Mode::CompleteSelector if !shell.editor.command.is_empty() => (),
            _ => return None
        }

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
    render: |shell, layout, current, mut term| {
        let hint = match shell.editor.mode {
            editor::Mode::Normal
            | editor::Mode::Visual
            | editor::Mode::PathSelector => true,
            editor::Mode::CompleteSelector if !shell.editor.command.is_empty() => true,
            _ => false
        };
        
        if hint {
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
    info: |shell| shell.editor.ready
        .map(|c| layout::SpaceInfo {
            length: c.width().unwrap_or_default() + 2,
            cursor: None
        }),
    render: |shell, layout, current, mut term| {
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
    info: |shell| {
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
    render: |shell, layout, current, mut term| {
        let path = shell.editor.path_selector.path();
        queue!(term, style::Print(path.display()))?;
        *current = layout.range.end;
        Ok(())
    }
};

const PATH_SELECTOR: (ElementImpl, ElementImpl, ElementImpl) = {
    fn info(shell: &Shell, n: u8) -> Option<layout::SpaceInfo> {
        matches!(shell.editor.mode, editor::Mode::PathSelector)
            .then_some(())?;
        let length = match n {
            0 => shell.editor.path_selector.parent.len(),
            1 => shell.editor.path_selector.current.len(),
            2 => shell.editor.path_selector.children.len(),
            _ => unreachable!()
        };
        Some(layout::SpaceInfo { length, cursor: None })        
    }

    fn render(
        shell: &Shell,
        layout: &Layout,
        current: &mut layout::Point,
        mut term: RefWriter<'_>,
        n: u8
    )
        -> anyhow::Result<()>
    {
        use crate::editor::path_selector::EntryType;
    
        let list = match n {
            0 => &shell.editor.path_selector.parent,
            1 => &shell.editor.path_selector.current,
            2 => &shell.editor.path_selector.children,
            _ => unreachable!()
        };

        let config = shell.config.borrow();
        let mut count = 0;

        let mut iter = list.take(layout.size.1.into()).peekable();

        while let Some((hint, entry)) = iter.next() {
            let name = entry.name();

            let color = match entry.type_() {
                EntryType::Dir => config.theme.variable.color(),
                EntryType::File => Some(style::Color::White),
                EntryType::Other => config.theme.single_str.color()
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
                cursor::MoveToColumn(layout.range.start.x),
                style::SetColors(color),
                style::Print(LimitAndFill(
                    name.as_encoded_bytes().chars(),
                    Some(' '),
                    layout.size.0.saturating_sub(1).into()
                )),
                style::ResetColor,
            )?;

            if iter.peek().is_some() {
                queue!(term, style::Print('\n'))?;
                count += 1;
            }
        }

        queue!(term, cursor::MoveToColumn(layout.range.end.x))?;

        current.x = layout.range.end.x;
        current.y += count;
        Ok(())      
    }

    const fn imp<const N: u8>() -> ElementImpl {
        ElementImpl {
            info: |shell| info(shell, N),
            render: |shell, layout, current, term|
                render(shell, layout, current, term, N)
        }
    }

    (imp::<0>(), imp::<1>(), imp::<2>())
};

const COMPLETE_SELECTOR: ElementImpl = ElementImpl {
    info: |shell| {
        matches!(shell.editor.mode, editor::Mode::CompleteSelector)
            .then_some(())?;

        Some(layout::SpaceInfo {
            length: shell.editor.complete_selector.window.len(),
            cursor: None
        })
    },
    render: |shell, layout, current, mut term| {
        let selector = &shell.editor.complete_selector;

        #[allow(clippy::obfuscated_if_else)]
        let width = (selector.column == 1)
            .then_some(layout.size.0 as usize)
            .unwrap_or(selector.width);

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
                let (comp_limit, rem) = if width >= comp_width + prepad {
                    // 'comp '
                    (comp_width, width - comp_width)
                } else {
                    // 'com… '
                    (width - 2, 0)
                };
                let (pad, desc_limit) = rem.checked_sub(prepad + desc_width)
                    // 'comp(pad-1)[ ]'
                    // 'comp(pad)[()]desc[ ]'
                    .map(|pad| (pad - (desc_width == 0) as usize, desc_width))
                    // 'comp (desc…)'
                    .unwrap_or_else(|| (1, rem.saturating_sub(prepad + 1)));

                queue!(term, style::SetColors(color))?;

                if comp_limit == comp_width {
                    queue!(term, style::Print(comp), style::Print(Fill(' ', pad)))?;
                } else {
                    queue!(term,
                        style::Print(LimitAndFill(comp.chars(), None, comp_limit)),
                        style::Print("…"),
                    )?;
                }

                if !desc.is_empty() {
                    if desc_limit == desc_width {
                        queue!(term, style::Print('('), style::Print(desc), style::Print(')'))?
                    } else {
                        queue!(term,
                            style::Print('('),
                            style::Print(LimitAndFill(desc.chars(), None, desc_limit)),
                            style::Print('…'),
                            style::Print(')'),
                        )?;
                    }
                }

                queue!(term, style::ResetColor)?;

                if column + 1 != chunk.len() {
                    queue!(term, style::Print(' '))?;
                }
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

const ERROR: ElementImpl = ElementImpl {
    info: |shell| {
        let err = shell.error.as_ref()?;
        let err = err.lines().next().unwrap_or_default();
        Some(layout::SpaceInfo { length: err.width(), cursor: None })
    },
    render: |shell, layout, current, mut term| {
        let err = shell.error.as_deref().unwrap_or_default();
        let err = err.lines().next().unwrap_or_default();

        queue!(term, style::Print(err))?;

        debug_assert_eq!(
            current.x + err.width() as u16,
            layout.range.end.x
        );

        *current = layout.range.end;

        Ok(())
    }
};
