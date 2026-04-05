use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use base64::Engine;
use kiutils_sexpr::Span;
use uuid::Uuid;

use crate::diagnostic::Diagnostic;
use crate::error::Error;
use crate::model::{
    BusAlias, BusEntry, EmbeddedFile, EmbeddedFileType, FieldAutoplacement, Fill, FillType, Group,
    Image, ItemVariant, Junction, Label, LabelKind, LabelShape, LabelSpin, LibDrawItem,
    LibPinAlternate, LibSymbol, Line, LineKind, MirrorAxis, NoConnect, Page, Paper, Property,
    PropertyKind, RootSheet, SchItem, Schematic, Screen, Shape, ShapeKind, Sheet, SheetInstance,
    SheetLocalInstance, SheetPin, SheetPinShape, SheetSide, Stroke, StrokeStyle, Symbol,
    SymbolInstance, SymbolLocalInstance, SymbolPin, Table, TableCell, Text, TextBox, TextEffects,
    TextHJustify, TextKind, TextVJustify, TitleBlock,
};
use crate::token::{AtomClass, TokKind, Token, lex};

const SEXPR_SCHEMATIC_FILE_VERSION: i32 = 20260306;
const VERSION_GENERATOR_VERSION: i32 = 20231120;
const VERSION_TABLES: i32 = 20240101;
const VERSION_RULE_AREAS: i32 = 20240417;
const VERSION_EMBEDDED_FILES: i32 = 20240620;

const VERSION_PAGE_RENAMED_TO_PAPER: i32 = 20200506;
const VERSION_EMPTY_TILDE_IS_EMPTY: i32 = 20250318;
const VERSION_SHEET_INSTANCE_ROOT_PATH: i32 = 20221002;
const VERSION_SKIP_EMPTY_ROOT_SHEET_INSTANCE_PATH: i32 = 20221110;
const VERSION_NEW_OVERBAR_NOTATION: i32 = 20210621;
const VERSION_TEXT_OVERBAR_NOTATION: i32 = 20210606;
const VERSION_IMAGE_PPI_SCALE_ADJUSTMENT: i32 = 20230121;
const VERSION_VARIANT_IN_BOM_FIX: i32 = 20260306;
const VERSION_SYMBOL_PIN_UUID: i32 = 20210126;
const VERSION_CUSTOM_BODY_STYLES: i32 = 20250827;
const VERSION_WRONG_SHEET_FIELD_IDS: i32 = 20200310;
const DEFAULT_LINE_WIDTH_MM: f64 = 0.1524;
const DEFAULT_TEXT_SIZE_MM: f64 = 1.27;
const TEXT_MIN_SIZE_MM: f64 = 0.001;
const TEXT_MAX_SIZE_MM: f64 = 250.0;
const MIN_PAGE_SIZE_MM: f64 = 25.4;
const MAX_PAGE_SIZE_EESCHEMA_MM: f64 = 120000.0 * 0.0254;
const SIM_LEGACY_ENABLE_FIELD_V7: &str = "Sim.Enable";

#[derive(Clone, Copy)]
struct StandardPageInfo {
    kind: &'static str,
    dimensions_mm: Option<[f64; 2]>,
}

const STANDARD_PAGE_INFOS: &[StandardPageInfo] = &[
    StandardPageInfo {
        kind: "A5",
        dimensions_mm: Some([210.0, 148.0]),
    },
    StandardPageInfo {
        kind: "A4",
        dimensions_mm: Some([297.0, 210.0]),
    },
    StandardPageInfo {
        kind: "A3",
        dimensions_mm: Some([420.0, 297.0]),
    },
    StandardPageInfo {
        kind: "A2",
        dimensions_mm: Some([594.0, 420.0]),
    },
    StandardPageInfo {
        kind: "A1",
        dimensions_mm: Some([841.0, 594.0]),
    },
    StandardPageInfo {
        kind: "A0",
        dimensions_mm: Some([1189.0, 841.0]),
    },
    StandardPageInfo {
        kind: "A",
        dimensions_mm: Some([279.4, 215.9]),
    },
    StandardPageInfo {
        kind: "B",
        dimensions_mm: Some([431.8, 279.4]),
    },
    StandardPageInfo {
        kind: "C",
        dimensions_mm: Some([558.8, 431.8]),
    },
    StandardPageInfo {
        kind: "D",
        dimensions_mm: Some([863.6, 558.8]),
    },
    StandardPageInfo {
        kind: "E",
        dimensions_mm: Some([1117.6, 863.6]),
    },
    StandardPageInfo {
        kind: "GERBER",
        dimensions_mm: Some([812.8, 812.8]),
    },
    StandardPageInfo {
        kind: "User",
        dimensions_mm: None,
    },
    StandardPageInfo {
        kind: "USLetter",
        dimensions_mm: Some([279.4, 215.9]),
    },
    StandardPageInfo {
        kind: "USLegal",
        dimensions_mm: Some([355.6, 215.9]),
    },
    StandardPageInfo {
        kind: "USLedger",
        dimensions_mm: Some([431.8, 279.4]),
    },
];
const SIM_LEGACY_ENABLE_FIELD: &str = "Spice_Netlist_Enabled";

