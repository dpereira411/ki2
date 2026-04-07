use std::path::{Path, PathBuf};

use crate::diagnostic::Diagnostic;
use crate::error::Error;
use crate::sexpr::{ParseError, Span};
use crate::token::{TokKind, Token, lex};

#[derive(Debug, Clone, PartialEq)]
pub struct WorksheetTextItem {
    pub text: String,
    pub at: [f64; 2],
}

pub fn parse_reduced_worksheet_text_items(
    path: &Path,
    raw: &str,
) -> Result<Vec<WorksheetTextItem>, Error> {
    let tokens = lex(raw).map_err(|source| Error::SExpr {
        path: path.to_path_buf(),
        location: worksheet_sexpr_error_location(raw, &source),
        source,
    })?;

    ReducedWorksheetParser::new(path.to_path_buf(), raw, tokens).parse()
}

fn worksheet_sexpr_error_location(raw: &str, source: &ParseError) -> String {
    let offset = match source {
        ParseError::UnexpectedToken(offset) | ParseError::MaxNestingExceeded(offset) => {
            Some(*offset)
        }
        ParseError::UnexpectedEof => Some(raw.len()),
        ParseError::ExpectedSingleRoot(_) => None,
    };

    offset
        .map(|offset| {
            let prefix = &raw[..offset.min(raw.len())];
            let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
            let column = prefix
                .rsplit('\n')
                .next()
                .map(|line| line.chars().count() + 1)
                .unwrap_or(1);
            format!(":{line}:{column}")
        })
        .unwrap_or_default()
}

struct ReducedWorksheetParser {
    path: PathBuf,
    source: String,
    tokens: Vec<Token>,
    idx: usize,
}

impl ReducedWorksheetParser {
    fn new(path: PathBuf, source: &str, tokens: Vec<Token>) -> Self {
        Self {
            path,
            source: source.to_string(),
            tokens,
            idx: 0,
        }
    }

    // Upstream parity: reduced local analogue for the `DRAWING_SHEET_PARSER::Parse()` text-item
    // slice that ERC `TestTextVars()` needs. This is not a 1:1 worksheet parser because the local
    // tree currently skips non-text drawing-sheet items and most text styling branches; it exists
    // to unlock the exercised `tbtext` shown-text/assertion path before the fuller drawing-sheet
    // engine is ported.
    fn parse(&mut self) -> Result<Vec<WorksheetTextItem>, Error> {
        self.need_left()?;
        let head = self.need_atom("kicad_wks or drawing_sheet")?;

        if !matches!(head.as_str(), "kicad_wks" | "drawing_sheet" | "page_layout") {
            return Err(
                self.validation(self.current_span(), "expecting kicad_wks or drawing_sheet")
            );
        }

        let mut items = Vec::new();

        while !self.at_right() {
            self.need_left()?;
            let head = self.need_atom("worksheet item")?;

            match head.as_str() {
                "tbtext" => items.push(self.parse_tbtext()?),
                _ => self.skip_current_list_body()?,
            }
        }

        self.need_right()?;
        Ok(items)
    }

    fn parse_tbtext(&mut self) -> Result<WorksheetTextItem, Error> {
        let raw_text = self.need_symbol_number_or_quoted("text")?;
        let mut item = WorksheetTextItem {
            text: convert_legacy_drawing_sheet_text(&raw_text),
            at: [0.0, 0.0],
        };

        while !self.at_right() {
            self.need_left()?;
            let head = self.need_atom("tbtext child")?;

            match head.as_str() {
                "pos" => {
                    item.at = self.parse_pos()?;
                }
                _ => self.skip_current_list_body()?,
            }
        }

        self.need_right()?;
        Ok(item)
    }

    fn parse_pos(&mut self) -> Result<[f64; 2], Error> {
        let x = self.parse_double("x coordinate")?;
        let y = self.parse_double("y coordinate")?;

        if !self.at_right() {
            let _ = self.need_atom("corner")?;
        }

        self.need_right()?;
        Ok([x, y])
    }

    fn parse_double(&mut self, expected: &str) -> Result<f64, Error> {
        let token = self.current_token().clone();

        if let TokKind::Atom(value) = &token.kind {
            self.idx += 1;
            return value.parse::<f64>().map_err(|_| Error::Validation {
                path: self.path.clone(),
                diagnostic: Diagnostic::validation(
                    "worksheet-number",
                    format!("invalid {expected}"),
                )
                .with_path(self.path.clone())
                .with_span(token.span)
                .with_position(self.line(token.span.start), self.column(token.span.start)),
            });
        }

        Err(self.validation(token.span, format!("expecting {expected}")))
    }

