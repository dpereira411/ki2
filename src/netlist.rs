use std::collections::BTreeMap;

use crate::core::SchematicProject;
use crate::loader::resolved_symbol_text_state;
use crate::model::{Property, SchItem, Symbol};

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

// Upstream parity: reduced local analogue for `NETLIST_EXPORTER_XML::makeRoot()` /
// `makeSymbols()`. This is not a 1:1 KiCad netlist exporter because the Rust tree still emits only
// the first live XML component slice and omits libparts/libraries/nets, but it preserves the same
// outer XML root and component element ownership instead of inventing a repo-local export schema.
pub fn render_reduced_xml_netlist(project: &SchematicProject) -> String {
    let mut xml = String::from("<export version=\"E\">\n  <components>\n");

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

    xml.push_str("  </libparts>\n</export>\n");
    xml
}
