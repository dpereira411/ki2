use std::path::{Path, PathBuf};

use base64::Engine;
use kiutils_sexpr::Span;
use uuid::Uuid;

use crate::diagnostic::Diagnostic;
use crate::error::Error;
use crate::model::{
    BusAlias, BusEntry, EmbeddedFile, Fill, FillType, Group, Image, ItemVariant, Junction, Label,
    LabelKind, LabelShape, LabelSpin, LibDrawItem, LibPinAlternate, LibSymbol, Line, LineKind,
    MirrorAxis, NoConnect, Page, Paper, Property, PropertyKind, RootSheet, SchItem, Schematic,
    Screen, Shape, ShapeKind, Sheet, SheetInstance, SheetLocalInstance, SheetPin, SheetPinShape,
    SheetSide, Stroke, StrokeStyle, Symbol, SymbolInstance, SymbolLocalInstance, SymbolPin, Table,
    Text, TextBox, TextEffects, TextHJustify, TextKind, TextVJustify, TitleBlock, VariantField,
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
const VERSION_SET_LEGACY_SYMBOL_INSTANCE_DATA: i32 = 20200828;
const VERSION_CUSTOM_BODY_STYLES: i32 = 20250827;
const VERSION_WRONG_SHEET_FIELD_IDS: i32 = 20200310;
const DEFAULT_LINE_WIDTH_MM: f64 = 0.1524;
const DEFAULT_TEXT_SIZE_MM: f64 = 1.27;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldParent {
    Symbol,
    Sheet,
    GlobalLabel,
    OtherLabel,
}

#[derive(Clone, Copy)]
enum SchTextTarget {
    Text,
    Label(LabelKind),
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
    screen: Screen,
    pending_groups: Vec<Group>,
}

impl KiCadSchematicParser {
    fn new(path: PathBuf, tokens: Vec<Token>) -> Self {
        let page_info = Self::lookup_standard_page_info("A4").expect("A4 page info must exist");
        let [width, height] = page_info.dimensions_mm.expect("A4 dimensions must exist");

        Self {
            path,
            tokens,
            idx: 0,
            version: None,
            generator: None,
            generator_version: None,
            root_uuid: None,
            screen: Screen {
                uuid: None,
                paper: Some(Paper {
                    kind: page_info.kind.to_string(),
                    width: Some(width),
                    height: Some(height),
                    portrait: false,
                }),
                page: None,
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

        if self.current_is_list_named("version") {
            self.need_left()?;
            if self.need_unquoted_symbol_atom("version")? != "version" {
                return Err(self.expecting("version"));
            }
            self.reject_duplicate(self.version.is_some(), "version")?;
            self.version = Some(self.parse_i32_atom("version")?);
            self.need_right()?;
        } else {
            self.version = Some(SEXPR_SCHEMATIC_FILE_VERSION);
        }

        self.parse_schematic_body()?;
        let version = self
            .version
            .ok_or_else(|| self.error_here("missing version"))?;
        self.update_local_lib_symbol_links();
        self.fixup_legacy_lib_symbol_body_styles();
        self.fixup_embedded_data();

        if version < VERSION_SET_LEGACY_SYMBOL_INSTANCE_DATA {
            self.set_legacy_symbol_instance_data();
        }

        self.resolve_groups();
        self.need_right()?;
        self.expect_eof()?;

        self.check_version(version, Some(self.current_span()))?;
        let generator = self
            .generator
            .clone()
            .ok_or_else(|| self.error_here("missing generator"))?;

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
            generator,
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
            let head = self.need_unquoted_symbol_atom(
                "generator, host, generator_version, uuid, paper, page, title_block, embedded_fonts, embedded_files, lib_symbols, bus_alias, symbol, sheet, junction, no_connect, bus_entry, wire, bus, polyline, label, global_label, hierarchical_label, directive_label, class_label, netclass_flag, text, text_box, table, image, arc, circle, rectangle, bezier, rule_area, sheet_instances, symbol_instances, or group",
            )?;
            let mut effective_head = head.as_str();

            if effective_head == "page"
                && self
                    .require_known_version()
                    .unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION)
                    <= VERSION_PAGE_RENAMED_TO_PAPER
            {
                effective_head = "paper";
            }
            let mut parsed_item = None;
            let mut section_consumed_right = false;

            match effective_head {
                "generator" => self.generator = Some(self.need_symbol_atom("generator")?),
                "host" => {
                    self.generator = Some(self.need_symbol_atom("host")?);
                    if self.require_known_version()? < 20200827 {
                        let _ = self.need_symbol_atom("host version")?;
                    }
                }
                "generator_version" => {
                    self.require_version(VERSION_GENERATOR_VERSION, "generator_version")?;
                    self.generator_version = Some(self.parse_string_atom("generator_version")?);
                }
                "uuid" => {
                    let uuid = self.need_symbol_atom("uuid")?;
                    self.screen.uuid = Some(uuid.clone());
                    self.root_uuid = Some(uuid);
                }
                "paper" => {
                    self.screen.paper = Some(self.parse_page_info()?);
                    section_consumed_right = true;
                }
                "page" => {
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
                "title_block" => self.parse_title_block()?,
                "embedded_fonts" => {
                    self.screen.embedded_fonts = Some(self.parse_bool_atom("embedded_fonts")?);
                }
                "embedded_files" => {
                    self.require_version(VERSION_EMBEDDED_FILES, "embedded_files")?;
                    let block_depth = self.current_nesting_depth();
                    match (|| -> Result<Vec<EmbeddedFile>, Error> {
                        let mut files = Vec::new();

                        while !self.at_right() {
                            self.need_left()?;
                            let head = self.need_unquoted_symbol_atom("file")?;
                            if head != "file" {
                                return Err(self.expecting("file"));
                            }
                            let mut name = None;
                            let mut data = None;

                            if self.at_atom() {
                                name = Some(self.need_atom()?);
                            }
                            if self.at_atom() {
                                data = Some(self.need_atom()?);
                            }

                            while !self.at_right() {
                                self.need_left()?;
                                let head = self.need_unquoted_symbol_atom("name or data")?;
                                match head.as_str() {
                                    "name" => name = Some(self.parse_string_atom("name")?),
                                    "data" => data = Some(self.parse_string_atom("data")?),
                                    _ => return Err(self.expecting("name or data")),
                                }
                                self.need_right()?;
                            }

                            let file = EmbeddedFile { name, data };
                            self.need_right()?;
                            files.push(file);
                        }

                        Ok(files)
                    })() {
                        Ok(files) => self.screen.embedded_files.extend(files),
                        Err(err) => {
                            self.screen.parse_warnings.push(err.to_string());
                            self.skip_to_block_right(block_depth);
                        }
                    }
                }
                "lib_symbols" => self.parse_sch_lib_symbols()?,
                "bus_alias" => self.parse_bus_alias()?,
                "symbol" => parsed_item = Some(SchItem::Symbol(self.parse_symbol()?)),
                "sheet" => parsed_item = Some(SchItem::Sheet(self.parse_sheet()?)),
                "junction" => parsed_item = Some(SchItem::Junction(self.parse_junction()?)),
                "no_connect" => parsed_item = Some(SchItem::NoConnect(self.parse_no_connect()?)),
                "bus_entry" => parsed_item = Some(SchItem::BusEntry(self.parse_bus_entry()?)),
                "wire" => parsed_item = Some(SchItem::Wire(self.parse_sch_line(LineKind::Wire)?)),
                "bus" => parsed_item = Some(SchItem::Bus(self.parse_sch_line(LineKind::Bus)?)),
                "polyline" => {
                    let shape = self.parse_polyline_shape()?;
                    if shape.points.len() < 2 {
                        return Err(self.error_here("Schematic polyline has too few points"));
                    }
                    if shape.points.len() == 2 {
                        parsed_item = Some(SchItem::Polyline(Line {
                            kind: LineKind::Polyline,
                            points: shape.points,
                            has_stroke: shape.has_stroke,
                            stroke: shape.stroke,
                            uuid: shape.uuid,
                        }));
                    } else {
                        parsed_item = Some(SchItem::Shape(shape));
                    }
                }
                "label" | "global_label" | "hierarchical_label" | "directive_label"
                | "class_label" | "netclass_flag" | "text" => {
                    parsed_item = Some(self.parse_sch_text(effective_head)?)
                }
                "text_box" => parsed_item = Some(SchItem::TextBox(self.parse_sch_text_box()?)),
                "table" => parsed_item = Some(SchItem::Table(self.parse_sch_table()?)),
                "image" => parsed_item = Some(SchItem::Image(self.parse_sch_image()?)),
                "arc" => parsed_item = Some(SchItem::Shape(self.parse_sch_arc()?)),
                "circle" => parsed_item = Some(SchItem::Shape(self.parse_sch_circle()?)),
                "rectangle" => parsed_item = Some(SchItem::Shape(self.parse_sch_rectangle()?)),
                "bezier" => parsed_item = Some(SchItem::Shape(self.parse_sch_bezier()?)),
                "rule_area" => parsed_item = Some(SchItem::Shape(self.parse_rule_area_shape()?)),
                "sheet_instances" => {
                    self.screen.sheet_instances = self.parse_sch_sheet_instances()?
                }
                "symbol_instances" => {
                    self.screen.symbol_instances = self.parse_sch_symbol_instances()?
                }
                "group" => self.parse_group()?,
                _ => return Err(self.expecting_known_section(&head)),
            }
            if let Some(item) = parsed_item {
                self.screen.items.push(item);
            }
            if !section_consumed_right {
                self.need_right()?;
            }
        }

        Ok(())
    }

    fn parse_title_block(&mut self) -> Result<(), Error> {
        let mut title_block = TitleBlock::default();
        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom("title, date, rev, company, or comment")?;
            match head.as_str() {
                "title" => {
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
                        _ => return Err(self.error_here("Invalid title block comment number")),
                    };

                    if let Some(existing) = title_block
                        .comments
                        .iter_mut()
                        .find(|(existing_idx, _)| *existing_idx == comment_number)
                    {
                        existing.1 = value;
                    } else {
                        title_block.comments.push((comment_number, value));
                    }
                }
                _ => return Err(self.expecting("title, date, rev, company, or comment")),
            }
            self.need_right()?;
        }
        self.screen.title_block = Some(title_block);
        Ok(())
    }

    fn parse_sch_lib_symbols(&mut self) -> Result<(), Error> {
        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom("symbol")?;
            if head != "symbol" {
                return Err(self.expecting("symbol"));
            }
            let block_depth = self.current_nesting_depth();
            match self.parse_lib_symbol() {
                Ok(symbol) => {
                    self.need_right()?;
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
        Ok(())
    }

    fn parse_lib_symbol(&mut self) -> Result<LibSymbol, Error> {
        let raw_name = self
            .need_symbol_atom("lib symbol name")
            .map_err(|_| self.error_here("Invalid symbol name"))?;
        let name = raw_name.replace("{slash}", "/");

        if let Some(ch) = Self::find_invalid_library_identifier_char(&name) {
            return Err(self.error_here(format!("Symbol {name} contains invalid character '{ch}'")));
        }

        if name.is_empty() {
            return Err(self.error_here("Invalid library identifier"));
        }

        let mut extends = None;
        let mut power = false;
        let mut local_power = false;
        let mut body_style_names = Vec::new();
        let mut has_demorgan = false;
        let mut pin_name_offset = None;
        let mut show_pin_names = true;
        let mut show_pin_numbers = true;
        let mut excluded_from_sim = false;
        let mut in_bom = true;
        let mut on_board = true;
        let mut in_pos_files = true;
        let mut duplicate_pin_numbers_are_jumpers = false;
        let mut jumper_pin_groups = Vec::new();
        let mut keywords = None;
        let mut description = None;
        let mut fp_filters = Vec::new();
        let mut locked_units = false;
        let mut properties: Vec<Property> = Vec::new();
        let mut units = Vec::new();
        let mut embedded_fonts = None;
        let mut embedded_files = Vec::new();

        while !self.at_right() {
            self.need_left()?;
            let branch = self.need_unquoted_symbol_atom(
                "pin_names, pin_numbers, arc, bezier, circle, pin, polyline, rectangle, or text",
            )?;
            match branch.as_str() {
                "power" => {
                    power = true;
                    if self.at_atom() {
                        match self.need_unquoted_symbol_atom("global or local")?.as_str() {
                            "local" => local_power = true,
                            "global" => local_power = false,
                            _ => return Err(self.expecting("global or local")),
                        }
                    }
                    self.need_right()?;
                }
                "body_styles" => {
                    while !self.at_right() {
                        if self.at_unquoted_symbol_with("demorgan") {
                            let _ = self.need_unquoted_symbol_atom("demorgan")?;
                            has_demorgan = true;
                        } else {
                            body_style_names.push(self.need_symbol_atom("property value")?);
                        }
                    }
                    self.need_right()?;
                }
                "pin_names" => {
                    while !self.at_right() {
                        if self.at_unquoted_symbol_with("hide") {
                            let _ = self.need_unquoted_symbol_atom("hide")?;
                            show_pin_names = false;
                            continue;
                        }

                        self.need_left()?;
                        match self.need_unquoted_symbol_atom("offset or hide")?.as_str() {
                            "offset" => {
                                pin_name_offset = Some(self.parse_f64_atom("pin name offset")?);
                                self.need_right()?;
                            }
                            "hide" => {
                                show_pin_names = !self.parse_bool_atom("hide")?;
                                self.need_right()?;
                            }
                            _ => return Err(self.expecting("offset or hide")),
                        }
                    }
                    self.need_right()?;
                }
                "pin_numbers" => {
                    while !self.at_right() {
                        if self.at_unquoted_symbol_with("hide") {
                            let _ = self.need_unquoted_symbol_atom("hide")?;
                            show_pin_numbers = false;
                            continue;
                        }

                        self.need_left()?;
                        match self.need_unquoted_symbol_atom("hide")?.as_str() {
                            "hide" => {
                                show_pin_numbers = !self.parse_bool_atom("hide")?;
                                self.need_right()?;
                            }
                            _ => return Err(self.expecting("hide")),
                        }
                    }
                    self.need_right()?;
                }
                "exclude_from_sim" => {
                    excluded_from_sim = self.parse_bool_atom("exclude_from_sim")?;
                    self.need_right()?;
                }
                "in_bom" => {
                    in_bom = self.parse_bool_atom("in_bom")?;
                    self.need_right()?;
                }
                "on_board" => {
                    on_board = self.parse_bool_atom("on_board")?;
                    self.need_right()?;
                }
                "in_pos_files" => {
                    in_pos_files = self.parse_bool_atom("in_pos_files")?;
                    self.need_right()?;
                }
                "duplicate_pin_numbers_are_jumpers" => {
                    duplicate_pin_numbers_are_jumpers =
                        self.parse_bool_atom("duplicate_pin_numbers_are_jumpers")?;
                    self.need_right()?;
                }
                "jumper_pin_groups" => {
                    while !self.at_right() {
                        self.need_left()?;
                        let mut group = Vec::new();
                        while !self.at_right() {
                            group.push(self.need_quoted_atom("list of pin names")?);
                        }
                        self.need_right()?;
                        jumper_pin_groups.push(group);
                    }
                    self.need_right()?;
                }
                "property" => {
                    let mut property = self.parse_lib_property()?;
                    match property.key.as_str() {
                        "ki_keywords" => keywords = Some(property.value),
                        "ki_description" => description = Some(property.value),
                        "ki_fp_filters" => {
                            fp_filters = property
                                .value
                                .split_whitespace()
                                .map(str::to_string)
                                .collect();
                        }
                        "ki_locked" => locked_units = true,
                        _ => {
                            if matches!(
                                property.kind,
                                PropertyKind::SymbolReference
                                    | PropertyKind::SymbolValue
                                    | PropertyKind::SymbolFootprint
                                    | PropertyKind::SymbolDatasheet
                            ) {
                                if let Some(existing) =
                                    properties.iter_mut().find(|p| p.kind == property.kind)
                                {
                                    *existing = property;
                                } else {
                                    properties.push(property);
                                }
                            } else if properties
                                .iter()
                                .any(|existing| existing.key == property.key)
                            {
                                let base = property.key.clone();

                                for suffix in 1..10 {
                                    let candidate = format!("{base}_{suffix}");

                                    if !properties.iter().any(|existing| existing.key == candidate) {
                                        property.key = candidate;
                                        properties.push(property);
                                        break;
                                    }
                                }
                            } else {
                                properties.push(property);
                            }
                        }
                    }
                    self.need_right()?;
                }
                "extends" => {
                    extends = Some(
                        self.need_symbol_atom("parent symbol name")
                            .map_err(|_| self.error_here("Invalid parent symbol name"))?
                            .replace("{slash}", "/"),
                    );
                    self.need_right()?;
                }
                "symbol" => {
                    let unit_name_raw = self
                        .need_symbol_atom("symbol unit name")
                        .map_err(|_| self.error_here("Invalid symbol unit name"))?;
                    let unit_full_name = unit_name_raw.replace("{slash}", "/");

                    if !unit_full_name.starts_with(&name) {
                        return Err(self.error_here(format!(
                            "invalid symbol unit name prefix {unit_full_name}"
                        )));
                    }

                    let suffix = unit_full_name
                        .strip_prefix(&name)
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

                    let mut unit_name = None;
                    let mut draw_item_kinds = Vec::new();
                    let mut draw_items = Vec::new();

                    while !self.at_right() {
                        self.need_left()?;
                        let head = self.need_unquoted_symbol_atom(
                            "arc, bezier, circle, pin, polyline, rectangle, or text",
                        )?;
                        match head.as_str() {
                            "unit_name" => {
                                if matches!(
                                    self.current().atom_class,
                                    Some(AtomClass::Symbol | AtomClass::Quoted)
                                ) {
                                    unit_name = Some(self.need_symbol_atom("unit_name")?);
                                }
                                self.need_right()?;
                            }
                            "arc" | "bezier" | "circle" | "pin" | "polyline" | "rectangle"
                            | "text" | "text_box" => {
                                let item = match head.as_str() {
                                    "arc" => self.parse_lib_arc_draw_item(unit_number, body_style),
                                    "bezier" => {
                                        self.parse_lib_bezier_draw_item(unit_number, body_style)
                                    }
                                    "circle" => {
                                        self.parse_lib_circle_draw_item(unit_number, body_style)
                                    }
                                    "polyline" => {
                                        self.parse_lib_polyline_draw_item(unit_number, body_style)
                                    }
                                    "rectangle" => self
                                        .parse_lib_rectangle_draw_item(unit_number, body_style),
                                    "text" => self.parse_lib_text_draw_item(unit_number, body_style),
                                    "text_box" => {
                                        self.parse_lib_text_box_draw_item(unit_number, body_style)
                                    }
                                    "pin" => self.parse_lib_pin_draw_item(unit_number, body_style),
                                    _ => Err(self.expecting(
                                        "arc, bezier, circle, pin, polyline, rectangle, text, or text_box",
                                    )),
                                }?;
                                self.need_right()?;
                                draw_item_kinds.push(head.to_string());
                                draw_items.push(item);
                            }
                            _ => {
                                return Err(self.expecting(
                                    "arc, bezier, circle, pin, polyline, rectangle, or text",
                                ));
                            }
                        }
                    }

                    units.push(crate::model::LibSymbolUnit {
                        name: unit_full_name,
                        unit_number,
                        body_style,
                        unit_name,
                        draw_item_kinds,
                        draw_items,
                    });
                    self.need_right()?;
                }
                kind @ ("arc" | "bezier" | "circle" | "pin" | "polyline" | "rectangle"
                | "text" | "text_box") => {
                    let item = match kind {
                        "arc" => self.parse_lib_arc_draw_item(1, 1),
                        "bezier" => self.parse_lib_bezier_draw_item(1, 1),
                        "circle" => self.parse_lib_circle_draw_item(1, 1),
                        "polyline" => self.parse_lib_polyline_draw_item(1, 1),
                        "rectangle" => self.parse_lib_rectangle_draw_item(1, 1),
                        "text" => self.parse_lib_text_draw_item(1, 1),
                        "text_box" => self.parse_lib_text_box_draw_item(1, 1),
                        "pin" => self.parse_lib_pin_draw_item(1, 1),
                        _ => Err(self.expecting(
                            "arc, bezier, circle, pin, polyline, rectangle, text, or text_box",
                        )),
                    }?;
                    self.need_right()?;

                    if let Some(unit) = units.iter_mut().find(|unit| {
                        unit.unit_number == 1
                            && unit.body_style == 1
                            && unit.name == format!("{name}_{}_{}", 1, 1)
                    }) {
                        unit.draw_item_kinds.push(item.kind.clone());
                        unit.draw_items.push(item);
                    } else {
                        units.push(crate::model::LibSymbolUnit {
                            name: format!("{name}_{}_{}", 1, 1),
                            unit_number: 1,
                            body_style: 1,
                            unit_name: None,
                            draw_item_kinds: vec![item.kind.clone()],
                            draw_items: vec![item],
                        });
                    }
                }
                "embedded_fonts" => {
                    embedded_fonts = Some(self.parse_bool_atom("embedded_fonts")?);
                    self.need_right()?;
                }
                "embedded_files" => {
                    let block_depth = self.current_nesting_depth();
                    match (|| -> Result<Vec<EmbeddedFile>, Error> {
                        let mut files = Vec::new();

                        while !self.at_right() {
                            self.need_left()?;
                            let head = self.need_unquoted_symbol_atom("file")?;
                            if head != "file" {
                                return Err(self.expecting("file"));
                            }
                            let mut name = None;
                            let mut data = None;

                            if self.at_atom() {
                                name = Some(self.need_atom()?);
                            }
                            if self.at_atom() {
                                data = Some(self.need_atom()?);
                            }

                            while !self.at_right() {
                                self.need_left()?;
                                let head = self.need_unquoted_symbol_atom("name or data")?;
                                match head.as_str() {
                                    "name" => name = Some(self.parse_string_atom("name")?),
                                    "data" => data = Some(self.parse_string_atom("data")?),
                                    _ => return Err(self.expecting("name or data")),
                                }
                                self.need_right()?;
                            }

                            let file = EmbeddedFile { name, data };
                            self.need_right()?;
                            files.push(file);
                        }

                        Ok(files)
                    })() {
                        Ok(files) => embedded_files = files,
                        Err(err) => {
                            self.screen.parse_warnings.push(err.to_string());
                            self.skip_to_block_right(block_depth);
                        }
                    }
                    self.need_right()?;
                }
                _ => {
                    return Err(self.expecting(
                        "pin_names, pin_numbers, arc, bezier, circle, pin, polyline, rectangle, or text",
                    ))
                }
            }
        }

        Ok(LibSymbol {
            name,
            extends,
            power,
            local_power,
            body_style_names,
            has_demorgan,
            pin_name_offset,
            show_pin_names,
            show_pin_numbers,
            excluded_from_sim,
            in_bom,
            on_board,
            in_pos_files,
            duplicate_pin_numbers_are_jumpers,
            jumper_pin_groups,
            keywords,
            description,
            fp_filters,
            locked_units,
            properties,
            units,
            embedded_fonts,
            embedded_files,
        })
    }

    fn parse_lib_arc_draw_item(
        &mut self,
        unit_number: i32,
        body_style: i32,
    ) -> Result<LibDrawItem, Error> {
        let mut is_private = false;
        if self.at_unquoted_symbol_with("private") {
            let _ = self.need_unquoted_symbol_atom("private")?;
            is_private = true;
        }
        let mut item = LibDrawItem {
            kind: "arc".to_string(),
            is_private,
            unit_number,
            body_style,
            visible: true,
            at: None,
            angle: None,
            points: vec![[1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            end: None,
            radius: None,
            arc_center: Some([0.0, 0.0]),
            arc_start_angle: Some(0.0),
            arc_end_angle: Some(90.0),
            length: None,
            text: None,
            name: None,
            number: None,
            name_effects: None,
            number_effects: None,
            electrical_type: None,
            graphic_shape: None,
            alternates: Vec::new(),
            stroke: None,
            fill: None,
            effects: None,
            converted_to_field: false,
        };
        let mut saw_start = false;
        let mut saw_mid = false;
        let mut saw_end = false;

        while !self.at_right() {
            self.need_left()?;
            match self
                .need_unquoted_symbol_atom("start, mid, end, radius, stroke, or fill")?
                .as_str()
            {
                "start" => {
                    item.points[0] = self.parse_xy2("arc start")?;
                    saw_start = true;
                    self.need_right()?;
                }
                "mid" => {
                    item.points[1] = self.parse_xy2("arc mid")?;
                    saw_mid = true;
                    self.need_right()?;
                }
                "end" => {
                    item.points[2] = self.parse_xy2("arc end")?;
                    saw_end = true;
                    self.need_right()?;
                }
                "radius" => {
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
                "stroke" => item.stroke = Some(self.parse_stroke()?),
                "fill" => item.fill = Some(self.parse_fill()?),
                _ => return Err(self.expecting("start, mid, end, radius, stroke, or fill")),
            }
        }

        if !saw_mid {
            item.points.remove(1);
        } else if !saw_start || !saw_end {
            // keep defaults when an explicit midpoint path only partially specifies endpoints
        }

        Ok(item)
    }

    fn parse_lib_bezier_draw_item(
        &mut self,
        unit_number: i32,
        body_style: i32,
    ) -> Result<LibDrawItem, Error> {
        let mut is_private = false;
        if self.at_unquoted_symbol_with("private") {
            let _ = self.need_unquoted_symbol_atom("private")?;
            is_private = true;
        }
        let mut item = LibDrawItem {
            kind: "bezier".to_string(),
            is_private,
            unit_number,
            body_style,
            visible: true,
            at: None,
            angle: None,
            points: Vec::new(),
            end: None,
            radius: None,
            arc_center: None,
            arc_start_angle: None,
            arc_end_angle: None,
            length: None,
            text: None,
            name: None,
            number: None,
            name_effects: None,
            number_effects: None,
            electrical_type: None,
            graphic_shape: None,
            alternates: Vec::new(),
            stroke: None,
            fill: None,
            effects: None,
            converted_to_field: false,
        };

        while !self.at_right() {
            self.need_left()?;
            match self
                .need_unquoted_symbol_atom("pts, stroke, or fill")?
                .as_str()
            {
                "pts" => {
                    let mut points = Vec::new();
                    while !self.at_right() {
                        self.need_left()?;
                        if self.need_unquoted_symbol_atom("xy")? != "xy" {
                            return Err(self.expecting("xy"));
                        }
                        if points.len() >= 4 {
                            return Err(self.error_here("unexpected control point"));
                        }
                        points.push(self.parse_xy2("bezier point")?);
                        self.need_right()?;
                    }
                    item.points = points;
                    self.need_right()?;
                }
                "stroke" => item.stroke = Some(self.parse_stroke()?),
                "fill" => item.fill = Some(self.parse_fill()?),
                _ => return Err(self.expecting("pts, stroke, or fill")),
            }
        }

        Ok(item)
    }

    fn parse_lib_circle_draw_item(
        &mut self,
        unit_number: i32,
        body_style: i32,
    ) -> Result<LibDrawItem, Error> {
        let mut is_private = false;
        if self.at_unquoted_symbol_with("private") {
            let _ = self.need_unquoted_symbol_atom("private")?;
            is_private = true;
        }
        let mut item = LibDrawItem {
            kind: "circle".to_string(),
            is_private,
            unit_number,
            body_style,
            visible: true,
            at: None,
            angle: None,
            points: vec![[0.0, 0.0]],
            end: None,
            radius: Some(1.0),
            arc_center: None,
            arc_start_angle: None,
            arc_end_angle: None,
            length: None,
            text: None,
            name: None,
            number: None,
            name_effects: None,
            number_effects: None,
            electrical_type: None,
            graphic_shape: None,
            alternates: Vec::new(),
            stroke: None,
            fill: None,
            effects: None,
            converted_to_field: false,
        };

        while !self.at_right() {
            self.need_left()?;
            match self
                .need_unquoted_symbol_atom("center, radius, stroke, or fill")?
                .as_str()
            {
                "center" => {
                    item.points[0] = self.parse_xy2("circle center")?;
                    self.need_right()?;
                }
                "radius" => {
                    item.radius = Some(self.parse_f64_atom("radius length")?);
                    self.need_right()?;
                }
                "stroke" => item.stroke = Some(self.parse_stroke()?),
                "fill" => item.fill = Some(self.parse_fill()?),
                _ => return Err(self.expecting("center, radius, stroke, or fill")),
            }
        }

        Ok(item)
    }

    fn parse_lib_polyline_draw_item(
        &mut self,
        unit_number: i32,
        body_style: i32,
    ) -> Result<LibDrawItem, Error> {
        let mut is_private = false;
        if self.at_unquoted_symbol_with("private") {
            let _ = self.need_unquoted_symbol_atom("private")?;
            is_private = true;
        }
        let mut item = LibDrawItem {
            kind: "polyline".to_string(),
            is_private,
            unit_number,
            body_style,
            visible: true,
            at: None,
            angle: None,
            points: Vec::new(),
            end: None,
            radius: None,
            arc_center: None,
            arc_start_angle: None,
            arc_end_angle: None,
            length: None,
            text: None,
            name: None,
            number: None,
            name_effects: None,
            number_effects: None,
            electrical_type: None,
            graphic_shape: None,
            alternates: Vec::new(),
            stroke: None,
            fill: None,
            effects: None,
            converted_to_field: false,
        };

        while !self.at_right() {
            self.need_left()?;
            match self
                .need_unquoted_symbol_atom("pts, stroke, or fill")?
                .as_str()
            {
                "pts" => {
                    let mut points = Vec::new();
                    while !self.at_right() {
                        self.need_left()?;
                        let head = self.need_unquoted_symbol_atom("xy")?;
                        if head != "xy" {
                            return Err(self.expecting("xy"));
                        }
                        points.push(self.parse_xy2("xy")?);
                        self.need_right()?;
                    }
                    item.points = points;
                    self.need_right()?;
                }
                "stroke" => item.stroke = Some(self.parse_stroke()?),
                "fill" => item.fill = Some(self.parse_fill()?),
                _ => return Err(self.expecting("pts, stroke, or fill")),
            }
        }

        Ok(item)
    }

    fn parse_lib_rectangle_draw_item(
        &mut self,
        unit_number: i32,
        body_style: i32,
    ) -> Result<LibDrawItem, Error> {
        let mut is_private = false;
        if self.at_unquoted_symbol_with("private") {
            let _ = self.need_unquoted_symbol_atom("private")?;
            is_private = true;
        }
        let mut item = LibDrawItem {
            kind: "rectangle".to_string(),
            is_private,
            unit_number,
            body_style,
            visible: true,
            at: None,
            angle: None,
            points: Vec::new(),
            end: None,
            radius: None,
            arc_center: None,
            arc_start_angle: None,
            arc_end_angle: None,
            length: None,
            text: None,
            name: None,
            number: None,
            name_effects: None,
            number_effects: None,
            electrical_type: None,
            graphic_shape: None,
            alternates: Vec::new(),
            stroke: None,
            fill: None,
            effects: None,
            converted_to_field: false,
        };

        while !self.at_right() {
            self.need_left()?;
            match self
                .need_unquoted_symbol_atom("start, end, stroke, or fill")?
                .as_str()
            {
                "start" => {
                    item.points.push(self.parse_xy2("rectangle start")?);
                    self.need_right()?;
                }
                "end" => {
                    item.end = Some(self.parse_xy2("rectangle end")?);
                    self.need_right()?;
                }
                "radius" => {
                    item.radius = Some(self.parse_f64_atom("corner radius")?);
                    self.need_right()?;
                }
                "stroke" => item.stroke = Some(self.parse_stroke()?),
                "fill" => item.fill = Some(self.parse_fill()?),
                _ => return Err(self.expecting("start, end, stroke, or fill")),
            }
        }

        Ok(item)
    }

    fn parse_lib_text_draw_item(
        &mut self,
        unit_number: i32,
        body_style: i32,
    ) -> Result<LibDrawItem, Error> {
        let mut is_private = false;
        if self.at_unquoted_symbol_with("private") {
            let _ = self.need_unquoted_symbol_atom("private")?;
            is_private = true;
        }

        let mut text = self
            .need_symbol_atom("text string")
            .map_err(|_| self.error_here("Invalid text string"))?;
        if self.version.unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION) < VERSION_TEXT_OVERBAR_NOTATION {
            text = self.convert_to_new_overbar_notation(text);
        }
        let mut at = None;
        let mut angle = None;
        let mut visible = true;
        let mut effects = None;

        while !self.at_right() {
            self.need_left()?;
            match self.need_unquoted_symbol_atom("at or effects")?.as_str() {
                "at" => {
                    let parsed = self.parse_xy3("text at")?;
                    at = Some([parsed[0], parsed[1]]);
                    angle = Some(parsed[2] / 10.0);
                    self.need_right()?;
                }
                "effects" => {
                    let parsed = self.parse_eda_text()?;
                    visible = !parsed.hidden;
                    effects = Some(parsed);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("at or effects")),
            }
        }

        Ok(LibDrawItem {
            kind: "text".to_string(),
            is_private,
            unit_number,
            body_style,
            visible,
            at,
            angle,
            points: Vec::new(),
            end: None,
            radius: None,
            arc_center: None,
            arc_start_angle: None,
            arc_end_angle: None,
            length: None,
            text: Some(text),
            name: None,
            number: None,
            name_effects: None,
            number_effects: None,
            electrical_type: None,
            graphic_shape: None,
            alternates: Vec::new(),
            stroke: None,
            fill: None,
            effects,
            converted_to_field: !visible,
        })
    }

    fn parse_lib_text_box_draw_item(
        &mut self,
        unit_number: i32,
        body_style: i32,
    ) -> Result<LibDrawItem, Error> {
        let mut is_private = false;
        if self.at_unquoted_symbol_with("private") {
            let _ = self.need_unquoted_symbol_atom("private")?;
            is_private = true;
        }
        let text = self
            .need_symbol_atom("text box text")
            .map_err(|_| self.error_here("Invalid text string"))?;
        let mut at = None;
        let mut angle = 0.0;
        let mut end = None;
        let mut size = None;
        let mut has_effects = false;
        let mut effects = None;
        let mut stroke = None;
        let mut fill = None;
        let mut margins = None;
        let mut stroke_width = None;
        let mut text_size_y = None;

        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom("at, size, stroke, fill or effects")?;
            match head.as_str() {
                "start" => {
                    at = Some(self.parse_xy2("text_box start")?);
                    self.need_right()?;
                }
                "end" => {
                    end = Some(self.parse_xy2("text_box end")?);
                    self.need_right()?;
                }
                "at" => {
                    let parsed = self.parse_xy3("text_box at")?;
                    at = Some([parsed[0], parsed[1]]);
                    angle = parsed[2];
                    self.need_right()?;
                }
                "size" => {
                    size = Some(self.parse_xy2("text_box size")?);
                    self.need_right()?;
                }
                "stroke" => {
                    let parsed_stroke = self.parse_stroke()?;
                    stroke_width = parsed_stroke.width;
                    stroke = Some(parsed_stroke);
                }
                "fill" => {
                    fill = Some(self.parse_fill()?);
                }
                "effects" => {
                    let parsed_effects = self.parse_eda_text()?;
                    has_effects = true;
                    text_size_y = parsed_effects.font_size.map(|size| size[1]);
                    effects = Some(parsed_effects);
                    self.need_right()?;
                }
                "margins" => {
                    margins = Some([
                        self.parse_f64_atom("margin left")?,
                        self.parse_f64_atom("margin top")?,
                        self.parse_f64_atom("margin right")?,
                        self.parse_f64_atom("margin bottom")?,
                    ]);
                    self.need_right()?;
                }
                _ => {
                    return Err(self.expecting("at, size, stroke, fill or effects"));
                }
            }
        }

        let at = at.unwrap_or([0.0, 0.0]);
        let end = match (end, size) {
            (Some(end), _) => end,
            (None, Some(size)) => [at[0] + size[0], at[1] + size[1]],
            (None, None) => return Err(self.expecting("size")),
        };
        let margins = margins.or_else(|| {
            let margin = Self::legacy_text_box_margin(
                stroke_width.unwrap_or(DEFAULT_LINE_WIDTH_MM),
                text_size_y.unwrap_or(DEFAULT_TEXT_SIZE_MM),
            );
            Some([margin, margin, margin, margin])
        });
        let visible = !effects
            .as_ref()
            .map(|effects| effects.hidden)
            .unwrap_or(false);
        let _ = has_effects;
        let _ = margins;

        Ok(LibDrawItem {
            kind: "text_box".to_string(),
            is_private,
            unit_number,
            body_style,
            visible,
            at: Some(at),
            angle: Some(angle),
            points: Vec::new(),
            end: Some(end),
            radius: None,
            arc_center: None,
            arc_start_angle: None,
            arc_end_angle: None,
            length: None,
            text: Some(text),
            name: None,
            number: None,
            name_effects: None,
            number_effects: None,
            electrical_type: None,
            graphic_shape: None,
            alternates: Vec::new(),
            stroke,
            fill,
            effects,
            converted_to_field: false,
        })
    }

    fn parse_lib_pin_draw_item(
        &mut self,
        unit_number: i32,
        body_style: i32,
    ) -> Result<LibDrawItem, Error> {
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
        let mut at = None;
        let mut angle = None;
        let mut length = None;
        let mut visible = true;
        let mut name = None;
        let mut number = None;
        let mut name_effects = None;
        let mut number_effects = None;
        let mut alternates = Vec::new();

        while !self.at_right() {
            if self.at_unquoted_symbol_with("hide") {
                let _ = self.need_unquoted_symbol_atom("hide")?;
                visible = false;
                continue;
            }

            self.need_left()?;
            match self
                .need_unquoted_symbol_atom("at, name, number, hide, length, or alternate")?
                .as_str()
            {
                "at" => {
                    let parsed = self.parse_xy3("pin at")?;
                    match parsed[2] as i32 {
                        0 | 90 | 180 | 270 => {}
                        _ => return Err(self.expecting("0, 90, 180, or 270")),
                    }
                    at = Some([parsed[0], parsed[1]]);
                    angle = Some(parsed[2]);
                    self.need_right()?;
                }
                "length" => {
                    length = Some(self.parse_f64_atom("pin length")?);
                    self.need_right()?;
                }
                "hide" => {
                    visible = !self.parse_bool_atom("hide")?;
                    self.need_right()?;
                }
                "name" => {
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
                        parsed = self.convert_to_new_overbar_notation(parsed);
                    }
                    name = Some(parsed);
                    if self.at_right() {
                        self.need_right()?;
                        continue;
                    }
                    self.need_left()?;
                    if self.need_unquoted_symbol_atom("effects")? != "effects" {
                        return Err(self.expecting("effects"));
                    }
                    let parsed = self.parse_eda_text()?;
                    name_effects = Some(parsed);
                    self.need_right()?;
                    self.need_right()?;
                }
                "number" => {
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
                        parsed = self.convert_to_new_overbar_notation(parsed);
                    }
                    number = Some(parsed);
                    if self.at_right() {
                        self.need_right()?;
                        continue;
                    }
                    self.need_left()?;
                    if self.need_unquoted_symbol_atom("effects")? != "effects" {
                        return Err(self.expecting("effects"));
                    }
                    let parsed = self.parse_eda_text()?;
                    number_effects = Some(parsed);
                    self.need_right()?;
                    self.need_right()?;
                }
                "alternate" => {
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
                        alt_name = self.convert_to_new_overbar_notation(alt_name);
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
                    alternates.push(LibPinAlternate {
                        name: alt_name,
                        electrical_type: alt_type,
                        graphic_shape: alt_shape,
                    });
                    self.need_right()?;
                }
                _ => return Err(self.expecting("at, name, number, hide, length, or alternate")),
            }
        }

        Ok(LibDrawItem {
            kind: "pin".to_string(),
            is_private: false,
            unit_number,
            body_style,
            visible,
            at,
            angle,
            points: Vec::new(),
            end: None,
            radius: None,
            arc_center: None,
            arc_start_angle: None,
            arc_end_angle: None,
            length,
            text: None,
            name,
            number,
            name_effects,
            number_effects,
            electrical_type: Some(electrical_type),
            graphic_shape: Some(graphic_shape),
            alternates,
            stroke: None,
            fill: None,
            effects: None,
            converted_to_field: false,
        })
    }

    fn parse_lib_property(&mut self) -> Result<Property, Error> {
        let mut is_private = false;
        if self.at_unquoted_symbol_with("private") {
            let _ = self.need_unquoted_symbol_atom("private")?;
            is_private = true;
        }
        let key = self
            .need_symbol_atom("property name")
            .map_err(|_| self.error_here("Invalid property name"))?;
        if key.is_empty() {
            return Err(self.error_here("Empty property name"));
        }
        let value = self
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
        let key_lower = key.to_ascii_lowercase();
        let kind = match key_lower.as_str() {
            "reference" => PropertyKind::SymbolReference,
            "value" => PropertyKind::SymbolValue,
            "footprint" => PropertyKind::SymbolFootprint,
            "datasheet" => PropertyKind::SymbolDatasheet,
            _ => PropertyKind::User,
        };
        let key = match kind {
            PropertyKind::SymbolReference => "Reference".to_string(),
            PropertyKind::SymbolValue => "Value".to_string(),
            PropertyKind::SymbolFootprint => "Footprint".to_string(),
            PropertyKind::SymbolDatasheet => "Datasheet".to_string(),
            _ => key,
        };
        let mut property = Property {
            key,
            value,
            kind,
            is_private: matches!(kind, PropertyKind::User) && is_private,
            at: None,
            angle: None,
            visible: true,
            show_name: true,
            can_autoplace: true,
            has_effects: false,
            effects: None,
        };

        while !self.at_right() {
            self.need_left()?;
            match self
                .need_unquoted_symbol_atom("id, at, hide, show_name, do_not_autoplace, or effects")?
                .as_str()
            {
                "id" => {
                    let _ = self.parse_i32_atom("field ID")?;
                    self.need_right()?;
                }
                "at" => {
                    let parsed = self.parse_xy3("property at")?;
                    property.at = Some([parsed[0], parsed[1]]);
                    property.angle = Some(parsed[2]);
                    self.need_right()?;
                }
                "hide" => {
                    property.visible = !self.parse_bool_atom("hide")?;
                    self.need_right()?;
                }
                "show_name" => {
                    property.show_name = self.parse_maybe_absent_bool(true)?;
                    self.need_right()?;
                }
                "do_not_autoplace" => {
                    property.can_autoplace = !self.parse_maybe_absent_bool(true)?;
                    self.need_right()?;
                }
                "effects" => {
                    let effects = self.parse_eda_text()?;
                    property.has_effects = true;
                    if effects.hidden {
                        property.visible = false;
                    }
                    property.effects = Some(effects);
                    self.need_right()?;
                }
                _ => {
                    return Err(
                        self.expecting("id, at, hide, show_name, do_not_autoplace, or effects")
                    );
                }
            }
        }

        Ok(property)
    }

    fn parse_bus_alias(&mut self) -> Result<(), Error> {
        let mut name = self.need_symbol_atom("bus alias name")?;
        let version = self.require_known_version()?;
        if version < VERSION_NEW_OVERBAR_NOTATION {
            name = self.convert_to_new_overbar_notation(name);
        }

        self.need_left()?;
        if self.need_unquoted_symbol_atom("members")? != "members" {
            return Err(self.expecting("members"));
        }

        let mut members = Vec::new();
        while !self.at_right() {
            let mut member = self.need_quoted_atom("quoted string")?;
            if version < VERSION_NEW_OVERBAR_NOTATION {
                member = self.convert_to_new_overbar_notation(member);
            }
            members.push(member);
        }
        self.need_right()?;
        self.screen.bus_aliases.push(BusAlias { name, members });
        Ok(())
    }

    fn parse_junction(&mut self) -> Result<Junction, Error> {
        let mut at = None;
        let mut diameter = None;
        let mut color = None;
        let mut uuid = None;
        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom("at, diameter, color or uuid")?;
            match head.as_str() {
                "at" => {
                    at = Some(self.parse_xy2("junction at")?);
                    self.need_right()?;
                }
                "diameter" => {
                    diameter = Some(self.parse_f64_atom("junction diameter")?);
                    self.need_right()?;
                }
                "color" => {
                    color = Some([
                        f64::from(self.parse_i32_atom("red")?) / 255.0,
                        f64::from(self.parse_i32_atom("green")?) / 255.0,
                        f64::from(self.parse_i32_atom("blue")?) / 255.0,
                        self.parse_f64_atom("alpha")?.clamp(0.0, 1.0),
                    ]);
                    self.need_right()?;
                }
                "uuid" => {
                    uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("at, diameter, color or uuid")),
            }
        }
        Ok(Junction {
            at: at.unwrap_or([0.0, 0.0]),
            diameter,
            color,
            uuid,
        })
    }

    fn parse_no_connect(&mut self) -> Result<NoConnect, Error> {
        let mut at = None;
        let mut uuid = None;
        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom("at or uuid")?;
            match head.as_str() {
                "at" => {
                    at = Some(self.parse_xy2("no_connect at")?);
                    self.need_right()?;
                }
                "uuid" => {
                    uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("at or uuid")),
            }
        }
        Ok(NoConnect {
            at: at.unwrap_or([0.0, 0.0]),
            uuid,
        })
    }

    fn parse_bus_entry(&mut self) -> Result<BusEntry, Error> {
        let mut at = None;
        let mut size = None;
        let mut has_stroke = false;
        let mut stroke = None;
        let mut uuid = None;
        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom("at, size, uuid or stroke")?;
            match head.as_str() {
                "at" => {
                    at = Some(self.parse_xy2("bus_entry at")?);
                    self.need_right()?;
                }
                "size" => {
                    size = Some(self.parse_xy2("bus_entry size")?);
                    self.need_right()?;
                }
                "stroke" => {
                    has_stroke = true;
                    let mut parsed_stroke = self.parse_stroke()?;
                    if self.require_known_version()? <= 20211123
                        && parsed_stroke.style == StrokeStyle::Default
                    {
                        parsed_stroke.style = StrokeStyle::Dash;
                    }
                    stroke = Some(parsed_stroke);
                }
                "uuid" => {
                    uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("at, size, uuid or stroke")),
            }
        }
        Ok(BusEntry {
            at: at.unwrap_or([0.0, 0.0]),
            size: size.unwrap_or([0.0, 0.0]),
            has_stroke,
            stroke,
            uuid,
        })
    }

    fn parse_sch_line(&mut self, kind: LineKind) -> Result<Line, Error> {
        let mut points = Vec::new();
        let mut has_stroke = false;
        let mut stroke = None;
        let mut uuid = None;
        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom("at, uuid or stroke")?;
            match head.as_str() {
                "pts" => {
                    self.need_left()?;
                    if self.need_unquoted_symbol_atom("xy")? != "xy" {
                        return Err(self.expecting("xy"));
                    }
                    let start = self.parse_xy2("xy")?;
                    self.need_right()?;
                    self.need_left()?;
                    if self.need_unquoted_symbol_atom("xy")? != "xy" {
                        return Err(self.expecting("xy"));
                    }
                    let end = self.parse_xy2("xy")?;
                    self.need_right()?;
                    self.need_right()?;
                    points = vec![start, end];
                }
                "uuid" => {
                    uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                "stroke" => {
                    has_stroke = true;
                    let mut parsed_stroke = self.parse_stroke()?;
                    if self.require_known_version()? <= 20211123
                        && parsed_stroke.style == StrokeStyle::Default
                    {
                        parsed_stroke.style = StrokeStyle::Dash;
                    }
                    stroke = Some(parsed_stroke);
                }
                _ => return Err(self.expecting("at, uuid or stroke")),
            }
        }
        Ok(Line {
            kind,
            points: if points.is_empty() {
                vec![[0.0, 0.0], [0.0, 0.0]]
            } else {
                points
            },
            has_stroke,
            stroke,
            uuid,
        })
    }

    fn parse_sch_text(&mut self, kind: &str) -> Result<SchItem, Error> {
        let target = match kind {
            "text" => SchTextTarget::Text,
            "label" => SchTextTarget::Label(LabelKind::Local),
            "global_label" => SchTextTarget::Label(LabelKind::Global),
            "hierarchical_label" => SchTextTarget::Label(LabelKind::Hierarchical),
            "directive_label" => SchTextTarget::Label(LabelKind::Directive),
            "class_label" | "netclass_flag" => SchTextTarget::Label(LabelKind::NetclassFlag),
            _ => return Err(self.error_here(format!("invalid schematic text kind `{kind}`"))),
        };

        let mut text = self
            .need_symbol_atom("text value")
            .map_err(|_| self.error_here("Invalid text string"))?;
        if self.version.unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION) < VERSION_TEXT_OVERBAR_NOTATION {
            text = self.convert_to_new_overbar_notation(text);
        }

        let mut at = None;
        let mut shape = None;
        let mut pin_length = None;
        let mut iref_at = None;
        let mut excluded_from_sim = false;
        let mut fields_autoplaced = false;
        let mut visible = true;
        let mut has_effects = false;
        let mut effects = None;
        let mut uuid = None;
        let mut properties: Vec<Property> = Vec::new();

        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom("at, shape, iref, uuid or effects")?;
            match head.as_str() {
                "exclude_from_sim" => {
                    excluded_from_sim = self.parse_bool_atom("exclude_from_sim")?;
                    self.need_right()?;
                }
                "at" => {
                    let parsed = self.parse_xy3("text at")?;
                    at = Some([parsed[0], parsed[1], Self::keep_upright_angle(parsed[2])]);
                    self.need_right()?;
                }
                "shape" => {
                    let SchTextTarget::Label(label_kind) = target else {
                        return Err(self.unexpected("shape"));
                    };
                    if matches!(label_kind, LabelKind::Local) {
                        return Err(self.unexpected("shape"));
                    }
                    shape = Some(match self.need_unquoted_symbol_atom("shape")?.as_str() {
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
                    });
                    self.need_right()?;
                }
                "length" => {
                    let SchTextTarget::Label(label_kind) = target else {
                        return Err(self.unexpected("length"));
                    };
                    if !matches!(label_kind, LabelKind::Directive | LabelKind::NetclassFlag) {
                        return Err(self.unexpected("length"));
                    }
                    pin_length = Some(self.parse_f64_atom("pin length")?);
                    self.need_right()?;
                }
                "fields_autoplaced" => {
                    fields_autoplaced = self.parse_maybe_absent_bool(true)?;
                    self.need_right()?;
                }
                "effects" => {
                    let parsed_effects = self.parse_eda_text()?;
                    has_effects = true;
                    self.need_right()?;
                    effects = Some(parsed_effects);
                    visible = true;
                }
                "iref" => {
                    if matches!(target, SchTextTarget::Label(LabelKind::Global)) {
                        iref_at = Some(self.parse_xy2("iref")?);
                        self.need_right()?;
                        let property = Property {
                            key: "Intersheet References".to_string(),
                            value: String::new(),
                            kind: PropertyKind::GlobalLabelIntersheetRefs,
                            is_private: false,
                            at: iref_at,
                            angle: None,
                            visible: true,
                            show_name: true,
                            can_autoplace: true,
                            has_effects: false,
                            effects: None,
                        };
                        if let Some(existing) = properties
                            .iter_mut()
                            .find(|p| p.kind == PropertyKind::GlobalLabelIntersheetRefs)
                        {
                            *existing = property;
                        } else {
                            properties.push(property);
                        }
                    }
                }
                "uuid" => {
                    uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                "property" => {
                    let SchTextTarget::Label(label_kind) = target else {
                        return Err(self.unexpected("property"));
                    };
                    if matches!(label_kind, LabelKind::Global) {
                        let property = self.parse_sch_field(FieldParent::GlobalLabel)?;
                        if let Some(existing) =
                            properties.iter_mut().find(|p| p.kind == property.kind)
                        {
                            *existing = property;
                        } else {
                            properties.push(property);
                        }
                    } else {
                        let property = self.parse_sch_field(FieldParent::OtherLabel)?;
                        properties.push(property);
                    }
                    self.need_right()?;
                }
                _ => return Err(self.expecting("at, shape, iref, uuid or effects")),
            }
        }

        match target {
            SchTextTarget::Text => Ok(SchItem::Text(Text {
                kind: TextKind::Text,
                text,
                at,
                excluded_from_sim,
                fields_autoplaced,
                visible,
                has_effects,
                effects,
                uuid,
            })),
            SchTextTarget::Label(kind) => {
                let [x, y, angle] = at.unwrap_or([0.0, 0.0, 0.0]);
                Ok(SchItem::Label(Label {
                    kind,
                    text,
                    at: [x, y],
                    angle,
                    spin: Self::label_spin_from_angle(angle),
                    shape,
                    pin_length,
                    iref_at,
                    excluded_from_sim,
                    fields_autoplaced: if properties.is_empty() {
                        true
                    } else {
                        fields_autoplaced
                    },
                    visible,
                    has_effects,
                    effects,
                    uuid,
                    properties,
                }))
            }
        }
    }

    fn parse_sch_text_box(&mut self) -> Result<TextBox, Error> {
        self.parse_sch_text_box_content(false)
    }

    fn parse_sch_table_cell(&mut self) -> Result<TextBox, Error> {
        self.parse_sch_text_box_content(true)
    }

    fn parse_sch_text_box_content(&mut self, table_cell: bool) -> Result<TextBox, Error> {
        let text = self
            .need_symbol_atom("text box text")
            .map_err(|_| self.error_here("Invalid text string"))?;
        let mut at = None;
        let mut angle = 0.0;
        let mut end = None;
        let mut size = None;
        let mut excluded_from_sim = false;
        let mut has_effects = false;
        let mut effects = None;
        let mut stroke = None;
        let mut fill = None;
        let mut span = None;
        let mut margins = None;
        let mut uuid = None;
        let mut stroke_width = None;
        let mut text_size_y = None;

        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom(if table_cell {
                "at, size, stroke, fill, effects, span or uuid"
            } else {
                "at, size, stroke, fill, effects or uuid"
            })?;
            match head.as_str() {
                "exclude_from_sim" => {
                    excluded_from_sim = self.parse_bool_atom("exclude_from_sim")?;
                    self.need_right()?;
                }
                "start" => {
                    at = Some(self.parse_xy2("text_box start")?);
                    self.need_right()?;
                }
                "end" => {
                    end = Some(self.parse_xy2("text_box end")?);
                    self.need_right()?;
                }
                "at" => {
                    let parsed = self.parse_xy3("text_box at")?;
                    at = Some([parsed[0], parsed[1]]);
                    angle = parsed[2];
                    self.need_right()?;
                }
                "size" => {
                    size = Some(self.parse_xy2("text_box size")?);
                    self.need_right()?;
                }
                "span" if table_cell => {
                    span = Some([
                        self.parse_i32_atom("column span")?,
                        self.parse_i32_atom("row span")?,
                    ]);
                    self.need_right()?;
                }
                "stroke" => {
                    let parsed_stroke = self.parse_stroke()?;
                    stroke_width = parsed_stroke.width;
                    stroke = Some(parsed_stroke);
                }
                "fill" => {
                    fill = Some(self.parse_fill()?);
                }
                "effects" => {
                    let parsed_effects = self.parse_eda_text()?;
                    has_effects = true;
                    text_size_y = parsed_effects.font_size.map(|size| size[1]);
                    effects = Some(parsed_effects);
                    self.need_right()?;
                }
                "margins" => {
                    margins = Some([
                        self.parse_f64_atom("margin left")?,
                        self.parse_f64_atom("margin top")?,
                        self.parse_f64_atom("margin right")?,
                        self.parse_f64_atom("margin bottom")?,
                    ]);
                    self.need_right()?;
                }
                "uuid" => {
                    uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                _ => {
                    return Err(self.expecting(if table_cell {
                        "at, size, stroke, fill, effects, span or uuid"
                    } else {
                        "at, size, stroke, fill, effects or uuid"
                    }));
                }
            }
        }

        let at = at.unwrap_or([0.0, 0.0]);
        let end = match (end, size) {
            (Some(end), _) => end,
            (None, Some(size)) => [at[0] + size[0], at[1] + size[1]],
            (None, None) => return Err(self.expecting("size")),
        };
        let margins = margins.or_else(|| {
            let margin = Self::legacy_text_box_margin(
                stroke_width.unwrap_or(DEFAULT_LINE_WIDTH_MM),
                text_size_y.unwrap_or(DEFAULT_TEXT_SIZE_MM),
            );
            Some([margin, margin, margin, margin])
        });

        Ok(TextBox {
            text,
            at,
            angle,
            end,
            excluded_from_sim,
            has_effects,
            effects,
            stroke,
            fill,
            span,
            margins,
            uuid,
        })
    }

    fn parse_sch_table(&mut self) -> Result<Table, Error> {
        self.require_version(VERSION_TABLES, "table")?;
        let mut column_count = None;
        let mut column_widths = Vec::new();
        let mut row_heights = Vec::new();
        let mut cells = Vec::new();
        let mut border_external = None;
        let mut border_header = None;
        let mut border_stroke = None;
        let mut separators_rows = None;
        let mut separators_cols = None;
        let mut separators_stroke = None;
        let mut uuid = None;
        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom(
                "columns, col_widths, row_heights, border, separators, uuid, header or cells",
            )?;
            match head.as_str() {
                "column_count" => {
                    column_count = Some(self.parse_i32_atom("column count")?);
                    self.need_right()?;
                }
                "column_widths" => {
                    let mut values = Vec::new();
                    while !self.at_right() {
                        values.push(self.parse_f64_atom("column width")?);
                    }
                    column_widths = values;
                    self.need_right()?;
                }
                "row_heights" => {
                    let mut values = Vec::new();
                    while !self.at_right() {
                        values.push(self.parse_f64_atom("row height")?);
                    }
                    row_heights = values;
                    self.need_right()?;
                }
                "cells" => {
                    while !self.at_right() {
                        self.need_left()?;
                        if self.need_unquoted_symbol_atom("table_cell")? != "table_cell" {
                            return Err(self.expecting("table_cell"));
                        }
                        let cell = self.parse_sch_table_cell()?;
                        self.need_right()?;
                        cells.push(cell);
                    }
                    self.need_right()?;
                }
                "border" => {
                    while !self.at_right() {
                        self.need_left()?;
                        match self
                            .need_unquoted_symbol_atom("external, header or stroke")?
                            .as_str()
                        {
                            "external" => {
                                border_external = Some(self.parse_bool_atom("external")?);
                                self.need_right()?;
                            }
                            "header" => {
                                border_header = Some(self.parse_bool_atom("header")?);
                                self.need_right()?;
                            }
                            "stroke" => {
                                border_stroke = Some(self.parse_stroke()?);
                            }
                            _ => return Err(self.expecting("external, header or stroke")),
                        }
                    }
                    self.need_right()?;
                }
                "separators" => {
                    while !self.at_right() {
                        self.need_left()?;
                        match self
                            .need_unquoted_symbol_atom("rows, cols, or stroke")?
                            .as_str()
                        {
                            "rows" => {
                                separators_rows = Some(self.parse_bool_atom("rows")?);
                                self.need_right()?;
                            }
                            "cols" => {
                                separators_cols = Some(self.parse_bool_atom("cols")?);
                                self.need_right()?;
                            }
                            "stroke" => {
                                separators_stroke = Some(self.parse_stroke()?);
                            }
                            _ => return Err(self.expecting("rows, cols, or stroke")),
                        }
                    }
                    self.need_right()?;
                }
                "uuid" => {
                    uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                _ => {
                    return Err(self.expecting(
                        "columns, col_widths, row_heights, border, separators, uuid, header or cells",
                    ));
                }
            }
        }
        if cells.is_empty() {
            return Err(self.error_here("Invalid table: no cells defined"));
        }
        Ok(Table {
            column_count,
            column_widths,
            row_heights,
            cells,
            border_external,
            border_header,
            border_stroke,
            separators_rows,
            separators_cols,
            separators_stroke,
            uuid,
        })
    }

    fn parse_sch_image(&mut self) -> Result<Image, Error> {
        let mut at = None;
        let mut scale = 1.0;
        let mut data = None;
        let mut uuid = None;
        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom("at, scale, uuid or data")?;
            match head.as_str() {
                "at" => {
                    at = Some(self.parse_xy2("image at")?);
                    self.need_right()?;
                }
                "scale" => {
                    let parsed_scale = self.parse_f64_atom("image scale factor")?;
                    scale = if parsed_scale.is_normal() {
                        parsed_scale
                    } else {
                        1.0
                    };
                    self.need_right()?;
                }
                "uuid" => {
                    uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                "data" => {
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
                    if self.require_known_version()? <= VERSION_IMAGE_PPI_SCALE_ADJUSTMENT {
                        if let Some(ppi) = Self::png_ppi(&decoded) {
                            scale *= ppi / 300.0;
                        }
                    }
                    data = Some(encoded);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("at, scale, uuid or data")),
            }
        }
        Ok(Image {
            at: at.unwrap_or([0.0, 0.0]),
            scale,
            data,
            uuid,
        })
    }

    fn parse_polyline_shape(&mut self) -> Result<Shape, Error> {
        let mut points = Vec::new();
        let mut has_stroke = false;
        let mut has_fill = false;
        let mut stroke = None;
        let mut fill = None;
        let mut uuid = None;
        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom("pts, uuid, stroke, or fill")?;
            match head.as_str() {
                "pts" => {
                    let mut parsed_points = Vec::new();
                    while !self.at_right() {
                        self.need_left()?;
                        let head = self.need_unquoted_symbol_atom("xy")?;
                        if head != "xy" {
                            return Err(self.expecting("xy"));
                        }
                        parsed_points.push(self.parse_xy2("xy")?);
                        self.need_right()?;
                    }
                    points = parsed_points;
                    self.need_right()?;
                }
                "stroke" => {
                    has_stroke = true;
                    let mut parsed_stroke = self.parse_stroke()?;
                    if self.require_known_version()? <= 20211123
                        && parsed_stroke.style == StrokeStyle::Default
                    {
                        parsed_stroke.style = StrokeStyle::Dash;
                    }
                    stroke = Some(parsed_stroke);
                }
                "fill" => {
                    has_fill = true;
                    fill = Some(self.parse_fill()?);
                }
                "uuid" => {
                    uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                _ => {
                    return Err(self.expecting("pts, uuid, stroke, or fill"));
                }
            }
        }
        Self::fixup_schematic_fill_mode(&mut fill, &stroke);
        Ok(Shape {
            kind: ShapeKind::Polyline,
            points,
            radius: None,
            corner_radius: None,
            has_stroke,
            has_fill,
            stroke,
            fill,
            excluded_from_sim: false,
            in_bom: true,
            on_board: true,
            dnp: false,
            uuid,
        })
    }

    fn parse_sch_arc(&mut self) -> Result<Shape, Error> {
        let mut points = Vec::new();
        let mut has_stroke = false;
        let mut has_fill = false;
        let mut stroke = None;
        let mut fill = None;
        let mut uuid = None;
        while !self.at_right() {
            self.need_left()?;
            match self
                .need_unquoted_symbol_atom("start, mid, end, stroke, fill or uuid")?
                .as_str()
            {
                "start" => {
                    points.push(self.parse_xy2("shape start")?);
                    self.need_right()?;
                }
                "mid" => {
                    points.push(self.parse_xy2("shape mid")?);
                    self.need_right()?;
                }
                "end" => {
                    points.push(self.parse_xy2("shape end")?);
                    self.need_right()?;
                }
                "stroke" => {
                    has_stroke = true;
                    stroke = Some(self.parse_stroke()?);
                }
                "fill" => {
                    has_fill = true;
                    fill = Some(self.parse_fill()?);
                }
                "uuid" => {
                    uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("start, mid, end, stroke, fill or uuid")),
            }
        }
        Self::fixup_schematic_fill_mode(&mut fill, &stroke);
        let mut geometry = [[0.0, 0.0], [0.0, 0.0], [0.0, 0.0]];
        for (slot, point) in points.into_iter().take(3).enumerate() {
            geometry[slot] = point;
        }
        Ok(Shape {
            kind: ShapeKind::Arc,
            points: geometry.to_vec(),
            radius: None,
            corner_radius: None,
            has_stroke,
            has_fill,
            stroke,
            fill,
            excluded_from_sim: false,
            in_bom: true,
            on_board: true,
            dnp: false,
            uuid,
        })
    }

    fn parse_sch_circle(&mut self) -> Result<Shape, Error> {
        let mut center = None;
        let mut radius = Some(0.0);
        let mut has_stroke = false;
        let mut has_fill = false;
        let mut stroke = None;
        let mut fill = None;
        let mut uuid = None;
        while !self.at_right() {
            self.need_left()?;
            match self
                .need_unquoted_symbol_atom("center, radius, stroke, fill or uuid")?
                .as_str()
            {
                "center" => {
                    center = Some(self.parse_xy2("center")?);
                    self.need_right()?;
                }
                "radius" => {
                    radius = Some(self.parse_f64_atom("radius length")?);
                    self.need_right()?;
                }
                "stroke" => {
                    has_stroke = true;
                    stroke = Some(self.parse_stroke()?);
                }
                "fill" => {
                    has_fill = true;
                    fill = Some(self.parse_fill()?);
                }
                "uuid" => {
                    uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("center, radius, stroke, fill or uuid")),
            }
        }
        Self::fixup_schematic_fill_mode(&mut fill, &stroke);
        Ok(Shape {
            kind: ShapeKind::Circle,
            points: vec![center.unwrap_or([0.0, 0.0])],
            radius,
            corner_radius: None,
            has_stroke,
            has_fill,
            stroke,
            fill,
            excluded_from_sim: false,
            in_bom: true,
            on_board: true,
            dnp: false,
            uuid,
        })
    }

    fn parse_sch_rectangle(&mut self) -> Result<Shape, Error> {
        let mut start = [0.0, 0.0];
        let mut end = [0.0, 0.0];
        let mut corner_radius = None;
        let mut has_stroke = false;
        let mut has_fill = false;
        let mut stroke = None;
        let mut fill = None;
        let mut uuid = None;
        while !self.at_right() {
            self.need_left()?;
            match self
                .need_unquoted_symbol_atom("start, end, stroke, fill or uuid")?
                .as_str()
            {
                "start" => {
                    start = self.parse_xy2("start")?;
                    self.need_right()?;
                }
                "end" => {
                    end = self.parse_xy2("end")?;
                    self.need_right()?;
                }
                "radius" => {
                    corner_radius = Some(self.parse_f64_atom("corner radius")?);
                    self.need_right()?;
                }
                "stroke" => {
                    has_stroke = true;
                    stroke = Some(self.parse_stroke()?);
                }
                "fill" => {
                    has_fill = true;
                    fill = Some(self.parse_fill()?);
                }
                "uuid" => {
                    uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("start, end, stroke, fill or uuid")),
            }
        }
        Self::fixup_schematic_fill_mode(&mut fill, &stroke);
        Ok(Shape {
            kind: ShapeKind::Rectangle,
            points: vec![start, end],
            radius: None,
            corner_radius,
            has_stroke,
            has_fill,
            stroke,
            fill,
            excluded_from_sim: false,
            in_bom: true,
            on_board: true,
            dnp: false,
            uuid,
        })
    }

    fn parse_sch_bezier(&mut self) -> Result<Shape, Error> {
        let mut points = vec![[0.0, 0.0], [0.0, 0.0], [0.0, 0.0], [0.0, 0.0]];
        let mut has_stroke = false;
        let mut has_fill = false;
        let mut stroke = None;
        let mut fill = None;
        let mut uuid = None;
        while !self.at_right() {
            self.need_left()?;
            match self
                .need_unquoted_symbol_atom("pts, stroke, fill or uuid")?
                .as_str()
            {
                "pts" => {
                    let mut ii = 0;
                    while !self.at_right() {
                        self.need_left()?;
                        if self.need_unquoted_symbol_atom("xy")? != "xy" {
                            return Err(self.expecting("xy"));
                        }
                        match ii {
                            0..=3 => points[ii] = self.parse_xy2("xy")?,
                            _ => return Err(self.unexpected("control point")),
                        }
                        ii += 1;
                        self.need_right()?;
                    }
                    self.need_right()?;
                }
                "stroke" => {
                    has_stroke = true;
                    stroke = Some(self.parse_stroke()?);
                }
                "fill" => {
                    has_fill = true;
                    fill = Some(self.parse_fill()?);
                }
                "uuid" => {
                    uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("pts, stroke, fill or uuid")),
            }
        }
        Self::fixup_schematic_fill_mode(&mut fill, &stroke);
        Ok(Shape {
            kind: ShapeKind::Bezier,
            points,
            radius: None,
            corner_radius: None,
            has_stroke,
            has_fill,
            stroke,
            fill,
            excluded_from_sim: false,
            in_bom: true,
            on_board: true,
            dnp: false,
            uuid,
        })
    }

    fn parse_rule_area_shape(&mut self) -> Result<Shape, Error> {
        self.require_version(VERSION_RULE_AREAS, "rule_area")?;
        let mut shape = Shape {
            kind: ShapeKind::RuleArea,
            points: Vec::new(),
            radius: None,
            corner_radius: None,
            has_stroke: false,
            has_fill: false,
            stroke: None,
            fill: None,
            excluded_from_sim: false,
            in_bom: true,
            on_board: true,
            dnp: false,
            uuid: None,
        };
        while !self.at_right() {
            self.need_left()?;
            match self
                .need_unquoted_symbol_atom("exclude_from_sim, on_board, in_bom, dnp, or polyline")?
                .as_str()
            {
                "polyline" => {
                    let polyline = self.parse_polyline_shape()?;
                    shape.points = polyline.points;
                    shape.has_stroke = polyline.has_stroke;
                    shape.has_fill = polyline.has_fill;
                    shape.stroke = polyline.stroke;
                    shape.fill = polyline.fill;
                    shape.uuid = polyline.uuid;
                    self.need_right()?;
                }
                "exclude_from_sim" => {
                    shape.excluded_from_sim = self.parse_bool_atom("exclude_from_sim")?;
                    self.need_right()?;
                }
                "in_bom" => {
                    shape.in_bom = self.parse_bool_atom("in_bom")?;
                    self.need_right()?;
                }
                "on_board" => {
                    shape.on_board = self.parse_bool_atom("on_board")?;
                    self.need_right()?;
                }
                "dnp" => {
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
        Ok(shape)
    }

    fn parse_symbol(&mut self) -> Result<Symbol, Error> {
        let mut lib_id = None;
        let mut lib_name = None;
        let mut at = None;
        let mut mirror = None;
        let mut unit = None;
        let mut body_style = None;
        let mut excluded_from_sim = false;
        let mut in_bom = true;
        let mut on_board = true;
        let mut in_pos_files = true;
        let mut dnp = false;
        let mut fields_autoplaced = false;
        let mut uuid = None;
        let mut properties: Vec<Property> = Vec::new();
        let mut instances = Vec::new();
        let mut default_reference = None;
        let mut default_unit = None;
        let mut default_value = None;
        let mut default_footprint = None;
        let mut pins = Vec::new();

        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom(
                "lib_id, lib_name, at, mirror, uuid, exclude_from_sim, on_board, in_bom, dnp, default_instance, property, pin, or instances",
            )?;
            match head.as_str() {
                "lib_id" => {
                    let raw = self.need_symbol_or_number_atom("symbol|number")?;
                    let normalized = raw.replace("{slash}", "/");

                    if let Some(ch) = Self::find_invalid_library_identifier_char(&normalized) {
                        return Err(self.error_here(format!(
                            "Symbol {normalized} contains invalid character '{ch}'"
                        )));
                    }

                    if normalized.is_empty() {
                        return Err(self.error_here("Invalid symbol library ID"));
                    }

                    lib_id = Some(normalized);
                    self.need_right()?;
                }
                "lib_name" => {
                    lib_name = Some(
                        self.need_symbol_atom("lib_name")
                            .map_err(|_| self.error_here("Invalid symbol library name"))?
                            .replace("{slash}", "/"),
                    );
                    self.need_right()?;
                }
                "at" => {
                    let parsed = self.parse_xy3("symbol at")?;
                    match parsed[2] as i32 {
                        0 | 90 | 180 | 270 => at = Some(parsed),
                        _ => return Err(self.expecting("0, 90, 180, or 270")),
                    }
                    self.need_right()?;
                }
                "mirror" => {
                    mirror = Some(
                        match self.need_unquoted_symbol_atom("mirror axis")?.as_str() {
                            "x" => MirrorAxis::X,
                            "y" => MirrorAxis::Y,
                            _ => return Err(self.expecting("x or y")),
                        },
                    );
                    self.need_right()?;
                }
                "convert" | "body_style" => {
                    body_style = Some(self.parse_i32_atom("symbol body style")?);
                    self.need_right()?;
                }
                "unit" => {
                    unit = Some(self.parse_i32_atom("unit")?);
                    self.need_right()?;
                }
                "exclude_from_sim" => {
                    excluded_from_sim = self.parse_bool_atom("exclude_from_sim")?;
                    self.need_right()?;
                }
                "in_bom" => {
                    in_bom = self.parse_bool_atom("in_bom")?;
                    self.need_right()?;
                }
                "on_board" => {
                    on_board = self.parse_bool_atom("on_board")?;
                    self.need_right()?;
                }
                "in_pos_files" => {
                    in_pos_files = self.parse_bool_atom("in_pos_files")?;
                    self.need_right()?;
                }
                "dnp" => {
                    dnp = self.parse_bool_atom("dnp")?;
                    self.need_right()?;
                }
                "fields_autoplaced" => {
                    fields_autoplaced = self.parse_maybe_absent_bool(true)?;
                    self.need_right()?;
                }
                "uuid" => {
                    uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                "property" => {
                    let property = self.parse_sch_field(FieldParent::Symbol)?;
                    if property.key == SIM_LEGACY_ENABLE_FIELD_V7 {
                        excluded_from_sim = property.value == "0";
                        self.need_right()?;
                        continue;
                    }
                    if property.key == SIM_LEGACY_ENABLE_FIELD {
                        excluded_from_sim = property.value == "N";
                        self.need_right()?;
                        continue;
                    }

                    if matches!(
                        property.kind,
                        PropertyKind::SymbolReference
                            | PropertyKind::SymbolValue
                            | PropertyKind::SymbolFootprint
                            | PropertyKind::SymbolDatasheet
                    ) {
                        if let Some(existing) =
                            properties.iter_mut().find(|p| p.kind == property.kind)
                        {
                            *existing = property;
                        } else {
                            properties.push(property);
                        }
                    } else {
                        properties.push(property);
                    }
                    self.need_right()?;
                }
                "instances" => {
                    while !self.at_right() {
                        self.need_left()?;
                        if self.need_unquoted_symbol_atom("project")? != "project" {
                            return Err(self.expecting("project"));
                        }
                        let project = self.need_symbol_atom("project name")?;
                        while !self.at_right() {
                            self.need_left()?;
                            if self.need_unquoted_symbol_atom("path")? != "path" {
                                return Err(self.expecting("path"));
                            }
                            let path = self.need_symbol_atom("symbol instance path")?;
                            let mut reference = None;
                            let mut unit = None;
                            let mut value = None;
                            let mut footprint = None;
                            let mut variants = Vec::new();
                            while !self.at_right() {
                                self.need_left()?;
                                match self
                                    .need_unquoted_symbol_atom(
                                        "reference, unit, value, footprint, or variant",
                                    )?
                                    .as_str()
                                {
                                    "reference" => {
                                        reference = Some(self.need_symbol_atom("reference")?);
                                        self.need_right()?;
                                    }
                                    "unit" => {
                                        unit = Some(self.parse_i32_atom("symbol unit")?);
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
                                        let property = Property {
                                            key: "Value".to_string(),
                                            value: parsed.clone(),
                                            kind: PropertyKind::SymbolValue,
                                            is_private: false,
                                            at: None,
                                            angle: None,
                                            visible: true,
                                            show_name: true,
                                            can_autoplace: true,
                                            has_effects: false,
                                            effects: None,
                                        };
                                        if let Some(existing) = properties
                                            .iter_mut()
                                            .find(|p| p.kind == PropertyKind::SymbolValue)
                                        {
                                            *existing = property;
                                        } else {
                                            properties.push(property);
                                        }
                                        value = Some(parsed);
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
                                        let property = Property {
                                            key: "Footprint".to_string(),
                                            value: parsed.clone(),
                                            kind: PropertyKind::SymbolFootprint,
                                            is_private: false,
                                            at: None,
                                            angle: None,
                                            visible: true,
                                            show_name: true,
                                            can_autoplace: true,
                                            has_effects: false,
                                            effects: None,
                                        };
                                        if let Some(existing) = properties
                                            .iter_mut()
                                            .find(|p| p.kind == PropertyKind::SymbolFootprint)
                                        {
                                            *existing = property;
                                        } else {
                                            properties.push(property);
                                        }
                                        footprint = Some(parsed);
                                        self.need_right()?;
                                    }
                                    "variant" => {
                                        let mut variant_name = String::new();
                                        let mut variant_dnp = dnp;
                                        let mut variant_excluded_from_sim = excluded_from_sim;
                                        let mut variant_in_bom = in_bom;
                                        let mut variant_on_board = on_board;
                                        let mut variant_in_pos_files = in_pos_files;
                                        let mut variant_fields = Vec::new();

                                        while !self.at_right() {
                                            self.need_left()?;
                                            match self
                                                .need_unquoted_symbol_atom(
                                                    "dnp, exclude_from_sim, field, in_bom, in_pos_files, name, or on_board",
                                                )?
                                                .as_str()
                                            {
                                                "name" => {
                                                    variant_name = self
                                                        .need_symbol_atom("name")
                                                        .map_err(|_| {
                                                            self.error_here("Invalid variant name")
                                                        })?;
                                                    self.need_right()?;
                                                }
                                                "dnp" => {
                                                    variant_dnp = self.parse_bool_atom("dnp")?;
                                                    self.need_right()?;
                                                }
                                                "exclude_from_sim" => {
                                                    variant_excluded_from_sim =
                                                        self.parse_bool_atom("exclude_from_sim")?;
                                                    self.need_right()?;
                                                }
                                                "in_bom" => {
                                                    variant_in_bom =
                                                        self.parse_bool_atom("in_bom")?;
                                                    if self.require_known_version()?
                                                        < VERSION_VARIANT_IN_BOM_FIX
                                                    {
                                                        variant_in_bom = !variant_in_bom;
                                                    }
                                                    self.need_right()?;
                                                }
                                                "on_board" => {
                                                    variant_on_board =
                                                        self.parse_bool_atom("on_board")?;
                                                    self.need_right()?;
                                                }
                                                "in_pos_files" => {
                                                    variant_in_pos_files =
                                                        self.parse_bool_atom("in_pos_files")?;
                                                    self.need_right()?;
                                                }
                                                "field" => {
                                                    let mut field_name = None;
                                                    let mut field_value = None;

                                                    while !self.at_right() {
                                                        self.need_left()?;
                                                        match self
                                                            .need_unquoted_symbol_atom(
                                                                "name or value",
                                                            )?
                                                            .as_str()
                                                        {
                                                            "name" => {
                                                                field_name = Some(
                                                                    self.need_symbol_atom("name")
                                                                        .map_err(|_| {
                                                                            self.error_here(
                                                                                "Invalid variant field name",
                                                                            )
                                                                        })?,
                                                                );
                                                                self.need_right()?;
                                                            }
                                                            "value" => {
                                                                field_value = Some(
                                                                    self.need_symbol_atom("value")
                                                                        .map_err(|_| {
                                                                            self.error_here(
                                                                                "Invalid variant field value",
                                                                            )
                                                                        })?,
                                                                );
                                                                self.need_right()?;
                                                            }
                                                            _ => {
                                                                return Err(
                                                                    self.expecting("name or value")
                                                                );
                                                            }
                                                        }
                                                    }

                                                    variant_fields.push(VariantField {
                                                        name: field_name.unwrap_or_default(),
                                                        value: field_value.unwrap_or_default(),
                                                    });
                                                    self.need_right()?;
                                                }
                                                _ => {
                                                    return Err(self.expecting(
                                                        "dnp, exclude_from_sim, field, in_bom, in_pos_files, name, or on_board",
                                                    ));
                                                }
                                            }
                                        }

                                        variants.push(ItemVariant {
                                            name: variant_name,
                                            dnp: variant_dnp,
                                            excluded_from_sim: variant_excluded_from_sim,
                                            in_bom: variant_in_bom,
                                            on_board: variant_on_board,
                                            in_pos_files: variant_in_pos_files,
                                            fields: variant_fields,
                                        });
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
                            instances.push(SymbolLocalInstance {
                                project: project.clone(),
                                path,
                                reference,
                                unit,
                                value,
                                footprint,
                                variants,
                            });
                        }
                        self.need_right()?;
                    }
                    self.need_right()?;
                }
                "default_instance" => {
                    while !self.at_right() {
                        self.need_left()?;
                        match self
                            .need_unquoted_symbol_atom("reference, unit, value or footprint")?
                            .as_str()
                        {
                            "reference" => {
                                default_reference = Some(self.need_symbol_atom("reference")?);
                                self.need_right()?;
                            }
                            "unit" => {
                                default_unit = Some(self.parse_i32_atom("symbol unit")?);
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
                                let property = Property {
                                    key: "Value".to_string(),
                                    value: parsed.clone(),
                                    kind: PropertyKind::SymbolValue,
                                    is_private: false,
                                    at: None,
                                    angle: None,
                                    visible: true,
                                    show_name: true,
                                    can_autoplace: true,
                                    has_effects: false,
                                    effects: None,
                                };
                                if let Some(existing) = properties
                                    .iter_mut()
                                    .find(|p| p.kind == PropertyKind::SymbolValue)
                                {
                                    *existing = property;
                                } else {
                                    properties.push(property);
                                }
                                default_value = Some(parsed);
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
                                let property = Property {
                                    key: "Footprint".to_string(),
                                    value: parsed.clone(),
                                    kind: PropertyKind::SymbolFootprint,
                                    is_private: false,
                                    at: None,
                                    angle: None,
                                    visible: true,
                                    show_name: true,
                                    can_autoplace: true,
                                    has_effects: false,
                                    effects: None,
                                };
                                if let Some(existing) = properties
                                    .iter_mut()
                                    .find(|p| p.kind == PropertyKind::SymbolFootprint)
                                {
                                    *existing = property;
                                } else {
                                    properties.push(property);
                                }
                                default_footprint = Some(parsed);
                                self.need_right()?;
                            }
                            _ => {
                                return Err(self.expecting("reference, unit, value or footprint"));
                            }
                        }
                    }
                    self.need_right()?;
                }
                "pin" => {
                    let number = self.need_symbol_atom("pin number")?;
                    let mut alternate = None;
                    let mut pin_uuid = None;
                    while !self.at_right() {
                        self.need_left()?;
                        match self
                            .need_unquoted_symbol_atom("alternate or uuid")?
                            .as_str()
                        {
                            "alternate" => {
                                alternate = Some(self.need_symbol_atom("alternate")?);
                                self.need_right()?;
                            }
                            "uuid" => {
                                let parsed = self.need_symbol_atom("uuid")?;
                                if self.require_known_version()? >= VERSION_SYMBOL_PIN_UUID {
                                    pin_uuid = Some(parsed);
                                }
                                self.need_right()?;
                            }
                            _ => return Err(self.expecting("alternate or uuid")),
                        }
                    }
                    self.need_right()?;
                    pins.push(SymbolPin {
                        number,
                        alternate,
                        uuid: pin_uuid,
                    });
                }
                _ => {
                    return Err(self.expecting(
                        "lib_id, lib_name, at, mirror, uuid, exclude_from_sim, on_board, in_bom, dnp, default_instance, property, pin, or instances",
                    ));
                }
            }
        }

        let lib_id = lib_id.unwrap_or_default();
        let [x, y, angle] = at.unwrap_or([0.0, 0.0, 0.0]);
        let lib_name = lib_name.filter(|name| name != &lib_id);
        Ok(Symbol {
            lib_id,
            lib_name,
            linked_lib_symbol_name: None,
            at: [x, y],
            angle,
            mirror,
            unit,
            body_style,
            excluded_from_sim,
            in_bom,
            on_board,
            in_pos_files,
            dnp,
            fields_autoplaced,
            uuid,
            properties,
            instances,
            default_reference,
            default_unit,
            default_value,
            default_footprint,
            pins,
        })
    }

    fn parse_sheet(&mut self) -> Result<Sheet, Error> {
        let mut at = None;
        let mut size = None;
        let mut has_stroke = false;
        let mut has_fill = false;
        let mut stroke = None;
        let mut fill = None;
        let mut excluded_from_sim = false;
        let mut in_bom = true;
        let mut on_board = true;
        let mut dnp = false;
        let mut fields_autoplaced = false;
        let mut uuid = None;
        let mut properties: Vec<Property> = Vec::new();
        let mut pins = Vec::new();
        let mut instances = Vec::new();

        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom(
                "at, size, stroke, background, instances, uuid, property, or pin",
            )?;
            match head.as_str() {
                "at" => {
                    at = Some(self.parse_xy2("sheet at")?);
                    self.need_right()?;
                }
                "size" => {
                    size = Some(self.parse_xy2("sheet size")?);
                    self.need_right()?;
                }
                "exclude_from_sim" => {
                    excluded_from_sim = self.parse_bool_atom("exclude_from_sim")?;
                    self.need_right()?;
                }
                "in_bom" => {
                    in_bom = self.parse_bool_atom("in_bom")?;
                    self.need_right()?;
                }
                "on_board" => {
                    on_board = self.parse_bool_atom("on_board")?;
                    self.need_right()?;
                }
                "dnp" => {
                    dnp = self.parse_bool_atom("dnp")?;
                    self.need_right()?;
                }
                "fields_autoplaced" => {
                    fields_autoplaced = self.parse_maybe_absent_bool(true)?;
                    self.need_right()?;
                }
                "stroke" => {
                    has_stroke = true;
                    stroke = Some(self.parse_stroke()?);
                }
                "fill" => {
                    has_fill = true;
                    fill = Some(self.parse_fill()?);
                }
                "uuid" => {
                    uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                "property" => {
                    let mut property = self.parse_sch_field(FieldParent::Sheet)?;
                    if self
                        .require_known_version()
                        .unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION)
                        <= VERSION_WRONG_SHEET_FIELD_IDS
                    {
                        if properties.is_empty() {
                            property.key = "Sheetname".to_string();
                            property.kind = PropertyKind::SheetName;
                        } else if properties.len() == 1 {
                            property.key = "Sheetfile".to_string();
                            property.kind = PropertyKind::SheetFile;
                        }
                    }
                    if matches!(
                        property.kind,
                        PropertyKind::SheetName | PropertyKind::SheetFile
                    ) {
                        if let Some(existing) =
                            properties.iter_mut().find(|p| p.kind == property.kind)
                        {
                            *existing = property;
                        } else {
                            properties.push(property);
                        }
                    } else {
                        properties.push(property);
                    }
                    self.need_right()?;
                }
                "pin" => {
                    pins.push(self.parse_sch_sheet_pin()?);
                    self.need_right()?;
                }
                "instances" => {
                    let mut parsed_instances = Vec::new();
                    while !self.at_right() {
                        self.need_left()?;
                        if self.need_unquoted_symbol_atom("project")? != "project" {
                            return Err(self.expecting("project"));
                        }
                        let project = self.need_symbol_atom("project name")?;
                        while !self.at_right() {
                            self.need_left()?;
                            if self.need_unquoted_symbol_atom("path")? != "path" {
                                return Err(self.expecting("path"));
                            }
                            let path = self.need_symbol_atom("sheet instance path")?;
                            let mut page = None;
                            let mut variants = Vec::new();
                            while !self.at_right() {
                                self.need_left()?;
                                match self.need_unquoted_symbol_atom("page or variant")?.as_str() {
                                    "page" => {
                                        let raw_page = self.need_symbol_atom("page")?;
                                        page = Some(self.normalize_page_number(raw_page));
                                        self.need_right()?;
                                    }
                                    "variant" => {
                                        let mut variant_name = String::new();
                                        let mut variant_dnp = dnp;
                                        let mut variant_excluded_from_sim = excluded_from_sim;
                                        let mut variant_in_bom = in_bom;
                                        let mut variant_on_board = on_board;
                                        let mut variant_in_pos_files = false;
                                        let mut variant_fields = Vec::new();

                                        while !self.at_right() {
                                            self.need_left()?;
                                            match self
                                                .need_unquoted_symbol_atom(
                                                    "dnp, exclude_from_sim, field, in_bom, in_pos_files, name, or on_board",
                                                )?
                                                .as_str()
                                            {
                                                "name" => {
                                                    variant_name = self
                                                        .need_symbol_atom("name")
                                                        .map_err(|_| {
                                                            self.error_here("Invalid variant name")
                                                        })?;
                                                    self.need_right()?;
                                                }
                                                "dnp" => {
                                                    variant_dnp = self.parse_bool_atom("dnp")?;
                                                    self.need_right()?;
                                                }
                                                "exclude_from_sim" => {
                                                    variant_excluded_from_sim =
                                                        self.parse_bool_atom("exclude_from_sim")?;
                                                    self.need_right()?;
                                                }
                                                "in_bom" => {
                                                    variant_in_bom =
                                                        self.parse_bool_atom("in_bom")?;
                                                    if self.require_known_version()?
                                                        < VERSION_VARIANT_IN_BOM_FIX
                                                    {
                                                        variant_in_bom = !variant_in_bom;
                                                    }
                                                    self.need_right()?;
                                                }
                                                "on_board" => {
                                                    variant_on_board =
                                                        self.parse_bool_atom("on_board")?;
                                                    self.need_right()?;
                                                }
                                                "in_pos_files" => {
                                                    variant_in_pos_files =
                                                        self.parse_bool_atom("in_pos_files")?;
                                                    self.need_right()?;
                                                }
                                                "field" => {
                                                    let mut field_name = None;
                                                    let mut field_value = None;

                                                    while !self.at_right() {
                                                        self.need_left()?;
                                                        match self
                                                            .need_unquoted_symbol_atom(
                                                                "name or value",
                                                            )?
                                                            .as_str()
                                                        {
                                                            "name" => {
                                                                field_name = Some(
                                                                    self.need_symbol_atom("name")
                                                                        .map_err(|_| {
                                                                            self.error_here(
                                                                                "Invalid variant field name",
                                                                            )
                                                                        })?,
                                                                );
                                                                self.need_right()?;
                                                            }
                                                            "value" => {
                                                                field_value = Some(
                                                                    self.need_symbol_atom("value")
                                                                        .map_err(|_| {
                                                                            self.error_here(
                                                                                "Invalid variant field value",
                                                                            )
                                                                        })?,
                                                                );
                                                                self.need_right()?;
                                                            }
                                                            _ => {
                                                                return Err(
                                                                    self.expecting("name or value")
                                                                );
                                                            }
                                                        }
                                                    }

                                                    variant_fields.push(VariantField {
                                                        name: field_name.unwrap_or_default(),
                                                        value: field_value.unwrap_or_default(),
                                                    });
                                                    self.need_right()?;
                                                }
                                                _ => {
                                                    return Err(self.expecting(
                                                        "dnp, exclude_from_sim, field, in_bom, in_pos_files, name, or on_board",
                                                    ));
                                                }
                                            }
                                        }

                                        variants.push(ItemVariant {
                                            name: variant_name,
                                            dnp: variant_dnp,
                                            excluded_from_sim: variant_excluded_from_sim,
                                            in_bom: variant_in_bom,
                                            on_board: variant_on_board,
                                            in_pos_files: variant_in_pos_files,
                                            fields: variant_fields,
                                        });
                                        self.need_right()?;
                                    }
                                    _ => return Err(self.expecting("page or variant")),
                                }
                            }
                            self.need_right()?;
                            parsed_instances.push(SheetLocalInstance {
                                project: project.clone(),
                                path,
                                page,
                                variants,
                            });
                        }
                        self.need_right()?;
                    }
                    instances = parsed_instances;
                    self.need_right()?;
                }
                _ => {
                    return Err(self.expecting(
                        "at, size, stroke, background, instances, uuid, property, or pin",
                    ));
                }
            }
        }

        let name = properties
            .iter()
            .find(|property| property.kind == PropertyKind::SheetName)
            .map(|property| property.value.clone());
        let filename = properties
            .iter()
            .find(|property| property.kind == PropertyKind::SheetFile)
            .map(|property| property.value.clone());

        if name.is_none() {
            return Err(self.error_here("Missing sheet name property"));
        }
        if filename.is_none() {
            return Err(self.error_here("Missing sheet file property"));
        }

        Ok(Sheet {
            at: at.unwrap_or([0.0, 0.0]),
            size: size.unwrap_or([0.0, 0.0]),
            has_stroke,
            has_fill,
            stroke,
            fill,
            excluded_from_sim,
            in_bom,
            on_board,
            dnp,
            fields_autoplaced,
            uuid,
            name,
            filename,
            properties,
            pins,
            instances,
        })
    }

    fn parse_sch_sheet_pin(&mut self) -> Result<SheetPin, Error> {
        let name = self
            .need_symbol_atom("sheet pin name")
            .map_err(|_| self.error_here("Invalid sheet pin name"))?;
        if name.is_empty() {
            return Err(self.error_here("Empty sheet pin name"));
        }
        let shape = match self.need_unquoted_symbol_atom("sheet pin shape")?.as_str() {
            "input" => SheetPinShape::Input,
            "output" => SheetPinShape::Output,
            "bidirectional" => SheetPinShape::Bidirectional,
            "tri_state" => SheetPinShape::TriState,
            "passive" => SheetPinShape::Passive,
            _ => return Err(self.expecting("input, output, bidirectional, tri_state, or passive")),
        };

        let mut at = None;
        let mut side = None;
        let mut visible = true;
        let mut has_effects = false;
        let mut effects = None;
        let mut uuid = None;

        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom("at, uuid or effects")?;
            match head.as_str() {
                "at" => {
                    let parsed = self.parse_xy3("sheet pin at")?;
                    let parsed_side = match parsed[2] as i32 {
                        0 => SheetSide::Right,
                        90 => SheetSide::Top,
                        180 => SheetSide::Left,
                        270 => SheetSide::Bottom,
                        _ => return Err(self.expecting("0, 90, 180, or 270")),
                    };
                    at = Some([parsed[0], parsed[1]]);
                    side = Some(parsed_side);
                    self.need_right()?;
                }
                "uuid" => {
                    uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                "effects" => {
                    let parsed_effects = self.parse_eda_text()?;
                    visible = !parsed_effects.hidden;
                    has_effects = true;
                    effects = Some(parsed_effects);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("at, uuid or effects")),
            }
        }

        Ok(SheetPin {
            name,
            shape,
            at,
            side,
            visible,
            has_effects,
            effects,
            uuid,
        })
    }

    fn parse_sch_sheet_instances(&mut self) -> Result<Vec<SheetInstance>, Error> {
        let mut out = Vec::new();
        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom("path")?;
            if head != "path" {
                return Err(self.expecting("path"));
            }
            let raw_path = self.need_symbol_atom("sheet instance path")?;
            let path = self.normalize_instance_path(
                raw_path,
                self.require_known_version()? < VERSION_SHEET_INSTANCE_ROOT_PATH,
            );
            let mut page = None;
            while !self.at_right() {
                self.need_left()?;
                let child = self.need_unquoted_symbol_atom("page")?;
                match child.as_str() {
                    "page" => {
                        let raw_page = self.need_symbol_atom("page")?;
                        page = Some(self.normalize_page_number(raw_page));
                    }
                    _ => return Err(self.expecting("page")),
                }
                self.need_right()?;
            }
            self.need_right()?;
            if self.require_known_version()? >= VERSION_SKIP_EMPTY_ROOT_SHEET_INSTANCE_PATH
                && path.is_empty()
            {
                continue;
            }
            out.push(SheetInstance { path, page });
        }
        Ok(out)
    }

    fn parse_sch_symbol_instances(&mut self) -> Result<Vec<SymbolInstance>, Error> {
        let mut out = Vec::new();
        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom("path")?;
            if head != "path" {
                return Err(self.expecting("path"));
            }
            let raw_path = self.need_symbol_atom("symbol instance path")?;
            let path = self.normalize_instance_path(raw_path, true);
            let mut reference = None;
            let mut unit = None;
            let mut value = None;
            let mut footprint = None;
            while !self.at_right() {
                self.need_left()?;
                let child =
                    self.need_unquoted_symbol_atom("reference, unit, value or footprint")?;
                match child.as_str() {
                    "reference" => reference = Some(self.need_symbol_atom("reference")?),
                    "unit" => unit = Some(self.parse_i32_atom("unit")?),
                    "value" => {
                        value = Some({
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
                        footprint = Some({
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
                    _ => return Err(self.expecting("reference, unit, value or footprint")),
                }
                self.need_right()?;
            }
            self.need_right()?;
            out.push(SymbolInstance {
                path,
                reference,
                unit,
                value,
                footprint,
            });
        }
        Ok(out)
    }

    fn parse_group(&mut self) -> Result<(), Error> {
        let mut name = None;

        while self.at_atom() {
            if self.at_unquoted_symbol_with("locked") {
                let _ = self.need_unquoted_symbol_atom("locked")?;
                continue;
            }
            name = Some(self.need_quoted_atom("group name or locked")?);
            break;
        }

        let mut uuid = None;
        let mut lib_id = None;
        let mut members = Vec::new();

        while !self.at_right() {
            self.need_left()?;
            match self
                .need_unquoted_symbol_atom("uuid, lib_id, members")?
                .as_str()
            {
                "uuid" => {
                    uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                "lib_id" => {
                    let raw = self.need_symbol_or_number_atom("symbol|number")?;
                    let normalized = raw.replace("{slash}", "/");

                    if let Some(ch) = Self::find_invalid_library_identifier_char(&normalized) {
                        return Err(self.error_here(format!(
                            "Group library link {normalized} contains invalid character '{ch}'"
                        )));
                    }

                    if normalized.is_empty() {
                        return Err(self.error_here("Invalid library ID"));
                    }

                    lib_id = Some(normalized);
                    self.need_right()?;
                }
                "members" => {
                    while !self.at_right() {
                        members.push(self.need_symbol_atom("group member uuid")?);
                    }
                    self.need_right()?;
                }
                _ => return Err(self.expecting("uuid, lib_id, members")),
            }
        }

        self.pending_groups.push(Group {
            name,
            uuid,
            lib_id,
            members,
        });
        Ok(())
    }

    fn parse_sch_field(&mut self, parent: FieldParent) -> Result<Property, Error> {
        let mut is_private = false;
        if self.at_unquoted_symbol_with("private") {
            let _ = self.need_unquoted_symbol_atom("private")?;
            is_private = true;
        }
        let key = self
            .need_symbol_atom("property name")
            .map_err(|_| self.error_here("Invalid property name"))?;
        if key.is_empty() {
            return Err(self.error_here("Empty property name"));
        }
        let mut value = self
            .need_symbol_atom("property value")
            .map_err(|_| self.error_here("Invalid property value"))?;
        if self.version.unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION) < VERSION_EMPTY_TILDE_IS_EMPTY
            && value == "~"
        {
            value.clear();
        }
        let mut key = key;
        let kind = match parent {
            FieldParent::Symbol => match key.to_ascii_lowercase().as_str() {
                "reference" => PropertyKind::SymbolReference,
                "value" => PropertyKind::SymbolValue,
                "footprint" => PropertyKind::SymbolFootprint,
                "datasheet" => PropertyKind::SymbolDatasheet,
                _ => PropertyKind::User,
            },
            FieldParent::Sheet => match key.to_ascii_lowercase().as_str() {
                "sheetname" | "sheet name" => PropertyKind::SheetName,
                "sheetfile" | "sheet file" => PropertyKind::SheetFile,
                _ => PropertyKind::SheetUser,
            },
            FieldParent::GlobalLabel => match key.to_ascii_lowercase().as_str() {
                "intersheet references" => PropertyKind::GlobalLabelIntersheetRefs,
                _ => PropertyKind::User,
            },
            FieldParent::OtherLabel => PropertyKind::User,
        };
        key = match kind {
            PropertyKind::SymbolReference => "Reference".to_string(),
            PropertyKind::SymbolValue => "Value".to_string(),
            PropertyKind::SymbolFootprint => "Footprint".to_string(),
            PropertyKind::SymbolDatasheet => "Datasheet".to_string(),
            PropertyKind::SheetName => "Sheetname".to_string(),
            PropertyKind::SheetFile => "Sheetfile".to_string(),
            PropertyKind::GlobalLabelIntersheetRefs => "Intersheet References".to_string(),
            _ => key,
        };
        let mut at = None;
        let mut angle = None;
        let mut visible = true;
        let mut show_name = true;
        let mut can_autoplace = true;
        let mut has_effects = false;
        let mut effects = None;

        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom(
                "id, at, hide, show_name, do_not_autoplace or effects",
            )?;
            match head.as_str() {
                "id" => {
                    let _ = self.parse_i32_atom("field ID")?;
                    self.need_right()?;
                }
                "at" => {
                    let parsed = self.parse_xy3("property at")?;
                    at = Some([parsed[0], parsed[1]]);
                    angle = Some(parsed[2]);
                    self.need_right()?;
                }
                "hide" => {
                    visible = !self.parse_bool_atom("hide")?;
                    self.need_right()?;
                }
                "show_name" => {
                    show_name = self.parse_maybe_absent_bool(true)?;
                    self.need_right()?;
                }
                "do_not_autoplace" => {
                    can_autoplace = !self.parse_maybe_absent_bool(true)?;
                    self.need_right()?;
                }
                "effects" => {
                    let parsed_effects = self.parse_eda_text()?;
                    has_effects = true;
                    if parsed_effects.hidden {
                        visible = false;
                    }
                    effects = Some(parsed_effects);
                    self.need_right()?;
                }
                _ => {
                    return Err(
                        self.expecting("id, at, hide, show_name, do_not_autoplace or effects")
                    );
                }
            }
        }
        Ok(Property {
            key,
            value,
            kind,
            is_private: matches!(kind, PropertyKind::User) && is_private,
            at,
            angle,
            visible,
            show_name,
            can_autoplace,
            has_effects,
            effects,
        })
    }

    fn parse_stroke(&mut self) -> Result<Stroke, Error> {
        let mut stroke = Stroke {
            width: None,
            style: StrokeStyle::Default,
            color: None,
        };

        while !self.at_right() {
            self.need_left()?;
            match self
                .need_unquoted_symbol_atom("width, type or color")?
                .as_str()
            {
                "width" => {
                    stroke.width = Some(self.parse_f64_atom("stroke width")?);
                    self.need_right()?;
                }
                "type" => {
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
        let mut fill = Fill {
            fill_type: FillType::None,
            color: None,
        };

        while !self.at_right() {
            self.need_left()?;
            match self.need_unquoted_symbol_atom("type or color")?.as_str() {
                "type" => {
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

    fn fixup_schematic_fill_mode(fill: &mut Option<Fill>, stroke: &Option<Stroke>) {
        if let Some(fill) = fill.as_mut() {
            if fill.fill_type == FillType::Outline {
                fill.fill_type = FillType::Color;
                fill.color = stroke.as_ref().and_then(|stroke| stroke.color);
            }
        }
    }

    fn find_invalid_library_identifier_char(value: &str) -> Option<char> {
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

    fn parse_eda_text(&mut self) -> Result<TextEffects, Error> {
        let mut effects = TextEffects::default();

        while !self.at_right() {
            if self.at_atom() {
                match self
                    .need_unquoted_symbol_atom("font, justify, hide or href")?
                    .as_str()
                {
                    "hide" => {
                        effects.hidden = self.parse_maybe_absent_bool(true)?;
                    }
                    _ => return Err(self.expecting("font, justify, hide or href")),
                }

                continue;
            }

            self.need_left()?;
            match self
                .need_unquoted_symbol_atom("font, justify, hide or href")?
                .as_str()
            {
                "font" => {
                    while !self.at_right() {
                        if self.at_atom() {
                            match self
                                .need_unquoted_symbol_atom(
                                    "face, size, thickness, line_spacing, bold, or italic",
                                )?
                                .as_str()
                            {
                                "bold" => effects.bold = self.parse_inline_optional_bool(true)?,
                                "italic" => {
                                    effects.italic = self.parse_inline_optional_bool(true)?
                                }
                                _ => {
                                    return Err(self.expecting(
                                        "face, size, thickness, line_spacing, bold, or italic",
                                    ));
                                }
                            }

                            continue;
                        }

                        self.need_left()?;
                        match self
                            .need_unquoted_symbol_atom(
                                "face, size, thickness, line_spacing, bold, or italic",
                            )?
                            .as_str()
                        {
                            "face" => {
                                effects.font_face = Some(self.parse_string_atom("font face")?);
                                self.need_right()?;
                            }
                            "size" => {
                                effects.font_size = Some([
                                    self.parse_f64_atom("font width")?,
                                    self.parse_f64_atom("font height")?,
                                ]);
                                self.need_right()?;
                            }
                            "thickness" => {
                                effects.thickness = Some(self.parse_f64_atom("text thickness")?);
                                self.need_right()?;
                            }
                            "color" => {
                                effects.color = Some([
                                    f64::from(self.parse_i32_atom("red")?) / 255.0,
                                    f64::from(self.parse_i32_atom("green")?) / 255.0,
                                    f64::from(self.parse_i32_atom("blue")?) / 255.0,
                                    self.parse_f64_atom("alpha")?.clamp(0.0, 1.0),
                                ]);
                                self.need_right()?;
                            }
                            "line_spacing" => {
                                effects.line_spacing = Some(self.parse_f64_atom("line spacing")?);
                                self.need_right()?;
                            }
                            "bold" => {
                                effects.bold = self.parse_maybe_absent_bool(true)?;
                                self.need_right()?;
                            }
                            "italic" => {
                                effects.italic = self.parse_maybe_absent_bool(true)?;
                                self.need_right()?;
                            }
                            _ => {
                                return Err(self.expecting(
                                    "face, size, thickness, line_spacing, bold, or italic",
                                ));
                            }
                        }
                    }

                    self.need_right()?;
                }
                "justify" => {
                    while !self.at_right() {
                        match self
                            .need_unquoted_symbol_atom("left, right, top, bottom, or mirror")?
                            .as_str()
                        {
                            "left" => effects.h_justify = TextHJustify::Left,
                            "right" => effects.h_justify = TextHJustify::Right,
                            "top" => effects.v_justify = TextVJustify::Top,
                            "bottom" => effects.v_justify = TextVJustify::Bottom,
                            "mirror" => {}
                            _ => return Err(self.expecting("left, right, top, bottom, or mirror")),
                        }
                    }

                    self.need_right()?;
                }
                "href" => {
                    let href = self.parse_string_atom("hyperlink url")?;
                    if !Self::is_valid_hyperlink(&href) {
                        return Err(self.error_here(format!("invalid hyperlink url `{href}`")));
                    }
                    effects.hyperlink = Some(href);
                    self.need_right()?;
                }
                "hide" => {
                    effects.hidden = self.parse_maybe_absent_bool(true)?;
                    self.need_right()?;
                }
                _ => return Err(self.expecting("font, justify, hide or href")),
            }
        }

        Ok(effects)
    }

    fn is_valid_hyperlink(href: &str) -> bool {
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

    fn label_spin_from_angle(angle: f64) -> Option<LabelSpin> {
        match angle.rem_euclid(360.0) as i32 {
            0 => Some(LabelSpin::Right),
            90 => Some(LabelSpin::Up),
            180 => Some(LabelSpin::Left),
            270 => Some(LabelSpin::Bottom),
            _ => None,
        }
    }

    fn keep_upright_angle(angle: f64) -> f64 {
        let mut normalized = angle.rem_euclid(360.0);

        if normalized <= 45.0 || normalized >= 315.0 || (normalized > 135.0 && normalized <= 225.0)
        {
            normalized = 0.0;
        } else {
            normalized = 90.0;
        }

        normalized
    }

    fn legacy_text_box_margin(stroke_width: f64, text_size_y: f64) -> f64 {
        (stroke_width / 2.0) + (text_size_y * 0.75)
    }

    fn png_ppi(data: &[u8]) -> Option<f64> {
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

    fn parse_string_atom(&mut self, field: impl Into<String>) -> Result<String, Error> {
        let field = field.into();
        self.need_atom()
            .map_err(|_| self.error_here(format!("missing {field}")))
    }

    fn parse_i32_atom(&mut self, field: impl Into<String>) -> Result<i32, Error> {
        let field = field.into();
        let value = self
            .need_number_atom(field.as_str())
            .map_err(|_| self.error_here(format!("missing {field}")))?;
        value
            .parse::<i32>()
            .map_err(|_| self.error_here(format!("missing {field}")))
    }

    fn parse_f64_atom(&mut self, field: impl Into<String>) -> Result<f64, Error> {
        let field = field.into();
        let value = self
            .need_number_atom(field.as_str())
            .map_err(|_| self.error_here(format!("missing {field}")))?;
        value
            .parse::<f64>()
            .map_err(|_| self.error_here(format!("missing {field}")))
    }

    fn parse_bool_atom(&mut self, field: &str) -> Result<bool, Error> {
        let _ = field;
        match self.need_unquoted_symbol_atom("yes or no")?.as_str() {
            "yes" => Ok(true),
            "no" => Ok(false),
            _ => Err(self.expecting("yes or no")),
        }
    }

    fn parse_maybe_absent_bool(&mut self, default: bool) -> Result<bool, Error> {
        if self.at_right() {
            Ok(default)
        } else {
            self.parse_bool_atom("boolean")
        }
    }

    fn parse_inline_optional_bool(&mut self, default: bool) -> Result<bool, Error> {
        match &self.current().kind {
            TokKind::Right | TokKind::Left | TokKind::Eof => Ok(default),
            TokKind::Atom(value) if matches!(value.as_str(), "yes" | "no") => {
                self.parse_bool_atom("boolean")
            }
            TokKind::Atom(_) => Ok(default),
        }
    }

    fn reject_duplicate(&self, duplicate: bool, field: &str) -> Result<(), Error> {
        if duplicate {
            return Err(self.error_here(format!("duplicate {field} section")));
        }
        Ok(())
    }

    fn require_known_version(&self) -> Result<i32, Error> {
        self.version
            .ok_or_else(|| self.error_here("version must appear before this section"))
    }

    fn require_version(&self, minimum: i32, section: &str) -> Result<(), Error> {
        let version = self.require_known_version()?;
        if version < minimum {
            return Err(self.error_here(format!(
                "{section} requires schematic version {minimum} or newer"
            )));
        }
        Ok(())
    }

    fn check_version(&self, version: i32, span: Option<Span>) -> Result<(), Error> {
        if version > SEXPR_SCHEMATIC_FILE_VERSION {
            return Err(self.validation(
                span,
                format!(
                    "future schematic version `{version}` is newer than supported `{SEXPR_SCHEMATIC_FILE_VERSION}`"
                ),
            ));
        }
        Ok(())
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

    fn need_atom(&mut self) -> Result<String, Error> {
        match &self.current().kind {
            TokKind::Atom(value) => {
                let out = value.clone();
                self.idx += 1;
                Ok(out)
            }
            _ => Err(self.expecting("symbol")),
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

    fn need_number_atom(&mut self, expected: &str) -> Result<String, Error> {
        match &self.current().kind {
            TokKind::Atom(value) if self.current().atom_class == Some(AtomClass::Number) => {
                let out = value.clone();
                self.idx += 1;
                Ok(out)
            }
            _ => Err(self.expecting(expected)),
        }
    }

    fn expect_eof(&self) -> Result<(), Error> {
        if matches!(self.current().kind, TokKind::Eof) {
            Ok(())
        } else {
            Err(self.expecting("end of file"))
        }
    }

    fn at_right(&self) -> bool {
        matches!(self.current().kind, TokKind::Right)
    }

    fn at_atom(&self) -> bool {
        matches!(self.current().kind, TokKind::Atom(_))
    }

    fn at_unquoted_symbol_with(&self, expected: &str) -> bool {
        matches!(
            (&self.current().kind, self.current().atom_class),
            (TokKind::Atom(value), Some(AtomClass::Symbol)) if value == expected
        )
    }

    fn current_is_list_named(&self, expected: &str) -> bool {
        matches!(self.current().kind, TokKind::Left)
            && matches!(
                self.tokens.get(self.idx + 1).map(|token| &token.kind),
                Some(TokKind::Atom(value)) if value == expected
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

    fn expecting_known_section(&self, found: &str) -> Error {
        self.validation(
            Some(self.current_span()),
            format!("unsupported schematic section `{found}`"),
        )
    }

    fn error_here(&self, message: impl Into<String>) -> Error {
        self.validation(Some(self.current_span()), message)
    }

    fn normalize_page_number(&self, mut page: String) -> String {
        if page.is_empty() {
            return "#".to_string();
        }
        page.retain(|ch| !matches!(ch, '\r' | '\n' | '\t' | ' '));
        if page.is_empty() {
            "#".to_string()
        } else {
            page
        }
    }

    fn lookup_standard_page_info(kind: &str) -> Option<StandardPageInfo> {
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
        let page_info = Self::lookup_standard_page_info(&raw_kind)
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

    fn normalize_instance_path(&self, path: String, prepend_root_uuid: bool) -> String {
        if !prepend_root_uuid {
            return path;
        }
        let Some(root_uuid) = self.root_uuid.as_ref() else {
            return path;
        };
        if path.is_empty() {
            return String::new();
        }
        let prefix = format!("/{root_uuid}");
        if path == prefix || path.starts_with(&(prefix.clone() + "/")) {
            path
        } else if path.starts_with('/') {
            format!("{prefix}{path}")
        } else {
            format!("{prefix}/{path}")
        }
    }

    fn convert_to_new_overbar_notation(&self, old: String) -> String {
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

    fn update_local_lib_symbol_links(&mut self) {
        let known_symbols: std::collections::HashSet<String> = self
            .screen
            .lib_symbols
            .iter()
            .map(|symbol| symbol.name.clone())
            .collect();

        for item in &mut self.screen.items {
            if let SchItem::Symbol(symbol) = item {
                let lookup_name = symbol
                    .lib_name
                    .as_deref()
                    .unwrap_or(symbol.lib_id.as_str())
                    .to_string();

                symbol.linked_lib_symbol_name =
                    known_symbols.contains(&lookup_name).then_some(lookup_name);
            }
        }
    }

    fn fixup_legacy_lib_symbol_body_styles(&mut self) {
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
            .map(|(idx, symbol)| (symbol.name.clone(), idx))
            .collect();

        let mut cache = std::collections::HashMap::new();

        for idx in 0..self.screen.lib_symbols.len() {
            let has_demorgan = Self::legacy_symbol_has_alternate_body_style(
                idx,
                &self.screen.lib_symbols,
                &symbol_index,
                &mut cache,
            );
            self.screen.lib_symbols[idx].has_demorgan = has_demorgan;
        }
    }

    fn legacy_symbol_has_alternate_body_style(
        idx: usize,
        symbols: &[LibSymbol],
        symbol_index: &std::collections::HashMap<String, usize>,
        cache: &mut std::collections::HashMap<usize, bool>,
    ) -> bool {
        if let Some(cached) = cache.get(&idx) {
            return *cached;
        }

        let symbol = &symbols[idx];

        if symbol.units.iter().any(|unit| unit.body_style > 1) {
            cache.insert(idx, true);
            return true;
        }

        if let Some(parent_name) = symbol.extends.as_ref() {
            if let Some(parent_idx) = symbol_index.get(parent_name) {
                let inherited = Self::legacy_symbol_has_alternate_body_style(
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
        let global_files: std::collections::HashMap<String, String> = self
            .screen
            .embedded_files
            .iter()
            .filter_map(|file| Some((file.name.clone()?, file.data.clone()?)))
            .collect();

        for lib_symbol in &mut self.screen.lib_symbols {
            for embedded_file in &mut lib_symbol.embedded_files {
                if embedded_file.data.is_none() {
                    if let Some(name) = embedded_file.name.as_ref() {
                        if let Some(data) = global_files.get(name) {
                            embedded_file.data = Some(data.clone());
                        }
                    }
                }
            }
        }
    }

    fn set_legacy_symbol_instance_data(&mut self) {
        // Upstream: SCH_SCREEN::SetLegacySymbolInstanceData()
        // For files < 20200828, per-symbol instance data may carry stale value/footprint
        // fields. The C++ code re-adds each instance with only path/reference/unit, which
        // effectively clears value and footprint so that the later post-load
        // UpdateSymbolInstanceData can fill them from the screen-level symbol_instances list.
        for item in &mut self.screen.items {
            if let SchItem::Symbol(symbol) = item {
                for instance in &mut symbol.instances {
                    instance.value = None;
                    instance.footprint = None;
                }
            }
        }
    }

    fn resolve_groups(&mut self) {
        let mut known_uuids = std::collections::HashSet::new();

        for item in &self.screen.items {
            match item {
                SchItem::Junction(item) => {
                    if let Some(uuid) = item.uuid.as_ref() {
                        known_uuids.insert(uuid.clone());
                    }
                }
                SchItem::NoConnect(item) => {
                    if let Some(uuid) = item.uuid.as_ref() {
                        known_uuids.insert(uuid.clone());
                    }
                }
                SchItem::BusEntry(item) => {
                    if let Some(uuid) = item.uuid.as_ref() {
                        known_uuids.insert(uuid.clone());
                    }
                }
                SchItem::Wire(item) | SchItem::Bus(item) | SchItem::Polyline(item) => {
                    if let Some(uuid) = item.uuid.as_ref() {
                        known_uuids.insert(uuid.clone());
                    }
                }
                SchItem::Label(item) => {
                    if let Some(uuid) = item.uuid.as_ref() {
                        known_uuids.insert(uuid.clone());
                    }
                }
                SchItem::Text(item) => {
                    if let Some(uuid) = item.uuid.as_ref() {
                        known_uuids.insert(uuid.clone());
                    }
                }
                SchItem::TextBox(item) => {
                    if let Some(uuid) = item.uuid.as_ref() {
                        known_uuids.insert(uuid.clone());
                    }
                }
                SchItem::Table(item) => {
                    if let Some(uuid) = item.uuid.as_ref() {
                        known_uuids.insert(uuid.clone());
                    }
                }
                SchItem::Image(item) => {
                    if let Some(uuid) = item.uuid.as_ref() {
                        known_uuids.insert(uuid.clone());
                    }
                }
                SchItem::Shape(item) => {
                    if let Some(uuid) = item.uuid.as_ref() {
                        known_uuids.insert(uuid.clone());
                    }
                }
                SchItem::Symbol(item) => {
                    if let Some(uuid) = item.uuid.as_ref() {
                        known_uuids.insert(uuid.clone());
                    }
                }
                SchItem::Sheet(item) => {
                    if let Some(uuid) = item.uuid.as_ref() {
                        known_uuids.insert(uuid.clone());
                    }
                }
                SchItem::Group(_) => {}
            }
        }

        for group in &self.pending_groups {
            if let Some(uuid) = group.uuid.as_ref() {
                known_uuids.insert(uuid.clone());
            }
        }

        self.screen
            .items
            .extend(self.pending_groups.drain(..).map(|mut group| {
                group.members.retain(|member| known_uuids.contains(member));
                SchItem::Group(group)
            }));
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
