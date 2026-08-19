//! Paragraph shaping, scene breaks, XHTML escaping, and chapter sizing.

use quick_xml::escape::escape;

use super::chapters::is_cjk_dominant;

const MAX_CHAPTER_TEXT_BYTES: usize = 100 * 1024;

#[derive(Clone)]
pub(super) enum Block {
    Paragraph(String),
    Scene,
}

pub(super) fn paragraph_blocks(text: &str) -> Vec<Block> {
    // Hard-wrapped continuation lines join with a space in space-delimited
    // scripts and with nothing in CJK, where a space would land inside a
    // sentence. The choice is made once per chapter body: a minority-script
    // paragraph inherits the dominant script's joiner.
    let joiner = if is_cjk_dominant(text) { "" } else { " " };
    let lines: Vec<&str> = text.lines().collect();
    if lines.iter().any(|line| line.trim().is_empty()) {
        return blank_separated_blocks(&lines, joiner);
    }
    if lines.iter().any(|line| starts_indented(line)) {
        return indented_blocks(&lines, joiner);
    }
    if is_cjk_dominant(text) && lines.len() > 1 {
        return lines.iter().filter_map(|line| line_block(line)).collect();
    }
    unindented_blocks(&lines, joiner)
}

fn blank_separated_blocks(lines: &[&str], joiner: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut paragraph = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        if line.trim().is_empty() {
            push_paragraph(&mut blocks, &paragraph, joiner);
            paragraph.clear();
            let mut blanks = 0;
            while index < lines.len() && lines[index].trim().is_empty() {
                blanks += 1;
                index += 1;
            }
            if blanks >= 3 && !matches!(blocks.last(), Some(Block::Scene)) {
                blocks.push(Block::Scene);
            }
            continue;
        }
        if is_scene_line(line) {
            push_paragraph(&mut blocks, &paragraph, joiner);
            paragraph.clear();
            if !matches!(blocks.last(), Some(Block::Scene)) {
                blocks.push(Block::Scene);
            }
        } else {
            paragraph.push(line);
        }
        index += 1;
    }
    push_paragraph(&mut blocks, &paragraph, joiner);
    blocks
}

fn indented_blocks(lines: &[&str], joiner: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut paragraph = Vec::new();
    for line in lines {
        if is_scene_line(line) {
            push_paragraph(&mut blocks, &paragraph, joiner);
            paragraph.clear();
            if !matches!(blocks.last(), Some(Block::Scene)) {
                blocks.push(Block::Scene);
            }
        } else {
            if starts_indented(line) && !paragraph.is_empty() {
                push_paragraph(&mut blocks, &paragraph, joiner);
                paragraph.clear();
            }
            paragraph.push(*line);
        }
    }
    push_paragraph(&mut blocks, &paragraph, joiner);
    blocks
}

fn unindented_blocks(lines: &[&str], joiner: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut paragraph = Vec::new();
    for line in lines {
        if is_scene_line(line) {
            push_paragraph(&mut blocks, &paragraph, joiner);
            paragraph.clear();
            if !matches!(blocks.last(), Some(Block::Scene)) {
                blocks.push(Block::Scene);
            }
        } else {
            paragraph.push(*line);
        }
    }
    push_paragraph(&mut blocks, &paragraph, joiner);
    blocks
}

fn line_block(line: &str) -> Option<Block> {
    if is_scene_line(line) {
        Some(Block::Scene)
    } else {
        let paragraph = strip_indent(line).trim_end();
        (!paragraph.is_empty()).then(|| Block::Paragraph(paragraph.into()))
    }
}

fn push_paragraph(blocks: &mut Vec<Block>, lines: &[&str], joiner: &str) {
    let paragraph = lines
        .iter()
        .map(|line| strip_indent(line).trim_end())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(joiner);
    if !paragraph.is_empty() {
        blocks.push(Block::Paragraph(paragraph));
    }
}

pub(super) fn split_blocks(blocks: Vec<Block>) -> Vec<Vec<Block>> {
    let mut chunks = Vec::new();
    let mut chunk = Vec::new();
    let mut bytes = 0;
    for block in blocks {
        if let Block::Paragraph(text) = &block {
            if text.len() > MAX_CHAPTER_TEXT_BYTES {
                if !chunk.is_empty() {
                    chunks.push(std::mem::take(&mut chunk));
                    bytes = 0;
                }
                chunks.extend(
                    split_long_paragraph(text)
                        .into_iter()
                        .map(|part| vec![Block::Paragraph(part.to_string())]),
                );
                continue;
            }
        }
        let block_bytes = match &block {
            Block::Paragraph(text) => text.len(),
            Block::Scene => 0,
        };
        if !chunk.is_empty() && bytes + block_bytes > MAX_CHAPTER_TEXT_BYTES {
            chunks.push(std::mem::take(&mut chunk));
            bytes = 0;
        }
        bytes += block_bytes;
        chunk.push(block);
    }
    if !chunk.is_empty() {
        chunks.push(chunk);
    }
    chunks
}

fn split_long_paragraph(mut text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    while text.len() > MAX_CHAPTER_TEXT_BYTES {
        let split = floor_char_boundary(text, MAX_CHAPTER_TEXT_BYTES);
        let (part, rest) = text.split_at(split);
        parts.push(part);
        text = rest;
    }
    if !text.is_empty() {
        parts.push(text);
    }
    parts
}

pub(super) fn render_chapter(title: &str, blocks: &[Block]) -> String {
    let mut xhtml = format!("<h1>{}</h1>", escape(title));
    for block in blocks {
        match block {
            Block::Paragraph(text) => {
                xhtml.push_str("<p>");
                xhtml.push_str(&escape(text));
                xhtml.push_str("</p>");
            }
            Block::Scene => xhtml.push_str("<hr class=\"scene\"/>"),
        }
    }
    xhtml
}

pub(super) fn render_volume_break(title: &str) -> String {
    format!("<h2>{}</h2>", escape(title))
}

fn is_scene_line(line: &str) -> bool {
    let line = line.trim();
    let count = line.chars().count();
    (2..=20).contains(&count)
        && line
            .chars()
            .all(|ch| matches!(ch, '*' | '＊' | '✱' | '-' | '—' | '─' | '='))
}

fn starts_indented(line: &str) -> bool {
    line.starts_with([' ', '\t', '\u{3000}'])
}

fn strip_indent(line: &str) -> &str {
    line.trim_start_matches([' ', '\t', '\u{3000}'])
}

pub(super) fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}
