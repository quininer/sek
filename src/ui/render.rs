use std::{ fmt, cmp };
use std::io::{ self, Write };
use crossterm::{ queue, cursor, style, terminal };
use crate::util::RefWriter;
use crate::util::arena::{ Id, ArenaMap };
use crate::shell::Shell;
use super::layout::{ self, Layout };


pub struct Renderer<T> {
    pub size: (u16, u16),
    pub term: T,
    current: layout::Point,
    max_y: u16,
    queue: Vec<(Id<layout::Node>, Layout)>,
}

pub trait TermTarget {
    type Writer: io::Write;

    fn access(&self) -> Self::Writer;
}

#[derive(Debug, Clone, Copy)]
pub struct ElementImpl {
    pub info: SpaceInfoMethod<Shell>,
    pub render: RenderMethod<Shell, anyhow::Error>
}

type SpaceInfoMethod<State> = fn(&State, Id<layout::Node>) -> Option<layout::SpaceInfo>;
type RenderMethod<State, Error> = fn(
    &State,
    Id<layout::Node>,
    &Layout,
    &mut layout::Point,
    RefWriter<'_>
) -> Result<(), Error>;

impl<T> Renderer<T>
where
    T: TermTarget
{
    pub fn new(size: (u16, u16), term: T) -> Self {
        Renderer {
            size, term,
            current: layout::Point { x: 0, y: 0 },
            max_y: 0,
            queue: Vec::new(),
        }
    }

    pub fn new_line(&mut self, with_message: &dyn fmt::Display) -> io::Result<()> {
        let mut term = self.term.access();
        queue!(term,
            style::Print("\r\n"),
            terminal::Clear(terminal::ClearType::FromCursorDown),
            style::Print(with_message),
        )?;

        self.max_y = 0;
        self.current = layout::Point {
            x: 0,
            y: 0
        };

        term.flush()
    }

    pub fn screen_reset(&mut self)  -> io::Result<()> {
        let mut term = self.term.access();
        queue!(term,
            cursor::MoveTo(0, 0),
            terminal::Clear(terminal::ClearType::FromCursorDown),
        )?;

        self.max_y = 0;
        self.current = layout::Point {
            x: 0,
            y: 0
        };

        term.flush()
    }

    pub fn render(&mut self, table: &ArenaMap<layout::Node, ElementImpl>, shell: &Shell)
        -> anyhow::Result<()>
    {
        fn move_to<W: io::Write>(term: &mut W, src: &mut layout::Point, max_y: u16, dst: layout::Point)
            -> io::Result<()>
        {
            if *src != dst {
                let diff = src.y.abs_diff(dst.y);
                if diff != 0 {
                    match src.y > dst.y {
                        true => queue!(term, cursor::MoveToPreviousLine(diff))?,
                        false if dst.y > max_y =>
                            queue!(term, style::Print(Fill('\n', diff.into())))?,
                        false => queue!(term, cursor::MoveToNextLine(diff))?
                    }
                }

                if diff != 0 || src.x != dst.x {
                    queue!(term, cursor::MoveToColumn(dst.x))?;
                }

                *src = dst;
            }

            Ok(())
        }
        
        let space = RenderSpace {
            shell, table
        };

        let mut cursor = None;
        self.queue.clear();
        shell.as_ref().layout(&space, self.size, &mut cursor, &mut self.queue);
        self.queue.sort_by_key(|(_, layout)| (layout.range.start.y, layout.range.start.x));

        let mut term = self.term.access();
        let mut clear = Some(());

        for (id, layout) in &self.queue {
            let id = *id;
            let Some(vtable) = table.get(id)
                else { continue };

            move_to(&mut term, &mut self.current, self.max_y, layout.range.start)?;

            if clear.take().is_some() {
                queue!(term, terminal::Clear(terminal::ClearType::FromCursorDown))?;
            }

            (vtable.render)(shell, id, layout, &mut self.current, RefWriter(&mut term))?;
            self.max_y = cmp::max(self.max_y, self.current.y);
        }

        if let Some(cursor) = cursor {
            queue!(term, cursor::Show)?;
            move_to(&mut term, &mut self.current, self.max_y, cursor)?;
        } else {
            queue!(term, cursor::Hide)?;
        }

        term.flush()?;

        Ok(())
    }
}

struct RenderSpace<'a> {
    shell: &'a Shell,
    table: &'a ArenaMap<layout::Node, ElementImpl>
}

impl layout::Space for RenderSpace<'_> {
    fn info(&self, leaf: Id<layout::Node>) -> Option<layout::SpaceInfo> {
        let vtable = self.table.get(leaf)?;
        (vtable.info)(self.shell, leaf)
    }
}

pub struct Fill(pub char, pub usize);

impl fmt::Display for Fill {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use std::fmt::Write;
        
        for _ in 0..self.1 {
            f.write_char(self.0)?;
        }

        Ok(())
    }
}

pub struct LimitAndFill<T>(pub T, pub Option<char>, pub usize);

impl<T> fmt::Display for LimitAndFill<T>
where
    T: Clone + Iterator<Item = char>
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use std::fmt::Write;
        use unicode_width::UnicodeWidthChar;

        let LimitAndFill(s, c, len) = self;
        let mut len = *len;

        for c in s.clone() {
            let width = c.width().unwrap_or_default();

            match len.checked_sub(width) {
                Some(rem) => len = rem,
                None => break
            }

            f.write_char(c)?;
        }

        if let Some(c) = c {
            for _ in 0..len {
                f.write_char(*c)?;
            }
        }
        
        Ok(())
    }
}

impl<F, W> TermTarget for F
where
    F: Fn() -> W,
    W: io::Write
{
    type Writer = W;

    #[inline(always)]
    fn access(&self) -> Self::Writer {
        (self)()
    }
}
