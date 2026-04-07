use std::collections::BTreeMap;
use std::path::Path;

use crate::connectivity::{
    ConnectionMemberKind, collect_connection_components, projected_symbol_pin_info,
};
use crate::core::SchematicProject;
use crate::loader::{
    SymbolPinTextVarKind, points_equal, resolve_point_connectivity_text_var,
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
    pub excluded_from_bom: bool,
    pub excluded_from_board: bool,
    pub excluded_from_pos_files: bool,
    pub dnp: bool,
    pub sheet_properties: Vec<(String, String)>,
    pub keywords: Option<String>,
    pub fp_filters: Vec<String>,
    pub duplicate_pin_numbers_are_jumpers: bool,
    pub jumper_pin_groups: Vec<Vec<String>>,
    pub component_classes: Vec<String>,
    pub variants: Vec<NetlistComponentVariant>,
    pub properties: Vec<(String, String)>,
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
    pub pins: Vec<(String, String)>,
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

#[derive(Debug, Clone)]
struct NetNodeCandidate {
    net_name: String,
    net_class: String,
    has_no_connect: bool,
    node: NetlistNode,
    base_pin_key: NetNodeBasePinKey,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct NetNodeBasePinKey {
    symbol_uuid: Option<String>,
    at: (u64, u64),
    name: Option<String>,
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
    properties.sort_by(|(lhs, _), (rhs, _)| lhs.cmp(rhs));
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

fn is_auto_generated_net_name(net_name: &str) -> bool {
    net_name.starts_with("unconnected-(") || net_name.starts_with("Net-(")
}

// Upstream parity: reduced local analogue for the symbol iteration portion of
// `NETLIST_EXPORTER_XML::makeSymbols()`. This is not a 1:1 exporter-base walk because the Rust
// tree still omits libparts and resolved nets, but it preserves the current occurrence-aware
// component filtering, reference/value/footprint exposure, and `LIB_ID` split needed by the first
// live netlist CLI slice. Remaining divergence is the fuller KiCad duplicate-unit / variant /
// libpart walk, but reference ordering now follows the upstream `StrNumCmp` path instead of plain
// lexical sorting, the current component metadata carrier now includes the exercised
// lib/jumper/placement flags KiCad emits on `<comp>`, and the `for_board` flag now mirrors the
// `GNL_OPT_KICAD` exclusion path for symbol/sheet `on_board` state. Remaining divergence is the
// still-missing fuller sheet exclusion ownership outside the current loaded sheet-state carrier.
pub fn collect_xml_components(
    project: &SchematicProject,
    for_board: bool,
) -> Vec<NetlistComponent> {
    let mut components = Vec::new();

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

            let Some(component) = symbol_to_xml_component(project, sheet_path, symbol) else {
                continue;
            };

            components.push(component);
        }
    }

    components.sort_by(|lhs, rhs| str_num_cmp(&lhs.reference, &rhs.reference, true));

    let mut deduped = Vec::<NetlistComponent>::new();

    for component in components {
        if let Some(existing) = deduped.iter_mut().find(|existing| {
            existing
                .reference
                .eq_ignore_ascii_case(&component.reference)
        }) {
            let candidate_classes = component.component_classes.clone();
            let candidate_unit = component.unit_number;
            let current_uuid = existing.tstamps.last().cloned();
            let candidate_uuid = component.tstamps.last().cloned();
            let mut combined_tstamps = existing.tstamps.clone();
            combined_tstamps.extend(component.tstamps.clone());
            combined_tstamps.sort();
            combined_tstamps.dedup();

            match (&current_uuid, &candidate_uuid) {
                (Some(current_uuid), Some(candidate_uuid)) if candidate_uuid < current_uuid => {
                    let previous = existing.clone();
                    *existing = component;
                    existing.tstamps = combined_tstamps;
                    if !previous.value.is_empty()
                        && (existing.unit_number > previous.unit_number
                            || existing.value.is_empty())
                    {
                        existing.value = previous.value;
                    }
                    if !previous.footprint.is_empty()
                        && (existing.unit_number > previous.unit_number
                            || existing.footprint.is_empty())
                    {
                        existing.footprint = previous.footprint;
                    }
                    if !previous.datasheet.is_empty()
                        && (existing.unit_number > previous.unit_number
                            || existing.datasheet.is_empty())
                    {
                        existing.datasheet = previous.datasheet;
                    }
                    if !previous.description.is_empty()
                        && (existing.unit_number > previous.unit_number
                            || existing.description.is_empty())
                    {
                        existing.description = previous.description;
                    }

                    let mut properties =
                        previous.properties.into_iter().collect::<BTreeMap<_, _>>();
                    for (name, value) in existing.properties.clone() {
                        if candidate_unit <= previous.unit_number || !properties.contains_key(&name)
                        {
                            properties.insert(name, value);
                        }
                    }
                    existing.properties = properties.into_iter().collect();
                }
                _ => {
                    existing.tstamps = combined_tstamps;
                    if !component.value.is_empty()
                        && (candidate_unit < existing.unit_number || existing.value.is_empty())
                    {
                        existing.value = component.value.clone();
                    }
                    if !component.footprint.is_empty()
                        && (candidate_unit < existing.unit_number || existing.footprint.is_empty())
                    {
                        existing.footprint = component.footprint.clone();
                    }
                    if !component.datasheet.is_empty()
                        && (candidate_unit < existing.unit_number || existing.datasheet.is_empty())
                    {
                        existing.datasheet = component.datasheet.clone();
                    }
                    if !component.description.is_empty()
                        && (candidate_unit < existing.unit_number
                            || existing.description.is_empty())
                    {
                        existing.description = component.description.clone();
                    }

                    let mut properties = existing
                        .properties
                        .clone()
                        .into_iter()
                        .collect::<BTreeMap<_, _>>();
                    for (name, value) in component.properties.clone() {
                        if candidate_unit < existing.unit_number || !properties.contains_key(&name)
                        {
                            properties.insert(name, value);
                        }
                    }
                    existing.properties = properties.into_iter().collect();
                    existing.unit_number = existing.unit_number.min(candidate_unit);
                }
            }

            let mut combined_classes = existing.component_classes.clone();
            combined_classes.extend(candidate_classes);
            combined_classes.sort();
            combined_classes.dedup();
            existing.component_classes = combined_classes;

            continue;
        }

        deduped.push(component);
    }

    deduped
}

// Upstream parity: reduced local analogue for `NETLIST_EXPORTER_XML::makeLibParts()`. This is not
// a 1:1 KiCad libpart exporter because the Rust tree still sources libparts only from the
// schematic-linked lib-symbol snapshots instead of the full library adapter stack, but it
// preserves the exercised unique-libpart collection, reduced field/docs/footprint export, and
// duplicate-pin-number erasure needed by the first live XML netlist slice.
pub fn collect_xml_libparts(project: &SchematicProject) -> Vec<NetlistLibPart> {
    let mut libparts = BTreeMap::<String, NetlistLibPart>::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };

        for item in &schematic.screen.items {
            let SchItem::Symbol(symbol) = item else {
                continue;
            };

            if !symbol.in_netlist {
                continue;
            }

            let Some(lib_symbol) = symbol.lib_symbol.as_ref() else {
                continue;
            };

            let key = symbol.lib_id.clone();

            libparts
                .entry(key)
                .or_insert_with(|| lib_symbol_to_xml_libpart(symbol.lib_id.as_str(), lib_symbol));
        }
    }

    libparts.into_values().collect()
}

