use std::{ fmt, cmp };
use std::io::{ self, Write };
use crossterm::{ queue, cursor, style, terminal };
use crate::util::RefWriter;
use crate::util::arena::{ Id, ArenaMap };
use super::layout::{ self, Layout };


pub struct Renderer<S, T, E> {
    pub size: (u16, u16),
    pub term: T,
    current: layout::Point,
    max_y: u16,
    queue: Vec<(Id<layout::Node>, Layout)>,
    map: ArenaMap<layout::Node, RenderVtable<S, E>>,
}

pub trait Target {
    type Writer: io::Write;

    fn access(&self) -> Self::Writer;
}

pub trait Render {
    type State;
    type Error;

    fn length_and_cursor(state: &Self::State, leaf_id: Id<layout::Node>) -> (Option<usize>, Option<usize>);
    fn render(
        state: &Self::State,
        leaf_id: Id<layout::Node>,
        layout: &Layout,
        current: &mut layout::Point,
        term: RefWriter<'_>
    )
        -> Result<(), Self::Error>;
}

type LengthAndCursorMethod<State> = fn(&State, Id<layout::Node>) -> (Option<usize>, Option<usize>);
type RenderMethod<State, Error> = fn(
    &State,
    Id<layout::Node>,
    &Layout,
    &mut layout::Point,
    RefWriter<'_>
) -> Result<(), Error>;

struct RenderVtable<State, Error> {
    length_and_cursor: LengthAndCursorMethod<State>,
    render: RenderMethod<State, Error>
}

impl<Shell, Target, Error> Renderer<Shell, Target, Error> {
    pub fn insert<R>(&mut self, leaf_id: Id<layout::Node>)
        where R: Render<State = Shell, Error = Error>
    {
        self.map.insert(leaf_id, RenderVtable {
            length_and_cursor: R::length_and_cursor,
            render: R::render
        });
    }
}

impl<S, T, E> Renderer<S, T, E>
where
    T: Target
{
    pub fn new(size: (u16, u16), term: T) -> Self {
        Renderer {
            size, term,
            current: layout::Point { x: 0, y: 0 },
            max_y: 0,
            queue: Vec::new(),
            map: ArenaMap::default()
        }
    }

    pub fn new_line(&mut self, with_message: &dyn fmt::Display) -> io::Result<()> {
        let mut term = self.term.access();
        queue!(term,
            style::Print("\r\n"),
            style::Print(with_message),
            terminal::Clear(terminal::ClearType::FromCursorDown)
        )?;

        self.max_y = 0;
        self.current = layout::Point {
            x: 0,
            y: 0
        };

        term.flush()
    }    

    pub fn render(&mut self, shell: &S)
        -> Result<(), E>
    where
        S: AsRef<layout::Tree>,
        E: From<io::Error>
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
            shell, map: &self.map
        };

        let mut cursor = None;
        self.queue.clear();
        shell.as_ref().layout(&space, self.size, &mut cursor, &mut self.queue);
        self.queue.sort_by_key(|(_, layout)| (layout.range.start.y, layout.range.start.x));

        let mut term = self.term.access();
        let mut clear = Some(());

        for (id, layout) in &self.queue {
            let id = *id;
            let Some(vtable) = self.map.get(id)
                else { continue };

            move_to(&mut term, &mut self.current, self.max_y, layout.range.start)?;

            if clear.take().is_some() {
                queue!(term, terminal::Clear(terminal::ClearType::FromCursorDown))?;
            }

            (vtable.render)(shell, id, layout, &mut self.current, RefWriter(&mut term))?;
            self.max_y = cmp::max(self.max_y, self.current.y);
        }

        if let Some(cursor) = cursor {
            move_to(&mut term, &mut self.current, self.max_y, cursor)?;
        }

        term.flush()?;

        Ok(())
    }
}

struct RenderSpace<'a, Shell, Error> {
    shell: &'a Shell,
    map: &'a ArenaMap<layout::Node, RenderVtable<Shell, Error>>
}

impl<Shell, Error> layout::Space for RenderSpace<'_, Shell, Error> {
    fn length_and_cursor(&self, leaf: Id<layout::Node>) -> (Option<usize>, Option<usize>) {
        self.map.get(leaf)
            .map(|vtable| (vtable.length_and_cursor)(self.shell, leaf))
            .unwrap_or_default()
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

impl<F, W> Target for F
where
    F: Fn() -> W,
    W: io::Write
{
    type Writer = W;

    fn access(&self) -> Self::Writer {
        (self)()
    }
}
