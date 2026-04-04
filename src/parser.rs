use std::path::{Path, PathBuf};

use base64::Engine;
use kiutils_sexpr::Span;
use uuid::Uuid;

use crate::diagnostic::Diagnostic;
use crate::error::Error;
use crate::model::{
    BusAlias, BusEntry, EmbeddedFile, EmbeddedFileType, FieldAutoplacement, Fill, FillType, Group,
    Image, ItemVariant, Junction, Label, LabelKind, LabelShape, LabelSpin, LibDrawItem,
    LibPinAlternate, LibSymbol, LibSymbolUnit, Line, LineKind, MirrorAxis, NoConnect, Page, Paper,
    Property, PropertyKind, RootSheet, SchItem, Schematic, Screen, Shape, ShapeKind, Sheet,
    SheetInstance, SheetLocalInstance, SheetPin, SheetPinShape, SheetSide, Stroke, StrokeStyle,
    Symbol, SymbolInstance, SymbolLocalInstance, SymbolPin, Table, Text, TextBox, TextEffects,
    TextHJustify, TextKind, TextVJustify, TitleBlock, VariantField,
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
            screen: Screen {
                uuid: None,
                paper: Some(Paper {
                    kind: page_info.kind.to_string(),
                    width: Some(width),
                    height: Some(height),
                    portrait: false,
                }),
                page: None,
                root_sheet_page: None,
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

        self.parse_schematic_body()?;
        let version = self
            .version
            .ok_or_else(|| self.error_here("missing version"))?;
        self.update_local_lib_symbol_links();
        self.fixup_legacy_lib_symbol_alternate_body_styles();
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
            let head = match &self.current().kind {
                TokKind::Atom(value) if matches!(self.current().atom_class, Some(AtomClass::Symbol)) => {
                    value.clone()
                }
                _ => {
                    return Err(self.expecting(
                        "generator, host, generator_version, uuid, paper, page, title_block, embedded_fonts, embedded_files, lib_symbols, bus_alias, symbol, sheet, junction, no_connect, bus_entry, wire, bus, polyline, label, global_label, hierarchical_label, directive_label, class_label, netclass_flag, text, text_box, table, image, arc, circle, rectangle, bezier, rule_area, sheet_instances, symbol_instances, or group",
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
                }
                "uuid" => {
                    let _ = self.need_unquoted_symbol_atom("uuid")?;
                    let uuid = self.need_symbol_atom("uuid")?;
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
                    let _ = self.need_unquoted_symbol_atom("title_block")?;
                    self.parse_title_block()?
                }
                "embedded_fonts" => {
                    let _ = self.need_unquoted_symbol_atom("embedded_fonts")?;
                    self.screen.embedded_fonts = Some(self.parse_bool_atom("embedded_fonts")?);
                }
                "embedded_files" => {
                    let _ = self.need_unquoted_symbol_atom("embedded_files")?;
                    let version = self.require_known_version()?;
                    if version < VERSION_EMBEDDED_FILES {
                        return Err(self.error_here(format!(
                            "embedded_files requires schematic version {VERSION_EMBEDDED_FILES} or newer"
                        )));
                    }
                    let block_depth = self.current_nesting_depth();
                    match (|| -> Result<Vec<EmbeddedFile>, Error> {
                        let mut files = Vec::new();

                        while !self.at_right() {
                            self.need_left()?;
                            let head = self.need_unquoted_symbol_atom("file")?;
                            if head != "file" {
                                return Err(self.expecting("file"));
                            }
                            let mut file = EmbeddedFile {
                                name: None,
                                checksum: None,
                                file_type: None,
                                data: None,
                            };

                            while !self.at_right() {
                                self.need_left()?;
                                let head =
                                    self.need_unquoted_symbol_atom("checksum, data or name")?;
                                match head.as_str() {
                                    "name" => {
                                        file.name = Some(self.need_symbol_or_number_atom("name")?);
                                    }
                                    "checksum" => {
                                        if file.name.is_none() {
                                            return Err(self.expecting("name"));
                                        }
                                        file.checksum =
                                            Some(self.need_symbol_or_number_atom("checksum data")?);
                                    }
                                    "type" => {
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
                                                _ => return Err(self.expecting(
                                                    "datasheet, font, model, worksheet or other",
                                                )),
                                            },
                                        );
                                    }
                                    "data" => {
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
                                            encoded.push_str(
                                                &self.need_symbol_atom("base64 file data")?,
                                            );
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
                    })() {
                        Ok(files) => self.screen.embedded_files.extend(files),
                        Err(err) => {
                            self.screen.parse_warnings.push(err.to_string());
                            self.skip_to_block_right(block_depth);
                        }
                    }
                }
                "lib_symbols" => {
                    let _ = self.need_unquoted_symbol_atom("lib_symbols")?;
                    self.parse_sch_lib_symbols()?
                }
                "bus_alias" => {
                    let _ = self.need_unquoted_symbol_atom("bus_alias")?;
                    self.parse_bus_alias()?
                }
                "symbol" => {
                    let _ = self.need_unquoted_symbol_atom("symbol")?;
                    let symbol = self.parse_schematic_symbol()?;
                    self.screen.items.push(SchItem::Symbol(symbol));
                }
                "sheet" => {
                    let _ = self.need_unquoted_symbol_atom("sheet")?;
                    let sheet = self.parse_sch_sheet()?;
                    self.screen.items.push(SchItem::Sheet(sheet));
                }
                "junction" => {
                    let _ = self.need_unquoted_symbol_atom("junction")?;
                    let junction = self.parse_junction()?;
                    self.screen.items.push(SchItem::Junction(junction));
                }
                "no_connect" => {
                    let _ = self.need_unquoted_symbol_atom("no_connect")?;
                    let no_connect = self.parse_no_connect()?;
                    self.screen.items.push(SchItem::NoConnect(no_connect));
                }
                "bus_entry" => {
                    let _ = self.need_unquoted_symbol_atom("bus_entry")?;
                    let bus_entry = self.parse_bus_entry()?;
                    self.screen.items.push(SchItem::BusEntry(bus_entry));
                }
                "wire" => {
                    let wire = self.parse_sch_line()?;
                    self.screen.items.push(SchItem::Wire(wire));
                }
                "bus" => {
                    let bus = self.parse_sch_line()?;
                    self.screen.items.push(SchItem::Bus(bus));
                }
                "polyline" => {
                    let _ = self.need_unquoted_symbol_atom("polyline")?;
                    let shape = self.parse_sch_polyline()?;
                    if shape.points.len() < 2 {
                        return Err(self.error_here("Schematic polyline has too few points"));
                    }
                    if shape.points.len() == 2 {
                        self.screen.items.push(SchItem::Polyline(Line {
                            kind: LineKind::Polyline,
                            points: shape.points,
                            has_stroke: shape.has_stroke,
                            stroke: shape.stroke,
                            uuid: shape.uuid,
                        }));
                    } else {
                        self.screen.items.push(SchItem::Shape(shape));
                    }
                }
                "label" | "global_label" | "hierarchical_label" | "directive_label"
                | "class_label" | "netclass_flag" | "text" => {
                    let item = self.parse_sch_text()?;
                    self.screen.items.push(item)
                }
                "text_box" => {
                    let _ = self.need_unquoted_symbol_atom("text_box")?;
                    let text_box = self.parse_sch_text_box()?;
                    self.screen.items.push(SchItem::TextBox(text_box));
                }
                "table" => {
                    let _ = self.need_unquoted_symbol_atom("table")?;
                    let table = self.parse_sch_table()?;
                    self.screen.items.push(SchItem::Table(table));
                }
                "image" => {
                    let _ = self.need_unquoted_symbol_atom("image")?;
                    let image = self.parse_sch_image()?;
                    self.screen.items.push(SchItem::Image(image));
                }
                "arc" => {
                    let _ = self.need_unquoted_symbol_atom("arc")?;
                    let shape = self.parse_sch_arc()?;
                    self.screen.items.push(SchItem::Shape(shape));
                }
                "circle" => {
                    let _ = self.need_unquoted_symbol_atom("circle")?;
                    let shape = self.parse_sch_circle()?;
                    self.screen.items.push(SchItem::Shape(shape));
                }
                "rectangle" => {
                    let _ = self.need_unquoted_symbol_atom("rectangle")?;
                    let shape = self.parse_sch_rectangle()?;
                    self.screen.items.push(SchItem::Shape(shape));
                }
                "bezier" => {
                    let _ = self.need_unquoted_symbol_atom("bezier")?;
                    let shape = self.parse_sch_bezier()?;
                    self.screen.items.push(SchItem::Shape(shape));
                }
                "rule_area" => {
                    let _ = self.need_unquoted_symbol_atom("rule_area")?;
                    let shape = self.parse_sch_rule_area()?;
                    self.screen.items.push(SchItem::Shape(shape));
                }
                "sheet_instances" => {
                    let _ = self.need_unquoted_symbol_atom("sheet_instances")?;
                    self.parse_sch_sheet_instances()?
                }
                "symbol_instances" => {
                    let _ = self.need_unquoted_symbol_atom("symbol_instances")?;
                    self.parse_sch_symbol_instances()?
                }
                "group" => {
                    let _ = self.need_unquoted_symbol_atom("group")?;
                    self.parse_group()?
                }
                _ => {
                    let _ = self.need_unquoted_symbol_atom(
                        "generator, host, generator_version, uuid, paper, page, title_block, embedded_fonts, embedded_files, lib_symbols, bus_alias, symbol, sheet, junction, no_connect, bus_entry, wire, bus, polyline, label, global_label, hierarchical_label, directive_label, class_label, netclass_flag, text, text_box, table, image, arc, circle, rectangle, bezier, rule_area, sheet_instances, symbol_instances, or group",
                    )?;
                    return Err(self.validation(
                        Some(self.current_span()),
                        format!("unsupported schematic section `{head}`"),
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

    fn parse_body_styles(&mut self, symbol: &mut LibSymbol) -> Result<(), Error> {
        while !self.at_right() {
            if self.at_unquoted_symbol_with("demorgan") {
                let _ = self.need_unquoted_symbol_atom("demorgan")?;
                symbol.has_demorgan = true;
            } else {
                symbol
                    .body_style_names
                    .push(self.need_symbol_atom("property value")?);
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
            match self.need_unquoted_symbol_atom("offset or hide")?.as_str() {
                "offset" => {
                    symbol.pin_name_offset = Some(self.parse_f64_atom("pin name offset")?);
                    self.need_right()?;
                }
                "hide" => {
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
            match self.need_unquoted_symbol_atom("hide")?.as_str() {
                "hide" => {
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
        unit_number: i32,
        body_style: i32,
    ) -> Result<LibDrawItem, Error> {
        match self
            .need_unquoted_symbol_atom("arc, bezier, circle, pin, polyline, rectangle, or text")?
            .as_str()
        {
            "arc" => self.parse_symbol_arc(unit_number, body_style),
            "bezier" => self.parse_symbol_bezier(unit_number, body_style),
            "circle" => self.parse_symbol_circle(unit_number, body_style),
            "pin" => self.parse_symbol_pin(unit_number, body_style),
            "polyline" => self.parse_symbol_polyline(unit_number, body_style),
            "rectangle" => self.parse_symbol_rectangle(unit_number, body_style),
            "text" => self.parse_symbol_text(unit_number, body_style),
            "text_box" => self.parse_symbol_text_box(unit_number, body_style),
            _ => Err(self.expecting("arc, bezier, circle, pin, polyline, rectangle, or text")),
        }
    }

    fn parse_lib_symbol(&mut self) -> Result<LibSymbol, Error> {
        let raw_name = self
            .need_symbol_atom("lib symbol name")
            .map_err(|_| self.error_here("Invalid symbol name"))?;
        let name = raw_name.replace("{slash}", "/");

        if let Some(ch) = Self::find_invalid_lib_id_char(&name) {
            return Err(self.error_here(format!("Symbol {name} contains invalid character '{ch}'")));
        }

        if name.is_empty() {
            return Err(self.error_here("Invalid library identifier"));
        }

        let mut symbol = LibSymbol {
            name: name.clone(),
            extends: None,
            power: false,
            local_power: false,
            body_style_names: Vec::new(),
            has_demorgan: false,
            pin_name_offset: None,
            show_pin_names: true,
            show_pin_numbers: true,
            excluded_from_sim: false,
            in_bom: true,
            on_board: true,
            in_pos_files: true,
            duplicate_pin_numbers_are_jumpers: false,
            jumper_pin_groups: Vec::new(),
            keywords: None,
            description: None,
            fp_filters: Vec::new(),
            locked_units: false,
            properties: Vec::new(),
            units: vec![crate::model::LibSymbolUnit {
                name: format!("{name}_1_1"),
                unit_number: 1,
                body_style: 1,
                unit_name: None,
                draw_item_kinds: Vec::new(),
                draw_items: Vec::new(),
            }],
            embedded_fonts: None,
            embedded_files: Vec::new(),
        };

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
                    if matches!(self.current().kind, TokKind::Atom(_)) {
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
                    while !self.at_right() {
                        self.need_left()?;
                        let mut group = Vec::new();
                        while !self.at_right() {
                            group.push(self.need_quoted_atom("list of pin names")?);
                        }
                        self.need_right()?;
                        symbol.jumper_pin_groups.push(group);
                    }
                    self.need_right()?;
                }
                "property" => {
                    let _ = self.need_unquoted_symbol_atom("property")?;
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

                    let unit_name = unit_full_name;
                    let unit_index = if let Some(index) = symbol.units.iter().position(|existing| {
                        existing.unit_number == unit_number
                            && existing.body_style == body_style
                            && existing.name == unit_name
                    }) {
                        index
                    } else {
                        symbol.units.push(LibSymbolUnit {
                            name: unit_name.clone(),
                            unit_number,
                            body_style,
                            unit_name: None,
                            draw_item_kinds: Vec::new(),
                            draw_items: Vec::new(),
                        });
                        symbol.units.len() - 1
                    };

                    while !self.at_right() {
                        self.need_left()?;
                        if self.at_unquoted_symbol_with("unit_name") {
                            let _ = self.need_unquoted_symbol_atom("unit_name")?;
                            if matches!(
                                self.current().atom_class,
                                Some(AtomClass::Symbol | AtomClass::Quoted)
                            ) {
                                symbol.units[unit_index].unit_name =
                                    Some(self.need_symbol_atom("unit_name")?);
                            }
                            self.need_right()?;
                        } else {
                            let item = self.parse_symbol_draw_item(unit_number, body_style)?;
                            self.need_right()?;
                            symbol.units[unit_index].draw_item_kinds.push(item.kind.clone());
                            symbol.units[unit_index].draw_items.push(item);
                        }
                    }
                    self.need_right()?;
                }
                "arc" | "bezier" | "circle" | "pin" | "polyline" | "rectangle" | "text"
                | "text_box" => {
                    let item = self.parse_symbol_draw_item(1, 1)?;
                    self.need_right()?;
                    symbol.units[0].draw_item_kinds.push(item.kind.clone());
                    symbol.units[0].draw_items.push(item);
                }
                "embedded_fonts" => {
                    let _ = self.need_unquoted_symbol_atom("embedded_fonts")?;
                    symbol.embedded_fonts = Some(self.parse_bool_atom("embedded_fonts")?);
                    self.need_right()?;
                }
                "embedded_files" => {
                    let _ = self.need_unquoted_symbol_atom("embedded_files")?;
                    let block_depth = self.current_nesting_depth();
                    match (|| -> Result<Vec<EmbeddedFile>, Error> {
                        let mut files = Vec::new();

                        while !self.at_right() {
                            self.need_left()?;
                            let head = self.need_unquoted_symbol_atom("file")?;
                            if head != "file" {
                                return Err(self.expecting("file"));
                            }
                            let mut file = EmbeddedFile {
                                name: None,
                                checksum: None,
                                file_type: None,
                                data: None,
                            };

                            while !self.at_right() {
                                self.need_left()?;
                                let head =
                                    self.need_unquoted_symbol_atom("checksum, data or name")?;
                                match head.as_str() {
                                    "name" => {
                                        file.name = Some(self.need_symbol_or_number_atom("name")?);
                                    }
                                    "checksum" => {
                                        if file.name.is_none() {
                                            return Err(self.expecting("name"));
                                        }
                                        file.checksum =
                                            Some(self.need_symbol_or_number_atom("checksum data")?);
                                    }
                                    "type" => {
                                        if file.name.is_none() {
                                            return Err(self.expecting("name"));
                                        }
                                        file.file_type =
                                            Some(match self.need_unquoted_symbol_atom(
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
                                                    return Err(self.expecting(
                                                        "datasheet, font, model, worksheet or other",
                                                    ))
                                                }
                                            });
                                    }
                                    "data" => {
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
                                            encoded.push_str(&self.need_symbol_atom(
                                                "base64 file data",
                                            )?);
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
                    })() {
                        Ok(files) => symbol.embedded_files = files,
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

        Ok(symbol)
    }

    fn parse_symbol_arc(
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

    fn parse_symbol_bezier(
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

    fn parse_symbol_circle(
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

    fn parse_symbol_polyline(
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

    fn parse_symbol_rectangle(
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

    fn parse_symbol_text(
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
            kind: "text".to_string(),
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
            text: Some(
                self.need_symbol_atom("text string")
                    .map_err(|_| self.error_here("Invalid text string"))?,
            ),
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
        };

        while !self.at_right() {
            self.need_left()?;
            match self.need_unquoted_symbol_atom("at or effects")?.as_str() {
                "at" => {
                    let parsed = self.parse_xy3("text at")?;
                    item.at = Some([parsed[0], parsed[1]]);
                    item.angle = Some(parsed[2] / 10.0);
                    self.need_right()?;
                }
                "effects" => {
                    let mut parsed = TextEffects::default();
                    self.parse_eda_text(
                        item.text.as_mut(),
                        &mut parsed,
                        &mut item.visible,
                        true,
                        false,
                    )?;
                    item.effects = Some(parsed);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("at or effects")),
            }
        }

        if !item.visible {
            item.kind = "field".to_string();
        }

        Ok(item)
    }

    fn parse_symbol_text_box(
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
            kind: "text_box".to_string(),
            is_private,
            unit_number,
            body_style,
            visible: true,
            at: None,
            angle: Some(0.0),
            points: Vec::new(),
            end: None,
            radius: None,
            arc_center: None,
            arc_start_angle: None,
            arc_end_angle: None,
            length: None,
            text: Some(
                self.need_symbol_atom("text box text")
                    .map_err(|_| self.error_here("Invalid text string"))?,
            ),
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
        };
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
            let head = self.need_unquoted_symbol_atom("at, size, stroke, fill or effects")?;
            match head.as_str() {
                "start" => {
                    pos = Some(self.parse_xy2("text_box start")?);
                    self.need_right()?;
                }
                "end" => {
                    end = Some(self.parse_xy2("text_box end")?);
                    found_end = true;
                    self.need_right()?;
                }
                "at" => {
                    let parsed = self.parse_xy3("text_box at")?;
                    pos = Some([parsed[0], parsed[1]]);
                    item.angle = Some(parsed[2]);
                    self.need_right()?;
                }
                "size" => {
                    size = Some(self.parse_xy2("text_box size")?);
                    found_size = true;
                    self.need_right()?;
                }
                "stroke" => {
                    let parsed_stroke = self.parse_stroke()?;
                    stroke_width = parsed_stroke.width;
                    item.stroke = Some(parsed_stroke);
                }
                "fill" => {
                    item.fill = Some(self.parse_fill()?);
                }
                "effects" => {
                    let mut parsed_effects = TextEffects::default();
                    let mut visible = item.visible;
                    self.parse_eda_text(None, &mut parsed_effects, &mut visible, false, true)?;
                    text_size_y = parsed_effects.font_size.map(|size| size[1]);
                    item.visible = visible;
                    item.effects = Some(parsed_effects);
                    self.need_right()?;
                }
                "margins" => {
                    let _margins = [
                        self.parse_f64_atom("margin left")?,
                        self.parse_f64_atom("margin top")?,
                        self.parse_f64_atom("margin right")?,
                        self.parse_f64_atom("margin bottom")?,
                    ];
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
            let _margins = Some({
                let margin = Self::get_legacy_text_margin(
                    stroke_width.unwrap_or(DEFAULT_LINE_WIDTH_MM),
                    text_size_y.unwrap_or(DEFAULT_TEXT_SIZE_MM),
                );
                [margin, margin, margin, margin]
            });
        }

        Ok(item)
    }

    fn parse_symbol_pin(
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
        let mut item = LibDrawItem {
            kind: "pin".to_string(),
            is_private: false,
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
            electrical_type: Some(electrical_type),
            graphic_shape: Some(graphic_shape),
            alternates: Vec::new(),
            stroke: None,
            fill: None,
            effects: None,
        };

        while !self.at_right() {
            if self.at_unquoted_symbol_with("hide") {
                let _ = self.need_unquoted_symbol_atom("hide")?;
                item.visible = false;
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
                    item.at = Some([parsed[0], parsed[1]]);
                    item.angle = Some(parsed[2]);
                    self.need_right()?;
                }
                "length" => {
                    item.length = Some(self.parse_f64_atom("pin length")?);
                    self.need_right()?;
                }
                "hide" => {
                    item.visible = !self.parse_bool_atom("hide")?;
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
                        parsed = self.convert_old_overbar_notation(parsed);
                    }
                    item.name = Some(parsed);
                    if self.at_right() {
                        self.need_right()?;
                        continue;
                    }
                    self.need_left()?;
                    if self.need_unquoted_symbol_atom("effects")? != "effects" {
                        return Err(self.expecting("effects"));
                    }
                    let mut parsed = TextEffects::default();
                    let mut visible = true;
                    self.parse_eda_text(item.name.as_mut(), &mut parsed, &mut visible, true, true)?;
                    item.name_effects = Some(parsed);
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
                        parsed = self.convert_old_overbar_notation(parsed);
                    }
                    item.number = Some(parsed);
                    if self.at_right() {
                        self.need_right()?;
                        continue;
                    }
                    self.need_left()?;
                    if self.need_unquoted_symbol_atom("effects")? != "effects" {
                        return Err(self.expecting("effects"));
                    }
                    let mut parsed = TextEffects::default();
                    let mut visible = true;
                    self.parse_eda_text(
                        item.number.as_mut(),
                        &mut parsed,
                        &mut visible,
                        true,
                        true,
                    )?;
                    item.number_effects = Some(parsed);
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
                    item.alternates.push(LibPinAlternate {
                        name: alt_name,
                        electrical_type: alt_type,
                        graphic_shape: alt_shape,
                    });
                    self.need_right()?;
                }
                _ => return Err(self.expecting("at, name, number, hide, length, or alternate")),
            }
        }

        Ok(item)
    }

    fn parse_lib_property(&mut self, symbol: &mut LibSymbol) -> Result<(), Error> {
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
        let field_id = match name.to_ascii_lowercase().as_str() {
            "reference" => PropertyKind::SymbolReference,
            "value" => PropertyKind::SymbolValue,
            "footprint" => PropertyKind::SymbolFootprint,
            "datasheet" => PropertyKind::SymbolDatasheet,
            _ => PropertyKind::User,
        };
        let mut property = Property {
            id: field_id.default_field_id(),
            key: match field_id {
                PropertyKind::SymbolReference => "Reference".to_string(),
                PropertyKind::SymbolValue => "Value".to_string(),
                PropertyKind::SymbolFootprint => "Footprint".to_string(),
                PropertyKind::SymbolDatasheet => "Datasheet".to_string(),
                _ => name.clone(),
            },
            value: String::new(),
            kind: field_id,
            is_private,
            at: None,
            angle: None,
            visible: true,
            show_name: true,
            can_autoplace: true,
            has_effects: false,
            effects: None,
        };

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
                    let mut effects = TextEffects::default();
                    self.parse_eda_text(
                        Some(&mut property.value),
                        &mut effects,
                        &mut property.visible,
                        property.kind == PropertyKind::SymbolValue,
                        true,
                    )?;
                    property.has_effects = true;
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

        if matches!(
            property.kind,
            PropertyKind::SymbolReference
                | PropertyKind::SymbolValue
                | PropertyKind::SymbolFootprint
                | PropertyKind::SymbolDatasheet
        ) {
            if let Some(existing) = symbol
                .properties
                .iter_mut()
                .find(|existing| existing.kind == property.kind)
            {
                *existing = property;
            } else {
                symbol.properties.push(property);
            }
        } else if name == "ki_keywords" {
            symbol.keywords = Some(property.value);
        } else if name == "ki_description" {
            symbol.description = Some(property.value);
        } else if name == "ki_fp_filters" {
            symbol.fp_filters = property
                .value
                .split_whitespace()
                .map(str::to_string)
                .collect();
        } else if name == "ki_locked" {
            symbol.locked_units = true;
        } else {
            let mut property = property;
            let mut existing = symbol
                .properties
                .iter()
                .any(|existing| existing.key == property.key);

            if existing {
                let base = property.key.clone();

                for suffix in 1..10 {
                    let candidate = format!("{base}_{suffix}");

                    if !symbol
                        .properties
                        .iter()
                        .any(|existing| existing.key == candidate)
                    {
                        property.key = candidate;
                        existing = false;
                        break;
                    }
                }
            }

            if !existing {
                symbol.properties.push(property);
            }
        }

        self.need_right()?;
        Ok(())
    }

    fn parse_bus_alias(&mut self) -> Result<(), Error> {
        let mut alias = BusAlias {
            name: self.need_symbol_atom("bus alias name")?,
            members: Vec::new(),
        };
        let version = self.require_known_version()?;
        if version < VERSION_NEW_OVERBAR_NOTATION {
            alias.name = self.convert_old_overbar_notation(alias.name);
        }

        self.need_left()?;
        if self.need_unquoted_symbol_atom("members")? != "members" {
            return Err(self.expecting("members"));
        }

        while !self.at_right() {
            let mut member = self.need_quoted_atom("quoted string")?;
            if version < VERSION_NEW_OVERBAR_NOTATION {
                member = self.convert_old_overbar_notation(member);
            }
            alias.members.push(member);
        }
        self.need_right()?;
        self.screen.bus_aliases.push(alias);
        Ok(())
    }

    fn parse_junction(&mut self) -> Result<Junction, Error> {
        let mut junction = Junction {
            at: [0.0, 0.0],
            diameter: None,
            color: None,
            uuid: None,
        };
        let mut has_at = false;
        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom("at, diameter, color or uuid")?;
            match head.as_str() {
                "at" => {
                    junction.at = self.parse_xy2("junction at")?;
                    has_at = true;
                    self.need_right()?;
                }
                "diameter" => {
                    junction.diameter = Some(self.parse_f64_atom("junction diameter")?);
                    self.need_right()?;
                }
                "color" => {
                    junction.color = Some([
                        f64::from(self.parse_i32_atom("red")?) / 255.0,
                        f64::from(self.parse_i32_atom("green")?) / 255.0,
                        f64::from(self.parse_i32_atom("blue")?) / 255.0,
                        self.parse_f64_atom("alpha")?.clamp(0.0, 1.0),
                    ]);
                    self.need_right()?;
                }
                "uuid" => {
                    junction.uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("at, diameter, color or uuid")),
            }
        }
        if !has_at {
            junction.at = [0.0, 0.0];
        }
        Ok(junction)
    }

    fn parse_no_connect(&mut self) -> Result<NoConnect, Error> {
        let mut no_connect = NoConnect {
            at: [0.0, 0.0],
            uuid: None,
        };
        let mut has_at = false;
        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom("at or uuid")?;
            match head.as_str() {
                "at" => {
                    no_connect.at = self.parse_xy2("no_connect at")?;
                    has_at = true;
                    self.need_right()?;
                }
                "uuid" => {
                    no_connect.uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("at or uuid")),
            }
        }
        if !has_at {
            no_connect.at = [0.0, 0.0];
        }
        Ok(no_connect)
    }

    fn parse_bus_entry(&mut self) -> Result<BusEntry, Error> {
        let mut bus_entry = BusEntry {
            at: [0.0, 0.0],
            size: [0.0, 0.0],
            has_stroke: false,
            stroke: None,
            uuid: None,
        };
        let mut has_at = false;
        let mut has_size = false;
        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom("at, size, uuid or stroke")?;
            match head.as_str() {
                "at" => {
                    bus_entry.at = self.parse_xy2("bus_entry at")?;
                    has_at = true;
                    self.need_right()?;
                }
                "size" => {
                    bus_entry.size = self.parse_xy2("bus_entry size")?;
                    has_size = true;
                    self.need_right()?;
                }
                "stroke" => {
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
                    bus_entry.uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("at, size, uuid or stroke")),
            }
        }
        if !has_at {
            bus_entry.at = [0.0, 0.0];
        }
        if !has_size {
            bus_entry.size = [0.0, 0.0];
        }
        Ok(bus_entry)
    }

    fn parse_sch_line(&mut self) -> Result<Line, Error> {
        let kind = match self.need_unquoted_symbol_atom("wire or bus")?.as_str() {
            "wire" => LineKind::Wire,
            "bus" => LineKind::Bus,
            _ => return Err(self.error_here("invalid schematic line kind")),
        };
        let mut line = Line {
            kind,
            points: vec![[0.0, 0.0], [0.0, 0.0]],
            has_stroke: false,
            stroke: None,
            uuid: None,
        };
        let mut has_pts = false;
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
                    line.points = vec![start, end];
                    has_pts = true;
                }
                "uuid" => {
                    line.uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                "stroke" => {
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

        match target {
            SchTextTarget::Text => {
                let mut text = Text {
                    kind: TextKind::Text,
                    text,
                    at: None,
                    excluded_from_sim: false,
                    fields_autoplaced: FieldAutoplacement::None,
                    visible: true,
                    has_effects: false,
                    effects: None,
                    uuid: None,
                };

                while !self.at_right() {
                    self.need_left()?;
                    let head =
                        self.need_unquoted_symbol_atom("at, shape, iref, uuid or effects")?;
                    match head.as_str() {
                        "exclude_from_sim" => {
                            text.excluded_from_sim = self.parse_bool_atom("exclude_from_sim")?;
                            self.need_right()?;
                        }
                        "at" => {
                            let parsed = self.parse_xy3("text at")?;
                            text.at =
                                Some([parsed[0], parsed[1], Self::normalize_text_angle(parsed[2])]);
                            self.need_right()?;
                        }
                        "fields_autoplaced" => {
                            if self.parse_maybe_absent_bool(true)? {
                                text.fields_autoplaced = FieldAutoplacement::Auto;
                            }
                            self.need_right()?;
                        }
                        "effects" => {
                            let mut parsed_effects = TextEffects::default();
                            self.parse_eda_text(
                                Some(&mut text.text),
                                &mut parsed_effects,
                                &mut text.visible,
                                true,
                                true,
                            )?;
                            text.has_effects = true;
                            text.effects = Some(parsed_effects);
                            text.visible = true;
                            self.need_right()?;
                        }
                        "shape" => return Err(self.unexpected("shape")),
                        "length" => return Err(self.unexpected("length")),
                        "iref" => {}
                        "uuid" => {
                            text.uuid = Some(self.need_symbol_atom("uuid")?);
                            self.need_right()?;
                        }
                        "property" => return Err(self.unexpected("property")),
                        _ => return Err(self.expecting("at, shape, iref, uuid or effects")),
                    }
                }

                Ok(SchItem::Text(text))
            }
            SchTextTarget::Label(kind) => {
                let mut label = Label {
                    kind,
                    text,
                    at: [0.0, 0.0],
                    angle: 0.0,
                    spin: Some(LabelSpin::Right),
                    shape: None,
                    pin_length: None,
                    iref_at: None,
                    excluded_from_sim: false,
                    fields_autoplaced: FieldAutoplacement::None,
                    visible: true,
                    has_effects: false,
                    effects: None,
                    uuid: None,
                    properties: Vec::new(),
                };

                if matches!(label.kind, LabelKind::Global) {
                    label.properties.push(Property {
                        id: PropertyKind::GlobalLabelIntersheetRefs.default_field_id(),
                        key: PropertyKind::GlobalLabelIntersheetRefs
                            .canonical_key()
                            .to_string(),
                        value: "${INTERSHEET_REFS}".to_string(),
                        kind: PropertyKind::GlobalLabelIntersheetRefs,
                        is_private: false,
                        at: Some([0.0, 0.0]),
                        angle: None,
                        visible: false,
                        show_name: true,
                        can_autoplace: true,
                        has_effects: false,
                        effects: None,
                    });
                }

                while !self.at_right() {
                    self.need_left()?;
                    let head =
                        self.need_unquoted_symbol_atom("at, shape, iref, uuid or effects")?;
                    match head.as_str() {
                        "exclude_from_sim" => {
                            label.excluded_from_sim = self.parse_bool_atom("exclude_from_sim")?;
                            self.need_right()?;
                        }
                        "at" => {
                            let parsed = self.parse_xy3("text at")?;
                            label.at = [parsed[0], parsed[1]];
                            label.angle = Self::normalize_text_angle(parsed[2]);
                            label.spin = Self::get_label_spin_style(label.angle);
                            self.need_right()?;
                        }
                        "shape" => {
                            if matches!(label.kind, LabelKind::Local) {
                                return Err(self.unexpected("shape"));
                            }
                            label.shape = Some(match self.need_unquoted_symbol_atom("shape")?.as_str() {
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
                            if !matches!(label.kind, LabelKind::Directive | LabelKind::NetclassFlag)
                            {
                                return Err(self.unexpected("length"));
                            }
                            label.pin_length = Some(self.parse_f64_atom("pin length")?);
                            self.need_right()?;
                        }
                        "fields_autoplaced" => {
                            if self.parse_maybe_absent_bool(true)? {
                                label.fields_autoplaced = FieldAutoplacement::Auto;
                            }
                            self.need_right()?;
                        }
                        "effects" => {
                            let mut parsed_effects = TextEffects::default();
                            self.parse_eda_text(
                                Some(&mut label.text),
                                &mut parsed_effects,
                                &mut label.visible,
                                true,
                                true,
                            )?;
                            label.has_effects = true;
                            label.effects = Some(parsed_effects);
                            self.need_right()?;
                        }
                        "iref" => {
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
                            label.uuid = Some(self.need_symbol_atom("uuid")?);
                            self.need_right()?;
                        }
                        "property" => {
                            let property = if matches!(label.kind, LabelKind::Global) {
                                self.parse_sch_field(FieldParent::GlobalLabel)?
                            } else {
                                self.parse_sch_field(FieldParent::OtherLabel)?
                            };

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
                            self.need_right()?;
                        }
                        _ => return Err(self.expecting("at, shape, iref, uuid or effects")),
                    }
                }

                if label.properties.is_empty() {
                    label.fields_autoplaced = FieldAutoplacement::Auto;
                }

                Ok(SchItem::Label(label))
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
        let mut text_box = TextBox {
            text: self
                .need_symbol_atom("text box text")
                .map_err(|_| self.error_here("Invalid text string"))?,
            at: [0.0, 0.0],
            angle: 0.0,
            end: [0.0, 0.0],
            excluded_from_sim: false,
            has_effects: false,
            effects: None,
            stroke: None,
            fill: None,
            span: None,
            margins: None,
            uuid: None,
        };
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
            let head = self.need_unquoted_symbol_atom(if table_cell {
                "at, size, stroke, fill, effects, span or uuid"
            } else {
                "at, size, stroke, fill, effects or uuid"
            })?;
            match head.as_str() {
                "exclude_from_sim" => {
                    text_box.excluded_from_sim = self.parse_bool_atom("exclude_from_sim")?;
                    self.need_right()?;
                }
                "start" => {
                    pos = Some(self.parse_xy2("text_box start")?);
                    self.need_right()?;
                }
                "end" => {
                    end = Some(self.parse_xy2("text_box end")?);
                    found_end = true;
                    self.need_right()?;
                }
                "at" => {
                    let parsed = self.parse_xy3("text_box at")?;
                    pos = Some([parsed[0], parsed[1]]);
                    text_box.angle = parsed[2];
                    self.need_right()?;
                }
                "size" => {
                    size = Some(self.parse_xy2("text_box size")?);
                    found_size = true;
                    self.need_right()?;
                }
                "span" if table_cell => {
                    text_box.span = Some([
                        self.parse_i32_atom("column span")?,
                        self.parse_i32_atom("row span")?,
                    ]);
                    self.need_right()?;
                }
                "stroke" => {
                    let parsed_stroke = self.parse_stroke()?;
                    stroke_width = parsed_stroke.width;
                    text_box.stroke = Some(parsed_stroke);
                }
                "fill" => {
                    text_box.fill = Some(self.parse_fill()?);
                    Self::fixup_sch_fill_mode(&mut text_box.fill, &text_box.stroke);
                }
                "effects" => {
                    let mut parsed_effects = TextEffects::default();
                    let mut visible = true;
                    self.parse_eda_text(
                        Some(&mut text_box.text),
                        &mut parsed_effects,
                        &mut visible,
                        false,
                        true,
                    )?;
                    text_box.has_effects = true;
                    text_size_y = parsed_effects.font_size.map(|size| size[1]);
                    text_box.effects = Some(parsed_effects);
                    self.need_right()?;
                }
                "margins" => {
                    text_box.margins = Some([
                        self.parse_f64_atom("margin left")?,
                        self.parse_f64_atom("margin top")?,
                        self.parse_f64_atom("margin right")?,
                        self.parse_f64_atom("margin bottom")?,
                    ]);
                    found_margins = true;
                    self.need_right()?;
                }
                "uuid" => {
                    text_box.uuid = Some(self.need_symbol_atom("uuid")?);
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

        text_box.at = pos.unwrap_or([0.0, 0.0]);
        text_box.end = if found_end {
            end.unwrap_or([0.0, 0.0])
        } else if found_size {
            let size = size.unwrap_or([0.0, 0.0]);
            [text_box.at[0] + size[0], text_box.at[1] + size[1]]
        } else {
            return Err(self.expecting("size"));
        };
        if !found_margins {
            text_box.margins = Some({
                let margin = Self::get_legacy_text_margin(
                    stroke_width.unwrap_or(DEFAULT_LINE_WIDTH_MM),
                    text_size_y.unwrap_or(DEFAULT_TEXT_SIZE_MM),
                );
                [margin, margin, margin, margin]
            });
        }

        Ok(text_box)
    }

    fn parse_sch_table(&mut self) -> Result<Table, Error> {
        let version = self.require_known_version()?;
        if version < VERSION_TABLES {
            return Err(self.error_here(format!(
                "table requires schematic version {VERSION_TABLES} or newer"
            )));
        }
        let mut table = Table {
            column_count: None,
            column_widths: Vec::new(),
            row_heights: Vec::new(),
            cells: Vec::new(),
            border_external: None,
            border_header: None,
            border_stroke: None,
            separators_rows: None,
            separators_cols: None,
            separators_stroke: None,
            uuid: None,
        };
        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom(
                "columns, col_widths, row_heights, border, separators, uuid, header or cells",
            )?;
            match head.as_str() {
                "column_count" => {
                    table.column_count = Some(self.parse_i32_atom("column count")?);
                    self.need_right()?;
                }
                "column_widths" => {
                    let mut values = Vec::new();
                    while !self.at_right() {
                        values.push(self.parse_f64_atom("column width")?);
                    }
                    table.column_widths = values;
                    self.need_right()?;
                }
                "row_heights" => {
                    let mut values = Vec::new();
                    while !self.at_right() {
                        values.push(self.parse_f64_atom("row height")?);
                    }
                    table.row_heights = values;
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
                        table.cells.push(cell);
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
                                table.border_external = Some(self.parse_bool_atom("external")?);
                                self.need_right()?;
                            }
                            "header" => {
                                table.border_header = Some(self.parse_bool_atom("header")?);
                                self.need_right()?;
                            }
                            "stroke" => {
                                table.border_stroke = Some(self.parse_stroke()?);
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
                                table.separators_rows = Some(self.parse_bool_atom("rows")?);
                                self.need_right()?;
                            }
                            "cols" => {
                                table.separators_cols = Some(self.parse_bool_atom("cols")?);
                                self.need_right()?;
                            }
                            "stroke" => {
                                table.separators_stroke = Some(self.parse_stroke()?);
                            }
                            _ => return Err(self.expecting("rows, cols, or stroke")),
                        }
                    }
                    self.need_right()?;
                }
                "uuid" => {
                    table.uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                _ => {
                    return Err(self.expecting(
                        "columns, col_widths, row_heights, border, separators, uuid, header or cells",
                    ));
                }
            }
        }
        if table.cells.is_empty() {
            return Err(self.error_here("Invalid table: no cells defined"));
        }
        Ok(table)
    }

    fn parse_sch_image(&mut self) -> Result<Image, Error> {
        let mut image = Image {
            at: [0.0, 0.0],
            scale: 1.0,
            data: None,
            uuid: None,
        };
        let mut has_at = false;
        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom("at, scale, uuid or data")?;
            match head.as_str() {
                "at" => {
                    image.at = self.parse_xy2("image at")?;
                    has_at = true;
                    self.need_right()?;
                }
                "scale" => {
                    let parsed_scale = self.parse_f64_atom("image scale factor")?;
                    image.scale = if parsed_scale.is_normal() {
                        parsed_scale
                    } else {
                        1.0
                    };
                    self.need_right()?;
                }
                "uuid" => {
                    image.uuid = Some(self.need_symbol_atom("uuid")?);
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
        Ok(image)
    }

    fn parse_sch_polyline(&mut self) -> Result<Shape, Error> {
        let mut shape = Shape {
            kind: ShapeKind::Polyline,
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
                    shape.points = parsed_points;
                    self.need_right()?;
                }
                "stroke" => {
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
                    shape.has_fill = true;
                    shape.fill = Some(self.parse_fill()?);
                }
                "uuid" => {
                    shape.uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                _ => {
                    return Err(self.expecting("pts, uuid, stroke, or fill"));
                }
            }
        }
        Self::fixup_sch_fill_mode(&mut shape.fill, &shape.stroke);
        Ok(shape)
    }

    fn parse_sch_arc(&mut self) -> Result<Shape, Error> {
        let mut shape = Shape {
            kind: ShapeKind::Arc,
            points: vec![[0.0, 0.0], [0.0, 0.0], [0.0, 0.0]],
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
                .need_unquoted_symbol_atom("start, mid, end, stroke, fill or uuid")?
                .as_str()
            {
                "start" => {
                    shape.points[0] = self.parse_xy2("shape start")?;
                    self.need_right()?;
                }
                "mid" => {
                    shape.points[1] = self.parse_xy2("shape mid")?;
                    self.need_right()?;
                }
                "end" => {
                    shape.points[2] = self.parse_xy2("shape end")?;
                    self.need_right()?;
                }
                "stroke" => {
                    shape.has_stroke = true;
                    shape.stroke = Some(self.parse_stroke()?);
                }
                "fill" => {
                    shape.has_fill = true;
                    shape.fill = Some(self.parse_fill()?);
                }
                "uuid" => {
                    shape.uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("start, mid, end, stroke, fill or uuid")),
            }
        }
        Self::fixup_sch_fill_mode(&mut shape.fill, &shape.stroke);
        Ok(shape)
    }

    fn parse_sch_circle(&mut self) -> Result<Shape, Error> {
        let mut shape = Shape {
            kind: ShapeKind::Circle,
            points: vec![[0.0, 0.0]],
            radius: Some(0.0),
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
                .need_unquoted_symbol_atom("center, radius, stroke, fill or uuid")?
                .as_str()
            {
                "center" => {
                    shape.points[0] = self.parse_xy2("center")?;
                    self.need_right()?;
                }
                "radius" => {
                    shape.radius = Some(self.parse_f64_atom("radius length")?);
                    self.need_right()?;
                }
                "stroke" => {
                    shape.has_stroke = true;
                    shape.stroke = Some(self.parse_stroke()?);
                }
                "fill" => {
                    shape.has_fill = true;
                    shape.fill = Some(self.parse_fill()?);
                }
                "uuid" => {
                    shape.uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("center, radius, stroke, fill or uuid")),
            }
        }
        Self::fixup_sch_fill_mode(&mut shape.fill, &shape.stroke);
        Ok(shape)
    }

    fn parse_sch_rectangle(&mut self) -> Result<Shape, Error> {
        let mut shape = Shape {
            kind: ShapeKind::Rectangle,
            points: vec![[0.0, 0.0], [0.0, 0.0]],
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
                .need_unquoted_symbol_atom("start, end, stroke, fill or uuid")?
                .as_str()
            {
                "start" => {
                    shape.points[0] = self.parse_xy2("start")?;
                    self.need_right()?;
                }
                "end" => {
                    shape.points[1] = self.parse_xy2("end")?;
                    self.need_right()?;
                }
                "radius" => {
                    shape.corner_radius = Some(self.parse_f64_atom("corner radius")?);
                    self.need_right()?;
                }
                "stroke" => {
                    shape.has_stroke = true;
                    shape.stroke = Some(self.parse_stroke()?);
                }
                "fill" => {
                    shape.has_fill = true;
                    shape.fill = Some(self.parse_fill()?);
                }
                "uuid" => {
                    shape.uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("start, end, stroke, fill or uuid")),
            }
        }
        Self::fixup_sch_fill_mode(&mut shape.fill, &shape.stroke);
        Ok(shape)
    }

    fn parse_sch_bezier(&mut self) -> Result<Shape, Error> {
        let mut shape = Shape {
            kind: ShapeKind::Bezier,
            points: vec![[0.0, 0.0], [0.0, 0.0], [0.0, 0.0], [0.0, 0.0]],
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
                            0..=3 => shape.points[ii] = self.parse_xy2("xy")?,
                            _ => return Err(self.unexpected("control point")),
                        }
                        ii += 1;
                        self.need_right()?;
                    }
                    self.need_right()?;
                }
                "stroke" => {
                    shape.has_stroke = true;
                    shape.stroke = Some(self.parse_stroke()?);
                }
                "fill" => {
                    shape.has_fill = true;
                    shape.fill = Some(self.parse_fill()?);
                }
                "uuid" => {
                    shape.uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("pts, stroke, fill or uuid")),
            }
        }
        Self::fixup_sch_fill_mode(&mut shape.fill, &shape.stroke);
        Ok(shape)
    }

    fn parse_sch_rule_area(&mut self) -> Result<Shape, Error> {
        let version = self.require_known_version()?;
        if version < VERSION_RULE_AREAS {
            return Err(self.error_here(format!(
                "rule_area requires schematic version {VERSION_RULE_AREAS} or newer"
            )));
        }
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
                    let polyline = self.parse_sch_polyline()?;
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

    fn parse_schematic_symbol(&mut self) -> Result<Symbol, Error> {
        let mut symbol = Symbol {
            lib_id: String::new(),
            lib_name: None,
            linked_lib_symbol_name: None,
            at: [0.0, 0.0],
            angle: 0.0,
            mirror: None,
            unit: None,
            body_style: None,
            excluded_from_sim: false,
            in_bom: true,
            on_board: true,
            in_pos_files: true,
            dnp: false,
            fields_autoplaced: FieldAutoplacement::None,
            uuid: None,
            properties: Vec::new(),
            instances: Vec::new(),
            pins: Vec::new(),
        };

        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom(
                "lib_id, lib_name, at, mirror, uuid, exclude_from_sim, on_board, in_bom, dnp, default_instance, property, pin, or instances",
            )?;
            match head.as_str() {
                "lib_id" => {
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
                "lib_name" => {
                    symbol.lib_name = Some(
                        self.need_symbol_atom("lib_name")
                            .map_err(|_| self.error_here("Invalid symbol library name"))?
                            .replace("{slash}", "/"),
                    );
                    self.need_right()?;
                }
                "at" => {
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
                    symbol.mirror = Some(
                        match self.need_unquoted_symbol_atom("mirror axis")?.as_str() {
                            "x" => MirrorAxis::X,
                            "y" => MirrorAxis::Y,
                            _ => return Err(self.expecting("x or y")),
                        },
                    );
                    self.need_right()?;
                }
                "convert" | "body_style" => {
                    symbol.body_style = Some(self.parse_i32_atom("symbol body style")?);
                    self.need_right()?;
                }
                "unit" => {
                    symbol.unit = Some(self.parse_i32_atom("unit")?);
                    self.need_right()?;
                }
                "exclude_from_sim" => {
                    symbol.excluded_from_sim = self.parse_bool_atom("exclude_from_sim")?;
                    self.need_right()?;
                }
                "in_bom" => {
                    symbol.in_bom = self.parse_bool_atom("in_bom")?;
                    self.need_right()?;
                }
                "on_board" => {
                    symbol.on_board = self.parse_bool_atom("on_board")?;
                    self.need_right()?;
                }
                "in_pos_files" => {
                    symbol.in_pos_files = self.parse_bool_atom("in_pos_files")?;
                    self.need_right()?;
                }
                "dnp" => {
                    symbol.dnp = self.parse_bool_atom("dnp")?;
                    self.need_right()?;
                }
                "fields_autoplaced" => {
                    if self.parse_maybe_absent_bool(true)? {
                        symbol.fields_autoplaced = FieldAutoplacement::Auto;
                    }
                    self.need_right()?;
                }
                "uuid" => {
                    symbol.uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                "property" => {
                    let property = self.parse_sch_field(FieldParent::Symbol)?;
                    if property.key == SIM_LEGACY_ENABLE_FIELD_V7 {
                        symbol.excluded_from_sim = property.value == "0";
                        self.need_right()?;
                        continue;
                    }
                    if property.key == SIM_LEGACY_ENABLE_FIELD {
                        symbol.excluded_from_sim = property.value == "N";
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
                        if let Some(existing) = symbol
                            .properties
                            .iter_mut()
                            .find(|existing| existing.kind == property.kind)
                        {
                            *existing = property;
                        } else {
                            symbol.properties.push(property);
                        }
                    } else {
                        if let Some(existing) = symbol
                            .properties
                            .iter_mut()
                            .find(|existing| existing.key == property.key)
                        {
                            *existing = property;
                        } else {
                            symbol.properties.push(property);
                        }
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
                            let mut instance = SymbolLocalInstance {
                                project: project.clone(),
                                path,
                                reference: None,
                                unit: None,
                                variants: Vec::new(),
                            };
                            while !self.at_right() {
                                self.need_left()?;
                                match self
                                    .need_unquoted_symbol_atom(
                                        "reference, unit, value, footprint, or variant",
                                    )?
                                    .as_str()
                                {
                                    "reference" => {
                                        instance.reference =
                                            Some(self.need_symbol_atom("reference")?);
                                        self.need_right()?;
                                    }
                                    "unit" => {
                                        instance.unit = Some(self.parse_i32_atom("symbol unit")?);
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
                                        if let Some(existing) =
                                            symbol.properties.iter_mut().find(|property| {
                                                property.kind == PropertyKind::SymbolValue
                                            })
                                        {
                                            existing.id = PropertyKind::SymbolValue
                                                .default_field_id()
                                                .or(existing.id);
                                            existing.key = PropertyKind::SymbolValue
                                                .canonical_key()
                                                .to_string();
                                            existing.value = parsed.clone();
                                        } else {
                                            symbol.properties.push(Property::new(
                                                PropertyKind::SymbolValue,
                                                parsed.clone(),
                                            ));
                                        }
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
                                        if let Some(existing) =
                                            symbol.properties.iter_mut().find(|property| {
                                                property.kind == PropertyKind::SymbolFootprint
                                            })
                                        {
                                            existing.id = PropertyKind::SymbolFootprint
                                                .default_field_id()
                                                .or(existing.id);
                                            existing.key = PropertyKind::SymbolFootprint
                                                .canonical_key()
                                                .to_string();
                                            existing.value = parsed.clone();
                                        } else {
                                            symbol.properties.push(Property::new(
                                                PropertyKind::SymbolFootprint,
                                                parsed.clone(),
                                            ));
                                        }
                                        self.need_right()?;
                                    }
                                    "variant" => {
                                        let mut variant = ItemVariant {
                                            name: String::new(),
                                            dnp: symbol.dnp,
                                            excluded_from_sim: symbol.excluded_from_sim,
                                            in_bom: symbol.in_bom,
                                            on_board: symbol.on_board,
                                            in_pos_files: symbol.in_pos_files,
                                            fields: Vec::new(),
                                        };

                                        while !self.at_right() {
                                            self.need_left()?;
                                            match self
                                                .need_unquoted_symbol_atom(
                                                    "dnp, exclude_from_sim, field, in_bom, in_pos_files, name, or on_board",
                                                )?
                                                .as_str()
                                            {
                                                "name" => {
                                                    variant.name = self
                                                        .need_symbol_atom("name")
                                                        .map_err(|_| {
                                                            self.error_here("Invalid variant name")
                                                        })?;
                                                    self.need_right()?;
                                                }
                                                "dnp" => {
                                                    variant.dnp = self.parse_bool_atom("dnp")?;
                                                    self.need_right()?;
                                                }
                                                "exclude_from_sim" => {
                                                    variant.excluded_from_sim =
                                                        self.parse_bool_atom("exclude_from_sim")?;
                                                    self.need_right()?;
                                                }
                                                "in_bom" => {
                                                    variant.in_bom = self.parse_bool_atom("in_bom")?;
                                                    if self.require_known_version()?
                                                        < VERSION_VARIANT_IN_BOM_FIX
                                                    {
                                                        variant.in_bom = !variant.in_bom;
                                                    }
                                                    self.need_right()?;
                                                }
                                                "on_board" => {
                                                    variant.on_board =
                                                        self.parse_bool_atom("on_board")?;
                                                    self.need_right()?;
                                                }
                                                "in_pos_files" => {
                                                    variant.in_pos_files =
                                                        self.parse_bool_atom("in_pos_files")?;
                                                    self.need_right()?;
                                                }
                                                "field" => {
                                                    let mut field = VariantField {
                                                        name: String::new(),
                                                        value: String::new(),
                                                    };

                                                    while !self.at_right() {
                                                        self.need_left()?;
                                                        match self
                                                            .need_unquoted_symbol_atom(
                                                                "name or value",
                                                            )?
                                                            .as_str()
                                                        {
                                                            "name" => {
                                                                field.name = self
                                                                    .need_symbol_atom("name")
                                                                    .map_err(|_| {
                                                                        self.error_here(
                                                                            "Invalid variant field name",
                                                                        )
                                                                    })?;
                                                                self.need_right()?;
                                                            }
                                                            "value" => {
                                                                field.value = self
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

                                                    variant.fields.push(field);
                                                    self.need_right()?;
                                                }
                                                _ => {
                                                    return Err(self.expecting(
                                                        "dnp, exclude_from_sim, field, in_bom, in_pos_files, name, or on_board",
                                                    ));
                                                }
                                            }
                                        }

                                        instance.variants.push(variant);
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
                            symbol.instances.push(instance);
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
                                if let Some(existing) = symbol
                                    .properties
                                    .iter_mut()
                                    .find(|property| property.kind == PropertyKind::SymbolValue)
                                {
                                    existing.id = PropertyKind::SymbolValue
                                        .default_field_id()
                                        .or(existing.id);
                                    existing.key =
                                        PropertyKind::SymbolValue.canonical_key().to_string();
                                    existing.value = parsed;
                                } else {
                                    symbol
                                        .properties
                                        .push(Property::new(PropertyKind::SymbolValue, parsed));
                                }
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
                                if let Some(existing) = symbol
                                    .properties
                                    .iter_mut()
                                    .find(|property| property.kind == PropertyKind::SymbolFootprint)
                                {
                                    existing.id = PropertyKind::SymbolFootprint
                                        .default_field_id()
                                        .or(existing.id);
                                    existing.key =
                                        PropertyKind::SymbolFootprint.canonical_key().to_string();
                                    existing.value = parsed;
                                } else {
                                    symbol
                                        .properties
                                        .push(Property::new(PropertyKind::SymbolFootprint, parsed));
                                }
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
                    symbol.pins.push(SymbolPin {
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

        symbol.lib_name = symbol.lib_name.take().filter(|name| name != &symbol.lib_id);
        Ok(symbol)
    }

    fn parse_sch_sheet(&mut self) -> Result<Sheet, Error> {
        let mut sheet = Sheet {
            at: [0.0, 0.0],
            size: [0.0, 0.0],
            has_stroke: false,
            has_fill: false,
            stroke: None,
            fill: None,
            excluded_from_sim: false,
            in_bom: true,
            on_board: true,
            dnp: false,
            fields_autoplaced: FieldAutoplacement::None,
            uuid: None,
            properties: Vec::new(),
            pins: Vec::new(),
            instances: Vec::new(),
        };
        let mut properties = Vec::new();
        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom(
                "at, size, stroke, background, instances, uuid, property, or pin",
            )?;
            match head.as_str() {
                "at" => {
                    sheet.at = self.parse_xy2("sheet at")?;
                    self.need_right()?;
                }
                "size" => {
                    sheet.size = self.parse_xy2("sheet size")?;
                    self.need_right()?;
                }
                "exclude_from_sim" => {
                    sheet.excluded_from_sim = self.parse_bool_atom("exclude_from_sim")?;
                    self.need_right()?;
                }
                "in_bom" => {
                    sheet.in_bom = self.parse_bool_atom("in_bom")?;
                    self.need_right()?;
                }
                "on_board" => {
                    sheet.on_board = self.parse_bool_atom("on_board")?;
                    self.need_right()?;
                }
                "dnp" => {
                    sheet.dnp = self.parse_bool_atom("dnp")?;
                    self.need_right()?;
                }
                "fields_autoplaced" => {
                    if self.parse_maybe_absent_bool(true)? {
                        sheet.fields_autoplaced = FieldAutoplacement::Auto;
                    }
                    self.need_right()?;
                }
                "stroke" => {
                    sheet.has_stroke = true;
                    sheet.stroke = Some(self.parse_stroke()?);
                }
                "fill" => {
                    sheet.has_fill = true;
                    sheet.fill = Some(self.parse_fill()?);
                }
                "uuid" => {
                    sheet.uuid = Some(self.need_symbol_atom("uuid")?);
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
                            property.id = PropertyKind::SheetName.default_field_id();
                        } else if properties.len() == 1 {
                            property.key = "Sheetfile".to_string();
                            property.kind = PropertyKind::SheetFile;
                            property.id = PropertyKind::SheetFile.default_field_id();
                        }
                    }
                    properties.push(property);
                    self.need_right()?;
                }
                "pin" => {
                    sheet.pins.push(self.parse_sch_sheet_pin()?);
                    self.need_right()?;
                }
                "instances" => {
                    let mut instances = Vec::new();

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
                            let mut instance = SheetLocalInstance {
                                project: project.clone(),
                                path,
                                page: None,
                                variants: Vec::new(),
                            };
                            while !self.at_right() {
                                self.need_left()?;
                                match self.need_unquoted_symbol_atom("page or variant")?.as_str() {
                                    "page" => {
                                        let mut parsed_page = self.need_symbol_atom("page")?;

                                        if parsed_page.is_empty() {
                                            parsed_page = "#".to_string();
                                        } else {
                                            parsed_page.retain(|ch| {
                                                !matches!(ch, '\r' | '\n' | '\t' | ' ')
                                            });

                                            if parsed_page.is_empty() {
                                                parsed_page = "#".to_string();
                                            }
                                        }

                                        instance.page = Some(parsed_page);
                                        self.need_right()?;
                                    }
                                    "variant" => {
                                        let mut variant = ItemVariant {
                                            name: String::new(),
                                            dnp: sheet.dnp,
                                            excluded_from_sim: sheet.excluded_from_sim,
                                            in_bom: sheet.in_bom,
                                            on_board: sheet.on_board,
                                            in_pos_files: false,
                                            fields: Vec::new(),
                                        };

                                        while !self.at_right() {
                                            self.need_left()?;
                                            match self
                                                .need_unquoted_symbol_atom(
                                                    "dnp, exclude_from_sim, field, in_bom, in_pos_files, name, or on_board",
                                                )?
                                                .as_str()
                                            {
                                                "name" => {
                                                    variant.name = self
                                                        .need_symbol_atom("name")
                                                        .map_err(|_| {
                                                            self.error_here("Invalid variant name")
                                                        })?;
                                                    self.need_right()?;
                                                }
                                                "dnp" => {
                                                    variant.dnp = self.parse_bool_atom("dnp")?;
                                                    self.need_right()?;
                                                }
                                                "exclude_from_sim" => {
                                                    variant.excluded_from_sim =
                                                        self.parse_bool_atom("exclude_from_sim")?;
                                                    self.need_right()?;
                                                }
                                                "in_bom" => {
                                                    variant.in_bom = self.parse_bool_atom("in_bom")?;
                                                    if self.require_known_version()?
                                                        < VERSION_VARIANT_IN_BOM_FIX
                                                    {
                                                        variant.in_bom = !variant.in_bom;
                                                    }
                                                    self.need_right()?;
                                                }
                                                "on_board" => {
                                                    variant.on_board =
                                                        self.parse_bool_atom("on_board")?;
                                                    self.need_right()?;
                                                }
                                                "in_pos_files" => {
                                                    variant.in_pos_files =
                                                        self.parse_bool_atom("in_pos_files")?;
                                                    self.need_right()?;
                                                }
                                                "field" => {
                                                    let mut field = VariantField {
                                                        name: String::new(),
                                                        value: String::new(),
                                                    };

                                                    while !self.at_right() {
                                                        self.need_left()?;
                                                        match self
                                                            .need_unquoted_symbol_atom(
                                                                "name or value",
                                                            )?
                                                            .as_str()
                                                        {
                                                            "name" => {
                                                                field.name = self
                                                                    .need_symbol_atom("name")
                                                                    .map_err(|_| {
                                                                        self.error_here(
                                                                            "Invalid variant field name",
                                                                        )
                                                                    })?;
                                                                self.need_right()?;
                                                            }
                                                            "value" => {
                                                                field.value = self
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

                                                    variant.fields.push(field);
                                                    self.need_right()?;
                                                }
                                                _ => {
                                                    return Err(self.expecting(
                                                        "dnp, exclude_from_sim, field, in_bom, in_pos_files, name, or on_board",
                                                    ));
                                                }
                                            }
                                        }

                                        instance.variants.push(variant);
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
                    sheet.instances = instances;
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

        Ok(sheet)
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
            _ => {
                return Err(self.expecting("input, output, bidirectional, tri_state, or passive"));
            }
        };

        let mut sheet_pin = SheetPin {
            name,
            shape,
            at: None,
            side: None,
            visible: true,
            has_effects: false,
            effects: None,
            uuid: None,
        };

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
                    sheet_pin.at = Some([parsed[0], parsed[1]]);
                    sheet_pin.side = Some(parsed_side);
                    self.need_right()?;
                }
                "uuid" => {
                    sheet_pin.uuid = Some(self.need_symbol_atom("uuid")?);
                    self.need_right()?;
                }
                "effects" => {
                    let mut parsed_effects = TextEffects::default();
                    self.parse_eda_text(
                        None,
                        &mut parsed_effects,
                        &mut sheet_pin.visible,
                        true,
                        true,
                    )?;
                    sheet_pin.has_effects = true;
                    sheet_pin.effects = Some(parsed_effects);
                    self.need_right()?;
                }
                _ => return Err(self.expecting("at, uuid or effects")),
            }
        }

        Ok(sheet_pin)
    }

    fn parse_sch_sheet_instances(&mut self) -> Result<(), Error> {
        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom("path")?;
            if head != "path" {
                return Err(self.expecting("path"));
            }
            let raw_path = self.need_symbol_atom("sheet instance path")?;
            let mut instance = SheetInstance {
                path: raw_path,
                page: None,
            };

            if self.require_known_version()? < VERSION_SHEET_INSTANCE_ROOT_PATH {
                if let Some(root_uuid) = self.root_uuid.as_ref() {
                    if !instance.path.is_empty() {
                        let prefix = format!("/{root_uuid}");

                        instance.path = if instance.path == prefix
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
            }

            while !self.at_right() {
                self.need_left()?;
                let child = self.need_unquoted_symbol_atom("path or page")?;
                match child.as_str() {
                    "page" => {
                        let mut parsed_page = self.need_symbol_atom("page")?;

                        if parsed_page.is_empty() {
                            parsed_page = "#".to_string();
                        } else {
                            parsed_page.retain(|ch| !matches!(ch, '\r' | '\n' | '\t' | ' '));

                            if parsed_page.is_empty() {
                                parsed_page = "#".to_string();
                            }
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
                self.screen.sheet_instances.push(instance);
            }
        }
        Ok(())
    }

    fn parse_sch_symbol_instances(&mut self) -> Result<(), Error> {
        while !self.at_right() {
            self.need_left()?;
            let head = self.need_unquoted_symbol_atom("path")?;
            if head != "path" {
                return Err(self.expecting("path"));
            }
            let raw_path = self.need_symbol_atom("symbol instance path")?;
            let path = if let Some(root_uuid) = self.root_uuid.as_ref() {
                if raw_path.is_empty() {
                    String::new()
                } else {
                    let prefix = format!("/{root_uuid}");

                    if raw_path == prefix || raw_path.starts_with(&(prefix.clone() + "/")) {
                        raw_path
                    } else if raw_path.starts_with('/') {
                        format!("{prefix}{raw_path}")
                    } else {
                        format!("{prefix}/{raw_path}")
                    }
                }
            } else {
                raw_path
            };
            let mut instance = SymbolInstance {
                path,
                reference: None,
                unit: None,
                value: None,
                footprint: None,
            };
            while !self.at_right() {
                self.need_left()?;
                let child = self.need_unquoted_symbol_atom("path, unit, value or footprint")?;
                match child.as_str() {
                    "reference" => instance.reference = Some(self.need_symbol_atom("reference")?),
                    "unit" => instance.unit = Some(self.parse_i32_atom("unit")?),
                    "value" => {
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
            self.screen.symbol_instances.push(instance);
        }
        Ok(())
    }

    fn parse_group(&mut self) -> Result<(), Error> {
        let mut group = Group {
            name: None,
            uuid: None,
            lib_id: None,
            members: Vec::new(),
        };

        while matches!(self.current().kind, TokKind::Atom(_)) {
            if self.at_unquoted_symbol_with("locked") {
                let _ = self.need_unquoted_symbol_atom("locked")?;
                continue;
            }
            group.name = Some(self.need_quoted_atom("group name or locked")?);
            break;
        }

        while !self.at_right() {
            self.need_left()?;
            match self
                .need_unquoted_symbol_atom("uuid, lib_id, members")?
                .as_str()
            {
                "uuid" => {
                    group.uuid = Some(self.need_symbol_atom("uuid")?);
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
                    while !self.at_right() {
                        group
                            .members
                            .push(self.need_symbol_atom("group member uuid")?);
                    }
                    self.need_right()?;
                }
                _ => return Err(self.expecting("uuid, lib_id, members")),
            }
        }

        self.pending_groups.push(group);
        Ok(())
    }

    fn parse_sch_field(&mut self, parent: FieldParent) -> Result<Property, Error> {
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

        let field_id = match parent {
            FieldParent::Symbol => match name.to_ascii_lowercase().as_str() {
                "reference" => PropertyKind::SymbolReference,
                "value" => PropertyKind::SymbolValue,
                "footprint" => PropertyKind::SymbolFootprint,
                "datasheet" => PropertyKind::SymbolDatasheet,
                _ => PropertyKind::User,
            },
            FieldParent::Sheet => match name.to_ascii_lowercase().as_str() {
                "sheetname" | "sheet name" => PropertyKind::SheetName,
                "sheetfile" | "sheet file" => PropertyKind::SheetFile,
                _ => PropertyKind::SheetUser,
            },
            FieldParent::GlobalLabel => match name.to_ascii_lowercase().as_str() {
                "intersheet references" => PropertyKind::GlobalLabelIntersheetRefs,
                _ => PropertyKind::User,
            },
            FieldParent::OtherLabel => PropertyKind::User,
        };

        let mut property = Property {
            id: field_id.default_field_id(),
            key: match field_id {
                PropertyKind::SymbolReference => "Reference".to_string(),
                PropertyKind::SymbolValue => "Value".to_string(),
                PropertyKind::SymbolFootprint => "Footprint".to_string(),
                PropertyKind::SymbolDatasheet => "Datasheet".to_string(),
                PropertyKind::SheetName => "Sheetname".to_string(),
                PropertyKind::SheetFile => "Sheetfile".to_string(),
                PropertyKind::GlobalLabelIntersheetRefs => "Intersheet References".to_string(),
                _ => name.clone(),
            },
            value: String::new(),
            kind: field_id,
            is_private: matches!(field_id, PropertyKind::User) && is_private,
            at: None,
            angle: None,
            visible: true,
            show_name: true,
            can_autoplace: true,
            has_effects: false,
            effects: None,
        };

        property.value = self
            .need_symbol_atom("property value")
            .map_err(|_| self.error_here("Invalid property value"))?;

        if self.version.unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION) < VERSION_EMPTY_TILDE_IS_EMPTY
            && property.value == "~"
        {
            property.value.clear();
        }

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
                    let mut parsed_effects = TextEffects::default();
                    self.parse_eda_text(
                        Some(&mut property.value),
                        &mut parsed_effects,
                        &mut property.visible,
                        property.kind == PropertyKind::SymbolValue,
                        true,
                    )?;
                    property.has_effects = true;
                    property.effects = Some(parsed_effects);
                    self.need_right()?;
                }
                _ => {
                    return Err(
                        self.expecting("id, at, hide, show_name, do_not_autoplace or effects")
                    );
                }
            }
        }
        Ok(property)
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

    fn parse_eda_text(
        &mut self,
        text: Option<&mut String>,
        effects: &mut TextEffects,
        visible: &mut bool,
        convert_overbar_syntax: bool,
        _enforce_min_text_size: bool,
    ) -> Result<(), Error> {
        if convert_overbar_syntax
            && self.version.unwrap_or(SEXPR_SCHEMATIC_FILE_VERSION) < VERSION_TEXT_OVERBAR_NOTATION
        {
            if let Some(text) = text {
                *text = self.convert_old_overbar_notation(text.clone());
            }
        }

        effects.h_justify = TextHJustify::Center;
        effects.v_justify = TextVJustify::Center;

        while !self.at_right() {
            if matches!(self.current().kind, TokKind::Atom(_)) {
                match self
                    .need_unquoted_symbol_atom("font, justify, hide or href")?
                    .as_str()
                {
                    "hide" => {
                        effects.hidden = self.parse_maybe_absent_bool(true)?;
                        *visible = !effects.hidden;
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
                        if matches!(self.current().kind, TokKind::Atom(_)) {
                            match self
                                .need_unquoted_symbol_atom(
                                    "face, size, thickness, line_spacing, bold, or italic",
                                )?
                                .as_str()
                            {
                                "bold" => {
                                    effects.bold = match &self.current().kind {
                                        TokKind::Right | TokKind::Left | TokKind::Eof => true,
                                        TokKind::Atom(value)
                                            if matches!(value.as_str(), "yes" | "no") =>
                                        {
                                            self.parse_bool_atom("boolean")?
                                        }
                                        TokKind::Atom(_) => true,
                                    }
                                }
                                "italic" => {
                                    effects.italic = match &self.current().kind {
                                        TokKind::Right | TokKind::Left | TokKind::Eof => true,
                                        TokKind::Atom(value)
                                            if matches!(value.as_str(), "yes" | "no") =>
                                        {
                                            self.parse_bool_atom("boolean")?
                                        }
                                        TokKind::Atom(_) => true,
                                    }
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
                                effects.font_face = Some(
                                    self.need_symbol_atom("font face")
                                        .map_err(|_| self.error_here("missing font face"))?,
                                );
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
                    let href = self
                        .need_symbol_atom("hyperlink url")
                        .map_err(|_| self.error_here("missing hyperlink url"))?;
                    if !Self::validate_hyperlink(&href) {
                        return Err(self.error_here(format!("invalid hyperlink url `{href}`")));
                    }
                    effects.hyperlink = Some(href);
                    self.need_right()?;
                }
                "hide" => {
                    effects.hidden = self.parse_maybe_absent_bool(true)?;
                    *visible = !effects.hidden;
                    self.need_right()?;
                }
                _ => return Err(self.expecting("font, justify, hide or href")),
            }
        }

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

    fn get_label_spin_style(angle: f64) -> Option<LabelSpin> {
        match angle.rem_euclid(360.0) as i32 {
            0 => Some(LabelSpin::Right),
            90 => Some(LabelSpin::Up),
            180 => Some(LabelSpin::Left),
            270 => Some(LabelSpin::Bottom),
            _ => None,
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
            .map(|(idx, symbol)| (symbol.name.clone(), idx))
            .collect();

        let mut cache = std::collections::HashMap::new();

        for idx in 0..self.screen.lib_symbols.len() {
            let has_demorgan = Self::has_legacy_alternate_body_style(
                idx,
                &self.screen.lib_symbols,
                &symbol_index,
                &mut cache,
            );
            self.screen.lib_symbols[idx].has_demorgan = has_demorgan;
        }
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

        if symbol.units.iter().any(|unit| unit.body_style > 1) {
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
