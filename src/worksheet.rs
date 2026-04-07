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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorksheetPageOption {
    AllPages,
    FirstPageOnly,
    SubsequentPages,
}

#[derive(Debug, Clone)]
struct ParsedWorksheetTextItem {
    text: String,
    at: [f64; 2],
    page_option: WorksheetPageOption,
}

#[derive(Debug, Clone)]
struct WorksheetTextTemplate {
    text: String,
    at: [f64; 2],
    repeat_count: i32,
    incrx: f64,
    incry: f64,
    incrlabel: i32,
    page_option: WorksheetPageOption,
}

const REDUCED_DEFAULT_DRAWING_SHEET: &str = r#"(kicad_wks
  (version 20210606)
  (generator pl_editor)
  (tbtext "Date: ${ISSUE_DATE}" (pos 87 6.9))
  (tbtext "${KICAD_VERSION}" (pos 109 4.1))
  (tbtext "Rev: ${REVISION}" (pos 24 6.9))
  (tbtext "Size: ${PAPER}" (pos 109 6.9))
  (tbtext "Id: ${#}/${##}" (pos 24 4.1))
  (tbtext "Title: ${TITLE}" (pos 109 10.7))
  (tbtext "File: ${FILENAME}" (pos 109 14.3))
  (tbtext "Sheet: ${SHEETPATH}" (pos 109 17))
  (tbtext "${COMPANY}" (pos 109 20))
  (tbtext "${COMMENT1}" (pos 109 23))
  (tbtext "${COMMENT2}" (pos 109 26))
  (tbtext "${COMMENT3}" (pos 109 29))
  (tbtext "${COMMENT4}" (pos 109 32)))"#;

// Upstream parity: reduced local analogue for KiCad's built-in default worksheet load path. This
// is not 1:1 because the local tree still parses only the exercised text-bearing worksheet slice,
// but it now also applies KiCad's first-page/subsequent-page gating before returning text items.
pub fn default_reduced_worksheet_text_items(
    current_virtual_page_number: Option<usize>,
) -> Result<Vec<WorksheetTextItem>, Error> {
    parse_reduced_worksheet_text_items(
        Path::new("<default drawing sheet>"),
        REDUCED_DEFAULT_DRAWING_SHEET,
        current_virtual_page_number,
    )
}

