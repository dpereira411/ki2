use crate::connectivity::{
    ConnectionMemberKind, ReducedNetBasePinKey, collect_connection_components,
    collect_connection_points, collect_reduced_label_component_snapshots,
    collect_reduced_project_net_map, projected_symbol_pin_info, reduced_bus_members,
    reduced_text_is_bus, resolve_reduced_driver_conflict_at,
    resolve_reduced_net_name_for_symbol_pin, resolve_reduced_non_bus_driver_priority_at,
    resolve_reduced_project_net_at, resolve_reduced_project_net_for_symbol_pin,
};
use crate::core::SchematicProject;
use crate::diagnostic::{Diagnostic, Severity};
use crate::loader::{
    LoadedErcSeverity, collect_wire_segments, point_on_wire_segment, points_equal,
    reduced_net_name_sheet_path_prefix, resolve_cross_reference_text_var,
    resolve_label_connectivity_text_var, resolve_label_text_token_without_connectivity,
    resolve_sheet_text_var, resolve_text_variables, resolved_sheet_text_state,
    resolved_symbol_text_property_value, resolved_symbol_text_state,
};
use crate::model::{LabelKind, Property, PropertyKind, SchItem};
use std::collections::BTreeMap;

// Upstream parity: local entrypoint for the implemented `ERC_TESTER` slice. This is not a 1:1
// KiCad ERC runner because the current tree still lacks markers, the full pin-conflict matrix,
// and full `CONNECTION_GRAPH` ownership. It exists so ERC work can proceed in upstream routine
// order against real loaded schematic state instead of ad-hoc checks, and it now runs the
// collected diagnostics back through a reduced project-owned severity resolver instead of leaving
// rule severity hard-coded at every check site. Remaining divergence is the broader unported
// `ERC_TESTER` surface beyond the reduced rules currently implemented here.
pub fn run(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(check_duplicate_sheet_names(project));
    diagnostics.extend(check_text_assertions(project));
    diagnostics.extend(check_unresolved_text_variables(project));
    diagnostics.extend(check_multiunit_footprints(project));
    diagnostics.extend(check_missing_netclasses(project));
    diagnostics.extend(check_missing_units(project));
    diagnostics.extend(check_label_multiple_wires(project));
    diagnostics.extend(check_four_way_junction(project));
    diagnostics.extend(check_floating_wires(project));
    diagnostics.extend(check_dangling_wire_endpoints(project));
    diagnostics.extend(check_no_connect_pins(project));
    diagnostics.extend(check_no_connect_markers(project));
    diagnostics.extend(check_label_connectivity(project));
    diagnostics.extend(check_directive_labels(project));
    diagnostics.extend(check_hierarchical_sheets(project));
    diagnostics.extend(check_bus_to_net_conflicts(project));
    diagnostics.extend(check_bus_to_bus_conflicts(project));
    diagnostics.extend(check_bus_to_bus_entry_conflicts(project));
    diagnostics.extend(check_mult_unit_pin_conflicts(project));
    diagnostics.extend(check_pin_to_pin(project));
    diagnostics.extend(check_driver_conflicts(project));
    diagnostics.extend(check_duplicate_pin_nets(project));
    diagnostics.extend(check_single_global_labels(project));
    diagnostics.extend(check_similar_labels(project));
    diagnostics.extend(check_same_local_global_label(project));
    diagnostics.extend(check_footprint_filters(project));
    diagnostics.extend(check_stacked_pin_notation(project));
    diagnostics.extend(check_ground_pins(project));
    diagnostics.extend(check_off_grid_endpoints(project));
    diagnostics.extend(check_field_name_whitespace(project));
    apply_configured_rule_severities(project, diagnostics)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReducedPinType {
    Input = 0,
    Output = 1,
    Bidirectional = 2,
    TriState = 3,
    Passive = 4,
    Free = 5,
    Unspecified = 6,
    PowerIn = 7,
    PowerOut = 8,
    OpenCollector = 9,
    OpenEmitter = 10,
    NoConnect = 11,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PinConflict {
    Ok,
    Warning,
    Error,
}

struct ReducedErcPinContext {
    at: [f64; 2],
    path: std::path::PathBuf,
    reference: String,
    pin_number: String,
    pin_type: ReducedPinType,
}

// Upstream parity: reduced local helper for the modulus test inside
// `ERC_TESTER::TestOffGridEndpoints()`. This is not a 1:1 KiCad IU check because the Rust tree
// stores schematic geometry in millimeters, but it exists so the typed project connection-grid
// setting can drive the same endpoint-on-grid rule without inventing an ad hoc tolerance per call.
fn point_is_on_grid(at: [f64; 2], grid_size_mm: f64) -> bool {
    const EPSILON: f64 = 1e-6;

    if grid_size_mm <= 0.0 {
        return true;
    }

    [at[0], at[1]].into_iter().all(|coordinate| {
        let scaled = coordinate / grid_size_mm;
        (scaled - scaled.round()).abs() <= EPSILON
    })
}

fn parse_reduced_pin_type(electrical_type: &str) -> Option<ReducedPinType> {
    Some(match electrical_type {
        "input" => ReducedPinType::Input,
        "output" => ReducedPinType::Output,
        "bidirectional" => ReducedPinType::Bidirectional,
        "tri_state" => ReducedPinType::TriState,
        "passive" => ReducedPinType::Passive,
        "free" => ReducedPinType::Free,
        "unspecified" => ReducedPinType::Unspecified,
        "power_in" => ReducedPinType::PowerIn,
        "power_out" => ReducedPinType::PowerOut,
        "open_collector" => ReducedPinType::OpenCollector,
        "open_emitter" => ReducedPinType::OpenEmitter,
        "no_connect" => ReducedPinType::NoConnect,
        _ => return None,
    })
}

fn resolve_base_pin_type(
    project: &SchematicProject,
    base_pin: &ReducedNetBasePinKey,
) -> Option<ReducedErcPinContext> {
    let sheet_path = project
        .sheet_paths
        .iter()
        .find(|sheet_path| sheet_path.instance_path == base_pin.sheet_instance_path)?;
    let schematic = project.schematic(&sheet_path.schematic_path)?;
    let symbol = schematic.screen.items.iter().find_map(|item| match item {
        SchItem::Symbol(symbol) if symbol.uuid == base_pin.symbol_uuid => Some(symbol),
        _ => None,
    })?;

    projected_symbol_pin_info(symbol)
        .into_iter()
        .find(|pin| {
            pin.at[0].to_bits() == base_pin.at.0
                && pin.at[1].to_bits() == base_pin.at.1
                && match (&base_pin.name, &pin.name) {
                    (Some(base_name), Some(pin_name)) => pin_name == base_name,
                    (Some(base_name), None) => base_name == "~",
                    _ => true,
                }
        })
        .and_then(|pin| {
            let pin_number = pin.number?;
            let electrical_type = pin.electrical_type?;
            let pin_type = parse_reduced_pin_type(&electrical_type)?;
            let reference = resolved_symbol_text_property_value(
                &project.schematics,
                sheet_path,
                project.project.as_ref(),
                project.current_variant(),
                symbol,
                "Reference",
            )?;

            Some(ReducedErcPinContext {
                at: pin.at,
                path: schematic.path.clone(),
                reference,
                pin_number,
                pin_type,
            })
        })
}

fn pin_type_index(pin_type: ReducedPinType) -> usize {
    match pin_type {
        ReducedPinType::Input => 0,
        ReducedPinType::Output => 1,
        ReducedPinType::Bidirectional => 2,
        ReducedPinType::TriState => 3,
        ReducedPinType::Passive => 4,
        ReducedPinType::Free => 5,
        ReducedPinType::Unspecified => 6,
        ReducedPinType::PowerIn => 7,
        ReducedPinType::PowerOut => 8,
        ReducedPinType::OpenCollector => 9,
        ReducedPinType::OpenEmitter => 10,
        ReducedPinType::NoConnect => 11,
    }
}

// Upstream parity: reduced local helper for the `ERC_SETTINGS::GetPinMapValue()` branch used by
// `ERC_TESTER::TestPinToPin()`. This is not a 1:1 KiCad ERC settings object because the Rust tree
// still only carries the typed companion-project `erc.pin_map` matrix slice, but it keeps pin
// conflict severity on the same project-owned override path instead of hard-coding the default
// matrix for every project. Remaining divergence is the broader ERC settings surface and KiCad's
// richer pin-mismatch ranking/marker policy.
fn configured_pin_conflict(
    project: &SchematicProject,
    lhs: ReducedPinType,
    rhs: ReducedPinType,
) -> PinConflict {
    let lhs_index = pin_type_index(lhs);
    let rhs_index = pin_type_index(rhs);

    match project
        .project
        .as_ref()
        .and_then(|settings| settings.erc_pin_map_value(lhs_index, rhs_index))
    {
        Some(0) => PinConflict::Ok,
        Some(1) => PinConflict::Warning,
        Some(2 | 3) => PinConflict::Error,
        Some(_) | None => pin_conflict(lhs, rhs),
    }
}

fn configured_rule_severity(
    project: &SchematicProject,
    settings_key: &'static str,
    default: Option<Severity>,
) -> Option<Severity> {
    match project
        .project
        .as_ref()
        .and_then(|settings| settings.erc_rule_severity(settings_key))
    {
        Some(LoadedErcSeverity::Warning) => Some(Severity::Warning),
        Some(LoadedErcSeverity::Error) => Some(Severity::Error),
        Some(LoadedErcSeverity::Ignore) => None,
        None => default,
    }
}

fn configured_missing_driver_severity(
    project: &SchematicProject,
    message: &str,
) -> Option<Severity> {
    let settings_key = if message.starts_with("Power input pin") {
        "power_pin_not_driven"
    } else {
        "pin_not_driven"
    };

    configured_rule_severity(project, settings_key, Some(Severity::Error))
}

fn apply_configured_rule_severity(
    project: &SchematicProject,
    mut diagnostic: Diagnostic,
) -> Option<Diagnostic> {
    let severity = match diagnostic.code {
        "erc-duplicate-sheet-name" => {
            configured_rule_severity(project, "duplicate_sheet_names", Some(Severity::Error))
        }
        "erc-missing-units" => {
            configured_rule_severity(project, "missing_unit", Some(Severity::Warning))
        }
        "erc-undefined-netclass" => {
            configured_rule_severity(project, "undefined_netclass", Some(Severity::Error))
        }
        "erc-label-multiple-wires" => {
            configured_rule_severity(project, "label_multiple_wires", Some(Severity::Warning))
        }
        "erc-four-way-junction" => configured_rule_severity(project, "four_way_junction", None),
        "erc-nc-pin-connected" | "erc-no-connect-connected" => {
            configured_rule_severity(project, "no_connect_connected", Some(Severity::Warning))
        }
        "erc-label-not-connected" => {
            configured_rule_severity(project, "label_dangling", Some(Severity::Error))
        }
        "erc-label-single-pin" => {
            configured_rule_severity(project, "isolated_pin_label", Some(Severity::Warning))
        }
        "erc-unconnected-wire-endpoint" => configured_rule_severity(
            project,
            "unconnected_wire_endpoint",
            Some(Severity::Warning),
        ),
        "erc-wire-dangling" => {
            configured_rule_severity(project, "wire_dangling", Some(Severity::Error))
        }
        "erc-pin-not-connected" => {
            configured_rule_severity(project, "pin_not_connected", Some(Severity::Error))
        }
        "erc-hierarchical-label-mismatch" => {
            configured_rule_severity(project, "hier_label_mismatch", Some(Severity::Error))
        }
        "erc-bus-to-net-conflict" => {
            configured_rule_severity(project, "bus_to_net_conflict", Some(Severity::Error))
        }
        "erc-bus-to-bus-conflict" => {
            configured_rule_severity(project, "bus_to_bus_conflict", Some(Severity::Error))
        }
        "erc-bus-entry-conflict" => {
            configured_rule_severity(project, "net_not_bus_member", Some(Severity::Warning))
        }
        "erc-pin-to-pin-warning" => {
            configured_rule_severity(project, "pin_to_pin", Some(Severity::Warning))
        }
        "erc-pin-to-pin-error" => {
            configured_rule_severity(project, "pin_to_pin", Some(Severity::Error))
        }
        "erc-missing-driver" => configured_missing_driver_severity(project, &diagnostic.message),
        "erc-driver-conflict" => {
            configured_rule_severity(project, "multiple_net_names", Some(Severity::Warning))
        }
        "erc-single-global-label" => configured_rule_severity(project, "single_global_label", None),
        "erc-similar-labels" => {
            configured_rule_severity(project, "similar_labels", Some(Severity::Warning))
        }
        "erc-similar-power" => {
            configured_rule_severity(project, "similar_power", Some(Severity::Warning))
        }
        "erc-similar-label-and-power" => {
            configured_rule_severity(project, "similar_label_and_power", Some(Severity::Warning))
        }
        "erc-same-local-global-label" => {
            configured_rule_severity(project, "same_local_global_label", Some(Severity::Warning))
        }
        "erc-ground-pin-not-ground" => {
            configured_rule_severity(project, "ground_pin_not_ground", Some(Severity::Warning))
        }
        "erc-endpoint-off-grid" => {
            configured_rule_severity(project, "endpoint_off_grid", Some(Severity::Warning))
        }
        "erc-field-name-whitespace" => {
            configured_rule_severity(project, "field_name_whitespace", Some(Severity::Warning))
        }
        "erc-unresolved-variable" => {
            configured_rule_severity(project, "unresolved_variable", Some(Severity::Error))
        }
        _ => Some(diagnostic.severity),
    }?;

    diagnostic.severity = severity;
    Some(diagnostic)
}

// Upstream parity: reduced local analogue for the `ERC_SETTINGS::m_ERCSeverities` application
// path. This is not a 1:1 KiCad marker/report owner because the current tree still collects plain
// `Diagnostic`s instead of `SCH_MARKER`s linked to `ERC_ITEM`s, but it keeps the exercised
// severity/default-ignore policy in one project-owned post-pass instead of hard-coding rule policy
// at each call site. Remaining divergence is the broader ERC item registry and marker lifecycle.
fn apply_configured_rule_severities(
    project: &SchematicProject,
    diagnostics: Vec<Diagnostic>,
) -> Vec<Diagnostic> {
    diagnostics
        .into_iter()
        .filter_map(|diagnostic| apply_configured_rule_severity(project, diagnostic))
        .collect()
}

fn pin_conflict(lhs: ReducedPinType, rhs: ReducedPinType) -> PinConflict {
    use PinConflict::{Error as Err, Ok, Warning as Warn};

    const MAP: [[PinConflict; 12]; 12] = [
        [Ok, Ok, Ok, Ok, Ok, Ok, Warn, Ok, Ok, Ok, Ok, Err],
        [Ok, Err, Ok, Warn, Ok, Ok, Warn, Ok, Err, Err, Err, Err],
        [Ok, Ok, Ok, Ok, Ok, Ok, Warn, Ok, Warn, Ok, Warn, Err],
        [Ok, Warn, Ok, Ok, Ok, Ok, Warn, Warn, Err, Warn, Warn, Err],
        [Ok, Ok, Ok, Ok, Ok, Ok, Warn, Ok, Ok, Ok, Ok, Err],
        [Ok, Ok, Ok, Ok, Ok, Ok, Ok, Ok, Ok, Ok, Ok, Err],
        [
            Warn, Warn, Warn, Warn, Warn, Ok, Warn, Warn, Warn, Warn, Warn, Err,
        ],
        [Ok, Ok, Ok, Warn, Ok, Ok, Warn, Ok, Ok, Ok, Ok, Err],
        [Ok, Err, Warn, Err, Ok, Ok, Warn, Ok, Err, Err, Err, Err],
        [Ok, Err, Ok, Warn, Ok, Ok, Warn, Ok, Err, Ok, Ok, Err],
        [Ok, Err, Warn, Warn, Ok, Ok, Warn, Ok, Err, Ok, Ok, Err],
        [Err, Err, Err, Err, Err, Err, Err, Err, Err, Err, Err, Err],
    ];

    MAP[lhs as usize][rhs as usize]
}

// Upstream parity: reduced local helper for the symbol-pin net lookup that several
// `ERC_TESTER` pin rules consume through the connection graph. This is not a 1:1 KiCad
// `SCH_PIN::Connection()` owner because the Rust tree still lacks full `CONNECTION_SUBGRAPH`
// item ownership for every projected pin, so it prefers the shared project-level reduced graph
// identity and falls back to the older current-sheet point-net resolver where that reduced item
// identity is still incomplete. The helper exists to keep that temporary divergence in one place
// while the backlog drives the remaining item-to-subgraph parity work.
fn resolved_pin_net_name(
    project: &SchematicProject,
    sheet_path: &crate::loader::LoadedSheetPath,
    schematic: &crate::model::Schematic,
    symbol: &crate::model::Symbol,
    pin_at: [f64; 2],
    pin_name: Option<&str>,
) -> String {
    let graph = project.reduced_project_net_graph(false);
    if let Some(net) =
        resolve_reduced_project_net_for_symbol_pin(&graph, sheet_path, symbol, pin_at, pin_name)
    {
        return net.name;
    }

    let sheet_path_prefix = reduced_net_name_sheet_path_prefix(&project.sheet_paths, sheet_path);
    resolve_reduced_net_name_for_symbol_pin(
        schematic,
        symbol,
        pin_at,
        Some(&sheet_path_prefix),
        |label| shown_label_text(project, sheet_path, label),
    )
    .unwrap_or_default()
}

// Upstream parity: reduced local helper for the generic connection-point net lookup that the
// graph-owned ERC marker passes use through `CONNECTION_GRAPH::GetResolvedSubgraphName()`. This is
// not a 1:1 KiCad subgraph owner because the Rust tree still lacks live item-owned
// `CONNECTION_SUBGRAPH`s, but ERC now reads current point-net identity from the shared
// project-level reduced graph owner instead of carrying a second current-sheet fallback resolver.
// Remaining divergence is the still-missing fuller item/subgraph object model, not duplicate
// point-net ownership inside ERC.
fn resolved_point_net_name(
    project: &SchematicProject,
    sheet_path: &crate::loader::LoadedSheetPath,
    at: [f64; 2],
) -> Option<String> {
    let graph = project.reduced_project_net_graph(false);
    resolve_reduced_project_net_at(&graph, sheet_path, at).map(|net| net.name)
}

fn is_driven_pin_type(pin_type: ReducedPinType) -> bool {
    matches!(pin_type, ReducedPinType::Input | ReducedPinType::PowerIn)
}

fn is_normal_driver_pin_type(pin_type: ReducedPinType) -> bool {
    matches!(
        pin_type,
        ReducedPinType::Output
            | ReducedPinType::PowerOut
            | ReducedPinType::Passive
            | ReducedPinType::TriState
            | ReducedPinType::Bidirectional
    )
}

fn is_power_driver_pin_type(pin_type: ReducedPinType) -> bool {
    pin_type == ReducedPinType::PowerOut
}

fn parse_alphanumeric_pin_token(token: &str) -> (String, Option<i64>) {
    let split_at = token
        .char_indices()
        .find(|(_, ch)| ch.is_ascii_digit())
        .map(|(index, _)| index)
        .unwrap_or(token.len());
    let (prefix, digits) = token.split_at(split_at);

    if digits.is_empty() {
        return (prefix.to_string(), None);
    }

    (prefix.to_string(), digits.parse::<i64>().ok())
}

// Upstream parity: reduced local analogue for `ExpandStackedPinNotation()`. This is not a 1:1
// KiCad utility port because it only returns validity for the exercised ERC path instead of the
// fully expanded sorted list KiCad uses elsewhere, but it preserves the same bracket/comma/range
// syntax rules needed by `TestStackedPinNotation()`.
fn stacked_pin_notation_is_valid(pin_name: &str) -> bool {
    let has_open = pin_name.contains('[');
    let has_close = pin_name.contains(']');

    if has_open || has_close {
        if !pin_name.starts_with('[') || !pin_name.ends_with(']') {
            return false;
        }
    }

    if !pin_name.starts_with('[') || !pin_name.ends_with(']') {
        return true;
    }

    let inner = &pin_name[1..pin_name.len() - 1];
    let mut expanded_any = false;

    for part in inner.split(',') {
        let part = part.trim();

        if part.is_empty() {
            continue;
        }

        if let Some((start_text, end_text)) = part.split_once('-') {
            let (start_prefix, start_value) = parse_alphanumeric_pin_token(start_text.trim());
            let (end_prefix, end_value) = parse_alphanumeric_pin_token(end_text.trim());

            let (Some(start_value), Some(end_value)) = (start_value, end_value) else {
                return false;
            };

            if start_prefix != end_prefix || start_value > end_value {
                return false;
            }

            expanded_any = true;
        } else {
            expanded_any = true;
        }
    }

    expanded_any
}

#[derive(Clone)]
struct SymbolOccurrence {
    schematic_path: std::path::PathBuf,
    reference: String,
    footprint: String,
    unit: Option<i32>,
    lib_unit_count: Option<usize>,
}

fn parse_text_assertion(text: &str) -> Option<(Severity, String)> {
    for (prefix, severity) in [
        ("${ERC_WARNING", Severity::Warning),
        ("${ERC_ERROR", Severity::Error),
    ] {
        let Some(rest) = text.strip_prefix(prefix) else {
            continue;
        };
        let Some((message, _tail)) = rest.split_once('}') else {
            continue;
        };
        return Some((severity, message.trim().to_string()));
    }

    None
}

fn text_assertion_diagnostic(
    path: &std::path::Path,
    severity: Severity,
    message: String,
) -> Diagnostic {
    Diagnostic {
        severity,
        code: match severity {
            Severity::Warning => "erc-generic-warning",
            Severity::Error => "erc-generic-error",
        },
        kind: crate::diagnostic::DiagnosticKind::Validation,
        message,
        path: Some(path.to_path_buf()),
        span: None,
        line: None,
        column: None,
    }
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

fn collect_symbol_occurrences(project: &SchematicProject) -> Vec<SymbolOccurrence> {
    let mut occurrences = Vec::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project
            .schematics
            .iter()
            .find(|schematic| schematic.path == sheet_path.schematic_path)
        else {
            continue;
        };

        for item in &schematic.screen.items {
            let SchItem::Symbol(symbol) = item else {
                continue;
            };

            let state = resolved_symbol_text_state(
                symbol,
                &sheet_path.instance_path,
                project.current_variant(),
            );
            let Some(reference) = resolved_property_value(&state.properties, "Reference") else {
                continue;
            };
            let footprint =
                resolved_property_value(&state.properties, "Footprint").unwrap_or_default();
            let lib_unit_count = symbol.lib_symbol.as_ref().map(|lib_symbol| {
                lib_symbol
                    .units
                    .iter()
                    .map(|unit| unit.unit_number)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
            });

            occurrences.push(SymbolOccurrence {
                schematic_path: schematic.path.clone(),
                reference,
                footprint,
                unit: symbol.unit,
                lib_unit_count,
            });
        }
    }

    occurrences
}

fn unresolved_variable_diagnostic(path: &std::path::Path, message: String) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "erc-unresolved-variable",
        kind: crate::diagnostic::DiagnosticKind::Validation,
        message,
        path: Some(path.to_path_buf()),
        span: None,
        line: None,
        column: None,
    }
}

