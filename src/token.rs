use kiutils_sexpr::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtomClass {
    Symbol,
    Number,
    Quoted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokKind {
    Left,
    Right,
    Atom(String),
    Eof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokKind,
    pub atom_class: Option<AtomClass>,
    pub span: Span,
}

/// Pre-scan the raw file to extract the `(version NNNNN)` number before full tokenization.
/// Returns the version number if found, or `None`.
pub fn prescan_version(input: &str) -> Option<i32> {
    // Quick scan for the pattern "(version DIGITS)" within the first few hundred bytes.
    // The version token always appears very early in the file.
    let haystack = if input.len() > 512 {
        &input[..512]
    } else {
        input
    };
    let marker = "(version ";
    if let Some(pos) = haystack.find(marker) {
        let rest = &haystack[pos + marker.len()..];
        let end = rest.find(')').unwrap_or(rest.len());
        rest[..end].trim().parse::<i32>().ok()
    } else {
        None
    }
}

/// The version at which `|` became an s-expression separator in KiCad.
const VERSION_KNOWS_BAR: i32 = 20240620;

pub fn lex(input: &str) -> Result<Vec<Token>, kiutils_sexpr::ParseError> {
    let knows_bar = prescan_version(input).unwrap_or(0) >= VERSION_KNOWS_BAR;
    lex_with_bar(input, knows_bar)
}

fn lex_with_bar(input: &str, knows_bar: bool) -> Result<Vec<Token>, kiutils_sexpr::ParseError> {
    let bytes = input.as_bytes();
    let mut i = 0usize;
    let mut tokens = Vec::new();

    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\n' | b'\r' => i += 1,
            b'(' => {
                tokens.push(Token {
                    kind: TokKind::Left,
                    atom_class: None,
                    span: Span {
                        start: i,
                        end: i + 1,
                    },
                });
                i += 1;
            }
            b')' => {
                tokens.push(Token {
                    kind: TokKind::Right,
                    atom_class: None,
                    span: Span {
                        start: i,
                        end: i + 1,
                    },
                });
                i += 1;
            }
            b'"' => {
                let start = i;
                i += 1;
                let mut out = Vec::<u8>::new();
                while i < bytes.len() {
                    match bytes[i] {
                        b'\\' => {
                            i += 1;
                            if i >= bytes.len() {
                                return Err(kiutils_sexpr::ParseError::UnexpectedEof);
                            }
                            out.push(bytes[i]);
                            i += 1;
                        }
                        b'"' => {
                            i += 1;
                            let text = String::from_utf8(out)
                                .map_err(|_| kiutils_sexpr::ParseError::UnexpectedToken(start))?;
                            tokens.push(Token {
                                kind: TokKind::Atom(text),
                                atom_class: Some(AtomClass::Quoted),
                                span: Span { start, end: i },
                            });
                            break;
                        }
                        other => {
                            out.push(other);
                            i += 1;
                        }
                    }
                }
                if i > bytes.len() {
                    return Err(kiutils_sexpr::ParseError::UnexpectedEof);
                }
            }
            b'|' if knows_bar => {
                // Upstream: when knowsBar is true, `|` is both a separator and its own token.
                tokens.push(Token {
                    kind: TokKind::Atom("|".to_string()),
                    atom_class: Some(AtomClass::Symbol),
                    span: Span {
                        start: i,
                        end: i + 1,
                    },
                });
                i += 1;
            }
            _ => {
                let start = i;
                while i < bytes.len()
                    && !matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r' | b'(' | b')')
                    && !(knows_bar && bytes[i] == b'|')
                {
                    i += 1;
                }
                if start == i {
                    return Err(kiutils_sexpr::ParseError::UnexpectedToken(i));
                }
                let text = String::from_utf8(bytes[start..i].to_vec())
                    .map_err(|_| kiutils_sexpr::ParseError::UnexpectedToken(start))?;
                let atom_class = if text.parse::<f64>().is_ok() {
                    AtomClass::Number
                } else {
                    AtomClass::Symbol
                };
                tokens.push(Token {
                    kind: TokKind::Atom(text),
                    atom_class: Some(atom_class),
                    span: Span { start, end: i },
                });
            }
        }
    }

    tokens.push(Token {
        kind: TokKind::Eof,
        atom_class: None,
        span: Span {
            start: input.len(),
            end: input.len(),
        },
    });
    Ok(tokens)
}
