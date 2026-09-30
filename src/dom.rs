pub type NodeId = usize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Doctype {
    pub name: Option<String>,
    pub public_id: Option<String>,
    pub system_id: Option<String>,
    pub force_quirks: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Namespace {
    Html,
    MathMl,
    Svg,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    pub namespace: Namespace,
    pub tag: String,
    pub attributes: Vec<Attribute>,
}

impl Element {
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|attribute| attribute.name == name)
            .map(|attribute| attribute.value.as_str())
    }

    pub fn has_class(&self, class: &str) -> bool {
        self.attribute("class")
            .is_some_and(|value| value.split_ascii_whitespace().any(|item| item == class))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    Document,
    TemplateContent,
    Doctype(Doctype),
    Comment(String),
    ProcessingInstruction { target: String, data: String },
    Element(Element),
    Text(String),
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: NodeKind,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
}

#[derive(Debug, Clone)]
pub struct Document {
    pub nodes: Vec<Node>,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            nodes: vec![Node {
                kind: NodeKind::Document,
                parent: None,
                children: Vec::new(),
            }],
        }
    }
}

impl Document {
    /// Node IDs follow allocation order, which may differ from tree order
    /// after HTML foster parenting and formatting-element reconstruction.
    pub fn preorder(&self) -> Vec<NodeId> {
        let mut order = Vec::with_capacity(self.nodes.len());
        let mut pending = vec![0];
        while let Some(id) = pending.pop() {
            order.push(id);
            pending.extend(self.nodes[id].children.iter().rev().copied());
        }
        order
    }

    pub(crate) fn sync_selectedcontent(&mut self) {
        let selects: Vec<_> = (1..self.nodes.len())
            .filter(|&id| {
                self.element(id).is_some_and(|element| {
                    element.namespace == Namespace::Html && element.tag == "select"
                })
            })
            .collect();
        for select in selects {
            let select_element = self.element(select).unwrap();
            if select_element.attribute("multiple").is_some() {
                continue;
            }
            let size = select_element
                .attribute("size")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(1);
            let mut selectedcontent = None;
            let mut options = Vec::new();
            let mut pending: Vec<_> = self.nodes[select].children.iter().rev().copied().collect();
            while let Some(id) = pending.pop() {
                if let Some(element) = self.element(id)
                    && element.namespace == Namespace::Html
                {
                    if element.tag == "select" {
                        continue;
                    }
                    if element.tag == "selectedcontent" {
                        selectedcontent.get_or_insert(id);
                        continue;
                    }
                    if element.tag == "option" {
                        options.push(id);
                        continue;
                    }
                }
                pending.extend(self.nodes[id].children.iter().rev().copied());
            }
            let Some(target) = selectedcontent else {
                continue;
            };
            let selected = options
                .iter()
                .rev()
                .copied()
                .find(|&id| self.element(id).unwrap().attribute("selected").is_some())
                .or_else(|| {
                    (size == 1)
                        .then(|| {
                            options.iter().copied().find(|&id| {
                                if self.element(id).unwrap().attribute("disabled").is_some() {
                                    return false;
                                }
                                let mut ancestor = self.nodes[id].parent;
                                while let Some(parent) = ancestor {
                                    if parent == select {
                                        break;
                                    }
                                    if self.element(parent).is_some_and(|element| {
                                        element.tag == "optgroup"
                                            && element.attribute("disabled").is_some()
                                    }) {
                                        return false;
                                    }
                                    ancestor = self.nodes[parent].parent;
                                }
                                true
                            })
                        })
                        .flatten()
                });
            for child in std::mem::take(&mut self.nodes[target].children) {
                self.nodes[child].parent = None;
            }
            if let Some(option) = selected {
                let mut pending = Vec::new();
                for &child in &self.nodes[option].children.clone() {
                    let copy = self.append(target, self.nodes[child].kind.clone());
                    pending.push((child, copy));
                }
                while let Some((source, destination)) = pending.pop() {
                    for &child in &self.nodes[source].children.clone() {
                        let copy = self.append(destination, self.nodes[child].kind.clone());
                        pending.push((child, copy));
                    }
                }
            }
        }
    }

    pub(crate) fn reachable_clone(&self) -> Self {
        let mut result = Self::default();
        let mut pending = vec![(0, 0)];
        while let Some((source, destination)) = pending.pop() {
            let mut children = Vec::new();
            for &child in &self.nodes[source].children {
                let copy = result.append(destination, self.nodes[child].kind.clone());
                children.push((child, copy));
            }
            pending.extend(children.into_iter().rev());
        }
        result
    }

    pub(crate) fn detach(&mut self, id: NodeId) {
        if let Some(parent) = self.nodes[id].parent.take() {
            let position = self.nodes[parent]
                .children
                .iter()
                .position(|&child| child == id)
                .expect("node must belong to its parent");
            self.nodes[parent].children.remove(position);
        }
    }

    pub(crate) fn create_detached(&mut self, kind: NodeKind) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(Node {
            kind,
            parent: None,
            children: Vec::new(),
        });
        id
    }

    pub(crate) fn move_before(&mut self, id: NodeId, parent: NodeId, reference: Option<NodeId>) {
        if let Some(old_parent) = self.nodes[id].parent {
            let old_index = self.nodes[old_parent]
                .children
                .iter()
                .position(|&child| child == id)
                .expect("node must belong to its parent");
            self.nodes[old_parent].children.remove(old_index);
        }
        self.nodes[id].parent = Some(parent);
        if let Some(reference) = reference {
            let index = self.nodes[parent]
                .children
                .iter()
                .position(|&child| child == reference)
                .expect("reference node must belong to insertion parent");
            self.nodes[parent].children.insert(index, id);
        } else {
            self.nodes[parent].children.push(id);
        }
    }

    pub fn append(&mut self, parent: NodeId, kind: NodeKind) -> NodeId {
        self.insert_before(parent, None, kind)
    }

    pub fn insert_before(
        &mut self,
        parent: NodeId,
        reference: Option<NodeId>,
        kind: NodeKind,
    ) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(Node {
            kind,
            parent: Some(parent),
            children: Vec::new(),
        });
        if let Some(reference) = reference {
            let index = self.nodes[parent]
                .children
                .iter()
                .rposition(|&child| child == reference)
                .expect("reference node must belong to insertion parent");
            self.nodes[parent].children.insert(index, id);
        } else {
            self.nodes[parent].children.push(id);
        }
        id
    }

    pub fn element(&self, id: NodeId) -> Option<&Element> {
        match &self.nodes[id].kind {
            NodeKind::Element(element) => Some(element),
            _ => None,
        }
    }

    pub fn stylesheets(&self) -> String {
        let mut css = String::new();
        for id in self.preorder() {
            let node = &self.nodes[id];
            if let NodeKind::Element(element) = &node.kind {
                if element.tag != "style" {
                    continue;
                }
                let mut ancestor = self.nodes[id].parent;
                let mut in_template = false;
                while let Some(parent) = ancestor {
                    if matches!(self.nodes[parent].kind, NodeKind::TemplateContent) {
                        in_template = true;
                        break;
                    }
                    ancestor = self.nodes[parent].parent;
                }
                if in_template {
                    continue;
                }
                for &child in &node.children {
                    if let NodeKind::Text(text) = &self.nodes[child].kind {
                        css.push_str(text);
                        css.push('\n');
                    }
                }
            }
        }
        css
    }
}
