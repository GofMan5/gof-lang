use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::source::{SourceFile, Span};
use crate::token::{Token, TokenKind};

pub fn lex(source: &SourceFile) -> Result<Vec<Token>, Diagnostics> {
    let mut tokens = Vec::new();
    let mut diagnostics = Diagnostics::default();
    let mut indent_stack = vec![0usize];

    for (index, raw_line) in source.text().lines().enumerate() {
        let line_number = index + 1;
        if raw_line.trim().is_empty() || raw_line.trim_start().starts_with('#') {
            continue;
        }

        let indent = count_indent(raw_line, line_number, &mut diagnostics);
        if !diagnostics.is_empty() {
            continue;
        }

        let current_indent = *indent_stack
            .last()
            .expect("indent stack always contains zero");

        if indent > current_indent {
            if indent != current_indent + 4 {
                diagnostics.push(
                    Diagnostic::error(
                        "GOF1001",
                        "invalid indentation step",
                        "indentation must grow in blocks of exactly 4 spaces",
                        Span::new(line_number, 1, indent.max(1)),
                    )
                    .with_fix_it("use four spaces for each nested block"),
                );
                continue;
            }
            indent_stack.push(indent);
            tokens.push(Token::new(
                TokenKind::Indent,
                Span::new(line_number, 1, indent),
            ));
        } else if indent < current_indent {
            while indent < *indent_stack.last().expect("indent stack cannot be empty") {
                indent_stack.pop();
                tokens.push(Token::new(
                    TokenKind::Dedent,
                    Span::new(line_number, 1, indent + 1),
                ));
            }

            if indent != *indent_stack.last().expect("indent stack cannot be empty") {
                diagnostics.push(
                    Diagnostic::error(
                        "GOF1002",
                        "inconsistent dedent",
                        "dedent must match a previous indentation level",
                        Span::new(line_number, 1, indent + 1),
                    )
                    .with_fix_it("align this line with a previous block start"),
                );
                continue;
            }
        }

        lex_line(
            &raw_line[indent..],
            line_number,
            indent + 1,
            &mut tokens,
            &mut diagnostics,
        );
        tokens.push(Token::new(
            TokenKind::Newline,
            Span::new(line_number, raw_line.len() + 1, raw_line.len() + 1),
        ));
    }

    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }

    while indent_stack.len() > 1 {
        indent_stack.pop();
        let last_line = source.text().lines().count().max(1);
        tokens.push(Token::new(TokenKind::Dedent, Span::new(last_line, 1, 1)));
    }
    let eof_line = source.text().lines().count().max(1);
    tokens.push(Token::new(TokenKind::Eof, Span::new(eof_line, 1, 1)));

    Ok(tokens)
}

fn count_indent(raw_line: &str, line_number: usize, diagnostics: &mut Diagnostics) -> usize {
    let mut count = 0usize;

    for character in raw_line.chars() {
        match character {
            ' ' => count += 1,
            '\t' => {
                diagnostics.push(
                    Diagnostic::error(
                        "GOF1001",
                        "tabs are not allowed for indentation",
                        "gof uses spaces only for indentation-sensitive blocks",
                        Span::new(line_number, count + 1, count + 2),
                    )
                    .with_fix_it("replace the tab with four spaces"),
                );
                return count;
            }
            _ => break,
        }
    }

    if count % 4 != 0 {
        diagnostics.push(
            Diagnostic::error(
                "GOF1001",
                "indentation is not a multiple of four spaces",
                "every indentation level in gof must use four spaces",
                Span::new(line_number, 1, count.max(1)),
            )
            .with_fix_it("pad or trim the leading spaces to a multiple of four"),
        );
    }

    count
}

