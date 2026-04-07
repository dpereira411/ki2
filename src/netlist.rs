use std::collections::BTreeMap;
use std::path::Path;

use crate::connectivity::{
    ConnectionMemberKind, collect_connection_components, projected_symbol_pin_info,
};
use crate::core::SchematicProject;
use crate::loader::{
    SymbolPinTextVarKind, points_equal, resolve_point_connectivity_text_var,
    resolve_schematic_text_var, resolve_sheet_text_var, resolve_text_variables,
    resolved_symbol_text_state,
};
use crate::model::{Property, SchItem, Symbol};
use time::{OffsetDateTime, macros::format_description};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetlistComponent {
    pub reference: String,
    pub value: String,
    pub footprint: String,
    pub datasheet: String,
    pub description: String,
    pub lib: String,
    pub part: String,
    pub path: String,
    pub properties: Vec<(String, String)>,
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

// Upstream parity: reduced local analogue for the symbol iteration portion of
// `NETLIST_EXPORTER_XML::makeSymbols()`. This is not a 1:1 exporter-base walk because the Rust
// tree still omits libparts and resolved nets, but it preserves the current occurrence-aware
// component filtering, reference/value/footprint exposure, and `LIB_ID` split needed by the first
// live netlist CLI slice. Remaining divergence is the fuller KiCad duplicate-unit / variant /
// libpart walk, but reference ordering now follows the upstream `StrNumCmp` path instead of plain
// lexical sorting.
pub fn collect_xml_components(project: &SchematicProject) -> Vec<NetlistComponent> {
    let mut components = Vec::new();

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

            let Some(component) = symbol_to_xml_component(project, sheet_path, symbol) else {
                continue;
            };

            components.push(component);
        }
    }

    components.sort_by(|lhs, rhs| str_num_cmp(&lhs.reference, &rhs.reference, true));
    components
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
// exercised current-sheet node grouping, per-net net/class lookup, duplicate ref/pin erasure,
// stacked-pin expansion, and single-node/all-stacked `+no_connect` marking needed by the first
// live XML netlist slice. Remaining divergence is the fuller KiCad subgraph object model and
// graph-owned netcode/name caches. Net ordering now follows the upstream `StrNumCmp` path instead
// of the old lexical `BTreeMap` order.
pub fn collect_xml_nets(project: &SchematicProject) -> Vec<NetlistNet> {
    let mut nets = BTreeMap::<
        String,
        (
            String,
            bool,
            BTreeMap<(String, String), NetlistNode>,
            Vec<NetNodeBasePinKey>,
        ),
    >::new();

    for sheet_path in &project.sheet_paths {
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

            let net_nodes = nets
                .entry(net_name)
                .or_insert_with(|| (net_class, false, BTreeMap::new(), Vec::new()));

            if component
                .members
                .iter()
                .any(|member| member.kind == ConnectionMemberKind::NoConnectMarker)
            {
                net_nodes.1 = true;
            }

            for item in &schematic.screen.items {
                let SchItem::Symbol(symbol) = item else {
                    continue;
                };

                if !symbol.in_netlist {
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

                    let pinfunction = pin.name.clone().and_then(|name| {
                        let trimmed = name.trim();
                        (!trimmed.is_empty() && trimmed != "~").then_some(name)
                    });
                    let (expanded_numbers, stacked_valid) =
                        expand_stacked_pin_notation(&base_pin_number);
                    let base_pin_key = NetNodeBasePinKey {
                        symbol_uuid: symbol.uuid.clone(),
                        at: (pin.at[0].to_bits(), pin.at[1].to_bits()),
                        name: pin.name.clone(),
                    };
                    net_nodes.3.push(base_pin_key);

                    for pin_number in expanded_numbers {
                        let pinfunction = if stacked_valid {
                            match pinfunction.as_ref() {
                                Some(base_name) => Some(format!("{base_name}_{pin_number}")),
                                None if base_pin_number != pin_number => Some(pin_number.clone()),
                                None => None,
                            }
                        } else {
                            pinfunction.clone()
                        };

                        let node = NetlistNode {
                            reference: reference.clone(),
                            pin: pin_number.clone(),
                            pinfunction,
                            pintype: pin.electrical_type.clone().unwrap_or_default(),
                        };

                        net_nodes.2.insert((reference.clone(), pin_number), node);
                    }
                }
            }
        }
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
// occurrence-aware symbol text state instead of serializing raw parser-owned fields directly.
fn symbol_to_xml_component(
    project: &SchematicProject,
    sheet_path: &crate::loader::LoadedSheetPath,
    symbol: &Symbol,
) -> Option<NetlistComponent> {
    let state =
        resolved_symbol_text_state(symbol, &sheet_path.instance_path, project.current_variant());
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

    let mut fields = BTreeMap::new();

    for property in &state.properties {
        if property.kind.is_mandatory() || property.is_private {
            continue;
        }

        fields.insert(property.key.clone(), property.value.clone());
    }

    Some(NetlistComponent {
        reference,
        value,
        footprint,
        datasheet,
        description,
        lib,
        part,
        path: sheet_path.instance_path.clone(),
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

// Upstream parity: reduced local analogue for `NETLIST_EXPORTER_XML::makeRoot()`. This is not a
// 1:1 KiCad netlist exporter because the Rust tree still omits the full exporter base, libraries,
// variants/groups, and non-XML formats, but it preserves the same outer XML root ownership and the
// live reduced `design` / `components` / `libparts` / `nets` sections instead of inventing a
// repo-local export schema.
pub fn render_reduced_xml_netlist(project: &SchematicProject) -> String {
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

    for component in collect_xml_components(project) {
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
            "      <sheetpath names=\"/\" tstamps=\"{}\" />\n",
            escape_xml(if component.path.is_empty() {
                "/"
            } else {
                &component.path
            })
        ));

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

    for net in collect_xml_nets(project) {
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
