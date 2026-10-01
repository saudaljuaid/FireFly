//! Local paint groups for the deliberately bounded static stacking subset.
//!
//! Every principal block/inline-block is a paint barrier. Its own background
//! precedes all descendants; descendant groups sort negative, in-flow,
//! auto/zero positioned, positive. Groups retain their own clips and cannot
//! escape an unrelated ancestor. This is narrower than CSS Appendix E's
//! promotion of positioned descendants through non-stacking ancestors.

use std::collections::HashMap;

use crate::dom::{Document, NodeId};
use crate::layout::Primitive;
use crate::style::{ComputedStyle, Display, Position};

#[derive(Debug, Clone)]
pub struct PaintGroup {
    pub node: NodeId,
    pub parent: Option<NodeId>,
    pub start: usize,
    pub content_start: usize,
    pub content_end: usize,
    pub end: usize,
}

impl PaintGroup {
    pub fn offset(&mut self, offset: usize) {
        self.start += offset;
        self.content_start += offset;
        self.content_end += offset;
        self.end += offset;
    }
}

struct Order<'a> {
    groups: &'a [PaintGroup],
    styles: &'a [ComputedStyle],
    document: &'a Document,
    inline_backgrounds: HashMap<NodeId, Vec<usize>>,
    children: Vec<Vec<usize>>,
    tree_order: Vec<usize>,
    indices: Vec<usize>,
    emitted: Vec<bool>,
    visited: Vec<bool>,
}

impl Order<'_> {
    fn emit(&mut self, start: usize, end: usize) {
        for index in start.min(self.emitted.len())..end.min(self.emitted.len()) {
            if !self.emitted[index] {
                self.emitted[index] = true;
                self.indices.push(index);
            }
        }
    }

    fn phase(&self, index: usize) -> (u8, i32, i32, usize) {
        let node = self.groups[index].node;
        let style = &self.styles[node];
        let item = style.position != Position::Absolute
            && self.document.nodes[node].parent.is_some_and(|parent| {
                self.styles[parent].display.is_flex() || self.styles[parent].display.is_grid()
            });
        let (phase, z) = if style.position == Position::Static && !(item && style.z_index.is_some())
        {
            (1, 0)
        } else {
            match style.z_index.unwrap_or(0) {
                z if z < 0 => (0, z),
                0 => (2, 0),
                z => (3, z),
            }
        };
        (
            phase,
            z,
            if item { style.order } else { 0 },
            self.tree_order[node],
        )
    }

    fn group(&mut self, index: usize, depth: usize) {
        if depth > 256 || self.visited[index] {
            return;
        }
        self.visited[index] = true;
        let group = &self.groups[index];
        let (start, content_start, content_end, end) = (
            group.start,
            group.content_start,
            group.content_end,
            group.end,
        );
        self.emit(start, content_start);
        self.content(index, content_start, content_end, depth + 1);
        self.emit(content_end, end);
    }

    fn content(&mut self, parent: usize, start: usize, end: usize, depth: usize) {
        let mut children = self.children[parent].clone();
        children.sort_by_key(|&index| self.phase(index));
        for &index in &children {
            if self.phase(index).0 == 0 {
                self.ancestor_inline_backgrounds(index, parent, start, end);
                self.group(index, depth);
            }
        }
        // Only ranges physically contained in this flow interval are removed
        // from it. Absolute children appended by the second pass are painted
        // in their appropriate phase through the parent link.
        let mut physical: Vec<_> = children
            .iter()
            .copied()
            .filter(|&index| {
                let group = &self.groups[index];
                group.start >= start && group.end <= end
            })
            .collect();
        physical.sort_by_key(|&index| self.groups[index].start);
        let mut cursor = start;
        for index in physical {
            let group_start = self.groups[index].start;
            let group_end = self.groups[index].end;
            self.emit(cursor, group_start);
            if self.phase(index).0 == 1 {
                self.group(index, depth);
            }
            cursor = cursor.max(group_end);
        }
        self.emit(cursor, end);
        for index in children {
            if self.phase(index).0 >= 2 {
                self.group(index, depth);
            }
        }
    }

    fn ancestor_inline_backgrounds(
        &mut self,
        child: usize,
        parent: usize,
        start: usize,
        end: usize,
    ) {
        let principal = self.groups.get(parent).map(|group| group.node);
        let mut ancestors = Vec::new();
        let mut current = self.document.nodes[self.groups[child].node].parent;
        let mut depth = 0;
        while let Some(node) = current {
            if depth >= 256 {
                break;
            }
            depth += 1;
            if Some(node) == principal {
                break;
            }
            if self.styles[node].display == Display::Inline {
                ancestors.push(node);
            }
            current = self.document.nodes[node].parent;
        }
        // Paint each decorated inline ancestor once, outer to inner, while
        // retaining its principal ancestor's active clip. All of its wrapped
        // fragments precede the negative positioned descendant.
        for node in ancestors.into_iter().rev() {
            if let Some(indices) = self.inline_backgrounds.remove(&node) {
                for index in indices
                    .into_iter()
                    .filter(|&index| start <= index && index < end)
                {
                    self.emit(index, index + 1);
                }
            }
        }
    }
}

/// Flatten local groups once. No global primitive sort can detach a child from
/// its ancestor's border or clip. `normal_end` marks the original in-flow
/// prefix; later absolute subtrees belong to their recorded parent groups.
pub fn order(
    primitives: Vec<Primitive>,
    groups: &[PaintGroup],
    inline_backgrounds: &[(NodeId, usize)],
    styles: &[ComputedStyle],
    document: &Document,
    normal_end: usize,
) -> Vec<Primitive> {
    let mut tree_order = vec![0; document.nodes.len()];
    let mut pending = vec![0];
    let mut sequence = 0;
    while let Some(node) = pending.pop() {
        tree_order[node] = sequence;
        sequence += 1;
        pending.extend(document.nodes[node].children.iter().rev().copied());
    }
    let root = groups.len();
    let by_node: HashMap<_, _> = groups
        .iter()
        .enumerate()
        .map(|(index, group)| (group.node, index))
        .collect();
    let mut children = vec![Vec::new(); groups.len() + 1];
    for (index, group) in groups.iter().enumerate() {
        let parent = group
            .parent
            .and_then(|node| by_node.get(&node).copied())
            .unwrap_or(root);
        children[parent].push(index);
    }
    let mut order = Order {
        groups,
        styles,
        document,
        inline_backgrounds: {
            let mut mapped = HashMap::<NodeId, Vec<usize>>::new();
            for &(node, index) in inline_backgrounds {
                mapped.entry(node).or_default().push(index);
            }
            mapped
        },
        children,
        tree_order,
        indices: Vec::with_capacity(primitives.len()),
        emitted: vec![false; primitives.len()],
        visited: vec![false; groups.len()],
    };
    order.content(root, 0, normal_end.min(primitives.len()), 0);
    let mut primitives: Vec<_> = primitives.into_iter().map(Some).collect();
    order
        .indices
        .into_iter()
        .filter_map(|index| primitives[index].take())
        .collect()
}