#[derive(Clone, Copy)]
enum FieldParent<'a> {
    Symbol(&'a Symbol),
    Sheet(&'a Sheet),
    Label(&'a Label),
}

#[derive(Clone, Copy)]
enum SchTextTarget {
    Text,
    Label(LabelKind),
}

enum ParsedSchText {
    Text(Text),
    Label(Label),
}

enum ParsedTextBoxOwner<'a> {
    TextBox(&'a mut TextBox),
    TableCell(&'a mut TableCell),
}

impl<'a> ParsedTextBoxOwner<'a> {
    fn is_table_cell(&self) -> bool {
        matches!(self, Self::TableCell(_))
    }
}

struct ParsedEdaTextOwner<'a> {
    text: Option<&'a mut String>,
    visible: &'a mut bool,
    has_effects: Option<&'a mut bool>,
    effects: &'a mut Option<TextEffects>,
}

impl<'a> ParsedEdaTextOwner<'a> {
    fn text(text: &'a mut Text) -> Self {
        Self {
            text: Some(&mut text.text),
            visible: &mut text.visible,
            has_effects: Some(&mut text.has_effects),
            effects: &mut text.effects,
        }
    }

    fn label(label: &'a mut Label) -> Self {
        Self {
            text: Some(&mut label.text),
            visible: &mut label.visible,
            has_effects: Some(&mut label.has_effects),
            effects: &mut label.effects,
        }
    }

    fn property(property: &'a mut Property) -> Self {
        Self {
            text: Some(&mut property.value),
            visible: &mut property.visible,
            has_effects: Some(&mut property.has_effects),
            effects: &mut property.effects,
        }
    }

    fn sheet_pin(pin: &'a mut SheetPin) -> Self {
        Self {
            text: None,
            visible: &mut pin.visible,
            has_effects: Some(&mut pin.has_effects),
            effects: &mut pin.effects,
        }
    }

    fn lib_item_text(item: &'a mut LibDrawItem) -> Self {
        Self {
            text: item.text.as_mut(),
            visible: &mut item.visible,
            has_effects: None,
            effects: &mut item.effects,
        }
    }

    fn text_box(text_box: &'a mut TextBox) -> Self {
        Self {
            text: Some(&mut text_box.text),
            visible: &mut text_box.visible,
            has_effects: Some(&mut text_box.has_effects),
            effects: &mut text_box.effects,
        }
    }

    fn table_cell(cell: &'a mut TableCell) -> Self {
        Self {
            text: Some(&mut cell.text),
            visible: &mut cell.visible,
            has_effects: Some(&mut cell.has_effects),
            effects: &mut cell.effects,
        }
    }

    fn detached(
        text: Option<&'a mut String>,
        effects: &'a mut Option<TextEffects>,
        visible: &'a mut bool,
    ) -> Self {
        Self {
            text,
            visible,
            has_effects: None,
            effects,
        }
    }
}

pub fn parse_schematic_file(path: &Path) -> Result<Schematic, Error> {
    let raw = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let tokens = lex(&raw).map_err(|source| Error::SExpr {
        path: path.to_path_buf(),
        source,
    })?;
    KiCadSchematicParser::new(path.to_path_buf(), tokens).parse_schematic()
}

struct KiCadSchematicParser {
    path: PathBuf,
    tokens: Vec<Token>,
    idx: usize,
    version: Option<i32>,
    generator: Option<String>,
    generator_version: Option<String>,
    root_uuid: Option<String>,
    used_uuids: HashSet<String>,
    screen: Screen,
    pending_groups: Vec<PendingGroupInfo>,
}

#[derive(Debug, Clone)]
struct PendingGroupInfo {
    name: Option<String>,
    uuid: Option<String>,
    lib_id: Option<String>,
    member_uuids: Vec<String>,
}

impl KiCadSchematicParser {
    fn new(path: PathBuf, tokens: Vec<Token>) -> Self {
        let page_info = Self::find_standard_page_info("A4").expect("A4 page info must exist");
        let [width, height] = page_info.dimensions_mm.expect("A4 dimensions must exist");

        Self {
            path,
            tokens,
            idx: 0,
            version: None,
            generator: None,
            generator_version: None,
            root_uuid: None,
            used_uuids: HashSet::new(),
            screen: Screen {
                file_format_version_at_load: None,
                uuid: None,
                paper: Some(Paper {
                    kind: page_info.kind.to_string(),
                    width: Some(width),
                    height: Some(height),
                    portrait: false,
                }),
                page: None,
                root_sheet_page: None,
                content_modified: false,
                title_block: None,
                embedded_fonts: None,
                embedded_files: Vec::new(),
                parse_warnings: Vec::new(),
                bus_aliases: Vec::new(),
                lib_symbols: Vec::new(),
                items: Vec::new(),
                sheet_instances: Vec::new(),
                symbol_instances: Vec::new(),
            },
            pending_groups: Vec::new(),
        }
    }

    fn parse_schematic(mut self) -> Result<Schematic, Error> {
        self.need_left()?;
        if self.need_unquoted_symbol_atom("kicad_sch")? != "kicad_sch" {
            return Err(self.expecting("kicad_sch"));
        }

        if matches!(self.current().kind, TokKind::Left)
            && matches!(
                self.tokens.get(self.idx + 1).map(|token| &token.kind),
                Some(TokKind::Atom(value)) if value == "version"
            )
        {
            self.need_left()?;
            if self.need_unquoted_symbol_atom("version")? != "version" {
                return Err(self.expecting("version"));
            }
            if self.version.is_some() {
                return Err(self.error_here("duplicate version section"));
            }
            self.version = Some(self.parse_i32_atom("version")?);
            self.need_right()?;
        } else {
            self.version = Some(SEXPR_SCHEMATIC_FILE_VERSION);
        }

        if self.version.unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION) < 20210406
            && self.root_uuid.is_none()
        {
            let generated = Uuid::new_v4().to_string();
            self.screen.uuid = Some(generated.clone());
            self.root_uuid = Some(generated);
        }

        self.screen.file_format_version_at_load = self.version;

        let check_future_version = |version: i32| {
            if version > SEXPR_SCHEMATIC_FILE_VERSION {
                Err(self.validation(
                    Some(self.current_span()),
                    format!(
                        "future schematic version `{version}` is newer than supported `{SEXPR_SCHEMATIC_FILE_VERSION}`"
                    ),
                ))
            } else {
                Ok(())
            }
        };

        if self.version.unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION) < VERSION_GENERATOR_VERSION {
            check_future_version(self.version.unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION))?;
        }

        self.parse_schematic_body()?;
        let version = self
            .version
            .ok_or_else(|| self.error_here("missing version"))?;
        self.fixup_legacy_lib_symbol_alternate_body_styles();
        self.update_local_lib_symbol_links();
        self.fixup_embedded_data();

        self.resolve_groups();
        self.need_right()?;
        if !matches!(self.current().kind, TokKind::Eof) {
            return Err(self.expecting("end of file"));
        }
        if version > SEXPR_SCHEMATIC_FILE_VERSION {
            return Err(self.validation(
                Some(self.current_span()),
                format!(
                    "future schematic version `{version}` is newer than supported `{SEXPR_SCHEMATIC_FILE_VERSION}`"
                ),
            ));
        }

        // Upstream: if file has no uuid, auto-generate one regardless of version.
        // (The C++ code at the end of ParseSchematic always fills in root UUID from
        // the screen's auto-generated UUID when fileHasUuid is false.)
        if self.root_uuid.is_none() {
            let generated = Uuid::new_v4().to_string();
            self.screen.uuid = Some(generated.clone());
            self.root_uuid = Some(generated);
        }

        Ok(Schematic {
            path: self.path,
            version,
            generator: self.generator.unwrap_or_default(),
            generator_version: self.generator_version,
            root_sheet: RootSheet {
                uuid: self.root_uuid.clone(),
            },
            screen: self.screen,
        })
    }

    fn parse_schematic_body(&mut self) -> Result<(), Error> {
        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value) if matches!(self.current().atom_class, Some(AtomClass::Symbol)) => {
                    value.clone()
                }
                _ => {
                    return Err(self.expecting(
                        "bitmap, bus, bus_alias, bus_entry, class_label, embedded_files, global_label, hierarchical_label, junction, label, line, no_connect, page, paper, rule_area, sheet, symbol, symbol_instances, text, title_block",
                    ))
                }
            };
            let mut effective_head = head.as_str();

            if effective_head == "page"
                && self
                    .require_known_version()
                    .unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION)
                    <= VERSION_PAGE_RENAMED_TO_PAPER
            {
                effective_head = "paper";
            }
            let mut section_consumed_right = false;

            match effective_head {
                "generator" => {
                    let _ = self.need_unquoted_symbol_atom("generator")?;
                    self.generator = Some(self.need_symbol_atom("generator")?)
                }
                "host" => {
                    let _ = self.need_unquoted_symbol_atom("host")?;
                    self.generator = Some(self.need_symbol_atom("host")?);
                    if self.require_known_version()? < 20200827 {
                        let _ = self.need_symbol_atom("host version")?;
                    }
                }
                "generator_version" => {
                    let _ = self.need_unquoted_symbol_atom("generator_version")?;
                    let version = self.require_known_version()?;
                    if version < VERSION_GENERATOR_VERSION {
                        return Err(self.error_here(format!(
                            "generator_version requires schematic version {VERSION_GENERATOR_VERSION} or newer"
                        )));
                    }
                    self.generator_version = Some(match &self.current().kind {
                        TokKind::Atom(value) => {
                            let out = value.clone();
                            self.idx += 1;
                            out
                        }
                        _ => return Err(self.error_here("missing generator_version")),
                    });
                    if self.version.unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION)
                        > SEXPR_SCHEMATIC_FILE_VERSION
                    {
                        return Err(self.validation(
                            Some(self.current_span()),
                            format!(
                                "future schematic version `{}` is newer than supported `{SEXPR_SCHEMATIC_FILE_VERSION}`",
                                self.version.unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION)
                            ),
                        ));
                    }
                }
                "uuid" => {
                    let _ = self.need_unquoted_symbol_atom("uuid")?;
                    let uuid = self.parse_kiid()?;
                    self.screen.uuid = Some(uuid.clone());
                    self.root_uuid = Some(uuid);
                }
                "paper" => {
                    let _ = self.need_unquoted_symbol_atom("paper")?;
                    self.screen.paper = Some(self.parse_page_info()?);
                    section_consumed_right = true;
                }
                "page" => {
                    let _ = self.need_unquoted_symbol_atom("page")?;
                    let page = self
                        .need_symbol_or_number_atom("page number")
                        .map_err(|_| self.error_here("missing page number"))?;
                    let sheet = self
                        .need_symbol_or_number_atom("page sheet")
                        .map_err(|_| self.error_here("missing page sheet"))?;
                    self.screen.page = Some(Page { page, sheet });
                    self.need_right()?;
                    section_consumed_right = true;
                }
                "title_block" => {
                    self.parse_title_block()?;
                    section_consumed_right = true;
                }
                "embedded_fonts" => {
                    let _ = self.need_unquoted_symbol_atom("embedded_fonts")?;
                    self.screen.embedded_fonts = Some(self.parse_bool_atom("embedded_fonts")?);
                }
                "embedded_files" => {
                    let version = self.require_known_version()?;
                    if version < VERSION_EMBEDDED_FILES {
                        return Err(self.error_here(format!(
                            "embedded_files requires schematic version {VERSION_EMBEDDED_FILES} or newer"
                        )));
                    }
                    let _ = self.need_unquoted_symbol_atom("embedded_files")?;
                    let block_depth = self.current_nesting_depth();
                    match self.parse_embedded_files() {
                        Ok(files) => {
                            self.screen.embedded_files.extend(files);
                            self.need_right()?;
                        }
                        Err(err) => {
                            self.screen.parse_warnings.push(err.to_string());
                            self.skip_to_block_right(block_depth);
                            self.need_right()?;
                        }
                    }
                    section_consumed_right = true;
                }
                "lib_symbols" => {
                    self.parse_sch_lib_symbols()?;
                    section_consumed_right = true;
                }
                "bus_alias" => {
                    self.parse_bus_alias()?;
                    section_consumed_right = true;
                }
                "symbol" => {
                    let symbol = self.parse_schematic_symbol()?;
                    self.screen.items.push(SchItem::Symbol(symbol));
                    section_consumed_right = true;
                }
                "sheet" => {
                    let sheet = self.parse_sch_sheet()?;
                    self.screen.items.push(SchItem::Sheet(sheet));
                    section_consumed_right = true;
                }
                "junction" => {
                    let junction = self.parse_junction()?;
                    self.screen.items.push(SchItem::Junction(junction));
                    section_consumed_right = true;
                }
                "no_connect" => {
                    let no_connect = self.parse_no_connect()?;
                    self.screen.items.push(SchItem::NoConnect(no_connect));
                    section_consumed_right = true;
                }
                "bus_entry" => {
                    let bus_entry = self.parse_bus_entry()?;
                    self.screen.items.push(SchItem::BusEntry(bus_entry));
                    section_consumed_right = true;
                }
                "wire" => {
                    let wire = self.parse_sch_line()?;
                    self.screen.items.push(SchItem::Wire(wire));
                    section_consumed_right = true;
                }
                "bus" => {
                    let bus = self.parse_sch_line()?;
                    self.screen.items.push(SchItem::Bus(bus));
                    section_consumed_right = true;
                }
                "polyline" => {
                    let shape = self.parse_sch_polyline()?;
                    if shape.points.len() < 2 {
                        return Err(self.error_here("Schematic polyline has too few points"));
                    }
                    if shape.points.len() == 2 {
                        let mut line = Line::new(LineKind::Polyline);
                        line.points = shape.points;
                        line.has_stroke = shape.has_stroke;
                        line.stroke = shape.stroke;
                        line.uuid = shape.uuid;
                        self.screen.items.push(SchItem::Polyline(line));
                    } else {
                        self.screen.items.push(SchItem::Shape(shape));
                    }
                    section_consumed_right = true;
                }
                "label" | "global_label" | "hierarchical_label" | "directive_label"
                | "class_label" | "netclass_flag" | "text" => {
                    let item = self.parse_sch_text()?;
                    self.screen.items.push(item);
                    section_consumed_right = true;
                }
                "text_box" => {
                    let text_box = self.parse_sch_text_box()?;
                    self.screen.items.push(SchItem::TextBox(text_box));
                    section_consumed_right = true;
                }
                "table" => {
                    let table = self.parse_sch_table()?;
                    self.screen.items.push(SchItem::Table(table));
                    section_consumed_right = true;
                }
                "image" => {
                    let image = self.parse_sch_image()?;
                    self.screen.items.push(SchItem::Image(image));
                    section_consumed_right = true;
                }
                "arc" => {
                    let shape = self.parse_sch_arc()?;
                    self.screen.items.push(SchItem::Shape(shape));
                    section_consumed_right = true;
                }
                "circle" => {
                    let shape = self.parse_sch_circle()?;
                    self.screen.items.push(SchItem::Shape(shape));
                    section_consumed_right = true;
                }
                "rectangle" => {
                    let shape = self.parse_sch_rectangle()?;
                    self.screen.items.push(SchItem::Shape(shape));
                    section_consumed_right = true;
                }
                "bezier" => {
                    let shape = self.parse_sch_bezier()?;
                    self.screen.items.push(SchItem::Shape(shape));
                    section_consumed_right = true;
                }
                "rule_area" => {
                    let shape = self.parse_sch_rule_area()?;
                    self.screen.items.push(SchItem::Shape(shape));
                    section_consumed_right = true;
                }
                "sheet_instances" => {
                    self.parse_sch_sheet_instances()?;
                    section_consumed_right = true;
                }
                "symbol_instances" => {
                    self.parse_sch_symbol_instances()?;
                    section_consumed_right = true;
                }
                "group" => {
                    self.parse_group()?;
                    section_consumed_right = true;
                }
                _ => {
                    return Err(self.expecting(
                        "bitmap, bus, bus_alias, bus_entry, class_label, embedded_files, global_label, hierarchical_label, junction, label, line, no_connect, page, paper, rule_area, sheet, symbol, symbol_instances, text, title_block",
                    ));
                }
            }
            if !section_consumed_right {
                self.need_right()?;
            }
        }

        Ok(())
    }

    fn parse_title_block(&mut self) -> Result<(), Error> {
        let _ = self.need_unquoted_symbol_atom("title_block")?;
        let mut title_block = TitleBlock::default();
        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("title, date, rev, company, or comment")),
            };
            match head.as_str() {
                "title" => {
                    let _ = self.need_unquoted_symbol_atom("title")?;
                    title_block.title = Some(match &self.current().kind {
                        TokKind::Atom(value) => {
                            let out = value.clone();
                            self.idx += 1;
                            out
                        }
                        _ => return Err(self.error_here("missing title")),
                    })
                }
                "date" => {
                    let _ = self.need_unquoted_symbol_atom("date")?;
                    title_block.date = Some(match &self.current().kind {
                        TokKind::Atom(value) => {
                            let out = value.clone();
                            self.idx += 1;
                            out
                        }
                        _ => return Err(self.error_here("missing date")),
                    })
                }
                "rev" => {
                    let _ = self.need_unquoted_symbol_atom("rev")?;
                    title_block.revision = Some(match &self.current().kind {
                        TokKind::Atom(value) => {
                            let out = value.clone();
                            self.idx += 1;
                            out
                        }
                        _ => return Err(self.error_here("missing rev")),
                    })
                }
                "company" => {
                    let _ = self.need_unquoted_symbol_atom("company")?;
                    title_block.company = Some(match &self.current().kind {
                        TokKind::Atom(value) => {
                            let out = value.clone();
                            self.idx += 1;
                            out
                        }
                        _ => return Err(self.error_here("missing company")),
                    })
                }
                "comment" => {
                    let _ = self.need_unquoted_symbol_atom("comment")?;
                    let index_span = self.current_span();
                    let idx = self.parse_i32_atom("comment index")?;
                    let value = match &self.current().kind {
                        TokKind::Atom(value) => {
                            let out = value.clone();
                            self.idx += 1;
                            out
                        }
                        _ => return Err(self.error_here("missing comment value")),
                    };

                    let comment_number = match idx {
                        1 => 1,
                        2 => 2,
                        3 => 3,
                        4 => 4,
                        5 => 5,
                        6 => 6,
                        7 => 7,
                        8 => 8,
                        9 => 9,
                        _ => {
                            return Err(self.validation(
                                Some(index_span),
                                "Invalid title block comment number",
                            ));
                        }
                    };

                    title_block.set_comment(comment_number as usize, value);
                }
                _ => return Err(self.expecting("title, date, rev, company, or comment")),
            }
            self.need_right()?;
        }
        self.screen.title_block = Some(title_block);
        self.need_right()?;
        Ok(())
    }

    fn parse_sch_lib_symbols(&mut self) -> Result<(), Error> {
        let _ = self.need_unquoted_symbol_atom("lib_symbols")?;
        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("symbol")),
            };
            if head != "symbol" {
                return Err(self.expecting("symbol"));
            }
            let block_depth = self.current_nesting_depth();
            match self.parse_lib_symbol() {
                Ok(symbol) => {
                    self.screen.lib_symbols.push(symbol);
                }
                Err(err) => {
                    self.screen.parse_warnings.push(format!(
                        "Error parsing symbol: {}\nSkipping symbol and continuing.",
                        err
                    ));
                    self.skip_to_block_right(block_depth);
                    self.need_right()?;
                }
            }
        }
        self.need_right()?;
        Ok(())
    }

    fn parse_body_styles(&mut self, symbol: &mut LibSymbol) -> Result<(), Error> {
        symbol.body_styles_specified = true;
        while !self.at_right() {
            if self.at_unquoted_symbol_with("demorgan") {
                let _ = self.need_unquoted_symbol_atom("demorgan")?;
                symbol.has_demorgan = true;
            } else {
                symbol.body_style_names.push(
                    self.need_symbol_atom("property value")
                        .map_err(|_| self.error_here("Invalid property value"))?,
                );
            }
        }

        self.need_right()?;
        Ok(())
    }

    fn parse_pin_names(&mut self, symbol: &mut LibSymbol) -> Result<(), Error> {
        while !self.at_right() {
            if self.at_unquoted_symbol_with("hide") {
                let _ = self.need_unquoted_symbol_atom("hide")?;
                symbol.show_pin_names = false;
                continue;
            }

            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("offset or hide")),
            };
            match head.as_str() {
                "offset" => {
                    let _ = self.need_unquoted_symbol_atom("offset")?;
                    symbol.pin_name_offset = Some(self.parse_f64_atom("pin name offset")?);
                    self.need_right()?;
                }
                "hide" => {
                    let _ = self.need_unquoted_symbol_atom("hide")?;
                    symbol.show_pin_names = !self.parse_bool_atom("hide")?;
                    self.need_right()?;
                }
                _ => return Err(self.expecting("offset or hide")),
            }
        }

        self.need_right()?;
        Ok(())
    }

    fn parse_pin_numbers(&mut self, symbol: &mut LibSymbol) -> Result<(), Error> {
        while !self.at_right() {
            if self.at_unquoted_symbol_with("hide") {
                let _ = self.need_unquoted_symbol_atom("hide")?;
                symbol.show_pin_numbers = false;
                continue;
            }

            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("hide")),
            };
            match head.as_str() {
                "hide" => {
                    let _ = self.need_unquoted_symbol_atom("hide")?;
                    symbol.show_pin_numbers = !self.parse_bool_atom("hide")?;
                    self.need_right()?;
                }
                _ => return Err(self.expecting("hide")),
            }
        }

        self.need_right()?;
        Ok(())
    }

    fn parse_symbol_draw_item(
        &mut self,
        symbol: &LibSymbol,
        unit_number: i32,
        body_style: i32,
    ) -> Result<LibDrawItem, Error> {
        let head = match &self.current().kind {
            TokKind::Atom(value)
                if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
            {
                value.clone()
            }
            _ => {
                return Err(
                    self.expecting("arc, bezier, circle, pin, polyline, rectangle, or text")
                );
            }
        };
        match head.as_str() {
            "arc" => self.parse_symbol_arc(unit_number, body_style),
            "bezier" => self.parse_symbol_bezier(unit_number, body_style),
            "circle" => self.parse_symbol_circle(unit_number, body_style),
            "pin" => self.parse_symbol_pin(unit_number, body_style),
            "polyline" => self.parse_symbol_polyline(unit_number, body_style),
            "rectangle" => self.parse_symbol_rectangle(unit_number, body_style),
            "text" => self.parse_symbol_text(symbol, unit_number, body_style),
            "text_box" => self.parse_symbol_text_box(unit_number, body_style),
            _ => Err(self.expecting("arc, bezier, circle, pin, polyline, rectangle, or text")),
        }
    }

    fn parse_lib_symbol(&mut self) -> Result<LibSymbol, Error> {
        let _ = self.need_unquoted_symbol_atom("symbol")?;
        let raw_name = self
            .need_symbol_atom("lib symbol name")
            .map_err(|_| self.error_here("Invalid symbol name"))?;
        let lib_id = raw_name.replace("{slash}", "/");

        if let Some(ch) = Self::find_invalid_lib_id_char(&lib_id) {
            return Err(
                self.error_here(format!("Symbol {lib_id} contains invalid character '{ch}'"))
            );
        }

        if lib_id.is_empty() {
            return Err(self.error_here("Invalid library identifier"));
        }

        let mut symbol = LibSymbol::new(lib_id.clone());

        while !self.at_right() {
            self.need_left()?;
            let branch = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => {
                    return Err(self.expecting(
                        "pin_names, pin_numbers, arc, bezier, circle, pin, polyline, rectangle, or text",
                    ))
                }
            };
            match branch.as_str() {
                "power" => {
                    let _ = self.need_unquoted_symbol_atom("power")?;
                    symbol.power = true;
                    if !self.at_right() {
                        match self.need_unquoted_symbol_atom("global or local")?.as_str() {
                            "local" => symbol.local_power = true,
                            "global" => symbol.local_power = false,
                            _ => return Err(self.expecting("global or local")),
                        }
                    }
                    self.need_right()?;
                }
                "body_styles" => {
                    let _ = self.need_unquoted_symbol_atom("body_styles")?;
                    self.parse_body_styles(&mut symbol)?;
                }
                "pin_names" => {
                    let _ = self.need_unquoted_symbol_atom("pin_names")?;
                    self.parse_pin_names(&mut symbol)?;
                }
                "pin_numbers" => {
                    let _ = self.need_unquoted_symbol_atom("pin_numbers")?;
                    self.parse_pin_numbers(&mut symbol)?;
                }
                "exclude_from_sim" => {
                    let _ = self.need_unquoted_symbol_atom("exclude_from_sim")?;
                    symbol.excluded_from_sim = self.parse_bool_atom("exclude_from_sim")?;
                    self.need_right()?;
                }
                "in_bom" => {
                    let _ = self.need_unquoted_symbol_atom("in_bom")?;
                    symbol.in_bom = self.parse_bool_atom("in_bom")?;
                    self.need_right()?;
                }
                "on_board" => {
                    let _ = self.need_unquoted_symbol_atom("on_board")?;
                    symbol.on_board = self.parse_bool_atom("on_board")?;
                    self.need_right()?;
                }
                "in_pos_files" => {
                    let _ = self.need_unquoted_symbol_atom("in_pos_files")?;
                    symbol.in_pos_files = self.parse_bool_atom("in_pos_files")?;
                    self.need_right()?;
                }
                "duplicate_pin_numbers_are_jumpers" => {
                    let _ = self.need_unquoted_symbol_atom("duplicate_pin_numbers_are_jumpers")?;
                    symbol.duplicate_pin_numbers_are_jumpers =
                        self.parse_bool_atom("duplicate_pin_numbers_are_jumpers")?;
                    self.need_right()?;
                }
                "jumper_pin_groups" => {
                    let _ = self.need_unquoted_symbol_atom("jumper_pin_groups")?;
                    let mut current_group: Option<BTreeSet<String>> = None;

                    while current_group.is_some() || !self.at_right() {
                        match &self.current().kind {
                            TokKind::Left => {
                                self.need_left()?;
                                current_group = Some(BTreeSet::new());
                            }
                            TokKind::Atom(_)
                                if matches!(self.current().atom_class, Some(AtomClass::Quoted)) =>
                            {
                                let group = current_group
                                    .as_mut()
                                    .ok_or_else(|| self.expecting("list of pin names"))?;
                                group.insert(self.need_quoted_atom("list of pin names")?);
                            }
                            TokKind::Right => {
                                self.need_right()?;
                                if let Some(group) = current_group.take() {
                                    symbol.jumper_pin_groups.push(group);
                                }
                            }
                            _ => return Err(self.expecting("list of pin names")),
                        }
                    }
                    self.need_right()?;
                }
                "property" => {
                    self.parse_lib_property(&mut symbol)?;
                }
                "extends" => {
                    let _ = self.need_unquoted_symbol_atom("extends")?;
                    symbol.extends = Some(
                        self.need_symbol_atom("parent symbol name")
                            .map_err(|_| self.error_here("Invalid parent symbol name"))?
                            .replace("{slash}", "/"),
                    );
                    self.need_right()?;
                }
                "symbol" => {
                    let _ = self.need_unquoted_symbol_atom("symbol")?;
                    let unit_name_raw = self
                        .need_symbol_atom("symbol unit name")
                        .map_err(|_| self.error_here("Invalid symbol unit name"))?;
                    let unit_full_name = unit_name_raw.replace("{slash}", "/");

                    if !unit_full_name.starts_with(&symbol.name) {
                        return Err(self.error_here(format!(
                            "invalid symbol unit name prefix {unit_full_name}"
                        )));
                    }

                    let suffix = unit_full_name
                        .strip_prefix(&symbol.name)
                        .and_then(|rest| rest.strip_prefix('_'))
                        .ok_or_else(|| {
                            self.error_here(format!(
                                "invalid symbol unit name prefix {unit_full_name}"
                            ))
                        })?;
                    let mut parts = suffix.split('_');
                    let unit_number = parts
                        .next()
                        .ok_or_else(|| {
                            self.error_here(format!("invalid symbol unit name suffix {suffix}"))
                        })?
                        .parse::<i32>()
                        .map_err(|_| self.error_here(format!("invalid symbol unit number {suffix}")))?;
                    let body_style = parts
                        .next()
                        .ok_or_else(|| {
                            self.error_here(format!("invalid symbol unit name suffix {suffix}"))
                        })?
                        .parse::<i32>()
                        .map_err(|_| {
                            self.error_here(format!("invalid symbol body style number {suffix}"))
                        })?;

                    if parts.next().is_some() {
                        return Err(
                            self.error_here(format!("invalid symbol unit name suffix {suffix}"))
                        );
                    }

                    let unit_name = unit_full_name;
                    symbol.ensure_unit_index(unit_name.clone(), unit_number, body_style);

                    while !self.at_right() {
                        self.need_left()?;
                        if self.at_unquoted_symbol_with("unit_name") {
                            let _ = self.need_unquoted_symbol_atom("unit_name")?;
                            let token = self.current().clone();
                            self.idx += 1;

                            if let TokKind::Atom(value) = token.kind
                                && matches!(
                                    token.atom_class,
                                    Some(AtomClass::Symbol | AtomClass::Quoted)
                                )
                            {
                                symbol.set_unit_display_name(unit_number, value);
                            }
                            self.need_right()?;
                        } else {
                            let item =
                                self.parse_symbol_draw_item(&symbol, unit_number, body_style)?;
                            symbol.add_draw_item(item);
                        }
                    }
                    self.need_right()?;
                }
                "arc" | "bezier" | "circle" | "pin" | "polyline" | "rectangle" | "text"
                | "text_box" => {
                    let item = self.parse_symbol_draw_item(&symbol, 1, 1)?;
                    symbol.add_draw_item(item);
                }
                "embedded_fonts" => {
                    let _ = self.need_unquoted_symbol_atom("embedded_fonts")?;
                    symbol.embedded_fonts = Some(self.parse_bool_atom("embedded_fonts")?);
                    self.need_right()?;
                }
                "embedded_files" => {
                    let _ = self.need_unquoted_symbol_atom("embedded_files")?;
                    let block_depth = self.current_nesting_depth();
                    match self.parse_embedded_files() {
                        Ok(files) => {
                            symbol.embedded_files = files;
                            self.need_right()?;
                        }
                        Err(err) => {
                            self.screen.parse_warnings.push(err.to_string());
                            self.skip_to_block_right(block_depth);
                            self.need_right()?;
                        }
                    }
                }
                _ => {
                    return Err(self.expecting(
                        "pin_names, pin_numbers, arc, bezier, circle, pin, polyline, rectangle, or text",
                    ))
                }
            }
        }

        if self.version.unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION) < VERSION_CUSTOM_BODY_STYLES {
            symbol.has_demorgan = symbol.has_legacy_alternate_body_style();
        }

        symbol.refresh_library_tree_caches();
        self.need_right()?;
        Ok(symbol)
    }

    fn parse_embedded_files(&mut self) -> Result<Vec<EmbeddedFile>, Error> {
        let mut files = Vec::new();

        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("file")),
            };
            if head != "file" {
                return Err(self.expecting("file"));
            }
            let _ = self.need_unquoted_symbol_atom("file")?;
            let mut file = EmbeddedFile::new();

            while !self.at_right() {
                self.need_left()?;
                let head = match &self.current().kind {
                    TokKind::Atom(value)
                        if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                    {
                        value.clone()
                    }
                    _ => return Err(self.expecting("checksum, data or name")),
                };
                match head.as_str() {
                    "name" => {
                        let _ = self.need_unquoted_symbol_atom("name")?;
                        file.name = Some(self.need_symbol_or_number_atom("name")?);
                    }
                    "checksum" => {
                        let _ = self.need_unquoted_symbol_atom("checksum")?;
                        if file.name.is_none() {
                            return Err(self.expecting("name"));
                        }
                        file.checksum = Some(self.need_symbol_or_number_atom("checksum data")?);
                    }
                    "type" => {
                        let _ = self.need_unquoted_symbol_atom("type")?;
                        if file.name.is_none() {
                            return Err(self.expecting("name"));
                        }
                        file.file_type = Some(
                            match self
                                .need_unquoted_symbol_atom(
                                    "datasheet, font, model, worksheet or other",
                                )?
                                .as_str()
                            {
                                "datasheet" => EmbeddedFileType::Datasheet,
                                "font" => EmbeddedFileType::Font,
                                "model" => EmbeddedFileType::Model,
                                "worksheet" => EmbeddedFileType::Worksheet,
                                "other" => EmbeddedFileType::Other,
                                _ => {
                                    return Err(self
                                        .expecting("datasheet, font, model, worksheet or other"));
                                }
                            },
                        );
                    }
                    "data" => {
                        let _ = self.need_unquoted_symbol_atom("data")?;
                        if file.name.is_none() {
                            return Err(self.expecting("name"));
                        }
                        if self.at_right() {
                            self.need_right()?;
                            continue;
                        }
                        let bar = self.need_unquoted_symbol_atom("|")?;
                        if bar != "|" {
                            return Err(self.expecting("|"));
                        }
                        let mut encoded = String::new();
                        while !self.at_unquoted_symbol_with("|") {
                            encoded.push_str(&self.need_symbol_atom("base64 file data")?);
                        }
                        let bar = self.need_unquoted_symbol_atom("|")?;
                        if bar != "|" {
                            return Err(self.expecting("|"));
                        }
                        file.data = Some(encoded);
                    }
                    _ => return Err(self.expecting("checksum, data or name")),
                }
                self.need_right()?;
            }

            self.need_right()?;
            files.push(file);
        }

        Ok(files)
    }

    fn parse_symbol_arc(
        &mut self,
        unit_number: i32,
        body_style: i32,
    ) -> Result<LibDrawItem, Error> {
        let _ = self.need_unquoted_symbol_atom("arc")?;
        let mut is_private = false;
        if self.at_unquoted_symbol_with("private") {
            let _ = self.need_unquoted_symbol_atom("private")?;
            is_private = true;
        }
        let mut item = LibDrawItem::new("arc", unit_number, body_style);
        item.is_private = is_private;
        item.points = vec![[1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        item.arc_center = Some([0.0, 0.0]);
        item.arc_start_angle = Some(0.0);
        item.arc_end_angle = Some(90.0);
        let mut saw_start = false;
        let mut saw_mid = false;
        let mut saw_end = false;

        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("start, mid, end, radius, stroke, or fill")),
            };
            match head.as_str() {
                "start" => {
                    let _ = self.need_unquoted_symbol_atom("start")?;
                    item.points[0] = self.parse_xy2("arc start")?;
                    saw_start = true;
                    self.need_right()?;
                }
                "mid" => {
                    let _ = self.need_unquoted_symbol_atom("mid")?;
                    item.points[1] = self.parse_xy2("arc mid")?;
                    saw_mid = true;
                    self.need_right()?;
                }
                "end" => {
                    let _ = self.need_unquoted_symbol_atom("end")?;
                    item.points[2] = self.parse_xy2("arc end")?;
                    saw_end = true;
                    self.need_right()?;
                }
                "radius" => {
                    let _ = self.need_unquoted_symbol_atom("radius")?;
                    while !self.at_right() {
                        self.need_left()?;
                        match self
                            .need_unquoted_symbol_atom("at, length, or angles")?
                            .as_str()
                        {
                            "at" => {
                                item.arc_center = Some(self.parse_xy2("arc center")?);
                                self.need_right()?;
                            }
                            "length" => {
                                item.radius = Some(self.parse_f64_atom("radius length")?);
                                self.need_right()?;
                            }
                            "angles" => {
                                item.arc_start_angle =
                                    Some(self.parse_f64_atom("start radius angle")?);
                                item.arc_end_angle = Some(self.parse_f64_atom("end radius angle")?);
                                self.need_right()?;
                            }
                            _ => return Err(self.expecting("at, length, or angles")),
                        }
                    }
                    self.need_right()?;
                }
                "stroke" => {
                    let _ = self.need_unquoted_symbol_atom("stroke")?;
                    item.stroke = Some(self.parse_stroke()?);
                }
                "fill" => {
                    let _ = self.need_unquoted_symbol_atom("fill")?;
                    item.fill = Some(self.parse_fill()?);
                }
                _ => return Err(self.expecting("start, mid, end, radius, stroke, or fill")),
            }
        }

        if !saw_mid {
            item.points.remove(1);
        } else if !saw_start || !saw_end {
            // keep defaults when an explicit midpoint path only partially specifies endpoints
        }

        self.need_right()?;
        Ok(item)
    }

    fn parse_symbol_bezier(
        &mut self,
        unit_number: i32,
        body_style: i32,
    ) -> Result<LibDrawItem, Error> {
        let _ = self.need_unquoted_symbol_atom("bezier")?;
        let mut is_private = false;
        if self.at_unquoted_symbol_with("private") {
            let _ = self.need_unquoted_symbol_atom("private")?;
            is_private = true;
        }
        let mut item = LibDrawItem::new("bezier", unit_number, body_style);
        item.is_private = is_private;

        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("pts, stroke, or fill")),
            };
            match head.as_str() {
                "pts" => {
                    let _ = self.need_unquoted_symbol_atom("pts")?;
                    let mut points = Vec::new();
                    while !self.at_right() {
                        self.need_left()?;
                        let head = match &self.current().kind {
                            TokKind::Atom(value)
                                if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                            {
                                value.clone()
                            }
                            _ => return Err(self.expecting("xy")),
                        };
                        if head != "xy" {
                            return Err(self.expecting("xy"));
                        }
                        let _ = self.need_unquoted_symbol_atom("xy")?;
                        if points.len() >= 4 {
                            return Err(self.error_here("unexpected control point"));
                        }
                        points.push(self.parse_xy2("bezier point")?);
                        self.need_right()?;
                    }
                    item.points = points;
                    self.need_right()?;
                }
                "stroke" => {
                    let _ = self.need_unquoted_symbol_atom("stroke")?;
                    item.stroke = Some(self.parse_stroke()?);
                }
                "fill" => {
                    let _ = self.need_unquoted_symbol_atom("fill")?;
                    item.fill = Some(self.parse_fill()?);
                }
                _ => return Err(self.expecting("pts, stroke, or fill")),
            }
        }

        self.need_right()?;
        Ok(item)
    }

    fn parse_symbol_circle(
        &mut self,
        unit_number: i32,
        body_style: i32,
    ) -> Result<LibDrawItem, Error> {
        let _ = self.need_unquoted_symbol_atom("circle")?;
        let mut is_private = false;
        if self.at_unquoted_symbol_with("private") {
            let _ = self.need_unquoted_symbol_atom("private")?;
            is_private = true;
        }
        let mut item = LibDrawItem::new("circle", unit_number, body_style);
        item.is_private = is_private;
        item.points = vec![[0.0, 0.0]];
        item.radius = Some(1.0);

        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("center, radius, stroke, or fill")),
            };
            match head.as_str() {
                "center" => {
                    let _ = self.need_unquoted_symbol_atom("center")?;
                    item.points[0] = self.parse_xy2("circle center")?;
                    self.need_right()?;
                }
                "radius" => {
                    let _ = self.need_unquoted_symbol_atom("radius")?;
                    item.radius = Some(self.parse_f64_atom("radius length")?);
                    self.need_right()?;
                }
                "stroke" => {
                    let _ = self.need_unquoted_symbol_atom("stroke")?;
                    item.stroke = Some(self.parse_stroke()?);
                }
                "fill" => {
                    let _ = self.need_unquoted_symbol_atom("fill")?;
                    item.fill = Some(self.parse_fill()?);
                }
                _ => return Err(self.expecting("center, radius, stroke, or fill")),
            }
        }

        self.need_right()?;
        Ok(item)
    }

    fn parse_symbol_polyline(
        &mut self,
        unit_number: i32,
        body_style: i32,
    ) -> Result<LibDrawItem, Error> {
        let _ = self.need_unquoted_symbol_atom("polyline")?;
        let mut is_private = false;
        if self.at_unquoted_symbol_with("private") {
            let _ = self.need_unquoted_symbol_atom("private")?;
            is_private = true;
        }
        let mut item = LibDrawItem::new("polyline", unit_number, body_style);
        item.is_private = is_private;

        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("pts, stroke, or fill")),
            };
            match head.as_str() {
                "pts" => {
                    let _ = self.need_unquoted_symbol_atom("pts")?;
                    let mut points = Vec::new();
                    while !self.at_right() {
                        self.need_left()?;
                        let head = match &self.current().kind {
                            TokKind::Atom(value)
                                if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                            {
                                value.clone()
                            }
                            _ => return Err(self.expecting("xy")),
                        };
                        if head != "xy" {
                            return Err(self.expecting("xy"));
                        }
                        let _ = self.need_unquoted_symbol_atom("xy")?;
                        points.push(self.parse_xy2("xy")?);
                        self.need_right()?;
                    }
                    item.points = points;
                    self.need_right()?;
                }
                "stroke" => {
                    let _ = self.need_unquoted_symbol_atom("stroke")?;
                    item.stroke = Some(self.parse_stroke()?);
                }
                "fill" => {
                    let _ = self.need_unquoted_symbol_atom("fill")?;
                    item.fill = Some(self.parse_fill()?);
                }
                _ => return Err(self.expecting("pts, stroke, or fill")),
            }
        }

        self.need_right()?;
        Ok(item)
    }

    fn parse_symbol_rectangle(
        &mut self,
        unit_number: i32,
        body_style: i32,
    ) -> Result<LibDrawItem, Error> {
        let _ = self.need_unquoted_symbol_atom("rectangle")?;
        let mut is_private = false;
        if self.at_unquoted_symbol_with("private") {
            let _ = self.need_unquoted_symbol_atom("private")?;
            is_private = true;
        }
        let mut item = LibDrawItem::new("rectangle", unit_number, body_style);
        item.is_private = is_private;

        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("start, end, stroke, or fill")),
            };
            match head.as_str() {
                "start" => {
                    let _ = self.need_unquoted_symbol_atom("start")?;
                    item.points.push(self.parse_xy2("rectangle start")?);
                    self.need_right()?;
                }
                "end" => {
                    let _ = self.need_unquoted_symbol_atom("end")?;
                    item.end = Some(self.parse_xy2("rectangle end")?);
                    self.need_right()?;
                }
                "radius" => {
                    let _ = self.need_unquoted_symbol_atom("radius")?;
                    item.radius = Some(self.parse_f64_atom("corner radius")?);
                    self.need_right()?;
                }
                "stroke" => {
                    let _ = self.need_unquoted_symbol_atom("stroke")?;
                    item.stroke = Some(self.parse_stroke()?);
                }
                "fill" => {
                    let _ = self.need_unquoted_symbol_atom("fill")?;
                    item.fill = Some(self.parse_fill()?);
                }
                _ => return Err(self.expecting("start, end, stroke, or fill")),
            }
        }

        self.need_right()?;
        Ok(item)
    }

    fn parse_symbol_text(
        &mut self,
        symbol: &LibSymbol,
        unit_number: i32,
        body_style: i32,
    ) -> Result<LibDrawItem, Error> {
        let _ = self.need_unquoted_symbol_atom("text")?;
        let mut is_private = false;
        if self.at_unquoted_symbol_with("private") {
            let _ = self.need_unquoted_symbol_atom("private")?;
            is_private = true;
        }

        let mut item = LibDrawItem::new("text", unit_number, body_style);
        item.is_private = is_private;
        item.text = Some(
            self.need_symbol_atom("text string")
                .map_err(|_| self.error_here("Invalid text string"))?,
        );

        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("at or effects")),
            };
            match head.as_str() {
                "at" => {
                    let _ = self.need_unquoted_symbol_atom("at")?;
                    let parsed = self.parse_xy3("text at")?;
                    item.at = Some([parsed[0], parsed[1]]);
                    item.angle = Some(parsed[2] / 10.0);
                    self.need_right()?;
                }
                "effects" => {
                    self.parse_eda_text(ParsedEdaTextOwner::lib_item_text(&mut item), true, true)?;
                }
                _ => return Err(self.expecting("at or effects")),
            }
        }

        if !item.visible {
            let field_ordinal = symbol.next_field_ordinal();
            item.kind = "field".to_string();
            item.field_id = PropertyKind::User.default_field_id();
            item.field_ordinal = Some(field_ordinal);
            item.name = Some(format!("Field{field_ordinal}"));
        }

        self.need_right()?;
        Ok(item)
    }

    fn parse_symbol_text_box(
        &mut self,
        unit_number: i32,
        body_style: i32,
    ) -> Result<LibDrawItem, Error> {
        let _ = self.need_unquoted_symbol_atom("text_box")?;
        let mut is_private = false;
        if self.at_unquoted_symbol_with("private") {
            let _ = self.need_unquoted_symbol_atom("private")?;
            is_private = true;
        }
        let mut item = LibDrawItem::new("text_box", unit_number, body_style);
        item.is_private = is_private;
        item.angle = Some(0.0);
        item.text = Some(
            self.need_symbol_atom("text box text")
                .map_err(|_| self.error_here("Invalid text string"))?,
        );
        let mut pos = None;
        let mut end = None;
        let mut size = None;
        let mut stroke_width = None;
        let mut text_size_y = None;
        let mut found_end = false;
        let mut found_size = false;
        let mut found_margins = false;

        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("at, size, stroke, fill or effects")),
            };
            match head.as_str() {
                "start" => {
                    let _ = self.need_unquoted_symbol_atom("start")?;
                    pos = Some(self.parse_xy2("text_box start")?);
                    self.need_right()?;
                }
                "end" => {
                    let _ = self.need_unquoted_symbol_atom("end")?;
                    end = Some(self.parse_xy2("text_box end")?);
                    found_end = true;
                    self.need_right()?;
                }
                "at" => {
                    let _ = self.need_unquoted_symbol_atom("at")?;
                    let parsed = self.parse_xy3("text_box at")?;
                    pos = Some([parsed[0], parsed[1]]);
                    item.angle = Some(parsed[2]);
                    self.need_right()?;
                }
                "size" => {
                    let _ = self.need_unquoted_symbol_atom("size")?;
                    size = Some(self.parse_xy2("text_box size")?);
                    found_size = true;
                    self.need_right()?;
                }
                "stroke" => {
                    let _ = self.need_unquoted_symbol_atom("stroke")?;
                    let parsed_stroke = self.parse_stroke()?;
                    stroke_width = parsed_stroke.width;
                    item.stroke = Some(parsed_stroke);
                }
                "fill" => {
                    let _ = self.need_unquoted_symbol_atom("fill")?;
                    item.fill = Some(self.parse_fill()?);
                }
                "effects" => {
                    self.parse_eda_text(ParsedEdaTextOwner::lib_item_text(&mut item), false, true)?;
                    text_size_y = item
                        .effects
                        .as_ref()
                        .and_then(|effects| effects.font_size.map(|size| size[1]));
                }
                "margins" => {
                    let _ = self.need_unquoted_symbol_atom("margins")?;
                    item.margins = Some([
                        self.parse_f64_atom("margin left")?,
                        self.parse_f64_atom("margin top")?,
                        self.parse_f64_atom("margin right")?,
                        self.parse_f64_atom("margin bottom")?,
                    ]);
                    found_margins = true;
                    self.need_right()?;
                }
                _ => {
                    return Err(self.expecting("at, size, stroke, fill or effects"));
                }
            }
        }

        let pos = pos.unwrap_or([0.0, 0.0]);
        item.at = Some(pos);
        item.end = Some(if found_end {
            end.unwrap_or([0.0, 0.0])
        } else if found_size {
            let size = size.unwrap_or([0.0, 0.0]);
            [pos[0] + size[0], pos[1] + size[1]]
        } else {
            return Err(self.expecting("size"));
        });
        if !found_margins {
            let margin = Self::get_legacy_text_margin(
                stroke_width.unwrap_or(DEFAULT_LINE_WIDTH_MM),
                text_size_y.unwrap_or(DEFAULT_TEXT_SIZE_MM),
            );
            item.margins = Some([margin, margin, margin, margin]);
        }

        self.need_right()?;
        Ok(item)
    }

    fn parse_symbol_pin(
        &mut self,
        unit_number: i32,
        body_style: i32,
    ) -> Result<LibDrawItem, Error> {
        let _ = self.need_unquoted_symbol_atom("pin")?;
        let electrical_type_token = self.need_unquoted_symbol_atom("pin type")?;
        let electrical_type = if matches!(
            electrical_type_token.as_str(),
            "input"
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
                | "no_connect"
                | "unconnected"
        ) {
            electrical_type_token
        } else {
            return Err(self.expecting(
                "input, output, bidirectional, tri_state, passive, unspecified, power_in, power_out, open_collector, open_emitter, free or no_connect",
            ));
        };
        let graphic_shape_token = self.need_unquoted_symbol_atom("pin shape")?;
        let graphic_shape = if matches!(
            graphic_shape_token.as_str(),
            "line"
                | "inverted"
                | "clock"
                | "inverted_clock"
                | "input_low"
                | "clock_low"
                | "output_low"
                | "edge_clock_high"
                | "non_logic"
        ) {
            graphic_shape_token
        } else {
            return Err(self.expecting(
                "line, inverted, clock, inverted_clock, input_low, clock_low, output_low, edge_clock_high, non_logic",
            ));
        };
        let mut item = LibDrawItem::new("pin", unit_number, body_style);
        item.electrical_type = Some(electrical_type);
        item.graphic_shape = Some(graphic_shape);

        while !self.at_right() {
            if self.at_unquoted_symbol_with("hide") {
                let _ = self.need_unquoted_symbol_atom("hide")?;
                item.visible = false;
                continue;
            }

            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("at, name, number, hide, length, or alternate")),
            };
            match head.as_str() {
                "at" => {
                    let _ = self.need_unquoted_symbol_atom("at")?;
                    let parsed = self.parse_xy3("pin at")?;
                    match parsed[2] as i32 {
                        0 | 90 | 180 | 270 => {}
                        _ => return Err(self.expecting("0, 90, 180, or 270")),
                    }
                    item.at = Some([parsed[0], parsed[1]]);
                    item.angle = Some(parsed[2]);
                    self.need_right()?;
                }
                "length" => {
                    let _ = self.need_unquoted_symbol_atom("length")?;
                    item.length = Some(self.parse_f64_atom("pin length")?);
                    self.need_right()?;
                }
                "hide" => {
                    let _ = self.need_unquoted_symbol_atom("hide")?;
                    item.visible = !self.parse_bool_atom("hide")?;
                    self.need_right()?;
                }
                "name" => {
                    let _ = self.need_unquoted_symbol_atom("name")?;
                    let mut parsed = self
                        .need_symbol_atom("pin name")
                        .map_err(|_| self.error_here("Invalid pin name"))?;
                    if self.version.unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION)
                        < VERSION_EMPTY_TILDE_IS_EMPTY
                        && parsed == "~"
                    {
                        parsed.clear();
                    } else if self.version.unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION)
                        < VERSION_TEXT_OVERBAR_NOTATION
                    {
                        parsed = self.convert_old_overbar_notation(parsed);
                    }
                    item.name = Some(parsed);
                    if self.at_right() {
                        self.need_right()?;
                        continue;
                    }
                    self.need_left()?;
                    let mut visible = true;
                    self.parse_eda_text(
                        ParsedEdaTextOwner::detached(
                            item.name.as_mut(),
                            &mut item.name_effects,
                            &mut visible,
                        ),
                        true,
                        false,
                    )?;
                    self.need_right()?;
                }
                "number" => {
                    let _ = self.need_unquoted_symbol_atom("number")?;
                    let mut parsed = self
                        .need_symbol_atom("pin number")
                        .map_err(|_| self.error_here("Invalid pin number"))?;
                    if self.version.unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION)
                        < VERSION_EMPTY_TILDE_IS_EMPTY
                        && parsed == "~"
                    {
                        parsed.clear();
                    } else if self.version.unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION)
                        < VERSION_TEXT_OVERBAR_NOTATION
                    {
                        parsed = self.convert_old_overbar_notation(parsed);
                    }
                    item.number = Some(parsed);
                    if self.at_right() {
                        self.need_right()?;
                        continue;
                    }
                    self.need_left()?;
                    let mut visible = true;
                    self.parse_eda_text(
                        ParsedEdaTextOwner::detached(
                            item.number.as_mut(),
                            &mut item.number_effects,
                            &mut visible,
                        ),
                        false,
                        false,
                    )?;
                    self.need_right()?;
                }
                "alternate" => {
                    let _ = self.need_unquoted_symbol_atom("alternate")?;
                    let mut alt_name = self
                        .need_symbol_atom("alternate pin name")
                        .map_err(|_| self.error_here("Invalid alternate pin name"))?;
                    if self.version.unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION)
                        < VERSION_EMPTY_TILDE_IS_EMPTY
                        && alt_name == "~"
                    {
                        alt_name.clear();
                    } else if self.version.unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION)
                        < VERSION_TEXT_OVERBAR_NOTATION
                    {
                        alt_name = self.convert_old_overbar_notation(alt_name);
                    }

                    let alt_type_token = self.need_unquoted_symbol_atom("alternate pin type")?;
                    let alt_type = if matches!(
                        alt_type_token.as_str(),
                        "input"
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
                            | "no_connect"
                            | "unconnected"
                    ) {
                        alt_type_token
                    } else {
                        return Err(self.expecting(
                            "input, output, bidirectional, tri_state, passive, unspecified, power_in, power_out, open_collector, open_emitter, free or no_connect",
                        ));
                    };
                    let alt_shape_token = self.need_unquoted_symbol_atom("alternate pin shape")?;
                    let alt_shape = if matches!(
                        alt_shape_token.as_str(),
                        "line"
                            | "inverted"
                            | "clock"
                            | "inverted_clock"
                            | "input_low"
                            | "clock_low"
                            | "output_low"
                            | "edge_clock_high"
                            | "non_logic"
                    ) {
                        alt_shape_token
                    } else {
                        return Err(self.expecting(
                            "line, inverted, clock, inverted_clock, input_low, clock_low, output_low, edge_clock_high, non_logic",
                        ));
                    };
                    item.alternates.insert(
                        alt_name.clone(),
                        LibPinAlternate {
                            name: alt_name,
                            electrical_type: alt_type,
                            graphic_shape: alt_shape,
                        },
                    );
                    self.need_right()?;
                }
                _ => return Err(self.expecting("at, name, number, hide, length, or alternate")),
            }
        }

        self.need_right()?;
        Ok(item)
    }

    fn parse_lib_property(&mut self, symbol: &mut LibSymbol) -> Result<(), Error> {
        let _ = self.need_unquoted_symbol_atom("property")?;
        let mut is_private = false;

        if self.at_unquoted_symbol_with("private") {
            let _ = self.need_unquoted_symbol_atom("private")?;
            is_private = true;
        }

        let name = self
            .need_symbol_atom("property name")
            .map_err(|_| self.error_here("Invalid property name"))?;

        if name.is_empty() {
            return Err(self.error_here("Empty property name"));
        }

        let mut field_id = PropertyKind::User;

        for kind in PropertyKind::SYMBOL_MANDATORY_FIELDS {
            if name.eq_ignore_ascii_case(kind.canonical_key()) {
                field_id = kind;
                break;
            }
        }

        let mut property = Property::new_named(field_id, &name, String::new(), is_private);

        if matches!(property.kind, PropertyKind::User) {
            property.ordinal = symbol.next_field_ordinal();
        }

        property.value = self
            .need_symbol_atom("property value")
            .map_err(|_| self.error_here("Invalid property value"))
            .map(|raw| {
                if self.version.unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION)
                    < VERSION_EMPTY_TILDE_IS_EMPTY
                    && raw == "~"
                {
                    String::new()
                } else {
                    raw
                }
            })?;

        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => {
                    return Err(
                        self.expecting("id, at, hide, show_name, do_not_autoplace, or effects")
                    );
                }
            };
            match head.as_str() {
                "id" => {
                    let _ = self.need_unquoted_symbol_atom("id")?;
                    let _ = self.parse_i32_atom("field ID")?;
                    self.need_right()?;
                }
                "at" => {
                    let _ = self.need_unquoted_symbol_atom("at")?;
                    let parsed = self.parse_xy3("property at")?;
                    property.at = Some([parsed[0], parsed[1]]);
                    property.angle = Some(parsed[2]);
                    self.need_right()?;
                }
                "hide" => {
                    let _ = self.need_unquoted_symbol_atom("hide")?;
                    property.visible = !self.parse_bool_atom("hide")?;
                    self.need_right()?;
                }
                "show_name" => {
                    let _ = self.need_unquoted_symbol_atom("show_name")?;
                    property.show_name = self.parse_maybe_absent_bool(true)?;
                }
                "do_not_autoplace" => {
                    let _ = self.need_unquoted_symbol_atom("do_not_autoplace")?;
                    property.can_autoplace = !self.parse_maybe_absent_bool(true)?;
                }
                "effects" => {
                    let convert_overbar = property.kind == PropertyKind::SymbolValue;
                    self.parse_eda_text(
                        ParsedEdaTextOwner::property(&mut property),
                        convert_overbar,
                        true,
                    )?;
                }
                _ => {
                    return Err(
                        self.expecting("id, at, hide, show_name, do_not_autoplace, or effects")
                    );
                }
            }
        }

        if matches!(
            property.kind,
            PropertyKind::SymbolReference
                | PropertyKind::SymbolValue
                | PropertyKind::SymbolFootprint
                | PropertyKind::SymbolDatasheet
                | PropertyKind::SymbolDescription
        ) {
            let existing = symbol
                .properties
                .iter_mut()
                .find(|existing| existing.kind == property.kind)
                .expect("lib symbols start with mandatory fields");
            *existing = property;
        } else if name == "ki_keywords" {
            symbol.keywords = Some(property.value);
        } else if name == "ki_description" {
            symbol.description = Some(property.value);
        } else if name == "ki_fp_filters" {
            symbol.fp_filters_specified = true;
            symbol.fp_filters = property
                .value
                .split_whitespace()
                .map(Self::unescape_string_markers)
                .collect();
        } else if name == "ki_locked" {
            symbol.locked_units = true;
        } else {
            let mut property = property;
            let field_name_in_use = |name: &str, symbol: &LibSymbol| {
                symbol
                    .properties
                    .iter()
                    .any(|existing| existing.key == name)
                    || symbol.units.iter().any(|unit| {
                        unit.draw_items.iter().any(|existing| {
                            existing.kind == "field" && existing.name.as_deref() == Some(name)
                        })
                    })
            };
            let mut existing = field_name_in_use(&property.key, symbol);

            if existing {
                let base = property.key.clone();

                for suffix in 1..10 {
                    let candidate = format!("{base}_{suffix}");

                    if !field_name_in_use(&candidate, symbol) {
                        property.key = candidate;
                        existing = false;
                        break;
                    }
                }
            }

            if !existing {
                let mut field = LibDrawItem::new("field", 1, 1);
                field.field_ordinal = Some(property.ordinal);
                field.field_id = property.id;
                field.is_private = property.is_private;
                field.visible = property.visible;
                field.show_name = property.show_name;
                field.can_autoplace = property.can_autoplace;
                field.at = property.at;
                field.angle = property.angle;
                field.name = Some(property.key);
                field.text = Some(property.value);
                field.effects = property.effects;
                symbol.add_draw_item(field);
            }
        }

        self.need_right()?;
        Ok(())
    }

    fn parse_bus_alias(&mut self) -> Result<(), Error> {
        let _ = self.need_unquoted_symbol_atom("bus_alias")?;
        let mut alias = BusAlias::new(self.need_symbol_atom("bus alias name")?);
        let version = self.require_known_version()?;
        if version < VERSION_NEW_OVERBAR_NOTATION {
            alias.name = self.convert_old_overbar_notation(alias.name);
        }

        self.need_left()?;
        let members_head = match &self.current().kind {
            TokKind::Atom(value)
                if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
            {
                value.clone()
            }
            _ => return Err(self.expecting("members")),
        };
        if members_head != "members" {
            return Err(self.expecting("members"));
        }
        let _ = self.need_unquoted_symbol_atom("members")?;

        while !self.at_right() {
            let mut member = self.need_quoted_atom("quoted string")?;
            if version < VERSION_NEW_OVERBAR_NOTATION {
                member = self.convert_old_overbar_notation(member);
            }
            alias.members.push(member);
        }
        self.need_right()?;
        self.screen.add_bus_alias(alias);
        self.need_right()?;
        Ok(())
    }

    fn parse_junction(&mut self) -> Result<Junction, Error> {
        let _ = self.need_unquoted_symbol_atom("junction")?;
        let mut junction = Junction::new();
        let mut has_at = false;
        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("at, diameter, color or uuid")),
            };
            match head.as_str() {
                "at" => {
                    let _ = self.need_unquoted_symbol_atom("at")?;
                    junction.at = self.parse_xy2("junction at")?;
                    has_at = true;
                    self.need_right()?;
                }
                "diameter" => {
                    let _ = self.need_unquoted_symbol_atom("diameter")?;
                    junction.diameter = Some(self.parse_f64_atom("junction diameter")?);
                    self.need_right()?;
                }
                "color" => {
                    let _ = self.need_unquoted_symbol_atom("color")?;
                    junction.color = Some([
                        f64::from(self.parse_i32_atom("red")?) / 255.0,
                        f64::from(self.parse_i32_atom("green")?) / 255.0,
                        f64::from(self.parse_i32_atom("blue")?) / 255.0,
                        self.parse_f64_atom("alpha")?.clamp(0.0, 1.0),
                    ]);
                    self.need_right()?;
                }
                "uuid" => {
                    let _ = self.need_unquoted_symbol_atom("uuid")?;
                    junction.uuid = Some(self.parse_kiid()?);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("at, diameter, color or uuid")),
            }
        }
        if !has_at {
            junction.at = [0.0, 0.0];
        }
        self.need_right()?;
        Ok(junction)
    }

    fn parse_no_connect(&mut self) -> Result<NoConnect, Error> {
        let _ = self.need_unquoted_symbol_atom("no_connect")?;
        let mut no_connect = NoConnect::new();
        let mut has_at = false;
        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("at or uuid")),
            };
            match head.as_str() {
                "at" => {
                    let _ = self.need_unquoted_symbol_atom("at")?;
                    no_connect.at = self.parse_xy2("no_connect at")?;
                    has_at = true;
                    self.need_right()?;
                }
                "uuid" => {
                    let _ = self.need_unquoted_symbol_atom("uuid")?;
                    no_connect.uuid = Some(self.parse_kiid()?);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("at or uuid")),
            }
        }
        if !has_at {
            no_connect.at = [0.0, 0.0];
        }
        self.need_right()?;
        Ok(no_connect)
    }

    fn parse_bus_entry(&mut self) -> Result<BusEntry, Error> {
        let _ = self.need_unquoted_symbol_atom("bus_entry")?;
        let mut bus_entry = BusEntry::new();
        let mut has_at = false;
        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("at, size, uuid or stroke")),
            };
            match head.as_str() {
                "at" => {
                    let _ = self.need_unquoted_symbol_atom("at")?;
                    bus_entry.at = self.parse_xy2("bus_entry at")?;
                    has_at = true;
                    self.need_right()?;
                }
                "size" => {
                    let _ = self.need_unquoted_symbol_atom("size")?;
                    bus_entry.size = self.parse_xy2("bus_entry size")?;
                    self.need_right()?;
                }
                "stroke" => {
                    let _ = self.need_unquoted_symbol_atom("stroke")?;
                    bus_entry.has_stroke = true;
                    let mut parsed_stroke = self.parse_stroke()?;
                    if self.require_known_version()? <= 20211123
                        && parsed_stroke.style == StrokeStyle::Default
                    {
                        parsed_stroke.style = StrokeStyle::Dash;
                    }
                    bus_entry.stroke = Some(parsed_stroke);
                }
                "uuid" => {
                    let _ = self.need_unquoted_symbol_atom("uuid")?;
                    bus_entry.uuid = Some(self.parse_kiid()?);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("at, size, uuid or stroke")),
            }
        }
        if !has_at {
            bus_entry.at = [0.0, 0.0];
        }
        self.need_right()?;
        Ok(bus_entry)
    }

    fn parse_sch_line(&mut self) -> Result<Line, Error> {
        let head = match &self.current().kind {
            TokKind::Atom(value)
                if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
            {
                value.clone()
            }
            _ => return Err(self.expecting("wire or bus")),
        };
        let kind = match head.as_str() {
            "wire" => {
                let _ = self.need_unquoted_symbol_atom("wire")?;
                LineKind::Wire
            }
            "bus" => {
                let _ = self.need_unquoted_symbol_atom("bus")?;
                LineKind::Bus
            }
            _ => return Err(self.error_here("invalid schematic line kind")),
        };
        let mut line = Line::new(kind);
        line.points = vec![[0.0, 0.0], [0.0, 0.0]];
        let mut has_pts = false;
        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("at, uuid or stroke")),
            };
            match head.as_str() {
                "pts" => {
                    let _ = self.need_unquoted_symbol_atom("pts")?;
                    self.need_left()?;
                    let start_head = match &self.current().kind {
                        TokKind::Atom(value)
                            if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                        {
                            value.clone()
                        }
                        _ => return Err(self.expecting("xy")),
                    };
                    if start_head != "xy" {
                        return Err(self.expecting("xy"));
                    }
                    let _ = self.need_unquoted_symbol_atom("xy")?;
                    let start = self.parse_xy2("xy")?;
                    self.need_right()?;
                    self.need_left()?;
                    let end_head = match &self.current().kind {
                        TokKind::Atom(value)
                            if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                        {
                            value.clone()
                        }
                        _ => return Err(self.expecting("xy")),
                    };
                    if end_head != "xy" {
                        return Err(self.expecting("xy"));
                    }
                    let _ = self.need_unquoted_symbol_atom("xy")?;
                    let end = self.parse_xy2("xy")?;
                    self.need_right()?;
                    self.need_right()?;
                    line.points = vec![start, end];
                    has_pts = true;
                }
                "uuid" => {
                    let _ = self.need_unquoted_symbol_atom("uuid")?;
                    line.uuid = Some(self.parse_kiid()?);
                    self.need_right()?;
                }
                "stroke" => {
                    let _ = self.need_unquoted_symbol_atom("stroke")?;
                    line.has_stroke = true;
                    let mut parsed_stroke = self.parse_stroke()?;
                    if self.require_known_version()? <= 20211123
                        && parsed_stroke.style == StrokeStyle::Default
                    {
                        parsed_stroke.style = StrokeStyle::Dash;
                    }
                    line.stroke = Some(parsed_stroke);
                }
                _ => return Err(self.expecting("at, uuid or stroke")),
            }
        }
        if !has_pts {
            line.points = vec![[0.0, 0.0], [0.0, 0.0]];
        }
        self.need_right()?;
        Ok(line)
    }

    fn parse_sch_text(&mut self) -> Result<SchItem, Error> {
        let target = match self
            .need_unquoted_symbol_atom(
                "text, label, global_label, hierarchical_label, directive_label, class_label, or netclass_flag",
            )?
            .as_str()
        {
            "text" => SchTextTarget::Text,
            "label" => SchTextTarget::Label(LabelKind::Local),
            "global_label" => SchTextTarget::Label(LabelKind::Global),
            "hierarchical_label" => SchTextTarget::Label(LabelKind::Hierarchical),
            "directive_label" => SchTextTarget::Label(LabelKind::Directive),
            "class_label" | "netclass_flag" => SchTextTarget::Label(LabelKind::NetclassFlag),
            _ => return Err(self.error_here("invalid schematic text kind")),
        };

        let text = self
            .need_symbol_atom("text value")
            .map_err(|_| self.error_here("Invalid text string"))?;
        let mut item = match target {
            SchTextTarget::Text => ParsedSchText::Text(Text::new(TextKind::Text, text)),
            SchTextTarget::Label(kind) => {
                let mut label = Label::new(kind, text);

                if matches!(label.kind, LabelKind::Global) {
                    label.properties.push(Property {
                        id: PropertyKind::GlobalLabelIntersheetRefs.default_field_id(),
                        ordinal: PropertyKind::GlobalLabelIntersheetRefs
                            .default_field_id()
                            .unwrap_or(0),
                        key: PropertyKind::GlobalLabelIntersheetRefs
                            .canonical_key()
                            .to_string(),
                        value: "${INTERSHEET_REFS}".to_string(),
                        kind: PropertyKind::GlobalLabelIntersheetRefs,
                        is_private: false,
                        at: Some([0.0, 0.0]),
                        angle: None,
                        visible: false,
                        show_name: false,
                        can_autoplace: true,
                        has_effects: false,
                        effects: None,
                    });
                }

                ParsedSchText::Label(label)
            }
        };

        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("at, shape, iref, uuid or effects")),
            };
            match head.as_str() {
                "exclude_from_sim" => {
                    let _ = self.need_unquoted_symbol_atom("exclude_from_sim")?;
                    let excluded_from_sim = self.parse_bool_atom("exclude_from_sim")?;
                    match &mut item {
                        ParsedSchText::Text(text) => text.excluded_from_sim = excluded_from_sim,
                        ParsedSchText::Label(label) => label.excluded_from_sim = excluded_from_sim,
                    }
                    self.need_right()?;
                }
                "at" => {
                    let _ = self.need_unquoted_symbol_atom("at")?;
                    let parsed = self.parse_xy3("text at")?;
                    match &mut item {
                        ParsedSchText::Text(text) => {
                            text.at = [parsed[0], parsed[1], Self::normalize_text_angle(parsed[2])];
                        }
                        ParsedSchText::Label(label) => {
                            label.at = [parsed[0], parsed[1]];
                            label.angle = Self::normalize_text_angle(parsed[2]);
                            label.spin = Self::get_label_spin_style(label.angle);
                            if matches!(label.kind, LabelKind::Global) {
                                let intersheet_refs = label
                                    .properties
                                    .iter_mut()
                                    .find(|property| {
                                        property.kind == PropertyKind::GlobalLabelIntersheetRefs
                                    })
                                    .expect("global labels start with intersheet refs property");
                                if !intersheet_refs.visible
                                    && intersheet_refs.at == Some([0.0, 0.0])
                                {
                                    intersheet_refs.at = Some(label.at);
                                }
                            }
                        }
                    }
                    self.need_right()?;
                }
                "shape" => {
                    let _ = self.need_unquoted_symbol_atom("shape")?;
                    let ParsedSchText::Label(label) = &mut item else {
                        return Err(self.unexpected("shape"));
                    };
                    if matches!(label.kind, LabelKind::Local) {
                        return Err(self.unexpected("shape"));
                    }
                    label.shape = match self.need_unquoted_symbol_atom("shape")?.as_str() {
                        "input" => LabelShape::Input,
                        "output" => LabelShape::Output,
                        "bidirectional" => LabelShape::Bidirectional,
                        "tri_state" => LabelShape::TriState,
                        "passive" => LabelShape::Passive,
                        "dot" => LabelShape::Dot,
                        "round" => LabelShape::Round,
                        "diamond" => LabelShape::Diamond,
                        "rectangle" => LabelShape::Rectangle,
                        _ => {
                            return Err(self.expecting(
                                "input, output, bidirectional, tri_state, passive, dot, round, diamond or rectangle",
                            ))
                        }
                    };
                    self.need_right()?;
                }
                "length" => {
                    let _ = self.need_unquoted_symbol_atom("length")?;
                    let ParsedSchText::Label(label) = &mut item else {
                        return Err(self.unexpected("length"));
                    };
                    if !matches!(label.kind, LabelKind::Directive | LabelKind::NetclassFlag) {
                        return Err(self.unexpected("length"));
                    }
                    label.pin_length = Some(self.parse_f64_atom("pin length")?);
                    self.need_right()?;
                }
                "fields_autoplaced" => {
                    let _ = self.need_unquoted_symbol_atom("fields_autoplaced")?;
                    if self.parse_maybe_absent_bool(true)? {
                        match &mut item {
                            ParsedSchText::Text(text) => {
                                text.fields_autoplaced = FieldAutoplacement::Auto;
                            }
                            ParsedSchText::Label(label) => {
                                label.fields_autoplaced = FieldAutoplacement::Auto;
                            }
                        }
                    }
                }
                "effects" => match &mut item {
                    ParsedSchText::Text(text) => {
                        self.parse_eda_text(ParsedEdaTextOwner::text(text), true, true)?;
                        text.visible = true;
                    }
                    ParsedSchText::Label(label) => {
                        self.parse_eda_text(ParsedEdaTextOwner::label(label), true, true)?;
                        label.visible = true;
                    }
                },
                "iref" => {
                    let _ = self.need_unquoted_symbol_atom("iref")?;
                    let ParsedSchText::Label(label) = &mut item else {
                        continue;
                    };
                    if matches!(label.kind, LabelKind::Global) {
                        label.iref_at = Some(self.parse_xy2("iref")?);
                        self.need_right()?;
                        let iref_at = label.iref_at;
                        let intersheet_refs = label
                            .properties
                            .iter_mut()
                            .find(|property| {
                                property.kind == PropertyKind::GlobalLabelIntersheetRefs
                            })
                            .expect("global labels start with intersheet refs property");
                        intersheet_refs.at = iref_at;
                        intersheet_refs.visible = true;
                    }
                }
                "uuid" => {
                    let _ = self.need_unquoted_symbol_atom("uuid")?;
                    let uuid = self.parse_kiid()?;
                    match &mut item {
                        ParsedSchText::Text(text) => text.uuid = Some(uuid),
                        ParsedSchText::Label(label) => label.uuid = Some(uuid),
                    }
                    self.need_right()?;
                }
                "property" => {
                    let ParsedSchText::Label(label) = &mut item else {
                        return Err(self.unexpected("property"));
                    };
                    let property = self.parse_sch_field(FieldParent::Label(label))?;

                    if property.kind == PropertyKind::GlobalLabelIntersheetRefs {
                        let existing = label
                            .properties
                            .iter_mut()
                            .find(|existing| {
                                existing.kind == PropertyKind::GlobalLabelIntersheetRefs
                            })
                            .expect("global labels start with intersheet refs property");
                        *existing = property;
                    } else {
                        label.properties.push(property);
                    }
                }
                _ => return Err(self.expecting("at, shape, iref, uuid or effects")),
            }
        }

        self.need_right()?;
        Ok(match item {
            ParsedSchText::Text(text) => SchItem::Text(text),
            ParsedSchText::Label(mut label) => {
                if label.properties.is_empty() {
                    label.fields_autoplaced = FieldAutoplacement::Auto;
                }
                SchItem::Label(label)
            }
        })
    }

    fn parse_sch_text_box(&mut self) -> Result<TextBox, Error> {
        let _ = self.need_unquoted_symbol_atom("text_box")?;
        let mut text_box = TextBox::new();
        self.parse_sch_text_box_content(ParsedTextBoxOwner::TextBox(&mut text_box))?;
        Ok(text_box)
    }

    fn parse_sch_table_cell(&mut self) -> Result<TableCell, Error> {
        let _ = self.need_unquoted_symbol_atom("table_cell")?;
        let mut text_box = TableCell::new();
        self.parse_sch_text_box_content(ParsedTextBoxOwner::TableCell(&mut text_box))?;
        Ok(text_box)
    }

    fn parse_sch_text_box_content(
        &mut self,
        mut owner: ParsedTextBoxOwner<'_>,
    ) -> Result<(), Error> {
        let is_table_cell = owner.is_table_cell();
        match &mut owner {
            ParsedTextBoxOwner::TextBox(text_box) => {
                text_box.text = self
                    .need_symbol_atom("text box text")
                    .map_err(|_| self.error_here("Invalid text string"))?;
            }
            ParsedTextBoxOwner::TableCell(text_box) => {
                text_box.text = self
                    .need_symbol_atom("text box text")
                    .map_err(|_| self.error_here("Invalid text string"))?;
            }
        }
        let mut pos = None;
        let mut end = None;
        let mut size = None;
        let mut stroke_width = None;
        let mut text_size_y = None;
        let mut found_end = false;
        let mut found_size = false;
        let mut found_margins = false;

        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => {
                    return Err(self.expecting(if is_table_cell {
                        "at, size, stroke, fill, effects, span or uuid"
                    } else {
                        "at, size, stroke, fill, effects or uuid"
                    }));
                }
            };
            match head.as_str() {
                "exclude_from_sim" => {
                    let _ = self.need_unquoted_symbol_atom("exclude_from_sim")?;
                    let excluded_from_sim = self.parse_bool_atom("exclude_from_sim")?;
                    match &mut owner {
                        ParsedTextBoxOwner::TextBox(text_box) => {
                            text_box.excluded_from_sim = excluded_from_sim;
                        }
                        ParsedTextBoxOwner::TableCell(text_box) => {
                            text_box.excluded_from_sim = excluded_from_sim;
                        }
                    }
                    self.need_right()?;
                }
                "start" => {
                    let _ = self.need_unquoted_symbol_atom("start")?;
                    pos = Some(self.parse_xy2("text_box start")?);
                    self.need_right()?;
                }
                "end" => {
                    let _ = self.need_unquoted_symbol_atom("end")?;
                    end = Some(self.parse_xy2("text_box end")?);
                    found_end = true;
                    self.need_right()?;
                }
                "at" => {
                    let _ = self.need_unquoted_symbol_atom("at")?;
                    let parsed = self.parse_xy3("text_box at")?;
                    pos = Some([parsed[0], parsed[1]]);
                    match &mut owner {
                        ParsedTextBoxOwner::TextBox(text_box) => text_box.angle = parsed[2],
                        ParsedTextBoxOwner::TableCell(text_box) => text_box.angle = parsed[2],
                    }
                    self.need_right()?;
                }
                "size" => {
                    let _ = self.need_unquoted_symbol_atom("size")?;
                    size = Some(self.parse_xy2("text_box size")?);
                    found_size = true;
                    self.need_right()?;
                }
                "span" if is_table_cell => {
                    let _ = self.need_unquoted_symbol_atom("span")?;
                    let col_span = self.parse_i32_atom("column span")?;
                    let row_span = self.parse_i32_atom("row span")?;
                    match &mut owner {
                        ParsedTextBoxOwner::TableCell(text_box) => {
                            text_box.col_span = col_span;
                            text_box.row_span = row_span;
                        }
                        ParsedTextBoxOwner::TextBox(_) => {
                            return Err(self.expecting("at, size, stroke, fill, effects or uuid"));
                        }
                    }
                    self.need_right()?;
                }
                "stroke" => {
                    let _ = self.need_unquoted_symbol_atom("stroke")?;
                    let parsed_stroke = self.parse_stroke()?;
                    stroke_width = parsed_stroke.width;
                    match &mut owner {
                        ParsedTextBoxOwner::TextBox(text_box) => {
                            text_box.stroke = Some(parsed_stroke)
                        }
                        ParsedTextBoxOwner::TableCell(text_box) => {
                            text_box.stroke = Some(parsed_stroke)
                        }
                    }
                }
                "fill" => {
                    let _ = self.need_unquoted_symbol_atom("fill")?;
                    let parsed_fill = Some(self.parse_fill()?);
                    match &mut owner {
                        ParsedTextBoxOwner::TextBox(text_box) => {
                            text_box.fill = parsed_fill;
                            Self::fixup_sch_fill_mode(&mut text_box.fill, &text_box.stroke);
                        }
                        ParsedTextBoxOwner::TableCell(text_box) => {
                            text_box.fill = parsed_fill;
                            Self::fixup_sch_fill_mode(&mut text_box.fill, &text_box.stroke);
                        }
                    }
                }
                "effects" => {
                    let parsed_font_size;
                    match &mut owner {
                        ParsedTextBoxOwner::TextBox(text_box) => {
                            self.parse_eda_text(
                                ParsedEdaTextOwner::text_box(text_box),
                                false,
                                true,
                            )?;
                            parsed_font_size = text_box
                                .effects
                                .as_ref()
                                .and_then(|effects| effects.font_size);
                        }
                        ParsedTextBoxOwner::TableCell(text_box) => {
                            self.parse_eda_text(
                                ParsedEdaTextOwner::table_cell(text_box),
                                false,
                                true,
                            )?;
                            parsed_font_size = text_box
                                .effects
                                .as_ref()
                                .and_then(|effects| effects.font_size);
                        }
                    }
                    text_size_y = parsed_font_size.map(|size| size[1]);
                }
                "margins" => {
                    let _ = self.need_unquoted_symbol_atom("margins")?;
                    let margins = Some([
                        self.parse_f64_atom("margin left")?,
                        self.parse_f64_atom("margin top")?,
                        self.parse_f64_atom("margin right")?,
                        self.parse_f64_atom("margin bottom")?,
                    ]);
                    match &mut owner {
                        ParsedTextBoxOwner::TextBox(text_box) => text_box.margins = margins,
                        ParsedTextBoxOwner::TableCell(text_box) => text_box.margins = margins,
                    }
                    found_margins = true;
                    self.need_right()?;
                }
                "uuid" => {
                    let _ = self.need_unquoted_symbol_atom("uuid")?;
                    let uuid = Some(self.parse_raw_kiid()?);
                    match &mut owner {
                        ParsedTextBoxOwner::TextBox(text_box) => text_box.uuid = uuid,
                        ParsedTextBoxOwner::TableCell(text_box) => text_box.uuid = uuid,
                    }
                    self.need_right()?;
                }
                _ => {
                    return Err(self.expecting(if is_table_cell {
                        "at, size, stroke, fill, effects, span or uuid"
                    } else {
                        "at, size, stroke, fill, effects or uuid"
                    }));
                }
            }
        }

        let at = pos.unwrap_or([0.0, 0.0]);
        let end = if found_end {
            end.unwrap_or([0.0, 0.0])
        } else if found_size {
            let size = size.unwrap_or([0.0, 0.0]);
            [at[0] + size[0], at[1] + size[1]]
        } else {
            return Err(self.expecting("size"));
        };
        match &mut owner {
            ParsedTextBoxOwner::TextBox(text_box) => {
                text_box.at = at;
                text_box.end = end;
            }
            ParsedTextBoxOwner::TableCell(text_box) => {
                text_box.at = at;
                text_box.end = end;
            }
        }
        if !found_margins {
            let margins = Some({
                let margin = Self::get_legacy_text_margin(
                    stroke_width.unwrap_or(DEFAULT_LINE_WIDTH_MM),
                    text_size_y.unwrap_or(DEFAULT_TEXT_SIZE_MM),
                );
                [margin, margin, margin, margin]
            });
            match &mut owner {
                ParsedTextBoxOwner::TextBox(text_box) => text_box.margins = margins,
                ParsedTextBoxOwner::TableCell(text_box) => text_box.margins = margins,
            }
        }

        self.need_right()?;
        Ok(())
    }

    fn parse_sch_table(&mut self) -> Result<Table, Error> {
        let _ = self.need_unquoted_symbol_atom("table")?;
        let version = self.require_known_version()?;
        if version < VERSION_TABLES {
            return Err(self.error_here(format!(
                "table requires schematic version {VERSION_TABLES} or newer"
            )));
        }
        let mut table = Table::new(DEFAULT_LINE_WIDTH_MM);
        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => {
                    return Err(self.expecting(
                        "columns, col_widths, row_heights, border, separators, uuid, header or cells",
                    ));
                }
            };
            match head.as_str() {
                "column_count" => {
                    let _ = self.need_unquoted_symbol_atom("column_count")?;
                    table.set_column_count(self.parse_i32_atom("column count")?);
                    self.need_right()?;
                }
                "column_widths" => {
                    let _ = self.need_unquoted_symbol_atom("column_widths")?;
                    let mut col = 0usize;
                    while !self.at_right() {
                        table.set_column_width(col, self.parse_f64_atom("column width")?);
                        col += 1;
                    }
                    self.need_right()?;
                }
                "row_heights" => {
                    let _ = self.need_unquoted_symbol_atom("row_heights")?;
                    let mut row = 0usize;
                    while !self.at_right() {
                        table.set_row_height(row, self.parse_f64_atom("row height")?);
                        row += 1;
                    }
                    self.need_right()?;
                }
                "cells" => {
                    let _ = self.need_unquoted_symbol_atom("cells")?;
                    while !self.at_right() {
                        self.need_left()?;
                        let cell = self.parse_sch_table_cell()?;
                        table.add_cell(cell);
                    }
                    self.need_right()?;
                }
                "border" => {
                    let _ = self.need_unquoted_symbol_atom("border")?;
                    while !self.at_right() {
                        self.need_left()?;
                        let border_head = match &self.current().kind {
                            TokKind::Atom(value)
                                if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                            {
                                value.clone()
                            }
                            _ => return Err(self.expecting("external, header or stroke")),
                        };
                        match border_head.as_str() {
                            "external" => {
                                let _ = self.need_unquoted_symbol_atom("external")?;
                                table.border_external = self.parse_bool_atom("external")?;
                                self.need_right()?;
                            }
                            "header" => {
                                let _ = self.need_unquoted_symbol_atom("header")?;
                                table.border_header = self.parse_bool_atom("header")?;
                                self.need_right()?;
                            }
                            "stroke" => {
                                let _ = self.need_unquoted_symbol_atom("stroke")?;
                                table.border_stroke = self.parse_stroke()?;
                            }
                            _ => return Err(self.expecting("external, header or stroke")),
                        }
                    }
                    self.need_right()?;
                }
                "separators" => {
                    let _ = self.need_unquoted_symbol_atom("separators")?;
                    while !self.at_right() {
                        self.need_left()?;
                        let separators_head = match &self.current().kind {
                            TokKind::Atom(value)
                                if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                            {
                                value.clone()
                            }
                            _ => return Err(self.expecting("rows, cols, or stroke")),
                        };
                        match separators_head.as_str() {
                            "rows" => {
                                let _ = self.need_unquoted_symbol_atom("rows")?;
                                table.separators_rows = self.parse_bool_atom("rows")?;
                                self.need_right()?;
                            }
                            "cols" => {
                                let _ = self.need_unquoted_symbol_atom("cols")?;
                                table.separators_cols = self.parse_bool_atom("cols")?;
                                self.need_right()?;
                            }
                            "stroke" => {
                                let _ = self.need_unquoted_symbol_atom("stroke")?;
                                table.separators_stroke = self.parse_stroke()?;
                            }
                            _ => return Err(self.expecting("rows, cols, or stroke")),
                        }
                    }
                    self.need_right()?;
                }
                "uuid" => {
                    let _ = self.need_unquoted_symbol_atom("uuid")?;
                    table.uuid = Some(self.parse_kiid()?);
                    self.need_right()?;
                }
                _ => {
                    return Err(self.expecting(
                        "columns, col_widths, row_heights, border, separators, uuid, header or cells",
                    ));
                }
            }
        }
        if table.get_cell(0, 0).is_none() {
            return Err(self.error_here("Invalid table: no cells defined"));
        }
        self.need_right()?;
        Ok(table)
    }

    fn parse_sch_image(&mut self) -> Result<Image, Error> {
        let _ = self.need_unquoted_symbol_atom("image")?;
        let mut image = Image::new();
        let mut has_at = false;
        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("at, scale, uuid or data")),
            };
            match head.as_str() {
                "at" => {
                    let _ = self.need_unquoted_symbol_atom("at")?;
                    image.at = self.parse_xy2("image at")?;
                    has_at = true;
                    self.need_right()?;
                }
                "scale" => {
                    let _ = self.need_unquoted_symbol_atom("scale")?;
                    let parsed_scale = self.parse_f64_atom("image scale factor")?;
                    image.scale = if parsed_scale.is_normal() {
                        parsed_scale
                    } else {
                        1.0
                    };
                    self.need_right()?;
                }
                "uuid" => {
                    let _ = self.need_unquoted_symbol_atom("uuid")?;
                    image.uuid = Some(self.parse_kiid()?);
                    self.need_right()?;
                }
                "data" => {
                    let _ = self.need_unquoted_symbol_atom("data")?;
                    let mut encoded = String::new();
                    while !self.at_right() {
                        encoded.push_str(
                            &self
                                .need_symbol_atom("base64 image data")
                                .map_err(|_| self.expecting("base64 image data"))?,
                        );
                    }
                    let decoded = base64::engine::general_purpose::STANDARD
                        .decode(&encoded)
                        .map_err(|_| self.error_here("Failed to read image data."))?;
                    if !Self::validate_image_data(&decoded) {
                        return Err(self.error_here("Failed to read image data."));
                    }
                    if self.require_known_version()? <= VERSION_IMAGE_PPI_SCALE_ADJUSTMENT {
                        if let Some(ppi) = Self::read_png_ppi(&decoded) {
                            image.scale *= ppi / 300.0;
                        }
                    }
                    image.data = Some(encoded);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("at, scale, uuid or data")),
            }
        }
        if !has_at {
            image.at = [0.0, 0.0];
        }
        self.need_right()?;
        Ok(image)
    }

    fn parse_sch_polyline(&mut self) -> Result<Shape, Error> {
        let _ = self.need_unquoted_symbol_atom("polyline")?;
        let mut shape = Shape::new(ShapeKind::Polyline);
        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("pts, uuid, stroke, or fill")),
            };
            match head.as_str() {
                "pts" => {
                    let _ = self.need_unquoted_symbol_atom("pts")?;
                    let mut parsed_points = Vec::new();
                    while !self.at_right() {
                        self.need_left()?;
                        let head = match &self.current().kind {
                            TokKind::Atom(value)
                                if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                            {
                                value.clone()
                            }
                            _ => return Err(self.expecting("xy")),
                        };
                        if head != "xy" {
                            return Err(self.expecting("xy"));
                        }
                        let _ = self.need_unquoted_symbol_atom("xy")?;
                        parsed_points.push(self.parse_xy2("xy")?);
                        self.need_right()?;
                    }
                    shape.points = parsed_points;
                    self.need_right()?;
                }
                "stroke" => {
                    let _ = self.need_unquoted_symbol_atom("stroke")?;
                    shape.has_stroke = true;
                    let mut parsed_stroke = self.parse_stroke()?;
                    if self.require_known_version()? <= 20211123
                        && parsed_stroke.style == StrokeStyle::Default
                    {
                        parsed_stroke.style = StrokeStyle::Dash;
                    }
                    shape.stroke = Some(parsed_stroke);
                }
                "fill" => {
                    let _ = self.need_unquoted_symbol_atom("fill")?;
                    shape.has_fill = true;
                    shape.fill = Some(self.parse_fill()?);
                }
                "uuid" => {
                    let _ = self.need_unquoted_symbol_atom("uuid")?;
                    shape.uuid = Some(self.parse_kiid()?);
                    self.need_right()?;
                }
                _ => {
                    return Err(self.expecting("pts, uuid, stroke, or fill"));
                }
            }
        }
        Self::fixup_sch_fill_mode(&mut shape.fill, &shape.stroke);
        self.need_right()?;
        Ok(shape)
    }

    fn parse_sch_arc(&mut self) -> Result<Shape, Error> {
        let _ = self.need_unquoted_symbol_atom("arc")?;
        let mut shape = Shape::new(ShapeKind::Arc);
        shape.points = vec![[0.0, 0.0], [0.0, 0.0], [0.0, 0.0]];
        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("start, mid, end, stroke, fill or uuid")),
            };
            match head.as_str() {
                "start" => {
                    let _ = self.need_unquoted_symbol_atom("start")?;
                    shape.points[0] = self.parse_xy2("shape start")?;
                    self.need_right()?;
                }
                "mid" => {
                    let _ = self.need_unquoted_symbol_atom("mid")?;
                    shape.points[1] = self.parse_xy2("shape mid")?;
                    self.need_right()?;
                }
                "end" => {
                    let _ = self.need_unquoted_symbol_atom("end")?;
                    shape.points[2] = self.parse_xy2("shape end")?;
                    self.need_right()?;
                }
                "stroke" => {
                    let _ = self.need_unquoted_symbol_atom("stroke")?;
                    shape.has_stroke = true;
                    shape.stroke = Some(self.parse_stroke()?);
                }
                "fill" => {
                    let _ = self.need_unquoted_symbol_atom("fill")?;
                    shape.has_fill = true;
                    shape.fill = Some(self.parse_fill()?);
                }
                "uuid" => {
                    let _ = self.need_unquoted_symbol_atom("uuid")?;
                    shape.uuid = Some(self.parse_raw_kiid()?);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("start, mid, end, stroke, fill or uuid")),
            }
        }
        Self::fixup_sch_fill_mode(&mut shape.fill, &shape.stroke);
        self.need_right()?;
        Ok(shape)
    }

    fn parse_sch_circle(&mut self) -> Result<Shape, Error> {
        let _ = self.need_unquoted_symbol_atom("circle")?;
        let mut shape = Shape::new(ShapeKind::Circle);
        shape.points = vec![[0.0, 0.0]];
        shape.radius = Some(0.0);
        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("center, radius, stroke, fill or uuid")),
            };
            match head.as_str() {
                "center" => {
                    let _ = self.need_unquoted_symbol_atom("center")?;
                    shape.points[0] = self.parse_xy2("center")?;
                    self.need_right()?;
                }
                "radius" => {
                    let _ = self.need_unquoted_symbol_atom("radius")?;
                    shape.radius = Some(self.parse_f64_atom("radius length")?);
                    self.need_right()?;
                }
                "stroke" => {
                    let _ = self.need_unquoted_symbol_atom("stroke")?;
                    shape.has_stroke = true;
                    shape.stroke = Some(self.parse_stroke()?);
                }
                "fill" => {
                    let _ = self.need_unquoted_symbol_atom("fill")?;
                    shape.has_fill = true;
                    shape.fill = Some(self.parse_fill()?);
                }
                "uuid" => {
                    let _ = self.need_unquoted_symbol_atom("uuid")?;
                    shape.uuid = Some(self.parse_raw_kiid()?);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("center, radius, stroke, fill or uuid")),
            }
        }
        Self::fixup_sch_fill_mode(&mut shape.fill, &shape.stroke);
        self.need_right()?;
        Ok(shape)
    }

    fn parse_sch_rectangle(&mut self) -> Result<Shape, Error> {
        let _ = self.need_unquoted_symbol_atom("rectangle")?;
        let mut shape = Shape::new(ShapeKind::Rectangle);
        shape.points = vec![[0.0, 0.0], [0.0, 0.0]];
        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("start, end, stroke, fill or uuid")),
            };
            match head.as_str() {
                "start" => {
                    let _ = self.need_unquoted_symbol_atom("start")?;
                    shape.points[0] = self.parse_xy2("start")?;
                    self.need_right()?;
                }
                "end" => {
                    let _ = self.need_unquoted_symbol_atom("end")?;
                    shape.points[1] = self.parse_xy2("end")?;
                    self.need_right()?;
                }
                "radius" => {
                    let _ = self.need_unquoted_symbol_atom("radius")?;
                    shape.corner_radius = Some(self.parse_f64_atom("corner radius")?);
                    self.need_right()?;
                }
                "stroke" => {
                    let _ = self.need_unquoted_symbol_atom("stroke")?;
                    shape.has_stroke = true;
                    shape.stroke = Some(self.parse_stroke()?);
                }
                "fill" => {
                    let _ = self.need_unquoted_symbol_atom("fill")?;
                    shape.has_fill = true;
                    shape.fill = Some(self.parse_fill()?);
                }
                "uuid" => {
                    let _ = self.need_unquoted_symbol_atom("uuid")?;
                    shape.uuid = Some(self.parse_raw_kiid()?);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("start, end, stroke, fill or uuid")),
            }
        }
        Self::fixup_sch_fill_mode(&mut shape.fill, &shape.stroke);
        self.need_right()?;
        Ok(shape)
    }

    fn parse_sch_bezier(&mut self) -> Result<Shape, Error> {
        let _ = self.need_unquoted_symbol_atom("bezier")?;
        let mut shape = Shape::new(ShapeKind::Bezier);
        shape.points = vec![[0.0, 0.0], [0.0, 0.0], [0.0, 0.0], [0.0, 0.0]];
        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("pts, stroke, fill or uuid")),
            };
            match head.as_str() {
                "pts" => {
                    let _ = self.need_unquoted_symbol_atom("pts")?;
                    let mut ii = 0;
                    while !self.at_right() {
                        self.need_left()?;
                        let head = match &self.current().kind {
                            TokKind::Atom(value)
                                if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                            {
                                value.clone()
                            }
                            _ => return Err(self.expecting("xy")),
                        };
                        if head != "xy" {
                            return Err(self.expecting("xy"));
                        }
                        let _ = self.need_unquoted_symbol_atom("xy")?;
                        match ii {
                            0..=3 => shape.points[ii] = self.parse_xy2("xy")?,
                            _ => return Err(self.unexpected("control point")),
                        }
                        ii += 1;
                        self.need_right()?;
                    }
                    self.need_right()?;
                }
                "stroke" => {
                    let _ = self.need_unquoted_symbol_atom("stroke")?;
                    shape.has_stroke = true;
                    shape.stroke = Some(self.parse_stroke()?);
                }
                "fill" => {
                    let _ = self.need_unquoted_symbol_atom("fill")?;
                    shape.has_fill = true;
                    shape.fill = Some(self.parse_fill()?);
                }
                "uuid" => {
                    let _ = self.need_unquoted_symbol_atom("uuid")?;
                    shape.uuid = Some(self.parse_raw_kiid()?);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("pts, stroke, fill or uuid")),
            }
        }
        Self::fixup_sch_fill_mode(&mut shape.fill, &shape.stroke);
        self.need_right()?;
        Ok(shape)
    }

    fn parse_sch_rule_area(&mut self) -> Result<Shape, Error> {
        let _ = self.need_unquoted_symbol_atom("rule_area")?;
        let version = self.require_known_version()?;
        if version < VERSION_RULE_AREAS {
            return Err(self.error_here(format!(
                "rule_area requires schematic version {VERSION_RULE_AREAS} or newer"
            )));
        }
        let mut shape = Shape::new(ShapeKind::RuleArea);
        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => {
                    return Err(
                        self.expecting("exclude_from_sim, on_board, in_bom, dnp, or polyline")
                    );
                }
            };
            match head.as_str() {
                "polyline" => {
                    let polyline = self.parse_sch_polyline()?;
                    shape.points = polyline.points;
                    shape.has_stroke = polyline.has_stroke;
                    shape.has_fill = polyline.has_fill;
                    shape.stroke = polyline.stroke;
                    shape.fill = polyline.fill;
                    shape.uuid = polyline.uuid;
                }
                "exclude_from_sim" => {
                    let _ = self.need_unquoted_symbol_atom("exclude_from_sim")?;
                    shape.excluded_from_sim = self.parse_bool_atom("exclude_from_sim")?;
                    self.need_right()?;
                }
                "in_bom" => {
                    let _ = self.need_unquoted_symbol_atom("in_bom")?;
                    shape.in_bom = self.parse_bool_atom("in_bom")?;
                    self.need_right()?;
                }
                "on_board" => {
                    let _ = self.need_unquoted_symbol_atom("on_board")?;
                    shape.on_board = self.parse_bool_atom("on_board")?;
                    self.need_right()?;
                }
                "dnp" => {
                    let _ = self.need_unquoted_symbol_atom("dnp")?;
                    shape.dnp = self.parse_bool_atom("dnp")?;
                    self.need_right()?;
                }
                _ => {
                    return Err(
                        self.expecting("exclude_from_sim, on_board, in_bom, dnp, or polyline")
                    );
                }
            }
        }
        self.need_right()?;
        Ok(shape)
    }

    fn parse_schematic_symbol(&mut self) -> Result<Symbol, Error> {
        let _ = self.need_unquoted_symbol_atom("symbol")?;
        let mut symbol = Symbol::new();
        let mut lib_name = None;
        symbol.fields_autoplaced = FieldAutoplacement::None;

        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => {
                    return Err(self.expecting(
                        "lib_id, lib_name, at, mirror, uuid, exclude_from_sim, on_board, in_bom, dnp, default_instance, property, pin, or instances",
                    ))
                }
            };
            match head.as_str() {
                "lib_name" => {
                    let _ = self.need_unquoted_symbol_atom("lib_name")?;
                    lib_name = Some(
                        self.need_symbol_atom("lib_name")
                            .map_err(|_| self.error_here("Invalid symbol library name"))?
                            .replace("{slash}", "/"),
                    );
                    self.need_right()?;
                }
                "lib_id" => {
                    let _ = self.need_unquoted_symbol_atom("lib_id")?;
                    let raw = self.need_symbol_or_number_atom("symbol|number")?;
                    let normalized = raw.replace("{slash}", "/");

                    if let Some(ch) = Self::find_invalid_lib_id_char(&normalized) {
                        return Err(self.error_here(format!(
                            "Symbol {normalized} contains invalid character '{ch}'"
                        )));
                    }

                    if normalized.is_empty() {
                        return Err(self.error_here("Invalid symbol library ID"));
                    }

                    symbol.lib_id = normalized;
                    self.need_right()?;
                }
                "at" => {
                    let _ = self.need_unquoted_symbol_atom("at")?;
                    let parsed = self.parse_xy3("symbol at")?;
                    match parsed[2] as i32 {
                        0 | 90 | 180 | 270 => {
                            symbol.at = [parsed[0], parsed[1]];
                            symbol.angle = parsed[2];
                        }
                        _ => return Err(self.expecting("0, 90, 180, or 270")),
                    }
                    self.need_right()?;
                }
                "mirror" => {
                    let _ = self.need_unquoted_symbol_atom("mirror")?;
                    let axis = self.need_unquoted_symbol_atom("mirror axis")?;
                    symbol.mirror = Some(match axis.as_str() {
                        "x" => MirrorAxis::X,
                        "y" => MirrorAxis::Y,
                        _ => return Err(self.expecting("x or y")),
                    });
                    self.need_right()?;
                }
                "convert" | "body_style" => {
                    let _ = self.need_unquoted_symbol_atom(&head)?;
                    symbol.body_style = Some(self.parse_i32_atom("symbol body style")?);
                    self.need_right()?;
                }
                "unit" => {
                    let _ = self.need_unquoted_symbol_atom("unit")?;
                    symbol.unit = Some(self.parse_i32_atom("unit")?);
                    self.need_right()?;
                }
                "exclude_from_sim" => {
                    let _ = self.need_unquoted_symbol_atom("exclude_from_sim")?;
                    symbol.excluded_from_sim = self.parse_bool_atom("exclude_from_sim")?;
                    self.need_right()?;
                }
                "in_bom" => {
                    let _ = self.need_unquoted_symbol_atom("in_bom")?;
                    symbol.in_bom = self.parse_bool_atom("in_bom")?;
                    self.need_right()?;
                }
                "on_board" => {
                    let _ = self.need_unquoted_symbol_atom("on_board")?;
                    symbol.on_board = self.parse_bool_atom("on_board")?;
                    self.need_right()?;
                }
                "in_pos_files" => {
                    let _ = self.need_unquoted_symbol_atom("in_pos_files")?;
                    symbol.in_pos_files = self.parse_bool_atom("in_pos_files")?;
                    self.need_right()?;
                }
                "dnp" => {
                    let _ = self.need_unquoted_symbol_atom("dnp")?;
                    symbol.dnp = self.parse_bool_atom("dnp")?;
                    self.need_right()?;
                }
                "fields_autoplaced" => {
                    let _ = self.need_unquoted_symbol_atom("fields_autoplaced")?;
                    if self.parse_maybe_absent_bool(true)? {
                        symbol.fields_autoplaced = FieldAutoplacement::Auto;
                    }
                }
                "uuid" => {
                    let _ = self.need_unquoted_symbol_atom("uuid")?;
                    symbol.uuid = Some(self.parse_kiid()?);
                    self.need_right()?;
                }
                "default_instance" => {
                    let _ = self.need_unquoted_symbol_atom("default_instance")?;
                    while !self.at_right() {
                        self.need_left()?;
                        match self
                            .need_unquoted_symbol_atom("reference, unit, value or footprint")?
                            .as_str()
                        {
                            "reference" => {
                                let _ = self.need_symbol_atom("reference")?;
                                self.need_right()?;
                            }
                            "unit" => {
                                let _ = self.parse_i32_atom("symbol unit")?;
                                self.need_right()?;
                            }
                            "value" => {
                                let parsed = {
                                    let value = self.need_symbol_atom("value")?;
                                    if self
                                        .require_known_version()
                                        .unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION)
                                        < VERSION_EMPTY_TILDE_IS_EMPTY
                                        && value == "~"
                                    {
                                        String::new()
                                    } else {
                                        value
                                    }
                                };
                                symbol.set_field_text(PropertyKind::SymbolValue, parsed);
                                self.need_right()?;
                            }
                            "footprint" => {
                                let parsed = {
                                    let value = self.need_symbol_atom("footprint")?;
                                    if self
                                        .require_known_version()
                                        .unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION)
                                        < VERSION_EMPTY_TILDE_IS_EMPTY
                                        && value == "~"
                                    {
                                        String::new()
                                    } else {
                                        value
                                    }
                                };
                                symbol.set_field_text(PropertyKind::SymbolFootprint, parsed);
                                self.need_right()?;
                            }
                            _ => {
                                return Err(self.expecting("reference, unit, value or footprint"));
                            }
                        }
                    }
                    self.need_right()?;
                }
                "instances" => {
                    let _ = self.need_unquoted_symbol_atom("instances")?;
                    while !self.at_right() {
                        self.need_left()?;
                        let head = match &self.current().kind {
                            TokKind::Atom(value)
                                if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                            {
                                value.clone()
                            }
                            _ => return Err(self.expecting("project")),
                        };
                        if head != "project" {
                            return Err(self.expecting("project"));
                        }
                        let _ = self.need_unquoted_symbol_atom("project")?;
                        let project = self.need_symbol_atom("project name")?;
                        while !self.at_right() {
                            self.need_left()?;
                            let head = match &self.current().kind {
                                TokKind::Atom(value)
                                    if matches!(
                                        self.current().atom_class,
                                        Some(AtomClass::Symbol)
                                    ) =>
                                {
                                    value.clone()
                                }
                                _ => return Err(self.expecting("path")),
                            };
                            if head != "path" {
                                return Err(self.expecting("path"));
                            }
                            let _ = self.need_unquoted_symbol_atom("path")?;
                            let path = self.need_symbol_atom("symbol instance path")?;
                            let mut instance = SymbolLocalInstance::new(project.clone(), path);
                            while !self.at_right() {
                                self.need_left()?;
                                let child = match &self.current().kind {
                                    TokKind::Atom(value)
                                        if matches!(
                                            self.current().atom_class,
                                            Some(AtomClass::Symbol)
                                        ) =>
                                    {
                                        value.clone()
                                    }
                                    _ => {
                                        return Err(self.expecting(
                                            "reference, unit, value, footprint, or variant",
                                        ));
                                    }
                                };
                                match child.as_str() {
                                    "reference" => {
                                        let _ = self.need_unquoted_symbol_atom("reference")?;
                                        instance.reference =
                                            Some(self.need_symbol_atom("reference")?);
                                        self.need_right()?;
                                    }
                                    "unit" => {
                                        let _ = self.need_unquoted_symbol_atom("unit")?;
                                        instance.unit = Some(self.parse_i32_atom("symbol unit")?);
                                        self.need_right()?;
                                    }
                                    "value" => {
                                        let _ = self.need_unquoted_symbol_atom("value")?;
                                        let parsed = {
                                            let value = self.need_symbol_atom("value")?;
                                            if self
                                                .require_known_version()
                                                .unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION)
                                                < VERSION_EMPTY_TILDE_IS_EMPTY
                                                && value == "~"
                                            {
                                                String::new()
                                            } else {
                                                value
                                            }
                                        };
                                        symbol.set_field_text(PropertyKind::SymbolValue, parsed);
                                        self.need_right()?;
                                    }
                                    "footprint" => {
                                        let _ = self.need_unquoted_symbol_atom("footprint")?;
                                        let parsed = {
                                            let value = self.need_symbol_atom("footprint")?;
                                            if self
                                                .require_known_version()
                                                .unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION)
                                                < VERSION_EMPTY_TILDE_IS_EMPTY
                                                && value == "~"
                                            {
                                                String::new()
                                            } else {
                                                value
                                            }
                                        };
                                        symbol
                                            .set_field_text(PropertyKind::SymbolFootprint, parsed);
                                        self.need_right()?;
                                    }
                                    "variant" => {
                                        let _ = self.need_unquoted_symbol_atom("variant")?;
                                        let mut variant = ItemVariant::new(
                                            symbol.dnp,
                                            symbol.excluded_from_sim,
                                            symbol.in_bom,
                                            symbol.on_board,
                                            symbol.in_pos_files,
                                        );

                                        while !self.at_right() {
                                            self.need_left()?;
                                            let variant_head = match &self.current().kind {
                                                TokKind::Atom(value)
                                                    if matches!(
                                                        self.current().atom_class,
                                                        Some(AtomClass::Symbol)
                                                    ) =>
                                                {
                                                    value.clone()
                                                }
                                                _ => {
                                                    return Err(self.expecting(
                                                        "dnp, exclude_from_sim, field, in_bom, in_pos_files, name, or on_board",
                                                    ))
                                                }
                                            };
                                            match variant_head.as_str() {
                                                "name" => {
                                                    let _ =
                                                        self.need_unquoted_symbol_atom("name")?;
                                                    variant.name = self
                                                        .need_symbol_atom("name")
                                                        .map_err(|_| {
                                                            self.error_here("Invalid variant name")
                                                        })?;
                                                    self.need_right()?;
                                                }
                                                "dnp" => {
                                                    let _ =
                                                        self.need_unquoted_symbol_atom("dnp")?;
                                                    variant.dnp = self.parse_bool_atom("dnp")?;
                                                    self.need_right()?;
                                                }
                                                "exclude_from_sim" => {
                                                    let _ = self.need_unquoted_symbol_atom(
                                                        "exclude_from_sim",
                                                    )?;
                                                    variant.excluded_from_sim =
                                                        self.parse_bool_atom("exclude_from_sim")?;
                                                    self.need_right()?;
                                                }
                                                "in_bom" => {
                                                    let _ =
                                                        self.need_unquoted_symbol_atom("in_bom")?;
                                                    variant.in_bom =
                                                        self.parse_bool_atom("in_bom")?;
                                                    if self.require_known_version()?
                                                        < VERSION_VARIANT_IN_BOM_FIX
                                                    {
                                                        variant.in_bom = !variant.in_bom;
                                                    }
                                                    self.need_right()?;
                                                }
                                                "on_board" => {
                                                    let _ =
                                                        self.need_unquoted_symbol_atom("on_board")?;
                                                    variant.on_board =
                                                        self.parse_bool_atom("on_board")?;
                                                    self.need_right()?;
                                                }
                                                "in_pos_files" => {
                                                    let _ = self.need_unquoted_symbol_atom(
                                                        "in_pos_files",
                                                    )?;
                                                    variant.in_pos_files =
                                                        self.parse_bool_atom("in_pos_files")?;
                                                    self.need_right()?;
                                                }
                                                "field" => {
                                                    let _ =
                                                        self.need_unquoted_symbol_atom("field")?;
                                                    let mut field_name = String::new();
                                                    let mut field_value = String::new();

                                                    while !self.at_right() {
                                                        self.need_left()?;
                                                        let field_head = match &self.current().kind
                                                        {
                                                            TokKind::Atom(value)
                                                                if matches!(
                                                                    self.current().atom_class,
                                                                    Some(AtomClass::Symbol)
                                                                ) =>
                                                            {
                                                                value.clone()
                                                            }
                                                            _ => {
                                                                return Err(
                                                                    self.expecting("name or value")
                                                                );
                                                            }
                                                        };
                                                        match field_head.as_str() {
                                                            "name" => {
                                                                let _ = self
                                                                    .need_unquoted_symbol_atom(
                                                                        "name",
                                                                    )?;
                                                                field_name = self
                                                                    .need_symbol_atom("name")
                                                                    .map_err(|_| {
                                                                        self.error_here(
                                                                            "Invalid variant field name",
                                                                        )
                                                                    })?;
                                                                self.need_right()?;
                                                            }
                                                            "value" => {
                                                                let _ = self
                                                                    .need_unquoted_symbol_atom(
                                                                        "value",
                                                                    )?;
                                                                field_value = self
                                                                    .need_symbol_atom("value")
                                                                    .map_err(|_| {
                                                                        self.error_here(
                                                                            "Invalid variant field value",
                                                                        )
                                                                    })?;
                                                                self.need_right()?;
                                                            }
                                                            _ => {
                                                                return Err(
                                                                    self.expecting("name or value")
                                                                );
                                                            }
                                                        }
                                                    }

                                                    variant.fields.insert(field_name, field_value);
                                                    self.need_right()?;
                                                }
                                                _ => {
                                                    return Err(self.expecting(
                                                        "dnp, exclude_from_sim, field, in_bom, in_pos_files, name, or on_board",
                                                    ));
                                                }
                                            }
                                            instance
                                                .variants
                                                .insert(variant.name.clone(), variant.clone());
                                        }
                                        self.need_right()?;
                                    }
                                    _ => {
                                        return Err(self.expecting(
                                            "reference, unit, value, footprint, or variant",
                                        ));
                                    }
                                }
                            }
                            self.need_right()?;
                            symbol.add_hierarchical_reference(instance);
                        }
                        self.need_right()?;
                    }
                    self.need_right()?;
                }
                "property" => {
                    let property = self.parse_sch_field(FieldParent::Symbol(&symbol))?;
                    if property.key == SIM_LEGACY_ENABLE_FIELD_V7 {
                        symbol.excluded_from_sim = property.value == "0";
                        continue;
                    }
                    if property.key == SIM_LEGACY_ENABLE_FIELD {
                        symbol.excluded_from_sim = property.value == "N";
                        continue;
                    }

                    if matches!(
                        property.kind,
                        PropertyKind::SymbolReference
                            | PropertyKind::SymbolValue
                            | PropertyKind::SymbolFootprint
                            | PropertyKind::SymbolDatasheet
                            | PropertyKind::SymbolDescription
                    ) {
                        let existing = symbol
                            .properties
                            .iter_mut()
                            .find(|existing| existing.kind == property.kind)
                            .expect("placed symbols start with mandatory fields");
                        let kind = property.kind;
                        *existing = property;
                        if kind == PropertyKind::SymbolReference {
                            symbol.update_prefix_from_reference();
                        }
                    } else if let Some(existing) = symbol
                        .properties
                        .iter_mut()
                        .find(|existing| existing.key == property.key)
                    {
                        *existing = property;
                    } else {
                        symbol.properties.push(property);
                    }
                }
                "pin" => {
                    let _ = self.need_unquoted_symbol_atom("pin")?;
                    let mut pin = SymbolPin::new(self.need_symbol_atom("pin number")?);
                    while !self.at_right() {
                        self.need_left()?;
                        match self
                            .need_unquoted_symbol_atom("alternate or uuid")?
                            .as_str()
                        {
                            "alternate" => {
                                pin.alternate = Some(self.need_symbol_atom("alternate")?);
                                self.need_right()?;
                            }
                            "uuid" => {
                                if self.require_known_version()? >= VERSION_SYMBOL_PIN_UUID {
                                    pin.uuid = Some(self.parse_kiid()?);
                                } else {
                                    let _ = self.need_symbol_atom("uuid")?;
                                }
                                self.need_right()?;
                            }
                            _ => return Err(self.expecting("alternate or uuid")),
                        }
                    }
                    self.need_right()?;
                    symbol.add_pin(pin);
                }
                _ => {
                    return Err(self.expecting(
                        "lib_id, lib_name, at, mirror, uuid, exclude_from_sim, on_board, in_bom, dnp, default_instance, property, pin, or instances",
                    ));
                }
            }
        }

        symbol.lib_name = lib_name.filter(|name| name != &symbol.lib_id);
        self.need_right()?;
        Ok(symbol)
    }

    fn parse_sch_sheet(&mut self) -> Result<Sheet, Error> {
        let _ = self.need_unquoted_symbol_atom("sheet")?;
        let mut sheet = Sheet::new();
        sheet.fields_autoplaced = FieldAutoplacement::None;
        let mut properties: Vec<Property> = Vec::new();
        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => {
                    return Err(self.expecting(
                        "at, size, stroke, background, instances, uuid, property, or pin",
                    ));
                }
            };
            match head.as_str() {
                "at" => {
                    let _ = self.need_unquoted_symbol_atom("at")?;
                    sheet.at = self.parse_xy2("sheet at")?;
                    self.need_right()?;
                }
                "size" => {
                    let _ = self.need_unquoted_symbol_atom("size")?;
                    sheet.size = self.parse_xy2("sheet size")?;
                    self.need_right()?;
                }
                "exclude_from_sim" => {
                    let _ = self.need_unquoted_symbol_atom("exclude_from_sim")?;
                    sheet.excluded_from_sim = self.parse_bool_atom("exclude_from_sim")?;
                    self.need_right()?;
                }
                "in_bom" => {
                    let _ = self.need_unquoted_symbol_atom("in_bom")?;
                    sheet.in_bom = self.parse_bool_atom("in_bom")?;
                    self.need_right()?;
                }
                "on_board" => {
                    let _ = self.need_unquoted_symbol_atom("on_board")?;
                    sheet.on_board = self.parse_bool_atom("on_board")?;
                    self.need_right()?;
                }
                "dnp" => {
                    let _ = self.need_unquoted_symbol_atom("dnp")?;
                    sheet.dnp = self.parse_bool_atom("dnp")?;
                    self.need_right()?;
                }
                "fields_autoplaced" => {
                    let _ = self.need_unquoted_symbol_atom("fields_autoplaced")?;
                    if self.parse_maybe_absent_bool(true)? {
                        sheet.fields_autoplaced = FieldAutoplacement::Auto;
                    }
                }
                "stroke" => {
                    let _ = self.need_unquoted_symbol_atom("stroke")?;
                    let stroke = self.parse_stroke()?;
                    sheet.border_width = stroke.width.unwrap_or(0.0);
                    sheet.border_color = stroke.color;
                }
                "fill" => {
                    let _ = self.need_unquoted_symbol_atom("fill")?;
                    let fill = self.parse_fill()?;
                    sheet.background_color = fill.color;
                }
                "uuid" => {
                    let _ = self.need_unquoted_symbol_atom("uuid")?;
                    sheet.uuid = Some(self.parse_kiid()?);
                    self.need_right()?;
                }
                "property" => {
                    let mut property = self.parse_sch_field(FieldParent::Sheet(&sheet))?;
                    if self
                        .require_known_version()
                        .unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION)
                        <= VERSION_WRONG_SHEET_FIELD_IDS
                    {
                        if properties.is_empty() {
                            property.key = "Sheetname".to_string();
                            property.kind = PropertyKind::SheetName;
                            property.id = PropertyKind::SheetName.default_field_id();
                        } else if properties.len() == 1 {
                            property.key = "Sheetfile".to_string();
                            property.kind = PropertyKind::SheetFile;
                            property.id = PropertyKind::SheetFile.default_field_id();
                        }
                    }
                    if matches!(property.kind, PropertyKind::SheetUser) {
                        property.ordinal = properties.iter().fold(42, |ordinal, existing| {
                            ordinal.max(existing.sort_ordinal() + 1)
                        });
                    }
                    properties.push(property);
                }
                "pin" => {
                    sheet.add_pin(self.parse_sch_sheet_pin(&sheet)?);
                }
                "instances" => {
                    let _ = self.need_unquoted_symbol_atom("instances")?;
                    let mut instances: Vec<SheetLocalInstance> = Vec::new();
                    while !self.at_right() {
                        self.need_left()?;
                        let head = match &self.current().kind {
                            TokKind::Atom(value)
                                if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                            {
                                value.clone()
                            }
                            _ => return Err(self.expecting("project")),
                        };
                        if head != "project" {
                            return Err(self.expecting("project"));
                        }
                        let _ = self.need_unquoted_symbol_atom("project")?;
                        let project = self.need_symbol_atom("project name")?;
                        while !self.at_right() {
                            self.need_left()?;
                            let head = match &self.current().kind {
                                TokKind::Atom(value)
                                    if matches!(
                                        self.current().atom_class,
                                        Some(AtomClass::Symbol)
                                    ) =>
                                {
                                    value.clone()
                                }
                                _ => return Err(self.expecting("path")),
                            };
                            if head != "path" {
                                return Err(self.expecting("path"));
                            }
                            let _ = self.need_unquoted_symbol_atom("path")?;
                            let path = self.need_symbol_atom("sheet instance path")?;
                            let mut instance = SheetLocalInstance::new(project.clone(), path);
                            while !self.at_right() {
                                self.need_left()?;
                                let child = match &self.current().kind {
                                    TokKind::Atom(value)
                                        if matches!(
                                            self.current().atom_class,
                                            Some(AtomClass::Symbol)
                                        ) =>
                                    {
                                        value.clone()
                                    }
                                    _ => return Err(self.expecting("page or variant")),
                                };
                                match child.as_str() {
                                    "page" => {
                                        let _ = self.need_unquoted_symbol_atom("page")?;
                                        let mut parsed_page = self.need_symbol_atom("page")?;

                                        if parsed_page.is_empty() {
                                            parsed_page = "#".to_string();
                                        } else {
                                            parsed_page.retain(|ch| {
                                                !matches!(ch, '\r' | '\n' | '\t' | ' ')
                                            });
                                        }

                                        instance.page = Some(parsed_page);
                                        self.need_right()?;
                                    }
                                    "variant" => {
                                        let _ = self.need_unquoted_symbol_atom("variant")?;
                                        let mut variant = ItemVariant::new(
                                            sheet.dnp,
                                            sheet.excluded_from_sim,
                                            sheet.in_bom,
                                            sheet.on_board,
                                            false,
                                        );

                                        while !self.at_right() {
                                            self.need_left()?;
                                            let variant_head = match &self.current().kind {
                                                TokKind::Atom(value)
                                                    if matches!(
                                                        self.current().atom_class,
                                                        Some(AtomClass::Symbol)
                                                    ) =>
                                                {
                                                    value.clone()
                                                }
                                                _ => {
                                                    return Err(self.expecting(
                                                        "dnp, exclude_from_sim, field, in_bom, in_pos_files, name, or on_board",
                                                    ))
                                                }
                                            };
                                            match variant_head.as_str() {
                                                "name" => {
                                                    let _ =
                                                        self.need_unquoted_symbol_atom("name")?;
                                                    variant.name = self
                                                        .need_symbol_atom("name")
                                                        .map_err(|_| {
                                                            self.error_here("Invalid variant name")
                                                        })?;
                                                    self.need_right()?;
                                                }
                                                "dnp" => {
                                                    let _ =
                                                        self.need_unquoted_symbol_atom("dnp")?;
                                                    variant.dnp = self.parse_bool_atom("dnp")?;
                                                    self.need_right()?;
                                                }
                                                "exclude_from_sim" => {
                                                    let _ = self.need_unquoted_symbol_atom(
                                                        "exclude_from_sim",
                                                    )?;
                                                    variant.excluded_from_sim =
                                                        self.parse_bool_atom("exclude_from_sim")?;
                                                    self.need_right()?;
                                                }
                                                "in_bom" => {
                                                    let _ =
                                                        self.need_unquoted_symbol_atom("in_bom")?;
                                                    variant.in_bom =
                                                        self.parse_bool_atom("in_bom")?;
                                                    if self.require_known_version()?
                                                        < VERSION_VARIANT_IN_BOM_FIX
                                                    {
                                                        variant.in_bom = !variant.in_bom;
                                                    }
                                                    self.need_right()?;
                                                }
                                                "on_board" => {
                                                    let _ =
                                                        self.need_unquoted_symbol_atom("on_board")?;
                                                    variant.on_board =
                                                        self.parse_bool_atom("on_board")?;
                                                    self.need_right()?;
                                                }
                                                "in_pos_files" => {
                                                    let _ = self.need_unquoted_symbol_atom(
                                                        "in_pos_files",
                                                    )?;
                                                    variant.in_pos_files =
                                                        self.parse_bool_atom("in_pos_files")?;
                                                    self.need_right()?;
                                                }
                                                "field" => {
                                                    let _ =
                                                        self.need_unquoted_symbol_atom("field")?;
                                                    let mut field_name = String::new();
                                                    let mut field_value = String::new();

                                                    while !self.at_right() {
                                                        self.need_left()?;
                                                        let field_head = match &self.current().kind
                                                        {
                                                            TokKind::Atom(value)
                                                                if matches!(
                                                                    self.current().atom_class,
                                                                    Some(AtomClass::Symbol)
                                                                ) =>
                                                            {
                                                                value.clone()
                                                            }
                                                            _ => {
                                                                return Err(
                                                                    self.expecting("name or value")
                                                                );
                                                            }
                                                        };
                                                        match field_head.as_str() {
                                                            "name" => {
                                                                let _ = self
                                                                    .need_unquoted_symbol_atom(
                                                                        "name",
                                                                    )?;
                                                                field_name = self
                                                                    .need_symbol_atom("name")
                                                                    .map_err(|_| {
                                                                        self.error_here(
                                                                            "Invalid variant field name",
                                                                        )
                                                                    })?;
                                                                self.need_right()?;
                                                            }
                                                            "value" => {
                                                                let _ = self
                                                                    .need_unquoted_symbol_atom(
                                                                        "value",
                                                                    )?;
                                                                field_value = self
                                                                    .need_symbol_atom("value")
                                                                    .map_err(|_| {
                                                                        self.error_here(
                                                                            "Invalid variant field value",
                                                                        )
                                                                    })?;
                                                                self.need_right()?;
                                                            }
                                                            _ => {
                                                                return Err(
                                                                    self.expecting("name or value")
                                                                );
                                                            }
                                                        }
                                                    }

                                                    variant.fields.insert(field_name, field_value);
                                                    self.need_right()?;
                                                }
                                                _ => {
                                                    return Err(self.expecting(
                                                        "dnp, exclude_from_sim, field, in_bom, in_pos_files, name, or on_board",
                                                    ));
                                                }
                                            }
                                            instance
                                                .variants
                                                .insert(variant.name.clone(), variant.clone());
                                        }
                                        self.need_right()?;
                                    }
                                    _ => return Err(self.expecting("page or variant")),
                                }
                            }
                            self.need_right()?;
                            instances.push(instance);
                        }
                        self.need_right()?;
                    }
                    sheet.set_instances(instances);
                    self.need_right()?;
                }
                _ => {
                    return Err(self.expecting(
                        "at, size, stroke, background, instances, uuid, property, or pin",
                    ));
                }
            }
        }

        sheet.set_properties(properties);

        if sheet.name().is_none() {
            return Err(self.error_here("Missing sheet name property"));
        }
        if sheet.filename().is_none() {
            return Err(self.error_here("Missing sheet file property"));
        }

        self.need_right()?;
        Ok(sheet)
    }

    fn parse_sch_sheet_pin(&mut self, sheet: &Sheet) -> Result<SheetPin, Error> {
        let _ = self.need_unquoted_symbol_atom("pin")?;
        let name = self
            .need_symbol_atom("sheet pin name")
            .map_err(|_| self.error_here("Invalid sheet pin name"))?;
        if name.is_empty() {
            return Err(self.error_here("Empty sheet pin name"));
        }
        let mut sheet_pin = SheetPin::new(name, sheet);

        match self.need_unquoted_symbol_atom("sheet pin shape")?.as_str() {
            "input" => sheet_pin.shape = SheetPinShape::Input,
            "output" => sheet_pin.shape = SheetPinShape::Output,
            "bidirectional" => sheet_pin.shape = SheetPinShape::Bidirectional,
            "tri_state" => sheet_pin.shape = SheetPinShape::TriState,
            "passive" => sheet_pin.shape = SheetPinShape::Passive,
            _ => {
                return Err(self.expecting("input, output, bidirectional, tri_state, or passive"));
            }
        }

        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("at, uuid or effects")),
            };
            match head.as_str() {
                "at" => {
                    let _ = self.need_unquoted_symbol_atom("at")?;
                    let parsed = self.parse_xy3("sheet pin at")?;
                    let parsed_side = match parsed[2] as i32 {
                        0 => SheetSide::Right,
                        90 => SheetSide::Top,
                        180 => SheetSide::Left,
                        270 => SheetSide::Bottom,
                        _ => return Err(self.expecting("0, 90, 180, or 270")),
                    };
                    sheet_pin.at = [parsed[0], parsed[1]];
                    sheet_pin.side = parsed_side;
                    self.need_right()?;
                }
                "uuid" => {
                    let _ = self.need_unquoted_symbol_atom("uuid")?;
                    sheet_pin.uuid = Some(self.parse_kiid()?);
                    self.need_right()?;
                }
                "effects" => {
                    self.parse_eda_text(ParsedEdaTextOwner::sheet_pin(&mut sheet_pin), true, true)?;
                }
                _ => return Err(self.expecting("at, uuid or effects")),
            }
        }

        self.need_right()?;
        Ok(sheet_pin)
    }

    fn parse_sch_sheet_instances(&mut self) -> Result<(), Error> {
        let _ = self.need_unquoted_symbol_atom("sheet_instances")?;
        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("path")),
            };
            if head != "path" {
                return Err(self.expecting("path"));
            }
            let _ = self.need_unquoted_symbol_atom("path")?;
            let raw_path = self.need_symbol_atom("sheet instance path")?;
            let mut instance = SheetInstance::new(raw_path);

            if self.require_known_version()? < VERSION_SHEET_INSTANCE_ROOT_PATH {
                if let Some(root_uuid) = self.root_uuid.as_ref() {
                    let prefix = format!("/{root_uuid}");

                    instance.path = if instance.path.is_empty() {
                        prefix
                    } else if instance.path == prefix
                        || instance.path.starts_with(&(prefix.clone() + "/"))
                    {
                        instance.path
                    } else if instance.path.starts_with('/') {
                        format!("{prefix}{}", instance.path)
                    } else {
                        format!("{prefix}/{}", instance.path)
                    };
                }
            }

            while !self.at_right() {
                self.need_left()?;
                let child = match &self.current().kind {
                    TokKind::Atom(value)
                        if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                    {
                        value.clone()
                    }
                    _ => return Err(self.expecting("path or page")),
                };
                match child.as_str() {
                    "page" => {
                        let _ = self.need_unquoted_symbol_atom("page")?;
                        let raw_page = self.need_symbol_atom("page")?;
                        let mut parsed_page = raw_page.clone();
                        let mut replacements = 0usize;

                        if parsed_page.is_empty() {
                            parsed_page = "#".to_string();
                            replacements += 1;
                        } else {
                            let original_len = parsed_page.chars().count();
                            parsed_page.retain(|ch| !matches!(ch, '\r' | '\n' | '\t' | ' '));
                            replacements +=
                                original_len.saturating_sub(parsed_page.chars().count());
                        }

                        if replacements > 0 {
                            self.screen.content_modified = true;
                        }

                        instance.page = Some(parsed_page);
                    }
                    _ => return Err(self.expecting("path or page")),
                }
                self.need_right()?;
            }
            self.need_right()?;
            if self.require_known_version()? >= VERSION_SKIP_EMPTY_ROOT_SHEET_INSTANCE_PATH
                && instance.path.is_empty()
            {
                self.screen.root_sheet_page = instance.page;
            } else {
                self.screen.add_sheet_instance(instance);
            }
        }
        self.need_right()?;
        Ok(())
    }

    fn parse_sch_symbol_instances(&mut self) -> Result<(), Error> {
        let _ = self.need_unquoted_symbol_atom("symbol_instances")?;
        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("path")),
            };
            if head != "path" {
                return Err(self.expecting("path"));
            }
            let _ = self.need_unquoted_symbol_atom("path")?;
            let raw_path = self.need_symbol_atom("symbol instance path")?;
            let path = if let Some(root_uuid) = self.root_uuid.as_ref() {
                let prefix = format!("/{root_uuid}");

                if raw_path.is_empty() {
                    prefix
                } else if raw_path == prefix || raw_path.starts_with(&(prefix.clone() + "/")) {
                    raw_path
                } else if raw_path.starts_with('/') {
                    format!("{prefix}{raw_path}")
                } else {
                    format!("{prefix}/{raw_path}")
                }
            } else {
                raw_path
            };
            let mut instance = SymbolInstance::new(path);
            while !self.at_right() {
                self.need_left()?;
                let child = match &self.current().kind {
                    TokKind::Atom(value)
                        if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                    {
                        value.clone()
                    }
                    _ => return Err(self.expecting("path, unit, value or footprint")),
                };
                match child.as_str() {
                    "reference" => {
                        let _ = self.need_unquoted_symbol_atom("reference")?;
                        instance.reference = Some(self.need_symbol_atom("reference")?)
                    }
                    "unit" => {
                        let _ = self.need_unquoted_symbol_atom("unit")?;
                        instance.unit = Some(self.parse_i32_atom("unit")?)
                    }
                    "value" => {
                        let _ = self.need_unquoted_symbol_atom("value")?;
                        instance.value = Some({
                            let value = self.need_symbol_atom("value")?;
                            if self
                                .require_known_version()
                                .unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION)
                                < VERSION_EMPTY_TILDE_IS_EMPTY
                                && value == "~"
                            {
                                String::new()
                            } else {
                                value
                            }
                        })
                    }
                    "footprint" => {
                        let _ = self.need_unquoted_symbol_atom("footprint")?;
                        instance.footprint = Some({
                            let value = self.need_symbol_atom("footprint")?;
                            if self
                                .require_known_version()
                                .unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION)
                                < VERSION_EMPTY_TILDE_IS_EMPTY
                                && value == "~"
                            {
                                String::new()
                            } else {
                                value
                            }
                        })
                    }
                    _ => return Err(self.expecting("path, unit, value or footprint")),
                }
                self.need_right()?;
            }
            self.need_right()?;
            self.screen.add_symbol_instance(instance);
        }
        self.need_right()?;
        Ok(())
    }

    fn parse_group(&mut self) -> Result<(), Error> {
        let _ = self.need_unquoted_symbol_atom("group")?;
        let mut group = PendingGroupInfo {
            name: None,
            uuid: None,
            lib_id: None,
            member_uuids: Vec::new(),
        };

        while !matches!(self.current().kind, TokKind::Left) {
            if self.at_unquoted_symbol_with("locked") {
                let _ = self.need_unquoted_symbol_atom("locked")?;
                continue;
            }

            group.name = Some(self.need_quoted_atom("group name or locked")?);
        }

        while !self.at_right() {
            self.need_left()?;
            match self
                .need_unquoted_symbol_atom("uuid, lib_id, members")?
                .as_str()
            {
                "uuid" => {
                    group.uuid = Some(self.parse_kiid()?);
                    self.need_right()?;
                }
                "lib_id" => {
                    let raw = self.need_symbol_or_number_atom("symbol|number")?;
                    let normalized = raw.replace("{slash}", "/");

                    if let Some(ch) = Self::find_invalid_lib_id_char(&normalized) {
                        return Err(self.error_here(format!(
                            "Group library link {normalized} contains invalid character '{ch}'"
                        )));
                    }

                    if normalized.is_empty() {
                        return Err(self.error_here("Invalid library ID"));
                    }

                    group.lib_id = Some(normalized);
                    self.need_right()?;
                }
                "members" => {
                    self.parse_group_members(&mut group)?;
                }
                _ => return Err(self.expecting("uuid, lib_id, members")),
            }
        }

        self.need_right()?;
        self.pending_groups.push(group);
        Ok(())
    }

    fn parse_group_members(&mut self, group: &mut PendingGroupInfo) -> Result<(), Error> {
        while !self.at_right() {
            let raw = match &self.current().kind {
                TokKind::Atom(value) => value.clone(),
                _ => return Err(self.expecting("group member uuid")),
            };
            self.idx += 1;
            group.member_uuids.push(self.normalize_kiid(raw, false));
        }

        self.need_right()?;
        Ok(())
    }

    fn parse_sch_field(&mut self, parent: FieldParent<'_>) -> Result<Property, Error> {
        let _ = self.need_unquoted_symbol_atom("property")?;
        let mut is_private = false;

        if self.at_unquoted_symbol_with("private") {
            let _ = self.need_unquoted_symbol_atom("private")?;
            is_private = true;
        }

        let name = self
            .need_symbol_atom("property name")
            .map_err(|_| self.error_here("Invalid property name"))?;

        if name.is_empty() {
            return Err(self.error_here("Empty property name"));
        }

        let value = self
            .need_symbol_atom("property value")
            .map_err(|_| self.error_here("Invalid property value"))?;

        let value = if self.version.unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION)
            < VERSION_EMPTY_TILDE_IS_EMPTY
            && value == "~"
        {
            String::new()
        } else {
            value
        };

        let field_id = match parent {
            FieldParent::Symbol(_) => {
                let mut field_id = PropertyKind::User;

                for kind in PropertyKind::SYMBOL_MANDATORY_FIELDS {
                    if name.eq_ignore_ascii_case(kind.canonical_key()) {
                        field_id = kind;
                        break;
                    }
                }

                field_id
            }
            FieldParent::Sheet(_) => {
                let mut field_id = PropertyKind::SheetUser;

                for kind in PropertyKind::SHEET_MANDATORY_FIELDS {
                    if name.eq_ignore_ascii_case(kind.canonical_key()) {
                        field_id = kind;
                        break;
                    }
                }

                if name.eq_ignore_ascii_case("Sheet name") {
                    field_id = PropertyKind::SheetName;
                } else if name.eq_ignore_ascii_case("Sheet file") {
                    field_id = PropertyKind::SheetFile;
                }

                field_id
            }
            FieldParent::Label(label) => {
                let mut field_id = PropertyKind::User;

                if matches!(label.kind, LabelKind::Global) {
                    for kind in PropertyKind::GLOBAL_LABEL_MANDATORY_FIELDS {
                        if name.eq_ignore_ascii_case(kind.canonical_key()) {
                            field_id = kind;
                            break;
                        }
                    }
                }

                field_id
            }
        };

        let mut property = Property::new_named(
            field_id,
            &name,
            String::new(),
            matches!(field_id, PropertyKind::User) && is_private,
        );

        property.ordinal = match parent {
            FieldParent::Symbol(symbol) if matches!(field_id, PropertyKind::User) => {
                symbol.next_field_ordinal()
            }
            FieldParent::Sheet(sheet) if matches!(field_id, PropertyKind::SheetUser) => {
                sheet.next_field_ordinal()
            }
            FieldParent::Label(label) if matches!(field_id, PropertyKind::User) => {
                label.next_field_ordinal()
            }
            _ => property.ordinal,
        };

        property.value = value;

        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => {
                    return Err(
                        self.expecting("id, at, hide, show_name, do_not_autoplace or effects")
                    );
                }
            };
            match head.as_str() {
                "id" => {
                    let _ = self.need_unquoted_symbol_atom("id")?;
                    let _ = self.parse_i32_atom("field ID")?;
                    self.need_right()?;
                }
                "at" => {
                    let _ = self.need_unquoted_symbol_atom("at")?;
                    let parsed = self.parse_xy3("property at")?;
                    property.at = Some([parsed[0], parsed[1]]);
                    property.angle = Some(parsed[2]);
                    self.need_right()?;
                }
                "hide" => {
                    let _ = self.need_unquoted_symbol_atom("hide")?;
                    property.visible = !self.parse_bool_atom("hide")?;
                    self.need_right()?;
                }
                "show_name" => {
                    let _ = self.need_unquoted_symbol_atom("show_name")?;
                    property.show_name = self.parse_maybe_absent_bool(true)?;
                }
                "do_not_autoplace" => {
                    let _ = self.need_unquoted_symbol_atom("do_not_autoplace")?;
                    property.can_autoplace = !self.parse_maybe_absent_bool(true)?;
                }
                "effects" => {
                    let convert_overbar = property.kind == PropertyKind::SymbolValue;
                    self.parse_eda_text(
                        ParsedEdaTextOwner::property(&mut property),
                        convert_overbar,
                        true,
                    )?;
                }
                _ => {
                    return Err(
                        self.expecting("id, at, hide, show_name, do_not_autoplace or effects")
                    );
                }
            }
        }
        self.need_right()?;
        Ok(property)
    }

    fn parse_stroke(&mut self) -> Result<Stroke, Error> {
        let mut stroke = Stroke::new();

        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("width, type or color")),
            };
            match head.as_str() {
                "width" => {
                    let _ = self.need_unquoted_symbol_atom("width")?;
                    stroke.width = Some(self.parse_f64_atom("stroke width")?);
                    self.need_right()?;
                }
                "type" => {
                    let _ = self.need_unquoted_symbol_atom("type")?;
                    stroke.style = match self
                        .need_unquoted_symbol_atom(
                            "default, dash, dot, dash_dot, dash_dot_dot, or solid",
                        )?
                        .as_str()
                    {
                        "default" => StrokeStyle::Default,
                        "dash" => StrokeStyle::Dash,
                        "dot" => StrokeStyle::Dot,
                        "dash_dot" => StrokeStyle::DashDot,
                        "dash_dot_dot" => StrokeStyle::DashDotDot,
                        "solid" => StrokeStyle::Solid,
                        _ => {
                            return Err(self.expecting(
                                "default, dash, dot, dash_dot, dash_dot_dot, or solid",
                            ));
                        }
                    };
                    self.need_right()?;
                }
                "color" => {
                    let _ = self.need_unquoted_symbol_atom("color")?;
                    stroke.color = Some([
                        f64::from(self.parse_i32_atom("red")?) / 255.0,
                        f64::from(self.parse_i32_atom("green")?) / 255.0,
                        f64::from(self.parse_i32_atom("blue")?) / 255.0,
                        self.parse_f64_atom("alpha")?.clamp(0.0, 1.0),
                    ]);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("width, type or color")),
            }
        }

        self.need_right()?;
        Ok(stroke)
    }

    fn parse_fill(&mut self) -> Result<Fill, Error> {
        let mut fill = Fill::new();

        while !self.at_right() {
            self.need_left()?;
            let head = match &self.current().kind {
                TokKind::Atom(value)
                    if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
                {
                    value.clone()
                }
                _ => return Err(self.expecting("type or color")),
            };
            match head.as_str() {
                "type" => {
                    let _ = self.need_unquoted_symbol_atom("type")?;
                    fill.fill_type = match self
                        .need_unquoted_symbol_atom(
                            "none, outline, hatch, reverse_hatch, cross_hatch, color or background",
                        )?
                        .as_str()
                    {
                        "none" => FillType::None,
                        "outline" => FillType::Outline,
                        "background" => FillType::Background,
                        "color" => FillType::Color,
                        "hatch" => FillType::Hatch,
                        "reverse_hatch" => FillType::ReverseHatch,
                        "cross_hatch" => FillType::CrossHatch,
                        _ => return Err(self.expecting(
                            "none, outline, hatch, reverse_hatch, cross_hatch, color or background",
                        )),
                    };
                    self.need_right()?;
                }
                "color" => {
                    let _ = self.need_unquoted_symbol_atom("color")?;
                    fill.color = Some([
                        f64::from(self.parse_i32_atom("red")?) / 255.0,
                        f64::from(self.parse_i32_atom("green")?) / 255.0,
                        f64::from(self.parse_i32_atom("blue")?) / 255.0,
                        self.parse_f64_atom("alpha")?.clamp(0.0, 1.0),
                    ]);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("type or color")),
            }
        }

        self.need_right()?;
        Ok(fill)
    }

    fn fixup_sch_fill_mode(fill: &mut Option<Fill>, stroke: &Option<Stroke>) {
        if let Some(fill) = fill.as_mut() {
            if fill.fill_type == FillType::Outline {
                fill.fill_type = FillType::Color;
                fill.color = stroke.as_ref().and_then(|stroke| stroke.color);
            }
        }
    }

    fn find_invalid_lib_id_char(value: &str) -> Option<char> {
        let (nickname, item) = match value.split_once(':') {
            Some((nickname, item)) => (Some(nickname), item),
            None => (None, value),
        };

        if let Some(nickname) = nickname {
            for ch in nickname.chars() {
                let illegal = match ch {
                    '\\' | ':' => true,
                    _ => ch.is_control(),
                };

                if illegal {
                    return Some(ch);
                }
            }
        }

        for ch in item.chars() {
            let illegal = match ch {
                ':' | '\t' | '\n' | '\r' => true,
                '\\' | '<' | '>' | '"' => true,
                _ => ch.is_control(),
            };

            if illegal {
                return Some(ch);
            }
        }

        None
    }

    fn clamp_text_size(size: [f64; 2]) -> [f64; 2] {
        [
            size[0].clamp(TEXT_MIN_SIZE_MM, TEXT_MAX_SIZE_MM),
            size[1].clamp(TEXT_MIN_SIZE_MM, TEXT_MAX_SIZE_MM),
        ]
    }

    fn parse_eda_text(
        &mut self,
        mut owner: ParsedEdaTextOwner<'_>,
        convert_overbar_syntax: bool,
        enforce_min_text_size: bool,
    ) -> Result<(), Error> {
        let _ = self.need_unquoted_symbol_atom("effects")?;
        if convert_overbar_syntax
            && self.version.unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION) < VERSION_TEXT_OVERBAR_NOTATION
        {
            if let Some(text) = owner.text.as_mut() {
                **text = self.convert_old_overbar_notation((**text).clone());
            }
        }

        let mut effects = owner.effects.take().unwrap_or_default();
        effects.h_justify = TextHJustify::Center;
        effects.v_justify = TextVJustify::Center;

        while !self.at_right() {
            let section_is_list = matches!(self.current().kind, TokKind::Left);
            if section_is_list {
                self.need_left()?;
            }
            let head = self.need_unquoted_symbol_atom("font, justify, hide or href")?;

            match head.as_str() {
                "font" => {
                    while !self.at_right() {
                        let font_is_list = matches!(self.current().kind, TokKind::Left);
                        if font_is_list {
                            self.need_left()?;
                        }
                        let head = self.need_unquoted_symbol_atom(
                            "face, size, thickness, line_spacing, bold, or italic",
                        )?;

                        match head.as_str() {
                            "face" => {
                                effects.font_face = Some(
                                    self.need_symbol_atom("font face")
                                        .map_err(|_| self.error_here("missing font face"))?,
                                );
                                if font_is_list {
                                    self.need_right()?;
                                }
                            }
                            "size" => {
                                let mut font_size = [
                                    self.parse_f64_atom("font width")?,
                                    self.parse_f64_atom("font height")?,
                                ];

                                if enforce_min_text_size {
                                    font_size = Self::clamp_text_size(font_size);
                                }

                                effects.font_size = Some(font_size);
                                if font_is_list {
                                    self.need_right()?;
                                }
                            }
                            "thickness" => {
                                effects.thickness = Some(self.parse_f64_atom("text thickness")?);
                                if font_is_list {
                                    self.need_right()?;
                                }
                            }
                            "color" => {
                                effects.color = Some([
                                    f64::from(self.parse_i32_atom("red")?) / 255.0,
                                    f64::from(self.parse_i32_atom("green")?) / 255.0,
                                    f64::from(self.parse_i32_atom("blue")?) / 255.0,
                                    self.parse_f64_atom("alpha")?.clamp(0.0, 1.0),
                                ]);
                                if font_is_list {
                                    self.need_right()?;
                                }
                            }
                            "line_spacing" => {
                                effects.line_spacing = Some(self.parse_f64_atom("line spacing")?);
                                if font_is_list {
                                    self.need_right()?;
                                }
                            }
                            "bold" => {
                                effects.bold = self.parse_maybe_absent_bool(true)?;
                            }
                            "italic" => {
                                effects.italic = self.parse_maybe_absent_bool(true)?;
                            }
                            _ => {
                                return Err(self.expecting(
                                    "face, size, thickness, line_spacing, bold, or italic",
                                ));
                            }
                        }
                    }

                    if section_is_list {
                        self.need_right()?;
                    }
                }
                "justify" => {
                    while !self.at_right() {
                        match self
                            .need_unquoted_symbol_atom("left, right, top, bottom, or mirror")?
                            .as_str()
                        {
                            "left" => {
                                effects.h_justify = TextHJustify::Left;
                            }
                            "right" => {
                                effects.h_justify = TextHJustify::Right;
                            }
                            "top" => {
                                effects.v_justify = TextVJustify::Top;
                            }
                            "bottom" => {
                                effects.v_justify = TextVJustify::Bottom;
                            }
                            "mirror" => {
                                // Upstream accepts but ignores mirror for schematic text.
                            }
                            _ => return Err(self.expecting("left, right, top, bottom, or mirror")),
                        }
                    }

                    if section_is_list {
                        self.need_right()?;
                    }
                }
                "href" => {
                    let href = self
                        .need_symbol_atom("hyperlink url")
                        .map_err(|_| self.error_here("missing hyperlink url"))?;
                    if !Self::validate_hyperlink(&href) {
                        return Err(self.error_here(format!("invalid hyperlink url `{href}`")));
                    }
                    effects.hyperlink = Some(href);
                    if section_is_list {
                        self.need_right()?;
                    }
                }
                "hide" => {
                    effects.hidden = self.parse_maybe_absent_bool(true)?;
                    *owner.visible = !effects.hidden;
                }
                _ => return Err(self.expecting("font, justify, hide or href")),
            }
        }

        if let Some(has_effects) = owner.has_effects {
            *has_effects = true;
        }
        *owner.effects = Some(effects);
        self.need_right()?;
        Ok(())
    }

    fn validate_hyperlink(href: &str) -> bool {
        // Match upstream EDA_TEXT::ValidateHyperlink:
        // - empty is valid (no hyperlink)
        // - "#page" goto-page refs are valid (IsGotoPageHref)
        // - any URI with a scheme (scheme:...) is valid
        if href.is_empty() || href.starts_with('#') {
            return true;
        }

        // Check for a URI scheme: at least one alpha char followed by ':'
        if let Some(colon_pos) = href.find(':') {
            let scheme = &href[..colon_pos];
            return !scheme.is_empty()
                && scheme
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'-' || b == b'.');
        }

        false
    }

    fn get_label_spin_style(angle: f64) -> LabelSpin {
        match angle.rem_euclid(360.0) as i32 {
            0 => LabelSpin::Right,
            90 => LabelSpin::Up,
            180 => LabelSpin::Left,
            270 => LabelSpin::Bottom,
            _ => LabelSpin::Right,
        }
    }

    fn normalize_text_angle(angle: f64) -> f64 {
        let mut normalized = angle.rem_euclid(360.0);

        if normalized <= 45.0 || normalized >= 315.0 || (normalized > 135.0 && normalized <= 225.0)
        {
            normalized = 0.0;
        } else {
            normalized = 90.0;
        }

        normalized
    }

    fn get_legacy_text_margin(stroke_width: f64, text_size_y: f64) -> f64 {
        (stroke_width / 2.0) + (text_size_y * 0.75)
    }

    fn read_png_ppi(data: &[u8]) -> Option<f64> {
        const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

        if data.len() < PNG_SIGNATURE.len() || &data[..8] != PNG_SIGNATURE {
            return None;
        }

        let mut offset = 8usize;

        while offset + 12 <= data.len() {
            let length = u32::from_be_bytes(data[offset..offset + 4].try_into().ok()?) as usize;
            let chunk_type = &data[offset + 4..offset + 8];
            let chunk_data_start = offset + 8;
            let chunk_data_end = chunk_data_start + length;
            let crc_end = chunk_data_end + 4;

            if crc_end > data.len() {
                return None;
            }

            if chunk_type == b"pHYs" && length >= 9 {
                let x_ppm = u32::from_be_bytes(
                    data[chunk_data_start..chunk_data_start + 4]
                        .try_into()
                        .ok()?,
                );
                let unit = data[chunk_data_start + 8];

                if unit == 1 {
                    return Some(f64::from(x_ppm) * 0.0254);
                }
            }

            offset = crc_end;
        }

        None
    }

    fn validate_image_data(data: &[u8]) -> bool {
        image::load_from_memory(data).is_ok()
    }

    fn parse_xy2(&mut self, context: &str) -> Result<[f64; 2], Error> {
        Ok([
            self.parse_f64_atom(format!("{context} x"))?,
            self.parse_f64_atom(format!("{context} y"))?,
        ])
    }

    fn parse_xy3(&mut self, context: &str) -> Result<[f64; 3], Error> {
        Ok([
            self.parse_f64_atom(format!("{context} x"))?,
            self.parse_f64_atom(format!("{context} y"))?,
            self.parse_f64_atom(format!("{context} angle"))?,
        ])
    }

    fn parse_i32_atom(&mut self, field: impl Into<String>) -> Result<i32, Error> {
        let field = field.into();
        let value = match &self.current().kind {
            TokKind::Atom(value) if self.current().atom_class == Some(AtomClass::Number) => {
                let out = value.clone();
                self.idx += 1;
                out
            }
            _ => return Err(self.error_here(format!("missing {field}"))),
        };
        value
            .parse::<i32>()
            .map_err(|_| self.error_here(format!("missing {field}")))
    }

    fn parse_f64_atom(&mut self, field: impl Into<String>) -> Result<f64, Error> {
        let field = field.into();
        let value = match &self.current().kind {
            TokKind::Atom(value) if self.current().atom_class == Some(AtomClass::Number) => {
                let out = value.clone();
                self.idx += 1;
                out
            }
            _ => return Err(self.error_here(format!("missing {field}"))),
        };
        value
            .parse::<f64>()
            .map_err(|_| self.error_here(format!("missing {field}")))
    }

    fn parse_bool_atom(&mut self, field: &str) -> Result<bool, Error> {
        let _ = field;
        let head = match &self.current().kind {
            TokKind::Atom(value)
                if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
            {
                value.clone()
            }
            _ => return Err(self.expecting("yes or no")),
        };
        match head.as_str() {
            "yes" => {
                let _ = self.need_unquoted_symbol_atom("yes")?;
                Ok(true)
            }
            "no" => {
                let _ = self.need_unquoted_symbol_atom("no")?;
                Ok(false)
            }
            _ => Err(self.expecting("yes or no")),
        }
    }

    fn parse_kiid(&mut self) -> Result<String, Error> {
        let raw = self.need_symbol_atom("uuid")?;
        Ok(self.normalize_kiid(raw, true))
    }

    fn parse_raw_kiid(&mut self) -> Result<String, Error> {
        let raw = self.need_symbol_atom("uuid")?;
        Ok(self.normalize_kiid(raw, false))
    }

    fn normalize_kiid(&mut self, raw: String, track_uniqueness: bool) -> String {
        let mut bytes = if !raw.is_empty()
            && raw.len() <= 8
            && raw.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            let mut bytes = [0_u8; 16];
            let padded = format!("{raw:0>8}");

            for i in 0..4 {
                bytes[12 + i] =
                    u8::from_str_radix(&padded[i * 2..i * 2 + 2], 16).unwrap_or_default();
            }

            bytes
        } else if let Ok(parsed) = Uuid::parse_str(&raw) {
            *parsed.as_bytes()
        } else {
            return raw;
        };

        let mut normalized = Uuid::from_bytes(bytes).hyphenated().to_string();

        if track_uniqueness {
            while self.used_uuids.contains(&normalized) {
                for byte in bytes.iter_mut().rev() {
                    *byte = byte.wrapping_add(1);

                    if *byte != 0 {
                        break;
                    }
                }

                normalized = Uuid::from_bytes(bytes).hyphenated().to_string();
            }

            self.used_uuids.insert(normalized.clone());
        }

        normalized
    }

    fn parse_maybe_absent_bool(&mut self, default: bool) -> Result<bool, Error> {
        let had_left = self.idx >= 2 && matches!(self.tokens[self.idx - 2].kind, TokKind::Left);

        if !had_left {
            return Ok(default);
        }

        if self.at_right() {
            self.need_right()?;
            return Ok(default);
        }

        let value = match &self.current().kind {
            TokKind::Atom(value) if matches!(value.as_str(), "yes" | "no") => {
                self.parse_bool_atom("boolean")?
            }
            _ => return Err(self.expecting("yes or no")),
        };

        self.need_right()?;
        Ok(value)
    }

    fn require_known_version(&self) -> Result<i32, Error> {
        self.version
            .ok_or_else(|| self.error_here("version must appear before this section"))
    }

    fn need_left(&mut self) -> Result<(), Error> {
        match self.current().kind {
            TokKind::Left => {
                self.idx += 1;
                Ok(())
            }
            _ => Err(self.expecting("(")),
        }
    }

    fn need_right(&mut self) -> Result<(), Error> {
        match self.current().kind {
            TokKind::Right => {
                self.idx += 1;
                Ok(())
            }
            _ => Err(self.expecting(")")),
        }
    }

    fn need_symbol_atom(&mut self, expected: &str) -> Result<String, Error> {
        match &self.current().kind {
            TokKind::Atom(value)
                if matches!(
                    self.current().atom_class,
                    Some(AtomClass::Symbol | AtomClass::Quoted)
                ) =>
            {
                let out = value.clone();
                self.idx += 1;
                Ok(out)
            }
            _ => Err(self.expecting(expected)),
        }
    }

    fn need_unquoted_symbol_atom(&mut self, expected: &str) -> Result<String, Error> {
        match &self.current().kind {
            TokKind::Atom(value)
                if matches!(self.current().atom_class, Some(AtomClass::Symbol)) =>
            {
                let out = value.clone();
                self.idx += 1;
                Ok(out)
            }
            _ => Err(self.expecting(expected)),
        }
    }

    fn need_quoted_atom(&mut self, expected: &str) -> Result<String, Error> {
        match &self.current().kind {
            TokKind::Atom(value)
                if matches!(self.current().atom_class, Some(AtomClass::Quoted)) =>
            {
                let out = value.clone();
                self.idx += 1;
                Ok(out)
            }
            _ => Err(self.expecting(expected)),
        }
    }

    fn need_symbol_or_number_atom(&mut self, expected: &str) -> Result<String, Error> {
        match &self.current().kind {
            TokKind::Atom(value)
                if matches!(
                    self.current().atom_class,
                    Some(AtomClass::Symbol | AtomClass::Number | AtomClass::Quoted)
                ) =>
            {
                let out = value.clone();
                self.idx += 1;
                Ok(out)
            }
            _ => Err(self.expecting(expected)),
        }
    }

    fn at_right(&self) -> bool {
        matches!(self.current().kind, TokKind::Right)
    }

    fn at_unquoted_symbol_with(&self, expected: &str) -> bool {
        matches!(
            (&self.current().kind, self.current().atom_class),
            (TokKind::Atom(value), Some(AtomClass::Symbol)) if value == expected
        )
    }

    fn current_nesting_depth(&self) -> usize {
        let mut depth = 0usize;

        for token in &self.tokens[..self.idx] {
            match token.kind {
                TokKind::Left => depth += 1,
                TokKind::Right => depth = depth.saturating_sub(1),
                _ => {}
            }
        }

        depth
    }

    fn skip_to_block_right(&mut self, target_depth: usize) {
        let mut depth = self.current_nesting_depth();

        while !matches!(self.current().kind, TokKind::Eof) {
            match self.current().kind {
                TokKind::Left => {
                    depth += 1;
                    self.idx += 1;
                }
                TokKind::Right => {
                    if depth == target_depth {
                        break;
                    }

                    depth = depth.saturating_sub(1);
                    self.idx += 1;
                }
                _ => {
                    self.idx += 1;
                }
            }
        }
    }

    fn current(&self) -> &Token {
        &self.tokens[self.idx]
    }

    fn current_span(&self) -> Span {
        self.current().span
    }

    fn expecting(&self, expected: &str) -> Error {
        self.error_here(format!("expecting {expected}"))
    }

    fn unexpected(&self, found: &str) -> Error {
        self.error_here(format!("unexpected {found}"))
    }

    fn error_here(&self, message: impl Into<String>) -> Error {
        self.validation(Some(self.current_span()), message)
    }

    fn find_standard_page_info(kind: &str) -> Option<StandardPageInfo> {
        STANDARD_PAGE_INFOS
            .iter()
            .find(|candidate| candidate.kind.eq_ignore_ascii_case(kind))
            .copied()
    }

    fn parse_page_info(&mut self) -> Result<Paper, Error> {
        let token_span = self.current_span();
        let raw_kind = self
            .need_symbol_atom("paper kind")
            .map_err(|_| self.error_here("missing paper kind"))?;
        let page_info = Self::find_standard_page_info(&raw_kind)
            .ok_or_else(|| self.validation(Some(token_span), "Invalid page type"))?;
        let [width, height] = if page_info.kind == "User" {
            let width = self
                .parse_f64_atom("width")?
                .clamp(MIN_PAGE_SIZE_MM, MAX_PAGE_SIZE_EESCHEMA_MM);
            let height = self
                .parse_f64_atom("height")?
                .clamp(MIN_PAGE_SIZE_MM, MAX_PAGE_SIZE_EESCHEMA_MM);
            [width, height]
        } else {
            page_info
                .dimensions_mm
                .ok_or_else(|| self.error_here("Invalid page type"))?
        };
        let token = self.current().clone();
        self.idx += 1;
        let portrait = match token.kind {
            TokKind::Right => false,
            TokKind::Atom(value)
                if token.atom_class == Some(AtomClass::Symbol) && value == "portrait" =>
            {
                self.need_right()?;
                true
            }
            _ => return Err(self.validation(Some(token.span), "expecting portrait")),
        };

        let kind = page_info.kind.to_string();
        let mut width = width;
        let mut height = height;
        let mut portrait_state = kind == "User" && height > width;

        if portrait && !portrait_state {
            std::mem::swap(&mut width, &mut height);
            portrait_state = true;
        }

        Ok(Paper {
            kind,
            width: Some(width),
            height: Some(height),
            portrait: portrait_state,
        })
    }

    fn convert_old_overbar_notation(&self, old: String) -> String {
        if old == "~" {
            return old;
        }

        let chars: Vec<char> = old.chars().collect();
        let mut out = String::with_capacity(old.len());
        let mut in_overbar = false;
        let mut i = 0usize;

        while i < chars.len() {
            let ch = chars[i];

            if ch == '~' {
                if i + 1 < chars.len() && chars[i + 1] == '~' {
                    if i + 2 < chars.len() && chars[i + 2] == '{' {
                        out.push_str("~~{}");
                        i += 2;
                        i += 1;
                        continue;
                    }

                    out.push('~');
                    i += 2;
                    continue;
                } else if i + 1 < chars.len() && chars[i + 1] == '{' {
                    return old;
                } else {
                    if in_overbar {
                        out.push('}');
                        in_overbar = false;
                    } else {
                        out.push('~');
                        out.push('{');
                        in_overbar = true;
                    }
                    i += 1;
                    continue;
                }
            } else if matches!(ch, ' ' | '}' | ')') && in_overbar {
                out.push('}');
                in_overbar = false;
            }

            out.push(ch);
            i += 1;
        }

        if in_overbar {
            out.push('}');
        }

        out
    }

    fn unescape_string_markers(source: &str) -> String {
        if source.len() <= 2 {
            return source.to_string();
        }

        let chars: Vec<char> = source.chars().collect();
        let mut out = String::with_capacity(source.len());
        let mut prev = '\0';
        let mut i = 0usize;

        while i < chars.len() {
            let ch = chars[i];

            if ch == '{' {
                let mut token = String::new();
                let mut depth = 1usize;
                let mut j = i + 1;
                let mut terminated = false;

                while j < chars.len() {
                    let nested = chars[j];

                    if nested == '{' {
                        depth += 1;
                    } else if nested == '}' {
                        depth -= 1;
                    }

                    if depth == 0 {
                        terminated = true;
                        break;
                    }

                    token.push(nested);
                    j += 1;
                }

                if !terminated {
                    out.push('{');
                    out.push_str(&Self::unescape_string_markers(&token));
                    break;
                }

                if matches!(prev, '$' | '~' | '^' | '_') {
                    out.push('{');
                    out.push_str(&Self::unescape_string_markers(&token));
                    out.push('}');
                } else {
                    match token.as_str() {
                        "dblquote" => out.push('"'),
                        "quote" => out.push('\''),
                        "lt" => out.push('<'),
                        "gt" => out.push('>'),
                        "backslash" => out.push('\\'),
                        "slash" => out.push('/'),
                        "bar" => out.push('|'),
                        "comma" => out.push(','),
                        "colon" => out.push(':'),
                        "space" => out.push(' '),
                        "dollar" => out.push('$'),
                        "tab" => out.push('\t'),
                        "return" => out.push('\n'),
                        "brace" => out.push('{'),
                        _ => {
                            out.push('{');
                            out.push_str(&Self::unescape_string_markers(&token));
                            out.push('}');
                        }
                    }
                }

                prev = '}';
                i = j + 1;
                continue;
            }

            out.push(ch);
            prev = ch;
            i += 1;
        }

        out
    }

    fn fixup_legacy_lib_symbol_alternate_body_styles(&mut self) {
        let Some(version) = self.version else {
            return;
        };

        if version >= VERSION_CUSTOM_BODY_STYLES {
            return;
        }

        let symbol_index: std::collections::HashMap<String, usize> = self
            .screen
            .lib_symbols
            .iter()
            .enumerate()
            .map(|(idx, symbol)| (symbol.lib_id.clone(), idx))
            .collect();

        let mut cache = std::collections::HashMap::new();

        for idx in 0..self.screen.lib_symbols.len() {
            if self.screen.lib_symbols[idx].extends.is_none() {
                self.screen.lib_symbols[idx].has_demorgan = if version < VERSION_CUSTOM_BODY_STYLES
                {
                    self.screen.lib_symbols[idx].has_legacy_alternate_body_style()
                } else {
                    self.screen.lib_symbols[idx].has_demorgan
                };
                cache.insert(idx, self.screen.lib_symbols[idx].has_demorgan);
                continue;
            }

            let has_demorgan = Self::has_legacy_alternate_body_style(
                idx,
                &self.screen.lib_symbols,
                &symbol_index,
                &mut cache,
            );
            self.screen.lib_symbols[idx].has_demorgan = has_demorgan;
        }
    }

    fn update_local_lib_symbol_links(&mut self) {
        let lib_symbols: std::collections::HashMap<String, LibSymbol> = self
            .screen
            .lib_symbols
            .iter()
            .cloned()
            .map(|symbol| (symbol.lib_id.clone(), symbol))
            .collect();
        let mut flattened = std::collections::HashMap::new();

        for item in &mut self.screen.items {
            if let SchItem::Symbol(symbol) = item {
                let lib_name = symbol.lib_name.as_deref().unwrap_or(&symbol.lib_id);
                symbol.lib_symbol = Self::flatten_local_lib_symbol(
                    lib_name,
                    &lib_symbols,
                    &mut flattened,
                    &mut std::collections::BTreeSet::new(),
                );
            }
        }
    }

    fn flatten_local_lib_symbol(
        lib_id: &str,
        symbols: &std::collections::HashMap<String, LibSymbol>,
        cache: &mut std::collections::HashMap<String, LibSymbol>,
        stack: &mut std::collections::BTreeSet<String>,
    ) -> Option<LibSymbol> {
        if let Some(symbol) = cache.get(lib_id) {
            return Some(symbol.clone());
        }

        let symbol = symbols.get(lib_id)?.clone();

        if !stack.insert(lib_id.to_string()) {
            return Some(symbol);
        }

        let mut flattened = if let Some(parent_name) = symbol.extends.as_deref() {
            if let Some(mut parent) =
                Self::flatten_local_lib_symbol(parent_name, symbols, cache, stack)
            {
                parent.lib_id = symbol.lib_id.clone();
                parent.name = symbol.name.clone();
                parent.extends = None;

                for unit in &mut parent.units {
                    unit.name = format!("{}_{}_{}", parent.name, unit.unit_number, unit.body_style);
                }

                if symbol.body_styles_specified {
                    parent.body_styles_specified = true;
                    parent.body_style_names = symbol.body_style_names.clone();
                    parent.has_demorgan = symbol.has_demorgan;
                }

                for unit in &symbol.units {
                    if let Some(unit_name) = unit.unit_name.as_ref() {
                        parent.set_unit_display_name(unit.unit_number, unit_name.clone());
                    }
                }

                for property in &symbol.properties {
                    if property.kind.is_mandatory() && !property.value.is_empty() {
                        if let Some(existing) = parent
                            .properties
                            .iter_mut()
                            .find(|existing| existing.kind == property.kind)
                        {
                            *existing = property.clone();
                        }
                    }
                }

                for unit in &symbol.units {
                    for item in &unit.draw_items {
                        if item.kind == "field"
                            && let Some(field_name) = item.name.as_deref()
                        {
                            for existing_unit in &mut parent.units {
                                existing_unit.draw_items.retain(|existing| {
                                    !(existing.kind == "field"
                                        && existing.name.as_deref() == Some(field_name))
                                });
                                existing_unit.draw_item_kinds = existing_unit
                                    .draw_items
                                    .iter()
                                    .map(|existing| existing.kind.clone())
                                    .collect();
                            }
                        }

                        parent.add_draw_item(item.clone());
                    }
                }

                if let Some(keywords) = symbol.keywords.as_ref() {
                    parent.keywords = Some(keywords.clone());
                }

                if let Some(description) = symbol.description.as_ref() {
                    parent.description = Some(description.clone());
                }

                if symbol.fp_filters_specified {
                    parent.fp_filters_specified = true;
                    parent.fp_filters = symbol.fp_filters.clone();
                }

                if symbol.embedded_fonts.is_some() {
                    parent.embedded_fonts = symbol.embedded_fonts;
                }

                if !symbol.embedded_files.is_empty() {
                    let mut named_files = std::collections::BTreeMap::new();
                    let mut unnamed_files = Vec::new();

                    for file in parent.embedded_files {
                        if let Some(name) = file.name.as_ref() {
                            named_files.insert(name.clone(), file);
                        } else {
                            unnamed_files.push(file);
                        }
                    }

                    for file in &symbol.embedded_files {
                        if let Some(name) = file.name.as_ref() {
                            named_files.insert(name.clone(), file.clone());
                        } else {
                            unnamed_files.push(file.clone());
                        }
                    }

                    parent.embedded_files =
                        named_files.into_values().chain(unnamed_files).collect();
                }

                if let Some(immediate_parent) = symbols.get(parent_name) {
                    parent.excluded_from_sim = immediate_parent.excluded_from_sim;
                    parent.in_bom = immediate_parent.in_bom;
                    parent.on_board = immediate_parent.on_board;
                }

                parent
            } else {
                symbol.clone()
            }
        } else {
            symbol.clone()
        };

        flattened.refresh_library_tree_caches();
        stack.remove(lib_id);
        cache.insert(lib_id.to_string(), flattened.clone());
        Some(flattened)
    }

    fn has_legacy_alternate_body_style(
        idx: usize,
        symbols: &[LibSymbol],
        symbol_index: &std::collections::HashMap<String, usize>,
        cache: &mut std::collections::HashMap<usize, bool>,
    ) -> bool {
        if let Some(cached) = cache.get(&idx) {
            return *cached;
        }

        let symbol = &symbols[idx];

        if symbol.has_legacy_alternate_body_style() {
            cache.insert(idx, true);
            return true;
        }

        if let Some(parent_name) = symbol.extends.as_ref() {
            if let Some(parent_idx) = symbol_index.get(parent_name) {
                let inherited = Self::has_legacy_alternate_body_style(
                    *parent_idx,
                    symbols,
                    symbol_index,
                    cache,
                );
                cache.insert(idx, inherited);
                return inherited;
            }
        }

        cache.insert(idx, false);
        false
    }

    fn fixup_embedded_data(&mut self) {
        let global_files: std::collections::HashMap<String, EmbeddedFile> = self
            .screen
            .embedded_files
            .iter()
            .filter_map(|file| Some((file.name.clone()?, file.clone())))
            .collect();

        let hydrate_files = |files: &mut Vec<EmbeddedFile>| {
            for embedded_file in files {
                if let Some(name) = embedded_file.name.as_ref() {
                    if let Some(file) = global_files.get(name) {
                        if embedded_file.checksum.is_none() {
                            embedded_file.checksum = file.checksum.clone();
                        }
                        if embedded_file.file_type.is_none() {
                            embedded_file.file_type = file.file_type;
                        }
                        if embedded_file.data.is_none() {
                            embedded_file.data = file.data.clone();
                        }
                    }
                }
            }
        };

        for lib_symbol in &mut self.screen.lib_symbols {
            hydrate_files(&mut lib_symbol.embedded_files);
        }

        for item in &mut self.screen.items {
            if let SchItem::Symbol(symbol) = item {
                if let Some(lib_symbol) = symbol.lib_symbol.as_mut() {
                    hydrate_files(&mut lib_symbol.embedded_files);
                }
            }
        }
    }

    fn resolve_groups(&mut self) {
        if self.pending_groups.is_empty() {
            return;
        }

        for group_info in &self.pending_groups {
            let mut group = Group::new();
            group.name = group_info.name.clone();
            group.uuid = group_info.uuid.clone();
            group.lib_id = group_info.lib_id.clone();
            self.screen.items.push(SchItem::Group(group));
        }

        let pending_groups = self.pending_groups.drain(..).collect::<Vec<_>>();

        for group_info in pending_groups {
            let Some(group_uuid) = group_info.uuid.as_ref() else {
                continue;
            };

            let Some(group_index) = self.get_item_index_by_uuid(group_uuid) else {
                continue;
            };

            let resolved_members = group_info
                .member_uuids
                .into_iter()
                .filter(|member_uuid| self.get_item_index_by_uuid(member_uuid).is_some())
                .collect::<Vec<_>>();

            let SchItem::Group(group) = &mut self.screen.items[group_index] else {
                continue;
            };

            group.members = resolved_members;
        }

        self.groups_sanity_check();
    }

    fn get_item_index_by_uuid(&self, uuid: &str) -> Option<usize> {
        self.screen
            .items
            .iter()
            .enumerate()
            .find_map(|(idx, item)| (Self::item_uuid(item) == Some(uuid)).then_some(idx))
    }

    fn item_uuid(item: &SchItem) -> Option<&str> {
        match item {
            SchItem::Junction(item) => item.uuid.as_deref(),
            SchItem::NoConnect(item) => item.uuid.as_deref(),
            SchItem::BusEntry(item) => item.uuid.as_deref(),
            SchItem::Wire(item) | SchItem::Bus(item) | SchItem::Polyline(item) => {
                item.uuid.as_deref()
            }
            SchItem::Label(item) => item.uuid.as_deref(),
            SchItem::Text(item) => item.uuid.as_deref(),
            SchItem::TextBox(item) => item.uuid.as_deref(),
            SchItem::Table(item) => item.uuid.as_deref(),
            SchItem::Image(item) => item.uuid.as_deref(),
            SchItem::Shape(item) => item.uuid.as_deref(),
            SchItem::Symbol(item) => item.uuid.as_deref(),
            SchItem::Sheet(item) => item.uuid.as_deref(),
            SchItem::Group(item) => item.uuid.as_deref(),
        }
    }

    fn groups_sanity_check(&mut self) {
        loop {
            let groups = self
                .screen
                .items
                .iter()
                .enumerate()
                .filter_map(|(idx, item)| match item {
                    SchItem::Group(group) => Some((idx, group)),
                    _ => None,
                })
                .collect::<Vec<_>>();

            let group_indices = groups
                .iter()
                .filter_map(|(idx, group)| Some((group.uuid.as_ref()?.clone(), *idx)))
                .collect::<std::collections::HashMap<_, _>>();

            let mut parent = std::collections::HashMap::<String, String>::new();

            for (_, group) in &groups {
                let Some(parent_uuid) = group.uuid.as_ref() else {
                    continue;
                };

                for member in &group.members {
                    if group_indices.contains_key(member) {
                        parent.entry(member.clone()).or_insert(parent_uuid.clone());
                    }
                }
            }

            let mut removed_uuid = None;
            let mut known_cycle_free = std::collections::HashSet::<String>::new();

            for (_, group) in &groups {
                let Some(start_uuid) = group.uuid.as_ref() else {
                    continue;
                };

                if known_cycle_free.contains(start_uuid) {
                    continue;
                }

                let mut current_chain = std::collections::HashSet::<String>::new();
                let mut current = start_uuid.clone();

                loop {
                    if current_chain.contains(&current) {
                        removed_uuid = Some(current);
                        break;
                    }

                    if known_cycle_free.contains(&current) {
                        break;
                    }

                    current_chain.insert(current.clone());

                    let Some(next) = parent.get(&current).cloned() else {
                        break;
                    };

                    current = next;
                }

                if removed_uuid.is_some() {
                    break;
                }

                known_cycle_free.extend(current_chain);
            }

            let Some(removed_uuid) = removed_uuid else {
                break;
            };

            if let Some(remove_idx) = group_indices.get(&removed_uuid).copied() {
                self.screen.items.remove(remove_idx);
            } else {
                break;
            }
        }

        let valid_uuids = self
            .screen
            .items
            .iter()
            .filter_map(Self::item_uuid)
            .map(str::to_string)
            .collect::<std::collections::HashSet<_>>();

        for item in &mut self.screen.items {
            if let SchItem::Group(group) = item {
                group.members.retain(|member| valid_uuids.contains(member));
            }
        }
    }

    fn validation(&self, span: Option<Span>, message: impl Into<String>) -> Error {
        let mut diagnostic =
            Diagnostic::error("schematic-parse", message.into()).with_path(self.path.clone());
        if let Some(span) = span {
            diagnostic = diagnostic.with_span(span);
        }
        Error::Validation {
            path: self.path.clone(),
            diagnostic,
        }
    }
}
