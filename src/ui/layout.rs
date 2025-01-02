use std::{ cmp, iter };
use std::convert::TryInto;
use std::ops::Range;
use smallvec::SmallVec;
use crate::util::arena::{ Arena, Id };


pub struct Tree {
    nodes: Arena<Node>,
    freelist: Vec<Id<Node>>,
    root: Id<Node>
}

pub struct Node {
    style: Style,
    children: SmallVec<[Id<Node>; 3]>
}

#[derive(Clone, Copy, Default)]
pub struct Style {
    pub axis: Axis,
    pub justify: Justify,
    pub overflow: bool,
    pub wrap: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    #[default]
    Horizontal,
    Vertical
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Justify {
    #[default]
    Start,
    Stretch,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Point {
    pub x: u16,
    pub y: u16
}

pub trait Space {
    fn length(&self, leaf: Id<Node>) -> Option<usize>;
}

impl Default for Tree {
    fn default() -> Self {
        let mut nodes = Arena::default();
        let root = nodes.alloc(Node {
            style: Style {
                axis: Axis::Vertical,
                justify: Justify::Start,
                overflow: false,
                wrap: false,
            },
            children: Default::default()
        });
        Tree {
            nodes, root,
            freelist: Default::default()
        }
    }
}

impl Tree {
    pub fn root(&self) -> Id<Node> {
        self.root
    }
    
    pub fn new_node(&mut self, parent: Id<Node>, style: Style) -> Id<Node> {
        let id = if let Some(id) = self.freelist.pop() {
            self.nodes[id].style = style;
            self.nodes[id].children.clear();
            id
        } else {
            self.nodes.alloc(Node {
                style, children: Default::default()
            })
        };
        self.nodes[parent].children.push(id);
        id
    }

    pub fn children(&self, id: Id<Node>) -> &[Id<Node>] {
        &self.nodes[id].children
    }

    pub fn clear(&mut self, parent: Id<Node>) {
        let mut list = std::mem::take(&mut self.nodes[parent].children);

        for &id in &list {
            self.clear(id);
        }
        
        self.freelist.extend(list.drain(..));
        self.nodes[parent].children = list;
    }

    pub fn layout(&self, space: &dyn Space, size: (u16, u16), output: &mut Vec<(Id<Node>, Layout)>) {
        let (columns, rows) = size;
        let root = &self.nodes[self.root];

        let state = State {
            space,
            parent_size: (columns, rows),
            range: Point { x: 0, y: 0 }..Point { x: columns, y: rows },
        };

        assert!(matches!(root.style.axis, Axis::Vertical));

        layout(self, state, self.root, output);
    }
}

#[derive(Clone, Debug)]
pub struct Layout {
    pub range: Range<Point>,
    pub size: (u16, u16),
    pub padding: u16
}

#[derive(Clone)]
struct State<'s> {
    space: &'s dyn Space,
    parent_size: (u16, u16),
    range: Range<Point>,
}


fn layout(tree: &Tree, state: State<'_>, node: Id<Node>, output: &mut Vec<(Id<Node>, Layout)>)
    -> Layout
{
    if tree.nodes[node].children.is_empty() {
        layout_leaf(tree, state, node, output)
    } else {
        layout_node(tree, state, node, output)
    }
}

fn layout_node(tree: &Tree, state: State<'_>, node: Id<Node>, output: &mut Vec<(Id<Node>, Layout)>)
    -> Layout
{
    let node = &tree.nodes[node];
    let mut state = state;
    let mut start = 0;
    let mut end = node.children.len();

    assert!(node.children.iter()
        .map(|&id| tree.nodes[id].style.justify)
        .is_sorted()
    );

    let mut node_layout = Layout {
        range: state.range.clone(),
        size: (0, 0),
        padding: 0
    };

    while start < end {
        let child_id = node.children[start];
        let child = &tree.nodes[child_id];

        if matches!(child.style.justify, Justify::Start) {
            start += 1;
        } else {
            break
        }

        assert!(!child.style.overflow);
        assert!(!child.style.wrap);

        let child_layout = layout(tree, state.clone(), child_id, output);
        match node.style.axis {
            Axis::Horizontal => {
                state.range.start.x += child_layout.size.0;
                node_layout.size.0 += child_layout.size.0;
                node_layout.size.1 = cmp::max(node_layout.size.1, child_layout.size.1);
            },
            Axis::Vertical => {
                state.range.start.y += child_layout.size.1;
                node_layout.size.0 = cmp::max(node_layout.size.0, child_layout.size.0);
                node_layout.size.1 += child_layout.size.1;
            },
        }
    }

    while start < end {
        let child_id = node.children[end - 1];
        let child = &tree.nodes[child_id];

        if matches!(child.style.justify, Justify::End) {
            end -= 1;
        } else {
            break
        }

        let child_layout = layout(tree, state.clone(), child_id, output);

        assert!(!child.style.overflow);
        assert!(!child.style.wrap);

        match node.style.axis {
            Axis::Horizontal => {
                state.range.end.x -= child_layout.size.0;
                node_layout.size.0 = state.parent_size.0;
                node_layout.size.1 = cmp::max(node_layout.size.1, child_layout.size.1);
            }
            Axis::Vertical => {
                state.range.end.y -= child_layout.size.1;
                node_layout.size.0 = cmp::max(node_layout.size.0, child_layout.size.0);
                node_layout.size.1 = state.parent_size.1;
            }
        }
    }

    let dynamic_nodes = &node.children[start..end];

    if !dynamic_nodes.is_empty() {
        let (step, half) = {
            let total = match node.style.axis {
                Axis::Horizontal => usize::from(state.range.end.x - state.range.start.x),
                Axis::Vertical => usize::from(state.range.end.y - state.range.start.y)
            };

            let step: u16 = total.div_ceil(dynamic_nodes.len()).try_into().unwrap();
            let half = step.div_ceil(2);
            (step, half)
        };

        let prev_end = state.range.end;

        for &child_id in dynamic_nodes {
            let child = &tree.nodes[child_id];

            assert_eq!(child.style.justify, Justify::Stretch);

            let rem = match node.style.axis {
                Axis::Horizontal => prev_end.x - state.range.start.x,
                Axis::Vertical => prev_end.y - state.range.start.y,
            };
            let step = match rem.checked_sub(step) {
                Some(rem) if rem > half => rem,
                Some(_) => step,
                None => rem
            };

            match node.style.axis {
                Axis::Horizontal => state.range.end.x = state.range.start.x + step,
                Axis::Vertical => state.range.end.y = state.range.start.y + step
            }

            let child_layout = layout(tree, state.clone(), child_id, output);

            match node.style.axis {
                Axis::Horizontal => {
                    state.range.start.x += child_layout.size.0;
                    node_layout.size.0 += child_layout.size.0;
                    node_layout.size.1 = cmp::max(node_layout.size.1, child_layout.size.1);
                },
                Axis::Vertical => {
                    state.range.start.y += child_layout.size.1;
                    node_layout.size.0 = cmp::max(node_layout.size.0, child_layout.size.0);
                    node_layout.size.1 += child_layout.size.1;
                },
            }
        }        
    }

    node_layout
}

fn layout_leaf(tree: &Tree, state: State<'_>, leaf_id: Id<Node>, output: &mut Vec<(Id<Node>, Layout)>)
    -> Layout
{
    let leaf = &tree.nodes[leaf_id];

    if leaf.style.wrap {
        assert!(!leaf.style.overflow);
    }
    
    let mut leaf_layout = Layout {
        range: state.range.start..state.range.start,
        size: (0, 0),
        padding: 0
    };

    if matches!(leaf.style.justify, Justify::End) {
        leaf_layout.range.start.x = state.range.end.x;
        leaf_layout.range.end.x = state.range.end.x;
    }
    
    let mut len = match state.space.length(leaf_id) {
        Some(len) => len,
        None => return leaf_layout
    };
    let first_line = state.range.end.x - state.range.start.x;
    let full_line = state.parent_size.0;

    match leaf.style.justify {
        Justify::Start => {
            if usize::from(first_line) > len {
                let len: u16 = len.try_into().unwrap();
                leaf_layout.range.end.x += len;
                leaf_layout.size.0 += len;
            } else {
                leaf_layout.range.end.x += first_line;
                leaf_layout.size.0 += first_line;
            }

            output.push((leaf_id, leaf_layout.clone()));
            return leaf_layout;
        },
        Justify::Stretch => (),
        Justify::End => {
            if usize::from(first_line) > len {
                let len: u16 = len.try_into().unwrap();
                leaf_layout.range.start.x -= len;
                leaf_layout.size.0 += len;
            } else {
                leaf_layout.range.start.x -= first_line;
                leaf_layout.size.0 += first_line;
            }

            output.push((leaf_id, leaf_layout.clone()));
            return leaf_layout; 
        }
    }

    for line in iter::once(first_line)
        .chain(iter::repeat(full_line))
        .map(usize::from)
        .take(usize::from(state.range.end.y - state.range.start.y))
    {
        if let Some(rem) = line.checked_sub(len) {
            let len: u16 = len.try_into().unwrap();
            let rem: u16 = rem.try_into().unwrap();
            leaf_layout.range.end.x += len;
            leaf_layout.size.0 += len;
            leaf_layout.size.1 += 1;

            if matches!(leaf.style.justify, Justify::Stretch) {
                leaf_layout.size.0 += rem;
                leaf_layout.padding = rem;
            }

            break
        } if !leaf.style.overflow {
            let line: u16 = line.try_into().unwrap();
            leaf_layout.range.end.x = line;
            leaf_layout.size.0 = line;
            leaf_layout.size.1 = 1;
            break
        } else {
            len -= line;
            leaf_layout.range.end.x = 0;
            leaf_layout.size.0 = 0;
            leaf_layout.size.1 += 1;
        }
    }

    output.push((leaf_id, leaf_layout.clone()));
    leaf_layout
}
