use std::{ io, fmt, cmp };
use crossterm::{ queue, cursor, style };
use crate::util::arena::{ Id, ArenaMap };
use super::layout::{ self, Layout };


pub struct Renderer<S, G, E> {
    pub size: (u16, u16),
    pub term: G,
    cursor: layout::Point,
    max_y: u16,
    queue: Vec<(Id<layout::Node>, Layout)>,
    map: ArenaMap<layout::Node, RenderVtable<S, E>>,
}

pub trait Render {
    type State;
    type Error;

    fn length(state: &Self::State, leaf_id: Id<layout::Node>) -> Option<usize>;
    fn render(
        state: &Self::State,
        leaf_id: Id<layout::Node>,
        layout: &Layout,
        current: &mut layout::Point,
        term: RefWriter<'_>
    )
        -> Result<(), Self::Error>;
}

struct RenderVtable<State, Error> {
    length: fn(&State, Id<layout::Node>) -> Option<usize>,
    render: fn(
        &State,
        Id<layout::Node>,
        &Layout,
        &mut layout::Point,
        RefWriter<'_>
    ) -> Result<(), Error>   
}

impl<Shell, GetWriter, Error> Renderer<Shell, GetWriter, Error> {
    pub fn insert<R>(&mut self, leaf_id: Id<layout::Node>)
        where R: Render<State = Shell, Error = Error>
    {
        self.map.insert(leaf_id, RenderVtable {
            length: R::length,
            render: R::render
        });
    }
}

impl<Shell, GetWriter, Error, Writer> Renderer<Shell, GetWriter, Error>
where
    GetWriter: Fn() -> Writer,
    Writer: io::Write
{
    pub fn new(size: (u16, u16), term: GetWriter) -> Self {
        Renderer {
            size, term,
            cursor: layout::Point { x: 0, y: 0 },
            max_y: 0,
            queue: Vec::new(),
            map: ArenaMap::default()
        }
    }

    pub fn render(&mut self, shell: &Shell)
        -> Result<(), Error>
    where
        Shell: AsRef<layout::Tree>,
        Error: From<io::Error>
    {
        let space = RenderSpace {
            shell, map: &self.map
        };

        self.queue.clear();
        shell.as_ref().layout(&space, self.size, &mut self.queue);
        self.queue.sort_by_key(|(_, layout)| (layout.range.start.y, layout.range.start.x));

        let mut term = (self.term)();

        for (id, layout) in &self.queue {
            let id = *id;
            let Some(vtable) = self.map.get(id)
                else { continue };

            if self.cursor != layout.range.start {
                if self.cursor.x != layout.range.start.x {
                    queue!(term, cursor::MoveToColumn(layout.range.start.x))?
                }

                let diff = self.cursor.y.abs_diff(layout.range.start.y);

                if diff != 0 {
                    match self.cursor.y > layout.range.start.y {
                        true => queue!(term, cursor::MoveToPreviousLine(diff))?,
                        false if layout.range.start.y > self.max_y =>
                            queue!(term, style::Print(Fill('\n', diff.into())))?,
                        false => queue!(term, cursor::MoveToNextLine(diff))?
                    }
                }
            }

            self.cursor = layout.range.start;
            (vtable.render)(shell, id, layout, &mut self.cursor, RefWriter(&mut term))?;
            self.max_y = cmp::max(self.max_y, self.cursor.y);
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
    fn length(&self, leaf: Id<layout::Node>) -> Option<usize> {
        (self.map.get(leaf)?.length)(self.shell, leaf)
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

pub struct RefWriter<'a>(pub &'a mut dyn io::Write);

impl io::Write for RefWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