fn component_contains_line_kind(
    component: &crate::connectivity::ConnectionComponent,
    line: &crate::model::Line,
) -> bool {
    line.points.windows(2).any(|segment| {
        component
            .members
            .iter()
            .any(|member| point_on_wire_segment(member.at, segment[0], segment[1]))
    })
}

fn child_sheet_path_for_sheet<'a>(
    project: &'a SchematicProject,
    parent_path: &crate::loader::LoadedSheetPath,
    sheet: &crate::model::Sheet,
) -> Option<&'a crate::loader::LoadedSheetPath> {
    project
        .child_sheet_paths(&parent_path.instance_path)
        .into_iter()
        .find(|child| child.sheet_uuid == sheet.uuid)
}

fn shown_symbol_property_text(
    project: &SchematicProject,
    sheet_path: &crate::loader::LoadedSheetPath,
    symbol: &crate::model::Symbol,
    property: &Property,
) -> String {
    let state =
        resolved_symbol_text_state(symbol, &sheet_path.instance_path, project.current_variant());

    resolve_text_variables(
        &property.value,
        &|token| {
            if token.contains(':') {
                if let Some(value) = resolve_cross_reference_text_var(
                    &project.schematics,
                    &project.sheet_paths,
                    sheet_path,
                    project.project.as_ref(),
                    project.current_variant(),
                    token,
                ) {
                    return Some(value);
                }
            }

            resolved_property_value(&state.properties, token).or_else(|| {
                resolve_sheet_text_var(
                    &project.schematics,
                    &project.sheet_paths,
                    sheet_path,
                    project.project.as_ref(),
                    project.current_variant(),
                    token,
                    1,
                )
            })
        },
        0,
    )
}

