use std::{ io, fmt, cmp };
use crossterm::{ queue, cursor, style };
use crate::util::arena::{ Id, ArenaMap };
use super::layout::{ self, Layout };


pub struct Renderer<Shell, Error> {
    pub size: (u16, u16),
    queue: Vec<(Id<layout::Node>, Layout)>,
    map: ArenaMap<layout::Node, RenderVtable<Shell, Error>>,
}

pub trait Render {
    type Data;
    type Error;

    fn length(data: &Self::Data, leaf_id: Id<layout::Node>) -> Option<usize>;
    fn render(
        data: &Self::Data,
        leaf_id: Id<layout::Node>,
        layout: &Layout,
        current: &mut layout::Point,
        term: RefWriter<'_>
    )
        -> Result<(), Self::Error>;
}

struct RenderVtable<Data, Error> {
    length: fn(&Data, Id<layout::Node>) -> Option<usize>,
    render: fn(
        &Data,
        Id<layout::Node>,
        &Layout,
        &mut layout::Point,
        RefWriter<'_>
    ) -> Result<(), Error>   
}

impl<Shell, Error> Renderer<Shell, Error> {
    pub fn new(size: (u16, u16)) -> Self {
        Renderer { size, queue: Vec::new(), map: ArenaMap::default() }
    }
    
    pub fn insert<R>(&mut self, leaf_id: Id<layout::Node>)
        where R: Render<Data = Shell, Error = Error>
    {
        self.map.insert(leaf_id, RenderVtable {
            length: R::length,
            render: R::render
        });
    }

    pub fn render<W>(&mut self, tree: &layout::Tree, shell: &Shell, term: &mut W)
        -> Result<(), Error>
    where
        W: io::Write,
        Error: From<io::Error>
    {
        let space = RenderSpace {
            shell, map: &self.map
        };

        self.queue.clear();
        tree.layout(&space, self.size, &mut self.queue);
        self.queue.sort_by_key(|(_, layout)| (layout.range.start.y, layout.range.start.x));

        let mut current = layout::Point {
            x: 0,
            y: 0
        };
        let mut max_y = 0;

        for (id, layout) in &self.queue {
            let id = *id;
            let Some(vtable) = self.map.get(id)
                else { continue };

            if current != layout.range.start {
                if current.x != layout.range.start.x {
                    queue!(term, cursor::MoveToColumn(layout.range.start.x))?
                }

                let diff = current.y.abs_diff(layout.range.start.y);

                if diff != 0 {
                    match current.y > layout.range.start.y {
                        true => queue!(term, cursor::MoveToPreviousLine(diff))?,
                        false if layout.range.start.y > max_y =>
                            queue!(term, style::Print(Fill('\n', diff.into())))?,
                        false => queue!(term, cursor::MoveToNextLine(diff))?
                    }
                }
            }

            current = layout.range.start;
            (vtable.render)(shell, id, layout, &mut current, RefWriter(term))?;
            max_y = cmp::max(max_y, current.y);
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
