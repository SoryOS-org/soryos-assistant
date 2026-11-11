/// Normalize arbitrary LLM text into something Piper will pronounce cleanly.
/// Strips display-only punctuation and converts dashes/bullets into brief pauses.
pub fn normalize_for_tts(input: &str) -> String {
    // Replace obvious whitespace.
    let mut text = input.replace(['\n', '\r', '\t'], " ");

    // Targeted replacements for better prosody.
    for (from, to) in [
        ("-", ", "),
        ("-", ", "),
        (" - ", ", "),
        ("  ", ", "),
        ("", ", "),
        ("ú", ", "),
        (".", "..."),
        (" / ", ", "),
        (" \\ ", ", "),
    ] {
        text = text.replace(from, to);
    }

    // Strip brackets/quotes that Piper ignores and can hurt pacing.
    let mut cleaned = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '(' | ')' | '[' | ']' | '{' | '}' | '"' | '\'' | 'Ž' | 'Ż' => cleaned.push(' '),
            _ => cleaned.push(ch),
        }
    }

    // Collapse whitespace.
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}