fn shown_lib_draw_text(
    project: &SchematicProject,
    sheet_path: &crate::loader::LoadedSheetPath,
    symbol: &crate::model::Symbol,
    text: &str,
) -> String {
    let state =
        resolved_symbol_text_state(symbol, &sheet_path.instance_path, project.current_variant());

    resolve_text_variables(
        text,
        &|token| {
            if token.contains(':') {
                if let Some(value) = resolve_cross_reference_text_var(
                    &project.schematics,
                    &project.sheet_paths,
                    sheet_path,
                    project.project.as_ref(),
                    project.current_variant(),
                    token,
                ) {
                    return Some(value);
                }
            }

            resolved_property_value(&state.properties, token).or_else(|| {
                resolve_sheet_text_var(
                    &project.schematics,
                    &project.sheet_paths,
                    sheet_path,
                    project.project.as_ref(),
                    project.current_variant(),
                    token,
                    1,
                )
            })
        },
        0,
    )
}

fn shown_sheet_property_text(
    project: &SchematicProject,
    sheet_path: &crate::loader::LoadedSheetPath,
    property: &Property,
) -> String {
    let Some(state) = resolved_sheet_text_state(
        &project.schematics,
        &project.sheet_paths,
        sheet_path,
        project.current_variant(),
    ) else {
        return property.value.clone();
    };

    resolve_text_variables(
        &property.value,
        &|token| {
            resolved_property_value(&state.properties, token).or_else(|| {
                resolve_sheet_text_var(
                    &project.schematics,
                    &project.sheet_paths,
                    sheet_path,
                    project.project.as_ref(),
                    project.current_variant(),
                    token,
                    1,
                )
            })
        },
        0,
    )
}

fn shown_label_property_text(
    project: &SchematicProject,
    sheet_path: &crate::loader::LoadedSheetPath,
    label: &crate::model::Label,
    property: &Property,
) -> String {
    resolve_text_variables(
        &property.value,
        &|token| {
            resolve_label_connectivity_text_var(
                &project.schematics,
                &project.sheet_paths,
                sheet_path,
                project.project.as_ref(),
                project.current_variant(),
                label,
                token,
            )
            .or_else(|| {
                if token.contains(':') {
                    resolve_cross_reference_text_var(
                        &project.schematics,
                        &project.sheet_paths,
                        sheet_path,
                        project.project.as_ref(),
                        project.current_variant(),
                        token,
                    )
                } else {
                    None
                }
            })
            .or_else(|| {
                resolve_label_text_token_without_connectivity(
                    &project.schematics,
                    &project.sheet_paths,
                    sheet_path,
                    project.project.as_ref(),
                    project.current_variant(),
                    label,
                    token,
                )
            })
        },
        0,
    )
}

// Upstream parity: reduced local analogue for `SCH_LABEL_BASE::GetShownText()`. This is not a 1:1
// KiCad label resolver because the current tree still runs through the reduced Rust text-variable
// helpers instead of KiCad's full label/item resolver stack, but it preserves the exercised shown-
// text behavior needed by ERC label checks.
fn shown_label_text(
    project: &SchematicProject,
    sheet_path: &crate::loader::LoadedSheetPath,
    label: &crate::model::Label,
) -> String {
    resolve_text_variables(
        &label.text,
        &|token| {
            resolve_label_connectivity_text_var(
                &project.schematics,
                &project.sheet_paths,
                sheet_path,
                project.project.as_ref(),
                project.current_variant(),
                label,
                token,
            )
            .or_else(|| {
                if token.contains(':') {
                    resolve_cross_reference_text_var(
                        &project.schematics,
                        &project.sheet_paths,
                        sheet_path,
                        project.project.as_ref(),
                        project.current_variant(),
                        token,
                    )
                } else {
                    None
                }
            })
            .or_else(|| {
                resolve_label_text_token_without_connectivity(
                    &project.schematics,
                    &project.sheet_paths,
                    sheet_path,
                    project.project.as_ref(),
                    project.current_variant(),
                    label,
                    token,
                )
            })
        },
        0,
    )
}

// Upstream parity: reduced local analogue for the power-label `GetValue( true, &sheet, false )`
// path used by `ERC_TESTER::TestSimilarLabels()`. This is not a 1:1 placed-pin/value resolver
// because the Rust tree still lacks live power-pin items, but it keeps power-symbol comparisons on
// the same shown-value semantics already used elsewhere in the reduced text stack.
fn shown_symbol_value_text(
    project: &SchematicProject,
    sheet_path: &crate::loader::LoadedSheetPath,
    symbol: &crate::model::Symbol,
) -> Option<String> {
    symbol
        .properties
        .iter()
        .find(|property| property.kind == PropertyKind::SymbolValue)
        .map(|property| shown_symbol_property_text(project, sheet_path, symbol, property))
}

// Upstream parity: reduced local analogue for the wildcard matching KiCad uses in
// `ERC_TESTER::TestFootprintFilters()`. This is not a 1:1 `wxString::Matches()` replacement
// because it only carries the exercised `*` and `?` glob semantics, but it is enough to keep
// footprint-filter ERC on the same pattern language instead of a repo-local exact-string check.
fn wildcard_matches(pattern: &str, text: &str) -> bool {
    fn inner(pattern: &[u8], text: &[u8]) -> bool {
        match pattern.split_first() {
            None => text.is_empty(),
            Some((b'*', rest)) => {
                inner(rest, text) || (!text.is_empty() && inner(pattern, &text[1..]))
            }
            Some((b'?', rest)) => !text.is_empty() && inner(rest, &text[1..]),
            Some((head, rest)) => !text.is_empty() && *head == text[0] && inner(rest, &text[1..]),
        }
    }

    inner(pattern.as_bytes(), text.as_bytes())
}

fn shown_text_item_text(
    project: &SchematicProject,
    sheet_path: &crate::loader::LoadedSheetPath,
    text: &str,
) -> String {
    resolve_text_variables(
        text,
        &|token| {
            resolve_sheet_text_var(
                &project.schematics,
                &project.sheet_paths,
                sheet_path,
                project.project.as_ref(),
                project.current_variant(),
                token,
                1,
            )
        },
        0,
    )
}

