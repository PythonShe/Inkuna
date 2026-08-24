//! Opinionated CSS intake: cssparser tokenization, retaining only the
//! selector shapes and declarations the engine honors. Everything else —
//! any other selector, property, at-rule, or parse error — is silently
//! skipped, never an error: the engine's look is settings-owned, and a
//! sheet it cannot read simply styles nothing.

use std::sync::Arc;

use cssparser::{ParseError, Parser, ParserInput, Token};

use super::fontface::{parse_font_face_block, FontFaceRule};
use super::model::{Direction, FontStyle, FontWeight, RubyPosition, TextAlign, WritingMode};
use crate::dom::ElementName;

/// One parsed sheet: only the retained rules, in source order.
#[derive(Debug, Default)]
pub struct Stylesheet {
    pub(crate) rules: Vec<Rule>,
    /// The sheet's `@font-face` rules, in source order — the publisher
    /// font loader consumes these; the cascade never sees them.
    pub(crate) font_faces: Vec<FontFaceRule>,
}

impl Stylesheet {
    /// The sheet's `@font-face` rules, in source order.
    pub fn font_faces(&self) -> &[FontFaceRule] {
        &self.font_faces
    }
}

/// One name in a `font-family` stack, in author order. Generic keywords
/// are folded at parse time; every other name — quoted or not — stays a
/// [`FamilyName::Named`] matched case-insensitively against `@font-face`
/// families later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FamilyName {
    Named(String),
    Serif,
    SansSerif,
    Monospace,
}

/// A retained `(selector, honored declarations)` pair.
#[derive(Debug)]
pub(crate) struct Rule {
    pub(crate) selector: Selector,
    pub(crate) declarations: Vec<Declaration>,
}

/// A descendant-combinator chain of simple selectors, ancestor-first;
/// the last part matches the target element.
#[derive(Debug)]
pub(crate) struct Selector {
    pub(crate) parts: Vec<SimpleSelector>,
    /// id 100 / class 10 / type 1.
    pub(crate) specificity: u32,
}

#[derive(Debug)]
pub(crate) enum SimpleSelector {
    Type(ElementName),
    Class(String),
    Id(String),
}

/// One honored declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Declaration {
    /// The element's `font-family` stack. Only consulted when the
    /// reader's font setting is `publisher`; every other setting owns
    /// the typography and ignores it. `Arc` because one rule's stack is
    /// shared by every element it matches.
    FontFamily(Arc<[FamilyName]>),
    /// Only meaningful on `html`/`body` rules; the cascade ignores it
    /// elsewhere.
    WritingMode(WritingMode),
    Direction(Direction),
    FontStyle(FontStyle),
    FontWeight(FontWeightValue),
    TextAlign(TextAlign),
    RubyPosition(RubyPosition),
    /// `display: none` — the only `display` value honored.
    DisplayNone,
}

/// A parsed `font-weight` value. `bolder`/`lighter` stay symbolic here
/// because CSS resolves them against the INHERITED weight, which only
/// the cascade knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FontWeightValue {
    Absolute(FontWeight),
    Bolder,
    Lighter,
}

