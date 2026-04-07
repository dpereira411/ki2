use std::collections::{BTreeMap, BTreeSet};
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
    pub file_format_version_at_load: Option<i32>,
    pub uuid: Option<String>,
    pub paper: Option<Paper>,
    pub page: Option<Page>,
    pub root_sheet_page: Option<String>,
    pub page_number: Option<String>,
    pub page_count: Option<usize>,
    pub virtual_page_number: Option<usize>,
    pub content_modified: bool,
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

#[derive(Debug, Clone, PartialEq)]
pub struct TitleBlock {
    pub title: Option<String>,
    pub date: Option<String>,
    pub revision: Option<String>,
    pub company: Option<String>,
    pub comments: [Option<String>; 9],
}

impl Default for TitleBlock {
    fn default() -> Self {
        Self {
            title: None,
            date: None,
            revision: None,
            company: None,
            comments: std::array::from_fn(|_| None),
        }
    }
}

impl TitleBlock {
    pub fn comment(&self, comment_number: usize) -> Option<&str> {
        self.comments
            .get(comment_number - 1)
            .and_then(|value| value.as_deref())
    }

    pub fn comment_count(&self) -> usize {
        self.comments.iter().filter(|value| value.is_some()).count()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LibSymbol {
    pub lib_id: String,
    pub name: String,
    pub extends: Option<String>,
    pub power: bool,
    pub local_power: bool,
    pub body_styles_specified: bool,
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
    pub jumper_pin_groups: Vec<BTreeSet<String>>,
    pub keywords: Option<String>,
    pub description: Option<String>,
    pub fp_filters_specified: bool,
    pub fp_filters: Vec<String>,
    pub locked_units: bool,
    pub properties: Vec<Property>,
    pub units: Vec<LibSymbolUnit>,
    pub embedded_fonts: Option<bool>,
    pub embedded_files: Vec<EmbeddedFile>,
}

impl LibSymbol {
    pub fn new(lib_id: String) -> Self {
        let name = lib_id
            .rsplit(':')
            .next()
            .unwrap_or(lib_id.as_str())
            .to_string();
        let mut properties = vec![
            Property::new(PropertyKind::SymbolReference, String::new()),
            Property::new(PropertyKind::SymbolValue, String::new()),
            Property::new(PropertyKind::SymbolFootprint, String::new()),
            Property::new(PropertyKind::SymbolDatasheet, String::new()),
            Property::new(PropertyKind::SymbolDescription, String::new()),
        ];
        for property in &mut properties {
            property.at = Some([0.0, 0.0]);
        }
        properties[2].visible = false;
        properties[3].visible = false;
        properties[4].visible = false;

        Self {
            units: vec![LibSymbolUnit {
                name: format!("{name}_1_1"),
                unit_number: 1,
                body_style: 1,
                unit_name: None,
                draw_item_kinds: Vec::new(),
                draw_items: Vec::new(),
            }],
            lib_id,
            name,
            extends: None,
            power: false,
            local_power: false,
            body_styles_specified: false,
            body_style_names: Vec::new(),
            has_demorgan: false,
            pin_name_offset: Some(0.508),
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
            fp_filters_specified: false,
            fp_filters: Vec::new(),
            locked_units: false,
            properties,
            embedded_fonts: None,
            embedded_files: Vec::new(),
        }
    }

    pub fn has_legacy_alternate_body_style(&self) -> bool {
        self.units.iter().any(|unit| unit.body_style > 1)
    }

    pub fn next_field_ordinal(&self) -> i32 {
        let property_ordinal = self.properties.iter().fold(42, |ordinal, property| {
            ordinal.max(property.sort_ordinal() + 1)
        });

        self.units.iter().fold(property_ordinal, |ordinal, unit| {
            unit.draw_items.iter().fold(ordinal, |ordinal, item| {
                ordinal.max(
                    item.field_ordinal
                        .map_or(ordinal, |item_ordinal| item_ordinal + 1),
                )
            })
        })
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

#[derive(Debug, Clone, PartialEq)]
pub struct LibDrawItem {
    pub kind: String,
    pub is_private: bool,
    pub field_ordinal: Option<i32>,
    pub field_id: Option<i32>,
    pub unit_number: i32,
    pub body_style: i32,
    pub visible: bool,
    pub show_name: bool,
    pub can_autoplace: bool,
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
    pub alternates: BTreeMap<String, LibPinAlternate>,
    pub stroke: Option<Stroke>,
    pub fill: Option<Fill>,
    pub effects: Option<TextEffects>,
    pub margins: Option<[f64; 4]>,
}

impl LibDrawItem {
    pub fn new(kind: &str, unit_number: i32, body_style: i32) -> Self {
        let mut stroke = Stroke::new();
        stroke.width = Some(0.0);

        let (points, radius, arc_center, arc_start_angle, arc_end_angle, angle) = match kind {
            "arc" => (
                vec![[1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
                None,
                Some([0.0, 0.0]),
                Some(0.0),
                Some(90.0),
                None,
            ),
            "circle" => (vec![[0.0, 0.0]], Some(1.0), None, None, None, None),
            "text_box" => (Vec::new(), None, None, None, None, Some(0.0)),
            _ => (Vec::new(), None, None, None, None, None),
        };

        Self {
            kind: kind.to_string(),
            is_private: false,
            field_ordinal: None,
            field_id: None,
            unit_number,
            body_style,
            visible: true,
            show_name: false,
            can_autoplace: true,
            at: None,
            angle,
            points,
            end: None,
            radius,
            arc_center,
            arc_start_angle,
            arc_end_angle,
            length: None,
            text: None,
            name: None,
            number: None,
            name_effects: None,
            number_effects: None,
            electrical_type: None,
            graphic_shape: None,
            alternates: BTreeMap::new(),
            stroke: Some(stroke),
            fill: Some(Fill::new()),
            effects: None,
            margins: None,
        }
    }

    fn sort_type_rank(&self) -> i32 {
        match self.kind.as_str() {
            "arc" | "bezier" | "circle" | "polyline" | "rectangle" => 0,
            "field" => 1,
            "text" => 2,
            "text_box" => 3,
            "pin" => 4,
            _ => 5,
        }
    }

    fn sort_position(&self) -> [f64; 2] {
        if let Some(at) = self.at {
            return at;
        }

        if let Some(point) = self.points.first() {
            return *point;
        }

        [0.0, 0.0]
    }
}

impl Eq for LibDrawItem {}

impl Ord for LibDrawItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        self.sort_type_rank()
            .cmp(&other.sort_type_rank())
            .then_with(|| self.unit_number.cmp(&other.unit_number))
            .then_with(|| self.body_style.cmp(&other.body_style))
            .then_with(|| self.is_private.cmp(&other.is_private))
            .then_with(|| {
                self.sort_position()[0]
                    .partial_cmp(&other.sort_position()[0])
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| {
                self.sort_position()[1]
                    .partial_cmp(&other.sort_position()[1])
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| self.field_ordinal.cmp(&other.field_ordinal))
            .then_with(|| self.kind.cmp(&other.kind))
            .then_with(|| self.text.cmp(&other.text))
            .then_with(|| self.name.cmp(&other.name))
            .then_with(|| self.number.cmp(&other.number))
    }
}

impl PartialOrd for LibDrawItem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
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
    pub size: f64,
    pub uuid: Option<String>,
}

impl NoConnect {
    pub fn new() -> Self {
        Self {
            at: [0.0, 0.0],
            size: 1.2192,
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
        let mut stroke = Stroke::new();
        stroke.width = Some(0.0);

        Self {
            at: [0.0, 0.0],
            size: [2.54, 2.54],
            has_stroke: false,
            stroke: Some(stroke),
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
        let mut stroke = Stroke::new();
        stroke.width = Some(0.0);

        Self {
            kind,
            points: vec![[0.0, 0.0], [0.0, 0.0]],
            has_stroke: false,
            stroke: Some(stroke),
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
    pub spin: LabelSpin,
    pub shape: LabelShape,
    pub pin_length: Option<f64>,
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
        let shape = match kind {
            LabelKind::Local => LabelShape::Input,
            LabelKind::Global => LabelShape::Bidirectional,
            LabelKind::Hierarchical => LabelShape::Input,
            LabelKind::Directive => LabelShape::Round,
        };
        let pin_length = match kind {
            LabelKind::Directive => Some(2.54),
            _ => None,
        };
        let properties = if matches!(kind, LabelKind::Global) {
            {
                let mut property = Property::new(
                    PropertyKind::GlobalLabelIntersheetRefs,
                    "${INTERSHEET_REFS}".to_string(),
                );
                property.visible = false;
                vec![property]
            }
        } else {
            Vec::new()
        };

        Self {
            kind,
            text,
            at: [0.0, 0.0],
            angle: 0.0,
            spin: LabelSpin::Right,
            shape,
            pin_length,
            excluded_from_sim: false,
            fields_autoplaced: FieldAutoplacement::None,
            visible: true,
            has_effects: false,
            effects: None,
            uuid: None,
            properties,
        }
    }

    pub fn next_field_ordinal(&self) -> i32 {
        self.properties.iter().fold(42, |ordinal, property| {
            ordinal.max(property.sort_ordinal() + 1)
        })
    }

    pub fn set_position(&mut self, at: [f64; 2]) {
        let delta = [at[0] - self.at[0], at[1] - self.at[1]];
        self.at = at;

        for property in &mut self.properties {
            if let Some(property_at) = property.at.as_mut() {
                property_at[0] += delta[0];
                property_at[1] += delta[1];
            }
        }
    }

    pub fn set_angle(&mut self, angle: f64) {
        self.angle = angle;
    }

    pub fn set_spin(&mut self, spin: LabelSpin) {
        self.spin = spin;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelKind {
    Local,
    Global,
    Hierarchical,
    Directive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelShape {
    Input,
    Output,
    Bidirectional,
    TriState,
    Unspecified,
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
    pub at: [f64; 3],
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
            at: [0.0, 0.0, 0.0],
            excluded_from_sim: false,
            fields_autoplaced: FieldAutoplacement::None,
            visible: true,
            has_effects: false,
            effects: None,
            uuid: None,
        }
    }

    pub fn set_position(&mut self, at: [f64; 2]) {
        self.at[0] = at[0];
        self.at[1] = at[1];
    }

    pub fn set_angle(&mut self, angle: f64) {
        self.at[2] = angle;
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
    pub visible: bool,
    pub has_effects: bool,
    pub effects: Option<TextEffects>,
    pub stroke: Option<Stroke>,
    pub fill: Option<Fill>,
    pub margins: Option<[f64; 4]>,
    pub uuid: Option<String>,
}

impl TextBox {
    pub fn new() -> Self {
        let mut stroke = Stroke::new();
        stroke.width = Some(0.0);

        Self {
            text: String::new(),
            at: [0.0, 0.0],
            angle: 0.0,
            end: [0.0, 0.0],
            excluded_from_sim: false,
            visible: true,
            has_effects: false,
            effects: None,
            stroke: Some(stroke),
            fill: Some(Fill::new()),
            margins: None,
            uuid: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableCell {
    pub text: String,
    pub row: usize,
    pub column: usize,
    pub at: [f64; 2],
    pub angle: f64,
    pub end: [f64; 2],
    pub excluded_from_sim: bool,
    pub visible: bool,
    pub has_effects: bool,
    pub effects: Option<TextEffects>,
    pub stroke: Option<Stroke>,
    pub fill: Option<Fill>,
    pub col_span: i32,
    pub row_span: i32,
    pub margins: Option<[f64; 4]>,
    pub uuid: Option<String>,
}

impl TableCell {
    pub fn new() -> Self {
        let mut stroke = Stroke::new();
        stroke.width = Some(0.0);

        Self {
            text: String::new(),
            row: 0,
            column: 0,
            at: [0.0, 0.0],
            angle: 0.0,
            end: [0.0, 0.0],
            excluded_from_sim: false,
            visible: true,
            has_effects: false,
            effects: None,
            stroke: Some(stroke),
            fill: Some(Fill::new()),
            col_span: 1,
            row_span: 1,
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
    pub border_external: bool,
    pub border_header: bool,
    pub border_stroke: Stroke,
    pub separators_rows: bool,
    pub separators_cols: bool,
    pub separators_stroke: Stroke,
    pub uuid: Option<String>,
}

impl Table {
    pub fn new(default_line_width: f64) -> Self {
        let mut border_stroke = Stroke::new();
        border_stroke.width = Some(default_line_width);
        let mut separators_stroke = Stroke::new();
        separators_stroke.width = Some(default_line_width);

        Self {
            default_line_width,
            column_count: None,
            column_widths: Vec::new(),
            row_heights: Vec::new(),
            cells: Vec::new(),
            border_external: true,
            border_header: true,
            border_stroke,
            separators_rows: true,
            separators_cols: true,
            separators_stroke,
            uuid: None,
        }
    }

    pub fn add_cell(&mut self, cell: TableCell) {
        let mut cell = cell;
        let (row, column) = self.next_available_cell_slot();
        cell.row = row;
        cell.column = column;
        self.cells.push(cell);
    }

    pub fn get_cell(&self, row: usize, column: usize) -> Option<&TableCell> {
        self.cells.iter().find(|cell| {
            let row_span = cell.row_span.max(1) as usize;
            let col_span = cell.col_span.max(1) as usize;
            row >= cell.row
                && row < cell.row + row_span
                && column >= cell.column
                && column < cell.column + col_span
        })
    }

    pub fn row_count(&self) -> usize {
        self.cells
            .iter()
            .map(|cell| cell.row + cell.row_span.max(1) as usize)
            .max()
            .unwrap_or(0)
    }

    fn next_available_cell_slot(&self) -> (usize, usize) {
        let column_count = self.column_count.unwrap_or(1).max(1) as usize;
        let mut row = 0usize;
        loop {
            for column in 0..column_count {
                if self.get_cell(row, column).is_none() {
                    return (row, column);
                }
            }

            row += 1;
        }
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
    pub hatch_lines: Vec<[[f64; 2]; 2]>,
    pub hatch_dirty: bool,
}

impl Shape {
    pub fn new(kind: ShapeKind) -> Self {
        let mut stroke = Stroke::new();
        stroke.width = Some(0.0);

        let (points, radius) = match kind {
            ShapeKind::Arc => (vec![[0.0, 0.0], [0.0, 0.0], [0.0, 0.0]], None),
            ShapeKind::Circle => (vec![[0.0, 0.0]], Some(0.0)),
            ShapeKind::Rectangle => (vec![[0.0, 0.0], [0.0, 0.0]], None),
            ShapeKind::Bezier => (vec![[0.0, 0.0], [0.0, 0.0], [0.0, 0.0], [0.0, 0.0]], None),
            ShapeKind::Polyline | ShapeKind::RuleArea => (Vec::new(), None),
        };

        Self {
            kind,
            points,
            radius,
            corner_radius: None,
            has_stroke: false,
            has_fill: false,
            stroke: Some(stroke),
            fill: Some(Fill::new()),
            excluded_from_sim: false,
            in_bom: true,
            on_board: true,
            dnp: false,
            uuid: None,
            hatch_lines: Vec::new(),
            hatch_dirty: true,
        }
    }

    // Upstream parity: reduced local analogue for `EDA_SHAPE::UpdateHatching()`. This is not a
    // 1:1 port of KiCad's polygon/hole cache because the current shape model still lacks
    // `SHAPE_POLY_SET`-style geometry ownership, but it now keeps current-screen hatch refresh as
    // real cached line state instead of a no-op. Remaining divergence is limited to the missing
    // polygon/knockout cache and fuller geometry exactness.
    pub fn update_hatching(&mut self) {
        if !self.hatch_dirty {
            return;
        }

        let Some(fill) = self.fill.as_ref() else {
            return;
        };

        let slopes: &[f64] = match fill.fill_type {
            FillType::Hatch => &[-1.0],
            FillType::ReverseHatch => &[1.0],
            FillType::CrossHatch => &[-1.0, 1.0],
            _ => return,
        };

        let spacing = self
            .stroke
            .as_ref()
            .and_then(|stroke| stroke.width)
            .unwrap_or(0.0)
            * 10.0;

        if spacing <= 0.0 {
            return;
        }

        let bbox = match self.hatch_bounds() {
            Some(bounds) => bounds,
            None => return,
        };

        self.hatch_lines.clear();

        let width = bbox[1][0] - bbox[0][0];
        let height = bbox[1][1] - bbox[0][1];
        let major = width.max(height);
        let step = if major > spacing * 100.0 {
            major / 100.0
        } else {
            spacing
        };

        let mut offset = 0.0;
        while offset <= major {
            for slope in slopes {
                if *slope < 0.0 {
                    self.hatch_lines.push([
                        [bbox[0][0], bbox[0][1] + offset.min(height)],
                        [bbox[0][0] + offset.min(width), bbox[0][1]],
                    ]);
                } else {
                    self.hatch_lines.push([
                        [bbox[0][0], bbox[1][1] - offset.min(height)],
                        [bbox[0][0] + offset.min(width), bbox[1][1]],
                    ]);
                }
            }

            offset += step;
        }

        self.hatch_dirty = false;
    }

    fn hatch_bounds(&self) -> Option<[[f64; 2]; 2]> {
        match self.kind {
            ShapeKind::Arc | ShapeKind::Bezier => None,
            ShapeKind::Circle => {
                let center = *self.points.first()?;
                let radius = self.radius?;
                Some([
                    [center[0] - radius, center[1] - radius],
                    [center[0] + radius, center[1] + radius],
                ])
            }
            ShapeKind::Rectangle => {
                let start = *self.points.first()?;
                let end = *self.points.get(1)?;
                Some([
                    [start[0].min(end[0]), start[1].min(end[1])],
                    [start[0].max(end[0]), start[1].max(end[1])],
                ])
            }
            ShapeKind::Polyline | ShapeKind::RuleArea => {
                if self.points.len() < 3 {
                    return None;
                }

                let mut min_x = self.points[0][0];
                let mut min_y = self.points[0][1];
                let mut max_x = self.points[0][0];
                let mut max_y = self.points[0][1];

                for point in &self.points[1..] {
                    min_x = min_x.min(point[0]);
                    min_y = min_y.min(point[1]);
                    max_x = max_x.max(point[0]);
                    max_y = max_y.max(point[1]);
                }

                Some([[min_x, min_y], [max_x, max_y]])
            }
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
    pub lib_symbol: Option<LibSymbol>,
    pub sim_model: Option<SimModel>,
    pub prefix: String,
    pub in_netlist: bool,
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
    pub occurrence_base: Option<Box<SymbolOccurrenceBase>>,
}

impl Symbol {
    pub fn new() -> Self {
        let mut properties = vec![
            Property::new(PropertyKind::SymbolReference, String::new()),
            Property::new(PropertyKind::SymbolValue, String::new()),
            Property::new(PropertyKind::SymbolFootprint, String::new()),
            Property::new(PropertyKind::SymbolDatasheet, String::new()),
            Property::new(PropertyKind::SymbolDescription, String::new()),
        ];
        for property in &mut properties {
            property.at = Some([0.0, 0.0]);
        }
        properties[2].visible = false;
        properties[3].visible = false;
        properties[4].visible = false;

        Self {
            lib_id: String::new(),
            lib_name: None,
            lib_symbol: None,
            sim_model: None,
            prefix: "U".to_string(),
            in_netlist: true,
            at: [0.0, 0.0],
            angle: 0.0,
            mirror: None,
            unit: Some(1),
            body_style: Some(1),
            excluded_from_sim: false,
            in_bom: true,
            on_board: true,
            in_pos_files: true,
            dnp: false,
            fields_autoplaced: FieldAutoplacement::None,
            uuid: None,
            properties,
            instances: Vec::new(),
            pins: Vec::new(),
            occurrence_base: None,
        }
    }

    pub fn set_field_text(&mut self, kind: PropertyKind, value: String) {
        let existing = self
            .properties
            .iter_mut()
            .find(|property| property.kind == kind)
            .expect("placed symbols start with mandatory fields");
        existing.id = kind.default_field_id().or(existing.id);
        existing.key = kind.canonical_key().to_string();
        existing.value = value;

        if kind == PropertyKind::SymbolReference {
            self.update_prefix_from_reference();
        }
    }

    pub fn set_position(&mut self, at: [f64; 2]) {
        let delta = [at[0] - self.at[0], at[1] - self.at[1]];
        self.at = at;

        for property in &mut self.properties {
            if let Some(property_at) = property.at.as_mut() {
                property_at[0] += delta[0];
                property_at[1] += delta[1];
            }
        }
    }

    pub fn set_angle(&mut self, angle: f64) {
        self.angle = angle;
    }

    pub fn update_prefix_from_reference(&mut self) {
        let Some(reference) = self
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolReference)
            .map(|property| property.value.replace('~', " "))
        else {
            return;
        };

        self.in_netlist = !reference.starts_with('#');

        let trimmed = reference
            .trim()
            .trim_end_matches(|ch: char| ch.is_ascii_digit() || matches!(ch, '?' | '*'))
            .trim();

        self.prefix = trimmed.to_string();
    }

    pub fn next_field_ordinal(&self) -> i32 {
        self.properties.iter().fold(42, |ordinal, property| {
            ordinal.max(property.sort_ordinal() + 1)
        })
    }

    // Upstream parity: local support for loader-side occurrence refresh. KiCad keeps this state on
    // its owning schematic/screen objects rather than a Rust-side snapshot, so this helper is not
    // 1:1 upstream. It exists to restore the symbol's non-occurrence baseline before reapplying
    // selected instance and current-variant state during load/current-sheet switching.
    pub fn capture_occurrence_base(&mut self) {
        self.occurrence_base = Some(Box::new(SymbolOccurrenceBase {
            unit: self.unit,
            excluded_from_sim: self.excluded_from_sim,
            in_bom: self.in_bom,
            on_board: self.on_board,
            in_pos_files: self.in_pos_files,
            dnp: self.dnp,
            properties: self.properties.clone(),
            sim_model: self.sim_model.clone(),
        }));
    }

    // Upstream parity: local support for loader-side occurrence refresh. This helper is not a 1:1
    // upstream function; it restores the Rust-side snapshot captured by `capture_occurrence_base`
    // so instance/variant refresh can start from a stable non-occurrence baseline.
    pub fn restore_occurrence_base(&mut self) {
        let Some(base) = self.occurrence_base.as_ref() else {
            return;
        };

        self.unit = base.unit;
        self.excluded_from_sim = base.excluded_from_sim;
        self.in_bom = base.in_bom;
        self.on_board = base.on_board;
        self.in_pos_files = base.in_pos_files;
        self.dnp = base.dnp;
        self.properties = base.properties.clone();
        self.sim_model = base.sim_model.clone();
    }

    pub fn sync_sim_model_from_properties(&mut self) {
        let value_binding = self
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .and_then(|property| match property.value.as_str() {
                "${SIM.PARAMS}" => Some(SimValueBinding::Params),
                "${SIM.NAME}" => Some(SimValueBinding::Name),
                _ => None,
            });
        let device = self
            .properties
            .iter()
            .find(|property| property.key == "Sim.Device")
            .map(|property| property.value.clone());
        let explicit_model_type = self
            .properties
            .iter()
            .find(|property| property.key == "Sim.Type")
            .map(|property| property.value.clone());
        let explicit_library = self
            .properties
            .iter()
            .find(|property| property.key == "Sim.Library")
            .map(|property| property.value.clone());
        let has_explicit_library = explicit_library.is_some();
        let explicit_name = self
            .properties
            .iter()
            .find(|property| property.key == "Sim.Name")
            .map(|property| property.value.clone());
        let has_explicit_name = explicit_name.is_some();
        let ibis_pin = self
            .properties
            .iter()
            .find(|property| property.key == "Sim.Ibis.Pin")
            .map(|property| property.value.clone());
        let ibis_model = self
            .properties
            .iter()
            .find(|property| property.key == "Sim.Ibis.Model")
            .map(|property| property.value.clone());
        let ibis_diff = self
            .properties
            .iter()
            .find(|property| property.key == "Sim.Ibis.Diff")
            .is_some_and(|property| property.value.trim() == "1");
        let params = self
            .properties
            .iter()
            .find(|property| property.key == "Sim.Params")
            .map(|property| property.value.clone());
        let param_pairs = params
            .as_deref()
            .map(parse_sim_param_pairs)
            .unwrap_or_default();
        let param_values = param_pairs.iter().cloned().collect::<BTreeMap<_, _>>();
        let model_type = explicit_model_type.or_else(|| {
            param_values
                .get("type")
                .filter(|value| !value.is_empty())
                .cloned()
        });
        let library = explicit_library.or_else(|| {
            param_values
                .get("lib")
                .filter(|value| !value.is_empty())
                .cloned()
        });
        let name = explicit_name.or_else(|| {
            param_values
                .get("model")
                .filter(|value| !value.is_empty())
                .cloned()
        });
        let pin_pairs = self
            .properties
            .iter()
            .find(|property| property.key == "Sim.Pins")
            .map(|property| parse_sim_pin_pairs(&property.value))
            .unwrap_or_default();
        let pins = pin_pairs.iter().cloned().collect::<BTreeMap<_, _>>();
        let origin = if device
            .as_deref()
            .is_some_and(|device| device.trim() == "IBIS")
            || ibis_diff
            || ibis_pin.is_some()
            || ibis_model.is_some()
        {
            Some(SimModelOrigin::Ibis)
        } else if library.is_some() || (has_explicit_library && has_explicit_name) {
            Some(SimModelOrigin::LibraryReference)
        } else if device.as_deref() == Some("SPICE") {
            Some(SimModelOrigin::RawSpice)
        } else if is_supported_builtin_sim_type(device.as_deref(), model_type.as_deref()) {
            Some(SimModelOrigin::BuiltIn)
        } else if value_binding.is_some() {
            Some(SimModelOrigin::InferredValue)
        } else if device.is_some()
            || ibis_diff
            || library.is_some()
            || name.is_some()
            || params.is_some()
            || !pins.is_empty()
        {
            Some(SimModelOrigin::Fields)
        } else {
            None
        };

        if device.is_none()
            && model_type.is_none()
            && library.is_none()
            && name.is_none()
            && ibis_pin.is_none()
            && ibis_model.is_none()
            && !ibis_diff
            && params.is_none()
            && pins.is_empty()
        {
            self.sim_model = None;
            return;
        }

        self.sim_model = Some(SimModel {
            device,
            model_type,
            library,
            name,
            ibis_pin,
            ibis_model,
            ibis_diff,
            params,
            param_pairs,
            param_values,
            pin_pairs,
            pins,
            value_binding,
            stored_value: None,
            enabled: !self.excluded_from_sim,
            origin,
            resolved_library: None,
            resolved_name: None,
            resolved_kind: None,
            resolved_model_type: None,
            resolved_ibis_model_type: None,
            resolved_ibis_diff_pin: None,
            generated_pin_names: Vec::new(),
            generated_param_pairs: Vec::new(),
        });
    }
}

fn is_supported_builtin_sim_type(device: Option<&str>, model_type: Option<&str>) -> bool {
    let Some(device) = device else {
        return false;
    };

    let device = device.trim();
    let model_type = model_type.unwrap_or("").trim();

    matches!(
        (device, model_type),
        ("R", "" | "POT" | "=")
            | ("C" | "L", "" | "=")
            | ("K", "")
            | ("TLINE", "" | "RLGC")
            | ("SW", "V" | "I")
            | ("D", "")
            | ("NPN" | "PNP", "VBIC" | "GUMMELPOON" | "HICUM2" | "HICUML2")
            | ("NJFET" | "PJFET", "SHICHMANHODGES" | "PARKERSKELLERN")
            | ("NMES" | "PMES", "STATZ" | "YTTERDAL" | "HFET1" | "HFET2")
            | (
                "NMOS" | "PMOS",
                "VDMOS"
                    | "MOS1"
                    | "MOS2"
                    | "MOS3"
                    | "BSIM1"
                    | "BSIM2"
                    | "MOS6"
                    | "BSIM3"
                    | "MOS9"
                    | "B4SOI"
                    | "BSIM4"
                    | "B3SOIFD"
                    | "B3SOIDD"
                    | "B3SOIPD"
                    | "HISIM2"
                    | "HISIMHV1"
                    | "HISIMHV2"
            )
            | (
                "V" | "I",
                "DC" | "SIN"
                    | "PULSE"
                    | "EXP"
                    | "AM"
                    | "SFFM"
                    | "PWL"
                    | "WHITENOISE"
                    | "PINKNOISE"
                    | "BURSTNOISE"
                    | "RANDUNIFORM"
                    | "RANDGAUSSIAN"
                    | "RANDEXP"
                    | "RANDPOISSON"
                    | "TRNOISE"
                    | "TRRANDOM"
                    | "="
            )
            | ("E" | "F" | "G" | "H" | "SUBCKT" | "XSPICE", "")
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimValueBinding {
    Value,
    Params,
    Name,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SymbolOccurrenceBase {
    pub unit: Option<i32>,
    pub excluded_from_sim: bool,
    pub in_bom: bool,
    pub on_board: bool,
    pub in_pos_files: bool,
    pub dnp: bool,
    pub properties: Vec<Property>,
    pub sim_model: Option<SimModel>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SheetOccurrenceBase {
    pub excluded_from_sim: bool,
    pub in_bom: bool,
    pub on_board: bool,
    pub dnp: bool,
    pub properties: Vec<Property>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimModelOrigin {
    Fields,
    RawSpice,
    BuiltIn,
    LibraryReference,
    Ibis,
    InferredValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimLibraryKind {
    Spice,
    Ibis,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimLibrarySource {
    Filesystem(PathBuf),
    SchematicEmbedded { name: String },
    SymbolEmbedded { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSimLibrary {
    pub source: SimLibrarySource,
    pub kind: SimLibraryKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedSimModelKind {
    SpiceModel,
    SpiceSubckt,
    IbisComponent,
    IbisDriverDc,
    IbisDriverRect,
    IbisDriverPrbs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimModel {
    pub device: Option<String>,
    pub model_type: Option<String>,
    pub library: Option<String>,
    pub name: Option<String>,
    pub ibis_pin: Option<String>,
    pub ibis_model: Option<String>,
    pub ibis_diff: bool,
    pub params: Option<String>,
    pub param_pairs: Vec<(String, String)>,
    pub param_values: BTreeMap<String, String>,
    pub pin_pairs: Vec<(String, String)>,
    pub pins: BTreeMap<String, String>,
    pub value_binding: Option<SimValueBinding>,
    pub stored_value: Option<String>,
    pub enabled: bool,
    pub origin: Option<SimModelOrigin>,
    pub resolved_library: Option<ResolvedSimLibrary>,
    pub resolved_name: Option<String>,
    pub resolved_kind: Option<ResolvedSimModelKind>,
    pub resolved_model_type: Option<String>,
    pub resolved_ibis_model_type: Option<String>,
    pub resolved_ibis_diff_pin: Option<String>,
    pub generated_pin_names: Vec<String>,
    pub generated_param_pairs: Vec<(String, Option<String>)>,
}

fn skip_sim_whitespace(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while chars.peek().is_some_and(|ch| ch.is_whitespace()) {
        chars.next();
    }
}

fn parse_sim_bare_token(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut token = String::new();

    while let Some(ch) = chars.peek().copied() {
        if ch.is_whitespace() || ch == '=' {
            break;
        }

        token.push(ch);
        chars.next();
    }

    token
}

fn parse_sim_quoted_token(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut token = String::new();

    while let Some(ch) = chars.next() {
        if ch == '"' {
            break;
        }

        if ch == '\\' {
            if let Some(escaped) = chars.next() {
                token.push(escaped);
            }
        } else {
            token.push(ch);
        }
    }

    token
}

pub(crate) fn parse_sim_param_pairs(params: &str) -> Vec<(String, String)> {
    let mut values = Vec::new();
    let mut chars = params.chars().peekable();

    while chars.peek().is_some() {
        skip_sim_whitespace(&mut chars);

        let key = parse_sim_bare_token(&mut chars);

        if key.is_empty() {
            break;
        }

        if chars.next_if_eq(&'=').is_none() {
            values.push((key, "1".to_string()));
            continue;
        }

        let value = if chars.next_if_eq(&'"').is_some() {
            parse_sim_quoted_token(&mut chars)
        } else {
            parse_sim_bare_token(&mut chars)
        };

        values.push((key, normalize_sim_param_value(&value)));
    }

    values
}

fn normalize_sim_param_value(value: &str) -> String {
    normalize_sim_si_value(value).unwrap_or_else(|| value.to_string())
}

fn normalize_sim_si_value(value: &str) -> Option<String> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        return None;
    }

    let split_at = trimmed
        .find(|ch: char| !(ch.is_ascii_digit() || matches!(ch, '.' | ',' | ' ')))
        .unwrap_or(trimmed.len());
    let (mantissa, suffix) = trimmed.split_at(split_at);

    if mantissa.is_empty() {
        return None;
    }

    let normalized_mantissa = normalize_sim_si_mantissa(mantissa)?;
    let normalized_suffix = normalize_sim_si_suffix(suffix.trim())?;

    Some(format!("{normalized_mantissa}{normalized_suffix}"))
}

fn normalize_sim_si_suffix(suffix: &str) -> Option<String> {
    if suffix.is_empty() {
        return Some(String::new());
    }

    if suffix == "µ" || suffix == "μ" {
        return Some("u".to_string());
    }

    if suffix.eq_ignore_ascii_case("Meg") {
        return Some("M".to_string());
    }

    if !suffix.chars().all(|ch| {
        matches!(
            ch,
            'f' | 'F'
                | 'p'
                | 'P'
                | 'n'
                | 'N'
                | 'u'
                | 'U'
                | 'm'
                | 'M'
                | 'k'
                | 'K'
                | 'g'
                | 'G'
                | 't'
                | 'T'
                | 'r'
                | 'R'
                | 'h'
                | 'H'
                | 'o'
                | 'O'
                | 'v'
                | 'V'
                | 'a'
                | 'A'
                | 'Ω'
                | 'Ω'
                | 'µ'
                | 'μ'
        )
    }) {
        return None;
    }

    Some(
        suffix
            .chars()
            .map(|ch| match ch {
                'µ' | 'μ' => 'u',
                _ => ch,
            })
            .collect(),
    )
}

fn normalize_sim_si_mantissa(mantissa: &str) -> Option<String> {
    let mut compact = mantissa.replace(' ', "");

    if compact.is_empty() {
        return None;
    }

    let mut ambiguous_separator: Option<char> = None;
    let mut thousands_separator: Option<char> = None;
    let mut thousands_found = false;
    let mut decimal_separator: Option<char> = None;
    let mut decimal_found = false;
    let mut digits = 0usize;
    let chars = compact.chars().collect::<Vec<_>>();

    for index in (0..chars.len()).rev() {
        let ch = chars[index];

        if ch.is_ascii_digit() {
            digits += 1;
            continue;
        }

        if !matches!(ch, '.' | ',') {
            return None;
        }

        match (decimal_separator, thousands_separator, ambiguous_separator) {
            (Some(decimal), Some(thousands), _) => {
                if ch == decimal {
                    if thousands_found || decimal_found {
                        return None;
                    }

                    decimal_found = true;
                } else if ch == thousands {
                    if digits != 3 {
                        return None;
                    }

                    thousands_found = true;
                } else {
                    return None;
                }
            }
            (None, None, Some(ambiguous)) => {
                if ch == ambiguous {
                    thousands_separator = Some(ambiguous);
                    thousands_found = true;
                    decimal_separator = Some(if ch == '.' { ',' } else { '.' });
                } else {
                    decimal_separator = Some(ambiguous);
                    decimal_found = true;
                    thousands_separator = Some(ch);
                    thousands_found = true;
                }
            }
            _ => {
                if (index == 1 && chars[0] == '0') || digits != 3 {
                    decimal_separator = Some(ch);
                    decimal_found = true;
                    thousands_separator = Some(if ch == '.' { ',' } else { '.' });
                } else {
                    ambiguous_separator = Some(ch);
                }
            }
        }

        digits = 0;
    }

    if decimal_separator.is_none() && thousands_separator.is_none() {
        decimal_separator = Some('.');
        thousands_separator = Some(',');
    }

    if let Some(thousands) = thousands_separator {
        compact = compact.replace(thousands, "");
    }

    if let Some(decimal) = decimal_separator {
        compact = compact
            .chars()
            .map(|ch| if ch == decimal { '.' } else { ch })
            .collect();
    }

    Some(compact)
}

fn parse_sim_pin_pairs(pins: &str) -> Vec<(String, String)> {
    let mut values = Vec::new();
    let mut chars = pins.chars().peekable();

    while chars.peek().is_some() {
        skip_sim_whitespace(&mut chars);

        let symbol_pin = parse_sim_bare_token(&mut chars);

        if symbol_pin.is_empty() {
            break;
        }

        if chars.next_if_eq(&'=').is_none() {
            continue;
        }

        let model_pin = if chars.next_if_eq(&'"').is_some() {
            parse_sim_quoted_token(&mut chars)
        } else {
            parse_sim_bare_token(&mut chars)
        };

        values.push((symbol_pin, model_pin));
    }

    values
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    pub at: [f64; 2],
    pub size: [f64; 2],
    pub border_width: f64,
    pub border_color: Option<[f64; 4]>,
    pub background_color: Option<[f64; 4]>,
    pub excluded_from_sim: bool,
    pub in_bom: bool,
    pub on_board: bool,
    pub dnp: bool,
    pub fields_autoplaced: FieldAutoplacement,
    pub uuid: Option<String>,
    pub properties: Vec<Property>,
    pub pins: Vec<SheetPin>,
    pub instances: Vec<SheetLocalInstance>,
    pub occurrence_base: Option<Box<SheetOccurrenceBase>>,
}

impl Sheet {
    pub fn new() -> Self {
        Self {
            at: [0.0, 0.0],
            size: [0.0, 0.0],
            border_width: 0.0,
            border_color: None,
            background_color: None,
            excluded_from_sim: false,
            in_bom: true,
            on_board: true,
            dnp: false,
            fields_autoplaced: FieldAutoplacement::Auto,
            uuid: None,
            properties: vec![
                Property::new(PropertyKind::SheetName, String::new()),
                Property::new(PropertyKind::SheetFile, String::new()),
            ],
            pins: Vec::new(),
            instances: Vec::new(),
            occurrence_base: None,
        }
    }

    pub fn set_position(&mut self, at: [f64; 2]) {
        let delta = [at[0] - self.at[0], at[1] - self.at[1]];
        self.at = at;

        for pin in &mut self.pins {
            pin.at[0] += delta[0];
            pin.at[1] += delta[1];
        }
    }

    pub fn set_size(&mut self, size: [f64; 2]) {
        self.size = size;

        for pin in &mut self.pins {
            pin.constrain_on_sheet_edge(self.at, self.size, false);
        }
    }

    pub fn name(&self) -> Option<&str> {
        self.properties
            .iter()
            .find(|property| property.kind == PropertyKind::SheetName)
            .map(|property| property.value.as_str())
    }

    pub fn filename(&self) -> Option<String> {
        self.properties
            .iter()
            .find(|property| property.kind == PropertyKind::SheetFile)
            .map(|property| property.value.replace('\\', "/"))
    }

    pub fn is_vertical_orientation(&self) -> bool {
        self.size[1] > self.size[0]
    }

    pub fn next_field_ordinal(&self) -> i32 {
        self.properties.iter().fold(42, |ordinal, property| {
            ordinal.max(property.sort_ordinal() + 1)
        })
    }

    // Upstream parity: local support for loader-side sheet occurrence refresh. KiCad keeps this
    // state on its owning project/sheet objects rather than a Rust snapshot, so this helper is not
    // 1:1 upstream. It exists to restore the sheet's non-variant baseline before reapplying live
    // current-variant state in the current model.
    pub fn capture_occurrence_base(&mut self) {
        self.occurrence_base = Some(Box::new(SheetOccurrenceBase {
            excluded_from_sim: self.excluded_from_sim,
            in_bom: self.in_bom,
            on_board: self.on_board,
            dnp: self.dnp,
            properties: self.properties.clone(),
        }));
    }

    // Upstream parity: local support for loader-side sheet occurrence refresh. This helper is not
    // a 1:1 upstream function; it restores the Rust-side snapshot captured by
    // `capture_occurrence_base` so variant refresh can start from a stable baseline.
    pub fn restore_occurrence_base(&mut self) {
        let Some(base) = self.occurrence_base.as_ref() else {
            return;
        };

        self.excluded_from_sim = base.excluded_from_sim;
        self.in_bom = base.in_bom;
        self.on_board = base.on_board;
        self.dnp = base.dnp;
        self.properties = base.properties.clone();
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        BusEntry, FieldAutoplacement, Junction, Label, LabelKind, LabelShape, LabelSpin,
        LibDrawItem, LibSymbol, LibSymbolUnit, Line, LineKind, NoConnect, Property, PropertyKind,
        Shape, ShapeKind, Sheet, SheetLocalInstance, SheetPin, SheetPinShape, SheetSide,
        SimModelOrigin, SimValueBinding, StrokeStyle, Symbol, SymbolLocalInstance, SymbolPin,
        Table, TableCell, Text, TextBox, TextKind,
    };

    fn push_lib_draw_item(symbol: &mut LibSymbol, item: LibDrawItem) {
        for unit_number in 1..=item.unit_number.max(1) {
            for body_style in 1..=item.body_style.max(1) {
                if symbol.units.iter().any(|existing| {
                    existing.unit_number == unit_number && existing.body_style == body_style
                }) {
                    continue;
                }

                let unit_name = symbol
                    .units
                    .iter()
                    .find(|existing| existing.unit_number == unit_number)
                    .and_then(|existing| existing.unit_name.clone());

                symbol.units.push(LibSymbolUnit {
                    name: format!("{}_{}_{}", symbol.name, unit_number, body_style),
                    unit_number,
                    body_style,
                    unit_name,
                    draw_item_kinds: Vec::new(),
                    draw_items: Vec::new(),
                });
            }
        }

        symbol
            .units
            .sort_by_key(|unit| (unit.unit_number, unit.body_style));
        let index = symbol
            .units
            .iter()
            .position(|existing| {
                existing.unit_number == item.unit_number && existing.body_style == item.body_style
            })
            .expect("materialized lib symbol unit must exist");
        symbol.units[index].draw_item_kinds.push(item.kind.clone());
        symbol.units[index].draw_items.push(item);
    }

    #[test]
    fn placed_symbols_start_with_mandatory_fields() {
        let symbol = Symbol::new();

        assert_eq!(symbol.prefix, "U");
        assert!(symbol.in_netlist);
        assert_eq!(symbol.unit, Some(1));
        assert_eq!(symbol.body_style, Some(1));
        assert_eq!(
            symbol
                .properties
                .iter()
                .map(|property| property.kind)
                .collect::<Vec<_>>(),
            vec![
                PropertyKind::SymbolReference,
                PropertyKind::SymbolValue,
                PropertyKind::SymbolFootprint,
                PropertyKind::SymbolDatasheet,
                PropertyKind::SymbolDescription,
            ]
        );
        assert_eq!(symbol.next_field_ordinal(), 42);
    }

    #[test]
    fn reference_updates_refresh_symbol_prefix() {
        let mut symbol = Symbol::new();

        symbol.set_field_text(PropertyKind::SymbolReference, "J12".to_string());
        assert_eq!(symbol.prefix, "J");
        assert!(symbol.in_netlist);

        symbol.set_field_text(PropertyKind::SymbolReference, "TP?".to_string());
        assert_eq!(symbol.prefix, "TP");
        assert!(symbol.in_netlist);

        symbol.set_field_text(PropertyKind::SymbolReference, "#PWR01".to_string());
        assert_eq!(symbol.prefix, "#PWR");
        assert!(!symbol.in_netlist);

        symbol.set_field_text(PropertyKind::SymbolReference, "".to_string());
        assert_eq!(symbol.prefix, "");
        assert!(symbol.in_netlist);
    }

    #[test]
    fn shared_symbol_field_setter_preserves_mandatory_identity() {
        let mut symbol = Symbol::new();

        symbol.set_field_text(PropertyKind::SymbolValue, "10k".to_string());
        symbol.set_field_text(
            PropertyKind::SymbolFootprint,
            "Resistor_SMD:R_0603".to_string(),
        );

        let value = symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolValue)
            .expect("value field");
        assert_eq!(value.id, Some(2));
        assert_eq!(value.key, "Value");
        assert_eq!(value.value, "10k");

        let footprint = symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolFootprint)
            .expect("footprint field");
        assert_eq!(footprint.id, Some(3));
        assert_eq!(footprint.key, "Footprint");
        assert_eq!(footprint.value, "Resistor_SMD:R_0603");
    }

    #[test]
    fn symbol_syncs_structured_sim_model_fields() {
        let mut symbol = Symbol::new();
        symbol.properties.push(Property::new_named(
            PropertyKind::User,
            "Sim.Device",
            "SPICE".to_string(),
            false,
        ));
        symbol.properties.push(Property::new_named(
            PropertyKind::User,
            "Sim.Params",
            r#"type="Q" model="BC\"547" lib="models.lib""#.to_string(),
            false,
        ));
        symbol.properties.push(Property::new_named(
            PropertyKind::User,
            "Sim.Library",
            "models.kicad_sim".to_string(),
            false,
        ));
        symbol.properties.push(Property::new_named(
            PropertyKind::User,
            "Sim.Name",
            "2N3904".to_string(),
            false,
        ));
        symbol.properties.push(Property::new_named(
            PropertyKind::User,
            "Sim.Ibis.Pin",
            "A1".to_string(),
            false,
        ));
        symbol.properties.push(Property::new_named(
            PropertyKind::User,
            "Sim.Ibis.Model",
            "TX_MODEL".to_string(),
            false,
        ));
        symbol.properties.push(Property::new_named(
            PropertyKind::User,
            "Sim.Ibis.Diff",
            "1".to_string(),
            false,
        ));
        symbol.properties.push(Property::new_named(
            PropertyKind::User,
            "Sim.Pins",
            "1=1 2=2".to_string(),
            false,
        ));

        symbol.sync_sim_model_from_properties();

        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .and_then(|sim_model| sim_model.device.as_deref()),
            Some("SPICE")
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .and_then(|sim_model| sim_model.library.as_deref()),
            Some("models.kicad_sim")
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .and_then(|sim_model| sim_model.name.as_deref()),
            Some("2N3904")
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .and_then(|sim_model| sim_model.ibis_pin.as_deref()),
            Some("A1")
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .and_then(|sim_model| sim_model.ibis_model.as_deref()),
            Some("TX_MODEL")
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .map(|sim_model| sim_model.ibis_diff),
            Some(true)
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .and_then(|sim_model| sim_model.model_type.as_deref()),
            Some("Q")
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .map(|sim_model| sim_model.param_pairs.clone()),
            Some(vec![
                ("type".to_string(), "Q".to_string()),
                ("model".to_string(), "BC\"547".to_string()),
                ("lib".to_string(), "models.lib".to_string()),
            ])
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .map(|sim_model| sim_model.param_values.clone()),
            Some(BTreeMap::from([
                ("lib".to_string(), "models.lib".to_string()),
                ("model".to_string(), "BC\"547".to_string()),
                ("type".to_string(), "Q".to_string()),
            ]))
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .map(|sim_model| sim_model.pin_pairs.clone()),
            Some(vec![
                ("1".to_string(), "1".to_string()),
                ("2".to_string(), "2".to_string()),
            ])
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .map(|sim_model| sim_model.pins.clone()),
            Some(BTreeMap::from([
                ("1".to_string(), "1".to_string()),
                ("2".to_string(), "2".to_string()),
            ]))
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .and_then(|sim_model| sim_model.value_binding),
            None
        );
        assert_eq!(
            symbol.sim_model.as_ref().map(|sim_model| sim_model.enabled),
            Some(true)
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .and_then(|sim_model| sim_model.origin),
            Some(SimModelOrigin::Ibis)
        );
    }

    #[test]
    fn current_value_backed_sim_binding_variant_exists_for_loader_hydration() {
        let _binding = SimValueBinding::Value;
        assert_eq!(_binding, SimValueBinding::Value);
    }

    #[test]
    fn symbol_treats_explicit_ibis_device_field_as_ibis_state() {
        let mut symbol = Symbol::new();
        symbol.properties.push(Property::new_named(
            PropertyKind::User,
            "Sim.Device",
            "IBIS".to_string(),
            false,
        ));
        symbol.properties.push(Property::new_named(
            PropertyKind::User,
            "Sim.Library",
            "driver.ibs".to_string(),
            false,
        ));
        symbol.properties.push(Property::new_named(
            PropertyKind::User,
            "Sim.Name",
            "DRIVER".to_string(),
            false,
        ));

        symbol.sync_sim_model_from_properties();

        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .and_then(|sim_model| sim_model.origin),
            Some(SimModelOrigin::Ibis)
        );
    }

    #[test]
    fn symbol_syncs_library_backed_sim_state_from_raw_params() {
        let mut symbol = Symbol::new();
        symbol.properties.push(Property::new_named(
            PropertyKind::User,
            "Sim.Device",
            "SPICE".to_string(),
            false,
        ));
        symbol.properties.push(Property::new_named(
            PropertyKind::User,
            "Sim.Params",
            r#"type="Q" model="BC\"547" lib="models.lib""#.to_string(),
            false,
        ));

        symbol.sync_sim_model_from_properties();

        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .and_then(|sim_model| sim_model.device.as_deref()),
            Some("SPICE")
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .and_then(|sim_model| sim_model.model_type.as_deref()),
            Some("Q")
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .and_then(|sim_model| sim_model.library.as_deref()),
            Some("models.lib")
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .and_then(|sim_model| sim_model.name.as_deref()),
            Some("BC\"547")
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .map(|sim_model| sim_model.param_pairs.clone()),
            Some(vec![
                ("type".to_string(), "Q".to_string()),
                ("model".to_string(), "BC\"547".to_string()),
                ("lib".to_string(), "models.lib".to_string()),
            ])
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .map(|sim_model| sim_model.param_values.clone()),
            Some(BTreeMap::from([
                ("lib".to_string(), "models.lib".to_string()),
                ("model".to_string(), "BC\"547".to_string()),
                ("type".to_string(), "Q".to_string()),
            ]))
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .and_then(|sim_model| sim_model.value_binding),
            None
        );
        assert_eq!(
            symbol.sim_model.as_ref().map(|sim_model| sim_model.enabled),
            Some(true)
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .and_then(|sim_model| sim_model.origin),
            Some(SimModelOrigin::LibraryReference)
        );
    }

    #[test]
    fn symbol_preserves_duplicate_and_ordered_sim_param_pairs() {
        let mut symbol = Symbol::new();
        symbol.properties.push(Property::new_named(
            PropertyKind::User,
            "Sim.Params",
            r#"gain=1 gain=2 mode="fast" extra="x y""#.to_string(),
            false,
        ));

        symbol.sync_sim_model_from_properties();

        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .map(|sim_model| sim_model.param_pairs.clone()),
            Some(vec![
                ("gain".to_string(), "1".to_string()),
                ("gain".to_string(), "2".to_string()),
                ("mode".to_string(), "fast".to_string()),
                ("extra".to_string(), "x y".to_string()),
            ])
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .map(|sim_model| sim_model.param_values.clone()),
            Some(BTreeMap::from([
                ("extra".to_string(), "x y".to_string()),
                ("gain".to_string(), "2".to_string()),
                ("mode".to_string(), "fast".to_string()),
            ]))
        );
    }

    #[test]
    fn symbol_preserves_flag_and_quoted_sim_param_pairs() {
        let mut symbol = Symbol::new();
        symbol.properties.push(Property::new_named(
            PropertyKind::User,
            "Sim.Params",
            r#"flag gain=2 model="BC\"547" extra="x y""#.to_string(),
            false,
        ));

        symbol.sync_sim_model_from_properties();

        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .map(|sim_model| sim_model.param_pairs.clone()),
            Some(vec![
                ("flag".to_string(), "1".to_string()),
                ("gain".to_string(), "2".to_string()),
                ("model".to_string(), "BC\"547".to_string()),
                ("extra".to_string(), "x y".to_string()),
            ])
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .map(|sim_model| sim_model.param_values.clone()),
            Some(BTreeMap::from([
                ("extra".to_string(), "x y".to_string()),
                ("flag".to_string(), "1".to_string()),
                ("gain".to_string(), "2".to_string()),
                ("model".to_string(), "BC\"547".to_string()),
            ]))
        );
    }

    #[test]
    fn symbol_normalizes_sim_param_pair_si_values() {
        let mut symbol = Symbol::new();
        symbol.properties.push(Property::new_named(
            PropertyKind::User,
            "Sim.Params",
            r#"gain=1Meg bias=3,300u extra="x y""#.to_string(),
            false,
        ));

        symbol.sync_sim_model_from_properties();

        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .map(|sim_model| sim_model.param_pairs.clone()),
            Some(vec![
                ("gain".to_string(), "1M".to_string()),
                ("bias".to_string(), "3300u".to_string()),
                ("extra".to_string(), "x y".to_string()),
            ])
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .map(|sim_model| sim_model.param_values.clone()),
            Some(BTreeMap::from([
                ("bias".to_string(), "3300u".to_string()),
                ("extra".to_string(), "x y".to_string()),
                ("gain".to_string(), "1M".to_string()),
            ]))
        );
    }

    #[test]
    fn symbol_preserves_ordered_sim_pin_pairs() {
        let mut symbol = Symbol::new();
        symbol.properties.push(Property::new_named(
            PropertyKind::User,
            "Sim.Pins",
            "2=1 1=2 10=3".to_string(),
            false,
        ));

        symbol.sync_sim_model_from_properties();

        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .map(|sim_model| sim_model.pin_pairs.clone()),
            Some(vec![
                ("2".to_string(), "1".to_string()),
                ("1".to_string(), "2".to_string()),
                ("10".to_string(), "3".to_string()),
            ])
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .map(|sim_model| sim_model.pins.clone()),
            Some(BTreeMap::from([
                ("1".to_string(), "2".to_string()),
                ("2".to_string(), "1".to_string()),
                ("10".to_string(), "3".to_string()),
            ]))
        );
    }

    #[test]
    fn symbol_preserves_quoted_sim_pin_pairs() {
        let mut symbol = Symbol::new();
        symbol.properties.push(Property::new_named(
            PropertyKind::User,
            "Sim.Pins",
            r#"1="PIN A" 2=B"#.to_string(),
            false,
        ));

        symbol.sync_sim_model_from_properties();

        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .map(|sim_model| sim_model.pin_pairs.clone()),
            Some(vec![
                ("1".to_string(), "PIN A".to_string()),
                ("2".to_string(), "B".to_string()),
            ])
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .map(|sim_model| sim_model.pins.clone()),
            Some(BTreeMap::from([
                ("1".to_string(), "PIN A".to_string()),
                ("2".to_string(), "B".to_string()),
            ]))
        );
    }

    #[test]
    fn symbol_tracks_sim_value_binding_placeholders() {
        let mut symbol = Symbol::new();
        symbol.set_field_text(PropertyKind::SymbolValue, "${SIM.PARAMS}".to_string());
        symbol.properties.push(Property::new_named(
            PropertyKind::User,
            "Sim.Device",
            "SPICE".to_string(),
            false,
        ));

        symbol.sync_sim_model_from_properties();

        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .and_then(|sim_model| sim_model.value_binding),
            Some(SimValueBinding::Params)
        );

        symbol.set_field_text(PropertyKind::SymbolValue, "${SIM.NAME}".to_string());
        symbol.sync_sim_model_from_properties();

        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .and_then(|sim_model| sim_model.value_binding),
            Some(SimValueBinding::Name)
        );
    }

    #[test]
    fn symbol_tracks_sim_enabled_state() {
        let mut symbol = Symbol::new();
        symbol.excluded_from_sim = true;
        symbol.properties.push(Property::new_named(
            PropertyKind::User,
            "Sim.Device",
            "SPICE".to_string(),
            false,
        ));

        symbol.sync_sim_model_from_properties();

        assert_eq!(
            symbol.sim_model.as_ref().map(|sim_model| sim_model.enabled),
            Some(false)
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .and_then(|sim_model| sim_model.origin),
            Some(SimModelOrigin::RawSpice)
        );
    }

    #[test]
    fn symbol_treats_ibis_diff_field_as_ibis_state() {
        let mut symbol = Symbol::new();
        symbol.properties.push(Property::new_named(
            PropertyKind::User,
            "Sim.Ibis.Diff",
            "1".to_string(),
            false,
        ));

        symbol.sync_sim_model_from_properties();

        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .map(|sim_model| sim_model.ibis_diff),
            Some(true)
        );
        assert_eq!(
            symbol
                .sim_model
                .as_ref()
                .and_then(|sim_model| sim_model.origin),
            Some(SimModelOrigin::Ibis)
        );
    }

    #[test]
    fn hierarchical_references_do_not_seed_live_symbol_state_from_first_instance() {
        let mut symbol = Symbol::new();
        let mut instance = SymbolLocalInstance {
            project: "demo".to_string(),
            path: "/A".to_string(),
            reference: None,
            unit: Some(1),
            value: None,
            footprint: None,
            variants: BTreeMap::new(),
        };
        instance.reference = Some("R7".to_string());
        instance.unit = Some(2);

        symbol
            .instances
            .retain(|existing| existing.path != instance.path);
        symbol.instances.push(instance);

        assert_eq!(symbol.unit, Some(1));
        assert_eq!(
            symbol
                .properties
                .iter()
                .find(|property| property.kind == PropertyKind::SymbolReference)
                .map(|property| property.value.as_str()),
            Some("")
        );
    }

    #[test]
    fn hierarchical_references_overwrite_by_path() {
        let mut symbol = Symbol::new();
        let mut first = SymbolLocalInstance {
            project: "demo".to_string(),
            path: "/A".to_string(),
            reference: None,
            unit: Some(1),
            value: None,
            footprint: None,
            variants: BTreeMap::new(),
        };
        first.reference = Some("R1".to_string());
        let mut second = SymbolLocalInstance {
            project: "demo".to_string(),
            path: "/A".to_string(),
            reference: None,
            unit: Some(1),
            value: None,
            footprint: None,
            variants: BTreeMap::new(),
        };
        second.reference = Some("R2".to_string());
        second.unit = Some(3);

        symbol
            .instances
            .retain(|existing| existing.path != first.path);
        symbol.instances.push(first);
        symbol
            .instances
            .retain(|existing| existing.path != second.path);
        symbol.instances.push(second);

        assert_eq!(symbol.instances.len(), 1);
        assert_eq!(symbol.instances[0].reference.as_deref(), Some("R2"));
        assert_eq!(symbol.instances[0].unit, Some(3));
    }

    #[test]
    fn placed_sheets_start_with_mandatory_fields() {
        let sheet = Sheet::new();

        assert_eq!(sheet.fields_autoplaced, FieldAutoplacement::Auto);
        assert_eq!(sheet.border_width, 0.0);
        assert_eq!(sheet.border_color, None);
        assert_eq!(sheet.background_color, None);
        assert_eq!(
            sheet
                .properties
                .iter()
                .map(|property| property.kind)
                .collect::<Vec<_>>(),
            vec![PropertyKind::SheetName, PropertyKind::SheetFile]
        );
        assert!(sheet.properties.iter().all(|property| !property.show_name));
        assert_eq!(sheet.next_field_ordinal(), 42);
    }

    #[test]
    fn lib_symbol_add_draw_item_routes_by_unit_and_body_style() {
        let mut symbol = LibSymbol::new("Device:R".to_string());

        push_lib_draw_item(&mut symbol, LibDrawItem::new("text", 2, 1));

        assert_eq!(symbol.units[0].draw_items.len(), 0);
        assert_eq!(symbol.units[1].draw_items.len(), 1);
        assert_eq!(symbol.units[1].draw_item_kinds, vec!["text"]);
    }

    #[test]
    fn lib_symbol_materializes_missing_unit_and_body_style_slots() {
        let mut symbol = LibSymbol::new("Device:R".to_string());

        push_lib_draw_item(&mut symbol, LibDrawItem::new("text", 2, 2));

        assert_eq!(
            symbol
                .units
                .iter()
                .map(|unit| (unit.name.as_str(), unit.unit_number, unit.body_style))
                .collect::<Vec<_>>(),
            vec![
                ("R_1_1", 1, 1),
                ("R_1_2", 1, 2),
                ("R_2_1", 2, 1),
                ("R_2_2", 2, 2),
            ]
        );
    }

    #[test]
    fn sheet_instance_lists_preserve_duplicates() {
        let mut sheet = Sheet::new();
        let mut first = SheetLocalInstance {
            project: "demo".to_string(),
            path: "/A".to_string(),
            page: None,
            variants: BTreeMap::new(),
        };
        first.page = Some("2".to_string());
        let mut second = SheetLocalInstance {
            project: "demo".to_string(),
            path: "/A".to_string(),
            page: None,
            variants: BTreeMap::new(),
        };
        second.page = Some("3".to_string());

        sheet.instances = vec![first, second];

        assert_eq!(sheet.instances.len(), 2);
        assert_eq!(sheet.instances[0].page.as_deref(), Some("2"));
        assert_eq!(sheet.instances[1].page.as_deref(), Some("3"));
    }

    #[test]
    fn text_boxes_start_visible() {
        let text_box = TextBox::new();
        let table_cell = TableCell::new();

        assert!(text_box.visible);
        assert!(table_cell.visible);
        assert_eq!(
            text_box.stroke.as_ref().expect("text box stroke").width,
            Some(0.0)
        );
        assert_eq!(
            text_box.fill.as_ref().expect("text box fill").fill_type,
            super::FillType::None
        );
        assert_eq!(
            table_cell.stroke.as_ref().expect("table cell stroke").width,
            Some(0.0)
        );
        assert_eq!(
            table_cell.fill.as_ref().expect("table cell fill").fill_type,
            super::FillType::None
        );
        assert_eq!(table_cell.col_span, 1);
        assert_eq!(table_cell.row_span, 1);
        assert_eq!(table_cell.row, 0);
        assert_eq!(table_cell.column, 0);
    }

    #[test]
    fn tables_place_cells_on_grid_from_spans() {
        let mut table = Table::new(0.0);
        table.column_count = Some(3);

        let mut first = TableCell::new();
        first.col_span = 2;
        table.add_cell(first);

        let second = TableCell::new();
        table.add_cell(second);

        let third = TableCell::new();
        table.add_cell(third);

        assert_eq!(table.cells[0].row, 0);
        assert_eq!(table.cells[0].column, 0);
        assert_eq!(table.cells[1].row, 0);
        assert_eq!(table.cells[1].column, 2);
        assert_eq!(table.cells[2].row, 1);
        assert_eq!(table.cells[2].column, 0);
        assert!(std::ptr::eq(
            table.get_cell(0, 1).expect("spanned cell"),
            &table.cells[0]
        ));
        assert!(std::ptr::eq(
            table.get_cell(0, 2).expect("third column cell"),
            &table.cells[1]
        ));
        assert_eq!(table.row_count(), 2);
    }

    #[test]
    fn tables_place_cells_around_row_spans() {
        let mut table = Table::new(0.0);
        table.column_count = Some(2);

        let mut first = TableCell::new();
        first.row_span = 2;
        table.add_cell(first);

        let second = TableCell::new();
        table.add_cell(second);

        let third = TableCell::new();
        table.add_cell(third);

        let fourth = TableCell::new();
        table.add_cell(fourth);

        assert_eq!(table.cells[0].row, 0);
        assert_eq!(table.cells[0].column, 0);
        assert_eq!(table.cells[1].row, 0);
        assert_eq!(table.cells[1].column, 1);
        assert_eq!(table.cells[2].row, 1);
        assert_eq!(table.cells[2].column, 1);
        assert_eq!(table.cells[3].row, 2);
        assert_eq!(table.cells[3].column, 0);
        assert!(std::ptr::eq(
            table.get_cell(1, 0).expect("row-spanned cell"),
            &table.cells[0]
        ));
        assert_eq!(table.row_count(), 3);
    }

    #[test]
    fn lib_symbol_refresh_updates_draw_item_caches() {
        let mut symbol = LibSymbol::new("Device:R".to_string());

        push_lib_draw_item(
            &mut symbol,
            LibDrawItem {
                kind: "text".to_string(),
                unit_number: 1,
                body_style: 2,
                ..LibDrawItem::new("text", 1, 2)
            },
        );
        push_lib_draw_item(
            &mut symbol,
            LibDrawItem {
                kind: "arc".to_string(),
                unit_number: 1,
                body_style: 2,
                ..LibDrawItem::new("arc", 1, 2)
            },
        );

        symbol.description = symbol
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolDescription)
            .map(|property| property.value.clone())
            .filter(|value| !value.is_empty());
        for unit in &mut symbol.units {
            unit.draw_items.sort();
            unit.draw_item_kinds = unit
                .draw_items
                .iter()
                .map(|item| item.kind.clone())
                .collect();
        }

        assert_eq!(symbol.units[1].body_style, 2);
        assert_eq!(symbol.units[1].draw_item_kinds, vec!["arc", "text"]);
    }

    #[test]
    fn lib_symbol_unit_display_names_are_shared_across_body_styles() {
        let mut symbol = LibSymbol::new("Device:R".to_string());
        push_lib_draw_item(&mut symbol, LibDrawItem::new("text", 1, 2));
        for unit in &mut symbol.units {
            if unit.unit_number == 1 {
                unit.unit_name = Some("Amplifier".to_string());
            }
        }

        assert_eq!(symbol.units[0].unit_name.as_deref(), Some("Amplifier"));
        assert_eq!(symbol.units[1].unit_name.as_deref(), Some("Amplifier"));
    }

    #[test]
    fn labels_start_with_upstream_default_shapes() {
        assert_eq!(
            Label::new(LabelKind::Local, "L".to_string()).shape,
            LabelShape::Input
        );
        assert_eq!(
            Label::new(LabelKind::Global, "G".to_string()).shape,
            LabelShape::Bidirectional
        );
        assert_eq!(
            Label::new(LabelKind::Hierarchical, "H".to_string()).shape,
            LabelShape::Input
        );
        assert_eq!(
            Label::new(LabelKind::Directive, "D".to_string()).shape,
            LabelShape::Round
        );
        assert_eq!(
            Label::new(LabelKind::Directive, "D".to_string()).pin_length,
            Some(2.54)
        );
    }

    #[test]
    fn global_labels_start_with_mandatory_intersheet_refs_field() {
        let label = Label::new(LabelKind::Global, "G".to_string());

        assert_eq!(label.properties.len(), 1);
        let property = &label.properties[0];
        assert_eq!(property.kind, PropertyKind::GlobalLabelIntersheetRefs);
        assert_eq!(property.key, "Intersheet References");
        assert_eq!(property.value, "${INTERSHEET_REFS}");
        assert_eq!(property.at, Some([0.0, 0.0]));
        assert_eq!(property.angle, Some(0.0));
        assert!(!property.visible);
    }

    #[test]
    fn properties_start_with_default_geometry() {
        let property = Property::new_named(PropertyKind::User, "User", "V".to_string(), false);

        assert_eq!(property.at, Some([0.0, 0.0]));
        assert_eq!(property.angle, Some(0.0));
        assert!(property.visible);
    }

    #[test]
    fn schematic_text_starts_with_default_position_and_angle() {
        let text = Text::new(TextKind::Text, "note".to_string());

        assert_eq!(text.at, [0.0, 0.0, 0.0]);
        assert!(text.visible);
    }

    #[test]
    fn moving_schematic_text_keeps_angle_separate_from_position() {
        let mut text = Text::new(TextKind::Text, "note".to_string());

        text.set_position([10.0, 20.0]);
        assert_eq!(text.at, [10.0, 20.0, 0.0]);

        text.set_angle(90.0);
        assert_eq!(text.at, [10.0, 20.0, 90.0]);
    }

    #[test]
    fn moving_label_moves_attached_properties_without_changing_orientation() {
        let mut label = Label::new(LabelKind::Global, "G".to_string());
        label.properties[0].at = Some([1.0, 2.0]);
        label.set_position([10.0, 20.0]);

        assert_eq!(label.at, [10.0, 20.0]);
        assert_eq!(label.angle, 0.0);
        assert_eq!(label.spin, LabelSpin::Right);
        assert_eq!(label.properties[0].at, Some([11.0, 22.0]));

        label.set_angle(180.0);
        label.set_spin(LabelSpin::Left);

        assert_eq!(label.angle, 180.0);
        assert_eq!(label.spin, LabelSpin::Left);
        assert_eq!(label.properties[0].at, Some([11.0, 22.0]));
    }

    #[test]
    fn sheet_pins_start_with_default_geometry() {
        let pin = SheetPin::new("IN".to_string(), &Sheet::new());

        assert_eq!(pin.at, [0.0, 0.0]);
        assert_eq!(pin.side, SheetSide::Left);
        assert_eq!(pin.shape, SheetPinShape::Input);
        assert!(pin.visible);
    }

    #[test]
    fn symbol_pins_start_with_empty_optional_state() {
        let pin = SymbolPin::new("1".to_string());

        assert_eq!(pin.number, "1");
        assert_eq!(pin.alternate, None);
        assert_eq!(pin.uuid, None);
    }

    #[test]
    fn moving_symbol_moves_attached_properties_without_changing_angle() {
        let mut symbol = Symbol::new();
        let reference_index = symbol
            .properties
            .iter()
            .position(|property| property.kind == PropertyKind::SymbolReference)
            .expect("reference field");
        symbol.properties[reference_index].at = Some([3.0, 4.0]);

        symbol.set_position([10.0, 20.0]);
        assert_eq!(symbol.at, [10.0, 20.0]);
        assert_eq!(symbol.angle, 0.0);
        assert_eq!(symbol.properties[reference_index].at, Some([13.0, 24.0]));

        symbol.set_angle(270.0);
        assert_eq!(symbol.angle, 270.0);
        assert_eq!(symbol.properties[reference_index].at, Some([13.0, 24.0]));
    }

    #[test]
    fn vertical_sheet_pins_start_on_top_side() {
        let mut sheet = Sheet::new();
        sheet.size = [5.0, 20.0];
        let pin = SheetPin::new("IN".to_string(), &sheet);

        assert_eq!(pin.at, [0.0, 0.0]);
        assert_eq!(pin.side, SheetSide::Top);
    }

    #[test]
    fn sheet_pin_defaults_track_sheet_owner_position() {
        let mut sheet = Sheet::new();
        sheet.set_position([11.0, 22.0]);

        let pin = SheetPin::new("IN".to_string(), &sheet);

        assert_eq!(pin.at, [11.0, 0.0]);
        assert_eq!(pin.side, SheetSide::Left);

        sheet.set_size([5.0, 20.0]);
        let vertical_pin = SheetPin::new("OUT".to_string(), &sheet);

        assert_eq!(vertical_pin.at, [0.0, 22.0]);
        assert_eq!(vertical_pin.side, SheetSide::Top);
    }

    #[test]
    fn moving_sheet_moves_existing_pins_with_owner() {
        let mut sheet = Sheet::new();
        let mut pin = SheetPin::new("IN".to_string(), &sheet);
        pin.at = [0.0, 3.0];
        sheet.pins.push(pin);

        sheet.set_position([11.0, 22.0]);

        assert_eq!(sheet.pins[0].at, [11.0, 25.0]);
    }

    #[test]
    fn resizing_sheet_reconstrains_existing_pins_with_owner() {
        let mut sheet = Sheet::new();
        sheet.set_size([10.0, 20.0]);
        let mut pin = SheetPin::new("IN".to_string(), &sheet);
        pin.at = [0.0, 30.0];
        pin.side = SheetSide::Left;
        sheet.pins.push(pin);

        sheet.set_size([10.0, 15.0]);

        assert_eq!(sheet.pins[0].at, [0.0, 15.0]);
        assert_eq!(sheet.pins[0].side, SheetSide::Left);
    }

    #[test]
    fn constraining_sheet_pin_to_explicit_side_uses_sheet_edges() {
        let mut sheet = Sheet::new();
        sheet.at = [10.0, 20.0];
        sheet.size = [30.0, 40.0];

        let mut pin = SheetPin::new("IN".to_string(), &sheet);
        pin.at = [999.0, 25.0];
        pin.constrain_on_sheet_edge(sheet.at, sheet.size, true);
        pin.set_side_with_sheet_geometry(sheet.at, sheet.size, SheetSide::Right);

        assert_eq!(pin.at, [40.0, 20.0]);
        assert_eq!(pin.side, SheetSide::Right);
    }

    #[test]
    fn resizing_sheet_reconstrains_existing_pins_on_same_side() {
        let mut sheet = Sheet::new();
        sheet.size = [10.0, 20.0];
        let mut pin = SheetPin::new("IN".to_string(), &sheet);
        pin.at = [0.0, 30.0];
        pin.side = SheetSide::Left;

        pin.constrain_on_sheet_edge(sheet.at, sheet.size, false);

        assert_eq!(pin.at, [0.0, 20.0]);
        assert_eq!(pin.side, SheetSide::Left);
    }

    #[test]
    fn lib_symbols_start_with_mandatory_fields() {
        let symbol = LibSymbol::new("Device:R".to_string());

        assert_eq!(symbol.lib_id, "Device:R");
        assert_eq!(symbol.name, "R");
        assert_eq!(symbol.pin_name_offset, Some(0.508));
        assert_eq!(
            symbol
                .properties
                .iter()
                .map(|property| (property.kind, property.visible))
                .collect::<Vec<_>>(),
            vec![
                (PropertyKind::SymbolReference, true),
                (PropertyKind::SymbolValue, true),
                (PropertyKind::SymbolFootprint, false),
                (PropertyKind::SymbolDatasheet, false),
                (PropertyKind::SymbolDescription, false),
            ]
        );
        assert!(symbol.properties.iter().all(|property| !property.show_name));
        assert_eq!(symbol.next_field_ordinal(), 42);
    }

    #[test]
    fn hidden_lib_fields_advance_lib_symbol_ordinals() {
        let mut symbol = LibSymbol::new("Device:R".to_string());

        let mut field = LibDrawItem::new("field", 1, 1);
        field.field_ordinal = Some(42);
        push_lib_draw_item(&mut symbol, field);

        assert_eq!(symbol.next_field_ordinal(), 43);
    }

    #[test]
    fn placed_symbols_start_with_mandatory_field_visibility() {
        let symbol = Symbol::new();

        assert_eq!(
            symbol
                .properties
                .iter()
                .map(|property| (property.kind, property.visible))
                .collect::<Vec<_>>(),
            vec![
                (PropertyKind::SymbolReference, true),
                (PropertyKind::SymbolValue, true),
                (PropertyKind::SymbolFootprint, false),
                (PropertyKind::SymbolDatasheet, false),
                (PropertyKind::SymbolDescription, false),
            ]
        );
        assert!(symbol.properties.iter().all(|property| !property.show_name));
    }

    #[test]
    fn tables_start_with_border_and_separator_defaults() {
        let table = Table::new(0.25);

        assert!(table.border_external);
        assert!(table.border_header);
        assert_eq!(table.border_stroke.width, Some(0.25));
        assert_eq!(table.border_stroke.style, StrokeStyle::Default);
        assert!(table.separators_rows);
        assert!(table.separators_cols);
        assert_eq!(table.separators_stroke.width, Some(0.25));
        assert_eq!(table.separators_stroke.style, StrokeStyle::Default);
    }

    #[test]
    fn lib_symbol_sorts_draw_items_by_kicad_type_order() {
        let mut symbol = LibSymbol::new("Device:R".to_string());
        push_lib_draw_item(&mut symbol, LibDrawItem::new("pin", 1, 1));
        push_lib_draw_item(&mut symbol, LibDrawItem::new("text_box", 1, 1));
        push_lib_draw_item(&mut symbol, LibDrawItem::new("text", 1, 1));
        let mut field = LibDrawItem::new("field", 1, 1);
        field.field_ordinal = Some(42);
        push_lib_draw_item(&mut symbol, field);
        push_lib_draw_item(&mut symbol, LibDrawItem::new("circle", 1, 1));

        for unit in &mut symbol.units {
            unit.draw_items.sort();
            unit.draw_item_kinds = unit
                .draw_items
                .iter()
                .map(|item| item.kind.clone())
                .collect();
        }

        assert_eq!(
            symbol.units[0]
                .draw_items
                .iter()
                .map(|item| item.kind.as_str())
                .collect::<Vec<_>>(),
            vec!["circle", "field", "text", "text_box", "pin"]
        );
    }

    #[test]
    fn shapes_start_with_graphic_defaults() {
        let arc = Shape::new(ShapeKind::Arc);
        let circle = Shape::new(ShapeKind::Circle);
        let rectangle = Shape::new(ShapeKind::Rectangle);
        let bezier = Shape::new(ShapeKind::Bezier);
        let polyline = Shape::new(ShapeKind::Polyline);

        assert_eq!(arc.stroke.as_ref().expect("shape stroke").width, Some(0.0));
        assert_eq!(
            arc.fill.as_ref().expect("shape fill").fill_type,
            super::FillType::None
        );
        assert_eq!(arc.points, vec![[0.0, 0.0], [0.0, 0.0], [0.0, 0.0]]);
        assert_eq!(circle.points, vec![[0.0, 0.0]]);
        assert_eq!(circle.radius, Some(0.0));
        assert_eq!(rectangle.points, vec![[0.0, 0.0], [0.0, 0.0]]);
        assert_eq!(
            bezier.points,
            vec![[0.0, 0.0], [0.0, 0.0], [0.0, 0.0], [0.0, 0.0]]
        );
        assert!(polyline.points.is_empty());
        assert!(!arc.has_stroke);
        assert!(!arc.has_fill);
    }

    #[test]
    fn connectivity_items_start_with_constructor_defaults() {
        let junction = Junction::new();
        let no_connect = NoConnect::new();
        let bus_entry = BusEntry::new();
        let line = Line::new(LineKind::Wire);

        assert_eq!(junction.at, [0.0, 0.0]);
        assert_eq!(no_connect.at, [0.0, 0.0]);
        assert_eq!(no_connect.size, 1.2192);
        assert_eq!(bus_entry.at, [0.0, 0.0]);
        assert_eq!(bus_entry.size, [2.54, 2.54]);
        assert_eq!(
            bus_entry.stroke.as_ref().expect("bus entry stroke").width,
            Some(0.0)
        );
        assert_eq!(line.stroke.as_ref().expect("line stroke").width, Some(0.0));
        assert_eq!(line.points, vec![[0.0, 0.0], [0.0, 0.0]]);
        assert!(!bus_entry.has_stroke);
        assert!(!line.has_stroke);
    }

    #[test]
    fn library_draw_items_start_with_graphic_defaults() {
        let circle = LibDrawItem::new("circle", 1, 1);
        let arc = LibDrawItem::new("arc", 1, 1);
        let text_box = LibDrawItem::new("text_box", 1, 1);

        assert_eq!(
            circle.stroke.as_ref().expect("lib draw stroke").width,
            Some(0.0)
        );
        assert_eq!(
            circle.fill.as_ref().expect("lib draw fill").fill_type,
            super::FillType::None
        );
        assert_eq!(circle.points, vec![[0.0, 0.0]]);
        assert_eq!(circle.radius, Some(1.0));
        assert_eq!(arc.points, vec![[1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        assert_eq!(arc.arc_center, Some([0.0, 0.0]));
        assert_eq!(arc.arc_start_angle, Some(0.0));
        assert_eq!(arc.arc_end_angle, Some(90.0));
        assert_eq!(text_box.angle, Some(0.0));
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

impl SymbolPin {
    pub fn new(number: String) -> Self {
        Self {
            number,
            alternate: None,
            uuid: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub id: Option<i32>,
    pub ordinal: i32,
    pub key: String,
    pub value: String,
    pub base_value: Option<String>,
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
        let base_value =
            matches!(kind, PropertyKind::GlobalLabelIntersheetRefs).then(|| value.clone());
        Self {
            id: kind.default_field_id(),
            ordinal: kind.default_field_id().unwrap_or(0),
            key: kind.canonical_key().to_string(),
            value,
            base_value,
            kind,
            is_private: false,
            at: Some([0.0, 0.0]),
            angle: Some(0.0),
            visible: true,
            show_name: false,
            can_autoplace: true,
            has_effects: false,
            effects: None,
        }
    }

    pub fn new_named(kind: PropertyKind, name: &str, value: String, is_private: bool) -> Self {
        let base_value =
            matches!(kind, PropertyKind::GlobalLabelIntersheetRefs).then(|| value.clone());
        Self {
            id: kind.default_field_id(),
            ordinal: kind.default_field_id().unwrap_or(0),
            key: match kind {
                PropertyKind::User | PropertyKind::SheetUser => name.to_string(),
                _ => kind.canonical_key().to_string(),
            },
            value,
            base_value,
            kind,
            is_private,
            at: Some([0.0, 0.0]),
            angle: Some(0.0),
            visible: true,
            show_name: false,
            can_autoplace: true,
            has_effects: false,
            effects: None,
        }
    }

    pub fn sort_ordinal(&self) -> i32 {
        if self.kind.is_mandatory() {
            self.id.unwrap_or(self.ordinal)
        } else {
            self.ordinal
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
    SymbolDescription,
    SheetName,
    SheetFile,
    SheetUser,
    GlobalLabelIntersheetRefs,
}

impl PropertyKind {
    pub const SYMBOL_MANDATORY_FIELDS: [PropertyKind; 5] = [
        PropertyKind::SymbolReference,
        PropertyKind::SymbolValue,
        PropertyKind::SymbolFootprint,
        PropertyKind::SymbolDatasheet,
        PropertyKind::SymbolDescription,
    ];

    pub const SHEET_MANDATORY_FIELDS: [PropertyKind; 2] =
        [PropertyKind::SheetName, PropertyKind::SheetFile];

    pub const GLOBAL_LABEL_MANDATORY_FIELDS: [PropertyKind; 1] =
        [PropertyKind::GlobalLabelIntersheetRefs];

    pub fn is_user_field(self) -> bool {
        matches!(self, PropertyKind::User | PropertyKind::SheetUser)
    }

    pub fn is_mandatory(self) -> bool {
        !self.is_user_field()
    }

    pub fn canonical_key(self) -> &'static str {
        match self {
            PropertyKind::User => "",
            PropertyKind::SymbolReference => "Reference",
            PropertyKind::SymbolValue => "Value",
            PropertyKind::SymbolFootprint => "Footprint",
            PropertyKind::SymbolDatasheet => "Datasheet",
            PropertyKind::SymbolDescription => "Description",
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
            PropertyKind::SymbolDescription => Some(5),
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
    pub at: [f64; 2],
    pub side: SheetSide,
    pub visible: bool,
    pub has_effects: bool,
    pub effects: Option<TextEffects>,
    pub uuid: Option<String>,
}

impl SheetPin {
    pub fn new(name: String, sheet: &Sheet) -> Self {
        let mut pin = Self {
            name,
            shape: SheetPinShape::Input,
            at: [0.0, 0.0],
            side: if sheet.is_vertical_orientation() {
                SheetSide::Top
            } else {
                SheetSide::Left
            },
            visible: true,
            has_effects: false,
            effects: None,
            uuid: None,
        };
        pin.set_side_with_sheet_geometry(sheet.at, sheet.size, pin.side);
        pin
    }

    pub fn set_side_with_sheet_geometry(
        &mut self,
        sheet_at: [f64; 2],
        sheet_size: [f64; 2],
        side: SheetSide,
    ) {
        self.side = side;
        match side {
            SheetSide::Left => {
                self.at[0] = sheet_at[0];
            }
            SheetSide::Right => {
                self.at[0] = sheet_at[0] + sheet_size[0];
            }
            SheetSide::Top => {
                self.at[1] = sheet_at[1];
            }
            SheetSide::Bottom => {
                self.at[1] = sheet_at[1] + sheet_size[1];
            }
        }
    }

    pub fn constrain_on_sheet_edge(
        &mut self,
        sheet_at: [f64; 2],
        sheet_size: [f64; 2],
        allow_edge_switch: bool,
    ) {
        let left = sheet_at[0];
        let right = sheet_at[0] + sheet_size[0];
        let top = sheet_at[1];
        let bottom = sheet_at[1] + sheet_size[1];

        if allow_edge_switch {
            let distances = [
                (0usize, (self.at[1] - top).abs()),
                (1usize, (self.at[0] - right).abs()),
                (2usize, (self.at[1] - bottom).abs()),
                (3usize, (self.at[0] - left).abs()),
            ];
            let nearest = distances
                .into_iter()
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(segment, _)| segment)
                .unwrap_or(3);
            let side = match nearest {
                0 => SheetSide::Top,
                1 => SheetSide::Right,
                2 => SheetSide::Bottom,
                _ => SheetSide::Left,
            };
            self.set_side_with_sheet_geometry(sheet_at, sheet_size, side);
        } else {
            self.set_side_with_sheet_geometry(sheet_at, sheet_size, self.side);
        }

        match self.side {
            SheetSide::Left | SheetSide::Right => {
                self.at[1] = self.at[1].clamp(top, bottom);
            }
            SheetSide::Top | SheetSide::Bottom => {
                self.at[0] = self.at[0].clamp(left, right);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetPinShape {
    Input,
    Output,
    Bidirectional,
    TriState,
    Unspecified,
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

#[derive(Debug, Clone, PartialEq)]
pub struct SymbolInstance {
    pub path: String,
    pub reference: Option<String>,
    pub unit: Option<i32>,
    pub value: Option<String>,
    pub footprint: Option<String>,
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

#[derive(Debug, Clone, PartialEq)]
pub struct SheetLocalInstance {
    pub project: String,
    pub path: String,
    pub page: Option<String>,
    pub variants: BTreeMap<String, ItemVariant>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SymbolLocalInstance {
    pub project: String,
    pub path: String,
    pub reference: Option<String>,
    pub unit: Option<i32>,
    pub value: Option<String>,
    pub footprint: Option<String>,
    pub variants: BTreeMap<String, ItemVariant>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BusAlias {
    pub name: String,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddedFile {
    pub name: Option<String>,
    pub checksum: Option<String>,
    pub file_type: Option<EmbeddedFileType>,
    pub data: Option<String>,
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

#[derive(Debug, Clone, PartialEq)]
pub struct SheetReference {
    pub sheet_uuid: Option<String>,
    pub sheet_name: Option<String>,
    pub filename: String,
    pub resolved_path: PathBuf,
}
