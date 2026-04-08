use std::collections::BTreeMap;
use std::path::Path;

use crate::connectivity::collect_reduced_project_net_map;
use crate::core::SchematicProject;
use crate::loader::{
    resolve_schematic_text_var, resolve_sheet_text_var, resolve_text_variables,
    resolved_label_text_property_value_without_connectivity, resolved_sheet_text_state,
    resolved_symbol_text_property_value, resolved_symbol_text_state,
};
use crate::model::{LabelKind, Property, SchItem, ShapeKind, Symbol};
use time::{OffsetDateTime, macros::format_description};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetlistComponent {
    pub reference: String,
    pub unit_number: i32,
    pub value: String,
    pub footprint: String,
    pub datasheet: String,
    pub description: String,
    pub lib: String,
    pub part: String,
    pub path_names: String,
    pub path: String,
    pub tstamps: Vec<String>,
    pub units: Vec<NetlistComponentUnit>,
    pub duplicate_pin_numbers_are_jumpers: bool,
    pub jumper_pin_groups: Vec<Vec<String>>,
    pub component_classes: Vec<String>,
    pub variants: Vec<NetlistComponentVariant>,
    pub metadata_properties: Vec<(String, Option<String>)>,
    pub fields: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetlistComponentUnit {
    pub name: String,
    pub pins: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetlistComponentVariant {
    pub name: String,
    pub properties: Vec<(String, String)>,
    pub fields: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetlistLibPart {
    pub lib: String,
    pub part: String,
    pub description: String,
    pub docs: String,
    pub fields: Vec<(String, String)>,
    pub footprints: Vec<String>,
    pub pins: Vec<NetlistLibPartPin>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetlistLibPartPin {
    pub number: String,
    pub name: String,
    pub electrical_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NetlistNode {
    pub reference: String,
    pub pin: String,
    pub pinfunction: Option<String>,
    pub pintype: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetlistNet {
    pub code: usize,
    pub name: String,
    pub class: String,
    pub nodes: Vec<NetlistNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetlistSheetDesign {
    pub number: usize,
    pub name: String,
    pub tstamps: String,
    pub title: String,
    pub company: String,
    pub revision: String,
    pub date: String,
    pub source: String,
    pub comments: Vec<(usize, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetlistDesign {
    pub source: String,
    pub date: String,
    pub tool: String,
    pub text_vars: Vec<(String, String)>,
    pub sheets: Vec<NetlistSheetDesign>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetlistGroup {
    pub name: String,
    pub uuid: String,
    pub lib_id: String,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetlistVariant {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetlistLibrary {
    pub logical: String,
    pub uri: Option<String>,
}

struct OrderedNetlistSymbol<'a> {
    sheet_path: &'a crate::loader::LoadedSheetPath,
    symbol: &'a Symbol,
    extra_symbols: Vec<&'a Symbol>,
}

fn resolved_property_value(properties: &[Property], token: &str) -> Option<String> {
    let canonical = token.to_ascii_uppercase();
    properties
        .iter()
        .find(|property| {
            let property_key = if property.kind.is_mandatory() {
                property.kind.canonical_key().to_ascii_uppercase()
            } else {
                property.key.to_ascii_uppercase()
            };
            property_key == canonical
        })
        .map(|property| property.value.clone())
}

fn point_in_polygon(point: [f64; 2], polygon: &[[f64; 2]]) -> bool {
    if polygon.len() < 3 {
        return false;
    }

    let mut inside = false;

    for (index, start) in polygon.iter().enumerate() {
        let end = polygon[(index + 1) % polygon.len()];

        let intersects = ((start[1] > point[1]) != (end[1] > point[1]))
            && (point[0]
                < ((end[0] - start[0]) * (point[1] - start[1]) / (end[1] - start[1])) + start[0]);

        if intersects {
            inside = !inside;
        }
    }

    inside
}

fn collect_rule_area_component_classes(
    project: &SchematicProject,
    sheet_path: &crate::loader::LoadedSheetPath,
    symbol: &Symbol,
) -> Vec<String> {
    let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
        return Vec::new();
    };

    schematic
        .screen
        .items
        .iter()
        .filter_map(|item| match item {
            SchItem::Shape(shape)
                if shape.kind == ShapeKind::RuleArea
                    && point_in_polygon(symbol.at, &shape.points) =>
            {
                Some(shape)
            }
            _ => None,
        })
        .flat_map(|rule_area| {
            schematic
                .screen
                .items
                .iter()
                .filter_map(move |item| match item {
                    SchItem::Label(label)
                        if label.kind == LabelKind::Directive
                            && point_in_polygon(label.at, &rule_area.points) =>
                    {
                        Some(label)
                    }
                    _ => None,
                })
        })
        .filter_map(|directive| {
            resolved_label_text_property_value_without_connectivity(
                &project.schematics,
                &project.sheet_paths,
                sheet_path,
                project.project.as_ref(),
                project.current_variant(),
                directive,
                "Component Class",
            )
        })
        .filter(|value| !value.is_empty())
        .collect()
}

// Upstream parity: reduced local analogue for `NETLIST_EXPORTER_XML::getComponentClassNamesForAllSymbolUnits()`.
// This is not a 1:1 KiCad component-class owner because the Rust tree still lacks KiCad's cached
// rule-area child-item membership and sheet-level component-class map, but it preserves the
// exported symbol-field and enclosing-rule-area directive-field collection, merges across
// same-reference units, and sorts/deduplicates class names before `<component_classes>` export.
// Remaining divergence is the still-missing child-pin/sheet-level rule-area coverage.
fn collect_component_class_names_for_all_symbol_units(
    project: &SchematicProject,
    symbol: &Symbol,
    symbol_sheet: &crate::loader::LoadedSheetPath,
    reference: &str,
) -> Vec<String> {
    let mut class_names = Vec::new();

    for candidate_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&candidate_path.schematic_path) else {
            continue;
        };

        for item in &schematic.screen.items {
            let SchItem::Symbol(candidate_symbol) = item else {
                continue;
            };

            let Some(candidate_reference) = resolved_symbol_text_property_value(
                &project.schematics,
                candidate_path,
                project.project.as_ref(),
                project.current_variant(),
                candidate_symbol,
                "Reference",
            ) else {
                continue;
            };

            if !candidate_reference.eq_ignore_ascii_case(reference) {
                continue;
            }

            if candidate_path.instance_path == symbol_sheet.instance_path
                && candidate_symbol.uuid == symbol.uuid
            {
                // Keep the same control-flow split as upstream: collect the primary symbol once,
                // then merge any additional same-reference units/screens onto the same class set.
            }

            if let Some(value) = resolved_symbol_text_property_value(
                &project.schematics,
                candidate_path,
                project.project.as_ref(),
                project.current_variant(),
                candidate_symbol,
                "Component Class",
            ) {
                if !value.is_empty() {
                    class_names.push(value);
                }
            }

            class_names.extend(collect_rule_area_component_classes(
                project,
                candidate_path,
                candidate_symbol,
            ));
        }
    }

    class_names.sort();
    class_names.dedup();
    class_names
}

fn human_component_sheet_path(
    project: &SchematicProject,
    sheet_path: &crate::loader::LoadedSheetPath,
) -> String {
    if sheet_path.instance_path.is_empty() {
        return "/".to_string();
    }

    let mut names = project
        .ancestor_sheet_paths(&sheet_path.instance_path)
        .into_iter()
        .rev()
        .filter_map(|path| path.sheet_name.as_deref())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();

    if let Some(name) = sheet_path
        .sheet_name
        .as_deref()
        .filter(|name| !name.is_empty())
    {
        names.push(name.to_string());
    }

    if names.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", names.join("/"))
    }
}

fn resolved_sheet_property_name(property: &Property) -> String {
    if property.kind.is_mandatory() {
        property.kind.canonical_key().to_string()
    } else {
        property.key.clone()
    }
}

// Upstream parity: reduced local analogue for the parent-sheet property loop inside
// `NETLIST_EXPORTER_XML::makeSymbols()`. This is not a 1:1 KiCad field resolver because the Rust
// tree still resolves from loaded sheet snapshots instead of live `SCH_SHEET_PATH` owners, but it
// is needed so component export can carry parent-sheet fields through the same occurrence/variant
// and sheet shown-text path instead of dropping them entirely.
fn collect_parent_sheet_properties(
    project: &SchematicProject,
    sheet_path: &crate::loader::LoadedSheetPath,
) -> Vec<(String, String)> {
    let Some(state) = resolved_sheet_text_state(
        &project.schematics,
        &project.sheet_paths,
        sheet_path,
        project.current_variant(),
    ) else {
        return Vec::new();
    };

    let mut properties = state
        .properties
        .iter()
        .map(|property| {
            (
                resolved_sheet_property_name(property),
                resolve_text_variables(
                    &property.value,
                    &|nested| {
                        resolve_sheet_text_var(
                            &project.schematics,
                            &project.sheet_paths,
                            sheet_path,
                            project.project.as_ref(),
                            project.current_variant(),
                            nested,
                            0,
                        )
                        .or_else(|| {
                            resolve_schematic_text_var(
                                &project.schematics,
                                sheet_path,
                                project.project.as_ref(),
                                project.current_variant(),
                                nested,
                            )
                        })
                    },
                    0,
                ),
            )
        })
        .collect::<Vec<_>>();
    properties.sort_by(|(lhs, _): &(String, String), (rhs, _): &(String, String)| lhs.cmp(rhs));
    properties
}

fn parse_alphanumeric_pin(pin: &str) -> (String, Option<i64>) {
    let Some(num_start) = pin
        .rfind(|ch: char| !ch.is_ascii_digit())
        .map(|index| index + 1)
    else {
        return (
            String::new(),
            (!pin.is_empty()).then(|| pin.parse::<i64>().ok()).flatten(),
        );
    };

    if num_start >= pin.len() {
        return (String::new(), None);
    }

    let prefix = pin[..num_start].to_string();
    let numeric = pin[num_start..].parse::<i64>().ok();
    (prefix, numeric)
}

fn expand_stacked_pin_notation(pin: &str) -> (Vec<String>, bool) {
    let has_open_bracket = pin.contains('[');
    let has_close_bracket = pin.contains(']');

    if has_open_bracket || has_close_bracket {
        if !pin.starts_with('[') || !pin.ends_with(']') {
            return (vec![pin.to_string()], false);
        }
    }

    if !pin.starts_with('[') || !pin.ends_with(']') {
        return (vec![pin.to_string()], true);
    }

    let inner = &pin[1..pin.len() - 1];
    let mut expanded = Vec::new();
    let mut valid = true;

    for part in inner.split(',') {
        let part = part.trim();

        if part.is_empty() {
            continue;
        }

        if let Some(dash_pos) = part.find('-') {
            let start_txt = part[..dash_pos].trim();
            let end_txt = part[dash_pos + 1..].trim();
            let (start_prefix, start_val) = parse_alphanumeric_pin(start_txt);
            let (end_prefix, end_val) = parse_alphanumeric_pin(end_txt);

            match (start_val, end_val) {
                (Some(start_val), Some(end_val))
                    if start_prefix == end_prefix && start_val <= end_val =>
                {
                    for value in start_val..=end_val {
                        if start_prefix.is_empty() {
                            expanded.push(value.to_string());
                        } else {
                            expanded.push(format!("{start_prefix}{value}"));
                        }
                    }
                }
                _ => {
                    valid = false;
                    expanded.clear();
                    expanded.push(pin.to_string());
                    return (expanded, valid);
                }
            }
        } else {
            expanded.push(part.to_string());
        }
    }

    if expanded.is_empty() {
        return (vec![pin.to_string()], false);
    }

    (expanded, valid)
}

fn natural_compare_segment(a: &str, b: &str) -> std::cmp::Ordering {
    let a_trimmed = a.trim_start_matches('0');
    let b_trimmed = b.trim_start_matches('0');
    let a_normalized = if a_trimmed.is_empty() { "0" } else { a_trimmed };
    let b_normalized = if b_trimmed.is_empty() { "0" } else { b_trimmed };

    a_normalized
        .len()
        .cmp(&b_normalized.len())
        .then_with(|| a_normalized.cmp(b_normalized))
        .then_with(|| a.len().cmp(&b.len()))
}

fn str_num_cmp(a: &str, b: &str, ignore_case: bool) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    if a == b {
        return Ordering::Equal;
    }

    let mut a_chars = a.chars().peekable();
    let mut b_chars = b.chars().peekable();

    loop {
        match (a_chars.peek().copied(), b_chars.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(a_ch), Some(b_ch)) if a_ch.is_ascii_digit() && b_ch.is_ascii_digit() => {
                let mut a_digits = String::new();
                let mut b_digits = String::new();

                while let Some(ch) = a_chars.peek().copied() {
                    if !ch.is_ascii_digit() {
                        break;
                    }

                    a_digits.push(ch);
                    a_chars.next();
                }

                while let Some(ch) = b_chars.peek().copied() {
                    if !ch.is_ascii_digit() {
                        break;
                    }

                    b_digits.push(ch);
                    b_chars.next();
                }

                let ordering = natural_compare_segment(&a_digits, &b_digits);

                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            (Some(a_ch), Some(b_ch)) => {
                let a_cmp = if ignore_case {
                    a_ch.to_ascii_lowercase()
                } else {
                    a_ch
                };
                let b_cmp = if ignore_case {
                    b_ch.to_ascii_lowercase()
                } else {
                    b_ch
                };
                let ordering = a_cmp.cmp(&b_cmp);
                a_chars.next();
                b_chars.next();

                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
        }
    }
}

// Upstream parity: reduced local helper for the `findNextSymbol()` + `makeSymbols()` ordered
// symbol walk in KiCad's common netlist exporter base. This is not a 1:1 exporter-base iterator
// because the Rust tree still lacks live `SCH_SYMBOL*` / libpart pointer ownership, but it now
// picks one primary symbol per same-reference group before component/libpart export, keeps the
// lowest-UUID symbol as the primary within each ordered per-sheet reference bucket, and skips
// later multi-unit duplicates through one shared exporter-base-style pass instead of repairing that
// ownership after raw component collection. Remaining divergence is the still-missing shared
// libpart usage cache and fuller exporter-base symbol filters beyond the exercised XML/KiCad path.
fn collect_ordered_netlist_symbols<'a>(
    project: &'a SchematicProject,
    for_board: bool,
) -> Vec<OrderedNetlistSymbol<'a>> {
    let mut ordered = Vec::new();
    let mut seen_multi_unit_refs = BTreeMap::<String, ()>::new();

    for sheet_path in &project.sheet_paths {
        if for_board
            && !resolved_sheet_text_state(
                &project.schematics,
                &project.sheet_paths,
                sheet_path,
                project.current_variant(),
            )
            .map(|state| state.on_board)
            .unwrap_or(true)
        {
            continue;
        }

        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };

        let mut primary_symbols =
            BTreeMap::<String, (String, String, &'a Symbol, Vec<&'a Symbol>)>::new();

        for item in &schematic.screen.items {
            let SchItem::Symbol(symbol) = item else {
                continue;
            };

            if !symbol.in_netlist {
                continue;
            }

            if for_board && !symbol.on_board {
                continue;
            }

            if symbol.lib_symbol.is_none() {
                continue;
            }

            let Some(reference) = resolved_symbol_text_property_value(
                &project.schematics,
                sheet_path,
                project.project.as_ref(),
                project.current_variant(),
                symbol,
                "Reference",
            ) else {
                continue;
            };

            if reference.starts_with('#') {
                continue;
            }

            let key = reference.to_ascii_uppercase();
            let uuid = symbol.uuid.clone().unwrap_or_default();

            match primary_symbols.get_mut(&key) {
                Some((_, current_uuid, current_symbol, extra_symbols)) => {
                    if uuid < *current_uuid {
                        extra_symbols.push(*current_symbol);
                        *current_uuid = uuid.clone();
                        *current_symbol = symbol;
                    } else {
                        extra_symbols.push(symbol);
                    }
                }
                None => {
                    primary_symbols.insert(key, (reference, uuid, symbol, Vec::new()));
                }
            }
        }

        let mut primaries = primary_symbols.into_values().collect::<Vec<_>>();
        primaries.sort_by(|(lhs_ref, ..), (rhs_ref, ..)| str_num_cmp(lhs_ref, rhs_ref, true));

        for (reference, _uuid, symbol, extra_symbols) in primaries {
            let unit_count = symbol
                .lib_symbol
                .as_ref()
                .map(|lib_symbol| lib_symbol.units.len())
                .unwrap_or(0);

            if unit_count > 1 && seen_multi_unit_refs.contains_key(&reference.to_ascii_uppercase())
            {
                continue;
            }

            if unit_count > 1 {
                seen_multi_unit_refs.insert(reference.to_ascii_uppercase(), ());
            }

            ordered.push(OrderedNetlistSymbol {
                sheet_path,
                symbol,
                extra_symbols,
            });
        }
    }

    ordered
}

// Upstream parity: reduced local analogue for the symbol iteration portion of
// `NETLIST_EXPORTER_XML::makeSymbols()`. This is not a 1:1 exporter-base walk because the Rust
// tree still omits the full libpart pointer cache and full connection-graph-backed symbol state,
// but it now shares one exporter-base-style ordered symbol pass, chooses same-reference primaries
// before component construction, and preserves the current occurrence-aware
// reference/value/footprint exposure and `LIB_ID` split needed by the live netlist CLI slice.
// Remaining divergence is the fuller KiCad variant/libpart walk and broader sheet/symbol filter
// ownership outside the current loaded sheet-state carrier.
pub fn collect_xml_components(
    project: &SchematicProject,
    for_board: bool,
) -> Vec<NetlistComponent> {
    let mut components = Vec::new();

    for ordered_symbol in collect_ordered_netlist_symbols(project, for_board) {
        if let Some(component) = symbol_to_xml_component(
            project,
            ordered_symbol.sheet_path,
            ordered_symbol.symbol,
            &ordered_symbol.extra_symbols,
        ) {
            components.push(component);
        }
    }

    components
}

// Upstream parity: reduced local analogue for `NETLIST_EXPORTER_XML::makeLibParts()`. This is not
// a 1:1 KiCad libpart exporter because the Rust tree still sources libparts only from the
// schematic-linked lib-symbol snapshots instead of the full library adapter stack, but it
// now shares the same exporter-base-style symbol walk used by component export before collecting
// unique libparts. Remaining divergence is the still-missing full library adapter stack beyond the
// current reduced field/docs/footprint and duplicate-pin-number export slice.
pub fn collect_xml_libparts(project: &SchematicProject) -> Vec<NetlistLibPart> {
    let mut libparts = BTreeMap::<String, NetlistLibPart>::new();

    for ordered_symbol in collect_ordered_netlist_symbols(project, false) {
        let symbol = ordered_symbol.symbol;
        let Some(lib_symbol) = symbol.lib_symbol.as_ref() else {
            continue;
        };

        let key = symbol.lib_id.clone();

        libparts
            .entry(key)
            .or_insert_with(|| lib_symbol_to_xml_libpart(symbol.lib_id.as_str(), lib_symbol));
    }

    libparts.into_values().collect()
}

// Upstream parity: reduced local analogue for `NETLIST_EXPORTER_XML::makeListOfNets()`. This is
// not a 1:1 KiCad net exporter because the Rust tree still derives `GetNetMap()` entries from
// reduced shared subgraphs instead of full `CONNECTION_GRAPH` objects, but it now consumes one
// shared reduced `GetNetMap()` view instead of re-grouping subgraphs inside the exporter itself.
// It also mirrors the exercised write-time filtering branch where power/virtual `#...` refs are
// skipped after net grouping, which can drop whole nets without renumbering the remaining emitted
// codes. The remaining divergence is the fuller KiCad subgraph object model and graph-owned
// netcode/name caches.
pub fn collect_xml_nets(project: &SchematicProject, for_board: bool) -> Vec<NetlistNet> {
    collect_reduced_project_net_map(project, for_board)
        .into_iter()
        .filter_map(|net| {
            let all_net_pins_stacked = !net.base_pins.is_empty()
                && net
                    .base_pins
                    .iter()
                    .all(|base_pin| *base_pin == net.base_pins[0]);
            let mut nodes = BTreeMap::<(String, String), NetlistNode>::new();

            for node in net.nodes {
                if node.reference.starts_with('#') {
                    continue;
                }

                nodes
                    .entry((node.reference.clone(), node.pin.clone()))
                    .or_insert(NetlistNode {
                        reference: node.reference,
                        pin: node.pin,
                        pinfunction: node.pinfunction,
                        pintype: node.pintype,
                    });
            }
            let mut nodes = nodes.into_values().collect::<Vec<_>>();

            if nodes.is_empty() {
                return None;
            }

            if net.has_no_connect && (nodes.len() == 1 || all_net_pins_stacked) {
                for node in &mut nodes {
                    node.pintype.push_str("+no_connect");
                }
            }

            Some(NetlistNet {
                code: net.code,
                name: net.name,
                class: net.class,
                nodes,
            })
        })
        .collect()
}

// Upstream parity: reduced local helper for `NETLIST_EXPORTER_XML::addSymbolFields()` /
// `makeSymbols()`. This is not a 1:1 KiCad field resolver because the Rust tree still lacks the
// full libpart/groups/variants export stack, but it keeps the first XML export slice on the same
// occurrence-aware symbol text state instead of serializing raw parser-owned fields directly. It
// now also carries the representable `addSymbolFields()` multi-unit field scavenging plus the
// `makeSymbols()` unit/tstamp data and exercised `UseLibIdLookup()` / schematic `lib_name` split
// so same-reference units can collapse onto one component owner without losing the KiCad
// `<libsource>` branch choice. Component `<units>` now also preserve the linked library-unit order
// instead of applying the old repo-local name sort, matching the exercised `GetUnitPinInfo()`
// ownership more closely.
fn symbol_to_xml_component(
    project: &SchematicProject,
    sheet_path: &crate::loader::LoadedSheetPath,
    symbol: &Symbol,
    extra_symbols: &[&Symbol],
) -> Option<NetlistComponent> {
    fn upsert_component_field(
        fields: &mut Vec<(String, (i32, String))>,
        key: &str,
        unit: i32,
        value: String,
    ) {
        match fields.iter_mut().find(|(field_key, _)| field_key == key) {
            Some((_, (field_unit, field_value))) if *field_unit <= unit => {}
            Some((_, (field_unit, field_value))) => {
                *field_unit = unit;
                *field_value = value;
            }
            None => fields.push((key.to_string(), (unit, value))),
        }
    }

    let state =
        resolved_symbol_text_state(symbol, &sheet_path.instance_path, project.current_variant());
    let base_state = resolved_symbol_text_state(symbol, &sheet_path.instance_path, None);
    let base_dnp = symbol
        .occurrence_base
        .as_ref()
        .map(|base| base.dnp)
        .unwrap_or(symbol.dnp);
    let base_in_bom = symbol
        .occurrence_base
        .as_ref()
        .map(|base| base.in_bom)
        .unwrap_or(symbol.in_bom);
    let base_excluded_from_sim = symbol
        .occurrence_base
        .as_ref()
        .map(|base| base.excluded_from_sim)
        .unwrap_or(symbol.excluded_from_sim);
    let base_in_pos_files = symbol
        .occurrence_base
        .as_ref()
        .map(|base| base.in_pos_files)
        .unwrap_or(symbol.in_pos_files);
    let reference = resolved_property_value(&state.properties, "Reference")?;
    let mut value = resolved_property_value(&state.properties, "Value").unwrap_or_default();
    let mut footprint = resolved_property_value(&state.properties, "Footprint").unwrap_or_default();
    let mut datasheet = resolved_property_value(&state.properties, "Datasheet").unwrap_or_default();
    let mut description =
        resolved_property_value(&state.properties, "Description").unwrap_or_default();
    let (lib, part) = match symbol.lib_name.as_deref() {
        Some(lib_name) if !lib_name.is_empty() => (String::new(), lib_name.to_string()),
        _ => symbol
            .lib_id
            .split_once(':')
            .map(|(lib, part)| (lib.to_string(), part.to_string()))
            .unwrap_or_else(|| (String::new(), symbol.lib_id.clone())),
    };
    let component_classes =
        collect_component_class_names_for_all_symbol_units(project, symbol, sheet_path, &reference);
    let variants = symbol
        .instances
        .iter()
        .find(|instance| instance.path == sheet_path.instance_path)
        .map(|instance| {
            instance
                .variants
                .iter()
                .filter_map(|(name, variant)| {
                    let mut properties = Vec::new();

                    if variant.dnp != base_dnp {
                        properties.push((
                            "dnp".to_string(),
                            if variant.dnp { "1" } else { "0" }.to_string(),
                        ));
                    }

                    if variant.in_bom != base_in_bom {
                        properties.push((
                            "exclude_from_bom".to_string(),
                            if variant.in_bom { "0" } else { "1" }.to_string(),
                        ));
                    }

                    if variant.excluded_from_sim != base_excluded_from_sim {
                        properties.push((
                            "exclude_from_sim".to_string(),
                            if variant.excluded_from_sim { "1" } else { "0" }.to_string(),
                        ));
                    }

                    if variant.in_pos_files != base_in_pos_files {
                        properties.push((
                            "exclude_from_pos_files".to_string(),
                            if variant.in_pos_files { "0" } else { "1" }.to_string(),
                        ));
                    }

                    let mut fields = variant
                        .fields
                        .iter()
                        .filter_map(|(field_name, field_value)| {
                            let base_value = base_state
                                .properties
                                .iter()
                                .find(|property| {
                                    let property_key = if property.kind.is_mandatory() {
                                        property.kind.canonical_key()
                                    } else {
                                        property.key.as_str()
                                    };

                                    property_key.eq_ignore_ascii_case(field_name)
                                })
                                .map(|property| property.value.as_str())
                                .unwrap_or_default();

                            (field_value != base_value)
                                .then(|| (field_name.clone(), field_value.clone()))
                        })
                        .collect::<Vec<_>>();
                    fields.sort_by(|(lhs, _), (rhs, _)| lhs.cmp(rhs));

                    (!properties.is_empty() || !fields.is_empty()).then(|| {
                        NetlistComponentVariant {
                            name: name.clone(),
                            properties,
                            fields,
                        }
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut fields = state
        .properties
        .iter()
        .filter(|property| !property.kind.is_mandatory() && !property.is_private)
        .map(|property| {
            (
                property.key.clone(),
                (symbol.unit.unwrap_or(1), property.value.clone()),
            )
        })
        .collect::<Vec<_>>();

    if symbol
        .lib_symbol
        .as_ref()
        .map(|lib_symbol| lib_symbol.units.len() > 1)
        .unwrap_or(false)
    {
        let mut min_unit = symbol.unit.unwrap_or(1);

        for candidate_path in &project.sheet_paths {
            let Some(schematic) = project.schematic(&candidate_path.schematic_path) else {
                continue;
            };

            for item in &schematic.screen.items {
                let SchItem::Symbol(candidate_symbol) = item else {
                    continue;
                };

                let Some(candidate_reference) = resolved_symbol_text_property_value(
                    &project.schematics,
                    candidate_path,
                    project.project.as_ref(),
                    project.current_variant(),
                    candidate_symbol,
                    "Reference",
                ) else {
                    continue;
                };

                if !candidate_reference.eq_ignore_ascii_case(&reference) {
                    continue;
                }

                let candidate_unit = candidate_symbol.unit.unwrap_or(1);
                let candidate_state = resolved_symbol_text_state(
                    candidate_symbol,
                    &candidate_path.instance_path,
                    project.current_variant(),
                );

                let candidate_value = resolved_property_value(&candidate_state.properties, "Value")
                    .unwrap_or_default();
                if !candidate_value.is_empty() && (candidate_unit < min_unit || value.is_empty()) {
                    value = candidate_value;
                }

                let candidate_footprint =
                    resolved_property_value(&candidate_state.properties, "Footprint")
                        .unwrap_or_default();
                if !candidate_footprint.is_empty()
                    && (candidate_unit < min_unit || footprint.is_empty())
                {
                    footprint = candidate_footprint;
                }

                let candidate_datasheet =
                    resolved_property_value(&candidate_state.properties, "Datasheet")
                        .unwrap_or_default();
                if !candidate_datasheet.is_empty()
                    && (candidate_unit < min_unit || datasheet.is_empty())
                {
                    datasheet = candidate_datasheet;
                }

                let candidate_description =
                    resolved_property_value(&candidate_state.properties, "Description")
                        .unwrap_or_default();
                if !candidate_description.is_empty()
                    && (candidate_unit < min_unit || description.is_empty())
                {
                    description = candidate_description;
                }

                for property in &candidate_state.properties {
                    if property.kind.is_mandatory() || property.is_private {
                        continue;
                    }

                    upsert_component_field(
                        &mut fields,
                        &property.key,
                        candidate_unit,
                        property.value.clone(),
                    );
                }

                min_unit = min_unit.min(candidate_unit);
            }
        }
    }

    upsert_component_field(&mut fields, "Footprint", i32::MAX, footprint.clone());
    upsert_component_field(&mut fields, "Datasheet", i32::MAX, datasheet.clone());
    upsert_component_field(&mut fields, "Description", i32::MAX, description.clone());

    let mut metadata_properties = state
        .properties
        .iter()
        .filter(|property| !property.kind.is_mandatory() && !property.is_private)
        .map(|property| (property.key.clone(), Some(property.value.clone())))
        .collect::<Vec<_>>();

    metadata_properties.extend(
        collect_parent_sheet_properties(project, sheet_path)
            .into_iter()
            .map(|(name, value)| (name, Some(value))),
    );

    if !symbol.in_bom {
        metadata_properties.push(("exclude_from_bom".to_string(), None));
    }

    if !symbol.on_board {
        metadata_properties.push(("exclude_from_board".to_string(), None));
    }

    if !symbol.in_pos_files {
        metadata_properties.push(("exclude_from_pos_files".to_string(), None));
    }

    if symbol.dnp {
        metadata_properties.push(("dnp".to_string(), None));
    }

    if let Some(keywords) = symbol
        .lib_symbol
        .as_ref()
        .and_then(|lib_symbol| lib_symbol.keywords.clone())
    {
        metadata_properties.push(("ki_keywords".to_string(), Some(keywords)));
    }

    let fp_filters = symbol
        .lib_symbol
        .as_ref()
        .map(|lib_symbol| lib_symbol.fp_filters.clone())
        .unwrap_or_default()
        .into_iter()
        .filter(|filter| !filter.is_empty())
        .collect::<Vec<_>>();
    if !fp_filters.is_empty() {
        metadata_properties.push(("ki_fp_filters".to_string(), Some(fp_filters.join(" "))));
    }

    Some(NetlistComponent {
        reference,
        unit_number: symbol.unit.unwrap_or(1),
        value: if value.is_empty() {
            "~".to_string()
        } else {
            value
        },
        footprint,
        datasheet,
        description,
        lib,
        part,
        path_names: human_component_sheet_path(project, sheet_path),
        path: sheet_path.instance_path.clone(),
        tstamps: {
            let mut tstamps = extra_symbols
                .iter()
                .filter_map(|extra_symbol| extra_symbol.uuid.clone())
                .collect::<Vec<_>>();
            if let Some(uuid) = symbol.uuid.clone() {
                tstamps.push(uuid);
            }
            tstamps
        },
        units: symbol
            .lib_symbol
            .as_ref()
            .map(|lib_symbol| {
                lib_symbol
                    .units
                    .iter()
                    .map(|unit| {
                        let mut pins = unit
                            .draw_items
                            .iter()
                            .filter(|item| item.kind == "pin")
                            .filter_map(|pin| pin.number.clone())
                            .collect::<Vec<_>>();
                        pins.sort_by(|lhs, rhs| str_num_cmp(lhs, rhs, true));
                        pins.dedup();

                        NetlistComponentUnit {
                            name: unit.unit_name.clone().unwrap_or_else(|| unit.name.clone()),
                            pins,
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        duplicate_pin_numbers_are_jumpers: symbol
            .lib_symbol
            .as_ref()
            .map(|lib_symbol| lib_symbol.duplicate_pin_numbers_are_jumpers)
            .unwrap_or(false),
        jumper_pin_groups: symbol
            .lib_symbol
            .as_ref()
            .map(|lib_symbol| {
                lib_symbol
                    .jumper_pin_groups
                    .iter()
                    .map(|group| {
                        let mut pins = group.iter().cloned().collect::<Vec<_>>();
                        pins.sort();
                        pins
                    })
                    .collect()
            })
            .unwrap_or_default(),
        component_classes,
        variants,
        metadata_properties,
        fields: fields
            .into_iter()
            .map(|(key, (_unit, value))| (key, value))
            .collect(),
    })
}

// Upstream parity: reduced local analogue for the pin portion of
// `NETLIST_EXPORTER_XML::makeLibParts()`. This is not a 1:1 library-adapter walk because the Rust
// tree still reads schematic-linked lib-symbol snapshots, but it preserves the exercised
// full libpart field list, duplicate-pin-number erasure, `StrNumCmp` pin ordering, stacked-pin
// expansion, pin-type emission, and library-field iteration order so downstream netlist consumers
// see the same logical pin list and field order KiCad exports.
fn lib_symbol_to_xml_libpart(lib_id: &str, lib_symbol: &crate::model::LibSymbol) -> NetlistLibPart {
    let (lib, part) = lib_id
        .split_once(':')
        .map(|(lib, part)| (lib.to_string(), part.to_string()))
        .unwrap_or_else(|| (String::new(), lib_id.to_string()));
    let docs = lib_symbol
        .properties
        .iter()
        .find(|property| property.kind == crate::model::PropertyKind::SymbolDatasheet)
        .map(|property| property.value.clone())
        .unwrap_or_default();
    let description = lib_symbol.description.clone().unwrap_or_else(|| {
        lib_symbol
            .properties
            .iter()
            .find(|property| property.kind == crate::model::PropertyKind::SymbolDescription)
            .map(|property| property.value.clone())
            .unwrap_or_default()
    });
    let fields = lib_symbol
        .properties
        .iter()
        .map(|property| {
            (
                if property.kind.is_mandatory() {
                    property.kind.canonical_key().to_string()
                } else {
                    property.key.clone()
                },
                property.value.clone(),
            )
        })
        .collect::<Vec<_>>();

    let mut pins = BTreeMap::<String, NetlistLibPartPin>::new();

    for unit in &lib_symbol.units {
        for pin in unit.draw_items.iter().filter(|item| item.kind == "pin") {
            let Some(number) = pin.number.clone() else {
                continue;
            };
            let name = pin.name.clone().unwrap_or_else(|| number.clone());
            let (expanded_numbers, stacked_valid) = expand_stacked_pin_notation(&number);

            if stacked_valid {
                for expanded_number in expanded_numbers {
                    pins.entry(expanded_number.clone())
                        .or_insert_with(|| NetlistLibPartPin {
                            number: expanded_number,
                            name: name.clone(),
                            electrical_type: pin.electrical_type.clone().unwrap_or_default(),
                        });
                }
            } else {
                pins.entry(number.clone()).or_insert(NetlistLibPartPin {
                    number,
                    name,
                    electrical_type: pin.electrical_type.clone().unwrap_or_default(),
                });
            }
        }
    }

    let mut pins = pins.into_values().collect::<Vec<_>>();
    pins.sort_by(|lhs, rhs| str_num_cmp(&lhs.number, &rhs.number, true));

    NetlistLibPart {
        lib,
        part,
        description,
        docs,
        fields,
        footprints: lib_symbol
            .fp_filters
            .iter()
            .filter(|filter| !filter.is_empty())
            .cloned()
            .collect(),
        pins,
    }
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn current_iso8601_datetime() -> String {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    now.format(format_description!(
        "[year]-[month]-[day]T[hour]:[minute]:[second]"
    ))
    .unwrap_or_else(|_| "1970-01-01T00:00:00".to_string())
}

fn path_filename(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

fn human_sheet_name(sheet_path: &crate::loader::LoadedSheetPath) -> String {
    if sheet_path.instance_path.is_empty() {
        "/".to_string()
    } else {
        sheet_path
            .sheet_name
            .clone()
            .unwrap_or_else(|| sheet_path.instance_path.clone())
    }
}

// Upstream parity: reduced local analogue for `NETLIST_EXPORTER_XML::makeDesignHeader()`. This is
// not a 1:1 KiCad design header because the Rust tree still lacks the full hierarchy-path display
// strings, project/build-version ownership, and broader worksheet/project metadata stack, but it
// preserves the current root-source/date/tool ownership, project text vars, and per-sheet
// title-block/source export needed by the live XML CLI slice.
pub fn collect_xml_design(project: &SchematicProject) -> NetlistDesign {
    let text_vars = project
        .project
        .as_ref()
        .map(|project| {
            project
                .text_variables
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect()
        })
        .unwrap_or_default();

    let mut sheets = Vec::new();

    for (index, sheet_path) in project.sheet_paths.iter().enumerate() {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };
        let title_block = schematic.screen.title_block.as_ref();
        let field = |raw: Option<&str>| {
            resolve_text_variables(
                raw.unwrap_or_default(),
                &|token| {
                    resolve_sheet_text_var(
                        &project.schematics,
                        &project.sheet_paths,
                        sheet_path,
                        project.project.as_ref(),
                        project.current_variant(),
                        token,
                        0,
                    )
                    .or_else(|| {
                        resolve_schematic_text_var(
                            &project.schematics,
                            sheet_path,
                            project.project.as_ref(),
                            project.current_variant(),
                            token,
                        )
                    })
                },
                0,
            )
        };
        let comments = (1..=9)
            .map(|number| {
                (
                    number,
                    field(title_block.and_then(|title_block| title_block.comment(number))),
                )
            })
            .collect();

        sheets.push(NetlistSheetDesign {
            number: index + 1,
            name: human_sheet_name(sheet_path),
            tstamps: if sheet_path.instance_path.is_empty() {
                "/".to_string()
            } else {
                sheet_path.instance_path.clone()
            },
            title: field(title_block.and_then(|title_block| title_block.title.as_deref())),
            company: field(title_block.and_then(|title_block| title_block.company.as_deref())),
            revision: field(title_block.and_then(|title_block| title_block.revision.as_deref())),
            date: field(title_block.and_then(|title_block| title_block.date.as_deref())),
            source: path_filename(&schematic.path),
            comments,
        });
    }

    NetlistDesign {
        source: project.root_path.to_string_lossy().into_owned(),
        date: current_iso8601_datetime(),
        tool: format!("Eeschema {}", env!("CARGO_PKG_VERSION")),
        text_vars,
        sheets,
    }
}

// Upstream parity: reduced local analogue for `NETLIST_EXPORTER_XML::makeGroups()` under KiCad's
// `GNL_OPT_KICAD` path. This is not a 1:1 design-block/group owner because the Rust tree still
// lacks KiCad's live `SCH_GROUP` item graph, but it preserves the exercised group name/uuid/lib_id
// payload and the symbol-member-only filtering used by the KiCad-format netlist root.
pub fn collect_kicad_groups(project: &SchematicProject) -> Vec<NetlistGroup> {
    let symbol_uuids = project
        .schematics
        .iter()
        .flat_map(|schematic| schematic.screen.items.iter())
        .filter_map(|item| match item {
            SchItem::Symbol(symbol) => symbol.uuid.clone(),
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>();
    let mut groups = Vec::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };

        for item in &schematic.screen.items {
            let SchItem::Group(group) = item else {
                continue;
            };

            let Some(uuid) = group.uuid.clone() else {
                continue;
            };

            groups.push(NetlistGroup {
                name: group.name.clone().unwrap_or_default(),
                uuid,
                lib_id: group.lib_id.clone().unwrap_or_default(),
                members: group
                    .members
                    .iter()
                    .filter(|member| symbol_uuids.contains(*member))
                    .cloned()
                    .collect(),
            });
        }
    }

    groups
}

// Upstream parity: reduced local analogue for `NETLIST_EXPORTER_XML::makeVariants()` under
// KiCad's `GNL_OPT_KICAD` path. This is not a 1:1 schematic/controller path because the current
// tree still sources variant descriptions only from the companion project JSON, but it preserves
// the exercised variant-name/description root section needed by the first live KiCad-format
// netlist slice.
pub fn collect_kicad_variants(project: &SchematicProject) -> Vec<NetlistVariant> {
    project
        .project
        .as_ref()
        .map(|project| {
            project
                .schematic
                .variant_descriptions
                .iter()
                .map(|(name, description)| NetlistVariant {
                    name: name.clone(),
                    description: description.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

// Upstream parity: reduced local analogue for `NETLIST_EXPORTER_XML::makeLibraries()`. This is
// not a 1:1 KiCad library-manager walk because the Rust tree still lacks the symbol-library
// adapter stack and URI resolver behind `GetFullURI()`, but it restores the owning `<libraries>`
// section boundary after `makeLibParts()` instead of omitting that root section entirely.
// Remaining divergence is child population: without the symbol-library subsystem, the reduced
// exporter can only keep the section live while URI-backed `<library>` items remain blocked.
pub fn collect_xml_libraries(_project: &SchematicProject) -> Vec<NetlistLibrary> {
    Vec::new()
}

// Upstream parity: reduced local analogue for `NETLIST_EXPORTER_XML::makeRoot()`. This is not a
// 1:1 KiCad netlist exporter because the Rust tree still lacks the full exporter base, populated
// `<libraries>` adapter stack, and non-XML format backends, but it preserves the same outer XML
// root ownership and the live reduced `design` / `components` / `libparts` / `nets` sections
// instead of inventing a repo-local export schema.
pub fn render_reduced_xml_netlist(project: &SchematicProject) -> String {
    render_reduced_netlist(project, false)
}

// Upstream parity: reduced local analogue for `NETLIST_EXPORTER_XML::makeRoot()` with
// `GNL_OPT_KICAD`. This is not a 1:1 KiCad serializer because the Rust tree still lacks the full
// exporter base and custom KiCad formatter, but it preserves the owning root-section order and the
// extra `groups` / top-level `variants` sections used by KiCad's native netlist flavor.
pub fn render_reduced_kicad_netlist(project: &SchematicProject) -> String {
    render_reduced_netlist(project, true)
}

fn render_reduced_netlist(project: &SchematicProject, include_kicad_sections: bool) -> String {
    let design = collect_xml_design(project);
    let mut xml = String::from("<export version=\"E\">\n");
    xml.push_str("  <design>\n");
    xml.push_str(&format!(
        "    <source>{}</source>\n",
        escape_xml(&design.source)
    ));
    xml.push_str(&format!("    <date>{}</date>\n", escape_xml(&design.date)));
    xml.push_str(&format!("    <tool>{}</tool>\n", escape_xml(&design.tool)));

    for (name, value) in design.text_vars {
        xml.push_str(&format!(
            "    <textvar name=\"{}\">{}</textvar>\n",
            escape_xml(&name),
            escape_xml(&value)
        ));
    }

    for sheet in design.sheets {
        xml.push_str(&format!(
            "    <sheet number=\"{}\" name=\"{}\" tstamps=\"{}\">\n",
            sheet.number,
            escape_xml(&sheet.name),
            escape_xml(&sheet.tstamps)
        ));
        xml.push_str("      <title_block>\n");
        xml.push_str(&format!(
            "        <title>{}</title>\n",
            escape_xml(&sheet.title)
        ));
        xml.push_str(&format!(
            "        <company>{}</company>\n",
            escape_xml(&sheet.company)
        ));
        xml.push_str(&format!(
            "        <rev>{}</rev>\n",
            escape_xml(&sheet.revision)
        ));
        xml.push_str(&format!(
            "        <date>{}</date>\n",
            escape_xml(&sheet.date)
        ));
        xml.push_str(&format!(
            "        <source>{}</source>\n",
            escape_xml(&sheet.source)
        ));

        for (number, value) in sheet.comments {
            xml.push_str(&format!(
                "        <comment number=\"{}\" value=\"{}\" />\n",
                number,
                escape_xml(&value)
            ));
        }

        xml.push_str("      </title_block>\n");
        xml.push_str("    </sheet>\n");
    }

    xml.push_str("  </design>\n");
    xml.push_str("  <components>\n");

    for component in collect_xml_components(project, include_kicad_sections) {
        xml.push_str(&format!(
            "    <comp ref=\"{}\">\n",
            escape_xml(&component.reference)
        ));
        let value = if component.value.is_empty() {
            "~"
        } else {
            component.value.as_str()
        };
        xml.push_str(&format!("      <value>{}</value>\n", escape_xml(value)));

        if !component.footprint.is_empty() {
            xml.push_str(&format!(
                "      <footprint>{}</footprint>\n",
                escape_xml(&component.footprint)
            ));
        }

        if !component.datasheet.is_empty() {
            xml.push_str(&format!(
                "      <datasheet>{}</datasheet>\n",
                escape_xml(&component.datasheet)
            ));
        }

        if !component.description.is_empty() {
            xml.push_str(&format!(
                "      <description>{}</description>\n",
                escape_xml(&component.description)
            ));
        }

        if !component.fields.is_empty() {
            xml.push_str("      <fields>\n");

            for (name, value) in component.fields {
                xml.push_str(&format!(
                    "        <field name=\"{}\">{}</field>\n",
                    escape_xml(&name),
                    escape_xml(&value)
                ));
            }

            xml.push_str("      </fields>\n");
        }

        xml.push_str("      <libsource");
        xml.push_str(&format!(" lib=\"{}\"", escape_xml(&component.lib)));
        xml.push_str(&format!(" part=\"{}\"", escape_xml(&component.part)));

        xml.push_str(&format!(
            " description=\"{}\"",
            escape_xml(&component.description)
        ));

        xml.push_str(" />\n");
        for (name, value) in component.metadata_properties {
            match value {
                Some(value) => xml.push_str(&format!(
                    "      <property name=\"{}\" value=\"{}\" />\n",
                    escape_xml(&name),
                    escape_xml(&value)
                )),
                None => xml.push_str(&format!(
                    "      <property name=\"{}\" />\n",
                    escape_xml(&name)
                )),
            }
        }

        if component.duplicate_pin_numbers_are_jumpers {
            xml.push_str(
                "      <duplicate_pin_numbers_are_jumpers>1</duplicate_pin_numbers_are_jumpers>\n",
            );
        }

        if !component.jumper_pin_groups.is_empty() {
            xml.push_str("      <jumper_pin_groups>\n");

            for group in component.jumper_pin_groups {
                xml.push_str("        <group>\n");

                for pin_name in group {
                    xml.push_str(&format!("          <pin>{}</pin>\n", escape_xml(&pin_name)));
                }

                xml.push_str("        </group>\n");
            }

            xml.push_str("      </jumper_pin_groups>\n");
        }

        if !component.variants.is_empty() {
            xml.push_str("      <variants>\n");

            for variant in component.variants {
                xml.push_str(&format!(
                    "        <variant name=\"{}\">\n",
                    escape_xml(&variant.name)
                ));

                for (name, value) in variant.properties {
                    xml.push_str(&format!(
                        "          <property name=\"{}\" value=\"{}\" />\n",
                        escape_xml(&name),
                        escape_xml(&value)
                    ));
                }

                if !variant.fields.is_empty() {
                    xml.push_str("          <fields>\n");

                    for (name, value) in variant.fields {
                        xml.push_str(&format!(
                            "            <field name=\"{}\">{}</field>\n",
                            escape_xml(&name),
                            escape_xml(&value)
                        ));
                    }

                    xml.push_str("          </fields>\n");
                }

                xml.push_str("        </variant>\n");
            }

            xml.push_str("      </variants>\n");
        }

        if !component.component_classes.is_empty() {
            xml.push_str("      <component_classes>\n");

            for class_name in component.component_classes {
                xml.push_str(&format!(
                    "        <class>{}</class>\n",
                    escape_xml(&class_name)
                ));
            }

            xml.push_str("      </component_classes>\n");
        }

        xml.push_str(&format!(
            "      <sheetpath names=\"{}\" tstamps=\"{}\" />\n",
            escape_xml(&component.path_names),
            escape_xml(if component.path.is_empty() {
                "/"
            } else {
                &component.path
            })
        ));

        if !component.tstamps.is_empty() {
            xml.push_str("      <tstamps>");

            for (index, tstamp) in component.tstamps.iter().enumerate() {
                if index > 0 {
                    xml.push(' ');
                }

                xml.push_str(&escape_xml(tstamp));
            }

            xml.push_str("</tstamps>\n");
        }

        if !component.units.is_empty() {
            xml.push_str("      <units>\n");

            for unit in component.units {
                xml.push_str(&format!(
                    "        <unit name=\"{}\">\n",
                    escape_xml(&unit.name)
                ));
                xml.push_str("          <pins>\n");

                for pin in unit.pins {
                    xml.push_str(&format!(
                        "            <pin num=\"{}\" />\n",
                        escape_xml(&pin)
                    ));
                }

                xml.push_str("          </pins>\n");
                xml.push_str("        </unit>\n");
            }

            xml.push_str("      </units>\n");
        }

        xml.push_str("    </comp>\n");
    }

    xml.push_str("  </components>\n");

    if include_kicad_sections {
        xml.push_str("  <groups>\n");

        for group in collect_kicad_groups(project) {
            xml.push_str(&format!(
                "    <group name=\"{}\" uuid=\"{}\" lib_id=\"{}\">\n",
                escape_xml(&group.name),
                escape_xml(&group.uuid),
                escape_xml(&group.lib_id)
            ));
            xml.push_str("      <members>\n");

            for member in group.members {
                xml.push_str(&format!(
                    "        <member uuid=\"{}\" />\n",
                    escape_xml(&member)
                ));
            }

            xml.push_str("      </members>\n");
            xml.push_str("    </group>\n");
        }

        xml.push_str("  </groups>\n");
        xml.push_str("  <variants>\n");

        for variant in collect_kicad_variants(project) {
            xml.push_str(&format!(
                "    <variant name=\"{}\"",
                escape_xml(&variant.name)
            ));

            if !variant.description.is_empty() {
                xml.push_str(&format!(
                    " description=\"{}\"",
                    escape_xml(&variant.description)
                ));
            }

            xml.push_str(" />\n");
        }

        xml.push_str("  </variants>\n");
    }

    xml.push_str("  <libparts>\n");

    for libpart in collect_xml_libparts(project) {
        xml.push_str(&format!(
            "    <libpart lib=\"{}\" part=\"{}\">\n",
            escape_xml(&libpart.lib),
            escape_xml(&libpart.part)
        ));

        if !libpart.description.is_empty() {
            xml.push_str(&format!(
                "      <description>{}</description>\n",
                escape_xml(&libpart.description)
            ));
        }

        if !libpart.docs.is_empty() {
            xml.push_str(&format!(
                "      <docs>{}</docs>\n",
                escape_xml(&libpart.docs)
            ));
        }

        if !libpart.footprints.is_empty() {
            xml.push_str("      <footprints>\n");

            for footprint in libpart.footprints {
                xml.push_str(&format!("        <fp>{}</fp>\n", escape_xml(&footprint)));
            }

            xml.push_str("      </footprints>\n");
        }

        xml.push_str("      <fields>\n");

        for (name, value) in libpart.fields {
            xml.push_str(&format!(
                "        <field name=\"{}\">{}</field>\n",
                escape_xml(&name),
                escape_xml(&value)
            ));
        }

        xml.push_str("      </fields>\n");

        if !libpart.pins.is_empty() {
            xml.push_str("      <pins>\n");

            for pin in libpart.pins {
                xml.push_str(&format!(
                    "        <pin num=\"{}\" name=\"{}\" type=\"{}\" />\n",
                    escape_xml(&pin.number),
                    escape_xml(&pin.name),
                    escape_xml(&pin.electrical_type)
                ));
            }

            xml.push_str("      </pins>\n");
        }

        xml.push_str("    </libpart>\n");
    }

    xml.push_str("  </libparts>\n");
    xml.push_str("  <libraries>\n");

    for library in collect_xml_libraries(project) {
        if let Some(uri) = library.uri {
            xml.push_str(&format!(
                "    <library logical=\"{}\">\n",
                escape_xml(&library.logical)
            ));
            xml.push_str(&format!("      <uri>{}</uri>\n", escape_xml(&uri)));
            xml.push_str("    </library>\n");
        }
    }

    xml.push_str("  </libraries>\n");
    xml.push_str("  <nets>\n");

    for net in collect_xml_nets(project, include_kicad_sections) {
        xml.push_str(&format!(
            "    <net code=\"{}\" name=\"{}\" class=\"{}\">\n",
            net.code,
            escape_xml(&net.name),
            escape_xml(&net.class)
        ));

        for node in net.nodes {
            xml.push_str(&format!(
                "      <node ref=\"{}\" pin=\"{}\"",
                escape_xml(&node.reference),
                escape_xml(&node.pin),
            ));

            if let Some(pinfunction) = node.pinfunction {
                xml.push_str(&format!(" pinfunction=\"{}\"", escape_xml(&pinfunction)));
            }

            xml.push_str(&format!(" pintype=\"{}\" />\n", escape_xml(&node.pintype)));
        }

        xml.push_str("    </net>\n");
    }

    xml.push_str("  </nets>\n</export>\n");
    xml
}
