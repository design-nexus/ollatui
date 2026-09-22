use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub fn width_of(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

pub fn truncate(s: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if width_of(s) <= width {
        return s.to_string();
    }
    if width == 1 {
        return "…".to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in s.chars() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + cw + 1 > width {
            break;
        }
        out.push(ch);
        used += cw;
    }
    out.push('…');
    out
}

/// Word-wrap `text`, preserving hard newlines. `width` is in terminal columns.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![];
    }
    let mut lines = Vec::new();
    for para in text.split('\n') {
        if para.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut line = String::new();
        let mut used = 0usize;
        for word in para.split(' ') {
            if word.is_empty() {
                if used > 0 && used + 1 <= width {
                    line.push(' ');
                    used += 1;
                }
                continue;
            }
            let word_width = width_of(word);
            if used == 0 {
                push_chunk(&mut lines, &mut line, &mut used, word, width);
            } else if used + 1 + word_width <= width {
                line.push(' ');
                line.push_str(word);
                used += 1 + word_width;
            } else {
                lines.push(std::mem::take(&mut line));
                used = 0;
                push_chunk(&mut lines, &mut line, &mut used, word, width);
            }
        }
        lines.push(std::mem::take(&mut line));
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn push_chunk(
    lines: &mut Vec<String>,
    line: &mut String,
    used: &mut usize,
    word: &str,
    width: usize,
) {
    if width_of(word) <= width {
        line.push_str(word);
        *used = width_of(word);
        return;
    }
    let mut buf = String::new();
    let mut buf_w = 0;
    for ch in word.chars() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
        if buf_w + cw > width && !buf.is_empty() {
            lines.push(std::mem::take(&mut buf));
            buf_w = 0;
        }
        buf.push(ch);
        buf_w += cw;
    }
    *line = buf;
    *used = buf_w;
}

/// Wrap keeping every character, including leading spaces. Used for code blocks.
pub fn hard_wrap(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![];
    }
    let mut lines = Vec::new();
    for para in text.split('\n') {
        if para.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut line = String::new();
        let mut used = 0;
        for ch in para.chars() {
            let cw = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
            if used + cw > width && !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                used = 0;
            }
            line.push(ch);
            used += cw;
        }
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

pub fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else if value >= 10.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub fn encode_form(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn encode_path_segment(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Pull `<think>` blocks out of streamed content. Unclosed tags count as thinking.
pub fn split_think(input: &str) -> (String, String) {
    let mut thinking = String::new();
    let mut body = String::new();
    let mut rest = input;
    while let Some(start) = rest.find("<think>") {
        body.push_str(&rest[..start]);
        rest = &rest[start + "<think>".len()..];
        if let Some(end) = rest.find("</think>") {
            thinking.push_str(&rest[..end]);
            rest = &rest[end + "</think>".len()..];
        } else {
            thinking.push_str(rest);
            rest = "";
            break;
        }
    }
    body.push_str(rest);
    (thinking, body)
}

pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn short_title(s: &str) -> String {
    let flat: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let flat = flat.trim();
    if flat.is_empty() {
        return "New chat".to_string();
    }
    truncate(flat, 48)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_on_words() {
        let lines = wrap("alpha beta gamma", 10);
        assert_eq!(lines, vec!["alpha beta".to_string(), "gamma".to_string()]);
    }

    #[test]
    fn keeps_blank_lines() {
        let lines = wrap("a\n\nb", 20);
        assert_eq!(lines, vec!["a".to_string(), String::new(), "b".to_string()]);
    }

    #[test]
    fn splits_think_blocks() {
        let (think, body) = split_think("before <think>hidden</think> after");
        assert_eq!(think, "hidden");
        assert_eq!(body, "before  after");
        let (think, body) = split_think("<think>still going");
        assert_eq!(think, "still going");
        assert!(body.is_empty());
    }

    #[test]
    fn formats_bytes() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(5 * 1024 * 1024), "5.0 MB");
    }
}
