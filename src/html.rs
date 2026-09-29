pub mod tokenizer;

use crate::Error;
use crate::dom::{Attribute, Document, Element, Namespace, NodeId, NodeKind};
use tokenizer::{Tag, Token, Tokenizer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InsertionMode {
    Initial,
    BeforeHtml,
    BeforeHead,
    InHead,
    AfterHead,
    InBody,
    Text,
    InTable,
    InTableText,
    InCaption,
    InColumnGroup,
    InTableBody,
    InRow,
    InCell,
    InTemplate,
    AfterBody,
    AfterAfterBody,
}

fn is_void(name: &str) -> bool {
    matches!(
        name,
        "area"
            | "base"
            | "basefont"
            | "bgsound"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

fn html_space(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\u{c}' | '\r' | ' ')
}

fn split_initial_space(text: &str) -> (&str, &str) {
    let end = text
        .char_indices()
        .find(|&(_, c)| !html_space(c))
        .map_or(text.len(), |(index, _)| index);
    text.split_at(end)
}

fn implied_tag(name: &str) -> Tag {
    Tag {
        name: name.into(),
        attributes: Vec::new(),
        self_closing: false,
    }
}

fn closes_p(name: &str) -> bool {
    matches!(
        name,
        "address"
            | "article"
            | "aside"
            | "blockquote"
            | "center"
            | "details"
            | "dialog"
            | "dir"
            | "div"
            | "dl"
            | "fieldset"
            | "figcaption"
            | "figure"
            | "footer"
            | "form"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "header"
            | "hgroup"
            | "hr"
            | "main"
            | "menu"
            | "nav"
            | "ol"
            | "p"
            | "pre"
            | "search"
            | "section"
            | "summary"
            | "table"
            | "ul"
    )
}

fn is_special(name: &str) -> bool {
    closes_p(name)
        || matches!(
            name,
            "applet"
                | "body"
                | "button"
                | "caption"
                | "col"
                | "colgroup"
                | "dd"
                | "dt"
                | "html"
                | "li"
                | "marquee"
                | "object"
                | "select"
                | "tbody"
                | "td"
                | "template"
                | "textarea"
                | "tfoot"
                | "th"
                | "thead"
                | "tr"
        )
}

fn is_formatting(name: &str) -> bool {
    matches!(
        name,
        "a" | "b"
            | "big"
            | "code"
            | "em"
            | "font"
            | "i"
            | "nobr"
            | "s"
            | "small"
            | "strike"
            | "strong"
            | "tt"
            | "u"
    )
}

fn svg_name(name: &str) -> &str {
    match name {
        "altglyph" => "altGlyph",
        "altglyphdef" => "altGlyphDef",
        "altglyphitem" => "altGlyphItem",
        "animatecolor" => "animateColor",
        "animatemotion" => "animateMotion",
        "animatetransform" => "animateTransform",
        "clippath" => "clipPath",
        "feblend" => "feBlend",
        "fecolormatrix" => "feColorMatrix",
        "fecomponenttransfer" => "feComponentTransfer",
        "fecomposite" => "feComposite",
        "feconvolvematrix" => "feConvolveMatrix",
        "fediffuselighting" => "feDiffuseLighting",
        "fedisplacementmap" => "feDisplacementMap",
        "fedistantlight" => "feDistantLight",
        "fedropshadow" => "feDropShadow",
        "feflood" => "feFlood",
        "fefunca" => "feFuncA",
        "fefuncb" => "feFuncB",
        "fefuncg" => "feFuncG",
        "fefuncr" => "feFuncR",
        "fegaussianblur" => "feGaussianBlur",
        "feimage" => "feImage",
        "femerge" => "feMerge",
        "femergenode" => "feMergeNode",
        "femorphology" => "feMorphology",
        "feoffset" => "feOffset",
        "fepointlight" => "fePointLight",
        "fespecularlighting" => "feSpecularLighting",
        "fespotlight" => "feSpotLight",
        "fetile" => "feTile",
        "feturbulence" => "feTurbulence",
        "foreignobject" => "foreignObject",
        "glyphref" => "glyphRef",
        "lineargradient" => "linearGradient",
        "radialgradient" => "radialGradient",
        "textpath" => "textPath",
        _ => name,
    }
}

fn foreign_attributes(attributes: &[Attribute], namespace: Namespace) -> Vec<Attribute> {
    attributes
        .iter()
        .map(|attribute| {
            let name = match (namespace, attribute.name.as_str()) {
                (Namespace::MathMl, "definitionurl") => "definitionURL",
                (Namespace::Svg, "attributename") => "attributeName",
                (Namespace::Svg, "attributetype") => "attributeType",
                (Namespace::Svg, "basefrequency") => "baseFrequency",
                (Namespace::Svg, "baseprofile") => "baseProfile",
                (Namespace::Svg, "calcmode") => "calcMode",
                (Namespace::Svg, "clippathunits") => "clipPathUnits",
                (Namespace::Svg, "diffuseconstant") => "diffuseConstant",
                (Namespace::Svg, "edgemode") => "edgeMode",
                (Namespace::Svg, "filterunits") => "filterUnits",
                (Namespace::Svg, "glyphref") => "glyphRef",
                (Namespace::Svg, "gradienttransform") => "gradientTransform",
                (Namespace::Svg, "gradientunits") => "gradientUnits",
                (Namespace::Svg, "kernelmatrix") => "kernelMatrix",
                (Namespace::Svg, "kernelunitlength") => "kernelUnitLength",
                (Namespace::Svg, "keypoints") => "keyPoints",
                (Namespace::Svg, "keysplines") => "keySplines",
                (Namespace::Svg, "keytimes") => "keyTimes",
                (Namespace::Svg, "lengthadjust") => "lengthAdjust",
                (Namespace::Svg, "limitingconeangle") => "limitingConeAngle",
                (Namespace::Svg, "markerheight") => "markerHeight",
                (Namespace::Svg, "markerunits") => "markerUnits",
                (Namespace::Svg, "markerwidth") => "markerWidth",
                (Namespace::Svg, "maskcontentunits") => "maskContentUnits",
                (Namespace::Svg, "maskunits") => "maskUnits",
                (Namespace::Svg, "numoctaves") => "numOctaves",
                (Namespace::Svg, "pathlength") => "pathLength",
                (Namespace::Svg, "patterncontentunits") => "patternContentUnits",
                (Namespace::Svg, "patterntransform") => "patternTransform",
                (Namespace::Svg, "patternunits") => "patternUnits",
                (Namespace::Svg, "pointsatx") => "pointsAtX",
                (Namespace::Svg, "pointsaty") => "pointsAtY",
                (Namespace::Svg, "pointsatz") => "pointsAtZ",
                (Namespace::Svg, "preservealpha") => "preserveAlpha",
                (Namespace::Svg, "preserveaspectratio") => "preserveAspectRatio",
                (Namespace::Svg, "primitiveunits") => "primitiveUnits",
                (Namespace::Svg, "refx") => "refX",
                (Namespace::Svg, "refy") => "refY",
                (Namespace::Svg, "repeatcount") => "repeatCount",
                (Namespace::Svg, "repeatdur") => "repeatDur",
                (Namespace::Svg, "requiredextensions") => "requiredExtensions",
                (Namespace::Svg, "requiredfeatures") => "requiredFeatures",
                (Namespace::Svg, "specularconstant") => "specularConstant",
                (Namespace::Svg, "specularexponent") => "specularExponent",
                (Namespace::Svg, "spreadmethod") => "spreadMethod",
                (Namespace::Svg, "startoffset") => "startOffset",
                (Namespace::Svg, "stddeviation") => "stdDeviation",
                (Namespace::Svg, "stitchtiles") => "stitchTiles",
                (Namespace::Svg, "surfacescale") => "surfaceScale",
                (Namespace::Svg, "systemlanguage") => "systemLanguage",
                (Namespace::Svg, "tablevalues") => "tableValues",
                (Namespace::Svg, "targetx") => "targetX",
                (Namespace::Svg, "targety") => "targetY",
                (Namespace::Svg, "textlength") => "textLength",
                (Namespace::Svg, "viewbox") => "viewBox",
                (Namespace::Svg, "viewtarget") => "viewTarget",
                (Namespace::Svg, "xchannelselector") => "xChannelSelector",
                (Namespace::Svg, "ychannelselector") => "yChannelSelector",
                (Namespace::Svg, "zoomandpan") => "zoomAndPan",
                (_, "xlink:actuate") => "xlink actuate",
                (_, "xlink:arcrole") => "xlink arcrole",
                (_, "xlink:href") => "xlink href",
                (_, "xlink:role") => "xlink role",
                (_, "xlink:show") => "xlink show",
                (_, "xlink:title") => "xlink title",
                (_, "xlink:type") => "xlink type",
                (_, "xml:lang") => "xml lang",
                (_, "xml:space") => "xml space",
                (_, "xmlns") => "xmlns xmlns",
                (_, "xmlns:xlink") => "xmlns xlink",
                _ => &attribute.name,
            };
            Attribute {
                name: name.into(),
                value: attribute.value.clone(),
            }
        })
        .collect()
}

fn is_foreign_breakout(tag: &Tag) -> bool {
    matches!(
        tag.name.as_str(),
        "b" | "big"
            | "blockquote"
            | "body"
            | "br"
            | "center"
            | "code"
            | "dd"
            | "div"
            | "dl"
            | "dt"
            | "em"
            | "embed"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "head"
            | "hr"
            | "i"
            | "img"
            | "li"
            | "listing"
            | "menu"
            | "meta"
            | "nobr"
            | "ol"
            | "p"
            | "pre"
            | "ruby"
            | "s"
            | "small"
            | "span"
            | "strong"
            | "strike"
            | "sub"
            | "sup"
            | "table"
            | "tt"
            | "u"
            | "ul"
            | "var"
    ) || (tag.name == "font"
        && tag
            .attributes
            .iter()
            .any(|attribute| matches!(attribute.name.as_str(), "color" | "face" | "size")))
}

struct TreeBuilder {
    document: Document,
    open: Vec<NodeId>,
    active_formatting: Vec<Option<NodeId>>,
    mode: InsertionMode,
    original_mode: InsertionMode,
    html: Option<NodeId>,
    head: Option<NodeId>,
    body: Option<NodeId>,
    foster_parenting: bool,
    pending_table_text: String,
    table_text_mode: InsertionMode,
    template_modes: Vec<InsertionMode>,
}

impl TreeBuilder {
    fn new() -> Self {
        Self {
            document: Document::default(),
            open: Vec::new(),
            active_formatting: Vec::new(),
            mode: InsertionMode::Initial,
            original_mode: InsertionMode::Initial,
            html: None,
            head: None,
            body: None,
            foster_parenting: false,
            pending_table_text: String::new(),
            table_text_mode: InsertionMode::InTable,
            template_modes: Vec::new(),
        }
    }

    fn current(&self) -> NodeId {
        self.open.last().copied().unwrap_or(0)
    }

    fn name(&self, id: NodeId) -> &str {
        &self
            .document
            .element(id)
            .expect("open node is an element")
            .tag
    }

    fn namespace(&self, id: NodeId) -> Namespace {
        self.document
            .element(id)
            .expect("open node is an element")
            .namespace
    }

    fn is_special_node(&self, id: NodeId) -> bool {
        match self.namespace(id) {
            Namespace::Html => is_special(self.name(id)),
            Namespace::MathMl => matches!(
                self.name(id),
                "mi" | "mo" | "mn" | "ms" | "mtext" | "annotation-xml"
            ),
            Namespace::Svg => matches!(self.name(id), "foreignObject" | "desc" | "title"),
        }
    }

    fn is_math_text_integration(&self, id: NodeId) -> bool {
        self.namespace(id) == Namespace::MathMl
            && matches!(self.name(id), "mi" | "mo" | "mn" | "ms" | "mtext")
    }

    fn is_html_integration(&self, id: NodeId) -> bool {
        let element = self.document.element(id).expect("open node is an element");
        match element.namespace {
            Namespace::Svg => matches!(element.tag.as_str(), "foreignObject" | "desc" | "title"),
            Namespace::MathMl if element.tag == "annotation-xml" => {
                element.attribute("encoding").is_some_and(|value| {
                    value.eq_ignore_ascii_case("text/html")
                        || value.eq_ignore_ascii_case("application/xhtml+xml")
                })
            }
            _ => false,
        }
    }

    fn use_foreign_rules(&self, token: &Token) -> bool {
        if self.open.is_empty()
            || self.namespace(self.current()) == Namespace::Html
            || matches!(token, Token::Eof)
        {
            return false;
        }
        let current = self.current();
        if self.is_math_text_integration(current) {
            if matches!(token, Token::Character(_)) {
                return false;
            }
            if let Token::StartTag(tag) = token
                && !matches!(tag.name.as_str(), "mglyph" | "malignmark")
            {
                return false;
            }
        }
        if self.namespace(current) == Namespace::MathMl
            && self.name(current) == "annotation-xml"
            && matches!(token, Token::StartTag(tag) if tag.name == "svg")
        {
            return false;
        }
        if self.is_html_integration(current)
            && matches!(token, Token::StartTag(_) | Token::Character(_))
        {
            return false;
        }
        true
    }

    fn insert_foreign(&mut self, tag: &Tag, namespace: Namespace) -> Result<(), Error> {
        if !tag.self_closing && self.open.len() >= 256 {
            return Err(Error::InvalidInput(
                "HTML nesting exceeds the 256 element limit".into(),
            ));
        }
        let (parent, reference) = self.insertion_location();
        let name = if namespace == Namespace::Svg {
            svg_name(&tag.name)
        } else {
            &tag.name
        };
        let id = self.document.insert_before(
            parent,
            reference,
            NodeKind::Element(Element {
                namespace,
                tag: name.into(),
                attributes: foreign_attributes(&tag.attributes, namespace),
            }),
        );
        if !tag.self_closing {
            self.open.push(id);
        }
        Ok(())
    }

    fn process_foreign(&mut self, token: &Token) -> Result<Option<bool>, Error> {
        match token {
            Token::Character(text) => {
                self.insert_text(&text.replace('\0', "\u{fffd}"));
                Ok(Some(false))
            }
            Token::Comment(data) => {
                self.insert_comment(data);
                Ok(Some(false))
            }
            Token::Doctype(_) => Ok(Some(false)),
            Token::StartTag(tag) if is_foreign_breakout(tag) => {
                while self.namespace(self.current()) != Namespace::Html
                    && !self.is_math_text_integration(self.current())
                    && !self.is_html_integration(self.current())
                {
                    self.open.pop();
                }
                Ok(Some(true))
            }
            Token::StartTag(tag) => {
                self.insert_foreign(tag, self.namespace(self.current()))?;
                Ok(Some(false))
            }
            Token::EndTag(tag) => {
                for index in (0..self.open.len()).rev() {
                    let id = self.open[index];
                    if self.name(id).eq_ignore_ascii_case(&tag.name) {
                        self.open.truncate(index);
                        return Ok(Some(false));
                    }
                    if self.namespace(id) == Namespace::Html {
                        return Ok(None);
                    }
                }
                Ok(Some(false))
            }
            Token::Eof => Ok(None),
        }
    }

    fn append_text(&mut self, parent: NodeId, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some(&last) = self.document.nodes[parent].children.last()
            && let NodeKind::Text(existing) = &mut self.document.nodes[last].kind
        {
            existing.push_str(text);
        } else {
            self.document.append(parent, NodeKind::Text(text.into()));
        }
    }

    fn template_content(&self, id: NodeId) -> Option<NodeId> {
        self.document
            .element(id)
            .filter(|element| element.namespace == Namespace::Html && element.tag == "template")
            .and_then(|_| self.document.nodes[id].children.first().copied())
    }

    fn insert_comment(&mut self, data: &str) {
        let (parent, reference) = self.insertion_location();
        self.document
            .insert_before(parent, reference, NodeKind::Comment(data.into()));
    }

    fn insertion_location_for(&self, current: NodeId) -> (NodeId, Option<NodeId>) {
        if self.foster_parenting
            && self.namespace(current) == Namespace::Html
            && matches!(
                self.name(current),
                "table" | "tbody" | "tfoot" | "thead" | "tr"
            )
            && let Some(&target) = self.open.iter().rev().find(|&&id| {
                self.namespace(id) == Namespace::Html
                    && matches!(self.name(id), "table" | "template")
            })
        {
            if let Some(content) = self.template_content(target) {
                return (content, None);
            }
            if let Some(parent) = self.document.nodes[target].parent {
                return (parent, Some(target));
            }
        }
        (self.template_content(current).unwrap_or(current), None)
    }

    fn insertion_location(&self) -> (NodeId, Option<NodeId>) {
        self.insertion_location_for(self.current())
    }

    fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let (parent, reference) = self.insertion_location();
        let Some(reference) = reference else {
            self.append_text(parent, text);
            return;
        };
        let index = self.document.nodes[parent]
            .children
            .iter()
            .rposition(|&id| id == reference)
            .expect("table must be a child of its parent");
        if index > 0 {
            let previous = self.document.nodes[parent].children[index - 1];
            if let NodeKind::Text(existing) = &mut self.document.nodes[previous].kind {
                existing.push_str(text);
                return;
            }
        }
        self.document
            .insert_before(parent, Some(reference), NodeKind::Text(text.into()));
    }

    fn insert_at(&mut self, parent: NodeId, tag: &Tag) -> Result<NodeId, Error> {
        if !is_void(&tag.name) && self.open.len() >= 256 {
            return Err(Error::InvalidInput(
                "HTML nesting exceeds the 256 element limit".into(),
            ));
        }
        let parent = self.template_content(parent).unwrap_or(parent);
        let id = self.document.insert_before(
            parent,
            None,
            NodeKind::Element(Element {
                namespace: Namespace::Html,
                tag: tag.name.clone(),
                attributes: tag.attributes.clone(),
            }),
        );
        if !is_void(&tag.name) {
            self.open.push(id);
        }
        Ok(id)
    }

    fn insert(&mut self, tag: &Tag) -> Result<NodeId, Error> {
        if !self.foster_parenting {
            return self.insert_at(self.current(), tag);
        }
        if !is_void(&tag.name) && self.open.len() >= 256 {
            return Err(Error::InvalidInput(
                "HTML nesting exceeds the 256 element limit".into(),
            ));
        }
        let (parent, reference) = self.insertion_location();
        let id = self.document.insert_before(
            parent,
            reference,
            NodeKind::Element(Element {
                namespace: Namespace::Html,
                tag: tag.name.clone(),
                attributes: tag.attributes.clone(),
            }),
        );
        if !is_void(&tag.name) {
            self.open.push(id);
        }
        Ok(id)
    }

    fn clone_element(&mut self, id: NodeId) -> NodeId {
        let element = self
            .document
            .element(id)
            .expect("formatting element")
            .clone();
        self.document.create_detached(NodeKind::Element(element))
    }

    fn last_active(&self, name: &str) -> Option<(usize, NodeId)> {
        self.active_formatting
            .iter()
            .enumerate()
            .rev()
            .take_while(|(_, entry)| entry.is_some())
            .find_map(|(index, entry)| {
                let id = entry.expect("entry before marker");
                (self.name(id) == name).then_some((index, id))
            })
    }

    fn push_active(&mut self, id: NodeId) {
        let element = self.document.element(id).expect("formatting element");
        let mut attributes = element.attributes.clone();
        attributes.sort_by(|a, b| (&a.name, &a.value).cmp(&(&b.name, &b.value)));
        let mut identical = Vec::new();
        for (index, entry) in self.active_formatting.iter().enumerate().rev() {
            let Some(other) = entry else { break };
            let other_element = self.document.element(*other).expect("formatting element");
            if other_element.tag == element.tag
                && other_element.attributes.len() == attributes.len()
            {
                let mut other_attributes = other_element.attributes.clone();
                other_attributes.sort_by(|a, b| (&a.name, &a.value).cmp(&(&b.name, &b.value)));
                if other_attributes == attributes {
                    identical.push(index);
                }
            }
        }
        if identical.len() >= 3 {
            self.active_formatting.remove(*identical.last().unwrap());
        }
        self.active_formatting.push(Some(id));
    }

    fn clear_active_to_marker(&mut self) {
        while let Some(entry) = self.active_formatting.pop() {
            if entry.is_none() {
                break;
            }
        }
    }

    fn reconstruct_active(&mut self) -> Result<(), Error> {
        let mut index = self.active_formatting.len();
        while index > 0 {
            let Some(id) = self.active_formatting[index - 1] else {
                break;
            };
            if self.open.contains(&id) {
                break;
            }
            index -= 1;
        }
        while index < self.active_formatting.len() {
            let id = self.active_formatting[index].expect("reconstruction starts after marker");
            let element = self.document.element(id).expect("formatting element");
            let tag = Tag {
                name: element.tag.clone(),
                attributes: element.attributes.clone(),
                self_closing: false,
            };
            let replacement = self.insert(&tag)?;
            self.active_formatting[index] = Some(replacement);
            index += 1;
        }
        Ok(())
    }

    fn in_scope(&self, id: NodeId) -> bool {
        for &entry in self.open.iter().rev() {
            if entry == id {
                return true;
            }
            if (self.namespace(entry) == Namespace::MathMl
                && matches!(
                    self.name(entry),
                    "mi" | "mo" | "mn" | "ms" | "mtext" | "annotation-xml"
                ))
                || (self.namespace(entry) == Namespace::Svg
                    && matches!(self.name(entry), "foreignObject" | "desc" | "title"))
            {
                return false;
            }
            if matches!(
                self.name(entry),
                "applet"
                    | "caption"
                    | "html"
                    | "marquee"
                    | "object"
                    | "table"
                    | "td"
                    | "template"
                    | "th"
            ) {
                return false;
            }
        }
        false
    }

    fn adoption_agency(&mut self, name: &str) {
        if self.name(self.current()) == name
            && !self.active_formatting.contains(&Some(self.current()))
        {
            self.open.pop();
            return;
        }
        for _ in 0..8 {
            let Some((formatting_index, formatting)) = self.last_active(name) else {
                self.generic_end(name);
                return;
            };
            let Some(open_index) = self.open.iter().position(|&id| id == formatting) else {
                self.active_formatting.remove(formatting_index);
                return;
            };
            if !self.in_scope(formatting) {
                return;
            }
            let Some(furthest) = self.open[open_index + 1..]
                .iter()
                .copied()
                .find(|&id| self.is_special_node(id))
            else {
                self.open.truncate(open_index);
                self.active_formatting.remove(formatting_index);
                return;
            };
            let common_ancestor = self.open[open_index - 1];
            let mut bookmark = formatting_index;
            let mut last_node = furthest;
            let mut stack_index = self.open.iter().position(|&id| id == furthest).unwrap();
            let mut inner_count = 0;
            while stack_index > open_index + 1 {
                stack_index -= 1;
                inner_count += 1;
                let node = self.open[stack_index];
                if inner_count > 3
                    && let Some(index) = self
                        .active_formatting
                        .iter()
                        .position(|&entry| entry == Some(node))
                {
                    self.active_formatting.remove(index);
                    if index < bookmark {
                        bookmark -= 1;
                    }
                }
                let Some(active_index) = self
                    .active_formatting
                    .iter()
                    .position(|&entry| entry == Some(node))
                else {
                    self.open.remove(stack_index);
                    continue;
                };
                let replacement = self.clone_element(node);
                self.active_formatting[active_index] = Some(replacement);
                self.open[stack_index] = replacement;
                if last_node == furthest {
                    bookmark = active_index + 1;
                }
                self.document.move_before(last_node, replacement, None);
                last_node = replacement;
            }
            let (parent, reference) = self.insertion_location_for(common_ancestor);
            self.document.move_before(last_node, parent, reference);

            let replacement = self.clone_element(formatting);
            let children = self.document.nodes[furthest].children.clone();
            for child in children {
                self.document.move_before(child, replacement, None);
            }
            self.document.move_before(replacement, furthest, None);

            let index = self
                .active_formatting
                .iter()
                .position(|&entry| entry == Some(formatting))
                .unwrap();
            self.active_formatting.remove(index);
            if index < bookmark {
                bookmark -= 1;
            }
            self.active_formatting.insert(bookmark, Some(replacement));
            let index = self.open.iter().position(|&id| id == formatting).unwrap();
            self.open.remove(index);
            let index = self.open.iter().position(|&id| id == furthest).unwrap();
            self.open.insert(index + 1, replacement);
        }
    }

    fn merge_attributes(&mut self, id: NodeId, attributes: &[Attribute]) {
        let NodeKind::Element(element) = &mut self.document.nodes[id].kind else {
            return;
        };
        for attribute in attributes {
            if element.attribute(&attribute.name).is_none() {
                element.attributes.push(attribute.clone());
            }
        }
    }

    fn text_element(
        &mut self,
        tag: &Tag,
        parent: NodeId,
        tokenizer: &mut Tokenizer,
    ) -> Result<(), Error> {
        self.insert_at(parent, tag)?;
        match tag.name.as_str() {
            "title" | "textarea" => tokenizer.enter_rcdata(&tag.name),
            "script" => tokenizer.enter_script_data(&tag.name),
            _ => tokenizer.enter_rawtext(&tag.name),
        }
        self.original_mode = self.mode;
        self.mode = InsertionMode::Text;
        Ok(())
    }

    fn head_start(
        &mut self,
        tag: &Tag,
        parent: NodeId,
        tokenizer: &mut Tokenizer,
    ) -> Result<bool, Error> {
        match tag.name.as_str() {
            "base" | "basefont" | "bgsound" | "link" | "meta" => {
                self.insert_at(parent, tag)?;
            }
            "title" | "style" | "script" | "noframes" => {
                self.text_element(tag, parent, tokenizer)?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn start_template(&mut self, tag: &Tag, parent: NodeId) -> Result<(), Error> {
        let id = self.insert_at(parent, tag)?;
        self.document.append(id, NodeKind::TemplateContent);
        self.active_formatting.push(None);
        self.template_modes.push(InsertionMode::InTemplate);
        self.mode = InsertionMode::InTemplate;
        Ok(())
    }

    fn close_template(&mut self) {
        if let Some(index) = self.position("template") {
            self.open.truncate(index);
            self.clear_active_to_marker();
            self.template_modes.pop();
            self.reset_mode();
        }
    }

    fn switch_template_mode(&mut self, mode: InsertionMode) {
        if let Some(current) = self.template_modes.last_mut() {
            *current = mode;
        }
        self.mode = mode;
    }

    fn process_in_template(
        &mut self,
        token: &Token,
        tokenizer: &mut Tokenizer,
    ) -> Result<bool, Error> {
        match token {
            Token::Character(_) | Token::Comment(_) | Token::Doctype(_) => {
                self.process_in_body(token, tokenizer, false)
            }
            Token::StartTag(tag)
                if matches!(
                    tag.name.as_str(),
                    "base"
                        | "basefont"
                        | "bgsound"
                        | "link"
                        | "meta"
                        | "noframes"
                        | "script"
                        | "style"
                        | "template"
                        | "title"
                ) =>
            {
                if tag.name == "template" {
                    self.start_template(tag, self.current())?;
                } else {
                    self.head_start(tag, self.current(), tokenizer)?;
                }
                Ok(false)
            }
            Token::StartTag(tag) => {
                let mode = match tag.name.as_str() {
                    "caption" | "colgroup" | "tbody" | "tfoot" | "thead" => InsertionMode::InTable,
                    "col" => InsertionMode::InColumnGroup,
                    "tr" => InsertionMode::InTableBody,
                    "td" | "th" => InsertionMode::InRow,
                    _ => InsertionMode::InBody,
                };
                self.switch_template_mode(mode);
                Ok(true)
            }
            Token::EndTag(_) | Token::Eof => Ok(false),
        }
    }

    fn position(&self, name: &str) -> Option<usize> {
        self.open
            .iter()
            .rposition(|&id| self.namespace(id) == Namespace::Html && self.name(id) == name)
    }

    fn in_table_scope(&self, name: &str) -> bool {
        for &id in self.open.iter().rev() {
            if self.namespace(id) != Namespace::Html {
                continue;
            }
            let current = self.name(id);
            if current == name {
                return true;
            }
            if matches!(current, "table" | "template" | "html") {
                return false;
            }
        }
        false
    }

    fn in_section_scope(&self) -> bool {
        ["tbody", "thead", "tfoot"]
            .iter()
            .any(|name| self.in_table_scope(name))
    }

    fn clear_to(&mut self, context: &[&str]) {
        while !context.contains(&self.name(self.current())) {
            self.open.pop();
        }
    }

    fn reset_mode(&mut self) {
        self.mode = self
            .open
            .iter()
            .rev()
            .find_map(|&id| match (self.namespace(id), self.name(id)) {
                (Namespace::Html, "td" | "th") => Some(InsertionMode::InCell),
                (Namespace::Html, "tr") => Some(InsertionMode::InRow),
                (Namespace::Html, "tbody" | "thead" | "tfoot") => Some(InsertionMode::InTableBody),
                (Namespace::Html, "caption") => Some(InsertionMode::InCaption),
                (Namespace::Html, "colgroup") => Some(InsertionMode::InColumnGroup),
                (Namespace::Html, "table") => Some(InsertionMode::InTable),
                (Namespace::Html, "template") => self.template_modes.last().copied(),
                (Namespace::Html, "body") => Some(InsertionMode::InBody),
                (Namespace::Html, "head") => Some(InsertionMode::InHead),
                _ => None,
            })
            .unwrap_or(if self.body.is_some() {
                InsertionMode::InBody
            } else {
                InsertionMode::AfterHead
            });
    }

    fn close_cell(&mut self) {
        if let Some(index) = self
            .open
            .iter()
            .rposition(|&id| matches!(self.name(id), "td" | "th"))
        {
            self.open.truncate(index);
            self.clear_active_to_marker();
            self.mode = InsertionMode::InRow;
        }
    }

    fn close_row(&mut self) {
        self.clear_to(&["tr", "template", "html"]);
        if self.name(self.current()) == "tr" {
            self.open.pop();
            self.mode = InsertionMode::InTableBody;
        }
    }

    fn close_section(&mut self) {
        self.clear_to(&["tbody", "thead", "tfoot", "template", "html"]);
        if matches!(self.name(self.current()), "tbody" | "thead" | "tfoot") {
            self.open.pop();
            self.mode = InsertionMode::InTable;
        }
    }

    fn generate_implied_end_tags(&mut self) {
        while matches!(
            self.name(self.current()),
            "dd" | "dt" | "li" | "optgroup" | "option" | "p" | "rb" | "rp" | "rt" | "rtc"
        ) {
            self.open.pop();
        }
    }

    fn generate_implied_end_tags_except(&mut self, except: &str) {
        while self.name(self.current()) != except
            && matches!(
                self.name(self.current()),
                "dd" | "dt" | "li" | "optgroup" | "option" | "p" | "rb" | "rp" | "rt" | "rtc"
            )
        {
            self.open.pop();
        }
    }

    fn in_scope_named(&self, name: &str) -> bool {
        self.position(name)
            .is_some_and(|index| self.in_scope(self.open[index]))
    }

    fn close_in_scope(&mut self, name: &str) {
        if self.in_scope_named(name) {
            self.generate_implied_end_tags_except(name);
            if let Some(index) = self.position(name) {
                self.open.truncate(index);
            }
        }
    }

    fn close_caption(&mut self) {
        self.generate_implied_end_tags();
        if let Some(index) = self.position("caption") {
            self.open.truncate(index);
            self.clear_active_to_marker();
            self.mode = InsertionMode::InTable;
        }
    }

    fn p_in_button_scope(&self) -> bool {
        for &id in self.open.iter().rev() {
            match self.name(id) {
                "p" => return true,
                "button" | "table" | "html" | "template" => return false,
                _ => {}
            }
        }
        false
    }

    fn close_p(&mut self) {
        if let Some(index) = self.position("p") {
            self.open.truncate(index);
        }
    }

    fn close_previous_li(&mut self) {
        for index in (0..self.open.len()).rev() {
            let name = self.name(self.open[index]);
            if name == "li" {
                self.open.truncate(index);
                return;
            }
            if self.is_special_node(self.open[index]) && !matches!(name, "address" | "div" | "p") {
                return;
            }
        }
    }

    fn close_block(&mut self, name: &str) {
        for index in (0..self.open.len()).rev() {
            let current = self.name(self.open[index]);
            if current == name {
                self.open.truncate(index);
                return;
            }
            if matches!(current, "body" | "html" | "table" | "template") {
                return;
            }
        }
    }

    fn generic_end(&mut self, name: &str) {
        for index in (0..self.open.len()).rev() {
            let current = self.name(self.open[index]);
            if current == name {
                self.open.truncate(index);
                return;
            }
            if self.is_special_node(self.open[index]) {
                return;
            }
        }
    }

    fn body_start(&mut self, tag: &Tag, tokenizer: &mut Tokenizer) -> Result<(), Error> {
        match tag.name.as_str() {
            "html" => {
                if self.position("template").is_none()
                    && let Some(id) = self.html
                {
                    self.merge_attributes(id, &tag.attributes);
                }
            }
            "body" => {
                if self.position("template").is_none()
                    && let Some(id) = self.body
                {
                    self.merge_attributes(id, &tag.attributes);
                }
            }
            "head" | "caption" | "col" | "colgroup" | "frame" | "tbody" | "td" | "tfoot" | "th"
            | "thead" | "tr" => {}
            "template" => self.start_template(tag, self.current())?,
            "base" | "basefont" | "bgsound" | "link" | "meta" | "title" | "style" | "script"
            | "noframes" => {
                self.head_start(tag, self.current(), tokenizer)?;
            }
            "textarea" | "xmp" | "iframe" | "noembed" => {
                if tag.name == "xmp" && self.p_in_button_scope() {
                    self.close_p();
                }
                if tag.name == "xmp" {
                    self.reconstruct_active()?;
                }
                self.text_element(tag, self.current(), tokenizer)?;
            }
            "a" => {
                if let Some((_, previous)) = self.last_active("a") {
                    self.adoption_agency("a");
                    self.active_formatting
                        .retain(|entry| *entry != Some(previous));
                    self.open.retain(|&id| id != previous);
                }
                self.reconstruct_active()?;
                let id = self.insert(tag)?;
                self.push_active(id);
            }
            "nobr" => {
                self.reconstruct_active()?;
                if let Some(index) = self.position("nobr")
                    && self.in_scope(self.open[index])
                {
                    self.adoption_agency("nobr");
                    self.reconstruct_active()?;
                }
                let id = self.insert(tag)?;
                self.push_active(id);
            }
            name if is_formatting(name) => {
                self.reconstruct_active()?;
                let id = self.insert(tag)?;
                self.push_active(id);
            }
            "applet" | "marquee" | "object" => {
                self.reconstruct_active()?;
                self.insert(tag)?;
                self.active_formatting.push(None);
            }
            "li" => {
                self.close_previous_li();
                if self.p_in_button_scope() {
                    self.close_p();
                }
                self.insert(tag)?;
            }
            "select" => {
                if self.in_scope_named("select") {
                    self.close_in_scope("select");
                } else {
                    self.reconstruct_active()?;
                    self.insert(tag)?;
                }
            }
            "option" => {
                if self.in_scope_named("select") {
                    self.generate_implied_end_tags_except("optgroup");
                } else if self.name(self.current()) == "option" {
                    self.open.pop();
                }
                self.reconstruct_active()?;
                self.insert(tag)?;
            }
            "optgroup" => {
                if self.in_scope_named("select") {
                    self.generate_implied_end_tags();
                } else if self.name(self.current()) == "option" {
                    self.open.pop();
                }
                self.reconstruct_active()?;
                self.insert(tag)?;
            }
            "input" => {
                if self.in_scope_named("select") {
                    self.close_in_scope("select");
                }
                self.reconstruct_active()?;
                self.insert(tag)?;
            }
            "hr" => {
                if self.p_in_button_scope() {
                    self.close_p();
                }
                if self.in_scope_named("select") {
                    self.generate_implied_end_tags();
                }
                self.insert(tag)?;
            }
            "math" | "svg" => {
                self.reconstruct_active()?;
                let namespace = if tag.name == "math" {
                    Namespace::MathMl
                } else {
                    Namespace::Svg
                };
                self.insert_foreign(tag, namespace)?;
            }
            "table" => {
                if self.p_in_button_scope() {
                    self.close_p();
                }
                self.insert(tag)?;
                self.mode = InsertionMode::InTable;
            }
            _ => {
                if closes_p(&tag.name) && self.p_in_button_scope() {
                    self.close_p();
                }
                if !closes_p(&tag.name) || (is_void(&tag.name) && tag.name != "hr") {
                    self.reconstruct_active()?;
                }
                self.insert(tag)?;
            }
        }
        Ok(())
    }

    fn body_end(&mut self, name: &str) -> Result<bool, Error> {
        match name {
            "body" => {
                if self.in_scope_named("body") {
                    self.mode = InsertionMode::AfterBody;
                }
            }
            "html" => {
                if self.in_scope_named("body") {
                    self.mode = InsertionMode::AfterBody;
                    return Ok(true);
                }
            }
            "p" => {
                if !self.p_in_button_scope() {
                    self.insert(&implied_tag("p"))?;
                }
                self.close_p();
            }
            "li" => self.close_block("li"),
            "select" | "option" | "optgroup" => self.close_in_scope(name),
            name if is_formatting(name) => self.adoption_agency(name),
            "applet" | "marquee" | "object" => {
                if let Some(index) = self.position(name)
                    && self.in_scope(self.open[index])
                {
                    self.generate_implied_end_tags();
                    self.open.truncate(index);
                    self.clear_active_to_marker();
                }
            }
            "br" => {
                self.reconstruct_active()?;
                self.insert(&implied_tag("br"))?;
            }
            name if closes_p(name) => self.close_block(name),
            _ => self.generic_end(name),
        }
        Ok(false)
    }

    fn process_in_body(
        &mut self,
        token: &Token,
        tokenizer: &mut Tokenizer,
        foster: bool,
    ) -> Result<bool, Error> {
        self.foster_parenting = foster;
        let result = match token {
            Token::Character(text) => {
                if !text.is_empty() {
                    self.reconstruct_active()?;
                }
                self.insert_text(text);
                Ok(false)
            }
            Token::Comment(data) => {
                self.insert_comment(data);
                Ok(false)
            }
            Token::Doctype(_) | Token::Eof => Ok(false),
            Token::StartTag(tag) => {
                self.body_start(tag, tokenizer)?;
                Ok(false)
            }
            Token::EndTag(tag) => self.body_end(&tag.name),
        };
        self.foster_parenting = false;
        result
    }

    fn process_in_table(
        &mut self,
        token: &Token,
        tokenizer: &mut Tokenizer,
    ) -> Result<bool, Error> {
        match token {
            Token::Character(_)
                if matches!(
                    self.name(self.current()),
                    "table" | "tbody" | "tfoot" | "thead" | "tr"
                ) =>
            {
                self.pending_table_text.clear();
                self.table_text_mode = self.mode;
                self.mode = InsertionMode::InTableText;
                Ok(true)
            }
            Token::Comment(data) => {
                self.insert_comment(data);
                Ok(false)
            }
            Token::Doctype(_) => Ok(false),
            Token::StartTag(tag) if tag.name == "caption" => {
                self.clear_to(&["table", "template", "html"]);
                self.insert(tag)?;
                self.active_formatting.push(None);
                self.mode = InsertionMode::InCaption;
                Ok(false)
            }
            Token::StartTag(tag) if tag.name == "colgroup" => {
                self.clear_to(&["table", "template", "html"]);
                self.insert(tag)?;
                self.mode = InsertionMode::InColumnGroup;
                Ok(false)
            }
            Token::StartTag(tag) if tag.name == "col" => {
                self.clear_to(&["table", "template", "html"]);
                self.insert(&implied_tag("colgroup"))?;
                self.mode = InsertionMode::InColumnGroup;
                Ok(true)
            }
            Token::StartTag(tag) if matches!(tag.name.as_str(), "tbody" | "thead" | "tfoot") => {
                self.clear_to(&["table", "template", "html"]);
                self.insert(tag)?;
                self.mode = InsertionMode::InTableBody;
                Ok(false)
            }
            Token::StartTag(tag) if matches!(tag.name.as_str(), "tr" | "td" | "th") => {
                self.clear_to(&["table", "template", "html"]);
                self.insert(&implied_tag("tbody"))?;
                self.mode = InsertionMode::InTableBody;
                Ok(true)
            }
            Token::StartTag(tag) if tag.name == "table" => {
                if self.in_table_scope("table") {
                    let index = self.position("table").unwrap();
                    self.open.truncate(index);
                    self.reset_mode();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Token::EndTag(tag) if tag.name == "table" => {
                if self.in_table_scope("table") {
                    let index = self.position("table").unwrap();
                    self.open.truncate(index);
                    self.reset_mode();
                }
                Ok(false)
            }
            Token::EndTag(tag)
                if matches!(
                    tag.name.as_str(),
                    "body"
                        | "caption"
                        | "col"
                        | "colgroup"
                        | "html"
                        | "tbody"
                        | "td"
                        | "tfoot"
                        | "th"
                        | "thead"
                        | "tr"
                ) =>
            {
                Ok(false)
            }
            Token::StartTag(tag) if matches!(tag.name.as_str(), "style" | "script") => {
                self.head_start(tag, self.current(), tokenizer)?;
                Ok(false)
            }
            Token::StartTag(tag) if tag.name == "template" => {
                self.start_template(tag, self.current())?;
                Ok(false)
            }
            Token::StartTag(tag)
                if tag.name == "input"
                    && tag.attributes.iter().any(|attr| {
                        attr.name == "type" && attr.value.eq_ignore_ascii_case("hidden")
                    }) =>
            {
                self.insert(tag)?;
                Ok(false)
            }
            Token::Eof => Ok(false),
            _ => self.process_in_body(token, tokenizer, true),
        }
    }

    fn process_in_caption(
        &mut self,
        token: &Token,
        tokenizer: &mut Tokenizer,
    ) -> Result<bool, Error> {
        match token {
            Token::EndTag(tag) if tag.name == "caption" => {
                if self.in_table_scope("caption") {
                    self.close_caption();
                }
                Ok(false)
            }
            Token::StartTag(tag)
                if matches!(
                    tag.name.as_str(),
                    "caption"
                        | "col"
                        | "colgroup"
                        | "tbody"
                        | "td"
                        | "tfoot"
                        | "th"
                        | "thead"
                        | "tr"
                ) =>
            {
                if self.in_table_scope("caption") {
                    self.close_caption();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Token::EndTag(tag) if tag.name == "table" => {
                if self.in_table_scope("caption") {
                    self.close_caption();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Token::EndTag(tag)
                if matches!(
                    tag.name.as_str(),
                    "body"
                        | "col"
                        | "colgroup"
                        | "html"
                        | "tbody"
                        | "td"
                        | "tfoot"
                        | "th"
                        | "thead"
                        | "tr"
                ) =>
            {
                Ok(false)
            }
            _ => self.process_in_body(token, tokenizer, false),
        }
    }

    fn process_in_column_group(
        &mut self,
        token: &Token,
        tokenizer: &mut Tokenizer,
    ) -> Result<bool, Error> {
        match token {
            Token::Character(text) if text.chars().all(html_space) => {
                self.insert_text(text);
                Ok(false)
            }
            Token::Comment(data) => {
                self.insert_comment(data);
                Ok(false)
            }
            Token::Doctype(_) => Ok(false),
            Token::StartTag(tag) if tag.name == "html" => {
                self.process_in_body(token, tokenizer, false)
            }
            Token::StartTag(tag) if tag.name == "col" => {
                self.insert(tag)?;
                Ok(false)
            }
            Token::EndTag(tag) if tag.name == "colgroup" => {
                if self.name(self.current()) == "colgroup" {
                    self.open.pop();
                    self.mode = InsertionMode::InTable;
                }
                Ok(false)
            }
            Token::EndTag(tag) if tag.name == "col" => Ok(false),
            Token::StartTag(tag) if tag.name == "template" => {
                self.start_template(tag, self.current())?;
                Ok(false)
            }
            Token::Eof => self.process_in_body(token, tokenizer, false),
            _ if self.name(self.current()) == "colgroup" => {
                self.open.pop();
                self.mode = InsertionMode::InTable;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn process_in_table_body(
        &mut self,
        token: &Token,
        tokenizer: &mut Tokenizer,
    ) -> Result<bool, Error> {
        match token {
            Token::StartTag(tag) if tag.name == "tr" => {
                self.clear_to(&["tbody", "thead", "tfoot", "template", "html"]);
                self.insert(tag)?;
                self.mode = InsertionMode::InRow;
                Ok(false)
            }
            Token::StartTag(tag) if matches!(tag.name.as_str(), "td" | "th") => {
                self.clear_to(&["tbody", "thead", "tfoot", "template", "html"]);
                self.insert(&implied_tag("tr"))?;
                self.mode = InsertionMode::InRow;
                Ok(true)
            }
            Token::EndTag(tag) if matches!(tag.name.as_str(), "tbody" | "thead" | "tfoot") => {
                if self.in_table_scope(&tag.name) {
                    self.close_section();
                }
                Ok(false)
            }
            Token::StartTag(tag)
                if matches!(
                    tag.name.as_str(),
                    "caption" | "col" | "colgroup" | "tbody" | "thead" | "tfoot"
                ) =>
            {
                if self.in_section_scope() {
                    self.close_section();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Token::EndTag(tag) if tag.name == "table" => {
                if self.in_section_scope() {
                    self.close_section();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Token::EndTag(tag)
                if matches!(
                    tag.name.as_str(),
                    "body" | "caption" | "col" | "colgroup" | "html" | "td" | "th" | "tr"
                ) =>
            {
                Ok(false)
            }
            _ => self.process_in_table(token, tokenizer),
        }
    }

    fn process_in_row(&mut self, token: &Token, tokenizer: &mut Tokenizer) -> Result<bool, Error> {
        match token {
            Token::StartTag(tag) if matches!(tag.name.as_str(), "td" | "th") => {
                self.clear_to(&["tr", "template", "html"]);
                self.insert(tag)?;
                self.active_formatting.push(None);
                self.mode = InsertionMode::InCell;
                Ok(false)
            }
            Token::EndTag(tag) if tag.name == "tr" => {
                if self.in_table_scope("tr") {
                    self.close_row();
                }
                Ok(false)
            }
            Token::StartTag(tag)
                if matches!(
                    tag.name.as_str(),
                    "caption" | "col" | "colgroup" | "tbody" | "thead" | "tfoot" | "tr"
                ) =>
            {
                if self.in_table_scope("tr") {
                    self.close_row();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Token::EndTag(tag) if tag.name == "table" => {
                if self.in_table_scope("tr") {
                    self.close_row();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Token::EndTag(tag) if matches!(tag.name.as_str(), "tbody" | "thead" | "tfoot") => {
                if self.in_table_scope(&tag.name) && self.in_table_scope("tr") {
                    self.close_row();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Token::EndTag(tag)
                if matches!(
                    tag.name.as_str(),
                    "body" | "caption" | "col" | "colgroup" | "html" | "td" | "th"
                ) =>
            {
                Ok(false)
            }
            _ => self.process_in_table(token, tokenizer),
        }
    }

    fn process_in_cell(&mut self, token: &Token, tokenizer: &mut Tokenizer) -> Result<bool, Error> {
        match token {
            Token::EndTag(tag) if matches!(tag.name.as_str(), "td" | "th") => {
                if self.in_table_scope(&tag.name) {
                    self.close_cell();
                }
                Ok(false)
            }
            Token::StartTag(tag)
                if matches!(
                    tag.name.as_str(),
                    "caption"
                        | "col"
                        | "colgroup"
                        | "tbody"
                        | "td"
                        | "tfoot"
                        | "th"
                        | "thead"
                        | "tr"
                ) =>
            {
                if self.in_table_scope("td") || self.in_table_scope("th") {
                    self.close_cell();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Token::EndTag(tag)
                if matches!(
                    tag.name.as_str(),
                    "table" | "tbody" | "tfoot" | "thead" | "tr"
                ) =>
            {
                if self.in_table_scope(&tag.name) {
                    self.close_cell();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Token::EndTag(tag)
                if matches!(
                    tag.name.as_str(),
                    "body" | "caption" | "col" | "colgroup" | "html"
                ) =>
            {
                Ok(false)
            }
            _ => self.process_in_body(token, tokenizer, false),
        }
    }

    fn consume(&mut self, mut token: Token, tokenizer: &mut Tokenizer) -> Result<(), Error> {
        for _ in 0..32 {
            if let Token::EndTag(tag) = &token
                && tag.name == "template"
                && (!self.use_foreign_rules(&token)
                    || self
                        .open
                        .iter()
                        .rev()
                        .find(|&&id| self.name(id).eq_ignore_ascii_case("template"))
                        .is_none_or(|&id| self.namespace(id) == Namespace::Html))
            {
                self.close_template();
                return Ok(());
            }
            if token == Token::Eof && self.mode != InsertionMode::InTableText {
                while self.position("template").is_some() {
                    self.close_template();
                }
            }
            if let Token::Character(text) = &token {
                let (spaces, rest) = split_initial_space(text);
                match self.mode {
                    InsertionMode::Initial
                    | InsertionMode::BeforeHtml
                    | InsertionMode::BeforeHead => {
                        if !spaces.is_empty() {
                            if rest.is_empty() {
                                return Ok(());
                            }
                            token = Token::Character(rest.into());
                            continue;
                        }
                    }
                    InsertionMode::InHead | InsertionMode::AfterHead if !spaces.is_empty() => {
                        self.append_text(self.current(), spaces);
                        if rest.is_empty() {
                            return Ok(());
                        }
                        token = Token::Character(rest.into());
                        continue;
                    }
                    InsertionMode::InColumnGroup if !spaces.is_empty() => {
                        self.insert_text(spaces);
                        if rest.is_empty() {
                            return Ok(());
                        }
                        token = Token::Character(rest.into());
                        continue;
                    }
                    _ => {}
                }
            }

            let foreign_result = if self.use_foreign_rules(&token) {
                self.process_foreign(&token)?
            } else {
                None
            };
            let reprocess = if let Some(reprocess) = foreign_result {
                reprocess
            } else {
                match self.mode {
                    InsertionMode::Initial => match &token {
                        Token::Comment(data) => {
                            self.document.append(0, NodeKind::Comment(data.clone()));
                            false
                        }
                        Token::Doctype(doctype) => {
                            self.document.append(0, NodeKind::Doctype(doctype.clone()));
                            self.mode = InsertionMode::BeforeHtml;
                            false
                        }
                        _ => {
                            self.mode = InsertionMode::BeforeHtml;
                            true
                        }
                    },
                    InsertionMode::BeforeHtml => match &token {
                        Token::Comment(data) => {
                            self.document.append(0, NodeKind::Comment(data.clone()));
                            false
                        }
                        Token::Doctype(_) => false,
                        Token::StartTag(tag) if tag.name == "html" => {
                            self.html = Some(self.insert(tag)?);
                            self.mode = InsertionMode::BeforeHead;
                            false
                        }
                        Token::EndTag(tag)
                            if !matches!(tag.name.as_str(), "head" | "body" | "html" | "br") =>
                        {
                            false
                        }
                        _ => {
                            self.html = Some(self.insert(&implied_tag("html"))?);
                            self.mode = InsertionMode::BeforeHead;
                            true
                        }
                    },
                    InsertionMode::BeforeHead => match &token {
                        Token::Comment(data) => {
                            self.insert_comment(data);
                            false
                        }
                        Token::Doctype(_) => false,
                        Token::StartTag(tag) if tag.name == "html" => {
                            self.merge_attributes(self.html.unwrap(), &tag.attributes);
                            false
                        }
                        Token::StartTag(tag) if tag.name == "head" => {
                            self.head = Some(self.insert(tag)?);
                            self.mode = InsertionMode::InHead;
                            false
                        }
                        Token::EndTag(tag)
                            if !matches!(tag.name.as_str(), "head" | "body" | "html" | "br") =>
                        {
                            false
                        }
                        _ => {
                            self.head = Some(self.insert(&implied_tag("head"))?);
                            self.mode = InsertionMode::InHead;
                            true
                        }
                    },
                    InsertionMode::InHead => match &token {
                        Token::Comment(data) => {
                            self.insert_comment(data);
                            false
                        }
                        Token::Doctype(_) => false,
                        Token::StartTag(tag) if tag.name == "html" => {
                            self.merge_attributes(self.html.unwrap(), &tag.attributes);
                            false
                        }
                        Token::StartTag(tag) if tag.name == "template" => {
                            self.start_template(tag, self.current())?;
                            false
                        }
                        Token::StartTag(tag)
                            if self.head_start(tag, self.current(), tokenizer)? =>
                        {
                            false
                        }
                        Token::EndTag(tag) if tag.name == "head" => {
                            self.open.pop();
                            self.mode = InsertionMode::AfterHead;
                            false
                        }
                        Token::EndTag(tag)
                            if !matches!(tag.name.as_str(), "body" | "html" | "br") =>
                        {
                            false
                        }
                        _ => {
                            self.open.pop();
                            self.mode = InsertionMode::AfterHead;
                            true
                        }
                    },
                    InsertionMode::AfterHead => match &token {
                        Token::Comment(data) => {
                            self.insert_comment(data);
                            false
                        }
                        Token::Doctype(_) => false,
                        Token::StartTag(tag) if tag.name == "html" => {
                            self.merge_attributes(self.html.unwrap(), &tag.attributes);
                            false
                        }
                        Token::StartTag(tag) if tag.name == "body" => {
                            self.body = Some(self.insert(tag)?);
                            self.mode = InsertionMode::InBody;
                            false
                        }
                        Token::StartTag(tag) if tag.name == "template" => {
                            self.start_template(tag, self.head.unwrap())?;
                            false
                        }
                        Token::StartTag(tag)
                            if self.head_start(tag, self.head.unwrap(), tokenizer)? =>
                        {
                            false
                        }
                        Token::EndTag(tag)
                            if !matches!(tag.name.as_str(), "body" | "html" | "br") =>
                        {
                            false
                        }
                        _ => {
                            self.body = Some(self.insert(&implied_tag("body"))?);
                            self.mode = InsertionMode::InBody;
                            true
                        }
                    },
                    InsertionMode::InBody => self.process_in_body(&token, tokenizer, false)?,
                    InsertionMode::InTable => self.process_in_table(&token, tokenizer)?,
                    InsertionMode::InCaption => self.process_in_caption(&token, tokenizer)?,
                    InsertionMode::InColumnGroup => {
                        self.process_in_column_group(&token, tokenizer)?
                    }
                    InsertionMode::InTableText => match &token {
                        Token::Character(text) => {
                            self.pending_table_text
                                .extend(text.chars().filter(|&c| c != '\0'));
                            false
                        }
                        _ => {
                            let pending = std::mem::take(&mut self.pending_table_text);
                            if pending.chars().all(html_space) {
                                self.append_text(self.current(), &pending);
                            } else {
                                self.process_in_body(&Token::Character(pending), tokenizer, true)?;
                            }
                            self.mode = self.table_text_mode;
                            true
                        }
                    },
                    InsertionMode::InTableBody => self.process_in_table_body(&token, tokenizer)?,
                    InsertionMode::InRow => self.process_in_row(&token, tokenizer)?,
                    InsertionMode::InCell => self.process_in_cell(&token, tokenizer)?,
                    InsertionMode::InTemplate => self.process_in_template(&token, tokenizer)?,
                    InsertionMode::Text => match &token {
                        Token::Character(text) => {
                            self.append_text(self.current(), text);
                            false
                        }
                        Token::EndTag(_) => {
                            self.open.pop();
                            self.mode = self.original_mode;
                            false
                        }
                        Token::Eof => {
                            self.open.pop();
                            self.mode = self.original_mode;
                            true
                        }
                        _ => false,
                    },
                    InsertionMode::AfterBody => match &token {
                        Token::Character(text) if text.chars().all(html_space) => {
                            self.append_text(self.current(), text);
                            false
                        }
                        Token::Comment(data) => {
                            self.document
                                .append(self.html.unwrap(), NodeKind::Comment(data.clone()));
                            false
                        }
                        Token::Doctype(_) => false,
                        Token::StartTag(tag) if tag.name == "html" => {
                            self.merge_attributes(self.html.unwrap(), &tag.attributes);
                            false
                        }
                        Token::EndTag(tag) if tag.name == "html" => {
                            self.mode = InsertionMode::AfterAfterBody;
                            false
                        }
                        Token::Eof => false,
                        _ => {
                            self.mode = InsertionMode::InBody;
                            true
                        }
                    },
                    InsertionMode::AfterAfterBody => match &token {
                        Token::Character(text) if text.chars().all(html_space) => {
                            self.append_text(self.current(), text);
                            false
                        }
                        Token::Comment(data) => {
                            self.document.append(0, NodeKind::Comment(data.clone()));
                            false
                        }
                        Token::Doctype(_) | Token::Eof => false,
                        Token::StartTag(tag) if tag.name == "html" => {
                            self.merge_attributes(self.html.unwrap(), &tag.attributes);
                            false
                        }
                        _ => {
                            self.mode = InsertionMode::InBody;
                            true
                        }
                    },
                }
            };
            if !reprocess {
                return Ok(());
            }
        }
        Err(Error::InvalidInput(
            "HTML insertion-mode loop exceeded".into(),
        ))
    }
}

pub fn parse(input: &str) -> Result<Document, Error> {
    if input.len() > 16 * 1024 * 1024 {
        return Err(Error::InvalidInput("HTML input exceeds 16 MiB".into()));
    }
    let mut tokenizer = Tokenizer::new(input);
    let mut builder = TreeBuilder::new();
    while let Some(token) = tokenizer.next_token() {
        let eof = token == Token::Eof;
        builder.consume(token, &mut tokenizer)?;
        if eof {
            break;
        }
    }
    Ok(builder.document)
}
