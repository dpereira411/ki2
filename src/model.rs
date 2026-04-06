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
    pub fn set_comment(&mut self, comment_number: usize, value: String) {
        self.comments[comment_number - 1] = Some(value);
    }

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

    fn materialize_unit_counts(&mut self, unit_count: i32, body_style_count: i32) {
        let unit_count = unit_count.max(1);
        let body_style_count = body_style_count.max(1);

        for unit_number in 1..=unit_count {
            for body_style in 1..=body_style_count {
                if self.units.iter().any(|existing| {
                    existing.unit_number == unit_number && existing.body_style == body_style
                }) {
                    continue;
                }

                let unit_name = self
                    .units
                    .iter()
                    .find(|existing| existing.unit_number == unit_number)
                    .and_then(|existing| existing.unit_name.clone());

                self.units.push(LibSymbolUnit {
                    name: format!("{}_{}_{}", self.name, unit_number, body_style),
                    unit_number,
                    body_style,
                    unit_name,
                    draw_item_kinds: Vec::new(),
                    draw_items: Vec::new(),
                });
            }
        }

        self.units
            .sort_by_key(|unit| (unit.unit_number, unit.body_style));
    }

    pub fn ensure_unit_index(&mut self, name: String, unit_number: i32, body_style: i32) -> usize {
        self.materialize_unit_counts(unit_number, body_style);

        let index = self
            .units
            .iter()
            .position(|existing| {
                existing.unit_number == unit_number && existing.body_style == body_style
            })
            .expect("materialized lib symbol unit must exist");
        self.units[index].name = name;
        index
    }

    pub fn add_draw_item(&mut self, item: LibDrawItem) {
        self.materialize_unit_counts(item.unit_number, item.body_style);

        let index = self
            .units
            .iter()
            .position(|existing| {
                existing.unit_number == item.unit_number && existing.body_style == item.body_style
            })
            .expect("materialized lib symbol unit must exist");

        self.units[index].draw_item_kinds.push(item.kind.clone());
        self.units[index].draw_items.push(item);
    }

    pub fn sort_draw_items(&mut self) {
        for unit in &mut self.units {
            unit.draw_items.sort();
            unit.draw_item_kinds = unit
                .draw_items
                .iter()
                .map(|item| item.kind.clone())
                .collect();
        }
    }

    pub fn has_legacy_alternate_body_style(&self) -> bool {
        self.units.iter().any(|unit| unit.body_style > 1)
    }

    pub fn refresh_library_tree_caches(&mut self) {
        self.description = self
            .properties
            .iter()
            .find(|property| property.kind == PropertyKind::SymbolDescription)
            .map(|property| property.value.clone())
            .filter(|value| !value.is_empty());
        self.sort_draw_items();
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

    pub fn set_position(&mut self, at: [f64; 2], angle: f64, spin: LabelSpin) {
        let delta = [at[0] - self.at[0], at[1] - self.at[1]];
        self.at = at;
        self.angle = angle;
        self.spin = spin;

        for property in &mut self.properties {
            if let Some(property_at) = property.at.as_mut() {
                property_at[0] += delta[0];
                property_at[1] += delta[1];
            }
        }
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

    pub fn set_position(&mut self, at: [f64; 2], angle: f64) {
        let delta = [at[0] - self.at[0], at[1] - self.at[1]];
        self.at = at;
        self.angle = angle;

        for property in &mut self.properties {
            if let Some(property_at) = property.at.as_mut() {
                property_at[0] += delta[0];
                property_at[1] += delta[1];
            }
        }
    }

    pub fn add_pin(&mut self, pin: SymbolPin) {
        self.pins.push(pin);
    }

    pub fn add_hierarchical_reference(&mut self, mut instance: SymbolLocalInstance) {
        if instance.unit.is_none() {
            instance.unit = Some(1);
        }

        self.instances
            .retain(|existing| existing.path != instance.path);

        self.instances.push(instance);
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

    pub fn filename(&self) -> Option<&str> {
        self.properties
            .iter()
            .find(|property| property.kind == PropertyKind::SheetFile)
            .map(|property| property.value.as_str())
    }

    pub fn add_pin(&mut self, pin: SheetPin) {
        self.pins.push(pin);
    }

    pub fn set_instances(&mut self, instances: Vec<SheetLocalInstance>) {
        self.instances = instances;
    }

    pub fn is_vertical_orientation(&self) -> bool {
        self.size[1] > self.size[0]
    }

    pub fn next_field_ordinal(&self) -> i32 {
        self.properties.iter().fold(42, |ordinal, property| {
            ordinal.max(property.sort_ordinal() + 1)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BusEntry, FieldAutoplacement, Junction, Label, LabelKind, LabelShape, LibDrawItem,
        LibSymbol, Line, LineKind, NoConnect, Property, PropertyKind, Shape, ShapeKind, Sheet,
        SheetLocalInstance, SheetPin, SheetPinShape, SheetSide, StrokeStyle, Symbol,
        SymbolLocalInstance, SymbolPin, Table, TableCell, Text, TextBox, TextKind,
    };

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
    fn hierarchical_references_do_not_seed_live_symbol_state_from_first_instance() {
        let mut symbol = Symbol::new();
        let mut instance = SymbolLocalInstance::new("demo".to_string(), "/A".to_string());
        instance.reference = Some("R7".to_string());
        instance.unit = Some(2);

        symbol.add_hierarchical_reference(instance);

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
        let mut first = SymbolLocalInstance::new("demo".to_string(), "/A".to_string());
        first.reference = Some("R1".to_string());
        let mut second = SymbolLocalInstance::new("demo".to_string(), "/A".to_string());
        second.reference = Some("R2".to_string());
        second.unit = Some(3);

        symbol.add_hierarchical_reference(first);
        symbol.add_hierarchical_reference(second);

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
        symbol.ensure_unit_index("R_2_1".to_string(), 2, 1);

        symbol.add_draw_item(LibDrawItem::new("text", 2, 1));

        assert_eq!(symbol.units[0].draw_items.len(), 0);
        assert_eq!(symbol.units[1].draw_items.len(), 1);
        assert_eq!(symbol.units[1].draw_item_kinds, vec!["text"]);
    }

    #[test]
    fn lib_symbol_materializes_missing_unit_and_body_style_slots() {
        let mut symbol = LibSymbol::new("Device:R".to_string());

        symbol.ensure_unit_index("R_2_2".to_string(), 2, 2);

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
    fn sheet_set_instances_preserves_duplicates() {
        let mut sheet = Sheet::new();
        let mut first = SheetLocalInstance::new("demo".to_string(), "/A".to_string());
        first.page = Some("2".to_string());
        let mut second = SheetLocalInstance::new("demo".to_string(), "/A".to_string());
        second.page = Some("3".to_string());

        sheet.set_instances(vec![first, second]);

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
        table.set_column_count(3);

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
        table.set_column_count(2);

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

        symbol.add_draw_item(LibDrawItem {
            kind: "text".to_string(),
            unit_number: 1,
            body_style: 2,
            ..LibDrawItem::new("text", 1, 2)
        });
        symbol.add_draw_item(LibDrawItem {
            kind: "arc".to_string(),
            unit_number: 1,
            body_style: 2,
            ..LibDrawItem::new("arc", 1, 2)
        });

        symbol.refresh_library_tree_caches();

        assert_eq!(symbol.units[1].body_style, 2);
        assert_eq!(symbol.units[1].draw_item_kinds, vec!["arc", "text"]);
    }

    #[test]
    fn lib_symbol_unit_display_names_are_shared_across_body_styles() {
        let mut symbol = LibSymbol::new("Device:R".to_string());
        symbol.ensure_unit_index("R_1_2".to_string(), 1, 2);
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
        sheet.add_pin(pin);

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
        sheet.add_pin(pin);

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
        symbol.add_draw_item(field);

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
        symbol.add_draw_item(LibDrawItem::new("pin", 1, 1));
        symbol.add_draw_item(LibDrawItem::new("text_box", 1, 1));
        symbol.add_draw_item(LibDrawItem::new("text", 1, 1));
        let mut field = LibDrawItem::new("field", 1, 1);
        field.field_ordinal = Some(42);
        symbol.add_draw_item(field);
        symbol.add_draw_item(LibDrawItem::new("circle", 1, 1));

        symbol.sort_draw_items();

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
            ordinal: kind.default_field_id().unwrap_or(0),
            key: kind.canonical_key().to_string(),
            value,
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
        Self {
            id: kind.default_field_id(),
            ordinal: kind.default_field_id().unwrap_or(0),
            key: match kind {
                PropertyKind::User | PropertyKind::SheetUser => name.to_string(),
                _ => kind.canonical_key().to_string(),
            },
            value,
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
            unit: Some(1),
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
