/// A text buffer with a byte cursor that always sits on a char boundary.
#[derive(Clone, Debug, Default)]
pub struct Field {
    pub value: String,
    pub cursor: usize,
}

impl Field {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        let cursor = value.len();
        Self { value, cursor }
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }

    pub fn insert(&mut self, ch: char) {
        self.value.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    pub fn insert_str(&mut self, s: &str) {
        self.value.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = prev_boundary(&self.value, self.cursor);
        self.value.replace_range(prev..self.cursor, "");
        self.cursor = prev;
    }

    pub fn delete(&mut self) {
        if self.cursor >= self.value.len() {
            return;
        }
        let next = next_boundary(&self.value, self.cursor);
        self.value.replace_range(self.cursor..next, "");
    }

    pub fn left(&mut self) {
        self.cursor = prev_boundary(&self.value, self.cursor);
    }

    pub fn right(&mut self) {
        self.cursor = next_boundary(&self.value, self.cursor);
    }

    pub fn home_line(&mut self) {
        self.cursor = line_start(&self.value, self.cursor);
    }

    pub fn end_line(&mut self) {
        self.cursor = line_end(&self.value, self.cursor);
    }

    pub fn clear_to_start(&mut self) {
        let start = line_start(&self.value, self.cursor);
        self.value.replace_range(start..self.cursor, "");
        self.cursor = start;
    }

    pub fn back_word(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let pre: Vec<char> = self.value[..self.cursor].chars().collect();
        let mut end = pre.len();
        while end > 0 && pre[end - 1].is_whitespace() {
            end -= 1;
        }
        while end > 0 && !pre[end - 1].is_whitespace() {
            end -= 1;
        }
        let new_pre: String = pre[..end].iter().collect();
        let rest = &self.value[self.cursor..];
        self.cursor = new_pre.len();
        self.value = format!("{new_pre}{rest}");
    }

    pub fn up(&mut self) {
        let (line, col) = self.line_col();
        if line == 0 {
            self.cursor = 0;
            return;
        }
        self.set_line_col(line - 1, col);
    }

    pub fn down(&mut self) {
        let (line, col) = self.line_col();
        self.set_line_col(line + 1, col);
    }

    fn line_col(&self) -> (usize, usize) {
        let pre = &self.value[..self.cursor];
        let line = pre.chars().filter(|c| *c == '\n').count();
        let col = pre.rsplit('\n').next().unwrap_or("").chars().count();
        (line, col)
    }

    fn set_line_col(&mut self, line: usize, col: usize) {
        let mut current = 0usize;
        let mut start = 0usize;
        for (idx, ch) in self.value.char_indices() {
            if ch == '\n' {
                if current == line {
                    let prefix: String = self.value[start..idx].chars().take(col).collect();
                    self.cursor = start + prefix.len();
                    return;
                }
                current += 1;
                start = idx + ch.len_utf8();
            }
        }
        if current == line {
            let prefix: String = self.value[start..].chars().take(col).collect();
            self.cursor = start + prefix.len();
        } else {
            self.cursor = self.value.len();
        }
    }
}

fn prev_boundary(s: &str, cursor: usize) -> usize {
    if cursor == 0 {
        return 0;
    }
    let mut i = cursor - 1;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn next_boundary(s: &str, cursor: usize) -> usize {
    if cursor >= s.len() {
        return s.len();
    }
    let mut i = cursor + 1;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

fn line_start(s: &str, cursor: usize) -> usize {
    s[..cursor].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

fn line_end(s: &str, cursor: usize) -> usize {
    s[cursor..]
        .find('\n')
        .map(|i| cursor + i)
        .unwrap_or(s.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_around_the_cursor() {
        let mut field = Field::new("ac");
        field.left();
        field.insert('b');
        assert_eq!(field.value, "abc");
        field.backspace();
        assert_eq!(field.value, "ac");
        field.back_word();
        assert_eq!(field.value, "c");
    }

    #[test]
    fn moves_between_lines() {
        let mut field = Field::new("hello\nworld");
        field.up();
        assert_eq!(&field.value[..field.cursor], "hello");
        field.home_line();
        assert_eq!(field.cursor, 0);
    }
}
