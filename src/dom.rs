pub type NodeId = usize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
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
    pub fn append(&mut self, parent: NodeId, kind: NodeKind) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(Node {
            kind,
            parent: Some(parent),
            children: Vec::new(),
        });
        self.nodes[parent].children.push(id);
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
        for node in &self.nodes {
            if let NodeKind::Element(element) = &node.kind {
                if element.tag != "style" {
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
