//! Style resolution: UA defaults < publisher sheets (specificity, then
//! source order) < inline `style` attributes. Infallible by design.

use super::model::{ComputedStyle, Direction, FontStyle, FontWeight, StyledDocument, WritingMode};
use super::sheet::{parse_declarations, Declaration, Selector, SimpleSelector, Stylesheet};
use crate::dom::{Document, ElementData, ElementName, NodeId, NodeKind};

/// Resolves every node's computed style against the publisher sheets, in
/// cascade order. Reader settings are NOT an input: they win the visual
/// properties later, downstream of this pass.
pub fn resolve<'d>(doc: &'d Document, sheets: &[Stylesheet]) -> StyledDocument<'d> {
    let mut resolver = Resolver {
        doc,
        sheets,
        styles: vec![ComputedStyle::default(); doc.nodes.len()],
        writing_mode: WritingMode::default(),
    };
    resolver.run();
    StyledDocument {
        doc,
        styles: resolver.styles,
        writing_mode: resolver.writing_mode,
    }
}

struct Resolver<'d, 's> {
    doc: &'d Document,
    sheets: &'s [Stylesheet],
    styles: Vec<ComputedStyle>,
    writing_mode: WritingMode,
}

impl Resolver<'_, '_> {
    /// Top-down resolution over an explicit worklist (no recursion, no
    /// stack risk from a deep arena); each entry carries the parent's
    /// computed style — the inheritable properties plus `display_none`.
    fn run(&mut self) {
        let doc = self.doc;
        let mut work = vec![(doc.root, ComputedStyle::default())];
        while let Some((id, inherited)) = work.pop() {
            let computed = match &doc.node(id).kind {
                // Text nodes render with their parent's style.
                NodeKind::Text(_) => inherited,
                NodeKind::Element(data) => self.compute(id, data, inherited),
            };
            self.styles[id.0 as usize] = computed;
            for &child in &doc.node(id).children {
                work.push((child, computed));
            }
        }
    }

    fn compute(
        &mut self,
        id: NodeId,
        data: &ElementData,
        inherited: ComputedStyle,
    ) -> ComputedStyle {
        let mut style = inherited; // inheritable props + display_none propagation

        // UA defaults. The dir attribute sits here: publisher `direction`
        // declarations override it.
        match data.name {
            ElementName::Em => style.font_style = FontStyle::Italic,
            ElementName::Strong
            | ElementName::H1
            | ElementName::H2
            | ElementName::H3
            | ElementName::H4
            | ElementName::H5
            | ElementName::H6 => style.font_weight = FontWeight::Bold,
            _ => {}
        }
        if let Some(rtl) = data.dir_rtl {
            style.direction = if rtl { Direction::Rtl } else { Direction::Ltr };
        }

        // Publisher rules, weakest first: specificity, then sheet order,
        // then rule order — so applying in sorted order lets the last
        // write win.
        let mut matched: Vec<(u32, usize, usize)> = Vec::new();
        for (sheet_at, sheet) in self.sheets.iter().enumerate() {
            for (rule_at, rule) in sheet.rules.iter().enumerate() {
                if self.selector_matches(&rule.selector, id, data) {
                    matched.push((rule.selector.specificity, sheet_at, rule_at));
                }
            }
        }
        matched.sort_unstable();
        let is_root_scope = matches!(data.name, ElementName::Html | ElementName::Body);
        for (_, sheet_at, rule_at) in matched {
            for declaration in &self.sheets[sheet_at].rules[rule_at].declarations {
                self.apply(&mut style, *declaration, is_root_scope);
            }
        }

        // Inline style beats every publisher rule.
        if let Some(inline) = &data.style_attr {
            for declaration in parse_declarations(inline) {
                self.apply(&mut style, declaration, is_root_scope);
            }
        }
        style
    }

    /// Applies one honored declaration. `writing-mode` never touches the
    /// per-node style: it is per-resource, honored only on `html`/`body`.
    fn apply(&mut self, style: &mut ComputedStyle, declaration: Declaration, is_root_scope: bool) {
        match declaration {
            Declaration::WritingMode(mode) => {
                if is_root_scope {
                    self.writing_mode = mode;
                }
            }
            Declaration::Direction(direction) => style.direction = direction,
            Declaration::FontStyle(font_style) => style.font_style = font_style,
            Declaration::FontWeight(weight) => style.font_weight = weight,
            Declaration::TextAlign(align) => style.text_align = align,
            Declaration::RubyPosition(position) => style.ruby_position = position,
            // display: none only ever hides; other display values were
            // dropped at the sheet parse, so nothing un-hides a subtree.
            Declaration::DisplayNone => style.display_none = true,
        }
    }

    /// Descendant matching: the last part must match the node itself,
    /// each earlier part some strict ancestor, in order.
    fn selector_matches(&self, selector: &Selector, id: NodeId, data: &ElementData) -> bool {
        let Some((target, ancestors)) = selector.parts.split_last() else {
            return false;
        };
        if !simple_matches(target, data) {
            return false;
        }
        let mut cursor = self.doc.node(id).parent;
        for part in ancestors.iter().rev() {
            loop {
                let Some(ancestor_id) = cursor else {
                    return false;
                };
                cursor = self.doc.node(ancestor_id).parent;
                if let Some(ancestor) = self.doc.element(ancestor_id) {
                    if simple_matches(part, ancestor) {
                        break;
                    }
                }
            }
        }
        true
    }
}

/// One simple selector against one element.
fn simple_matches(selector: &SimpleSelector, data: &ElementData) -> bool {
    match selector {
        SimpleSelector::Type(name) => data.name == *name,
        SimpleSelector::Class(class) => data
            .class
            .as_deref()
            .is_some_and(|list| list.split_ascii_whitespace().any(|word| word == class)),
        SimpleSelector::Id(id) => data.id.as_deref() == Some(id.as_str()),
    }
}
