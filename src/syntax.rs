#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Text,
    Keyword,
    Type,
    String,
    Comment,
    Number,
    Punctuation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub text: String,
    pub kind: TokenKind,
}

pub fn tokenize(line: &str, ext: &str) -> Vec<Token> {
    match ext.to_lowercase().as_str() {
        "rs" => tokenize_rust(line),
        "py" => tokenize_python(line),
        "rb" | "rake" | "gemspec" | "ru" => tokenize_ruby(line),
        "js" | "jsx" | "ts" | "tsx" | "json" | "mjs" | "cjs" => tokenize_js_ts(line),
        "toml" => tokenize_toml(line),
        "yaml" | "yml" => tokenize_yaml(line),
        "html" | "htm" | "xml" | "erb" | "vue" | "svelte" => tokenize_html_xml(line),
        "css" | "scss" | "sass" | "less" => tokenize_css(line),
        "md" | "markdown" => tokenize_markdown(line),
        _ => tokenize_plain(line),
    }
}

/// Markdown: line-oriented highlighting (headings, quotes, list markers,
/// code fences) plus inline `code` and **bold** / *italic* spans.
fn tokenize_markdown(line: &str) -> Vec<Token> {
    let trimmed = line.trim_start();

    // Whole-line constructs
    if trimmed.starts_with('#') {
        return vec![Token {
            text: line.to_string(),
            kind: TokenKind::Keyword,
        }];
    }
    if trimmed.starts_with('>') {
        return vec![Token {
            text: line.to_string(),
            kind: TokenKind::Comment,
        }];
    }
    if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
        return vec![Token {
            text: line.to_string(),
            kind: TokenKind::Punctuation,
        }];
    }

    let mut tokens = Vec::new();
    let mut scanner = Scanner::new(line);

    // Leading list / quote markers ("- ", "* ", "+ ", "1. ")
    let indent_len = line.len() - trimmed.len();
    if indent_len > 0 {
        let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
        for _ in 0..indent.chars().count() {
            scanner.next();
        }
        tokens.push(Token {
            text: indent,
            kind: TokenKind::Text,
        });
    }
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
        let marker = scanner.next().map(|c| c.to_string()).unwrap_or_default();
        tokens.push(Token {
            text: marker,
            kind: TokenKind::Punctuation,
        });
    }

    // Inline spans
    while !scanner.is_eof() {
        // `code`
        if scanner.starts_with("`") {
            let mut code = String::new();
            code.push(scanner.next().unwrap_or('`'));
            while let Some(nc) = scanner.next() {
                code.push(nc);
                if nc == '`' {
                    break;
                }
            }
            tokens.push(Token {
                text: code,
                kind: TokenKind::String,
            });
            continue;
        }
        // **bold** / *italic* (also _..._)
        if scanner.starts_with("**") || scanner.starts_with("__") {
            let delim: String = scanner.chars[scanner.pos..scanner.pos + 2].iter().collect();
            let mut span = delim.clone();
            scanner.consume_str(&delim);
            while !scanner.is_eof() {
                if scanner.starts_with(&delim) {
                    span.push_str(&delim);
                    scanner.consume_str(&delim);
                    break;
                }
                if let Some(nc) = scanner.next() {
                    span.push(nc);
                }
            }
            tokens.push(Token {
                text: span,
                kind: TokenKind::Type,
            });
            continue;
        }
        // [link text](url)
        if scanner.starts_with("[") {
            let mut span = String::new();
            let mut ok = false;
            let start_pos = scanner.pos;
            while let Some(nc) = scanner.next() {
                span.push(nc);
                if nc == ')' {
                    ok = true;
                    break;
                }
                if nc == '\n' || span.len() > 200 {
                    break;
                }
            }
            if ok && span.contains("](") {
                tokens.push(Token {
                    text: span,
                    kind: TokenKind::Keyword,
                });
                continue;
            }
            // Not a link: rewind and emit the bracket as plain text
            scanner.pos = start_pos + 1;
            tokens.push(Token {
                text: "[".to_string(),
                kind: TokenKind::Text,
            });
            continue;
        }
        // Plain run until the next special char
        let mut text = String::new();
        while let Some(nc) = scanner.peek() {
            if nc == '`' || nc == '[' || scanner.starts_with("**") || scanner.starts_with("__") {
                break;
            }
            text.push(nc);
            scanner.next();
        }
        if text.is_empty()
            && let Some(nc) = scanner.next()
        {
            text.push(nc);
        }
        tokens.push(Token {
            text,
            kind: TokenKind::Text,
        });
    }

    tokens
}

struct Scanner {
    chars: Vec<char>,
    pos: usize,
}

impl Scanner {
    fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
        }
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.chars.len()
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<char> {
        if self.pos < self.chars.len() {
            let c = self.chars[self.pos];
            self.pos += 1;
            Some(c)
        } else {
            None
        }
    }

    fn starts_with(&self, s: &str) -> bool {
        let mut s_iter = s.chars();
        for &c in &self.chars[self.pos..] {
            match s_iter.next() {
                Some(sc) if sc == c => continue,
                Some(_) => return false,
                None => return true,
            }
        }
        s_iter.next().is_none()
    }

    fn consume_str(&mut self, s: &str) {
        self.pos = (self.pos + s.chars().count()).min(self.chars.len());
    }
}

fn tokenize_plain(line: &str) -> Vec<Token> {
    let mut scanner = Scanner::new(line);
    let mut tokens = Vec::new();
    let mut current = String::new();

    while let Some(c) = scanner.peek() {
        if c.is_ascii_punctuation() && c != '_' {
            if !current.is_empty() {
                tokens.push(Token {
                    text: current.clone(),
                    kind: TokenKind::Text,
                });
                current.clear();
            }
            scanner.next();
            tokens.push(Token {
                text: c.to_string(),
                kind: TokenKind::Punctuation,
            });
        } else {
            current.push(c);
            scanner.next();
        }
    }

    if !current.is_empty() {
        tokens.push(Token {
            text: current,
            kind: TokenKind::Text,
        });
    }

    tokens
}

