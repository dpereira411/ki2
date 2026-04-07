use std::path::{Path, PathBuf};

use crate::diagnostic::Diagnostic;
use crate::error::Error;
use crate::model::Paper;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorksheetCornerAnchor {
    LeftTop,
    LeftBottom,
    RightBottom,
    RightTop,
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
    anchor: WorksheetCornerAnchor,
    repeat_count: i32,
    incrx: f64,
    incry: f64,
    incrlabel: i32,
    page_option: WorksheetPageOption,
}

const REDUCED_DEFAULT_DRAWING_SHEET: &str = r#"(kicad_wks
  (version 20210606)
  (generator pl_editor)
  (setup (textsize 1.5 1.5)(linewidth 0.15)(textlinewidth 0.15)
    (left_margin 10)(right_margin 10)(top_margin 10)(bottom_margin 10))
  (rect (name "") (start 110 34) (end 2 2) (comment "rect around the title block"))
  (rect (name "") (start 0 0 ltcorner) (end 0 0) (repeat 2) (incrx 2) (incry 2))
  (line (name "") (start 50 2 ltcorner) (end 50 0 ltcorner) (repeat 30) (incrx 50))
  (tbtext "1" (name "") (pos 25 1 ltcorner) (font (size 1.3 1.3)) (repeat 100) (incrx 50))
  (line (name "") (start 50 2 lbcorner) (end 50 0 lbcorner) (repeat 30) (incrx 50))
  (tbtext "1" (name "") (pos 25 1 lbcorner) (font (size 1.3 1.3)) (repeat 100) (incrx 50))
  (line (name "") (start 0 50 ltcorner) (end 2 50 ltcorner) (repeat 30) (incry 50))
  (tbtext "A" (name "") (pos 1 25 ltcorner) (font (size 1.3 1.3)) (justify center) (repeat 100) (incry 50))
  (line (name "") (start 0 50 rtcorner) (end 2 50 rtcorner) (repeat 30) (incry 50))
  (tbtext "A" (name "") (pos 1 25 rtcorner) (font (size 1.3 1.3)) (justify center) (repeat 100) (incry 50))
  (tbtext "Date: ${ISSUE_DATE}" (name "") (pos 87 6.9))
  (line (name "") (start 110 5.5) (end 2 5.5))
  (tbtext "${KICAD_VERSION}" (name "") (pos 109 4.1) (comment "Kicad version"))
  (line (name "") (start 110 8.5) (end 2 8.5))
  (tbtext "Rev: ${REVISION}" (name "") (pos 24 6.9) (font bold))
  (tbtext "Size: ${PAPER}" (name "") (pos 109 6.9) (comment "Paper format name"))
  (tbtext "Id: ${#}/${##}" (name "") (pos 24 4.1) (comment "Sheet id"))
  (line (name "") (start 110 12.5) (end 2 12.5))
  (tbtext "Title: ${TITLE}" (name "") (pos 109 10.7) (font (size 2 2) bold italic))
  (tbtext "File: ${FILENAME}" (name "") (pos 109 14.3))
  (line (name "") (start 110 18.5) (end 2 18.5))
  (tbtext "Sheet: ${SHEETPATH}" (name "") (pos 109 17))
  (tbtext "${COMPANY}" (name "") (pos 109 20) (font bold) (comment "Company name"))
  (tbtext "${COMMENT1}" (name "") (pos 109 23) (comment "Comment 0"))
  (tbtext "${COMMENT2}" (name "") (pos 109 26) (comment "Comment 1"))
  (tbtext "${COMMENT3}" (name "") (pos 109 29) (comment "Comment 2"))
  (tbtext "${COMMENT4}" (name "") (pos 109 32) (comment "Comment 3"))
  (line (name "") (start 90 8.5) (end 90 5.5))
  (line (name "") (start 26 8.5) (end 26 2)))"#;

