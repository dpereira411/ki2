use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Schematic {
    pub path: PathBuf,
    pub version: i32,
    pub generator: String,
    pub generator_version: Option<String>,
    pub root_sheet: RootSheet,
    pub screen: Screen,
}

impl Schematic {
    pub fn sheet_paths(&self) -> impl Iterator<Item = PathBuf> + '_ {
        let base_dir = self.path.parent().unwrap_or_else(|| Path::new("."));
        self.screen.items.iter().filter_map(move |item| match item {
            SchItem::Sheet(sheet) => sheet.filename().map(|name| base_dir.join(name)),
            _ => None,
        })
    }

    pub fn sheet_references(&self) -> Vec<SheetReference> {
        let base_dir = self.path.parent().unwrap_or_else(|| Path::new("."));
        self.screen
            .items
            .iter()
            .filter_map(|item| match item {
                SchItem::Sheet(sheet) => {
                    let filename = sheet.filename()?.to_string();
                    Some(SheetReference {
                        sheet_uuid: sheet.uuid.clone(),
                        sheet_name: sheet.name().map(str::to_string),
                        filename: filename.clone(),
                        resolved_path: base_dir.join(filename),
                    })
                }
                _ => None,
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RootSheet {
    pub uuid: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Screen {
    pub uuid: Option<String>,
    pub paper: Option<Paper>,
    pub page: Option<Page>,
    pub root_sheet_page: Option<String>,
    pub title_block: Option<TitleBlock>,
    pub embedded_fonts: Option<bool>,
    pub embedded_files: Vec<EmbeddedFile>,
    pub parse_warnings: Vec<String>,
    pub bus_aliases: Vec<BusAlias>,
    pub lib_symbols: Vec<LibSymbol>,
    pub items: Vec<SchItem>,
    pub sheet_instances: Vec<SheetInstance>,
    pub symbol_instances: Vec<SymbolInstance>,
}

impl Screen {
    pub fn add_bus_alias(&mut self, alias: BusAlias) {
        self.bus_aliases.push(alias);
    }

    pub fn add_sheet_instance(&mut self, instance: SheetInstance) {
        self.sheet_instances.push(instance);
    }

    pub fn add_symbol_instance(&mut self, instance: SymbolInstance) {
        self.symbol_instances.push(instance);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Paper {
    pub kind: String,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub portrait: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub page: String,
    pub sheet: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TitleBlock {
    pub title: Option<String>,
    pub date: Option<String>,
    pub revision: Option<String>,
    pub company: Option<String>,
    pub comments: Vec<(i32, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LibSymbol {
    pub name: String,
    pub extends: Option<String>,
    pub power: bool,
    pub local_power: bool,
    pub body_style_names: Vec<String>,
    pub has_demorgan: bool,
    pub pin_name_offset: Option<f64>,
    pub show_pin_names: bool,
    pub show_pin_numbers: bool,
    pub excluded_from_sim: bool,
    pub in_bom: bool,
    pub on_board: bool,
    pub in_pos_files: bool,
    pub duplicate_pin_numbers_are_jumpers: bool,
    pub jumper_pin_groups: Vec<Vec<String>>,
    pub keywords: Option<String>,
    pub description: Option<String>,
    pub fp_filters: Vec<String>,
    pub locked_units: bool,
    pub properties: Vec<Property>,
    pub units: Vec<LibSymbolUnit>,
    pub embedded_fonts: Option<bool>,
    pub embedded_files: Vec<EmbeddedFile>,
}

impl LibSymbol {
    pub fn new(name: String) -> Self {
        Self {
            units: vec![LibSymbolUnit {
                name: format!("{name}_1_1"),
                unit_number: 1,
                body_style: 1,
                unit_name: None,
                draw_item_kinds: Vec::new(),
                draw_items: Vec::new(),
            }],
            name,
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
            embedded_fonts: None,
            embedded_files: Vec::new(),
        }
    }

    pub fn ensure_unit_index(&mut self, name: String, unit_number: i32, body_style: i32) -> usize {
        if let Some(index) = self.units.iter().position(|existing| {
            existing.unit_number == unit_number
                && existing.body_style == body_style
                && existing.name == name
        }) {
            index
        } else {
            self.units.push(LibSymbolUnit {
                name,
                unit_number,
                body_style,
                unit_name: None,
                draw_item_kinds: Vec::new(),
                draw_items: Vec::new(),
            });
            self.units.len() - 1
        }
    }

    pub fn push_root_draw_item(&mut self, item: LibDrawItem) {
        self.units[0].push_draw_item(item);
    }

    pub fn insert_property(&mut self, raw_name: &str, mut property: Property) {
        if matches!(
            property.kind,
            PropertyKind::SymbolReference
                | PropertyKind::SymbolValue
                | PropertyKind::SymbolFootprint
                | PropertyKind::SymbolDatasheet
        ) {
            if let Some(existing) = self
                .properties
                .iter_mut()
                .find(|existing| existing.kind == property.kind)
            {
                *existing = property;
            } else {
                self.properties.push(property);
            }
        } else if raw_name == "ki_keywords" {
            self.keywords = Some(property.value);
        } else if raw_name == "ki_description" {
            self.description = Some(property.value);
        } else if raw_name == "ki_fp_filters" {
            self.fp_filters = property
                .value
                .split_whitespace()
                .map(str::to_string)
                .collect();
        } else if raw_name == "ki_locked" {
            self.locked_units = true;
        } else {
            let mut existing = self
                .properties
                .iter()
                .any(|existing| existing.key == property.key);

            if existing {
                let base = property.key.clone();

                for suffix in 1..10 {
                    let candidate = format!("{base}_{suffix}");

                    if !self
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
                self.properties.push(property);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LibSymbolUnit {
    pub name: String,
    pub unit_number: i32,
    pub body_style: i32,
    pub unit_name: Option<String>,
    pub draw_item_kinds: Vec<String>,
    pub draw_items: Vec<LibDrawItem>,
}

impl LibSymbolUnit {
    pub fn push_draw_item(&mut self, item: LibDrawItem) {
        self.draw_item_kinds.push(item.kind.clone());
        self.draw_items.push(item);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LibDrawItem {
    pub kind: String,
    pub is_private: bool,
    pub unit_number: i32,
    pub body_style: i32,
    pub visible: bool,
    pub at: Option<[f64; 2]>,
    pub angle: Option<f64>,
    pub points: Vec<[f64; 2]>,
    pub end: Option<[f64; 2]>,
    pub radius: Option<f64>,
    pub arc_center: Option<[f64; 2]>,
    pub arc_start_angle: Option<f64>,
    pub arc_end_angle: Option<f64>,
    pub length: Option<f64>,
    pub text: Option<String>,
    pub name: Option<String>,
    pub number: Option<String>,
    pub name_effects: Option<TextEffects>,
    pub number_effects: Option<TextEffects>,
    pub electrical_type: Option<String>,
    pub graphic_shape: Option<String>,
    pub alternates: Vec<LibPinAlternate>,
    pub stroke: Option<Stroke>,
    pub fill: Option<Fill>,
    pub effects: Option<TextEffects>,
    pub margins: Option<[f64; 4]>,
}

impl LibDrawItem {
    pub fn new(kind: &str, unit_number: i32, body_style: i32) -> Self {
        Self {
            kind: kind.to_string(),
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
            electrical_type: None,
            graphic_shape: None,
            alternates: Vec::new(),
            stroke: None,
            fill: None,
            effects: None,
            margins: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LibPinAlternate {
    pub name: String,
    pub electrical_type: String,
    pub graphic_shape: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SchItem {
    Junction(Junction),
    NoConnect(NoConnect),
    BusEntry(BusEntry),
    Wire(Line),
    Bus(Line),
    Polyline(Line),
    Label(Label),
    Text(Text),
    TextBox(TextBox),
    Table(Table),
    Image(Image),
    Shape(Shape),
    Symbol(Symbol),
    Sheet(Sheet),
    Group(Group),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Junction {
    pub at: [f64; 2],
    pub diameter: Option<f64>,
    pub color: Option<[f64; 4]>,
    pub uuid: Option<String>,
}

impl Junction {
    pub fn new() -> Self {
        Self {
            at: [0.0, 0.0],
            diameter: None,
            color: None,
            uuid: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NoConnect {
    pub at: [f64; 2],
    pub uuid: Option<String>,
}

impl NoConnect {
    pub fn new() -> Self {
        Self {
            at: [0.0, 0.0],
            uuid: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BusEntry {
    pub at: [f64; 2],
    pub size: [f64; 2],
    pub has_stroke: bool,
    pub stroke: Option<Stroke>,
    pub uuid: Option<String>,
}

impl BusEntry {
    pub fn new() -> Self {
        Self {
            at: [0.0, 0.0],
            size: [0.0, 0.0],
            has_stroke: false,
            stroke: None,
            uuid: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub kind: LineKind,
    pub points: Vec<[f64; 2]>,
    pub has_stroke: bool,
    pub stroke: Option<Stroke>,
    pub uuid: Option<String>,
}

impl Line {
    pub fn new(kind: LineKind) -> Self {
        Self {
            kind,
            points: Vec::new(),
            has_stroke: false,
            stroke: None,
            uuid: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Wire,
    Bus,
    Polyline,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub kind: LabelKind,
    pub text: String,
    pub at: [f64; 2],
    pub angle: f64,
    pub spin: Option<LabelSpin>,
    pub shape: Option<LabelShape>,
    pub pin_length: Option<f64>,
    pub iref_at: Option<[f64; 2]>,
    pub excluded_from_sim: bool,
    pub fields_autoplaced: FieldAutoplacement,
    pub visible: bool,
    pub has_effects: bool,
    pub effects: Option<TextEffects>,
    pub uuid: Option<String>,
    pub properties: Vec<Property>,
}

impl Label {
    pub fn new(kind: LabelKind, text: String) -> Self {
        Self {
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
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelKind {
    Local,
    Global,
    Hierarchical,
    Directive,
    NetclassFlag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelShape {
    Input,
    Output,
    Bidirectional,
    TriState,
    Passive,
    Dot,
    Round,
    Diamond,
    Rectangle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelSpin {
    Right,
    Up,
    Left,
    Bottom,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    pub kind: TextKind,
    pub text: String,
    pub at: Option<[f64; 3]>,
    pub excluded_from_sim: bool,
    pub fields_autoplaced: FieldAutoplacement,
    pub visible: bool,
    pub has_effects: bool,
    pub effects: Option<TextEffects>,
    pub uuid: Option<String>,
}

impl Text {
    pub fn new(kind: TextKind, text: String) -> Self {
        Self {
            kind,
            text,
            at: None,
            excluded_from_sim: false,
            fields_autoplaced: FieldAutoplacement::None,
            visible: true,
            has_effects: false,
            effects: None,
            uuid: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextKind {
    Text,
    TextBox,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextBox {
    pub text: String,
    pub at: [f64; 2],
    pub angle: f64,
    pub end: [f64; 2],
    pub excluded_from_sim: bool,
    pub has_effects: bool,
    pub effects: Option<TextEffects>,
    pub stroke: Option<Stroke>,
    pub fill: Option<Fill>,
    pub span: Option<[i32; 2]>,
    pub margins: Option<[f64; 4]>,
    pub uuid: Option<String>,
}

impl TextBox {
    pub fn new() -> Self {
        Self {
            text: String::new(),
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
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableCell {
    pub text: String,
    pub at: [f64; 2],
    pub angle: f64,
    pub end: [f64; 2],
    pub excluded_from_sim: bool,
    pub has_effects: bool,
    pub effects: Option<TextEffects>,
    pub stroke: Option<Stroke>,
    pub fill: Option<Fill>,
    pub span: Option<[i32; 2]>,
    pub margins: Option<[f64; 4]>,
    pub uuid: Option<String>,
}

impl TableCell {
    pub fn new() -> Self {
        Self {
            text: String::new(),
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
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrokeStyle {
    Default,
    Dash,
    Dot,
    DashDot,
    DashDotDot,
    Solid,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub width: Option<f64>,
    pub style: StrokeStyle,
    pub color: Option<[f64; 4]>,
}

impl Stroke {
    pub fn new() -> Self {
        Self {
            width: None,
            style: StrokeStyle::Default,
            color: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FillType {
    None,
    Outline,
    Background,
    Color,
    Hatch,
    ReverseHatch,
    CrossHatch,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fill {
    pub fill_type: FillType,
    pub color: Option<[f64; 4]>,
}

impl Fill {
    pub fn new() -> Self {
        Self {
            fill_type: FillType::None,
            color: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextEffects {
    pub font_face: Option<String>,
    pub font_size: Option<[f64; 2]>,
    pub thickness: Option<f64>,
    pub bold: bool,
    pub italic: bool,
    pub color: Option<[f64; 4]>,
    pub line_spacing: Option<f64>,
    pub h_justify: TextHJustify,
    pub v_justify: TextVJustify,
    pub hyperlink: Option<String>,
    pub hidden: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextHJustify {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextVJustify {
    Top,
    #[default]
    Center,
    Bottom,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub default_line_width: f64,
    pub column_count: Option<i32>,
    pub column_widths: Vec<f64>,
    pub row_heights: Vec<f64>,
    pub cells: Vec<TableCell>,
    pub border_external: Option<bool>,
    pub border_header: Option<bool>,
    pub border_stroke: Option<Stroke>,
    pub separators_rows: Option<bool>,
    pub separators_cols: Option<bool>,
    pub separators_stroke: Option<Stroke>,
    pub uuid: Option<String>,
}

impl Table {
    pub fn new(default_line_width: f64) -> Self {
        Self {
            default_line_width,
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
        }
    }

    pub fn set_column_count(&mut self, count: i32) {
        self.column_count = Some(count);
    }

    pub fn set_column_width(&mut self, col: usize, width: f64) {
        if self.column_widths.len() <= col {
            self.column_widths.resize(col + 1, 0.0);
        }

        self.column_widths[col] = width;
    }

    pub fn set_row_height(&mut self, row: usize, height: f64) {
        if self.row_heights.len() <= row {
            self.row_heights.resize(row + 1, 0.0);
        }

        self.row_heights[row] = height;
    }

    pub fn add_cell(&mut self, cell: TableCell) {
        self.cells.push(cell);
    }

    pub fn first_cell(&self) -> Option<&TableCell> {
        self.cells.first()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub at: [f64; 2],
    pub scale: f64,
    pub data: Option<String>,
    pub uuid: Option<String>,
}

impl Image {
    pub fn new() -> Self {
        Self {
            at: [0.0, 0.0],
            scale: 1.0,
            data: None,
            uuid: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub kind: ShapeKind,
    pub points: Vec<[f64; 2]>,
    pub radius: Option<f64>,
    pub corner_radius: Option<f64>,
    pub has_stroke: bool,
    pub has_fill: bool,
    pub stroke: Option<Stroke>,
    pub fill: Option<Fill>,
    pub excluded_from_sim: bool,
    pub in_bom: bool,
    pub on_board: bool,
    pub dnp: bool,
    pub uuid: Option<String>,
}

impl Shape {
    pub fn new(kind: ShapeKind) -> Self {
        Self {
            kind,
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
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeKind {
    Polyline,
    Arc,
    Circle,
    Rectangle,
    Bezier,
    RuleArea,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Symbol {
    pub lib_id: String,
    pub lib_name: Option<String>,
    pub linked_lib_symbol_name: Option<String>,
    pub at: [f64; 2],
    pub angle: f64,
    pub mirror: Option<MirrorAxis>,
    pub unit: Option<i32>,
    pub body_style: Option<i32>,
    pub excluded_from_sim: bool,
    pub in_bom: bool,
    pub on_board: bool,
    pub in_pos_files: bool,
    pub dnp: bool,
    pub fields_autoplaced: FieldAutoplacement,
    pub uuid: Option<String>,
    pub properties: Vec<Property>,
    pub instances: Vec<SymbolLocalInstance>,
    pub pins: Vec<SymbolPin>,
}

impl Symbol {
    pub fn new() -> Self {
        Self {
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
        }
    }

    pub fn insert_property(&mut self, property: Property) {
        if matches!(
            property.kind,
            PropertyKind::SymbolReference
                | PropertyKind::SymbolValue
                | PropertyKind::SymbolFootprint
                | PropertyKind::SymbolDatasheet
        ) {
            if let Some(existing) = self
                .properties
                .iter_mut()
                .find(|existing| existing.kind == property.kind)
            {
                *existing = property;
            } else {
                self.properties.push(property);
            }
        } else if let Some(existing) = self
            .properties
            .iter_mut()
            .find(|existing| existing.key == property.key)
        {
            *existing = property;
        } else {
            self.properties.push(property);
        }
    }

    pub fn set_field_text(&mut self, kind: PropertyKind, value: String) {
        if let Some(existing) = self
            .properties
            .iter_mut()
            .find(|property| property.kind == kind)
        {
            existing.id = kind.default_field_id().or(existing.id);
            existing.key = kind.canonical_key().to_string();
            existing.value = value;
        } else {
            self.properties.push(Property::new(kind, value));
        }
    }

    pub fn add_pin(&mut self, pin: SymbolPin) {
        self.pins.push(pin);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    pub at: [f64; 2],
    pub size: [f64; 2],
    pub has_stroke: bool,
    pub has_fill: bool,
    pub stroke: Option<Stroke>,
    pub fill: Option<Fill>,
    pub excluded_from_sim: bool,
    pub in_bom: bool,
    pub on_board: bool,
    pub dnp: bool,
    pub fields_autoplaced: FieldAutoplacement,
    pub uuid: Option<String>,
    pub properties: Vec<Property>,
    pub pins: Vec<SheetPin>,
    pub instances: Vec<SheetLocalInstance>,
}

impl Sheet {
    pub fn new() -> Self {
        Self {
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
        }
    }

    pub fn set_properties(&mut self, mut properties: Vec<Property>) {
        for property in &mut properties {
            if property.kind == PropertyKind::SheetFile {
                property.value = property.value.replace('\\', "/");
            }
        }

        self.properties = properties;
    }

    pub fn name(&self) -> Option<&str> {
        self.properties
            .iter()
            .find(|property| property.kind == PropertyKind::SheetName)
            .map(|property| property.value.as_str())
    }

    pub fn filename(&self) -> Option<&str> {
        self.properties
            .iter()
            .find(|property| property.kind == PropertyKind::SheetFile)
            .map(|property| property.value.as_str())
    }

    pub fn add_pin(&mut self, pin: SheetPin) {
        self.pins.push(pin);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorAxis {
    X,
    Y,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FieldAutoplacement {
    #[default]
    None,
    Auto,
    Manual,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SymbolPin {
    pub number: String,
    pub alternate: Option<String>,
    pub uuid: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub id: Option<i32>,
    pub key: String,
    pub value: String,
    pub kind: PropertyKind,
    pub is_private: bool,
    pub at: Option<[f64; 2]>,
    pub angle: Option<f64>,
    pub visible: bool,
    pub show_name: bool,
    pub can_autoplace: bool,
    pub has_effects: bool,
    pub effects: Option<TextEffects>,
}

impl Property {
    pub fn new(kind: PropertyKind, value: String) -> Self {
        Self {
            id: kind.default_field_id(),
            key: kind.canonical_key().to_string(),
            value,
            kind,
            is_private: false,
            at: None,
            angle: None,
            visible: true,
            show_name: true,
            can_autoplace: true,
            has_effects: false,
            effects: None,
        }
    }

    pub fn new_named(kind: PropertyKind, name: &str, value: String, is_private: bool) -> Self {
        Self {
            id: kind.default_field_id(),
            key: match kind {
                PropertyKind::User | PropertyKind::SheetUser => name.to_string(),
                _ => kind.canonical_key().to_string(),
            },
            value,
            kind,
            is_private,
            at: None,
            angle: None,
            visible: true,
            show_name: true,
            can_autoplace: true,
            has_effects: false,
            effects: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyKind {
    User,
    SymbolReference,
    SymbolValue,
    SymbolFootprint,
    SymbolDatasheet,
    SheetName,
    SheetFile,
    SheetUser,
    GlobalLabelIntersheetRefs,
}

impl PropertyKind {
    pub fn canonical_key(self) -> &'static str {
        match self {
            PropertyKind::User => "",
            PropertyKind::SymbolReference => "Reference",
            PropertyKind::SymbolValue => "Value",
            PropertyKind::SymbolFootprint => "Footprint",
            PropertyKind::SymbolDatasheet => "Datasheet",
            PropertyKind::SheetName => "Sheetname",
            PropertyKind::SheetFile => "Sheetfile",
            PropertyKind::SheetUser => "",
            PropertyKind::GlobalLabelIntersheetRefs => "Intersheet References",
        }
    }

    pub fn default_field_id(self) -> Option<i32> {
        match self {
            PropertyKind::User => Some(0),
            PropertyKind::SymbolReference => Some(1),
            PropertyKind::SymbolValue => Some(2),
            PropertyKind::SymbolFootprint => Some(3),
            PropertyKind::SymbolDatasheet => Some(4),
            PropertyKind::GlobalLabelIntersheetRefs => Some(6),
            PropertyKind::SheetName => Some(7),
            PropertyKind::SheetFile => Some(8),
            PropertyKind::SheetUser => Some(9),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SheetPin {
    pub name: String,
    pub shape: SheetPinShape,
    pub at: Option<[f64; 2]>,
    pub side: Option<SheetSide>,
    pub visible: bool,
    pub has_effects: bool,
    pub effects: Option<TextEffects>,
    pub uuid: Option<String>,
}

impl SheetPin {
    pub fn new(name: String, shape: SheetPinShape) -> Self {
        Self {
            name,
            shape,
            at: None,
            side: None,
            visible: true,
            has_effects: false,
            effects: None,
            uuid: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetPinShape {
    Input,
    Output,
    Bidirectional,
    TriState,
    Passive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetSide {
    Right,
    Top,
    Left,
    Bottom,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SheetInstance {
    pub path: String,
    pub page: Option<String>,
}

impl SheetInstance {
    pub fn new(path: String) -> Self {
        Self { path, page: None }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SymbolInstance {
    pub path: String,
    pub reference: Option<String>,
    pub unit: Option<i32>,
    pub value: Option<String>,
    pub footprint: Option<String>,
}

impl SymbolInstance {
    pub fn new(path: String) -> Self {
        Self {
            path,
            reference: None,
            unit: None,
            value: None,
            footprint: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ItemVariant {
    pub name: String,
    pub dnp: bool,
    pub excluded_from_sim: bool,
    pub in_bom: bool,
    pub on_board: bool,
    pub in_pos_files: bool,
    pub fields: BTreeMap<String, String>,
}

impl ItemVariant {
    pub fn new(
        dnp: bool,
        excluded_from_sim: bool,
        in_bom: bool,
        on_board: bool,
        in_pos_files: bool,
    ) -> Self {
        Self {
            name: String::new(),
            dnp,
            excluded_from_sim,
            in_bom,
            on_board,
            in_pos_files,
            fields: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SheetLocalInstance {
    pub project: String,
    pub path: String,
    pub page: Option<String>,
    pub variants: BTreeMap<String, ItemVariant>,
}

impl SheetLocalInstance {
    pub fn new(project: String, path: String) -> Self {
        Self {
            project,
            path,
            page: None,
            variants: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SymbolLocalInstance {
    pub project: String,
    pub path: String,
    pub reference: Option<String>,
    pub unit: Option<i32>,
    pub variants: BTreeMap<String, ItemVariant>,
}

impl SymbolLocalInstance {
    pub fn new(project: String, path: String) -> Self {
        Self {
            project,
            path,
            reference: None,
            unit: None,
            variants: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BusAlias {
    pub name: String,
    pub members: Vec<String>,
}

impl BusAlias {
    pub fn new(name: String) -> Self {
        Self {
            name,
            members: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddedFile {
    pub name: Option<String>,
    pub checksum: Option<String>,
    pub file_type: Option<EmbeddedFileType>,
    pub data: Option<String>,
}

impl EmbeddedFile {
    pub fn new() -> Self {
        Self {
            name: None,
            checksum: None,
            file_type: None,
            data: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddedFileType {
    Datasheet,
    Font,
    Model,
    Worksheet,
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub name: Option<String>,
    pub uuid: Option<String>,
    pub lib_id: Option<String>,
    pub members: Vec<String>,
}

impl Group {
    pub fn new() -> Self {
        Self {
            name: None,
            uuid: None,
            lib_id: None,
            members: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SheetReference {
    pub sheet_uuid: Option<String>,
    pub sheet_name: Option<String>,
    pub filename: String,
    pub resolved_path: PathBuf,
}