// Upstream parity: reduced local analogue for the worksheet text-item load path feeding
// `DS_DRAW_ITEM_TEXT` generation. This is not a full 1:1 worksheet parser because the local model
// only returns filtered `tbtext` items, but it now keeps the exercised repeat/increment/page
// option behavior before ERC/text-variable consumers see those items.
pub fn parse_reduced_worksheet_text_items(
    path: &Path,
    raw: &str,
    current_virtual_page_number: Option<usize>,
) -> Result<Vec<WorksheetTextItem>, Error> {
    let tokens = lex(raw).map_err(|source| Error::SExpr {
        path: path.to_path_buf(),
        location: worksheet_sexpr_error_location(raw, &source),
        source,
    })?;

    Ok(ReducedWorksheetParser::new(path.to_path_buf(), raw, tokens)
        .parse()?
        .into_iter()
        .filter(|item| include_on_page(item.page_option, current_virtual_page_number))
        .map(|item| WorksheetTextItem {
            text: item.text,
            at: item.at,
        })
        .collect())
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
    // tree currently skips non-text drawing-sheet items and most text styling branches, but it now
    // also preserves the exercised `tbtext` repeat/increment and page-option behavior instead of
    // collapsing every text item to one copy. Fuller styling semantics are still unported.
    fn parse(&mut self) -> Result<Vec<ParsedWorksheetTextItem>, Error> {
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
                "tbtext" => items.extend(self.parse_tbtext()?),
                _ => self.skip_current_list_body()?,
            }
        }

        self.need_right()?;
        Ok(items)
    }

    // Upstream parity: reduced local analogue for the `tbtext` branch in
    // `DRAWING_SHEET_PARSER::parseGraphic()`. This is not a 1:1 branch yet because the local
    // carrier only preserves text position plus the exercised repeat/increment controls, while
    // upstream also owns page-option, style, justification, and formatting state on worksheet
    // text items.
    fn parse_tbtext(&mut self) -> Result<Vec<ParsedWorksheetTextItem>, Error> {
        let raw_text = self.need_symbol_number_or_quoted("text")?;
        let mut item = WorksheetTextTemplate {
            text: convert_legacy_drawing_sheet_text(&raw_text),
            at: [0.0, 0.0],
            repeat_count: 1,
            incrx: 0.0,
            incry: 0.0,
            incrlabel: 1,
            page_option: WorksheetPageOption::AllPages,
        };

        while !self.at_right() {
            self.need_left()?;
            let head = self.need_atom("tbtext child")?;

            match head.as_str() {
                "pos" => {
                    item.at = self.parse_pos()?;
                }
                "repeat" => {
                    item.repeat_count = self.parse_int("repeat count")?;
                    self.need_right()?;
                }
                "incrx" => {
                    item.incrx = self.parse_double("x increment")?;
                    self.need_right()?;
                }
                "incry" => {
                    item.incry = self.parse_double("y increment")?;
                    self.need_right()?;
                }
                "incrlabel" => {
                    item.incrlabel = self.parse_int("label increment")?;
                    self.need_right()?;
                }
                "option" => {
                    item.page_option = self.parse_page_option()?;
                }
                _ => self.skip_current_list_body()?,
            }
        }

        self.need_right()?;
        Ok(expand_repeated_text_item(&item))
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

    // Upstream parity: reduced local analogue for `DRAWING_SHEET_PARSER::readOption()`. This is
    // not a full 1:1 worksheet-item option parser because the reduced worksheet model only keeps
    // the exercised first-page/subsequent-page visibility gate for `tbtext` items.
    fn parse_page_option(&mut self) -> Result<WorksheetPageOption, Error> {
        let mut option = WorksheetPageOption::AllPages;

        while !self.at_right() {
            let span = self.current_span();
            let head = self.need_atom("worksheet page option")?;

            option = match head.as_str() {
                "page1only" => WorksheetPageOption::FirstPageOnly,
                "notonpage1" => WorksheetPageOption::SubsequentPages,
                _ => return Err(self.validation(span, "invalid worksheet page option")),
            };
        }

        self.need_right()?;
        Ok(option)
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

    // Upstream parity: local worksheet-only integer reader used to keep `tbtext` repeat and
    // increment branches on numeric-token validation instead of collapsing them into generic text
    // parsing. It still exists because the reduced worksheet parser is separate from the main
    // schematic parser/token helpers.
    fn parse_int(&mut self, expected: &str) -> Result<i32, Error> {
        let token = self.current_token().clone();

        if let TokKind::Atom(value) = &token.kind {
            self.idx += 1;
            return value.parse::<i32>().map_err(|_| Error::Validation {
                path: self.path.clone(),
                diagnostic: Diagnostic::validation("worksheet-int", format!("invalid {expected}"))
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

// Upstream parity: reduced analogue for KiCad's worksheet text repetition expansion. This is not
// a full 1:1 drawing-sheet item materializer because the local worksheet model only emits the
// repeated text payloads ERC/text-variable checks currently observe.
fn expand_repeated_text_item(item: &WorksheetTextTemplate) -> Vec<ParsedWorksheetTextItem> {
    let repeat_count = item.repeat_count.clamp(1, 100);

    (0..repeat_count)
        .map(|index| ParsedWorksheetTextItem {
            text: increment_label_text(&item.text, index * item.incrlabel, index == 0),
            at: [
                item.at[0] + (item.incrx * f64::from(index)),
                item.at[1] + (item.incry * f64::from(index)),
            ],
            page_option: item.page_option,
        })
        .collect()
}

// Upstream parity: reduced local analogue for `DS_DRAW_ITEM_LIST::BuildDrawItemsList()` page-one
// gating before worksheet text reaches ERC/text-variable consumers. It still only covers the
// reduced worksheet text carrier, not the full drawing-sheet draw-item list.
fn include_on_page(
    option: WorksheetPageOption,
    current_virtual_page_number: Option<usize>,
) -> bool {
    let is_first_page = current_virtual_page_number.unwrap_or(1) == 1;

    match option {
        WorksheetPageOption::AllPages => true,
        WorksheetPageOption::FirstPageOnly => is_first_page,
        WorksheetPageOption::SubsequentPages => !is_first_page,
    }
}

// Upstream parity: reduced analogue for KiCad's repeated worksheet label increment behavior. It is
// intentionally narrower than the full worksheet text formatter and only covers the exercised
// single-line numeric/alphabetic suffix cases needed by repeated `tbtext` items.
fn increment_label_text(text: &str, increment: i32, first: bool) -> String {
    if first || text.contains('\n') || text.is_empty() {
        return text.to_string();
    }

    let mut chars = text.chars().collect::<Vec<_>>();
    let Some(last) = chars.pop() else {
        return text.to_string();
    };
    let mut out = chars.into_iter().collect::<String>();

    if last.is_ascii_digit() {
        let base = i32::from((last as u8) - b'0');
        out.push_str(&(base + increment).to_string());
        return out;
    }

    if last.is_ascii() {
        let base = last as i32;
        let next = base + increment;
        if let Some(next) = u32::try_from(next).ok().and_then(char::from_u32) {
            out.push(next);
            return out;
        }
    }

    out.push(last);
    out
}
