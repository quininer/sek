pub mod layout;
pub mod render;

use std::fmt;
use crate::util::arena::{ Id, ArenaMap };

pub trait Element: fmt::Debug {
    fn walk(
        &self,
        tree: &mut layout::Tree,
        table: &mut Table,
        map: &mut Vec<(Tag, Id<layout::Node>)>,
        parent: Id<layout::Node>
    );
}

pub type Table = ArenaMap<layout::Node, render::ElementImpl>;
pub type Tag = u32;

#[derive(Debug)]
pub struct Box<T>(pub Option<Tag>, pub layout::Style, pub T);

#[derive(Debug)]
pub struct Elem(pub Option<Tag>, pub layout::Style, pub render::ElementImpl);

impl<T: Element> Element for Box<T> {
    fn walk(
        &self,
        tree: &mut layout::Tree,
        table: &mut Table,
        map: &mut Vec<(Tag, Id<layout::Node>)>,
        parent: Id<layout::Node>
    ) {
        let id = tree.new_node(parent, self.1);
        self.2.walk(tree, table, map, id);

        if let Some(tag) = self.0 {
            map.push((tag, id));
        }        
    }
}

impl Element for Elem {
    fn walk(
        &self,
        tree: &mut layout::Tree,
        table: &mut Table,
        map: &mut Vec<(Tag, Id<layout::Node>)>,
        parent: Id<layout::Node>
    ) {
        let id = tree.new_node(parent, self.1);
        table.insert(id, self.2);

        if let Some(tag) = self.0 {
            map.push((tag, id));
        }
    }
}

impl<A, B> Element for (A, B)
where
    A: Element,
    B: Element,
{
    fn walk(
        &self,
        tree: &mut layout::Tree,
        table: &mut Table,
        map: &mut Vec<(Tag, Id<layout::Node>)>,
        parent: Id<layout::Node>)
    {
        self.0.walk(tree, table, map, parent);
        self.1.walk(tree, table, map, parent);
    }
}

impl<A> Element for (A,)
where
    A: Element,
{
    fn walk(
        &self,
        tree: &mut layout::Tree,
        table: &mut Table,
        map: &mut Vec<(Tag, Id<layout::Node>)>,
        parent: Id<layout::Node>)
    {
        self.0.walk(tree, table, map, parent);
    }
}

impl<A, B, C> Element for (A, B, C)
where
    A: Element,
    B: Element,
    C: Element,
{
    fn walk(
        &self,
        tree: &mut layout::Tree,
        table: &mut Table,
        map: &mut Vec<(Tag, Id<layout::Node>)>,
        parent: Id<layout::Node>)
    {
        self.0.walk(tree, table, map, parent);
        self.1.walk(tree, table, map, parent);
        self.2.walk(tree, table, map, parent);
    }
}

impl<A, B, C, D> Element for (A, B, C, D)
where
    A: Element,
    B: Element,
    C: Element,
    D: Element,
{
    fn walk(
        &self,
        tree: &mut layout::Tree,
        table: &mut Table,
        map: &mut Vec<(Tag, Id<layout::Node>)>,
        parent: Id<layout::Node>)
    {
        self.0.walk(tree, table, map, parent);
        self.1.walk(tree, table, map, parent);
        self.2.walk(tree, table, map, parent);
        self.3.walk(tree, table, map, parent);
    }
}
