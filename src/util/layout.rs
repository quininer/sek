use std::{ cmp, iter };
use std::convert::TryInto;
use std::ops::Range;
use smallvec::SmallVec;
use super::arena::{ Arena, Id };


pub struct Tree {
    nodes: Arena<Node>,
    freelist: Vec<Id<Node>>,
    root: Id<Node>
}

pub struct Node {
    style: Style,
    children: SmallVec<[Id<Node>; 3]>
}

#[derive(Clone, Copy)]
pub struct Style {
    axis: Axis,
    justify: Justify,
    overflow: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Justify {
    Start,
    Stretch,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    x: u16,
    y: u16
}

pub trait Space {
    fn size(&self) -> (u16, u16);
    fn length(&self, leaf: Id<Node>) -> Option<usize>;
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
        let list = std::mem::take(&mut self.nodes[parent].children);

        for &id in &list {
            self.clear(id);
        }
        
        self.freelist.extend(list);
    }

    pub fn layout(&self, space: &dyn Space, output: &mut Vec<(Id<Node>, Layout)>) {
        let (columns, rows) = space.size();
        let root = &self.nodes[self.root];

        let mut state = State {
            space,
            parent: self.root,
            parent_size: (columns, rows),
            range: Point { x: 0, y: 0 }..Point { x: columns, y: rows },
        };

        assert!(matches!(root.style.axis, Axis::Vertical));

        for &child in &root.children {
            let layout = layout(self, state.clone(), child, output);

            // TODO handle overflow
            state.parent_size.1 -= layout.size.1;
        }

        todo!()
    }
}

#[derive(Clone)]
pub struct Layout {
    range: Range<Point>,
    size: (u16, u16),
    padding: u16
}

#[derive(Clone)]
struct State<'s> {
    space: &'s dyn Space,

    parent: Id<Node>,    
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
    let mut state = state;
    state.parent = node;

    let node = &tree.nodes[node];
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

        let child_layout = layout(tree, state.clone(), child_id, output);
        match node.style.axis {
            Axis::Horizontal => {
                state.range.start.y += child_layout.size.1;
                node_layout.size.0 = cmp::max(node_layout.size.0, child_layout.size.0);
                node_layout.size.1 += child_layout.size.1;
            },
            Axis::Vertical => {
                state.range.start.x += child_layout.size.1;
                node_layout.size.0 += child_layout.size.0;
                node_layout.size.1 = cmp::max(node_layout.size.1, child_layout.size.1);
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

        match node.style.axis {
            Axis::Horizontal => {
                state.range.end.y -= child_layout.size.1;
                node_layout.size.0 = cmp::max(node_layout.size.0, child_layout.size.0);
                node_layout.size.1 += child_layout.size.1;
            }
            Axis::Vertical => {
                state.range.end.x -= child_layout.size.0;
                node_layout.size.0 += child_layout.size.0;
                node_layout.size.1 = cmp::max(node_layout.size.1, child_layout.size.1);
            }
        }
    }

    let dynamic_nodes = &node.children[start..end];

    if let Some((child_id, _child)) = dynamic_nodes.first()
        .map(|&id| (id, &tree.nodes[id]))
        .filter(|_| dynamic_nodes.len() == 1)
        .filter(|(_, node)| matches!(node.style.axis, Axis::Horizontal))
        .filter(|(_, node)| matches!(node.style.justify, Justify::Stretch))
        .filter(|(_, node)| node.style.overflow)
    {
        assert_eq!(node.children.len(), end);

        let child_layout = layout(tree, state.clone(), child_id, output);

        node_layout.size.0 += child_layout.size.0;
        node_layout.size.1 = state.parent_size.1;
    } else {
        let (step, half) = {
            let total = match node.style.axis {
                Axis::Horizontal => usize::from(state.range.end.y - state.range.start.y),
                Axis::Vertical => usize::from(state.range.end.x - state.range.start.x)
            };

            let step: u16 = total.div_ceil(dynamic_nodes.len()).try_into().unwrap();
            let half = step.div_ceil(2);
            (step, half)
        };

        let prev_end = state.range.end;

        for &child_id in dynamic_nodes {
            let child = &tree.nodes[child_id];

            assert_eq!(child.style.justify, Justify::Stretch);
            if matches!(child.style.axis, Axis::Horizontal) {
                assert!(!child.style.overflow);
            }

            let rem = match node.style.axis {
                Axis::Horizontal => prev_end.y - state.range.start.y,
                Axis::Vertical => prev_end.x - state.range.start.x,
            };
            let step = match rem.checked_sub(step) {
                Some(rem) if rem > half => rem,
                Some(_) => step,
                None => rem
            };

            match node.style.axis {
                Axis::Horizontal => state.range.end.y = state.range.start.y + step,
                Axis::Vertical => state.range.end.x = state.range.start.x + step
            }

            let child_layout = layout(tree, state.clone(), child_id, output);

            match node.style.axis {
                Axis::Horizontal => {
                    state.range.start.y += child_layout.size.1;
                    node_layout.size.0 = cmp::max(node_layout.size.0, child_layout.size.0);
                    node_layout.size.1 += child_layout.size.1;
                },
                Axis::Vertical => {
                    state.range.start.x += child_layout.size.0;
                    node_layout.size.0 += child_layout.size.0;
                    node_layout.size.1 = cmp::max(node_layout.size.1, child_layout.size.1);
                },
            }
        }        
    }

    node_layout
}

fn layout_leaf(tree: &Tree, state: State<'_>, leaf_id: Id<Node>, output: &mut Vec<(Id<Node>, Layout)>)
    -> Layout
{
    let mut leaf_layout = Layout {
        range: state.range.start..state.range.start,
        size: (0, 0),
        padding: 0
    };
    
    let leaf = &tree.nodes[leaf_id];
    let mut len = match state.space.length(leaf_id) {
        Some(len) => len,
        None => return leaf_layout
    };

    for line in iter::once(usize::from(state.range.end.y - state.range.start.y))
        .chain(iter::repeat(usize::from(state.parent_size.1)))
        .take(usize::from(state.range.end.x - state.range.start.x))
    {
        if let Some(rem) = line.checked_sub(len) {
            let len: u16 = len.try_into().unwrap();
            let rem: u16 = rem.try_into().unwrap();
            leaf_layout.range.end.y += len;
            leaf_layout.size.0 += 1;
            leaf_layout.size.1 += len + rem;
            leaf_layout.padding = rem;
            break
        } if !leaf.style.overflow {
            let line: u16 = line.try_into().unwrap();
            leaf_layout.range.end.y = line;
            leaf_layout.size.0 = 1;
            leaf_layout.size.1 = line;
            break
        } else {
            len -= line;
            leaf_layout.range.end.y = 0;
            leaf_layout.size.0 += 1;
            leaf_layout.size.1 = 0;
        }
    }

    output.push((leaf_id, leaf_layout.clone()));
    leaf_layout
}
