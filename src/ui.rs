pub mod layout;
pub mod render;

use std::fmt;
use crate::util::arena::{ Id, ArenaMap };

pub trait Element: fmt::Debug {
    fn walk(
        &self,
        tree: &mut layout::Tree,
        table: &mut Table,
        parent: Id<layout::Node>
    );
}

pub type Table = ArenaMap<layout::Node, render::ElementImpl>;

#[derive(Debug)]
pub struct Box<T>(pub layout::Style, pub T);

#[derive(Debug)]
pub struct Elem(pub layout::Style, pub render::ElementImpl);

impl<T: Element> Element for Box<T> {
    fn walk(&self, tree: &mut layout::Tree, table: &mut Table, parent: Id<layout::Node>) {
        let id = tree.new_node(parent, self.0);
        self.1.walk(tree, table, id);
    }
}

impl Element for Elem {
    fn walk(&self, tree: &mut layout::Tree, table: &mut Table, parent: Id<layout::Node>) {
        let id = tree.new_node(parent, self.0);
        table.insert(id, self.1);
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

impl<A> Element for (A,)
where
    A: Element,
{
    fn walk(&self, tree: &mut layout::Tree, table: &mut Table, parent: Id<layout::Node>) {
        self.0.walk(tree, table, parent);
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
