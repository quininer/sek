pub mod layout;
pub mod render;

use crate::util::arena::{ArenaMap, Id};
use std::fmt;

pub trait Element: fmt::Debug {
    fn walk(
        &self,
        tree: &mut layout::Tree,
        table: &mut Table,
        map: &mut Vec<(Tag, Id<layout::Node>)>,
        parent: Id<layout::Node>,
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
        parent: Id<layout::Node>,
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
        parent: Id<layout::Node>,
    ) {
        let id = tree.new_node(parent, self.1);
        table.insert(id, self.2);

        if let Some(tag) = self.0 {
            map.push((tag, id));
        }
    }
}

macro_rules! impl_elements {
    ( $( $typ:ident ),* ) => {
        impl<$( $typ , )*> Element for ( $( $typ , )* )
        where
        $(
            $typ: Element,
        )*
        {
            #[allow(non_snake_case)]
            fn walk(
                &self,
                tree: &mut layout::Tree,
                table: &mut Table,
                map: &mut Vec<(Tag, Id<layout::Node>)>,
                parent: Id<layout::Node>)
            {
                let ( $( $typ , )* ) = self;
                $(
                    $typ.walk(tree, table, map, parent);
                )*
            }
        }
    };
}

impl_elements!(A);
impl_elements!(A, B);
impl_elements!(A, B, C);
impl_elements!(A, B, C, D);
impl_elements!(A, B, C, D, E);
impl_elements!(A, B, C, D, E, F);

impl<T> Box<T> {
    pub fn new(node: T) -> Box<T> {
        Box(None, layout::Style::default(), node)
    }

    pub fn tag(mut self, tag: Tag) -> Self {
        self.0 = Some(tag);
        self
    }

    pub fn style<F>(mut self, f: F) -> Self
    where
        F: FnOnce(layout::Style) -> layout::Style,
    {
        self.1 = f(self.1);
        self
    }
}

impl Elem {
    pub fn new(imp: render::ElementImpl) -> Elem {
        Elem(None, layout::Style::default(), imp)
    }

    pub fn style<F>(mut self, f: F) -> Self
    where
        F: FnOnce(layout::Style) -> layout::Style,
    {
        self.1 = f(self.1);
        self
    }
}