// Upstream parity: reduced local analogue for the exercised unresolved-variable half of
// `ERC_TESTER::TestTextVars()`. This is not a 1:1 KiCad marker pass because the current tree still
// reports plain diagnostics and only covers the reduced worksheet `tbtext` slice instead of the
// full drawing-sheet draw-item model. It exists so ERC now checks the real loaded
// symbol/sheet/label/text/textbox/sheet-pin, linked-lib-text, and reduced current drawing-sheet
// shown-text paths. Remaining divergence is the fuller worksheet/default-drawing-sheet surface.
pub fn check_unresolved_text_variables(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project
            .schematics
            .iter()
            .find(|schematic| schematic.path == sheet_path.schematic_path)
        else {
            continue;
        };

        for item in &schematic.screen.items {
            match item {
                SchItem::Symbol(symbol) => {
                    for property in &symbol.properties {
                        let shown =
                            shown_symbol_property_text(project, sheet_path, symbol, property);

                        if shown.contains("${") {
                            diagnostics.push(unresolved_variable_diagnostic(
                                &schematic.path,
                                format!(
                                    "Unresolved text variable in symbol field '{}'",
                                    property.key
                                ),
                            ));
                        }
                    }

                    if let Some(lib_symbol) = symbol.lib_symbol.as_ref() {
                        for draw_item in lib_symbol
                            .units
                            .iter()
                            .flat_map(|unit| unit.draw_items.iter())
                            .filter(|draw_item| {
                                matches!(draw_item.kind.as_str(), "text" | "text_box")
                            })
                        {
                            let Some(text) = draw_item.text.as_deref() else {
                                continue;
                            };
                            let shown = shown_lib_draw_text(project, sheet_path, symbol, text);

                            if shown.contains("${") {
                                diagnostics.push(unresolved_variable_diagnostic(
                                    &schematic.path,
                                    format!(
                                        "Unresolved text variable in library {}",
                                        draw_item.kind
                                    ),
                                ));
                            }
                        }
                    }
                }
                SchItem::Label(label) => {
                    for property in &label.properties {
                        let shown = shown_label_property_text(project, sheet_path, label, property);

                        if shown.contains("${") {
                            diagnostics.push(unresolved_variable_diagnostic(
                                &schematic.path,
                                format!(
                                    "Unresolved text variable in label field '{}'",
                                    property.key
                                ),
                            ));
                        }
                    }
                }
                SchItem::Sheet(sheet) => {
                    for property in &sheet.properties {
                        let shown = shown_sheet_property_text(project, sheet_path, property);

                        if shown.contains("${") {
                            diagnostics.push(unresolved_variable_diagnostic(
                                &schematic.path,
                                format!(
                                    "Unresolved text variable in sheet field '{}'",
                                    property.key
                                ),
                            ));
                        }
                    }

                    if let Some(child_sheet_path) =
                        child_sheet_path_for_sheet(project, sheet_path, sheet)
                    {
                        for pin in &sheet.pins {
                            let shown = shown_text_item_text(project, child_sheet_path, &pin.name);

                            if shown.contains("${") {
                                diagnostics.push(unresolved_variable_diagnostic(
                                    &schematic.path,
                                    format!("Unresolved text variable in sheet pin '{}'", pin.name),
                                ));
                            }
                        }
                    }
                }
                SchItem::Text(text) => {
                    let shown = shown_text_item_text(project, sheet_path, &text.text);

                    if shown.contains("${") {
                        diagnostics.push(unresolved_variable_diagnostic(
                            &schematic.path,
                            "Unresolved text variable in schematic text".to_string(),
                        ));
                    }
                }
                SchItem::TextBox(text_box) => {
                    let shown = shown_text_item_text(project, sheet_path, &text_box.text);

                    if shown.contains("${") {
                        diagnostics.push(unresolved_variable_diagnostic(
                            &schematic.path,
                            "Unresolved text variable in schematic text box".to_string(),
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    if let Some(current) = project.current_schematic() {
        if let Ok(items) = project.current_drawing_sheet_shown_text_items() {
            for item in items {
                if item.text.contains("${") {
                    diagnostics.push(unresolved_variable_diagnostic(
                        &current.path,
                        "Unresolved text variable in drawing sheet".to_string(),
                    ));
                }
            }
        }
    }

    diagnostics
}

fn shown_sheet_name(sheet: &crate::model::Sheet) -> Option<&str> {
    sheet.properties.iter().find_map(|property| {
        (property.kind == crate::model::PropertyKind::SheetName)
            .then_some(property.value.as_str())
            .or_else(|| {
                property
                    .key
                    .eq_ignore_ascii_case("Sheetname")
                    .then_some(property.value.as_str())
            })
    })
}

// Upstream parity: reduced local analogue for `ERC_TESTER::TestDuplicateSheetNames()`. This is
// not a 1:1 KiCad marker pass because the Rust tree still reports plain diagnostics and still
// reads the already-loaded sheet-name field text instead of the full `GetShownName()` display
// stack, but it preserves the exercised same-screen duplicate-name comparison and case-insensitive
// matching. Remaining divergence is path-sensitive shown-name exactness.
pub fn check_duplicate_sheet_names(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for schematic in &project.schematics {
        let sheets = schematic
            .screen
            .items
            .iter()
            .filter_map(|item| match item {
                SchItem::Sheet(sheet) => Some(sheet),
                _ => None,
            })
            .collect::<Vec<_>>();

        for (index, sheet) in sheets.iter().enumerate() {
            let Some(name) = shown_sheet_name(sheet) else {
                continue;
            };

            for other in sheets.iter().skip(index + 1) {
                let Some(other_name) = shown_sheet_name(other) else {
                    continue;
                };

                if !name.eq_ignore_ascii_case(other_name) {
                    continue;
                }

                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    code: "erc-duplicate-sheet-name",
                    kind: crate::diagnostic::DiagnosticKind::Validation,
                    message: format!("Duplicate sheet name: '{name}'"),
                    path: Some(schematic.path.clone()),
                    span: None,
                    line: None,
                    column: None,
                });
            }
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for the exercised assertion-marker half of
// `ERC_TESTER::TestTextVars()`. This is not a 1:1 KiCad marker pass because the current tree
// still reports plain diagnostics and only covers the reduced worksheet `tbtext` slice instead of
// the full drawing-sheet draw-item model, but it preserves `${ERC_WARNING ...}` / `${ERC_ERROR
// ...}` handling on the exercised item families the local text-var walker now visits, including
// reduced drawing-sheet text. Remaining divergence is the broader unported assertion surface
// outside those item families.
pub fn check_text_assertions(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for schematic in &project.schematics {
        for item in &schematic.screen.items {
            match item {
                SchItem::Symbol(symbol) => {
                    for property in &symbol.properties {
                        if let Some((severity, message)) = parse_text_assertion(&property.value) {
                            diagnostics.push(text_assertion_diagnostic(
                                &schematic.path,
                                severity,
                                message,
                            ));
                        }
                    }

                    if let Some(lib_symbol) = symbol.lib_symbol.as_ref() {
                        for draw_item in lib_symbol
                            .units
                            .iter()
                            .flat_map(|unit| unit.draw_items.iter())
                            .filter(|draw_item| {
                                matches!(draw_item.kind.as_str(), "text" | "text_box")
                            })
                        {
                            let Some(text) = draw_item.text.as_deref() else {
                                continue;
                            };

                            if let Some((severity, message)) = parse_text_assertion(text) {
                                diagnostics.push(text_assertion_diagnostic(
                                    &schematic.path,
                                    severity,
                                    message,
                                ));
                            }
                        }
                    }
                }
                SchItem::Label(label) => {
                    for property in &label.properties {
                        if let Some((severity, message)) = parse_text_assertion(&property.value) {
                            diagnostics.push(text_assertion_diagnostic(
                                &schematic.path,
                                severity,
                                message,
                            ));
                        }
                    }
                }
                SchItem::Sheet(sheet) => {
                    for property in &sheet.properties {
                        if let Some((severity, message)) = parse_text_assertion(&property.value) {
                            diagnostics.push(text_assertion_diagnostic(
                                &schematic.path,
                                severity,
                                message,
                            ));
                        }
                    }
                }
                SchItem::Text(text) => {
                    if let Some((severity, message)) = parse_text_assertion(&text.text) {
                        diagnostics.push(text_assertion_diagnostic(
                            &schematic.path,
                            severity,
                            message,
                        ));
                    }
                }
                SchItem::TextBox(text_box) => {
                    if let Some((severity, message)) = parse_text_assertion(&text_box.text) {
                        diagnostics.push(text_assertion_diagnostic(
                            &schematic.path,
                            severity,
                            message,
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    if let Some(current) = project.current_schematic() {
        if let Ok(items) = project.current_drawing_sheet_text_items() {
            for item in items {
                if let Some((severity, message)) = parse_text_assertion(&item.text) {
                    diagnostics.push(text_assertion_diagnostic(&current.path, severity, message));
                }
            }
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for `ERC_TESTER::TestMultiunitFootprints()`. This is
// not a 1:1 KiCad marker/ref-map pass because the current tree still groups through reduced loaded
// symbol occurrence snapshots instead of `SCH_REFERENCE_LIST`, but it preserves the exercised
// same-reference footprint mismatch rule. Remaining divergence is richer unit-name/sheet-path
// marker context.
pub fn check_multiunit_footprints(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut by_reference: BTreeMap<String, Vec<SymbolOccurrence>> = BTreeMap::new();

    for occurrence in collect_symbol_occurrences(project) {
        by_reference
            .entry(occurrence.reference.clone())
            .or_default()
            .push(occurrence);
    }

    for (reference, occurrences) in by_reference {
        let Some(first_with_footprint) = occurrences
            .iter()
            .find(|occurrence| !occurrence.footprint.is_empty())
        else {
            continue;
        };

        for occurrence in occurrences.iter().skip(1) {
            if occurrence.footprint.is_empty()
                || occurrence.footprint == first_with_footprint.footprint
            {
                continue;
            }

            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                code: "erc-different-unit-footprint",
                kind: crate::diagnostic::DiagnosticKind::Validation,
                message: format!("Different footprints assigned to reference '{reference}'"),
                path: Some(occurrence.schematic_path.clone()),
                span: None,
                line: None,
                column: None,
            });
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for `ERC_TESTER::TestMissingUnits()`. This is not a 1:1
// KiCad reference-list pass because the current tree still groups through reduced loaded symbol
// occurrence snapshots and reports a simpler diagnostic message, but it preserves the exercised
// same-reference missing-unit check against linked library unit counts. Remaining divergence is
// richer unit-display-name/sheet-path marker context.
pub fn check_missing_units(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut by_reference: BTreeMap<String, Vec<SymbolOccurrence>> = BTreeMap::new();

    for occurrence in collect_symbol_occurrences(project) {
        by_reference
            .entry(occurrence.reference.clone())
            .or_default()
            .push(occurrence);
    }

    for (reference, occurrences) in by_reference {
        let Some(lib_unit_count) = occurrences
            .iter()
            .find_map(|occurrence| occurrence.lib_unit_count)
        else {
            continue;
        };

        if lib_unit_count <= 1 {
            continue;
        }

        let present_units = occurrences
            .iter()
            .filter_map(|occurrence| occurrence.unit)
            .collect::<std::collections::BTreeSet<_>>();

        if present_units.len() >= lib_unit_count {
            continue;
        }

        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            code: "erc-missing-units",
            kind: crate::diagnostic::DiagnosticKind::Validation,
            message: format!("Missing symbol units for reference '{reference}'"),
            path: occurrences
                .first()
                .map(|occurrence| occurrence.schematic_path.clone()),
            span: None,
            line: None,
            column: None,
        });
    }

    diagnostics
}

// Upstream parity: reduced local analogue for `ERC_TESTER::TestMissingNetclasses()`. This is not
// a 1:1 KiCad marker/settings pass because the current tree still uses a reduced typed
// companion-project netclass set instead of full `NET_SETTINGS`, but it preserves the exercised
// undefined-netclass check on item child fields using the same shown-text resolution paths as the
// local ERC text pass. Remaining divergence is broader project/netclass-pattern semantics.
pub fn check_missing_netclasses(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let default_netclass = project
        .project
        .as_ref()
        .map(|project| project.default_netclass().to_string())
        .unwrap_or_else(|| "Default".to_string());

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project
            .schematics
            .iter()
            .find(|schematic| schematic.path == sheet_path.schematic_path)
        else {
            continue;
        };

        for item in &schematic.screen.items {
            let mut check_value = |value: String| {
                if value.is_empty() || value == default_netclass {
                    return;
                }

                if project
                    .project
                    .as_ref()
                    .is_some_and(|project| project.has_netclass(&value))
                {
                    return;
                }

                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    code: "erc-undefined-netclass",
                    kind: crate::diagnostic::DiagnosticKind::Validation,
                    message: format!("Netclass {value} is not defined"),
                    path: Some(schematic.path.clone()),
                    span: None,
                    line: None,
                    column: None,
                });
            };

            match item {
                SchItem::Symbol(symbol) => {
                    for property in &symbol.properties {
                        if !property.key.eq_ignore_ascii_case("Netclass") {
                            continue;
                        }
                        check_value(shown_symbol_property_text(
                            project, sheet_path, symbol, property,
                        ));
                    }
                }
                SchItem::Label(label) => {
                    for property in &label.properties {
                        if !property.key.eq_ignore_ascii_case("Netclass") {
                            continue;
                        }
                        check_value(shown_label_property_text(
                            project, sheet_path, label, property,
                        ));
                    }
                }
                SchItem::Sheet(sheet) => {
                    for property in &sheet.properties {
                        if !property.key.eq_ignore_ascii_case("Netclass") {
                            continue;
                        }
                        check_value(shown_sheet_property_text(project, sheet_path, property));
                    }
                }
                _ => {}
            }
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for `ERC_TESTER::TestLabelMultipleWires()`. This is not
// a 1:1 KiCad overlapping-item pass because the current tree still uses reduced wire-segment
// geometry instead of a full connection-point graph, but it preserves the exercised local-label
// rule: a label touching more than one non-endpoint wire segment is an ERC error. Remaining
// divergence is the broader connection-point snapshot needed by the later connectivity routines.
pub fn check_label_multiple_wires(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project
            .schematics
            .iter()
            .find(|schematic| schematic.path == sheet_path.schematic_path)
        else {
            continue;
        };

        let wire_segments = collect_wire_segments(schematic);

        for item in &schematic.screen.items {
            let SchItem::Label(label) = item else {
                continue;
            };

            if label.kind != crate::model::LabelKind::Local {
                continue;
            }

            let touching_segments = wire_segments
                .iter()
                .filter(|segment| {
                    point_on_wire_segment(label.at, segment[0], segment[1])
                        && !points_equal(label.at, segment[0])
                        && !points_equal(label.at, segment[1])
                })
                .count();

            if touching_segments > 1 {
                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    code: "erc-label-multiple-wires",
                    kind: crate::diagnostic::DiagnosticKind::Validation,
                    message: format!(
                        "Label connects more than one wire at {}, {}",
                        label.at[0], label.at[1]
                    ),
                    path: Some(schematic.path.clone()),
                    span: None,
                    line: None,
                    column: None,
                });
            }
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for `ERC_TESTER::TestFourWayJunction()`. This is not a
// 1:1 KiCad marker pass because the Rust tree still lacks `SCH_MARKER` / `ERC_ITEM` and the full
// connection graph, but it now uses a shared connection-point snapshot that includes projected
// symbol pins and keeps bus segments separate from wire segments instead of collapsing both into a
// wire-only geometry shortcut. Remaining divergence is fuller connection-graph ownership and
// broader item-class participation beyond the exercised ERC slice.
pub fn check_four_way_junction(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project
            .schematics
            .iter()
            .find(|schematic| schematic.path == sheet_path.schematic_path)
        else {
            continue;
        };

        for point in collect_connection_points(schematic).into_values() {
            let junction_items = point
                .members
                .iter()
                .filter(|member| {
                    matches!(
                        member.kind,
                        ConnectionMemberKind::SymbolPin
                            | ConnectionMemberKind::SheetPin
                            | ConnectionMemberKind::Wire
                    )
                })
                .count();

            if junction_items < 4 {
                continue;
            }

            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                code: "erc-four-way-junction",
                kind: crate::diagnostic::DiagnosticKind::Validation,
                message: format!("Four items connected at {}, {}", point.at[0], point.at[1]),
                path: Some(schematic.path.clone()),
                span: None,
                line: None,
                column: None,
            });
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for `ERC_TESTER::TestNoConnectPins()`. This is not a
// 1:1 KiCad connectable-item walk because the Rust tree still lacks the full item connectivity API,
// but it uses the shared connection-point snapshot and projected symbol pins so no-connect ERC now
// checks real pin positions instead of a parser-only field approximation. Remaining divergence is
// fuller connectable-item coverage and connection-graph ownership beyond the exercised rule.
pub fn check_no_connect_pins(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project
            .schematics
            .iter()
            .find(|schematic| schematic.path == sheet_path.schematic_path)
        else {
            continue;
        };

        for point in collect_connection_points(schematic).into_values() {
            let nc_pins = point
                .members
                .iter()
                .filter(|member| {
                    member.kind == ConnectionMemberKind::SymbolPin
                        && member.electrical_type.as_deref() == Some("no_connect")
                })
                .count();

            if nc_pins == 0 {
                continue;
            }

            let connected_others = point.members.iter().filter(|member| {
                !matches!(member.kind, ConnectionMemberKind::NoConnectMarker)
                    && !(member.kind == ConnectionMemberKind::SymbolPin
                        && member.electrical_type.as_deref() == Some("no_connect"))
            });

            if connected_others.clone().next().is_none() {
                continue;
            }

            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                code: "erc-nc-pin-connected",
                kind: crate::diagnostic::DiagnosticKind::Validation,
                message: "Pin with 'no connection' type is connected".to_string(),
                path: Some(schematic.path.clone()),
                span: None,
                line: None,
                column: None,
            });
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for `CONNECTION_GRAPH::ercCheckNoConnects()`. This is
// not a 1:1 KiCad subgraph/item-marker pass because the Rust tree still lacks full
// `CONNECTION_SUBGRAPH` objects, hier-port child-subgraph traversal, and live item identity. It
// now prefers the shared project-level reduced graph owner for connection-point net identity, and
// it covers both exercised upstream branches:
// - connected no-connect markers on same-name nets
// - dangling no-connect markers with no pins or labels
// instead of leaving the reduced ERC path on the older connected-only point-local check.
// Remaining divergence is the fuller hier-pin and marker attachment path.
pub fn check_no_connect_markers(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut unique_pins_by_net =
        BTreeMap::<String, std::collections::BTreeSet<(String, (u64, u64))>>::new();
    let mut unique_labels_by_net =
        BTreeMap::<String, std::collections::BTreeSet<(std::path::PathBuf, (u64, u64))>>::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };
        for component in collect_connection_components(schematic) {
            let Some(net_name) = resolved_point_net_name(project, sheet_path, component.anchor)
            else {
                continue;
            };

            let pins = unique_pins_by_net.entry(net_name.clone()).or_default();

            for member in &component.members {
                if member.kind != ConnectionMemberKind::SymbolPin {
                    if member.kind == ConnectionMemberKind::Label {
                        unique_labels_by_net
                            .entry(net_name.clone())
                            .or_default()
                            .insert((
                                sheet_path.schematic_path.clone(),
                                (member.at[0].to_bits(), member.at[1].to_bits()),
                            ));
                    }

                    continue;
                }

                pins.insert((
                    member.symbol_uuid.clone().unwrap_or_default(),
                    (member.at[0].to_bits(), member.at[1].to_bits()),
                ));
            }
        }
    }

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };
        for component in collect_connection_components(schematic) {
            if !component
                .members
                .iter()
                .any(|member| member.kind == ConnectionMemberKind::NoConnectMarker)
            {
                continue;
            }

            let local_unique_pins = component
                .members
                .iter()
                .filter(|member| member.kind == ConnectionMemberKind::SymbolPin)
                .map(|member| {
                    (
                        member.symbol_uuid.clone().unwrap_or_default(),
                        (member.at[0].to_bits(), member.at[1].to_bits()),
                    )
                })
                .collect::<std::collections::BTreeSet<_>>();

            let has_sheet_pin = component
                .members
                .iter()
                .any(|member| member.kind == ConnectionMemberKind::SheetPin);
            let local_unique_labels = component
                .members
                .iter()
                .filter(|member| member.kind == ConnectionMemberKind::Label)
                .map(|member| {
                    (
                        sheet_path.schematic_path.clone(),
                        (member.at[0].to_bits(), member.at[1].to_bits()),
                    )
                })
                .collect::<std::collections::BTreeSet<_>>();
            let has_hierarchical_label = schematic.screen.items.iter().any(|item| match item {
                SchItem::Label(label) if label.kind == LabelKind::Hierarchical => {
                    component.members.iter().any(|member| {
                        member.kind == ConnectionMemberKind::Label
                            && points_equal(member.at, label.at)
                    })
                }
                _ => false,
            });
            let has_nc_pin = component.members.iter().any(|member| {
                member.kind == ConnectionMemberKind::SymbolPin
                    && member.electrical_type.as_deref() == Some("no_connect")
            });

            if ((has_sheet_pin || has_hierarchical_label) && local_unique_pins.is_empty())
                || (has_nc_pin && local_unique_pins.len() <= 1)
            {
                continue;
            }

            let net_name = resolved_point_net_name(project, sheet_path, component.anchor);
            let unique_pin_count = net_name
                .as_ref()
                .and_then(|name| unique_pins_by_net.get(name))
                .map(|pins| pins.len())
                .unwrap_or(local_unique_pins.len());
            let unique_label_count = net_name
                .as_ref()
                .and_then(|name| unique_labels_by_net.get(name))
                .map(|labels| labels.len())
                .unwrap_or(local_unique_labels.len());

            if unique_pin_count <= 1 {
                if unique_pin_count == 0 && unique_label_count == 0 {
                    diagnostics.push(Diagnostic {
                        severity: Severity::Warning,
                        code: "erc-no-connect-dangling",
                        kind: crate::diagnostic::DiagnosticKind::Validation,
                        message: "Unconnected \"no connection\" flag".to_string(),
                        path: Some(sheet_path.schematic_path.clone()),
                        span: None,
                        line: None,
                        column: None,
                    });
                }

                continue;
            }

            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                code: "erc-no-connect-connected",
                kind: crate::diagnostic::DiagnosticKind::Validation,
                message: "No-connect marker is attached to a connected net".to_string(),
                path: Some(sheet_path.schematic_path.clone()),
                span: None,
                line: None,
                column: None,
            });
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for `CONNECTION_GRAPH::ercCheckLabels()`. This is not a
// 1:1 KiCad graph pass because the Rust tree still lacks full cross-sheet subgraphs, bus-parent
// neighbor walks, and live `SCH_TEXT::IsDangling()` state. It exists so ERC now consumes shared
// reduced label/pin/no-connect component facts from `src/connectivity.rs` instead of another local
// geometry-only label scan. Remaining divergence is broader graph ownership beyond the current
// reduced connected-component carrier.
pub fn check_label_connectivity(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut label_components = Vec::new();
    let mut components_by_net =
        BTreeMap::<String, Vec<(std::path::PathBuf, [f64; 2], usize, bool, bool)>>::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };
        let sheet_path_prefix =
            reduced_net_name_sheet_path_prefix(&project.sheet_paths, sheet_path);

        for component in collect_reduced_label_component_snapshots(
            schematic,
            Some(&sheet_path_prefix),
            |label| shown_label_text(project, sheet_path, label),
        ) {
            if let Some(net_name) = component.net_name.clone().filter(|name| !name.is_empty()) {
                components_by_net.entry(net_name).or_default().push((
                    sheet_path.schematic_path.clone(),
                    component.anchor,
                    component.pin_count,
                    component.has_no_connect,
                    component.has_local_hierarchy,
                ));
            }

            label_components.push((sheet_path.schematic_path.clone(), component));
        }
    }

    for (schematic_path, component) in label_components {
        let mut all_pins = component.pin_count;
        let mut local_pins = component.pin_count;
        let mut has_no_connect = component.has_no_connect;
        let mut has_local_hierarchy = component.has_local_hierarchy;

        if let Some(net_name) = component.net_name.as_ref() {
            if let Some(neighbors) = components_by_net.get(net_name) {
                for (
                    neighbor_path,
                    neighbor_anchor,
                    neighbor_pin_count,
                    neighbor_has_no_connect,
                    neighbor_has_local_hierarchy,
                ) in neighbors
                {
                    if *neighbor_path == schematic_path
                        && points_equal(*neighbor_anchor, component.anchor)
                    {
                        continue;
                    }

                    all_pins += neighbor_pin_count;
                    has_no_connect |= *neighbor_has_no_connect;

                    if *neighbor_path == schematic_path {
                        local_pins += neighbor_pin_count;
                        has_local_hierarchy |= *neighbor_has_local_hierarchy;
                    }
                }
            }
        }

        for label in &component.labels {
            if label.kind == LabelKind::Directive {
                continue;
            }

            if label.dangling
                || (label.kind == LabelKind::Local
                    && local_pins == 0
                    && all_pins > 1
                    && !has_no_connect
                    && !has_local_hierarchy)
                || (all_pins == 0 && !has_no_connect)
            {
                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    code: "erc-label-not-connected",
                    kind: crate::diagnostic::DiagnosticKind::Validation,
                    message: format!("Label is not connected at {}, {}", label.at[0], label.at[1]),
                    path: Some(schematic_path.clone()),
                    span: None,
                    line: None,
                    column: None,
                });
                continue;
            }

            if all_pins == 1 && !has_no_connect {
                diagnostics.push(Diagnostic {
                    severity: Severity::Warning,
                    code: "erc-label-single-pin",
                    kind: crate::diagnostic::DiagnosticKind::Validation,
                    message: format!(
                        "Label is connected to only one pin at {}, {}",
                        label.at[0], label.at[1]
                    ),
                    path: Some(schematic_path.clone()),
                    span: None,
                    line: None,
                    column: None,
                });
            }
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for `CONNECTION_GRAPH::ercCheckDirectiveLabels()`. This
// is not a 1:1 KiCad `SCH_TEXT::IsDangling()`/marker path because the Rust tree still runs through
// the shared reduced label-component snapshot instead of live graph-owned text items. It exists so
// directive labels now participate in the same shared connectivity owner as the other graph-backed
// label checks instead of remaining an uncovered `RunERC()` branch.
pub fn check_directive_labels(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };
        let sheet_path_prefix =
            reduced_net_name_sheet_path_prefix(&project.sheet_paths, sheet_path);

        for component in collect_reduced_label_component_snapshots(
            schematic,
            Some(&sheet_path_prefix),
            |label| shown_label_text(project, sheet_path, label),
        ) {
            for label in component
                .labels
                .iter()
                .filter(|label| label.kind == LabelKind::Directive && label.dangling)
            {
                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    code: "erc-label-not-connected",
                    kind: crate::diagnostic::DiagnosticKind::Validation,
                    message: format!(
                        "Directive label is not connected at {}, {}",
                        label.at[0], label.at[1]
                    ),
                    path: Some(sheet_path.schematic_path.clone()),
                    span: None,
                    line: None,
                    column: None,
                });
            }
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for the wire/bus-entry part of
// `CONNECTION_GRAPH::ercCheckDanglingWireEndpoints()`. This is not a 1:1 KiCad endpoint-owner path
// because the Rust tree still uses the shared reduced point snapshot instead of live dangling flags
// on `SCH_LINE` / bus-entry items. It exists so ERC now checks unconnected wire endpoints on the
// same shared connectivity carrier as the other graph-owned wire rules. Remaining divergence is
// fuller bus-layer ownership beyond the current shared segment carrier.
pub fn check_dangling_wire_endpoints(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };
        let points = collect_connection_points(schematic);

        for item in &schematic.screen.items {
            let (endpoints, is_bus_entry) = match item {
                SchItem::Wire(line) => (
                    [line.points.first().copied(), line.points.last().copied()]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>(),
                    false,
                ),
                SchItem::BusEntry(entry) => (
                    vec![
                        entry.at,
                        [entry.at[0] + entry.size[0], entry.at[1] + entry.size[1]],
                    ],
                    true,
                ),
                _ => continue,
            };

            for endpoint in endpoints {
                let is_dangling = points
                    .values()
                    .find(|point| points_equal(point.at, endpoint))
                    .is_some_and(|point| {
                        !point.members.iter().any(|member| {
                            !matches!(
                                member.kind,
                                ConnectionMemberKind::Wire | ConnectionMemberKind::BusEntry
                            ) || point
                                .members
                                .iter()
                                .filter(|member| {
                                    matches!(
                                        member.kind,
                                        ConnectionMemberKind::Wire | ConnectionMemberKind::BusEntry
                                    ) && points_equal(member.at, endpoint)
                                })
                                .count()
                                > 1
                        })
                    });

                if !is_dangling {
                    continue;
                }

                diagnostics.push(Diagnostic {
                    severity: Severity::Warning,
                    code: "erc-unconnected-wire-endpoint",
                    kind: crate::diagnostic::DiagnosticKind::Validation,
                    message: if is_bus_entry {
                        format!(
                            "Unconnected wire to bus entry at {}, {}",
                            endpoint[0], endpoint[1]
                        )
                    } else {
                        format!(
                            "Unconnected wire endpoint at {}, {}",
                            endpoint[0], endpoint[1]
                        )
                    },
                    path: Some(sheet_path.schematic_path.clone()),
                    span: None,
                    line: None,
                    column: None,
                });
            }
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for the wire/bus-entry part of
// `CONNECTION_GRAPH::ercCheckFloatingWires()`. This is not a 1:1 KiCad subgraph-driver pass
// because the Rust tree still treats "floating" as a shared connected wire component with no
// attached pins, labels, sheet pins, or no-connect markers rather than a full graph-owned driver
// object. It exists so the current ERC runner now flags reduced floating wire components instead of
// stopping at point-local endpoint warnings. Remaining divergence is fuller driver ownership and
// bus-layer semantics beyond the current shared segment carrier.
pub fn check_floating_wires(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };

        for component in collect_connection_components(schematic) {
            let has_wire = component.members.iter().any(|member| {
                matches!(
                    member.kind,
                    ConnectionMemberKind::Wire | ConnectionMemberKind::BusEntry
                )
            });

            if !has_wire {
                continue;
            }

            let has_connection_owner = component.members.iter().any(|member| {
                matches!(
                    member.kind,
                    ConnectionMemberKind::SymbolPin
                        | ConnectionMemberKind::SheetPin
                        | ConnectionMemberKind::Label
                        | ConnectionMemberKind::NoConnectMarker
                )
            });

            if has_connection_owner {
                continue;
            }

            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                code: "erc-wire-dangling",
                kind: crate::diagnostic::DiagnosticKind::Validation,
                message: format!(
                    "Floating wire component at {}, {}",
                    component.anchor[0], component.anchor[1]
                ),
                path: Some(sheet_path.schematic_path.clone()),
                span: None,
                line: None,
                column: None,
            });
        }
    }

    diagnostics
}

fn sheet_pin_is_dangling(schematic: &crate::model::Schematic, pin_at: [f64; 2]) -> bool {
    let Some(component) = collect_connection_components(schematic)
        .into_iter()
        .find(|component| {
            component.members.iter().any(|member| {
                member.kind == ConnectionMemberKind::SheetPin && points_equal(member.at, pin_at)
            })
        })
    else {
        return true;
    };

    !component.members.iter().any(|member| {
        !matches!(member.kind, ConnectionMemberKind::SheetPin) || !points_equal(member.at, pin_at)
    })
}

// Upstream parity: reduced local analogue for `CONNECTION_GRAPH::ercCheckHierSheets()`. This is
// not a 1:1 KiCad marker/`GetShownText()` path because the Rust tree still compares raw sheet-pin
// names, not full pin shown-text with all project text expansion, and it still uses reduced sheet-
// path helpers instead of KiCad sheet/screen owners. It exists so the current ERC runner now owns
// the same hierarchy-side checks: root hierarchical labels, dangling parent sheet pins, and
// parent/child sheet-pin name mismatches. Remaining divergence is fuller pin shown-text and marker
// attachment parity.
pub fn check_hierarchical_sheets(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };

        if project
            .parent_sheet_path(&sheet_path.instance_path)
            .is_none()
        {
            for item in &schematic.screen.items {
                let SchItem::Label(label) = item else {
                    continue;
                };

                if label.kind != LabelKind::Hierarchical {
                    continue;
                }

                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    code: "erc-pin-not-connected",
                    kind: crate::diagnostic::DiagnosticKind::Validation,
                    message: format!(
                        "Hierarchical label '{}' in root sheet cannot be connected to non-existent parent sheet",
                        shown_label_text(project, sheet_path, label)
                    ),
                    path: Some(sheet_path.schematic_path.clone()),
                    span: None,
                    line: None,
                    column: None,
                });
            }
        }

        for item in &schematic.screen.items {
            let SchItem::Sheet(sheet) = item else {
                continue;
            };

            for pin in &sheet.pins {
                if sheet_pin_is_dangling(schematic, pin.at) {
                    diagnostics.push(Diagnostic {
                        severity: Severity::Error,
                        code: "erc-pin-not-connected",
                        kind: crate::diagnostic::DiagnosticKind::Validation,
                        message: format!("Sheet pin '{}' is not connected", pin.name),
                        path: Some(sheet_path.schematic_path.clone()),
                        span: None,
                        line: None,
                        column: None,
                    });
                }
            }

            let Some(_) = sheet.uuid.as_deref() else {
                continue;
            };
            let Some(child_sheet_path) = child_sheet_path_for_sheet(project, sheet_path, sheet)
            else {
                continue;
            };
            let Some(child_schematic) = project.schematic(&child_sheet_path.schematic_path) else {
                continue;
            };

            let mut pins = BTreeMap::new();
            for pin in &sheet.pins {
                pins.insert(pin.name.clone(), pin);
            }

            let mut child_labels = BTreeMap::new();
            for sub_item in &child_schematic.screen.items {
                let SchItem::Label(label) = sub_item else {
                    continue;
                };

                if label.kind != LabelKind::Hierarchical {
                    continue;
                }

                let label_text = shown_label_text(project, child_sheet_path, label);

                if pins.remove(&label_text).is_none() {
                    child_labels.insert(label_text, label);
                }
            }

            for (name, _) in pins {
                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    code: "erc-hierarchical-label-mismatch",
                    kind: crate::diagnostic::DiagnosticKind::Validation,
                    message: format!(
                        "Sheet pin {name} has no matching hierarchical label inside the sheet"
                    ),
                    path: Some(sheet_path.schematic_path.clone()),
                    span: None,
                    line: None,
                    column: None,
                });
            }

            for (name, _) in child_labels {
                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    code: "erc-hierarchical-label-mismatch",
                    kind: crate::diagnostic::DiagnosticKind::Validation,
                    message: format!(
                        "Hierarchical label {name} has no matching sheet pin in the parent sheet"
                    ),
                    path: Some(child_sheet_path.schematic_path.clone()),
                    span: None,
                    line: None,
                    column: None,
                });
            }
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for `CONNECTION_GRAPH::ercCheckBusToNetConflicts()`.
// This is not a 1:1 KiCad subgraph/`SCH_CONNECTION` pass because the Rust tree still classifies
// bus-vs-net ownership from line kinds and reduced shown-text instead of full bus-member
// connections. It exists so the current graph-backed ERC runner now flags connected bus/net mixes
// on the shared reduced component owner rather than leaving the branch entirely unported. Remaining
// divergence is fuller member-aware bus semantics.
pub fn check_bus_to_net_conflicts(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };

        for component in collect_connection_components(schematic) {
            if component
                .members
                .iter()
                .any(|member| member.kind == ConnectionMemberKind::BusEntry)
            {
                continue;
            }

            let mut has_bus_item = false;
            let mut has_net_item = false;
            let mut net_at = None;

            for item in &schematic.screen.items {
                match item {
                    SchItem::Wire(line) | SchItem::Bus(line)
                        if component_contains_line_kind(&component, line) =>
                    {
                        match line.kind {
                            crate::model::LineKind::Bus => has_bus_item = true,
                            crate::model::LineKind::Wire => {
                                has_net_item = true;
                                net_at.get_or_insert_with(|| {
                                    line.points.first().copied().unwrap_or(component.anchor)
                                });
                            }
                            crate::model::LineKind::Polyline => {}
                        }
                    }
                    SchItem::Label(label)
                        if component.members.iter().any(|member| {
                            member.kind == ConnectionMemberKind::Label
                                && points_equal(member.at, label.at)
                        }) =>
                    {
                        if label.kind == LabelKind::Directive {
                            continue;
                        }

                        if reduced_text_is_bus(
                            schematic,
                            &shown_label_text(project, sheet_path, label),
                        ) {
                            has_bus_item = true;
                        } else {
                            has_net_item = true;
                            net_at.get_or_insert(label.at);
                        }
                    }
                    SchItem::Sheet(sheet) => {
                        for pin in &sheet.pins {
                            if !component.members.iter().any(|member| {
                                member.kind == ConnectionMemberKind::SheetPin
                                    && points_equal(member.at, pin.at)
                            }) {
                                continue;
                            }

                            if reduced_text_is_bus(schematic, &pin.name) {
                                has_bus_item = true;
                            } else {
                                has_net_item = true;
                                net_at.get_or_insert(pin.at);
                            }
                        }
                    }
                    _ => {}
                }
            }

            if has_bus_item && has_net_item {
                let report_at = net_at.unwrap_or(component.anchor);
                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    code: "erc-bus-to-net-conflict",
                    kind: crate::diagnostic::DiagnosticKind::Validation,
                    message: format!(
                        "Bus and net items are graphically connected at {}, {}",
                        report_at[0], report_at[1]
                    ),
                    path: Some(sheet_path.schematic_path.clone()),
                    span: None,
                    line: None,
                    column: None,
                });
            }
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for `CONNECTION_GRAPH::ercCheckBusToBusConflicts()`.
// This is not a 1:1 KiCad bus-member connection pass because the Rust tree still expands only
// reduced alias/vector members instead of full `SCH_CONNECTION::Members()` trees. It exists so the
// graph-backed ERC runner now flags bus label/port pairs on one connected component when their
// reduced member-name sets do not overlap. Remaining divergence is fuller nested bus-member
// semantics beyond this reduced name-only overlap check.
pub fn check_bus_to_bus_conflicts(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };

        for component in collect_connection_components(schematic) {
            let mut label_members = None::<Vec<String>>;
            let mut label_at = None::<[f64; 2]>;
            let mut port_members = None::<Vec<String>>;

            for item in &schematic.screen.items {
                match item {
                    SchItem::Label(label)
                        if component.members.iter().any(|member| {
                            member.kind == ConnectionMemberKind::Label
                                && points_equal(member.at, label.at)
                        }) && matches!(label.kind, LabelKind::Local | LabelKind::Global) =>
                    {
                        let shown = shown_label_text(project, sheet_path, label);
                        if reduced_text_is_bus(schematic, &shown) {
                            label_at.get_or_insert(label.at);
                            label_members
                                .get_or_insert_with(|| reduced_bus_members(schematic, &shown));
                        }
                    }
                    SchItem::Label(label)
                        if component.members.iter().any(|member| {
                            member.kind == ConnectionMemberKind::Label
                                && points_equal(member.at, label.at)
                        }) && label.kind == LabelKind::Hierarchical =>
                    {
                        let shown = shown_label_text(project, sheet_path, label);
                        if reduced_text_is_bus(schematic, &shown) {
                            port_members
                                .get_or_insert_with(|| reduced_bus_members(schematic, &shown));
                        }
                    }
                    SchItem::Sheet(sheet) => {
                        for pin in &sheet.pins {
                            if !component.members.iter().any(|member| {
                                member.kind == ConnectionMemberKind::SheetPin
                                    && points_equal(member.at, pin.at)
                            }) {
                                continue;
                            }

                            if reduced_text_is_bus(schematic, &pin.name) {
                                port_members.get_or_insert_with(|| {
                                    reduced_bus_members(schematic, &pin.name)
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }

            let (Some(label_members), Some(port_members), Some(label_at)) =
                (label_members, port_members, label_at)
            else {
                continue;
            };

            let has_match = label_members
                .iter()
                .any(|member| port_members.iter().any(|test| test == member));

            if has_match {
                continue;
            }

            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                code: "erc-bus-to-bus-conflict",
                kind: crate::diagnostic::DiagnosticKind::Validation,
                message: format!(
                    "Bus label and port do not share members at {}, {}",
                    label_at[0], label_at[1]
                ),
                path: Some(sheet_path.schematic_path.clone()),
                span: None,
                line: None,
                column: None,
            });
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for `CONNECTION_GRAPH::ercCheckBusToBusEntryConflicts()`.
// This is not a 1:1 KiCad driver/subgraph pass because the Rust tree still derives bus members and
// net drivers from reduced shown-text carriers instead of `SCH_CONNECTION` plus connected-bus-item
// ownership. It now mirrors the exercised KiCad flow for both the member test and the follow-on
// suppression branch where a higher-priority global label or power pin overrides the bus member.
// Remaining divergence is fuller bus-object ownership and cached subgraph driver state.
pub fn check_bus_to_bus_entry_conflicts(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };

        for component in collect_connection_components(schematic) {
            if !component
                .members
                .iter()
                .any(|member| member.kind == ConnectionMemberKind::BusEntry)
            {
                continue;
            }

            let mut bus_name = None::<String>;
            let mut bus_members = None::<Vec<String>>;
            let mut net_names = Vec::<String>::new();
            let mut entry_at = component
                .members
                .iter()
                .find(|member| member.kind == ConnectionMemberKind::BusEntry)
                .map(|member| member.at)
                .unwrap_or(component.anchor);

            for item in &schematic.screen.items {
                match item {
                    SchItem::Bus(line) if component_contains_line_kind(&component, line) => {}
                    SchItem::Label(label)
                        if component.members.iter().any(|member| {
                            member.kind == ConnectionMemberKind::Label
                                && points_equal(member.at, label.at)
                        }) =>
                    {
                        if label.kind == LabelKind::Directive {
                            continue;
                        }

                        let shown = shown_label_text(project, sheet_path, label);

                        if reduced_text_is_bus(schematic, &shown) {
                            bus_name.get_or_insert(shown.clone());
                            bus_members
                                .get_or_insert_with(|| reduced_bus_members(schematic, &shown));
                        } else {
                            net_names.push(shown);
                            entry_at = label.at;
                        }
                    }
                    SchItem::Sheet(sheet) => {
                        for pin in &sheet.pins {
                            if !component.members.iter().any(|member| {
                                member.kind == ConnectionMemberKind::SheetPin
                                    && points_equal(member.at, pin.at)
                            }) {
                                continue;
                            }

                            if reduced_text_is_bus(schematic, &pin.name) {
                                bus_name.get_or_insert(pin.name.clone());
                                bus_members.get_or_insert_with(|| {
                                    reduced_bus_members(schematic, &pin.name)
                                });
                            } else {
                                net_names.push(pin.name.clone());
                                entry_at = pin.at;
                            }
                        }
                    }
                    _ => {}
                }
            }

            let (Some(bus_name), Some(bus_members)) = (bus_name, bus_members) else {
                continue;
            };

            let suppress_conflict =
                resolve_reduced_non_bus_driver_priority_at(schematic, component.anchor, |label| {
                    shown_label_text(project, sheet_path, label)
                })
                .is_some_and(|priority| priority >= 6);

            for net_name in net_names {
                if bus_members.iter().any(|member| member == &net_name) {
                    continue;
                }

                if suppress_conflict {
                    continue;
                }

                diagnostics.push(Diagnostic {
                    severity: Severity::Warning,
                    code: "erc-bus-entry-conflict",
                    kind: crate::diagnostic::DiagnosticKind::Validation,
                    message: format!(
                        "Net {net_name} is graphically connected to bus {bus_name} but is not a member of that bus at {}, {}",
                        entry_at[0], entry_at[1]
                    ),
                    path: Some(sheet_path.schematic_path.clone()),
                    span: None,
                    line: None,
                    column: None,
                });
            }
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for `ERC_TESTER::TestPinToPin()`. This is not a 1:1
// KiCad pin-matrix runner because the Rust tree still lacks full `ERC_SETTINGS`, graph-owned pin
// contexts, marker placement heuristics, and the full connection graph. It now runs over the
// shared reduced project net map like upstream `m_nets` instead of per-sheet connection
// components, and applies the typed companion-project `erc.pin_map` override slice on top of the
// upstream default matrix instead of hard-coding only the defaults. Remaining divergence is richer
// settings, driver-missing reporting, and full subgraph ownership.
pub fn check_pin_to_pin(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for net in collect_reduced_project_net_map(project, false) {
        let mut pins = net
            .base_pins
            .iter()
            .filter_map(|base_pin| resolve_base_pin_type(project, base_pin))
            .collect::<Vec<_>>();

        if pins.len() < 2 {
            continue;
        }

        pins.sort_by(|lhs, rhs| {
            let mut ordering = lhs
                .reference
                .to_ascii_uppercase()
                .cmp(&rhs.reference.to_ascii_uppercase());
            if ordering == std::cmp::Ordering::Equal {
                ordering = lhs.pin_number.cmp(&rhs.pin_number);
            }
            if ordering == std::cmp::Ordering::Equal {
                ordering = lhs.at[0].to_bits().cmp(&rhs.at[0].to_bits());
            }
            if ordering == std::cmp::Ordering::Equal {
                ordering = lhs.at[1].to_bits().cmp(&rhs.at[1].to_bits());
            }
            ordering
        });

        let is_power_net = pins
            .iter()
            .any(|pin| pin.pin_type == ReducedPinType::PowerIn);
        let has_driver = pins.iter().any(|pin| {
            if is_power_net {
                is_power_driver_pin_type(pin.pin_type)
            } else {
                is_normal_driver_pin_type(pin.pin_type)
            }
        });

        for (index, lhs_pin) in pins.iter().enumerate() {
            for rhs_pin in pins.iter().skip(index + 1) {
                let conflict = configured_pin_conflict(project, lhs_pin.pin_type, rhs_pin.pin_type);
                if conflict == PinConflict::Ok {
                    continue;
                }

                diagnostics.push(Diagnostic {
                    severity: match conflict {
                        PinConflict::Warning => Severity::Warning,
                        PinConflict::Error => Severity::Error,
                        PinConflict::Ok => continue,
                    },
                    code: match conflict {
                        PinConflict::Warning => "erc-pin-to-pin-warning",
                        PinConflict::Error => "erc-pin-to-pin-error",
                        PinConflict::Ok => continue,
                    },
                    kind: crate::diagnostic::DiagnosticKind::Validation,
                    message: format!(
                        "Conflicting pins connected at {}, {}",
                        lhs_pin.at[0], lhs_pin.at[1]
                    ),
                    path: Some(lhs_pin.path.clone()),
                    span: None,
                    line: None,
                    column: None,
                });
                break;
            }
        }

        if has_driver || net.has_no_connect {
            continue;
        }

        if let Some(pin) = pins.iter().find(|pin| is_driven_pin_type(pin.pin_type)) {
            let article = if pin.pin_type == ReducedPinType::PowerIn {
                "Power input pin is not driven"
            } else {
                "Input pin is not driven"
            };

            diagnostics.push(Diagnostic {
                severity: Severity::Warning,
                code: "erc-missing-driver",
                kind: crate::diagnostic::DiagnosticKind::Validation,
                message: article.to_string(),
                path: Some(pin.path.clone()),
                span: None,
                line: None,
                column: None,
            });
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for `CONNECTION_GRAPH::ercCheckMultipleDrivers()`. This
// is not a 1:1 KiCad subgraph-marker pass because the Rust tree still lacks full subgraph objects
// and marker-owned item identity, but it reports the exercised connection-graph rule: when two
// different strong driver names resolve on one connected component, the winning shared driver name
// is reported and the lower-priority name is flagged. Remaining divergence is fuller bus/power
// subgraph coverage and exact marker attachment.
pub fn check_driver_conflicts(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };

        for component in collect_connection_components(schematic) {
            let Some((primary_name, secondary_name)) =
                resolve_reduced_driver_conflict_at(schematic, component.anchor, |label| {
                    shown_label_text(project, sheet_path, label)
                })
            else {
                continue;
            };

            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                code: "erc-driver-conflict",
                kind: crate::diagnostic::DiagnosticKind::Validation,
                message: format!(
                    "Both {primary_name} and {secondary_name} are attached to the same items; {primary_name} will be used in the netlist"
                ),
                path: Some(schematic.path.clone()),
                span: None,
                line: None,
                column: None,
            });
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for `ERC_TESTER::TestMultUnitPinConflicts()`. This is
// not a 1:1 KiCad marker pass because the Rust tree still uses projected lib pins plus the shared
// reduced pin-net lookup instead of live `CONNECTION_SUBGRAPH`-owned `SCH_PIN` items, but it now
// prefers the shared project graph owner before falling back to the older point-net resolver where
// reduced item identity is still incomplete. Remaining divergence is fuller graph ownership and
// KiCad marker attachment.
pub fn check_mult_unit_pin_conflicts(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut pin_to_net = BTreeMap::<String, String>::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };
        for item in &schematic.screen.items {
            let SchItem::Symbol(symbol) = item else {
                continue;
            };

            let Some(lib_symbol) = symbol.lib_symbol.as_ref() else {
                continue;
            };

            let unit_count = lib_symbol
                .units
                .iter()
                .map(|unit| unit.unit_number)
                .collect::<std::collections::BTreeSet<_>>()
                .len();

            if unit_count < 2 {
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
                let Some(pin_number) = pin.number else {
                    continue;
                };

                let net_name = resolved_pin_net_name(
                    project,
                    sheet_path,
                    schematic,
                    symbol,
                    pin.at,
                    pin.name.as_deref(),
                );
                let key = format!("{reference}:{pin_number}");

                if let Some(existing_net) = pin_to_net.get(&key) {
                    if *existing_net != net_name {
                        diagnostics.push(Diagnostic {
                            severity: Severity::Error,
                            code: "erc-different-unit-net",
                            kind: crate::diagnostic::DiagnosticKind::Validation,
                            message: format!(
                                "Pin {} is connected to both {} and {}",
                                pin_number, net_name, existing_net
                            ),
                            path: Some(sheet_path.schematic_path.clone()),
                            span: None,
                            line: None,
                            column: None,
                        });
                    }
                } else {
                    pin_to_net.insert(key, net_name);
                }
            }
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for `ERC_TESTER::TestDuplicatePinNets()`. This is not a
// 1:1 KiCad marker pass because the Rust tree still groups projected lib pins by the shared
// reduced pin-net lookup instead of live `SCH_PIN::Connection()` objects, but it now prefers the
// shared project graph owner before falling back to the older point-net resolver where reduced
// item identity is still incomplete. It preserves the exercised rule: duplicate numbered pins on
// the same placed symbol must not resolve to different nets unless the lib symbol explicitly
// treats duplicate numbers as jumper pins. Remaining divergence is fuller connection-graph
// ownership and KiCad marker/item attachment.
pub fn check_duplicate_pin_nets(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };
        for item in &schematic.screen.items {
            let SchItem::Symbol(symbol) = item else {
                continue;
            };

            let Some(lib_symbol) = symbol.lib_symbol.as_ref() else {
                continue;
            };

            if lib_symbol.duplicate_pin_numbers_are_jumpers {
                continue;
            }

            let state = resolved_symbol_text_state(
                symbol,
                &sheet_path.instance_path,
                project.current_variant(),
            );
            let reference = resolved_property_value(&state.properties, "Reference")
                .unwrap_or_else(|| "?".to_string());

            let mut pins_by_number = BTreeMap::<String, Vec<(Option<String>, String)>>::new();

            for pin in projected_symbol_pin_info(symbol) {
                let Some(pin_number) = pin.number else {
                    continue;
                };

                let net_name = resolved_pin_net_name(
                    project,
                    sheet_path,
                    schematic,
                    symbol,
                    pin.at,
                    pin.name.as_deref(),
                );

                pins_by_number
                    .entry(pin_number)
                    .or_default()
                    .push((pin.name, net_name));
            }

            for (pin_number, pin_net_pairs) in pins_by_number {
                if pin_net_pairs.len() < 2 {
                    continue;
                }

                let first_net = pin_net_pairs[0].1.clone();
                let first_display = if first_net.is_empty() {
                    "<no net>".to_string()
                } else {
                    first_net.clone()
                };

                let mut conflict_net = None;

                for (_, net_name) in pin_net_pairs.iter().skip(1) {
                    if *net_name != first_net {
                        conflict_net = Some(net_name.clone());
                        break;
                    }
                }

                let Some(conflict_net) = conflict_net else {
                    continue;
                };

                let conflict_display = if conflict_net.is_empty() {
                    "<no net>".to_string()
                } else {
                    conflict_net
                };

                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    code: "erc-duplicate-pin-nets",
                    kind: crate::diagnostic::DiagnosticKind::Validation,
                    message: format!(
                        "Pin {} on symbol '{}' is connected to different nets: {} and {}",
                        pin_number, reference, first_display, conflict_display
                    ),
                    path: Some(sheet_path.schematic_path.clone()),
                    span: None,
                    line: None,
                    column: None,
                });
            }
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for `CONNECTION_GRAPH::ercCheckSingleGlobalLabel()`.
// This is not a 1:1 KiCad marker/severity path because the Rust tree still lacks ERC settings and
// marker-owned item attachment. It exists so the current ERC runner checks the same shown-text
// uniqueness rule across the loaded sheet list instead of leaving single global labels unchecked.
// Remaining divergence is configurable severity/default-ignore handling.
pub fn check_single_global_labels(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut label_data = BTreeMap::<String, (usize, Option<std::path::PathBuf>)>::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };
        for item in &schematic.screen.items {
            let SchItem::Label(label) = item else {
                continue;
            };

            if label.kind != LabelKind::Global {
                continue;
            }

            let shown_text = shown_label_text(project, sheet_path, label);
            let entry = label_data
                .entry(shown_text)
                .or_insert_with(|| (0, Some(sheet_path.schematic_path.clone())));
            entry.0 += 1;

            if entry.0 > 1 {
                entry.1 = None;
            }
        }
    }

    for (shown_text, (count, path)) in label_data {
        if count != 1 {
            continue;
        }

        diagnostics.push(Diagnostic {
            severity: Severity::Warning,
            code: "erc-single-global-label",
            kind: crate::diagnostic::DiagnosticKind::Validation,
            message: format!("Global label '{}' appears only once", shown_text),
            path,
            span: None,
            line: None,
            column: None,
        });
    }

    diagnostics
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SimilarLabelItemKind {
    LocalLabel,
    Label,
    Power,
}

#[derive(Clone, Debug)]
struct SimilarLabelEntry {
    kind: SimilarLabelItemKind,
    shown_text: String,
    path: std::path::PathBuf,
}

// Upstream parity: reduced local analogue for `ERC_TESTER::TestSimilarLabels()`. This is not a
// 1:1 KiCad marker pass because the Rust tree still compares reduced label/power snapshots instead
// of `CONNECTION_SUBGRAPH` items and marker objects, but it preserves the exercised normalized
// label/power collision rules, including the "similar local labels on different sheets are fine"
// exception. Remaining divergence is broader connection-graph participation and marker selection.
pub fn check_similar_labels(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen: BTreeMap<String, Vec<SimilarLabelEntry>> = BTreeMap::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };
        for item in &schematic.screen.items {
            match item {
                SchItem::Label(label)
                    if matches!(
                        label.kind,
                        LabelKind::Local | LabelKind::Global | LabelKind::Hierarchical
                    ) =>
                {
                    let shown_text = shown_label_text(project, sheet_path, label);
                    let normalized = shown_text.to_ascii_lowercase();
                    let kind = if label.kind == LabelKind::Local {
                        SimilarLabelItemKind::LocalLabel
                    } else {
                        SimilarLabelItemKind::Label
                    };

                    if let Some(existing) = seen.get(&normalized) {
                        for other in existing {
                            if shown_text == other.shown_text {
                                continue;
                            }

                            if kind == SimilarLabelItemKind::LocalLabel
                                && other.kind == SimilarLabelItemKind::LocalLabel
                                && sheet_path.schematic_path != other.path
                            {
                                continue;
                            }

                            let (code, message) = match (kind, other.kind) {
                                (SimilarLabelItemKind::Power, SimilarLabelItemKind::Power) => (
                                    "erc-similar-power",
                                    format!(
                                        "Similar power names differ only by case: '{}' and '{}'",
                                        shown_text, other.shown_text
                                    ),
                                ),
                                (SimilarLabelItemKind::Power, _)
                                | (_, SimilarLabelItemKind::Power) => (
                                    "erc-similar-label-and-power",
                                    format!(
                                        "Similar label and power names differ only by case: '{}' and '{}'",
                                        shown_text, other.shown_text
                                    ),
                                ),
                                _ => (
                                    "erc-similar-labels",
                                    format!(
                                        "Similar labels differ only by case: '{}' and '{}'",
                                        shown_text, other.shown_text
                                    ),
                                ),
                            };

                            diagnostics.push(Diagnostic {
                                severity: Severity::Warning,
                                code,
                                kind: crate::diagnostic::DiagnosticKind::Validation,
                                message,
                                path: Some(sheet_path.schematic_path.clone()),
                                span: None,
                                line: None,
                                column: None,
                            });
                        }
                    }

                    seen.entry(normalized).or_default().push(SimilarLabelEntry {
                        kind,
                        shown_text,
                        path: sheet_path.schematic_path.clone(),
                    });
                }
                SchItem::Symbol(symbol)
                    if symbol
                        .lib_symbol
                        .as_ref()
                        .is_some_and(|lib_symbol| lib_symbol.power) =>
                {
                    let Some(shown_text) = shown_symbol_value_text(project, sheet_path, symbol)
                    else {
                        continue;
                    };
                    let normalized = shown_text.to_ascii_lowercase();

                    if let Some(existing) = seen.get(&normalized) {
                        for other in existing {
                            if shown_text == other.shown_text {
                                continue;
                            }

                            let (code, message) = match other.kind {
                                SimilarLabelItemKind::Power => (
                                    "erc-similar-power",
                                    format!(
                                        "Similar power names differ only by case: '{}' and '{}'",
                                        shown_text, other.shown_text
                                    ),
                                ),
                                _ => (
                                    "erc-similar-label-and-power",
                                    format!(
                                        "Similar label and power names differ only by case: '{}' and '{}'",
                                        shown_text, other.shown_text
                                    ),
                                ),
                            };

                            diagnostics.push(Diagnostic {
                                severity: Severity::Warning,
                                code,
                                kind: crate::diagnostic::DiagnosticKind::Validation,
                                message,
                                path: Some(sheet_path.schematic_path.clone()),
                                span: None,
                                line: None,
                                column: None,
                            });
                        }
                    }

                    seen.entry(normalized).or_default().push(SimilarLabelEntry {
                        kind: SimilarLabelItemKind::Power,
                        shown_text,
                        path: sheet_path.schematic_path.clone(),
                    });
                }
                _ => {}
            }
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for `ERC_TESTER::TestSameLocalGlobalLabel()`. This is
// not a 1:1 KiCad marker pass because the Rust tree still compares current shown-text snapshots
// directly instead of subgraph-owned label items, but it preserves the exercised local-vs-global
// name collision rule across the loaded hierarchy. Remaining divergence is fuller connection-graph
// ownership and marker sheet-path metadata.
pub fn check_same_local_global_label(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut globals = BTreeMap::<String, std::path::PathBuf>::new();
    let mut locals = BTreeMap::<String, std::path::PathBuf>::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };
        for item in &schematic.screen.items {
            let SchItem::Label(label) = item else {
                continue;
            };

            let shown_text = shown_label_text(project, sheet_path, label);

            match label.kind {
                LabelKind::Global => {
                    globals
                        .entry(shown_text)
                        .or_insert_with(|| sheet_path.schematic_path.clone());
                }
                LabelKind::Local => {
                    locals
                        .entry(shown_text)
                        .or_insert_with(|| sheet_path.schematic_path.clone());
                }
                _ => {}
            }
        }
    }

    globals
        .into_iter()
        .filter_map(|(shown_text, path)| {
            locals.get(&shown_text).map(|_| Diagnostic {
                severity: Severity::Error,
                code: "erc-same-local-global-label",
                kind: crate::diagnostic::DiagnosticKind::Validation,
                message: format!(
                    "Local and global labels share the same shown text: '{}'",
                    shown_text
                ),
                path: Some(path),
                span: None,
                line: None,
                column: None,
            })
        })
        .collect()
}

// Upstream parity: reduced local analogue for `ERC_TESTER::TestFootprintFilters()`. This is not a
// 1:1 KiCad marker pass because the Rust tree still lacks full `LIB_ID` parsing and marker-owned
// symbol metadata, but it preserves the exercised footprint-filter matching flow on shown
// footprint text and library filters instead of dropping the rule entirely.
pub fn check_footprint_filters(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };
        for item in &schematic.screen.items {
            let SchItem::Symbol(symbol) = item else {
                continue;
            };

            let Some(lib_symbol) = symbol.lib_symbol.as_ref() else {
                continue;
            };

            if lib_symbol.fp_filters.is_empty() {
                continue;
            }

            let Some(footprint_property) = symbol
                .properties
                .iter()
                .find(|property| property.kind == PropertyKind::SymbolFootprint)
            else {
                continue;
            };

            let footprint =
                shown_symbol_property_text(project, sheet_path, symbol, footprint_property);
            let lower_id = footprint.to_ascii_lowercase();
            let Some((_, item_name)) = lower_id.rsplit_once(':') else {
                continue;
            };

            let found = lib_symbol.fp_filters.iter().any(|filter| {
                let filter = filter.to_ascii_lowercase();
                if filter.contains(':') {
                    wildcard_matches(&filter, &lower_id)
                } else {
                    wildcard_matches(&filter, item_name)
                }
            });

            if found {
                continue;
            }

            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                code: "erc-footprint-link-issues",
                kind: crate::diagnostic::DiagnosticKind::Validation,
                message: format!(
                    "Assigned footprint ({}) doesn't match footprint filters ({})",
                    item_name,
                    lib_symbol.fp_filters.join(" ")
                ),
                path: Some(sheet_path.schematic_path.clone()),
                span: None,
                line: None,
                column: None,
            });
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for `ERC_TESTER::TestStackedPinNotation()`. This is not
// a 1:1 KiCad marker pass because the Rust tree still validates projected lib-pin numbers instead
// of live `SCH_PIN` objects, but it preserves the exercised bracketed stacked-pin syntax rule and
// only warns on numbers that resemble stacked notation but do not parse like KiCad's helper.
pub fn check_stacked_pin_notation(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };
        for item in &schematic.screen.items {
            let SchItem::Symbol(symbol) = item else {
                continue;
            };

            for pin in projected_symbol_pin_info(symbol) {
                let Some(number) = pin.number.as_deref() else {
                    continue;
                };

                if stacked_pin_notation_is_valid(number) {
                    continue;
                }

                diagnostics.push(Diagnostic {
                    severity: Severity::Warning,
                    code: "erc-stacked-pin-syntax",
                    kind: crate::diagnostic::DiagnosticKind::Validation,
                    message: format!(
                        "Pin number resembles stacked pin notation but is invalid: '{}'",
                        number
                    ),
                    path: Some(sheet_path.schematic_path.clone()),
                    span: None,
                    line: None,
                    column: None,
                });
                break;
            }
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for `ERC_TESTER::TestGroundPins()`. This is not a 1:1
// KiCad marker pass because the Rust tree still checks projected lib pins through the shared
// reduced pin-net lookup instead of live `SCH_PIN` connections, but it now prefers the shared
// project graph owner before falling back to the older point-net resolver where reduced item
// identity is still incomplete. It preserves the exercised rule: once a symbol has a real ground
// net, any `GND`-named power pin on a different net is an ERC error. Remaining divergence is
// fuller connection-graph ownership and richer pin metadata.
pub fn check_ground_pins(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };
        for item in &schematic.screen.items {
            let SchItem::Symbol(symbol) = item else {
                continue;
            };

            let mut has_ground_net = false;
            let mut mismatched_pins = Vec::new();

            for pin in projected_symbol_pin_info(symbol) {
                let Some(pin_type) = pin.electrical_type.as_deref() else {
                    continue;
                };

                if !matches!(pin_type, "power_in" | "power_out") {
                    continue;
                }

                let net_name = resolved_pin_net_name(
                    project,
                    sheet_path,
                    schematic,
                    symbol,
                    pin.at,
                    pin.name.as_deref(),
                );
                let net_is_ground = net_name.to_ascii_uppercase().contains("GND");

                if net_is_ground {
                    has_ground_net = true;
                }

                if pin
                    .name
                    .as_deref()
                    .is_some_and(|name| name.to_ascii_uppercase().contains("GND"))
                    && !net_is_ground
                {
                    mismatched_pins.push((pin.name.unwrap_or_default(), pin.at));
                }
            }

            if !has_ground_net {
                continue;
            }

            for (pin_name, _pin_at) in mismatched_pins {
                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    code: "erc-ground-pin-not-ground",
                    kind: crate::diagnostic::DiagnosticKind::Validation,
                    message: format!("Pin {} not connected to ground net", pin_name),
                    path: Some(sheet_path.schematic_path.clone()),
                    span: None,
                    line: None,
                    column: None,
                });
            }
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for `ERC_TESTER::TestOffGridEndpoints()`. This is not a
// 1:1 KiCad marker pass because the Rust tree still checks reduced wire/bus-entry endpoints and
// projected lib pins in millimeter coordinates instead of live schematic items in KiCad IU, but it
// preserves the exercised rule: connectable wire endpoints, bus-entry endpoints, and non-NC symbol
// pins must land on the typed schematic connection grid from companion project settings. Remaining
// divergence is fuller item coverage and KiCad marker attachment.
pub fn check_off_grid_endpoints(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let grid_size_mm = project
        .project
        .as_ref()
        .map(|project| project.schematic.connection_grid_size_mm)
        .unwrap_or(1.27);

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };

        for item in &schematic.screen.items {
            match item {
                SchItem::Wire(line) => {
                    if let Some(point) = line
                        .points
                        .first()
                        .copied()
                        .filter(|point| !point_is_on_grid(*point, grid_size_mm))
                        .or_else(|| {
                            line.points
                                .last()
                                .copied()
                                .filter(|point| !point_is_on_grid(*point, grid_size_mm))
                        })
                    {
                        diagnostics.push(Diagnostic {
                            severity: Severity::Warning,
                            code: "erc-endpoint-off-grid",
                            kind: crate::diagnostic::DiagnosticKind::Validation,
                            message: format!(
                                "Endpoint off connection grid at {}, {}",
                                point[0], point[1]
                            ),
                            path: Some(schematic.path.clone()),
                            span: None,
                            line: None,
                            column: None,
                        });
                    }
                }
                SchItem::BusEntry(entry) => {
                    for point in [
                        entry.at,
                        [entry.at[0] + entry.size[0], entry.at[1] + entry.size[1]],
                    ] {
                        if point_is_on_grid(point, grid_size_mm) {
                            continue;
                        }

                        diagnostics.push(Diagnostic {
                            severity: Severity::Warning,
                            code: "erc-endpoint-off-grid",
                            kind: crate::diagnostic::DiagnosticKind::Validation,
                            message: format!(
                                "Endpoint off connection grid at {}, {}",
                                point[0], point[1]
                            ),
                            path: Some(schematic.path.clone()),
                            span: None,
                            line: None,
                            column: None,
                        });
                    }
                }
                SchItem::Symbol(symbol) => {
                    if let Some(point) = projected_symbol_pin_info(symbol)
                        .into_iter()
                        .find(|pin| {
                            pin.electrical_type.as_deref() != Some("no_connect")
                                && !point_is_on_grid(pin.at, grid_size_mm)
                        })
                        .map(|pin| pin.at)
                    {
                        diagnostics.push(Diagnostic {
                            severity: Severity::Warning,
                            code: "erc-endpoint-off-grid",
                            kind: crate::diagnostic::DiagnosticKind::Validation,
                            message: format!(
                                "Endpoint off connection grid at {}, {}",
                                point[0], point[1]
                            ),
                            path: Some(schematic.path.clone()),
                            span: None,
                            line: None,
                            column: None,
                        });
                    }
                }
                _ => {}
            }
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for `ERC_TESTER::TestFieldNameWhitespace()`. This is
// not a 1:1 KiCad marker pass because the Rust tree still reports plain diagnostics instead of
// `SCH_MARKER` / `ERC_ITEM`, but it preserves the same exercised symbol/sheet field-name
// whitespace rule and message text. Remaining divergence is richer sheet-path marker context.
pub fn check_field_name_whitespace(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for schematic in &project.schematics {
        for item in &schematic.screen.items {
            match item {
                SchItem::Symbol(symbol) => {
                    for property in &symbol.properties {
                        let trimmed = property.key.trim();

                        if property.key != trimmed {
                            diagnostics.push(Diagnostic {
                                severity: Severity::Warning,
                                code: "erc-field-name-whitespace",
                                kind: crate::diagnostic::DiagnosticKind::Validation,
                                message: format!(
                                    "Field name has leading or trailing whitespace: '{}'",
                                    property.key
                                ),
                                path: Some(schematic.path.clone()),
                                span: None,
                                line: None,
                                column: None,
                            });
                        }
                    }
                }
                SchItem::Sheet(sheet) => {
                    for property in &sheet.properties {
                        let trimmed = property.key.trim();

                        if property.key != trimmed {
                            diagnostics.push(Diagnostic {
                                severity: Severity::Warning,
                                code: "erc-field-name-whitespace",
                                kind: crate::diagnostic::DiagnosticKind::Validation,
                                message: format!(
                                    "Field name has leading or trailing whitespace: '{}'",
                                    property.key
                                ),
                                path: Some(schematic.path.clone()),
                                span: None,
                                line: None,
                                column: None,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    diagnostics
}