/// Parses one stylesheet, keeping only what the engine honors. Total CSS
/// input per resource is capped by the caller at the DOM module's
/// `MAX_STYLESHEET_BYTES` (whole sheets drop from the END of the cascade
/// list when the linked+inline sum exceeds it).
pub fn parse_sheet(css: &str) -> Stylesheet {
    let mut input = ParserInput::new(css);
    let mut parser = Parser::new(&mut input);
    let mut sheet = Stylesheet::default();

    // One qualified rule at a time: collect the prelude's selector list,
    // then its block. Anything unreadable poisons only its own rule.
    let mut selectors: Vec<Option<Selector>> = Vec::new();
    let mut current: Option<Selector> = Some(Selector {
        parts: Vec::new(),
        specificity: 0,
    });
    // A simple selector was just completed; another part token without
    // whitespace in between would form an unsupported compound.
    let mut after_part = false;
    // Inside an at-rule's prelude: `Some(is_font_face)`. A `@font-face`
    // block is descended into for its descriptors; every other at-rule
    // (`@media` included, so conditional `@font-face` is not collected)
    // drops to its `;` or is skipped block-wholesale.
    let mut at_rule: Option<bool> = None;

    loop {
        let token = match parser.next_including_whitespace() {
            Ok(token) => token.clone(),
            Err(_) => break, // end of input
        };
        if let Some(is_font_face) = at_rule {
            match token {
                Token::Semicolon | Token::CurlyBracketBlock => {
                    if is_font_face && matches!(token, Token::CurlyBracketBlock) {
                        let parsed = parser.parse_nested_block(|block| {
                            Ok::<_, ParseError<'_, ()>>(parse_font_face_block(block))
                        });
                        if let Ok(Some(rule)) = parsed {
                            sheet.font_faces.push(rule);
                        }
                    }
                    at_rule = None;
                    selectors.clear();
                    current = fresh_selector();
                    after_part = false;
                }
                _ => {}
            }
            continue;
        }
        match token {
            Token::WhiteSpace(_) => after_part = false,
            Token::Comma => {
                selectors.push(current.take().filter(|s| !s.parts.is_empty()));
                current = fresh_selector();
                after_part = false;
            }
            Token::CurlyBracketBlock => {
                selectors.push(current.take().filter(|s| !s.parts.is_empty()));
                let declarations = parser
                    .parse_nested_block(|block| parse_declarations_inner(block))
                    .unwrap_or_default();
                if !declarations.is_empty() {
                    for selector in selectors.drain(..).flatten() {
                        sheet.rules.push(Rule {
                            selector,
                            declarations: declarations.clone(),
                        });
                    }
                }
                selectors.clear();
                current = fresh_selector();
                after_part = false;
            }
            Token::AtKeyword(name) => {
                at_rule = Some(name.eq_ignore_ascii_case("font-face"));
            }
            Token::Ident(name) => {
                push_part(
                    &mut current,
                    &mut after_part,
                    SimpleSelector::Type(ElementName::from_tag(&name.to_ascii_lowercase())),
                    1,
                );
            }
            Token::Delim('.') => {
                // The class name must follow immediately.
                match parser.next_including_whitespace() {
                    Ok(Token::Ident(name)) => {
                        let name = name.to_string();
                        push_part(
                            &mut current,
                            &mut after_part,
                            SimpleSelector::Class(name),
                            10,
                        );
                    }
                    _ => current = None,
                }
            }
            Token::IDHash(name) => {
                let name = name.to_string();
                push_part(&mut current, &mut after_part, SimpleSelector::Id(name), 100);
            }
            // `>`, `+`, `~`, `*`, `[attr]`, `:pseudo`, quoted strings…
            // all unsupported: poison the selector, keep scanning to the
            // rule's block so recovery stays per-rule.
            _ => current = None,
        }
    }
    sheet
}

/// Applies the per-resource total-CSS budget documented on
/// [`parse_sheet`]: the longest prefix of `css_texts` whose byte sum fits
/// `max_total` (the DOM module's `MAX_STYLESHEET_BYTES`). Whole sheets
/// drop from the END of the cascade list; a kept sheet is never cut
/// mid-text. Callers use this instead of re-implementing the cap.
pub fn cap_sheet_sources<'s, 'a>(css_texts: &'s [&'a str], max_total: usize) -> &'s [&'a str] {
    let mut total = 0usize;
    for (kept, css) in css_texts.iter().enumerate() {
        total = total.saturating_add(css.len());
        if total > max_total {
            return &css_texts[..kept];
        }
    }
    css_texts
}

