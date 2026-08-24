//! `@font-face` descriptor parsing: the family name, style, weight
//! (single value or range), and `src` url list a publisher declares for
//! an embedded face. Consumed by the publisher font loader at session
//! open — the cascade never sees these rules. Same intake philosophy as
//! the sheet parser: anything unreadable drops the rule, never errors.

use cssparser::{Parser, Token};

use super::model::{FontStyle, FontWeight};

/// One parsed `@font-face` rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFaceRule {
    /// The declared family name, as written (matched case-insensitively).
    pub family: String,
    pub style: FontStyle,
    /// Inclusive weight range this face serves; a single declared value
    /// is `(v, v)`, the absent descriptor defaults to `(400, 400)`.
    pub weight: (u16, u16),
    /// The `src` url list in author order, verbatim (relative to the
    /// declaring stylesheet; `local(...)` entries are dropped — there is
    /// no local font database to consult).
    pub sources: Vec<String>,
}

/// Parses one `@font-face` block's descriptors. `None` when the rule
/// lacks a family or any url source — nothing could ever match or load.
pub(super) fn parse_font_face_block(parser: &mut Parser<'_, '_>) -> Option<FontFaceRule> {
    let mut family: Option<String> = None;
    let mut style = FontStyle::Normal;
    let mut weight = (
        FontWeight::NORMAL.value(),
        FontWeight::NORMAL.value(),
    );
    let mut sources: Vec<String> = Vec::new();
    loop {
        let property = match parser.next() {
            Ok(Token::Ident(name)) => name.to_ascii_lowercase(),
            Ok(_) => {
                consume_rest(parser);
                continue;
            }
            Err(_) => break,
        };
        if !matches!(parser.next(), Ok(Token::Colon)) {
            consume_rest(parser);
            continue;
        }
        match property.as_str() {
            "font-family" => {
                family = parse_family(parser).or(family);
            }
            "font-style" => {
                if let Some(parsed) = parse_style(parser) {
                    style = parsed;
                }
            }
            "font-weight" => {
                if let Some(parsed) = parse_weight(parser) {
                    weight = parsed;
                }
            }
            "src" => {
                let parsed = parse_src(parser);
                if !parsed.is_empty() {
                    sources = parsed;
                }
            }
            _ => consume_rest(parser),
        }
    }
    let family = family?;
    if sources.is_empty() {
        return None;
    }
    Some(FontFaceRule {
        family,
        style,
        weight,
        sources,
    })
}

/// Consumes to (and including) the next top-level `;`, skipping nested
/// blocks wholesale.
fn consume_rest(parser: &mut Parser<'_, '_>) {
    while let Ok(token) = parser.next() {
        if matches!(token, Token::Semicolon) {
            break;
        }
    }
}

/// The descriptor's single family name: one quoted string, or an ident
/// sequence joined with spaces. Consumes through the `;`.
fn parse_family(parser: &mut Parser<'_, '_>) -> Option<String> {
    let mut words: Vec<String> = Vec::new();
    let mut quoted: Option<String> = None;
    loop {
        match parser.next() {
            Ok(Token::Ident(name)) => words.push(name.to_string()),
            Ok(Token::QuotedString(name)) => quoted = Some(name.to_string()),
            Ok(Token::Semicolon) => break,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    if let Some(name) = quoted {
        return Some(name);
    }
    if words.is_empty() {
        None
    } else {
        Some(words.join(" "))
    }
}

/// `normal` / `italic` / `oblique` (any angle folds to italic, like the
/// property parser). Consumes through the `;`.
fn parse_style(parser: &mut Parser<'_, '_>) -> Option<FontStyle> {
    let mut parsed = None;
    loop {
        match parser.next() {
            Ok(Token::Ident(name)) if parsed.is_none() => {
                parsed = match name.to_ascii_lowercase().as_str() {
                    "normal" => Some(FontStyle::Normal),
                    "italic" | "oblique" => Some(FontStyle::Italic),
                    _ => None,
                };
            }
            Ok(Token::Semicolon) | Err(_) => break,
            Ok(_) => {}
        }
    }
    parsed
}

/// One or two weight values (`400`, `bold`, `300 700`), each a number in
/// CSS's 1..=1000 or a keyword, yielding an ordered inclusive range.
/// Consumes through the `;`.
fn parse_weight(parser: &mut Parser<'_, '_>) -> Option<(u16, u16)> {
    let mut values: Vec<u16> = Vec::new();
    let mut poisoned = false;
    loop {
        match parser.next() {
            Ok(Token::Ident(name)) => match name.to_ascii_lowercase().as_str() {
                "normal" => values.push(FontWeight::NORMAL.value()),
                "bold" => values.push(FontWeight::BOLD.value()),
                _ => poisoned = true,
            },
            Ok(Token::Number { value, .. }) => {
                if value.is_finite() && (1.0..=1000.0).contains(value) {
                    values.push(value.round() as u16);
                } else {
                    poisoned = true;
                }
            }
            Ok(Token::Semicolon) | Err(_) => break,
            Ok(_) => poisoned = true,
        }
    }
    if poisoned || values.is_empty() || values.len() > 2 {
        return None;
    }
    let (a, b) = (values[0], *values.last().unwrap_or(&values[0]));
    Some((a.min(b), a.max(b)))
}

/// The `src` list's urls in author order: `url(...)` tokens (unquoted or
/// function form), with `local(...)`, `format(...)`, and `tech(...)`
/// annotations skipped. Consumes through the `;`.
fn parse_src(parser: &mut Parser<'_, '_>) -> Vec<String> {
    let mut sources = Vec::new();
    loop {
        match parser.next() {
            Ok(Token::UnquotedUrl(url)) => sources.push(url.to_string()),
            Ok(Token::Function(name)) => {
                let name = name.to_ascii_lowercase();
                // Every function's block must be consumed to move on.
                let inner: Result<Option<String>, cssparser::ParseError<'_, ()>> = parser
                    .parse_nested_block(|block| {
                        let mut url = None;
                        while let Ok(token) = block.next() {
                            if let (Token::QuotedString(s) | Token::UnquotedUrl(s), None) =
                                (token, url.as_ref())
                            {
                                url = Some(s.to_string());
                            }
                        }
                        Ok(url)
                    });
                if name == "url" {
                    if let Ok(Some(url)) = inner {
                        sources.push(url);
                    }
                }
            }
            Ok(Token::Semicolon) | Err(_) => break,
            Ok(_) => {} // commas, whitespace-shaped tokens
        }
    }
    sources
}
