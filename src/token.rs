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
    pub is_keyword: bool,
    pub span: Span,
}

fn skip_utf8_bom(bytes: &[u8], i: &mut usize) {
    if bytes.get(*i..(*i + 3)) == Some(&[0xEF, 0xBB, 0xBF]) {
        *i += 3;
    }
}

/// Pre-scan the raw file to extract the `(version NNNNN)` number before full tokenization.
/// Returns the version number if found, or `None`.
pub fn prescan_version(input: &str) -> Option<i32> {
    let bytes = input.as_bytes();
    let mut i = 0usize;
    skip_utf8_bom(bytes, &mut i);
    loop {
        skip_whitespace_and_line_comments(bytes, &mut i);

        match bytes.get(i).copied() {
            Some(b'"') => {
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
            Some(b'(') => break,
            _ => return None,
        }
    }

    i += 1;

    skip_whitespace_and_line_comments(bytes, &mut i);

    let root_start = i;
    while i < bytes.len() && !matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r' | b'\0' | b'(' | b')')
    {
        i += 1;
    }

    if root_start == i || &input[root_start..i] != "kicad_sch" {
        return None;
    }

    loop {
        skip_whitespace_and_line_comments(bytes, &mut i);

        match bytes.get(i).copied() {
            Some(b'"') => {
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
            Some(b'(') => break,
            _ => return None,
        }
    }

    i += 1;

    skip_whitespace_and_line_comments(bytes, &mut i);

    let symbol_start = i;
    while i < bytes.len() && !matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r' | b'\0' | b'(' | b')')
    {
        i += 1;
    }

    if symbol_start == i || &input[symbol_start..i] != "version" {
        return None;
    }

    skip_whitespace_and_line_comments(bytes, &mut i);

    let number_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }

    if number_start == i {
        return None;
    }

    input[number_start..i].parse::<i32>().ok()
}

/// The version at which `|` became an s-expression separator in KiCad.
const VERSION_KNOWS_BAR: i32 = 20240620;

fn is_line_comment_start(bytes: &[u8], i: usize) -> bool {
    if bytes.get(i) != Some(&b'#') {
        return false;
    }

    let mut j = i;

    while j > 0 {
        let prev = bytes[j - 1];

        if prev == b'\n' {
            break;
        }

        if !matches!(prev, b' ' | b'\t' | b'\r') {
            return false;
        }

        j -= 1;
    }

    true
}

fn skip_whitespace_and_line_comments(bytes: &[u8], i: &mut usize) {
    loop {
        while *i < bytes.len() && matches!(bytes[*i], b' ' | b'\t' | b'\n' | b'\r' | b'\0') {
            *i += 1;
        }

        if *i < bytes.len() && is_line_comment_start(bytes, *i) {
            while *i < bytes.len() && bytes[*i] != b'\n' {
                *i += 1;
            }
            continue;
        }

        break;
    }
}

fn is_dsn_number(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    let len = bytes.len();
    let mut saw_number = false;

    if i < len && matches!(bytes[i], b'-' | b'+') {
        i += 1;
    }

    while i < len && bytes[i].is_ascii_digit() {
        i += 1;
        saw_number = true;
    }

    if i < len && bytes[i] == b'.' {
        i += 1;

        while i < len && bytes[i].is_ascii_digit() {
            i += 1;
            saw_number = true;
        }
    }

    if saw_number && i < len && matches!(bytes[i], b'e' | b'E') {
        i += 1;
        saw_number = false;

        if i < len && matches!(bytes[i], b'-' | b'+') {
            i += 1;
        }

        while i < len && bytes[i].is_ascii_digit() {
            i += 1;
            saw_number = true;
        }
    }

    saw_number && i == len
}

fn is_schematic_keyword(text: &str) -> bool {
    matches!(
        text,
        "kicad_sch"
            | "version"
            | "generator"
            | "generator_version"
            | "uuid"
            | "paper"
            | "page"
            | "portrait"
            | "title_block"
            | "title"
            | "date"
            | "rev"
            | "company"
            | "comment"
            | "lib_symbols"
            | "symbol"
            | "power"
            | "global"
            | "local"
            | "body_styles"
            | "demorgan"
            | "pin_names"
            | "pin_numbers"
            | "offset"
            | "hide"
            | "effects"
            | "exclude_from_sim"
            | "in_bom"
            | "on_board"
            | "in_pos_files"
            | "duplicate_pin_numbers_are_jumpers"
            | "jumper_pin_groups"
            | "property"
            | "extends"
            | "unit_name"
            | "embedded_fonts"
            | "embedded_files"
            | "file"
            | "host"
            | "name"
            | "checksum"
            | "type"
            | "data"
            | "datasheet"
            | "font"
            | "face"
            | "model"
            | "worksheet"
            | "other"
            | "arc"
            | "bezier"
            | "circle"
            | "pin"
            | "polyline"
            | "rectangle"
            | "text"
            | "text_box"
            | "private"
            | "start"
            | "mid"
            | "end"
            | "center"
            | "radius"
            | "diameter"
            | "at"
            | "length"
            | "angles"
            | "stroke"
            | "fill"
            | "pts"
            | "xy"
            | "width"
            | "color"
            | "line_spacing"
            | "size"
            | "thickness"
            | "bold"
            | "italic"
            | "justify"
            | "left"
            | "right"
            | "top"
            | "bottom"
            | "mirror"
            | "href"
            | "dot"
            | "round"
            | "diamond"
            | "input"
            | "output"
            | "bidirectional"
            | "tri_state"
            | "passive"
            | "unspecified"
            | "power_in"
            | "power_out"
            | "open_collector"
            | "open_emitter"
            | "free"
            | "unconnected"
            | "clock"
            | "inverted"
            | "inverted_clock"
            | "input_low"
            | "clock_low"
            | "output_low"
            | "edge_clock_high"
            | "non_logic"
            | "line"
            | "alternate"
            | "number"
            | "show_name"
            | "do_not_autoplace"
            | "id"
            | "fields_autoplaced"
            | "default_instance"
            | "instances"
            | "project"
            | "path"
            | "reference"
            | "unit"
            | "value"
            | "footprint"
            | "variant"
            | "dnp"
            | "field"
            | "lib_id"
            | "lib_name"
            | "convert"
            | "body_style"
            | "sheet"
            | "background"
            | "sheet_instances"
            | "symbol_instances"
            | "sheetname"
            | "sheetfile"
            | "pin_name"
            | "pin_number"
            | "junction"
            | "no_connect"
            | "bus_entry"
            | "wire"
            | "bus"
            | "rule_area"
            | "label"
            | "global_label"
            | "hierarchical_label"
            | "directive_label"
            | "netclass_flag"
            | "shape"
            | "iref"
            | "table"
            | "column_count"
            | "column_widths"
            | "row_heights"
            | "cells"
            | "table_cell"
            | "span"
            | "border"
            | "external"
            | "header"
            | "margins"
            | "separators"
            | "rows"
            | "cols"
            | "image"
            | "scale"
            | "x"
            | "y"
            | "bus_alias"
            | "members"
            | "group"
            | "locked"
            | "yes"
            | "no"
            | "none"
            | "outline"
            | "hatch"
            | "reverse_hatch"
            | "cross_hatch"
            | "solid"
            | "dash"
            | "|"
    )
}

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
    skip_utf8_bom(bytes, &mut i);

    while i < bytes.len() {
        if is_line_comment_start(bytes, i) {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }

        match bytes[i] {
            b' ' | b'\t' | b'\n' | b'\r' | b'\0' => i += 1,
            b'(' => {
                tokens.push(Token {
                    kind: TokKind::Left,
                    atom_class: None,
                    is_keyword: false,
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
                    is_keyword: false,
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
                                is_keyword: false,
                                span: Span { start, end: i },
                            });
                            closed = true;
                            break;
                        }
                        b'\n' | b'\r' => {
                            return Err(kiutils_sexpr::ParseError::UnexpectedEof);
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
                    is_keyword: true,
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
                    && !matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r' | b'\0' | b'(' | b')')
                    && !(knows_bar && bytes[i] == b'|')
                {
                    i += 1;
                }
                if start == i {
                    return Err(kiutils_sexpr::ParseError::UnexpectedToken(i));
                }
                let text = String::from_utf8(bytes[start..i].to_vec())
                    .map_err(|_| kiutils_sexpr::ParseError::UnexpectedToken(start))?;
                let atom_class = if is_dsn_number(&text) {
                    AtomClass::Number
                } else {
                    AtomClass::Symbol
                };
                let is_keyword = atom_class == AtomClass::Symbol && is_schematic_keyword(&text);
                tokens.push(Token {
                    kind: TokKind::Atom(text),
                    atom_class: Some(atom_class),
                    is_keyword,
                    span: Span { start, end: i },
                });
            }
        }
    }

    tokens.push(Token {
        kind: TokKind::Eof,
        atom_class: None,
        is_keyword: false,
        span: Span {
            start: input.len(),
            end: input.len(),
        },
    });
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::{AtomClass, TokKind, is_dsn_number, lex, prescan_version};

    #[test]
    fn prescan_version_skips_fake_version_text_inside_quotes() {
        let input = "(kicad_sch \"(version 1)\"   (version 20260306))";
        assert_eq!(prescan_version(input), Some(20260306));
    }

    #[test]
    fn prescan_version_ignores_late_or_nested_version_sections() {
        assert_eq!(
            prescan_version("(kicad_sch (generator \"eeschema\") (version 20260306))"),
            None
        );
        assert_eq!(
            prescan_version("(kicad_sch (sheet (property \"Version\" \"1\")) (version 20260306))"),
            None
        );
    }

    #[test]
    fn prescan_version_skips_full_line_comments() {
        let input = "# leading comment\n  # second comment\n(kicad_sch (version 20260306))";
        assert_eq!(prescan_version(input), Some(20260306));
    }

    #[test]
    fn prescan_version_treats_nul_as_whitespace_like_kicad() {
        let input = "\0\0(kicad_sch\0(version\020260306))";
        assert_eq!(prescan_version(input), Some(20260306));
    }

    #[test]
    fn prescan_version_skips_utf8_bom() {
        let input = "\u{feff}(kicad_sch (version 20260306))";
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

    #[test]
    fn lex_rejects_raw_newlines_inside_quoted_atoms() {
        let err = lex("(text \"line1\nline2\")").expect_err("lex must reject raw newline");
        assert_eq!(err, kiutils_sexpr::ParseError::UnexpectedEof);
    }

    #[test]
    fn dsn_number_classifier_matches_kicad_number_grammar() {
        for valid in [
            "0", "1", "-1", "+1", "1.", ".5", "-.5", "+.5", "1.25", "1e3", "1E3", "1e-3", "1e+3",
            ".5e2", "1.e2",
        ] {
            assert!(is_dsn_number(valid), "{valid} should be a DSN number");
        }

        for invalid in [
            "", "+", "-", ".", "+.", "-.", "e3", "1e", "1e+", "1e-", "NaN", "nan", "inf",
            "Infinity", "0x10", "1_0",
        ] {
            assert!(
                !is_dsn_number(invalid),
                "{invalid} should not be a DSN number"
            );
        }
    }

    #[test]
    fn lex_classifies_only_dsn_numbers_as_number_atoms() {
        let tokens = lex("(numbers NaN inf .5 1e3 1e 1.)").expect("lex");
        let atom_classes: Vec<Option<AtomClass>> = tokens
            .into_iter()
            .filter_map(|token| match token.kind {
                TokKind::Atom(_) => Some(token.atom_class),
                _ => None,
            })
            .collect();

        assert_eq!(
            atom_classes,
            vec![
                Some(AtomClass::Symbol),
                Some(AtomClass::Symbol),
                Some(AtomClass::Symbol),
                Some(AtomClass::Number),
                Some(AtomClass::Number),
                Some(AtomClass::Symbol),
                Some(AtomClass::Number),
            ]
        );
    }

    #[test]
    fn lex_skips_full_line_comments_but_keeps_hash_inside_atoms() {
        let tokens =
            lex("# comment line\n(kicad_sch\n  # another comment\n  (uuid \"#root\")\n  (text foo#bar))")
                .expect("lex");
        let atoms: Vec<(String, Option<AtomClass>)> = tokens
            .into_iter()
            .filter_map(|token| match token.kind {
                TokKind::Atom(value) => Some((value, token.atom_class)),
                _ => None,
            })
            .collect();

        assert!(atoms.iter().any(|(value, _)| value == "kicad_sch"));
        assert!(
            atoms
                .iter()
                .any(|(value, class)| value == "#root" && *class == Some(AtomClass::Quoted))
        );
        assert!(
            atoms
                .iter()
                .any(|(value, class)| value == "foo#bar" && *class == Some(AtomClass::Symbol))
        );
        assert!(!atoms.iter().any(|(value, _)| value.contains("comment")));
    }

    #[test]
    fn lex_treats_nul_as_whitespace_like_kicad() {
        let tokens = lex("(kicad_sch\0(uuid\0\"root\"))").expect("lex");
        let atoms: Vec<(String, Option<AtomClass>)> = tokens
            .into_iter()
            .filter_map(|token| match token.kind {
                TokKind::Atom(value) => Some((value, token.atom_class)),
                _ => None,
            })
            .collect();

        assert_eq!(
            atoms,
            vec![
                ("kicad_sch".to_string(), Some(AtomClass::Symbol)),
                ("uuid".to_string(), Some(AtomClass::Symbol)),
                ("root".to_string(), Some(AtomClass::Quoted)),
            ]
        );
    }

    #[test]
    fn lex_skips_utf8_bom() {
        let tokens = lex("\u{feff}(kicad_sch (uuid \"root\"))").expect("lex");
        let atoms: Vec<(String, Option<AtomClass>)> = tokens
            .into_iter()
            .filter_map(|token| match token.kind {
                TokKind::Atom(value) => Some((value, token.atom_class)),
                _ => None,
            })
            .collect();

        assert_eq!(
            atoms,
            vec![
                ("kicad_sch".to_string(), Some(AtomClass::Symbol)),
                ("uuid".to_string(), Some(AtomClass::Symbol)),
                ("root".to_string(), Some(AtomClass::Quoted)),
            ]
        );
    }

    #[test]
    fn lex_marks_reserved_unquoted_keywords() {
        let tokens = lex("(items hide plain 1 \"hide\")").expect("lex");
        let atoms: Vec<(String, Option<AtomClass>, bool)> = tokens
            .into_iter()
            .filter_map(|token| match token.kind {
                TokKind::Atom(value) => Some((value, token.atom_class, token.is_keyword)),
                _ => None,
            })
            .collect();

        assert_eq!(
            atoms,
            vec![
                ("items".to_string(), Some(AtomClass::Symbol), false),
                ("hide".to_string(), Some(AtomClass::Symbol), true),
                ("plain".to_string(), Some(AtomClass::Symbol), false),
                ("1".to_string(), Some(AtomClass::Number), false),
                ("hide".to_string(), Some(AtomClass::Quoted), false),
            ]
        );
    }

    #[test]
    fn lex_marks_parser_only_reserved_heads_as_keywords() {
        let tokens = lex(
            "(items host margins face diameter dot round diamond power_in inverted_clock effects x y)",
        )
        .expect("lex");
        let atoms: Vec<(String, bool)> = tokens
            .into_iter()
            .filter_map(|token| match token.kind {
                TokKind::Atom(value) => Some((value, token.is_keyword)),
                _ => None,
            })
            .collect();

        assert_eq!(
            atoms,
            vec![
                ("items".to_string(), false),
                ("host".to_string(), true),
                ("margins".to_string(), true),
                ("face".to_string(), true),
                ("diameter".to_string(), true),
                ("dot".to_string(), true),
                ("round".to_string(), true),
                ("diamond".to_string(), true),
                ("power_in".to_string(), true),
                ("inverted_clock".to_string(), true),
                ("effects".to_string(), true),
                ("x".to_string(), true),
                ("y".to_string(), true),
            ]
        );
    }
}