/// Parses a declaration list (an inline `style` attribute body).
pub(crate) fn parse_declarations(css: &str) -> Vec<Declaration> {
    let mut input = ParserInput::new(css);
    let mut parser = Parser::new(&mut input);
    parse_declarations_inner(&mut parser).unwrap_or_default()
}

fn fresh_selector() -> Option<Selector> {
    Some(Selector {
        parts: Vec::new(),
        specificity: 0,
    })
}

/// Appends a simple selector unless it would form an unsupported
/// compound (`p.note`), which poisons the whole selector.
fn push_part(
    current: &mut Option<Selector>,
    after_part: &mut bool,
    part: SimpleSelector,
    weight: u32,
) {
    if *after_part {
        *current = None;
        return;
    }
    if let Some(selector) = current {
        selector.parts.push(part);
        selector.specificity += weight;
    }
    *after_part = true;
}

/// The declaration-list parser shared by rule blocks and inline styles.
/// Infallible by design; the error type exists only to satisfy cssparser.
fn parse_declarations_inner<'i>(
    parser: &mut Parser<'i, '_>,
) -> Result<Vec<Declaration>, ParseError<'i, ()>> {
    let mut declarations = Vec::new();
    loop {
        let property = match parser.next() {
            Ok(Token::Ident(name)) => name.to_ascii_lowercase(),
            Ok(_) => {
                consume_declaration_rest(parser);
                continue;
            }
            Err(_) => break,
        };
        if !matches!(parser.next(), Ok(Token::Colon)) {
            consume_declaration_rest(parser);
            continue;
        }
        // `font-family` is the one list-valued property honored; its
        // parser consumes the whole declaration itself.
        if property == "font-family" {
            if let Some(stack) = parse_family_list(parser) {
                declarations.push(Declaration::FontFamily(stack));
            }
            continue;
        }
        // The first meaningful value token decides; the rest of the
        // declaration (`!important` included) is consumed and ignored.
        let value = parser.next().ok().cloned();
        consume_declaration_rest(parser);
        if let Some(declaration) = map_declaration(&property, value.as_ref()) {
            declarations.push(declaration);
        }
    }
    Ok(declarations)
}

/// Consumes tokens up to and including the next top-level `;` (or the
/// block's end), skipping nested blocks wholesale.
fn consume_declaration_rest(parser: &mut Parser<'_, '_>) {
    while let Ok(token) = parser.next() {
        if matches!(token, Token::Semicolon) {
            break;
        }
    }
}

/// Parses a `font-family` value list, consuming the declaration through
/// its `;`. Comma-separated names: a quoted string is one complete name;
/// consecutive idents join with single spaces (`Times New Roman`); a
/// single unquoted generic keyword folds to its variant (`serif`,
/// `sans-serif`, `monospace` — the roster the engine can serve; other
/// generics stay `Named` and simply never match). Any other value token
/// invalidates the declaration, browser-style; `!important` is ignored.
/// An empty result is `None`.
fn parse_family_list(parser: &mut Parser<'_, '_>) -> Option<Arc<[FamilyName]>> {
    let mut names: Vec<FamilyName> = Vec::new();
    // Unquoted idents accumulated since the last separator.
    let mut words: Vec<String> = Vec::new();
    let mut poisoned = false;
    let flush = |names: &mut Vec<FamilyName>, words: &mut Vec<String>| {
        if words.is_empty() {
            return;
        }
        let name = if words.len() == 1 {
            match words[0].to_ascii_lowercase().as_str() {
                "serif" => FamilyName::Serif,
                "sans-serif" => FamilyName::SansSerif,
                "monospace" => FamilyName::Monospace,
                _ => FamilyName::Named(words[0].clone()),
            }
        } else {
            FamilyName::Named(words.join(" "))
        };
        words.clear();
        names.push(name);
    };
    loop {
        match parser.next() {
            Ok(Token::Ident(name)) => words.push(name.to_string()),
            Ok(Token::QuotedString(name)) => {
                // A quoted name is complete on its own; adjacency with
                // idents (`"Foo" Bar`) is invalid CSS and poisons.
                if !words.is_empty() {
                    poisoned = true;
                }
                names.push(FamilyName::Named(name.to_string()));
            }
            Ok(Token::Comma) => flush(&mut names, &mut words),
            Ok(Token::Semicolon) => break,
            // `!important`: keep what was parsed, skip the rest.
            Ok(Token::Delim('!')) => {
                consume_declaration_rest(parser);
                break;
            }
            Ok(_) => poisoned = true,
            Err(_) => break, // end of the declaration list
        }
    }
    flush(&mut names, &mut words);
    if poisoned || names.is_empty() {
        return None;
    }
    Some(Arc::from(names))
}

