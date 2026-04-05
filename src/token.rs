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
    let bytes = input.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\n' | b'\r' => i += 1,
            b'"' => {
                i += 1;
                while i < bytes.len() {
                    match bytes[i] {
                        b'\\' => {
                            i += 1;
                            if i >= bytes.len() {
                                return None;
                            }
                            i += 1;
                        }
                        b'"' => {
                            i += 1;
                            break;
                        }
                        _ => i += 1,
                    }
                }
            }
            b'(' => {
                i += 1;

                while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r') {
                    i += 1;
                }

                let symbol_start = i;
                while i < bytes.len()
                    && !matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r' | b'(' | b')')
                {
                    i += 1;
                }

                if symbol_start == i || &input[symbol_start..i] != "version" {
                    continue;
                }

                while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r') {
                    i += 1;
                }

                let number_start = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }

                if number_start == i {
                    continue;
                }

                return input[number_start..i].parse::<i32>().ok();
            }
            _ => i += 1,
        }
    }

    None
}

/// The version at which `|` became an s-expression separator in KiCad.
const VERSION_KNOWS_BAR: i32 = 20240620;

pub fn lex(input: &str) -> Result<Vec<Token>, kiutils_sexpr::ParseError> {
    let knows_bar = prescan_version(input).unwrap_or(0) >= VERSION_KNOWS_BAR;
    lex_with_bar(input, knows_bar)
}

fn decode_quoted_escape(bytes: &[u8], i: &mut usize) -> Result<Vec<u8>, kiutils_sexpr::ParseError> {
    *i += 1;

    if *i >= bytes.len() {
        return Err(kiutils_sexpr::ParseError::UnexpectedEof);
    }

    let escape = bytes[*i];
    *i += 1;

    let decoded = match escape {
        b'"' | b'\\' => vec![escape],
        b'a' => vec![0x07],
        b'b' => vec![0x08],
        b'f' => vec![0x0c],
        b'n' => vec![b'\n'],
        b'r' => vec![b'\r'],
        b't' => vec![b'\t'],
        b'v' => vec![0x0b],
        b'x' => {
            let hex_start = *i;
            let mut hex_end = *i;

            while hex_end < bytes.len()
                && hex_end - hex_start < 2
                && bytes[hex_end].is_ascii_hexdigit()
            {
                hex_end += 1;
            }

            if hex_end > hex_start {
                *i = hex_end;
                let hex = std::str::from_utf8(&bytes[hex_start..hex_end])
                    .map_err(|_| kiutils_sexpr::ParseError::UnexpectedToken(hex_start))?;
                vec![
                    u8::from_str_radix(hex, 16)
                        .map_err(|_| kiutils_sexpr::ParseError::UnexpectedToken(hex_start))?,
                ]
            } else {
                vec![b'x']
            }
        }
        other => {
            let oct_start = *i - 1;
            let mut oct_end = oct_start;

            while oct_end < bytes.len()
                && oct_end - oct_start < 3
                && (b'0'..=b'7').contains(&bytes[oct_end])
            {
                oct_end += 1;
            }

            if oct_end > oct_start {
                *i = oct_end;
                let oct = std::str::from_utf8(&bytes[oct_start..oct_end])
                    .map_err(|_| kiutils_sexpr::ParseError::UnexpectedToken(oct_start))?;
                vec![
                    u8::from_str_radix(oct, 8)
                        .map_err(|_| kiutils_sexpr::ParseError::UnexpectedToken(oct_start))?,
                ]
            } else {
                vec![b'\\', other]
            }
        }
    };

    Ok(decoded)
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
                let mut closed = false;
                while i < bytes.len() {
                    match bytes[i] {
                        b'\\' => {
                            out.extend(decode_quoted_escape(bytes, &mut i)?);
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
                            closed = true;
                            break;
                        }
                        other => {
                            out.push(other);
                            i += 1;
                        }
                    }
                }
                if !closed {
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

#[cfg(test)]
mod tests {
    use super::{AtomClass, TokKind, lex, prescan_version};

    #[test]
    fn prescan_version_skips_fake_version_text_inside_quotes() {
        let input = "(kicad_sch \"(version 1)\"   (version 20260306))";
        assert_eq!(prescan_version(input), Some(20260306));
    }

    #[test]
    fn lex_decodes_kicad_quoted_escape_sequences() {
        let tokens = lex("(text \"a\\\\b\\\"c\\n\\r\\t\\a\\b\\f\\v\\x41\\101\")").expect("lex");
        assert_eq!(
            tokens[2].kind,
            TokKind::Atom("a\\b\"c\n\r\t\u{7}\u{8}\u{c}\u{b}AA".to_string())
        );
        assert_eq!(tokens[2].atom_class, Some(AtomClass::Quoted));
    }

    #[test]
    fn lex_handles_goofed_hex_and_octal_escape_sequences_like_kicad() {
        let tokens = lex("(text \"\\x \\8\")").expect("lex");
        assert_eq!(tokens[2].kind, TokKind::Atom("x \\8".to_string()));
        assert_eq!(tokens[2].atom_class, Some(AtomClass::Quoted));
    }
}
