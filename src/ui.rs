pub mod layout;
pub mod render;

use crate::util::arena::{ Id, ArenaMap };
use crate::shell::Shell;

pub trait Element {
    fn walk(
        &self,
        tree: &mut layout::Tree,
        table: &mut Table,
        parent: Id<layout::Node>
    );
}

pub type Table = ArenaMap<layout::Node, &'static render::RenderVtable<Shell, anyhow::Error>>;

pub struct Box<T>(pub layout::Style, pub T);
pub struct Elem<T>(pub layout::Style, pub T);

impl<T: Element> Element for Box<T> {
    fn walk(&self, tree: &mut layout::Tree, table: &mut Table, parent: Id<layout::Node>) {
        let id = tree.new_node(parent, self.0);
        self.1.walk(tree, table, id);
    }
}

impl<T> Element for Elem<T>
where
    T: render::Element<State = Shell, Error = anyhow::Error>
{
    fn walk(&self, tree: &mut layout::Tree, table: &mut Table, parent: Id<layout::Node>) {
        let id = tree.new_node(parent, self.0);
        table.insert(id, T::VTABLE);
    }
}

impl<A, B> Element for (A, B)
where
    A: Element,
    B: Element,
{
    fn walk(&self, tree: &mut layout::Tree, table: &mut Table, parent: Id<layout::Node>) {
        self.0.walk(tree, table, parent);
        self.1.walk(tree, table, parent);
    }
}

impl<A, B, C> Element for (A, B, C)
where
    A: Element,
    B: Element,
    C: Element,
{
    fn walk(&self, tree: &mut layout::Tree, table: &mut Table, parent: Id<layout::Node>) {
        self.0.walk(tree, table, parent);
        self.1.walk(tree, table, parent);
        self.2.walk(tree, table, parent);
    }
}