fn tokenize_rust(line: &str) -> Vec<Token> {
    let mut scanner = Scanner::new(line);
    let mut tokens = Vec::new();

    while !scanner.is_eof() {
        // 1. Whitespace
        if let Some(c) = scanner.peek()
            && c.is_whitespace()
        {
            let mut space = String::new();
            while let Some(nc) = scanner.peek() {
                if nc.is_whitespace() {
                    space.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }
            tokens.push(Token {
                text: space,
                kind: TokenKind::Text,
            });
            continue;
        }

        // 2. Comments
        if scanner.starts_with("//") {
            let remaining: String = scanner.chars[scanner.pos..].iter().collect();
            tokens.push(Token {
                text: remaining,
                kind: TokenKind::Comment,
            });
            break;
        }
        if scanner.starts_with("/*") {
            let mut comment = String::new();
            comment.push_str("/*");
            scanner.consume_str("/*");
            while let Some(nc) = scanner.peek() {
                if scanner.starts_with("*/") {
                    comment.push_str("*/");
                    scanner.consume_str("*/");
                    break;
                }
                comment.push(nc);
                scanner.next();
            }
            tokens.push(Token {
                text: comment,
                kind: TokenKind::Comment,
            });
            continue;
        }

        // 3. String literals
        if let Some(c) = scanner.peek()
            && c == '"'
        {
            let mut string_val = String::new();
            if let Some(ch) = scanner.next() {
                string_val.push(ch); // '"'
            }
            let mut escaped = false;
            while let Some(nc) = scanner.next() {
                string_val.push(nc);
                if escaped {
                    escaped = false;
                } else if nc == '\\' {
                    escaped = true;
                } else if nc == '"' {
                    break;
                }
            }
            tokens.push(Token {
                text: string_val,
                kind: TokenKind::String,
            });
            continue;
        }

        // 4. Character literals
        if let Some(c) = scanner.peek()
            && c == '\''
        {
            let mut char_val = String::new();
            if let Some(ch) = scanner.next() {
                char_val.push(ch); // '\''
            }
            let mut escaped = false;
            while let Some(nc) = scanner.next() {
                char_val.push(nc);
                if escaped {
                    escaped = false;
                } else if nc == '\\' {
                    escaped = true;
                } else if nc == '\'' {
                    break;
                }
            }
            tokens.push(Token {
                text: char_val,
                kind: TokenKind::String,
            });
            continue;
        }

        // 5. Identifiers
        if let Some(c) = scanner.peek()
            && (c.is_alphabetic() || c == '_')
        {
            let mut ident = String::new();
            while let Some(nc) = scanner.peek() {
                if nc.is_alphanumeric() || nc == '_' {
                    ident.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }

            let kind = if is_rust_keyword(&ident) {
                TokenKind::Keyword
            } else if is_rust_type(&ident) {
                TokenKind::Type
            } else {
                TokenKind::Text
            };

            tokens.push(Token { text: ident, kind });
            continue;
        }

        // 6. Number literals
        if let Some(c) = scanner.peek()
            && c.is_ascii_digit()
        {
            let mut num = String::new();
            while let Some(nc) = scanner.peek() {
                if nc.is_alphanumeric() || nc == '.' || nc == '_' {
                    num.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }
            tokens.push(Token {
                text: num,
                kind: TokenKind::Number,
            });
            continue;
        }

        // 7. Punctuation
        if let Some(c) = scanner.peek()
            && c.is_ascii_punctuation()
        {
            scanner.next();
            tokens.push(Token {
                text: c.to_string(),
                kind: TokenKind::Punctuation,
            });
            continue;
        }

        // 8. Fallback
        if let Some(c) = scanner.next() {
            tokens.push(Token {
                text: c.to_string(),
                kind: TokenKind::Text,
            });
        }
    }

    tokens
}

fn is_rust_keyword(s: &str) -> bool {
    matches!(
        s,
        "fn" | "let"
            | "mut"
            | "impl"
            | "struct"
            | "enum"
            | "use"
            | "pub"
            | "mod"
            | "crate"
            | "self"
            | "Self"
            | "match"
            | "if"
            | "else"
            | "loop"
            | "while"
            | "for"
            | "in"
            | "return"
            | "break"
            | "continue"
            | "as"
            | "async"
            | "await"
            | "dyn"
            | "ref"
            | "move"
            | "where"
            | "trait"
            | "type"
            | "const"
            | "static"
            | "unsafe"
            | "extern"
            | "true"
            | "false"
    )
}

fn is_rust_type(s: &str) -> bool {
    if matches!(
        s,
        "bool"
            | "char"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "f32"
            | "f64"
            | "str"
            | "Option"
            | "Result"
            | "String"
            | "Vec"
            | "Box"
            | "Rc"
            | "Arc"
    ) {
        return true;
    }
    if let Some(first) = s.chars().next() {
        first.is_uppercase()
    } else {
        false
    }
}

fn tokenize_python(line: &str) -> Vec<Token> {
    let mut scanner = Scanner::new(line);
    let mut tokens = Vec::new();

    while !scanner.is_eof() {
        // 1. Whitespace
        if let Some(c) = scanner.peek()
            && c.is_whitespace()
        {
            let mut space = String::new();
            while let Some(nc) = scanner.peek() {
                if nc.is_whitespace() {
                    space.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }
            tokens.push(Token {
                text: space,
                kind: TokenKind::Text,
            });
            continue;
        }

        // 2. Comments
        if scanner.starts_with("#") {
            let remaining: String = scanner.chars[scanner.pos..].iter().collect();
            tokens.push(Token {
                text: remaining,
                kind: TokenKind::Comment,
            });
            break;
        }

        // 3. String literals (including triple quotes if contained within the line)
        if scanner.starts_with("\"\"\"") {
            let mut string_val = String::new();
            string_val.push_str("\"\"\"");
            scanner.consume_str("\"\"\"");
            // Find closing """
            while let Some(nc) = scanner.peek() {
                if scanner.starts_with("\"\"\"") {
                    string_val.push_str("\"\"\"");
                    scanner.consume_str("\"\"\"");
                    break;
                }
                string_val.push(nc);
                scanner.next();
            }
            tokens.push(Token {
                text: string_val,
                kind: TokenKind::String,
            });
            continue;
        }

        if let Some(c) = scanner.peek()
            && (c == '"' || c == '\'')
        {
            let quote = c;
            let mut string_val = String::new();
            if let Some(ch) = scanner.next() {
                string_val.push(ch); // quote
            }
            let mut escaped = false;
            while let Some(nc) = scanner.next() {
                string_val.push(nc);
                if escaped {
                    escaped = false;
                } else if nc == '\\' {
                    escaped = true;
                } else if nc == quote {
                    break;
                }
            }
            tokens.push(Token {
                text: string_val,
                kind: TokenKind::String,
            });
            continue;
        }

        // 4. Identifiers
        if let Some(c) = scanner.peek()
            && (c.is_alphabetic() || c == '_')
        {
            let mut ident = String::new();
            while let Some(nc) = scanner.peek() {
                if nc.is_alphanumeric() || nc == '_' {
                    ident.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }

            let kind = if is_python_keyword(&ident) {
                TokenKind::Keyword
            } else if is_python_type(&ident) {
                TokenKind::Type
            } else {
                TokenKind::Text
            };

            tokens.push(Token { text: ident, kind });
            continue;
        }

        // 5. Numbers
        if let Some(c) = scanner.peek()
            && c.is_ascii_digit()
        {
            let mut num = String::new();
            while let Some(nc) = scanner.peek() {
                if nc.is_alphanumeric() || nc == '.' || nc == '_' {
                    num.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }
            tokens.push(Token {
                text: num,
                kind: TokenKind::Number,
            });
            continue;
        }

        // 6. Punctuation
        if let Some(c) = scanner.peek()
            && c.is_ascii_punctuation()
        {
            scanner.next();
            tokens.push(Token {
                text: c.to_string(),
                kind: TokenKind::Punctuation,
            });
            continue;
        }

        // 7. Fallback
        if let Some(c) = scanner.next() {
            tokens.push(Token {
                text: c.to_string(),
                kind: TokenKind::Text,
            });
        }
    }

    tokens
}

fn tokenize_ruby(line: &str) -> Vec<Token> {
    let mut scanner = Scanner::new(line);
    let mut tokens = Vec::new();

    while !scanner.is_eof() {
        // 1. Whitespace
        if let Some(c) = scanner.peek()
            && c.is_whitespace()
        {
            let mut space = String::new();
            while let Some(nc) = scanner.peek() {
                if nc.is_whitespace() {
                    space.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }
            tokens.push(Token {
                text: space,
                kind: TokenKind::Text,
            });
            continue;
        }

        // 2. Comments
        if scanner.starts_with("#") {
            let remaining: String = scanner.chars[scanner.pos..].iter().collect();
            tokens.push(Token {
                text: remaining,
                kind: TokenKind::Comment,
            });
            break;
        }

        // 3. String literals
        if let Some(c) = scanner.peek()
            && (c == '"' || c == '\'')
        {
            let quote = c;
            let mut string_val = String::new();
            if let Some(ch) = scanner.next() {
                string_val.push(ch);
            }
            let mut escaped = false;
            while let Some(nc) = scanner.next() {
                string_val.push(nc);
                if escaped {
                    escaped = false;
                } else if nc == '\\' {
                    escaped = true;
                } else if nc == quote {
                    break;
                }
            }
            tokens.push(Token {
                text: string_val,
                kind: TokenKind::String,
            });
            continue;
        }

        // 4. Symbols (:name) — highlighted like strings, the common editor style
        if scanner.starts_with(":")
            && let Some(c2) = scanner.chars.get(scanner.pos + 1).copied()
            && (c2.is_alphabetic() || c2 == '_')
        {
            let mut sym = String::new();
            sym.push(scanner.next().unwrap_or(':'));
            while let Some(nc) = scanner.peek() {
                if nc.is_alphanumeric() || nc == '_' || nc == '?' || nc == '!' {
                    sym.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }
            tokens.push(Token {
                text: sym,
                kind: TokenKind::String,
            });
            continue;
        }

        // 5. Instance/class/global variables (@foo, @@foo, $foo)
        if let Some(c) = scanner.peek()
            && (c == '@' || c == '$')
        {
            let mut var = String::new();
            var.push(scanner.next().unwrap_or(c));
            while let Some(nc) = scanner.peek() {
                if nc.is_alphanumeric() || nc == '_' || nc == '@' {
                    var.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }
            tokens.push(Token {
                text: var,
                kind: TokenKind::Type,
            });
            continue;
        }

        // 6. Identifiers (Ruby allows trailing ? / !)
        if let Some(c) = scanner.peek()
            && (c.is_alphabetic() || c == '_')
        {
            let mut ident = String::new();
            while let Some(nc) = scanner.peek() {
                if nc.is_alphanumeric() || nc == '_' {
                    ident.push(nc);
                    scanner.next();
                } else if (nc == '?' || nc == '!')
                    && !ident.is_empty()
                    && scanner
                        .chars
                        .get(scanner.pos + 1)
                        .is_none_or(|c2| !c2.is_alphanumeric())
                {
                    ident.push(nc);
                    scanner.next();
                    break;
                } else {
                    break;
                }
            }

            let kind = if is_ruby_keyword(&ident) {
                TokenKind::Keyword
            } else if ident.chars().next().is_some_and(|f| f.is_uppercase()) {
                TokenKind::Type
            } else {
                TokenKind::Text
            };

            tokens.push(Token { text: ident, kind });
            continue;
        }

        // 7. Numbers
        if let Some(c) = scanner.peek()
            && c.is_ascii_digit()
        {
            let mut num = String::new();
            while let Some(nc) = scanner.peek() {
                if nc.is_alphanumeric() || nc == '.' || nc == '_' {
                    num.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }
            tokens.push(Token {
                text: num,
                kind: TokenKind::Number,
            });
            continue;
        }

        // 8. Punctuation
        if let Some(c) = scanner.peek()
            && c.is_ascii_punctuation()
        {
            scanner.next();
            tokens.push(Token {
                text: c.to_string(),
                kind: TokenKind::Punctuation,
            });
            continue;
        }

        // 9. Fallback
        if let Some(c) = scanner.next() {
            tokens.push(Token {
                text: c.to_string(),
                kind: TokenKind::Text,
            });
        }
    }

    tokens
}

fn is_ruby_keyword(s: &str) -> bool {
    matches!(
        s,
        "def"
            | "end"
            | "class"
            | "module"
            | "if"
            | "elsif"
            | "else"
            | "unless"
            | "case"
            | "when"
            | "while"
            | "until"
            | "for"
            | "in"
            | "do"
            | "begin"
            | "rescue"
            | "ensure"
            | "retry"
            | "raise"
            | "return"
            | "yield"
            | "break"
            | "next"
            | "redo"
            | "then"
            | "and"
            | "or"
            | "not"
            | "true"
            | "false"
            | "nil"
            | "self"
            | "super"
            | "require"
            | "require_relative"
            | "include"
            | "extend"
            | "attr_accessor"
            | "attr_reader"
            | "attr_writer"
            | "private"
            | "public"
            | "protected"
            | "lambda"
            | "proc"
            | "new"
    )
}

fn is_python_keyword(s: &str) -> bool {
    matches!(
        s,
        "def"
            | "class"
            | "import"
            | "from"
            | "as"
            | "if"
            | "elif"
            | "else"
            | "while"
            | "for"
            | "in"
            | "is"
            | "not"
            | "and"
            | "or"
            | "return"
            | "yield"
            | "break"
            | "continue"
            | "pass"
            | "lambda"
            | "try"
            | "except"
            | "finally"
            | "raise"
            | "assert"
            | "with"
            | "global"
            | "nonlocal"
            | "del"
            | "True"
            | "False"
            | "None"
    )
}

fn is_python_type(s: &str) -> bool {
    if matches!(
        s,
        "int"
            | "float"
            | "str"
            | "bool"
            | "list"
            | "dict"
            | "set"
            | "tuple"
            | "object"
            | "type"
            | "Exception"
    ) {
        return true;
    }
    if let Some(first) = s.chars().next() {
        first.is_uppercase()
    } else {
        false
    }
}

fn tokenize_js_ts(line: &str) -> Vec<Token> {
    let mut scanner = Scanner::new(line);
    let mut tokens = Vec::new();

    while !scanner.is_eof() {
        // 1. Whitespace
        if let Some(c) = scanner.peek()
            && c.is_whitespace()
        {
            let mut space = String::new();
            while let Some(nc) = scanner.peek() {
                if nc.is_whitespace() {
                    space.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }
            tokens.push(Token {
                text: space,
                kind: TokenKind::Text,
            });
            continue;
        }

        // 2. Comments
        if scanner.starts_with("//") {
            let remaining: String = scanner.chars[scanner.pos..].iter().collect();
            tokens.push(Token {
                text: remaining,
                kind: TokenKind::Comment,
            });
            break;
        }
        if scanner.starts_with("/*") {
            let mut comment = String::new();
            comment.push_str("/*");
            scanner.consume_str("/*");
            while let Some(nc) = scanner.peek() {
                if scanner.starts_with("*/") {
                    comment.push_str("*/");
                    scanner.consume_str("*/");
                    break;
                }
                comment.push(nc);
                scanner.next();
            }
            tokens.push(Token {
                text: comment,
                kind: TokenKind::Comment,
            });
            continue;
        }

        // 3. String literals (including template literals if inside one line)
        if let Some(c) = scanner.peek()
            && (c == '"' || c == '\'' || c == '`')
        {
            let quote = c;
            let mut string_val = String::new();
            if let Some(ch) = scanner.next() {
                string_val.push(ch); // quote
            }
            let mut escaped = false;
            while let Some(nc) = scanner.next() {
                string_val.push(nc);
                if escaped {
                    escaped = false;
                } else if nc == '\\' {
                    escaped = true;
                } else if nc == quote {
                    break;
                }
            }
            tokens.push(Token {
                text: string_val,
                kind: TokenKind::String,
            });
            continue;
        }

        // 4. Identifiers
        if let Some(c) = scanner.peek()
            && (c.is_alphabetic() || c == '_' || c == '$')
        {
            let mut ident = String::new();
            while let Some(nc) = scanner.peek() {
                if nc.is_alphanumeric() || nc == '_' || nc == '$' {
                    ident.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }

            let kind = if is_js_ts_keyword(&ident) {
                TokenKind::Keyword
            } else if is_js_ts_type(&ident) {
                TokenKind::Type
            } else {
                TokenKind::Text
            };

            tokens.push(Token { text: ident, kind });
            continue;
        }

        // 5. Numbers
        if let Some(c) = scanner.peek()
            && c.is_ascii_digit()
        {
            let mut num = String::new();
            while let Some(nc) = scanner.peek() {
                if nc.is_alphanumeric() || nc == '.' || nc == '_' {
                    num.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }
            tokens.push(Token {
                text: num,
                kind: TokenKind::Number,
            });
            continue;
        }

        // 6. Punctuation
        if let Some(c) = scanner.peek()
            && c.is_ascii_punctuation()
        {
            scanner.next();
            tokens.push(Token {
                text: c.to_string(),
                kind: TokenKind::Punctuation,
            });
            continue;
        }

        // 7. Fallback
        if let Some(c) = scanner.next() {
            tokens.push(Token {
                text: c.to_string(),
                kind: TokenKind::Text,
            });
        }
    }

    tokens
}

fn is_js_ts_keyword(s: &str) -> bool {
    matches!(
        s,
        "const"
            | "let"
            | "var"
            | "function"
            | "class"
            | "import"
            | "export"
            | "from"
            | "default"
            | "if"
            | "else"
            | "switch"
            | "case"
            | "while"
            | "for"
            | "in"
            | "of"
            | "return"
            | "break"
            | "continue"
            | "try"
            | "catch"
            | "finally"
            | "throw"
            | "new"
            | "this"
            | "super"
            | "extends"
            | "implements"
            | "interface"
            | "type"
            | "as"
            | "async"
            | "await"
            | "yield"
            | "true"
            | "false"
            | "null"
            | "undefined"
            | "void"
            | "typeof"
            | "instanceof"
    )
}

fn is_js_ts_type(s: &str) -> bool {
    if matches!(
        s,
        "number" | "string" | "boolean" | "any" | "unknown" | "never" | "object" | "Array"
    ) {
        return true;
    }
    if let Some(first) = s.chars().next() {
        first.is_uppercase()
    } else {
        false
    }
}

fn tokenize_toml(line: &str) -> Vec<Token> {
    let mut scanner = Scanner::new(line);
    let mut tokens = Vec::new();

    while !scanner.is_eof() {
        // 1. Whitespace
        if let Some(c) = scanner.peek()
            && c.is_whitespace()
        {
            let mut space = String::new();
            while let Some(nc) = scanner.peek() {
                if nc.is_whitespace() {
                    space.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }
            tokens.push(Token {
                text: space,
                kind: TokenKind::Text,
            });
            continue;
        }

        // 2. Comments
        if scanner.starts_with("#") {
            let remaining: String = scanner.chars[scanner.pos..].iter().collect();
            tokens.push(Token {
                text: remaining,
                kind: TokenKind::Comment,
            });
            break;
        }

        // 3. String literals
        if let Some(c) = scanner.peek()
            && (c == '"' || c == '\'')
        {
            let quote = c;
            let mut string_val = String::new();
            if let Some(ch) = scanner.next() {
                string_val.push(ch);
            }
            while let Some(nc) = scanner.peek() {
                string_val.push(nc);
                scanner.next();
                if nc == quote {
                    break;
                }
            }
            tokens.push(Token {
                text: string_val,
                kind: TokenKind::String,
            });
            continue;
        }

        // 4. Key/Identifiers
        if let Some(c) = scanner.peek()
            && (c.is_alphabetic() || c == '_' || c == '-')
        {
            let mut ident = String::new();
            while let Some(nc) = scanner.peek() {
                if nc.is_alphanumeric() || nc == '_' || nc == '-' {
                    ident.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }

            let kind = if ident == "true" || ident == "false" {
                TokenKind::Keyword
            } else {
                TokenKind::Type // Keys in TOML look nicer with Type colors
            };

            tokens.push(Token { text: ident, kind });
            continue;
        }

        // 5. Numbers
        if let Some(c) = scanner.peek()
            && c.is_ascii_digit()
        {
            let mut num = String::new();
            while let Some(nc) = scanner.peek() {
                if nc.is_alphanumeric() || nc == '.' || nc == '-' || nc == '_' {
                    num.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }
            tokens.push(Token {
                text: num,
                kind: TokenKind::Number,
            });
            continue;
        }

        // 6. Punctuation
        if let Some(c) = scanner.peek()
            && c.is_ascii_punctuation()
        {
            scanner.next();
            tokens.push(Token {
                text: c.to_string(),
                kind: TokenKind::Punctuation,
            });
            continue;
        }

        // 7. Fallback
        if let Some(c) = scanner.next() {
            tokens.push(Token {
                text: c.to_string(),
                kind: TokenKind::Text,
            });
        }
    }

    tokens
}

fn tokenize_yaml(line: &str) -> Vec<Token> {
    let mut scanner = Scanner::new(line);
    let mut tokens = Vec::new();

    while !scanner.is_eof() {
        // 1. Whitespace
        if let Some(c) = scanner.peek()
            && c.is_whitespace()
        {
            let mut space = String::new();
            while let Some(nc) = scanner.peek() {
                if nc.is_whitespace() {
                    space.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }
            tokens.push(Token {
                text: space,
                kind: TokenKind::Text,
            });
            continue;
        }

        // 2. Comments
        if scanner.starts_with("#") {
            let remaining: String = scanner.chars[scanner.pos..].iter().collect();
            tokens.push(Token {
                text: remaining,
                kind: TokenKind::Comment,
            });
            break;
        }

        // 3. String literals
        if let Some(c) = scanner.peek()
            && (c == '"' || c == '\'')
        {
            let quote = c;
            let mut string_val = String::new();
            if let Some(ch) = scanner.next() {
                string_val.push(ch);
            }
            while let Some(nc) = scanner.peek() {
                string_val.push(nc);
                scanner.next();
                if nc == quote {
                    break;
                }
            }
            tokens.push(Token {
                text: string_val,
                kind: TokenKind::String,
            });
            continue;
        }

        // 4. Identifiers/Keys (YAML mapping keys end with :)
        if let Some(c) = scanner.peek()
            && (c.is_alphabetic() || c == '_' || c == '-')
        {
            let mut ident = String::new();
            while let Some(nc) = scanner.peek() {
                if nc.is_alphanumeric() || nc == '_' || nc == '-' {
                    ident.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }

            let kind = if ident == "true" || ident == "false" || ident == "null" {
                TokenKind::Keyword
            } else if scanner.peek() == Some(':') {
                TokenKind::Type // YAML Keys
            } else {
                TokenKind::Text
            };

            tokens.push(Token { text: ident, kind });
            continue;
        }

        // 5. Numbers
        if let Some(c) = scanner.peek()
            && c.is_ascii_digit()
        {
            let mut num = String::new();
            while let Some(nc) = scanner.peek() {
                if nc.is_alphanumeric() || nc == '.' || nc == '-' || nc == '_' {
                    num.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }
            tokens.push(Token {
                text: num,
                kind: TokenKind::Number,
            });
            continue;
        }

        // 6. Punctuation
        if let Some(c) = scanner.peek()
            && c.is_ascii_punctuation()
        {
            scanner.next();
            tokens.push(Token {
                text: c.to_string(),
                kind: TokenKind::Punctuation,
            });
            continue;
        }

        // 7. Fallback
        if let Some(c) = scanner.next() {
            tokens.push(Token {
                text: c.to_string(),
                kind: TokenKind::Text,
            });
        }
    }

    tokens
}

fn tokenize_html_xml(line: &str) -> Vec<Token> {
    let mut scanner = Scanner::new(line);
    let mut tokens = Vec::new();
    let mut in_tag = false;

    while !scanner.is_eof() {
        // Space
        if let Some(c) = scanner.peek()
            && c.is_whitespace()
        {
            let mut space = String::new();
            while let Some(nc) = scanner.peek() {
                if nc.is_whitespace() {
                    space.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }
            tokens.push(Token {
                text: space,
                kind: TokenKind::Text,
            });
            continue;
        }

        // Comment
        if scanner.starts_with("<!--") {
            let mut comment = String::new();
            comment.push_str("<!--");
            scanner.consume_str("<!--");
            while let Some(nc) = scanner.peek() {
                if scanner.starts_with("-->") {
                    comment.push_str("-->");
                    scanner.consume_str("-->");
                    break;
                }
                comment.push(nc);
                scanner.next();
            }
            tokens.push(Token {
                text: comment,
                kind: TokenKind::Comment,
            });
            continue;
        }

        // Tag start
        if scanner.starts_with("</") {
            scanner.consume_str("</");
            tokens.push(Token {
                text: "</".to_string(),
                kind: TokenKind::Punctuation,
            });
            in_tag = true;
            let mut tag_name = String::new();
            while let Some(nc) = scanner.peek() {
                if nc.is_alphanumeric() || nc == '-' || nc == '_' || nc == ':' {
                    tag_name.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }
            if !tag_name.is_empty() {
                tokens.push(Token {
                    text: tag_name,
                    kind: TokenKind::Type,
                });
            }
            continue;
        }

        if scanner.starts_with("<") {
            scanner.consume_str("<");
            tokens.push(Token {
                text: "<".to_string(),
                kind: TokenKind::Punctuation,
            });
            in_tag = true;
            let mut tag_name = String::new();
            while let Some(nc) = scanner.peek() {
                if nc.is_alphanumeric() || nc == '-' || nc == '_' || nc == ':' {
                    tag_name.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }
            if !tag_name.is_empty() {
                tokens.push(Token {
                    text: tag_name,
                    kind: TokenKind::Type,
                });
            }
            continue;
        }

        // Tag end
        if scanner.starts_with("/>") {
            scanner.consume_str("/>");
            tokens.push(Token {
                text: "/>".to_string(),
                kind: TokenKind::Punctuation,
            });
            in_tag = false;
            continue;
        }

        if scanner.starts_with(">") {
            scanner.consume_str(">");
            tokens.push(Token {
                text: ">".to_string(),
                kind: TokenKind::Punctuation,
            });
            in_tag = false;
            continue;
        }

        if in_tag {
            // Strings (Attribute values)
            if let Some(c) = scanner.peek()
                && (c == '"' || c == '\'')
            {
                let quote = c;
                let mut string_val = String::new();
                if let Some(ch) = scanner.next() {
                    string_val.push(ch);
                }
                while let Some(nc) = scanner.peek() {
                    string_val.push(nc);
                    scanner.next();
                    if nc == quote {
                        break;
                    }
                }
                tokens.push(Token {
                    text: string_val,
                    kind: TokenKind::String,
                });
                continue;
            }

            // Attribute names
            if let Some(c) = scanner.peek()
                && (c.is_alphabetic() || c == '_' || c == '-')
            {
                let mut ident = String::new();
                while let Some(nc) = scanner.peek() {
                    if nc.is_alphanumeric() || nc == '_' || nc == '-' || nc == ':' {
                        ident.push(nc);
                        scanner.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Token {
                    text: ident,
                    kind: TokenKind::Keyword,
                });
                continue;
            }
        }

        // Text content
        if let Some(c) = scanner.next() {
            let mut text = c.to_string();
            while let Some(nc) = scanner.peek() {
                if nc == '<' || nc == '"' || nc == '\'' || nc.is_whitespace() {
                    break;
                }
                if in_tag && nc == '>' {
                    break;
                }
                text.push(nc);
                scanner.next();
            }
            tokens.push(Token {
                text,
                kind: TokenKind::Text,
            });
        }
    }

    tokens
}

fn tokenize_css(line: &str) -> Vec<Token> {
    let mut scanner = Scanner::new(line);
    let mut tokens = Vec::new();

    while !scanner.is_eof() {
        // Space
        if let Some(c) = scanner.peek()
            && c.is_whitespace()
        {
            let mut space = String::new();
            while let Some(nc) = scanner.peek() {
                if nc.is_whitespace() {
                    space.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }
            tokens.push(Token {
                text: space,
                kind: TokenKind::Text,
            });
            continue;
        }

        // Comment /* ... */
        if scanner.starts_with("/*") {
            let mut comment = String::new();
            comment.push_str("/*");
            scanner.consume_str("/*");
            while let Some(nc) = scanner.peek() {
                if scanner.starts_with("*/") {
                    comment.push_str("*/");
                    scanner.consume_str("*/");
                    break;
                }
                comment.push(nc);
                scanner.next();
            }
            tokens.push(Token {
                text: comment,
                kind: TokenKind::Comment,
            });
            continue;
        }

        // Strings
        if let Some(c) = scanner.peek()
            && (c == '"' || c == '\'')
        {
            let quote = c;
            let mut string_val = String::new();
            if let Some(ch) = scanner.next() {
                string_val.push(ch);
            }
            while let Some(nc) = scanner.peek() {
                string_val.push(nc);
                scanner.next();
                if nc == quote {
                    break;
                }
            }
            tokens.push(Token {
                text: string_val,
                kind: TokenKind::String,
            });
            continue;
        }

        // Selectors (.class, #id)
        if let Some(c) = scanner.peek()
            && (c == '.' || c == '#')
        {
            scanner.next();
            let mut ident = c.to_string();
            while let Some(nc) = scanner.peek() {
                if nc.is_alphanumeric() || nc == '-' || nc == '_' {
                    ident.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }
            tokens.push(Token {
                text: ident,
                kind: TokenKind::Type,
            });
            continue;
        }

        // Property names / general identifiers
        if let Some(c) = scanner.peek()
            && (c.is_alphabetic() || c == '-' || c == '_')
        {
            let mut ident = String::new();
            while let Some(nc) = scanner.peek() {
                if nc.is_alphanumeric() || nc == '-' || nc == '_' {
                    ident.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }
            tokens.push(Token {
                text: ident,
                kind: TokenKind::Keyword,
            });
            continue;
        }

        // Numbers
        if let Some(c) = scanner.peek()
            && c.is_ascii_digit()
        {
            let mut num = String::new();
            while let Some(nc) = scanner.peek() {
                if nc.is_alphanumeric() || nc == '.' || nc == '%' {
                    num.push(nc);
                    scanner.next();
                } else {
                    break;
                }
            }
            tokens.push(Token {
                text: num,
                kind: TokenKind::Number,
            });
            continue;
        }

        // Punctuation
        if let Some(c) = scanner.peek()
            && c.is_ascii_punctuation()
        {
            scanner.next();
            tokens.push(Token {
                text: c.to_string(),
                kind: TokenKind::Punctuation,
            });
            continue;
        }

        // Fallback
        if let Some(c) = scanner.next() {
            tokens.push(Token {
                text: c.to_string(),
                kind: TokenKind::Text,
            });
        }
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_rust() {
        let tokens = tokenize("let x: u32 = 42; // comment", "rs");
        assert_eq!(
            tokens[0],
            Token {
                text: "let".to_string(),
                kind: TokenKind::Keyword
            }
        );
        assert_eq!(
            tokens[1],
            Token {
                text: " ".to_string(),
                kind: TokenKind::Text
            }
        );
        assert_eq!(
            tokens[2],
            Token {
                text: "x".to_string(),
                kind: TokenKind::Text
            }
        );
        assert_eq!(
            tokens[3],
            Token {
                text: ":".to_string(),
                kind: TokenKind::Punctuation
            }
        );
        assert_eq!(
            tokens[4],
            Token {
                text: " ".to_string(),
                kind: TokenKind::Text
            }
        );
        assert_eq!(
            tokens[5],
            Token {
                text: "u32".to_string(),
                kind: TokenKind::Type
            }
        );
        assert_eq!(
            tokens[6],
            Token {
                text: " ".to_string(),
                kind: TokenKind::Text
            }
        );
        assert_eq!(
            tokens[7],
            Token {
                text: "=".to_string(),
                kind: TokenKind::Punctuation
            }
        );
        assert_eq!(
            tokens[8],
            Token {
                text: " ".to_string(),
                kind: TokenKind::Text
            }
        );
        assert_eq!(
            tokens[9],
            Token {
                text: "42".to_string(),
                kind: TokenKind::Number
            }
        );
        assert_eq!(
            tokens[10],
            Token {
                text: ";".to_string(),
                kind: TokenKind::Punctuation
            }
        );
        assert_eq!(
            tokens[11],
            Token {
                text: " ".to_string(),
                kind: TokenKind::Text
            }
        );
        assert_eq!(
            tokens[12],
            Token {
                text: "// comment".to_string(),
                kind: TokenKind::Comment
            }
        );
    }

    #[test]
    fn test_tokenize_markdown() {
        let tokens = tokenize("# 見出し", "md");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Keyword);

        let tokens = tokenize("- item with `code` and **bold**", "md");
        assert!(
            tokens
                .iter()
                .any(|t| t.text == "`code`" && t.kind == TokenKind::String)
        );
        assert!(
            tokens
                .iter()
                .any(|t| t.text == "**bold**" && t.kind == TokenKind::Type)
        );
        assert!(
            tokens
                .iter()
                .any(|t| t.text == "-" && t.kind == TokenKind::Punctuation)
        );

        let tokens = tokenize("see [docs](https://example.com) here", "md");
        assert!(
            tokens
                .iter()
                .any(|t| t.text == "[docs](https://example.com)" && t.kind == TokenKind::Keyword)
        );
    }

    #[test]
    fn test_tokenize_ruby() {
        let tokens = tokenize("def save!(user) # persist", "rb");
        let kw: Vec<&str> = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Keyword)
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(kw, vec!["def"]);
        assert!(
            tokens
                .iter()
                .any(|t| t.text == "save!" && t.kind == TokenKind::Text)
        );
        assert!(
            tokens
                .iter()
                .any(|t| t.kind == TokenKind::Comment && t.text.starts_with('#'))
        );

        let tokens = tokenize("@items[:key] = User.new", "rb");
        assert!(
            tokens
                .iter()
                .any(|t| t.text == "@items" && t.kind == TokenKind::Type)
        );
        assert!(
            tokens
                .iter()
                .any(|t| t.text == ":key" && t.kind == TokenKind::String)
        );
        assert!(
            tokens
                .iter()
                .any(|t| t.text == "User" && t.kind == TokenKind::Type)
        );
    }

    #[test]
    fn test_tokenize_python() {
        let tokens = tokenize("def foo(x): # comment", "py");
        assert_eq!(
            tokens[0],
            Token {
                text: "def".to_string(),
                kind: TokenKind::Keyword
            }
        );
        assert_eq!(
            tokens[2],
            Token {
                text: "foo".to_string(),
                kind: TokenKind::Text
            }
        );
        assert_eq!(
            tokens[8],
            Token {
                text: "# comment".to_string(),
                kind: TokenKind::Comment
            }
        );
    }

    #[test]
    fn test_tokenize_js() {
        let tokens = tokenize("const a = \"hello\"; // desc", "js");
        assert_eq!(
            tokens[0],
            Token {
                text: "const".to_string(),
                kind: TokenKind::Keyword
            }
        );
        assert_eq!(
            tokens[6],
            Token {
                text: "\"hello\"".to_string(),
                kind: TokenKind::String
            }
        );
    }

    #[test]
    fn test_tokenize_toml() {
        let tokens = tokenize("name = \"git-dashboard\" # comment", "toml");
        assert_eq!(
            tokens[0],
            Token {
                text: "name".to_string(),
                kind: TokenKind::Type
            }
        );
        assert_eq!(
            tokens[4],
            Token {
                text: "\"git-dashboard\"".to_string(),
                kind: TokenKind::String
            }
        );
    }

    #[test]
    fn test_tokenize_html() {
        let tokens = tokenize("<div class=\"container\">hello</div>", "html");
        assert_eq!(
            tokens[0],
            Token {
                text: "<".to_string(),
                kind: TokenKind::Punctuation
            }
        );
        assert_eq!(
            tokens[1],
            Token {
                text: "div".to_string(),
                kind: TokenKind::Type
            }
        );
        assert_eq!(
            tokens[3],
            Token {
                text: "class".to_string(),
                kind: TokenKind::Keyword
            }
        );
        assert_eq!(
            tokens[5],
            Token {
                text: "\"container\"".to_string(),
                kind: TokenKind::String
            }
        );
        assert_eq!(
            tokens[6],
            Token {
                text: ">".to_string(),
                kind: TokenKind::Punctuation
            }
        );
        assert_eq!(
            tokens[7],
            Token {
                text: "hello".to_string(),
                kind: TokenKind::Text
            }
        );
    }

    #[test]
    fn test_tokenize_edge_cases() {
        // Empty inputs must never panic
        assert_eq!(tokenize("", "rs"), Vec::new());
        assert_eq!(tokenize("", "py"), Vec::new());
        assert_eq!(tokenize("", "js"), Vec::new());
        assert_eq!(tokenize("", "md"), Vec::new());

        // Unterminated string literals must not panic or infinite loop
        let unclosed = tokenize("let s = \"hello world", "rs");
        assert!(!unclosed.is_empty());

        // Multibyte characters and emojis must not panic
        let japanese = tokenize("let 変数 = \"こんにちは世界 🚀\";", "rs");
        assert!(!japanese.is_empty());
        let py_japanese = tokenize("# 日本語のコメント\nmsg = 'テスト'", "py");
        assert!(!py_japanese.is_empty());

        // Escaped quotes in strings
        let escaped = tokenize(r#"let json = "{\"key\": \"value\"}";"#, "rs");
        assert!(!escaped.is_empty());
    }
}

#[cfg(test)]
mod roundtrip_tests {
    use super::*;

    /// The tokenizer only classifies text; concatenating the tokens must give
    /// back exactly what went in. Anything else means the rendered diff shows
    /// something the file does not contain.
    fn assert_roundtrip(line: &str, ext: &str) {
        let joined: String = tokenize(line, ext)
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(
            joined, line,
            "tokenize({line:?}, {ext:?}) did not round-trip"
        );
    }

    #[test]
    fn tokenize_preserves_input_for_every_language() {
        let exts = [
            "rs", "py", "rb", "js", "ts", "json", "toml", "yaml", "yml", "html", "xml", "css",
            "scss", "md", "txt", "",
        ];
        let lines = [
            "",
            " ",
            "let x: u32 = 42; // comment",
            "def f(a, b):  # trailing",
            "\"\"\"docstring\"\"\"",
            "\"\"\"unterminated",
            "x = '''triple single'''",
            "key: \"value\" # note",
            "<div class=\"a\">text</div>",
            ".cls { color: #fff; }",
            "# heading with `code` and **bold**",
            "  - list item",
            "s = \"unterminated",
            "s = 'a\\\\'b'",
            "日本語のコメント # です",
            "emoji 🎉 and combining é",
            "a\tb\tc",
            "/* block */ code /* another */",
            "|||not a delimiter|||",
            "0x1F, 1_000, 3.14e-2",
        ];
        for ext in exts {
            for line in lines {
                assert_roundtrip(line, ext);
            }
        }
    }

    #[test]
    fn python_triple_quoted_string_is_not_corrupted() {
        // This used to emit a zero-width space inside the opening quotes.
        let joined: String = tokenize("\"\"\"doc\"\"\"", "py")
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(joined, "\"\"\"doc\"\"\"");
        assert!(!joined.contains('\u{200b}'));
    }
}