fn lex_line(
    line: &str,
    line_number: usize,
    base_column: usize,
    tokens: &mut Vec<Token>,
    diagnostics: &mut Diagnostics,
) {
    let chars: Vec<char> = line.chars().collect();
    let mut index = 0usize;

    while index < chars.len() {
        let character = chars[index];
        let column = base_column + index;

        match character {
            ' ' => index += 1,
            '#' => break,
            '(' => {
                tokens.push(Token::new(
                    TokenKind::LParen,
                    Span::new(line_number, column, column + 1),
                ));
                index += 1;
            }
            ')' => {
                tokens.push(Token::new(
                    TokenKind::RParen,
                    Span::new(line_number, column, column + 1),
                ));
                index += 1;
            }
            ':' => {
                tokens.push(Token::new(
                    TokenKind::Colon,
                    Span::new(line_number, column, column + 1),
                ));
                index += 1;
            }
            ',' => {
                tokens.push(Token::new(
                    TokenKind::Comma,
                    Span::new(line_number, column, column + 1),
                ));
                index += 1;
            }
            '=' => {
                if matches!(chars.get(index + 1), Some('=')) {
                    tokens.push(Token::new(
                        TokenKind::EqualEqual,
                        Span::new(line_number, column, column + 2),
                    ));
                    index += 2;
                } else {
                    tokens.push(Token::new(
                        TokenKind::Equal,
                        Span::new(line_number, column, column + 1),
                    ));
                    index += 1;
                }
            }
            '!' => {
                if matches!(chars.get(index + 1), Some('=')) {
                    tokens.push(Token::new(
                        TokenKind::BangEqual,
                        Span::new(line_number, column, column + 2),
                    ));
                    index += 2;
                } else {
                    diagnostics.push(
                        Diagnostic::error(
                            "GOF2001",
                            "unexpected character `!`",
                            "use `!=` for inequality comparisons",
                            Span::new(line_number, column, column + 1),
                        )
                        .with_fix_it("replace `!` with `!=` or remove it"),
                    );
                    return;
                }
            }
            '<' => {
                if matches!(chars.get(index + 1), Some('=')) {
                    tokens.push(Token::new(
                        TokenKind::LessEqual,
                        Span::new(line_number, column, column + 2),
                    ));
                    index += 2;
                } else {
                    tokens.push(Token::new(
                        TokenKind::Less,
                        Span::new(line_number, column, column + 1),
                    ));
                    index += 1;
                }
            }
            '>' => {
                if matches!(chars.get(index + 1), Some('=')) {
                    tokens.push(Token::new(
                        TokenKind::GreaterEqual,
                        Span::new(line_number, column, column + 2),
                    ));
                    index += 2;
                } else {
                    tokens.push(Token::new(
                        TokenKind::Greater,
                        Span::new(line_number, column, column + 1),
                    ));
                    index += 1;
                }
            }
            '+' => {
                tokens.push(Token::new(
                    TokenKind::Plus,
                    Span::new(line_number, column, column + 1),
                ));
                index += 1;
            }
            '-' => {
                tokens.push(Token::new(
                    TokenKind::Minus,
                    Span::new(line_number, column, column + 1),
                ));
                index += 1;
            }
            '*' => {
                tokens.push(Token::new(
                    TokenKind::Star,
                    Span::new(line_number, column, column + 1),
                ));
                index += 1;
            }
            '"' => match lex_string(&chars, index, line_number, base_column) {
                Ok((token, consumed)) => {
                    tokens.push(token);
                    index = consumed;
                }
                Err(diagnostic) => {
                    diagnostics.push(diagnostic);
                    return;
                }
            },
            character if character.is_ascii_digit() => {
                let start = index;
                while index < chars.len() && chars[index].is_ascii_digit() {
                    index += 1;
                }
                let literal = chars[start..index].iter().collect::<String>();
                let value = literal.parse::<i64>().unwrap_or_default();
                tokens.push(Token::new(
                    TokenKind::IntLiteral(value),
                    Span::new(line_number, base_column + start, base_column + index),
                ));
            }
            character if is_ident_start(character) => {
                let start = index;
                while index < chars.len() && is_ident_continue(chars[index]) {
                    index += 1;
                }
                let value = chars[start..index].iter().collect::<String>();
                let kind = match value.as_str() {
                    "module" => TokenKind::Module,
                    "fn" => TokenKind::Fn,
                    "if" => TokenKind::If,
                    "else" => TokenKind::Else,
                    "while" => TokenKind::While,
                    "return" => TokenKind::Return,
                    "struct" => TokenKind::Struct,
                    "enum" => TokenKind::Enum,
                    "protocol" => TokenKind::Protocol,
                    "match" => TokenKind::Match,
                    "async" => TokenKind::Async,
                    "await" => TokenKind::Await,
                    "select" => TokenKind::Select,
                    "defer" => TokenKind::Defer,
                    "unsafe" => TokenKind::Unsafe,
                    "mut" => TokenKind::Mut,
                    "true" => TokenKind::True,
                    "false" => TokenKind::False,
                    _ => TokenKind::Ident(value),
                };
                tokens.push(Token::new(
                    kind,
                    Span::new(line_number, base_column + start, base_column + index),
                ));
            }
            _ => {
                diagnostics.push(Diagnostic::error(
                    "GOF2001",
                    format!("unexpected character `{character}`"),
                    "the bootstrap lexer only accepts gof syntax tokens",
                    Span::new(line_number, column, column + 1),
                ));
                return;
            }
        }
    }
}

fn lex_string(
    chars: &[char],
    start: usize,
    line_number: usize,
    base_column: usize,
) -> Result<(Token, usize), Diagnostic> {
    let mut index = start + 1;
    let mut value = String::new();

    while index < chars.len() {
        match chars[index] {
            '"' => {
                return Ok((
                    Token::new(
                        TokenKind::StringLiteral(value),
                        Span::new(line_number, base_column + start, base_column + index + 1),
                    ),
                    index + 1,
                ));
            }
            '\\' => {
                index += 1;
                if index >= chars.len() {
                    break;
                }
                let escaped = match chars[index] {
                    '"' => '"',
                    'n' => '\n',
                    't' => '\t',
                    other => other,
                };
                value.push(escaped);
                index += 1;
            }
            character => {
                value.push(character);
                index += 1;
            }
        }
    }

    Err(Diagnostic::error(
        "GOF2001",
        "unterminated string literal",
        "string literals must end on the same line with a closing quote",
        Span::new(line_number, base_column + start, base_column + chars.len()),
    )
    .with_fix_it("add a closing double quote"))
}

fn is_ident_start(character: char) -> bool {
    character == '_' || character.is_ascii_alphabetic()
}

fn is_ident_continue(character: char) -> bool {
    character == '_' || character.is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::lex;
    use crate::source::SourceFile;
    use crate::token::TokenKind;

    #[test]
    fn lexes_indent_sensitive_tokens() {
        let source = SourceFile::new(
            "test.gof",
            "fn main():\n    return 1\nfn other():\n    return 2\n",
        );

        let tokens = lex(&source).expect("source should lex");

        assert!(tokens.iter().any(|token| token.kind == TokenKind::Indent));
        assert!(tokens.iter().any(|token| token.kind == TokenKind::Dedent));
        assert!(tokens.iter().any(|token| token.kind == TokenKind::Fn));
    }

    #[test]
    fn rejects_non_multiple_indentation() {
        let source = SourceFile::new("bad.gof", "fn main():\n   return 1\n");
        let diagnostics = lex(&source).expect_err("source should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF1001"]);
    }
}