// Upstream parity: reduced local analogue for `NETLIST_EXPORTER_XML::makeListOfNets()`. This is
// not a 1:1 KiCad net exporter because the Rust tree still derives net nodes from the reduced
// shared connectivity carrier instead of full `CONNECTION_GRAPH` subgraphs, but it preserves the
// exercised current-sheet node grouping, per-net net/class lookup, exporter-base duplicate
// ref/pin erasure with user-net preference over auto-generated nets, stacked-pin expansion,
// single-node/all-stacked `+no_connect` marking, and the `GNL_OPT_KICAD` `on_board` filter path
// needed by the first live XML/KiCad netlist slices. Remaining divergence is the fuller KiCad
// subgraph object model and graph-owned netcode/name caches. Net ordering now follows the
// upstream `StrNumCmp` path instead of the old lexical `BTreeMap` order.
pub fn collect_xml_nets(project: &SchematicProject, for_board: bool) -> Vec<NetlistNet> {
    let mut candidates = BTreeMap::<(String, String), NetNodeCandidate>::new();

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

        for component in collect_connection_components(schematic) {
            let net_name = resolve_point_connectivity_text_var(
                &project.schematics,
                &project.sheet_paths,
                sheet_path,
                project.project.as_ref(),
                project.current_variant(),
                component.anchor,
                SymbolPinTextVarKind::NetName,
            )
            .unwrap_or_default();

            if net_name.is_empty() {
                continue;
            }

            let net_class = resolve_point_connectivity_text_var(
                &project.schematics,
                &project.sheet_paths,
                sheet_path,
                project.project.as_ref(),
                project.current_variant(),
                component.anchor,
                SymbolPinTextVarKind::NetClass,
            )
            .unwrap_or_default();

            let has_no_connect = component
                .members
                .iter()
                .any(|member| member.kind == ConnectionMemberKind::NoConnectMarker);

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

                let state = resolved_symbol_text_state(
                    symbol,
                    &sheet_path.instance_path,
                    project.current_variant(),
                );
                let Some(reference) = resolved_property_value(&state.properties, "Reference")
                else {
                    continue;
                };

                for pin in projected_symbol_pin_info(symbol) {
                    let Some(base_pin_number) = pin.number.clone() else {
                        continue;
                    };

                    if !component.members.iter().any(|member| {
                        member.kind == ConnectionMemberKind::SymbolPin
                            && member.symbol_uuid == symbol.uuid
                            && points_equal(member.at, pin.at)
                    }) {
                        continue;
                    }

                    let pinfunction_base = pin.name.clone().and_then(|name| {
                        let trimmed = name.trim();
                        (!trimmed.is_empty() && trimmed != "~").then_some(name)
                    });
                    let (expanded_numbers, _) = expand_stacked_pin_notation(&base_pin_number);
                    let base_pin_key = NetNodeBasePinKey {
                        symbol_uuid: symbol.uuid.clone(),
                        at: (pin.at[0].to_bits(), pin.at[1].to_bits()),
                        name: pin.name.clone(),
                    };
                    let emits_expanded_pinfunction =
                        pinfunction_base.is_some() || expanded_numbers.len() > 1;

                    for pin_number in expanded_numbers {
                        let pinfunction = if emits_expanded_pinfunction {
                            match pinfunction_base.as_ref() {
                                Some(base_name) => Some(format!("{base_name}_{pin_number}")),
                                None => Some(pin_number.clone()),
                            }
                        } else {
                            None
                        };

                        let node = NetlistNode {
                            reference: reference.clone(),
                            pin: pin_number.clone(),
                            pinfunction,
                            pintype: pin.electrical_type.clone().unwrap_or_default(),
                        };
                        let candidate = NetNodeCandidate {
                            net_name: net_name.clone(),
                            net_class: net_class.clone(),
                            has_no_connect,
                            node,
                            base_pin_key: base_pin_key.clone(),
                        };
                        let key = (reference.clone(), pin_number);

                        match candidates.get(&key) {
                            Some(existing)
                                if is_auto_generated_net_name(&existing.net_name)
                                    && !is_auto_generated_net_name(&candidate.net_name) =>
                            {
                                candidates.insert(key, candidate);
                            }
                            None => {
                                candidates.insert(key, candidate);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    let mut nets = BTreeMap::<
        String,
        (
            String,
            bool,
            BTreeMap<(String, String), NetlistNode>,
            Vec<NetNodeBasePinKey>,
        ),
    >::new();

    for ((reference, pin_number), candidate) in candidates {
        let net_nodes = nets.entry(candidate.net_name).or_insert_with(|| {
            (
                candidate.net_class.clone(),
                false,
                BTreeMap::new(),
                Vec::new(),
            )
        });
        net_nodes.1 |= candidate.has_no_connect;
        net_nodes.2.insert((reference, pin_number), candidate.node);
        net_nodes.3.push(candidate.base_pin_key);
    }

    let mut nets = nets.into_iter().collect::<Vec<_>>();
    nets.sort_by(|(a_name, _), (b_name, _)| str_num_cmp(a_name, b_name, false));

    nets.into_iter()
        .enumerate()
        .map(
            |(index, (name, (class, has_no_connect, nodes, base_pins)))| {
                let mut nodes = nodes.into_values().collect::<Vec<_>>();
                let all_net_pins_stacked = !base_pins.is_empty()
                    && base_pins.iter().all(|base_pin| *base_pin == base_pins[0]);

                if has_no_connect && (nodes.len() == 1 || all_net_pins_stacked) {
                    for node in &mut nodes {
                        node.pintype.push_str("+no_connect");
                    }
                }

                NetlistNet {
                    code: index + 1,
                    name,
                    class,
                    nodes,
                }
            },
        )
        .collect()
}

// Upstream parity: reduced local helper for `NETLIST_EXPORTER_XML::addSymbolFields()` /
// `makeSymbols()`. This is not a 1:1 KiCad field resolver because the Rust tree still lacks the
// full libpart/groups/variants export stack, but it keeps the first XML export slice on the same
// occurrence-aware symbol text state instead of serializing raw parser-owned fields directly. It
// now also carries the representable `makeSymbols()` unit/tstamp data so multi-unit refs can be
// collapsed onto one component owner.
fn symbol_to_xml_component(
    project: &SchematicProject,
    sheet_path: &crate::loader::LoadedSheetPath,
    symbol: &Symbol,
) -> Option<NetlistComponent> {
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
    let value =
        resolved_property_value(&state.properties, "Value").unwrap_or_else(|| "~".to_string());
    let footprint = resolved_property_value(&state.properties, "Footprint").unwrap_or_default();
    let datasheet = resolved_property_value(&state.properties, "Datasheet").unwrap_or_default();
    let description = resolved_property_value(&state.properties, "Description").unwrap_or_default();
    let (lib, part) = symbol
        .lib_id
        .split_once(':')
        .map(|(lib, part)| (lib.to_string(), part.to_string()))
        .unwrap_or_else(|| (String::new(), symbol.lib_id.clone()));
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

    let mut fields = BTreeMap::new();

    for property in &state.properties {
        if property.kind.is_mandatory() || property.is_private {
            continue;
        }

        fields.insert(property.key.clone(), property.value.clone());
    }

    Some(NetlistComponent {
        reference,
        unit_number: symbol.unit.unwrap_or(1),
        value,
        footprint,
        datasheet,
        description,
        lib,
        part,
        path_names: human_component_sheet_path(project, sheet_path),
        path: sheet_path.instance_path.clone(),
        tstamps: symbol.uuid.clone().into_iter().collect(),
        units: symbol
            .lib_symbol
            .as_ref()
            .map(|lib_symbol| {
                let mut units = lib_symbol
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
                    .collect::<Vec<_>>();
                units.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));
                units
            })
            .unwrap_or_default(),
        excluded_from_bom: !symbol.in_bom,
        excluded_from_board: !symbol.on_board,
        excluded_from_pos_files: !symbol.in_pos_files,
        dnp: symbol.dnp,
        sheet_properties: collect_parent_sheet_properties(project, sheet_path),
        keywords: symbol
            .lib_symbol
            .as_ref()
            .and_then(|lib_symbol| lib_symbol.keywords.clone()),
        fp_filters: symbol
            .lib_symbol
            .as_ref()
            .map(|lib_symbol| lib_symbol.fp_filters.clone())
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
                    .map(|group| group.iter().cloned().collect())
                    .collect()
            })
            .unwrap_or_default(),
        component_classes,
        variants,
        properties: fields.into_iter().collect(),
    })
}

// Upstream parity: reduced local analogue for the pin portion of
// `NETLIST_EXPORTER_XML::makeLibParts()`. This is not a 1:1 library-adapter walk because the Rust
// tree still reads schematic-linked lib-symbol snapshots, but it preserves the exercised
// duplicate-pin-number erasure and stacked-pin expansion so downstream netlist consumers see the
// same logical pin list KiCad exports.
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
    let mut fields = BTreeMap::new();

    for property in &lib_symbol.properties {
        if property.kind.is_mandatory() || property.is_private {
            continue;
        }

        fields.insert(property.key.clone(), property.value.clone());
    }

    let mut pins = BTreeMap::<String, String>::new();

    for unit in &lib_symbol.units {
        for pin in unit.draw_items.iter().filter(|item| item.kind == "pin") {
            let Some(number) = pin.number.clone() else {
                continue;
            };
            let name = pin.name.clone().unwrap_or_else(|| number.clone());
            let (expanded_numbers, stacked_valid) = expand_stacked_pin_notation(&number);

            if stacked_valid {
                for expanded_number in expanded_numbers {
                    pins.entry(expanded_number).or_insert_with(|| name.clone());
                }
            } else {
                pins.entry(number).or_insert(name);
            }
        }
    }

    NetlistLibPart {
        lib,
        part,
        description,
        docs,
        fields: fields.into_iter().collect(),
        footprints: lib_symbol.fp_filters.clone(),
        pins: pins.into_iter().collect(),
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

// Upstream parity: reduced local analogue for `NETLIST_EXPORTER_XML::makeRoot()`. This is not a
// 1:1 KiCad netlist exporter because the Rust tree still omits the full exporter base, libraries,
// variants/groups, and non-XML formats, but it preserves the same outer XML root ownership and the
// live reduced `design` / `components` / `libparts` / `nets` sections instead of inventing a
// repo-local export schema.
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
        xml.push_str(&format!(
            "      <value>{}</value>\n",
            escape_xml(&component.value)
        ));

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

        xml.push_str("      <libsource");
        xml.push_str(&format!(" lib=\"{}\"", escape_xml(&component.lib)));
        xml.push_str(&format!(" part=\"{}\"", escape_xml(&component.part)));

        if !component.description.is_empty() {
            xml.push_str(&format!(
                " description=\"{}\"",
                escape_xml(&component.description)
            ));
        }

        xml.push_str(" />\n");
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

        if component.excluded_from_bom {
            xml.push_str("      <property name=\"exclude_from_bom\" />\n");
        }

        if component.excluded_from_board {
            xml.push_str("      <property name=\"exclude_from_board\" />\n");
        }

        if component.excluded_from_pos_files {
            xml.push_str("      <property name=\"exclude_from_pos_files\" />\n");
        }

        if component.dnp {
            xml.push_str("      <property name=\"dnp\" />\n");
        }

        for (name, value) in component.sheet_properties {
            xml.push_str(&format!(
                "      <property name=\"{}\" value=\"{}\" />\n",
                escape_xml(&name),
                escape_xml(&value)
            ));
        }

        if let Some(keywords) = component.keywords {
            xml.push_str(&format!(
                "      <property name=\"ki_keywords\" value=\"{}\" />\n",
                escape_xml(&keywords)
            ));
        }

        if !component.fp_filters.is_empty() {
            xml.push_str(&format!(
                "      <property name=\"ki_fp_filters\" value=\"{}\" />\n",
                escape_xml(&component.fp_filters.join(" "))
            ));
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

        if !component.properties.is_empty() {
            xml.push_str("      <fields>\n");

            for (name, value) in component.properties {
                xml.push_str(&format!(
                    "        <field name=\"{}\">{}</field>\n",
                    escape_xml(&name),
                    escape_xml(&value)
                ));
            }

            xml.push_str("      </fields>\n");
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

            for (number, name) in libpart.pins {
                xml.push_str(&format!(
                    "        <pin num=\"{}\" name=\"{}\" />\n",
                    escape_xml(&number),
                    escape_xml(&name)
                ));
            }

            xml.push_str("      </pins>\n");
        }

        xml.push_str("    </libpart>\n");
    }

    xml.push_str("  </libparts>\n");
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