/// Maps one `property: first-value` pair onto an honored declaration.
fn map_declaration(property: &str, value: Option<&Token<'_>>) -> Option<Declaration> {
    let ident = match value {
        Some(Token::Ident(name)) => Some(name.to_ascii_lowercase()),
        _ => None,
    };
    let ident = ident.as_deref();
    match property {
        "writing-mode" => match ident {
            // ONLY vertical-rl is honored. Every other value —
            // vertical-lr, sideways modes, legacy/vendor forms like
            // tb-rl, inherit — is skipped entirely, never retained as
            // horizontal, so it can never override an honored
            // vertical-rl elsewhere in the cascade.
            Some("vertical-rl") => Some(Declaration::WritingMode(WritingMode::VerticalRl)),
            _ => None,
        },
        "direction" => match ident {
            Some("ltr") => Some(Declaration::Direction(Direction::Ltr)),
            Some("rtl") => Some(Declaration::Direction(Direction::Rtl)),
            _ => None,
        },
        "font-style" => match ident {
            Some("normal") => Some(Declaration::FontStyle(FontStyle::Normal)),
            Some("italic" | "oblique") => Some(Declaration::FontStyle(FontStyle::Italic)),
            _ => None,
        },
        "font-weight" => match (ident, value) {
            (Some("normal"), _) => Some(Declaration::FontWeight(FontWeightValue::Absolute(
                FontWeight::NORMAL,
            ))),
            (Some("bold"), _) => Some(Declaration::FontWeight(FontWeightValue::Absolute(
                FontWeight::BOLD,
            ))),
            (Some("bolder"), _) => Some(Declaration::FontWeight(FontWeightValue::Bolder)),
            (Some("lighter"), _) => Some(Declaration::FontWeight(FontWeightValue::Lighter)),
            // Numbers outside [1, 1000] are invalid per css-fonts-4 and
            // drop the declaration, matching browser behavior (they must
            // not clamp into a different weight than the author wrote).
            // Fractional weights are legal; they round to the nearest
            // integer for the engine's u16 model.
            (None, Some(Token::Number { value, .. })) => {
                if value.is_finite() && (1.0..=1000.0).contains(value) {
                    Some(Declaration::FontWeight(FontWeightValue::Absolute(
                        FontWeight::new(value.round() as u16),
                    )))
                } else {
                    None
                }
            }
            _ => None,
        },
        "text-align" => match ident {
            Some("start" | "left") => Some(Declaration::TextAlign(TextAlign::Start)),
            Some("center") => Some(Declaration::TextAlign(TextAlign::Center)),
            Some("end" | "right") => Some(Declaration::TextAlign(TextAlign::End)),
            Some("justify") => Some(Declaration::TextAlign(TextAlign::Justify)),
            _ => None,
        },
        "ruby-position" => match ident {
            Some("over") => Some(Declaration::RubyPosition(RubyPosition::Over)),
            Some("under") => Some(Declaration::RubyPosition(RubyPosition::Under)),
            _ => None,
        },
        "display" => match ident {
            Some("none") => Some(Declaration::DisplayNone),
            _ => None, // every other display value is ignored
        },
        _ => None,
    }
}