    fn skip_current_list_body(&mut self) -> Result<(), Error> {
        while !self.at_right() {
            if self.at_left() {
                self.need_left()?;
                let _ = self.need_atom("list head")?;
                self.skip_current_list_body()?;
                continue;
            }

            self.idx += 1;
        }

        self.need_right()
    }

    fn at_left(&self) -> bool {
        matches!(self.current_token().kind, TokKind::Left)
    }

    fn at_right(&self) -> bool {
        matches!(self.current_token().kind, TokKind::Right)
    }

    fn need_left(&mut self) -> Result<(), Error> {
        let token = self.current_token().clone();

        if matches!(token.kind, TokKind::Left) {
            self.idx += 1;
            Ok(())
        } else {
            Err(self.validation(token.span, "expecting ("))
        }
    }

    fn need_right(&mut self) -> Result<(), Error> {
        let token = self.current_token().clone();

        if matches!(token.kind, TokKind::Right) {
            self.idx += 1;
            Ok(())
        } else {
            Err(self.validation(token.span, "expecting )"))
        }
    }

    fn need_atom(&mut self, expected: impl Into<String>) -> Result<String, Error> {
        let token = self.current_token().clone();

        if let TokKind::Atom(value) = &token.kind {
            self.idx += 1;
            return Ok(value.clone());
        }

        Err(self.validation(token.span, expected))
    }

    fn need_symbol_number_or_quoted(
        &mut self,
        expected: impl Into<String>,
    ) -> Result<String, Error> {
        self.need_atom(expected)
    }

    fn current_token(&self) -> &Token {
        self.tokens.get(self.idx).unwrap_or_else(|| {
            self.tokens
                .last()
                .expect("lexed token stream must have eof")
        })
    }

    fn current_span(&self) -> Span {
        self.current_token().span
    }

    fn validation(&self, span: Span, message: impl Into<String>) -> Error {
        let start = span.start.min(self.source.len());
        Error::Validation {
            path: self.path.clone(),
            diagnostic: Diagnostic::validation("worksheet-parse", message.into())
                .with_path(self.path.clone())
                .with_span(span)
                .with_position(self.line(start), self.column(start)),
        }
    }

    fn line(&self, offset: usize) -> usize {
        self.source[..offset]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1
    }

    fn column(&self, offset: usize) -> usize {
        self.source[..offset]
            .rsplit('\n')
            .next()
            .map(|line| line.chars().count() + 1)
            .unwrap_or(1)
    }
}

fn convert_legacy_drawing_sheet_text(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut index = 0;

    while index < chars.len() {
        if chars[index] != '%' {
            out.push(chars[index]);
            index += 1;
            continue;
        }

        index += 1;
        if index >= chars.len() {
            break;
        }

        match chars[index] {
            '%' => out.push('%'),
            'D' => out.push_str("${ISSUE_DATE}"),
            'R' => out.push_str("${REVISION}"),
            'K' => out.push_str("${KICAD_VERSION}"),
            'Z' => out.push_str("${PAPER}"),
            'S' => out.push_str("${#}"),
            'N' => out.push_str("${##}"),
            'F' => out.push_str("${FILENAME}"),
            'L' => out.push_str("${LAYER}"),
            'P' => out.push_str("${SHEETPATH}"),
            'Y' => out.push_str("${COMPANY}"),
            'T' => out.push_str("${TITLE}"),
            'C' => {
                index += 1;
                if index >= chars.len() {
                    break;
                }
                match chars[index] {
                    '0' => out.push_str("${COMMENT1}"),
                    '1' => out.push_str("${COMMENT2}"),
                    '2' => out.push_str("${COMMENT3}"),
                    '3' => out.push_str("${COMMENT4}"),
                    '4' => out.push_str("${COMMENT5}"),
                    '5' => out.push_str("${COMMENT6}"),
                    '6' => out.push_str("${COMMENT7}"),
                    '7' => out.push_str("${COMMENT8}"),
                    '8' => out.push_str("${COMMENT9}"),
                    other => {
                        out.push('%');
                        out.push('C');
                        out.push(other);
                    }
                }
            }
            other => {
                out.push('%');
                out.push(other);
            }
        }

        index += 1;
    }

    out
}
