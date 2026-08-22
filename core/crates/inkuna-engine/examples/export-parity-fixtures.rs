//! Writes every golden-roster fixture EPUB plus `manifest.json` into a
//! directory — the exact corpus the shells' on-device parity harness
//! (plan 02) imports and lays out. The harness runs exactly these
//! cases against the committed golden digests; it never invents its own
//! corpus.
//!
//! ```sh
//! cd core && cargo run -p inkuna-engine --example export-parity-fixtures \
//!     --features test_support -- <dir>
//! ```

use std::path::PathBuf;

use inkuna_engine::test_support::{golden_roster, golden_settings, GOLDEN_VIEWPORT_PT};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args()
        .nth(1)
        .ok_or("usage: export-parity-fixtures <dir>")?;
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(&dir)?;

    let settings = golden_settings();
    let mut entries = Vec::new();
    for fixture in golden_roster() {
        let file = format!("{}.epub", fixture.name);
        (fixture.build)().write(&dir.join(&file));
        // Hand-written JSON: tiny, fixed-shape, keeps the engine
        // dependency-lean (no serde).
        entries.push(format!(
            concat!(
                "  {{\n",
                "    \"file\": \"{file}\",\n",
                "    \"viewport\": {{ \"width\": {vw:?}, \"height\": {vh:?} }},\n",
                "    \"settings\": {{ \"reading_font\": \"{rf}\", \"reading_bold\": {rb}, ",
                "\"text_size_step\": {ts}, \"line_spacing\": {ls:?}, ",
                "\"letter_spacing\": {les:?}, \"word_spacing\": {ws:?}, ",
                "\"reading_margins\": {rm} }}\n",
                "  }}"
            ),
            file = file,
            vw = GOLDEN_VIEWPORT_PT.0,
            vh = GOLDEN_VIEWPORT_PT.1,
            rf = settings.reading_font,
            rb = settings.reading_bold,
            ts = settings.text_size_step,
            ls = settings.line_spacing,
            les = settings.letter_spacing,
            ws = settings.word_spacing,
            rm = settings.reading_margins,
        ));
    }
    std::fs::write(
        dir.join("manifest.json"),
        format!("[\n{}\n]\n", entries.join(",\n")),
    )?;
    println!(
        "wrote {} fixture EPUBs + manifest.json to {}",
        golden_roster().len(),
        dir.display()
    );
    Ok(())
}
