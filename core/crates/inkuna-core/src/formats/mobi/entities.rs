//! Loss-tolerant HTML5 entity decoding for MOBI6 text and attributes.

pub(super) fn decode_entities(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find('&') {
        output.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        let Some(end) = tail
            .as_bytes()
            .iter()
            .take(65)
            .position(|byte| *byte == b';')
            .filter(|end| {
                !tail.as_bytes()[..*end]
                    .iter()
                    .any(|byte| byte.is_ascii_whitespace() || *byte == b'&')
            })
        else {
            output.push('&');
            rest = tail;
            continue;
        };
        let reference = &rest[start..start + end + 2];
        match quick_xml::escape::unescape(reference) {
            Ok(value) => output.push_str(&value),
            Err(_) => output.push_str(reference),
        }
        rest = &tail[end + 1..];
    }
    output.push_str(rest);
    output
}

#[cfg(test)]
#[path = "entities_tests.rs"]
mod tests;
