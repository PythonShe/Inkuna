//! Splits a paragraph's text into uniform items: one UBA bidi level and
//! one resolved script per item. Common/Inherited characters merge into
//! the adjacent real script — the preceding run wins; leading commons
//! join the following run.

use std::ops::Range;

use unicode_bidi::{BidiInfo, Level};
use unicode_script::{Script, UnicodeScript};

/// A maximal slice of the input that shapes as one unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Byte range into the input text.
    pub range: Range<usize>,
    pub script: Script,
    pub bidi_level: u8,
}

fn is_real(script: Script) -> bool {
    script != Script::Common && script != Script::Inherited
}

/// UBA levels (paragraph base level from `base_rtl`), then script
/// itemization. Empty input yields no items.
pub fn itemize(text: &str, base_rtl: bool) -> Vec<Item> {
    if text.is_empty() {
        return Vec::new();
    }
    let base = if base_rtl { Level::rtl() } else { Level::ltr() };
    let bidi = BidiInfo::new(text, Some(base));

    let mut items: Vec<Item> = Vec::new();
    let mut run_start = 0usize;
    // The open run's state; `script` stays None over leading commons.
    let mut run_level: Option<u8> = None;
    let mut run_script: Option<Script> = None;

    for (i, ch) in text.char_indices() {
        let level = bidi.levels.get(i).map_or(0, |l| l.number());
        let script = ch.script();
        let real = is_real(script);
        match run_level {
            None => {
                run_level = Some(level);
                run_script = real.then_some(script);
            }
            Some(open_level) => {
                let script_breaks = real && run_script.is_some_and(|s| s != script);
                if level != open_level || script_breaks {
                    items.push(Item {
                        range: run_start..i,
                        script: run_script.unwrap_or(Script::Common),
                        bidi_level: open_level,
                    });
                    run_start = i;
                    run_level = Some(level);
                    run_script = real.then_some(script);
                } else if real && run_script.is_none() {
                    // Leading commons join the following real script.
                    run_script = Some(script);
                }
            }
        }
    }
    if let Some(open_level) = run_level {
        items.push(Item {
            range: run_start..text.len(),
            script: run_script.unwrap_or(Script::Common),
            bidi_level: open_level,
        });
    }
    items
}