// Upstream parity: reduced local analogue for KiCad's built-in default worksheet load path. This
// is not 1:1 because the local tree still parses only the exercised text-bearing worksheet slice,
// but it now also applies KiCad's first-page/subsequent-page gating before returning text items.
pub fn default_reduced_worksheet_text_items(
    current_virtual_page_number: Option<usize>,
    paper: Option<&Paper>,
) -> Result<Vec<WorksheetTextItem>, Error> {
    parse_reduced_worksheet_text_items(
        Path::new("<default drawing sheet>"),
        REDUCED_DEFAULT_DRAWING_SHEET,
        current_virtual_page_number,
        paper,
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
    paper: Option<&Paper>,
) -> Result<Vec<WorksheetTextItem>, Error> {
    let tokens = lex(raw).map_err(|source| Error::SExpr {
        path: path.to_path_buf(),
        location: worksheet_sexpr_error_location(raw, &source),
        source,
    })?;

    Ok(
        ReducedWorksheetParser::new(path.to_path_buf(), raw, tokens, paper)
            .parse()?
            .into_iter()
            .filter(|item| include_on_page(item.page_option, current_virtual_page_number))
            .map(|item| WorksheetTextItem {
                text: item.text,
                at: item.at,
            })
            .collect(),
    )
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
    paper_size_mm: Option<[f64; 2]>,
    margins_mm: [f64; 4],
}

impl ReducedWorksheetParser {
    fn new(path: PathBuf, source: &str, tokens: Vec<Token>, paper: Option<&Paper>) -> Self {
        Self {
            path,
            source: source.to_string(),
            tokens,
            idx: 0,
            paper_size_mm: paper.and_then(|paper| match (paper.width, paper.height) {
                (Some(width), Some(height)) => Some([width, height]),
                _ => None,
            }),
            margins_mm: [10.0, 10.0, 10.0, 10.0],
        }
    }

    // Upstream parity: reduced local analogue for the `DRAWING_SHEET_PARSER::Parse()` text-item
    // slice that ERC `TestTextVars()` needs. This is not a 1:1 worksheet parser because the local
    // tree currently skips non-text drawing-sheet items and most text styling branches, but it now
    // also preserves the exercised `tbtext` repeat/increment, page-option, and reduced
    // page-environment behavior instead of collapsing every text item to one copy. Fuller styling
    // semantics are still unported.
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
                "setup" => self.parse_setup()?,
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
            anchor: WorksheetCornerAnchor::RightBottom,
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
                    (item.at, item.anchor) = self.parse_pos()?;
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
        Ok(self.expand_repeated_text_item(&item))
    }

    fn parse_pos(&mut self) -> Result<([f64; 2], WorksheetCornerAnchor), Error> {
        let x = self.parse_double("x coordinate")?;
        let y = self.parse_double("y coordinate")?;
        let mut anchor = WorksheetCornerAnchor::RightBottom;

        if !self.at_right() {
            let span = self.current_span();
            anchor = match self.need_atom("corner")?.as_str() {
                "ltcorner" => WorksheetCornerAnchor::LeftTop,
                "lbcorner" => WorksheetCornerAnchor::LeftBottom,
                "rbcorner" => WorksheetCornerAnchor::RightBottom,
                "rtcorner" => WorksheetCornerAnchor::RightTop,
                _ => return Err(self.validation(span, "invalid worksheet corner")),
            };
        }

        self.need_right()?;
        Ok(([x, y], anchor))
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

    // Upstream parity: reduced local analogue for the worksheet `setup` branch that feeds
    // `DS_DATA_MODEL::SetupDrawEnvironment()`. This is not 1:1 because the local worksheet model
    // only keeps the exercised page margins needed to position and clip reduced `tbtext` items.
    fn parse_setup(&mut self) -> Result<(), Error> {
        while !self.at_right() {
            self.need_left()?;
            let head = self.need_atom("worksheet setup child")?;

            match head.as_str() {
                "left_margin" => {
                    self.margins_mm[0] = self.parse_double("left margin")?;
                    self.need_right()?;
                }
                "right_margin" => {
                    self.margins_mm[1] = self.parse_double("right margin")?;
                    self.need_right()?;
                }
                "top_margin" => {
                    self.margins_mm[2] = self.parse_double("top margin")?;
                    self.need_right()?;
                }
                "bottom_margin" => {
                    self.margins_mm[3] = self.parse_double("bottom margin")?;
                    self.need_right()?;
                }
                _ => self.skip_current_list_body()?,
            }
        }

        self.need_right()
    }

    // Upstream parity: reduced local analogue for the worksheet text materialization path in
    // `DS_DATA_ITEM_TEXT::SyncDrawItems()`. This is not 1:1 because the local tree still skips the
    // full draw-item/styling object model, but it now resolves corner-anchored positions and clips
    // repeated `tbtext` items against the current page bounds before ERC sees them.
    fn expand_repeated_text_item(
        &self,
        item: &WorksheetTextTemplate,
    ) -> Vec<ParsedWorksheetTextItem> {
        let repeat_count = item.repeat_count.clamp(1, 100);
        let mut out = Vec::new();

        for index in 0..repeat_count {
            let at = self.resolve_position(item, index);

            if index > 0 && !self.is_inside_page(at) {
                continue;
            }

            let expanded_text = replace_worksheet_backslash_sequences(&item.text);
            out.push(ParsedWorksheetTextItem {
                text: increment_label_text(&expanded_text, index * item.incrlabel, index == 0),
                at,
                page_option: item.page_option,
            });
        }

        out
    }

    fn resolve_position(&self, item: &WorksheetTextTemplate, index: i32) -> [f64; 2] {
        let base_x = item.at[0] + (item.incrx * f64::from(index));
        let base_y = item.at[1] + (item.incry * f64::from(index));
        let Some([paper_width, paper_height]) = self.paper_size_mm else {
            return [base_x, base_y];
        };
        let [left, right, top, bottom] = self.margins_mm;
        let lt_x = left;
        let lt_y = top;
        let rb_x = paper_width - right;
        let rb_y = paper_height - bottom;

        match item.anchor {
            WorksheetCornerAnchor::LeftTop => [lt_x + base_x, lt_y + base_y],
            WorksheetCornerAnchor::LeftBottom => [lt_x + base_x, rb_y - base_y],
            WorksheetCornerAnchor::RightBottom => [rb_x - base_x, rb_y - base_y],
            WorksheetCornerAnchor::RightTop => [rb_x - base_x, lt_y + base_y],
        }
    }

    fn is_inside_page(&self, at: [f64; 2]) -> bool {
        let Some([paper_width, paper_height]) = self.paper_size_mm else {
            return true;
        };
        let [left, right, top, bottom] = self.margins_mm;
        let lt_x = left;
        let lt_y = top;
        let rb_x = paper_width - right;
        let rb_y = paper_height - bottom;

        at[0] >= lt_x && at[0] <= rb_x && at[1] >= lt_y && at[1] <= rb_y
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

// Upstream parity: reduced analogue for `DS_DATA_ITEM_TEXT::ReplaceAntiSlashSequence()`. This is
// intentionally narrower than the full drawing-sheet text object flow, but it preserves the
// exercised newline and escaped-backslash behavior before ERC consumes worksheet text.
pub(crate) fn replace_worksheet_backslash_sequences(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();

    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }

        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('n') => out.push('\n'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }

    out
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
