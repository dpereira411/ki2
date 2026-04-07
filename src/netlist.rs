use std::collections::BTreeMap;
use std::path::Path;

use crate::connectivity::projected_symbol_pin_info;
use crate::core::SchematicProject;
use crate::loader::{
    SymbolPinTextVarKind, resolve_point_connectivity_text_var, resolve_schematic_text_var,
    resolve_sheet_text_var, resolve_text_variables, resolved_symbol_text_state,
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
    pub pinfunction: String,
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

// Upstream parity: reduced local analogue for the symbol iteration portion of
// `NETLIST_EXPORTER_XML::makeSymbols()`. This is not a 1:1 exporter-base walk because the Rust
// tree still omits libparts and resolved nets, but it preserves the current occurrence-aware
// component filtering, reference/value/footprint exposure, and `LIB_ID` split needed by the first
// live netlist CLI slice.
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

    components.sort_by(|lhs, rhs| lhs.reference.cmp(&rhs.reference));
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
// point-net resolver instead of `CONNECTION_GRAPH` subgraphs, but it preserves the exercised
// current-sheet node grouping, per-pin net/class lookup, and duplicate ref/pin erasure needed by
// the first live XML netlist slice.
pub fn collect_xml_nets(project: &SchematicProject) -> Vec<NetlistNet> {
    let mut nets = BTreeMap::<String, (String, BTreeMap<(String, String), NetlistNode>)>::new();

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

            let state = resolved_symbol_text_state(
                symbol,
                &sheet_path.instance_path,
                project.current_variant(),
            );
            let Some(reference) = resolved_property_value(&state.properties, "Reference") else {
                continue;
            };

            for pin in projected_symbol_pin_info(symbol) {
                let Some(pin_number) = pin.number.clone() else {
                    continue;
                };

                let net_name = resolve_point_connectivity_text_var(
                    &project.schematics,
                    &project.sheet_paths,
                    sheet_path,
                    project.project.as_ref(),
                    project.current_variant(),
                    pin.at,
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
                    pin.at,
                    SymbolPinTextVarKind::NetClass,
                )
                .unwrap_or_default();

                let node = NetlistNode {
                    reference: reference.clone(),
                    pin: pin_number.clone(),
                    pinfunction: pin.name.clone().unwrap_or_else(|| pin_number.clone()),
                    pintype: pin.electrical_type.clone().unwrap_or_default(),
                };

                nets.entry(net_name)
                    .or_insert_with(|| (net_class, BTreeMap::new()))
                    .1
                    .insert((reference.clone(), pin_number), node);
            }
        }
    }

    nets.into_iter()
        .enumerate()
        .map(|(index, (name, (class, nodes)))| NetlistNet {
            code: index + 1,
            name,
            class,
            nodes: nodes.into_values().collect(),
        })
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

            pins.entry(number).or_insert(name);
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
                "      <node ref=\"{}\" pin=\"{}\" pinfunction=\"{}\" pintype=\"{}\" />\n",
                escape_xml(&node.reference),
                escape_xml(&node.pin),
                escape_xml(&node.pinfunction),
                escape_xml(&node.pintype)
            ));
        }

        xml.push_str("    </net>\n");
    }

    xml.push_str("  </nets>\n</export>\n");
    xml
}
