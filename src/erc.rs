use crate::connectivity::{
    ConnectionMemberKind, ReducedNetBasePinKey, ReducedProjectDriverKind, ReducedProjectSymbolPin,
    collect_connection_points, collect_reduced_label_component_snapshots,
    collect_reduced_project_net_map, collect_reduced_project_subgraphs_by_name,
    collect_reduced_project_symbol_pin_inventories_in_sheet, reduced_bus_member_full_local_names,
    reduced_project_subgraph_by_index, reduced_project_subgraph_index, reduced_project_subgraphs,
    resolve_reduced_project_subgraph_at, resolve_reduced_project_subgraph_for_no_connect,
};
use crate::core::SchematicProject;
use crate::diagnostic::{Diagnostic, Severity};
use crate::loader::{
    LoadedErcSeverity, LoadedSheetPath, collect_wire_segments, point_on_wire_segment, points_equal,
    resolve_cross_reference_text_var, resolve_label_connectivity_text_var,
    resolve_label_text_token_without_connectivity, resolve_sheet_text_var, resolve_text_variables,
    resolved_sheet_text_state, resolved_symbol_text_state, shown_sheet_pin_text,
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

// Upstream parity: reduced local analogue for the top-level subgraph iteration inside
// `CONNECTION_GRAPH::RunERC()`. This is not a 1:1 KiCad iterator because the Rust tree still lacks
// live `CONNECTION_SUBGRAPH*` and absorbed-subgraph state, but it now preserves KiCad's
// `seenDriverInstances` behavior by deduplicating graph-owned ERC passes on reused screens through
// the shared reduced driver owner instead of sweeping every repeated subgraph independently.
// Remaining divergence is the still-missing live subgraph/driver object model behind that owner.
fn graph_run_erc_subgraphs(
    graph: &crate::connectivity::ReducedProjectNetGraph,
) -> Vec<&crate::connectivity::ReducedProjectSubgraphEntry> {
    let mut seen_driver_identities = std::collections::BTreeSet::new();

    reduced_project_subgraphs(graph)
        .iter()
        .filter(|subgraph| {
            crate::connectivity::reduced_project_subgraph_driver_identity(subgraph)
                .is_none_or(|identity| seen_driver_identities.insert(identity.clone()))
        })
        .collect()
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
    visible: bool,
    is_power_symbol: bool,
    path: std::path::PathBuf,
    sheet_instance_path: String,
    reference: String,
    pin_number: String,
    pin_name: Option<String>,
    symbol_uuid: Option<String>,
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

fn reduced_pin_type_weight(pin_type: ReducedPinType) -> usize {
    match pin_type {
        ReducedPinType::NoConnect => 0,
        ReducedPinType::PowerOut => 1,
        ReducedPinType::PowerIn => 2,
        ReducedPinType::Output => 3,
        ReducedPinType::Bidirectional => 4,
        ReducedPinType::TriState => 5,
        ReducedPinType::Input => 6,
        ReducedPinType::OpenEmitter => 7,
        ReducedPinType::OpenCollector => 8,
        ReducedPinType::Passive => 9,
        ReducedPinType::Unspecified => 10,
        ReducedPinType::Free => 11,
    }
}

fn reduced_pin_type_text(pin_type: ReducedPinType) -> &'static str {
    match pin_type {
        ReducedPinType::Input => "Input",
        ReducedPinType::Output => "Output",
        ReducedPinType::Bidirectional => "Bidirectional",
        ReducedPinType::TriState => "Tri-state",
        ReducedPinType::Passive => "Passive",
        ReducedPinType::Free => "Free",
        ReducedPinType::Unspecified => "Unspecified",
        ReducedPinType::PowerIn => "Power input",
        ReducedPinType::PowerOut => "Power output",
        ReducedPinType::OpenCollector => "Open collector",
        ReducedPinType::OpenEmitter => "Open emitter",
        ReducedPinType::NoConnect => "Unconnected",
    }
}

fn reduced_str_num_cmp_ignore_case(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    if a.eq_ignore_ascii_case(b) {
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

                let a_trimmed = a_digits.trim_start_matches('0');
                let b_trimmed = b_digits.trim_start_matches('0');
                let a_normalized = if a_trimmed.is_empty() { "0" } else { a_trimmed };
                let b_normalized = if b_trimmed.is_empty() { "0" } else { b_trimmed };
                let ordering = a_normalized
                    .len()
                    .cmp(&b_normalized.len())
                    .then_with(|| a_normalized.cmp(b_normalized))
                    .then_with(|| a_digits.len().cmp(&b_digits.len()));

                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            (Some(a_ch), Some(b_ch)) => {
                let ordering = a_ch.to_ascii_lowercase().cmp(&b_ch.to_ascii_lowercase());
                a_chars.next();
                b_chars.next();

                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
        }
    }
}

// Upstream parity: reduced local helper for the pin-owned context KiCad keeps on `SCH_PIN` items
// while `ERC_TESTER::TestPinToPin()` runs. This still projects from reduced graph-owned base-pin
// payload instead of live `SCH_PIN` objects, but the exercised ERC pin context now comes from the
// shared graph owner instead of re-walking loaded symbols at report time. Remaining divergence is
// the still-missing live pin object and marker attachment.
fn reduced_erc_pin_context_from_base_pin(
    base_pin: &crate::connectivity::ReducedProjectBasePin,
) -> Option<ReducedErcPinContext> {
    let pin_number = base_pin.number.clone()?;
    let electrical_type = base_pin.electrical_type.clone()?;
    let pin_type = parse_reduced_pin_type(&electrical_type)?;
    let reference = base_pin.reference.clone()?;

    Some(ReducedErcPinContext {
        at: [
            f64::from_bits(base_pin.key.at.0),
            f64::from_bits(base_pin.key.at.1),
        ],
        visible: base_pin.visible,
        is_power_symbol: base_pin.is_power_symbol,
        path: base_pin.schematic_path.clone(),
        sheet_instance_path: base_pin.key.sheet_instance_path.clone(),
        reference,
        pin_number,
        pin_name: base_pin.key.name.clone(),
        symbol_uuid: base_pin.key.symbol_uuid.clone(),
        pin_type,
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
        "erc-label-dangling" => {
            configured_rule_severity(project, "label_dangling", Some(Severity::Error))
        }
        "erc-isolated-pin-label" => {
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
        "erc-pin-not-driven" => {
            configured_rule_severity(project, "pin_not_driven", Some(Severity::Error))
        }
        "erc-power-pin-not-driven" => {
            configured_rule_severity(project, "power_pin_not_driven", Some(Severity::Error))
        }
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

fn reduced_project_symbol_pin_net_name(
    graph: &crate::connectivity::ReducedProjectNetGraph,
    pin: &ReducedProjectSymbolPin,
) -> String {
    pin.subgraph_index
        .and_then(|index| reduced_project_subgraph_by_index(graph, index))
        .map(|subgraph| subgraph.driver_connection.name.clone())
        .unwrap_or_default()
}

// Upstream parity: reduced local helper for the generic connection-point net lookup that the
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

fn reduced_preferred_missing_driver_pin<'a>(
    pins: &'a [ReducedErcPinContext],
    is_power_net: bool,
) -> Option<&'a ReducedErcPinContext> {
    let needs_driver = pins
        .iter()
        .filter(|pin| is_driven_pin_type(pin.pin_type))
        .find(|pin| pin.visible && (!is_power_net || pin.pin_type == ReducedPinType::PowerIn))
        .or_else(|| {
            pins.iter().find(|pin| {
                is_driven_pin_type(pin.pin_type)
                    && (!is_power_net || pin.pin_type == ReducedPinType::PowerIn)
            })
        })
        .or_else(|| {
            pins.iter()
                .filter(|pin| is_driven_pin_type(pin.pin_type))
                .find(|pin| pin.visible)
        })
        .or_else(|| pins.iter().find(|pin| is_driven_pin_type(pin.pin_type)));

    pins.iter()
        .filter(|pin| is_driven_pin_type(pin.pin_type) && !pin.is_power_symbol)
        .find(|pin| pin.visible)
        .or_else(|| {
            pins.iter()
                .find(|pin| is_driven_pin_type(pin.pin_type) && !pin.is_power_symbol)
        })
        .or(needs_driver)
}

// Upstream parity: reduced local helper for the stacked-pin suppression branch inside
// `ERC_TESTER::TestPinToPin()`. This is not a 1:1 KiCad `SCH_PIN` identity test because the Rust
// tree still compares reduced projected pin context instead of live pin items, but it now also
// keeps sheet-instance identity so reused-screen occurrences with the same schematic UUID do not
// collapse into one stacked-pin match. Remaining divergence is fuller live pin ownership and
// marker attachment.
fn reduced_erc_pins_are_stacked(lhs: &ReducedErcPinContext, rhs: &ReducedErcPinContext) -> bool {
    lhs.path == rhs.path
        && lhs.sheet_instance_path == rhs.sheet_instance_path
        && lhs.symbol_uuid == rhs.symbol_uuid
        && lhs.pin_type == rhs.pin_type
        && lhs.pin_name == rhs.pin_name
        && lhs.at[0].to_bits() == rhs.at[0].to_bits()
        && lhs.at[1].to_bits() == rhs.at[1].to_bits()
}

// Upstream parity: local helper for deterministic reduced label lookup keys. KiCad keeps live
// `SCH_TEXT*` identity inside `CONNECTION_SUBGRAPH`, so it does not need this enum-to-key helper.
// The reduced Rust ERC path still keys dangling-label facts by cloned `(sheet, point, kind)`
// tuples, and this keeps that carrier stable without broadening `LabelKind` itself.
fn reduced_label_kind_key(kind: LabelKind) -> u8 {
    match kind {
        LabelKind::Local => 0,
        LabelKind::Global => 1,
        LabelKind::Hierarchical => 2,
        LabelKind::Directive => 3,
    }
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

// Upstream parity: reduced local helper for `SCH_SHEET_PIN::GetShownText()` calls inside
// graph-owned ERC checks. This is not a 1:1 KiCad item method dispatch because the Rust tree
// still reaches sheet pins through reduced model carriers plus `child_sheet_path_for_sheet()`, but
// it keeps the child-path ownership in one place so ERC does not fall back to raw pin names after
// the loader-side shown-text owner exists. Remaining divergence is fuller live sheet/item
// ownership and marker attachment.
fn shown_sheet_pin_name(
    project: &SchematicProject,
    graph: &crate::connectivity::ReducedProjectNetGraph,
    parent_sheet_path: &crate::loader::LoadedSheetPath,
    sheet: &crate::model::Sheet,
    pin: &crate::model::SheetPin,
) -> String {
    let Some(child_sheet_path) = child_sheet_path_for_sheet(project, parent_sheet_path, sheet)
    else {
        return pin.name.clone();
    };

    shown_sheet_pin_text(
        &project.schematics,
        &project.sheet_paths,
        parent_sheet_path,
        &child_sheet_path,
        project.project.as_ref(),
        project.current_variant(),
        Some(graph),
        pin,
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
                        if !property.visible {
                            continue;
                        }
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
// `CONNECTION_SUBGRAPH` objects and hier-port child-subgraph traversal. It now starts from shared
// graph-owned no-connect-item identity plus same-name grouping instead of rebuilding the rule from
// per-sheet connection components, and now also follows `RunERC()`-style reused-screen
// de-duplication through the shared reduced driver owner. It covers both exercised upstream
// branches:
// - connected no-connect markers on same-name nets
// - dangling no-connect markers with no pins or labels
// - plain one-pin dangling subgraphs without a no-connect marker
// instead of leaving the reduced ERC path on the older connected-only point-local check. Same-name
// grouping on the real graph path now also keys from the graph-owned reduced driver connection
// name instead of the parallel reduced subgraph `name` field, and no-connect pin presence plus the
// exercised dangling-pin branch now read graph-owned base-pin payload instead of re-walking
// projected symbol pins at report time. Remaining divergence is the fuller hier-pin and marker
// attachment path plus KiCad's extra multi-pin power-symbol dangling branch.
pub fn check_no_connect_markers(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let graph = project.reduced_project_net_graph(false);
    let mut seen = std::collections::BTreeSet::new();
    let mut seen_driver_identities = std::collections::BTreeSet::new();
    let mut global_label_cache = std::collections::BTreeSet::new();
    let mut local_label_cache = std::collections::BTreeSet::new();

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
                    global_label_cache.insert(shown_text);
                }
                LabelKind::Local | LabelKind::Hierarchical => {
                    local_label_cache.insert((sheet_path.instance_path.clone(), shown_text));
                }
                LabelKind::Directive => {}
            }
        }
    }

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };

        for no_connect in schematic.screen.items.iter().filter_map(|item| match item {
            SchItem::NoConnect(no_connect) => Some(no_connect),
            _ => None,
        }) {
            let Some(subgraph) =
                resolve_reduced_project_subgraph_for_no_connect(&graph, sheet_path, no_connect.at)
            else {
                continue;
            };

            if crate::connectivity::reduced_project_subgraph_driver_identity(&subgraph)
                .is_some_and(|identity| !seen_driver_identities.insert(identity.clone()))
            {
                continue;
            }

            if !seen.insert((subgraph.sheet_instance_path.clone(), subgraph.subgraph_code)) {
                continue;
            }

            let local_unique_pins = subgraph
                .base_pins
                .iter()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>();
            let local_unique_labels = subgraph
                .label_links
                .iter()
                .map(|label| (subgraph.sheet_instance_path.clone(), label.at))
                .collect::<std::collections::BTreeSet<_>>();
            let has_sheet_pin = subgraph.hier_sheet_pins.iter().any(|pin| {
                pin.at
                    == crate::connectivity::PointKey(
                        no_connect.at[0].to_bits(),
                        no_connect.at[1].to_bits(),
                    )
            });
            let has_hierarchical_label = subgraph.hier_ports.iter().any(|label| {
                label.at
                    == crate::connectivity::PointKey(
                        no_connect.at[0].to_bits(),
                        no_connect.at[1].to_bits(),
                    )
            });
            let has_nc_pin = subgraph.base_pins.iter().any(|base_pin| {
                base_pin.electrical_type.as_deref() == Some("no_connect")
                    && base_pin.key.at
                        == crate::connectivity::PointKey(
                            no_connect.at[0].to_bits(),
                            no_connect.at[1].to_bits(),
                        )
            });

            if ((has_sheet_pin || has_hierarchical_label) && local_unique_pins.is_empty())
                || (has_nc_pin && local_unique_pins.len() <= 1)
            {
                continue;
            }

            let subgraph_name = subgraph.driver_connection.name.clone();
            let (unique_pin_count, unique_label_count) = if subgraph_name.is_empty() {
                (local_unique_pins.len(), local_unique_labels.len())
            } else {
                let neighbors = collect_reduced_project_subgraphs_by_name(&graph, &subgraph_name);
                let unique_pin_count = neighbors
                    .iter()
                    .flat_map(|neighbor| {
                        neighbor
                            .base_pins
                            .iter()
                            .map(|base_pin| base_pin.key.clone())
                    })
                    .collect::<std::collections::BTreeSet<ReducedNetBasePinKey>>()
                    .len();
                let unique_label_count = neighbors
                    .iter()
                    .flat_map(|neighbor| {
                        neighbor
                            .label_links
                            .iter()
                            .map(|label| (neighbor.sheet_instance_path.clone(), label.at))
                    })
                    .collect::<std::collections::BTreeSet<_>>()
                    .len();
                (unique_pin_count, unique_label_count)
            };

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
                message: "A pin with a \"no connection\" flag is connected".to_string(),
                path: Some(sheet_path.schematic_path.clone()),
                span: None,
                line: None,
                column: None,
            });
        }
    }

    for subgraph in reduced_project_subgraphs(&graph) {
        if subgraph.has_no_connect
            || !subgraph.no_connect_points.is_empty()
            || subgraph.base_pins.is_empty()
        {
            continue;
        }

        let mut has_other_connections = !subgraph.label_links.is_empty()
            || !subgraph.hier_sheet_pins.is_empty()
            || !subgraph.hier_ports.is_empty()
            || subgraph
                .drivers
                .iter()
                .any(|driver| driver.kind == ReducedProjectDriverKind::PowerPin);
        let pins = subgraph
            .base_pins
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        if pins.is_empty() {
            continue;
        }

        if !has_other_connections && pins.len() > 1 {
            for test_pin in pins.iter().skip(1) {
                let shares_symbol_point = test_pin.key.symbol_uuid == pins[0].key.symbol_uuid
                    && test_pin.key.at == pins[0].key.at;
                if test_pin.key != pins[0].key && !shares_symbol_point {
                    has_other_connections = true;
                    break;
                }
            }
        }

        let mut pin = pins[0];

        for test_pin in &pins {
            if test_pin.electrical_type.as_deref() == Some("power_in") && !test_pin.is_power_symbol
            {
                pin = test_pin;
                break;
            }
        }

        if !has_other_connections && !pin.is_power_symbol {
            if global_label_cache.contains(&pin.connection.name)
                || local_label_cache.contains(&(
                    subgraph.sheet_instance_path.clone(),
                    pin.connection.local_name.clone(),
                ))
            {
                has_other_connections = true;
            }
        }

        let same_name_has_no_connect_sibling = (subgraph
            .driver_connection
            .name
            .starts_with("Net-(")
            || subgraph.driver_connection.name.starts_with("unconnected-("))
            && collect_reduced_project_subgraphs_by_name(&graph, &subgraph.driver_connection.name)
                .iter()
                .any(|neighbor| {
                    neighbor.sheet_instance_path == subgraph.sheet_instance_path
                        && neighbor.subgraph_code != subgraph.subgraph_code
                        && (neighbor.has_no_connect || !neighbor.no_connect_points.is_empty())
                });

        if same_name_has_no_connect_sibling {
            continue;
        }

        if !has_other_connections
            && pin.electrical_type.as_deref() != Some("no_connect")
            && pin.electrical_type.as_deref() != Some("not_connected")
        {
            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                code: "erc-pin-not-connected",
                kind: crate::diagnostic::DiagnosticKind::Validation,
                message: "Pin not connected".to_string(),
                path: Some(pin.schematic_path.clone()),
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
// neighbor walks, and live `SCH_TEXT::IsDangling()` state. It now consumes shared graph-owned
// reduced label links plus shared reduced hierarchy pin/port links for local hierarchy state, and
// shared reduced project subgraphs for pin counts and same-name neighbor aggregation instead of
// grouping local component snapshots by ad-hoc `net_name` strings inside ERC. It now also follows
// `RunERC()`-style reused-screen de-duplication through the shared reduced driver owner, and now
// walks reduced member-keyed bus-parent links instead of bare parent index lists. Subgraph net
// names on the real graph path now also flow from the graph-owned reduced driver connection
// instead of the parallel reduced subgraph `name` field. Remaining divergence is fuller live
// bus-neighbor connection ownership plus the local dangling-label probe.
pub fn check_label_connectivity(project: &SchematicProject) -> Vec<Diagnostic> {
    fn subgraph_has_local_hierarchy_via_bus_parents(
        graph: &crate::connectivity::ReducedProjectNetGraph,
        subgraph_index: usize,
    ) -> bool {
        let Some(subgraph) = reduced_project_subgraph_by_index(graph, subgraph_index) else {
            return false;
        };

        subgraph.bus_parent_links.iter().any(|parent_link| {
            let Some(parent) = reduced_project_subgraph_by_index(graph, parent_link.subgraph_index)
            else {
                return false;
            };

            parent.sheet_instance_path == subgraph.sheet_instance_path
                && (!parent.hier_sheet_pins.is_empty() || !parent.hier_ports.is_empty())
        })
    }

    fn subgraph_has_no_connect_via_parent_chain(
        graph: &crate::connectivity::ReducedProjectNetGraph,
        subgraph_index: usize,
    ) -> bool {
        let Some(subgraph) = reduced_project_subgraph_by_index(graph, subgraph_index) else {
            return false;
        };
        let mut pending = subgraph
            .bus_parent_links
            .iter()
            .map(|link| link.subgraph_index)
            .collect::<Vec<_>>();
        let mut seen = std::collections::BTreeSet::new();

        while let Some(parent_index) = pending.pop() {
            if !seen.insert(parent_index) {
                continue;
            }

            let Some(parent) = reduced_project_subgraph_by_index(graph, parent_index) else {
                continue;
            };

            if parent.has_no_connect {
                return true;
            }

            let mut hier_parent_index = parent.hier_parent_index;

            while let Some(index) = hier_parent_index {
                if !seen.insert(index) {
                    break;
                }

                let Some(hier_parent) = reduced_project_subgraph_by_index(graph, index) else {
                    break;
                };

                if hier_parent.has_no_connect {
                    return true;
                }

                hier_parent_index = hier_parent.hier_parent_index;
            }
        }

        false
    }

    let mut diagnostics = Vec::new();
    let mut dangling_labels = BTreeMap::<(String, crate::connectivity::PointKey, u8), bool>::new();
    let graph = project.reduced_project_net_graph(false);
    let mut label_subgraphs = Vec::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project.schematic(&sheet_path.schematic_path) else {
            continue;
        };
        for component in collect_reduced_label_component_snapshots(schematic) {
            for label in component.labels {
                dangling_labels.insert(
                    (
                        sheet_path.instance_path.clone(),
                        crate::connectivity::PointKey(label.at[0].to_bits(), label.at[1].to_bits()),
                        reduced_label_kind_key(label.kind),
                    ),
                    label.dangling,
                );
            }
        }
    }

    for subgraph in graph_run_erc_subgraphs(&graph) {
        if subgraph.label_links.is_empty() {
            continue;
        }

        let subgraph_index = reduced_project_subgraph_index(&graph, &subgraph);

        let pin_count = subgraph.base_pins.len();
        let has_local_hierarchy = !subgraph.hier_sheet_pins.is_empty()
            || !subgraph.hier_ports.is_empty()
            || subgraph_index
                .is_some_and(|index| subgraph_has_local_hierarchy_via_bus_parents(&graph, index));
        let has_no_connect = subgraph.has_no_connect
            || subgraph_index
                .is_some_and(|index| subgraph_has_no_connect_via_parent_chain(&graph, index));

        label_subgraphs.push((
            subgraph.sheet_instance_path.clone(),
            subgraph.subgraph_code,
            subgraph.driver_connection.name.clone(),
            pin_count,
            has_no_connect,
            has_local_hierarchy,
            subgraph.label_links.clone(),
        ));
    }

    for (
        sheet_instance_path,
        subgraph_code,
        net_name,
        pin_count,
        subgraph_has_no_connect,
        subgraph_has_local_hierarchy,
        label_links,
    ) in label_subgraphs
    {
        let Some(sheet_path) = project
            .sheet_paths
            .iter()
            .find(|sheet_path| sheet_path.instance_path == sheet_instance_path)
        else {
            continue;
        };

        let mut all_pins = pin_count;
        let mut local_pins = pin_count;
        let mut has_no_connect = subgraph_has_no_connect;
        let mut has_local_hierarchy = subgraph_has_local_hierarchy;

        if !net_name.is_empty() {
            let neighbors = collect_reduced_project_subgraphs_by_name(&graph, &net_name);
            for (
                neighbor_sheet_instance_path,
                neighbor_subgraph_code,
                neighbor_pin_count,
                neighbor_has_no_connect,
                neighbor_has_local_hierarchy,
            ) in neighbors.into_iter().map(|neighbor| {
                let neighbor_has_local_hierarchy =
                    !neighbor.hier_sheet_pins.is_empty() || !neighbor.hier_ports.is_empty();

                (
                    neighbor.sheet_instance_path.clone(),
                    neighbor.subgraph_code,
                    neighbor.base_pins.len(),
                    neighbor.has_no_connect,
                    neighbor_has_local_hierarchy,
                )
            }) {
                if neighbor_sheet_instance_path == sheet_instance_path
                    && neighbor_subgraph_code == subgraph_code
                {
                    continue;
                }

                all_pins += neighbor_pin_count;
                has_no_connect |= neighbor_has_no_connect;

                if neighbor_sheet_instance_path == sheet_instance_path {
                    local_pins += neighbor_pin_count;
                    has_local_hierarchy |= neighbor_has_local_hierarchy;
                }
            }
        }

        for label in label_links {
            if label.kind == LabelKind::Directive {
                continue;
            }
            if matches!(
                label.connection.connection_type,
                crate::connectivity::ReducedProjectConnectionType::Bus
                    | crate::connectivity::ReducedProjectConnectionType::BusGroup
            ) {
                continue;
            }

            let dangling = dangling_labels
                .get(&(
                    sheet_instance_path.clone(),
                    label.at,
                    reduced_label_kind_key(label.kind),
                ))
                .copied()
                .unwrap_or(false);
            let graph_has_pins = all_pins > 0;

            if (dangling && !graph_has_pins)
                || (label.kind == LabelKind::Local
                    && local_pins == 0
                    && all_pins > 1
                    && !has_no_connect
                    && !has_local_hierarchy)
                || all_pins == 0
            {
                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    code: "erc-label-dangling",
                    kind: crate::diagnostic::DiagnosticKind::Validation,
                    message: "Label not connected".to_string(),
                    path: Some(sheet_path.schematic_path.clone()),
                    span: None,
                    line: None,
                    column: None,
                });
                continue;
            }

            if all_pins == 1 && !has_no_connect {
                diagnostics.push(Diagnostic {
                    severity: Severity::Warning,
                    code: "erc-isolated-pin-label",
                    kind: crate::diagnostic::DiagnosticKind::Validation,
                    message: "Label connected to only one pin".to_string(),
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
        for component in collect_reduced_label_component_snapshots(schematic) {
            for _label in component
                .labels
                .iter()
                .filter(|label| label.kind == LabelKind::Directive && label.dangling)
            {
                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    code: "erc-label-dangling",
                    kind: crate::diagnostic::DiagnosticKind::Validation,
                    message: "Label not connected".to_string(),
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
// because the Rust tree still uses reduced stored wire-item endpoints instead of live dangling
// flags on `SCH_LINE` / bus-entry items. It now runs on shared reduced project subgraphs instead
// of rebuilding from a per-sheet point snapshot, and now also follows `RunERC()`-style
// reused-screen driver de-duplication through the shared graph owner. Remaining divergence is
// fuller bus-layer/item ownership beyond the current reduced endpoint carrier.
pub fn check_dangling_wire_endpoints(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let graph = project.reduced_project_net_graph(false);

    for subgraph in graph_run_erc_subgraphs(&graph)
        .into_iter()
        .filter(|subgraph| !subgraph.wire_items.is_empty())
    {
        let Some(sheet_path) = project
            .sheet_paths
            .iter()
            .find(|sheet_path| sheet_path.instance_path == subgraph.sheet_instance_path)
        else {
            continue;
        };

        for wire_item in &subgraph.wire_items {
            let connected_bus_subgraph = if wire_item.is_bus_entry {
                crate::connectivity::reduced_project_connected_bus_subgraph_for_wire_item(
                    &graph, subgraph, wire_item,
                )
            } else {
                None
            };
            for endpoint in [wire_item.start, wire_item.end] {
                let endpoint_at = [f64::from_bits(endpoint.0), f64::from_bits(endpoint.1)];
                let endpoint_matches = |point: crate::connectivity::PointKey| {
                    points_equal(
                        endpoint_at,
                        [f64::from_bits(point.0), f64::from_bits(point.1)],
                    )
                };
                let endpoint_has_owner = subgraph
                    .base_pins
                    .iter()
                    .any(|base_pin| endpoint_matches(base_pin.key.at))
                    || subgraph
                        .bus_items
                        .iter()
                        .any(|item| endpoint_matches(item.start) || endpoint_matches(item.end))
                    || connected_bus_subgraph.is_some_and(|bus_subgraph| {
                        bus_subgraph
                            .bus_items
                            .iter()
                            .any(|item| endpoint_matches(item.start) || endpoint_matches(item.end))
                    })
                    || subgraph
                        .label_links
                        .iter()
                        .any(|label| endpoint_matches(label.at))
                    || subgraph
                        .hier_sheet_pins
                        .iter()
                        .any(|pin| endpoint_matches(pin.at))
                    || subgraph
                        .hier_ports
                        .iter()
                        .any(|port| endpoint_matches(port.at))
                    || subgraph
                        .no_connect_points
                        .iter()
                        .copied()
                        .any(endpoint_matches);
                let endpoint_has_same_sheet_owner = !endpoint_has_owner
                    && !subgraph.driver_connection.name.is_empty()
                    && collect_reduced_project_subgraphs_by_name(
                        &graph,
                        &subgraph.driver_connection.name,
                    )
                    .into_iter()
                    .filter(|neighbor| {
                        neighbor.sheet_instance_path == subgraph.sheet_instance_path
                            && neighbor.subgraph_code != subgraph.subgraph_code
                    })
                    .any(|neighbor| {
                        neighbor
                            .base_pins
                            .iter()
                            .any(|base_pin| endpoint_matches(base_pin.key.at))
                            || neighbor
                                .label_links
                                .iter()
                                .any(|label| endpoint_matches(label.at))
                            || neighbor
                                .hier_sheet_pins
                                .iter()
                                .any(|pin| endpoint_matches(pin.at))
                            || neighbor
                                .hier_ports
                                .iter()
                                .any(|port| endpoint_matches(port.at))
                            || neighbor
                                .no_connect_points
                                .iter()
                                .copied()
                                .any(endpoint_matches)
                    });
                let endpoint_wire_count = subgraph
                    .wire_items
                    .iter()
                    .filter(|other| endpoint_matches(other.start) || endpoint_matches(other.end))
                    .count();

                if endpoint_has_owner || endpoint_has_same_sheet_owner || endpoint_wire_count > 1 {
                    continue;
                }

                diagnostics.push(Diagnostic {
                    severity: Severity::Warning,
                    code: "erc-unconnected-wire-endpoint",
                    kind: crate::diagnostic::DiagnosticKind::Validation,
                    message: if wire_item.is_bus_entry {
                        "Unconnected wire to bus entry".to_string()
                    } else {
                        "Unconnected wire endpoint".to_string()
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
// because the Rust tree still treats "floating" as a reduced shared subgraph with wire-item
// membership but without a full graph-owned driver object. It now runs on shared reduced project
// subgraphs instead of rebuilding wire components per sheet, and now also follows `RunERC()`-style
// reused-screen driver de-duplication through that shared owner. Remaining divergence is fuller
// driver ownership and bus-layer semantics beyond the current reduced wire-item carrier.
pub fn check_floating_wires(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let graph = project.reduced_project_net_graph(false);

    for subgraph in graph_run_erc_subgraphs(&graph)
        .into_iter()
        .filter(|subgraph| !subgraph.wire_items.is_empty())
    {
        if !subgraph.base_pins.is_empty()
            || !subgraph.hier_sheet_pins.is_empty()
            || !subgraph.hier_ports.is_empty()
            || !subgraph.label_links.is_empty()
            || !subgraph.no_connect_points.is_empty()
        {
            continue;
        }

        let Some(sheet_path) = project
            .sheet_paths
            .iter()
            .find(|sheet_path| sheet_path.instance_path == subgraph.sheet_instance_path)
        else {
            continue;
        };
        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            code: "erc-wire-dangling",
            kind: crate::diagnostic::DiagnosticKind::Validation,
            message: "Wires not connected to anything".to_string(),
            path: Some(sheet_path.schematic_path.clone()),
            span: None,
            line: None,
            column: None,
        });
    }

    diagnostics
}

// Upstream parity: reduced local helper for the parent-sheet-pin query inside
// `CONNECTION_GRAPH::ercCheckHierSheets()`. This is not a 1:1 KiCad `GetSubgraphForItem()` lookup
// because the Rust tree still keys by `(sheet path, point)` instead of a live `SCH_SHEET_PIN*`,
// but it now asks the shared reduced project graph whether the pin belongs to any broader subgraph
// instead of rebuilding a one-off local connected component. Remaining divergence is fuller item
// identity and child-subgraph ownership.
fn sheet_pin_is_dangling(
    graph: &crate::connectivity::ReducedProjectNetGraph,
    sheet_path: &LoadedSheetPath,
    pin_at: [f64; 2],
) -> bool {
    let Some(subgraph) = resolve_reduced_project_subgraph_at(graph, sheet_path, pin_at) else {
        return true;
    };

    let pin_point = crate::connectivity::PointKey(pin_at[0].to_bits(), pin_at[1].to_bits());
    subgraph.base_pins.is_empty()
        && subgraph.label_links.is_empty()
        && subgraph.no_connect_points.is_empty()
        && subgraph.wire_items.is_empty()
        && subgraph.bus_items.is_empty()
        && !subgraph
            .hier_sheet_pins
            .iter()
            .any(|pin| pin.at != pin_point)
}

// Upstream parity: reduced local analogue for `CONNECTION_GRAPH::ercCheckHierSheets()`. This is
// not a 1:1 KiCad marker/`GetShownText()` path because the Rust tree still uses reduced sheet-path
// helpers and lacks full sheet-pin / marker owners, but it now compares parent sheet pins through
// a reduced `SCH_SHEET_PIN::GetShownText()` analogue instead of raw pin names. It also asks the
// shared reduced project graph whether parent sheet pins belong to a broader subgraph instead of
// rebuilding that query from a local connected-component scan. Remaining divergence is fuller
// marker attachment and item ownership parity.
pub fn check_hierarchical_sheets(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let graph = project.reduced_project_net_graph(false);

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
                if sheet_pin_is_dangling(&graph, sheet_path, pin.at) {
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
                let shown = shown_sheet_pin_name(project, &graph, sheet_path, sheet, pin);
                pins.insert(shown, pin);
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
// This is not a 1:1 KiCad subgraph/`SCH_CONNECTION` pass because the Rust tree still keeps
// reduced text-item and bus-member snapshots instead of live item-owned connections. It now
// classifies bus-vs-net ownership from shared reduced subgraph item membership instead of
// rescanning labels and sheet pins out of the schematic, and it still follows `RunERC()`-style
// reused-screen driver de-duplication through the shared reduced graph owner. Remaining divergence
// is fuller member-aware bus semantics plus the still-missing live item pointers on subgraphs.
pub fn check_bus_to_net_conflicts(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let graph = project.reduced_project_net_graph(false);

    for subgraph in graph_run_erc_subgraphs(&graph)
        .into_iter()
        .filter(|subgraph| !subgraph.wire_items.iter().any(|item| item.is_bus_entry))
    {
        let Some(sheet_path) = project
            .sheet_paths
            .iter()
            .find(|sheet_path| sheet_path.instance_path == subgraph.sheet_instance_path)
        else {
            continue;
        };
        let mut has_bus_item = !subgraph.bus_items.is_empty();
        let mut has_net_item = !subgraph.wire_items.is_empty();
        let mut net_at = subgraph
            .wire_items
            .first()
            .map(|item| [f64::from_bits(item.start.0), f64::from_bits(item.start.1)]);

        for label in &subgraph.label_links {
            if label.kind == LabelKind::Directive {
                continue;
            }

            if matches!(
                label.connection.connection_type,
                crate::connectivity::ReducedProjectConnectionType::Bus
                    | crate::connectivity::ReducedProjectConnectionType::BusGroup
            ) {
                has_bus_item = true;
            } else {
                has_net_item = true;
                net_at.get_or_insert([f64::from_bits(label.at.0), f64::from_bits(label.at.1)]);
            }
        }

        for pin in &subgraph.hier_sheet_pins {
            if matches!(
                pin.connection.connection_type,
                crate::connectivity::ReducedProjectConnectionType::Bus
                    | crate::connectivity::ReducedProjectConnectionType::BusGroup
            ) {
                has_bus_item = true;
            } else {
                has_net_item = true;
                net_at.get_or_insert([f64::from_bits(pin.at.0), f64::from_bits(pin.at.1)]);
            }
        }

        for port in &subgraph.hier_ports {
            if matches!(
                port.connection.connection_type,
                crate::connectivity::ReducedProjectConnectionType::Bus
                    | crate::connectivity::ReducedProjectConnectionType::BusGroup
            ) {
                has_bus_item = true;
            } else {
                has_net_item = true;
                net_at.get_or_insert([f64::from_bits(port.at.0), f64::from_bits(port.at.1)]);
            }
        }

        if has_bus_item && has_net_item {
            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                code: "erc-bus-to-net-conflict",
                kind: crate::diagnostic::DiagnosticKind::Validation,
                message: "Invalid connection between bus and net items".to_string(),
                path: Some(sheet_path.schematic_path.clone()),
                span: None,
                line: None,
                column: None,
            });
        }
    }

    diagnostics
}

// Upstream parity: reduced local analogue for `CONNECTION_GRAPH::ercCheckBusToBusConflicts()`.
// This is not a 1:1 KiCad bus-member connection pass because the Rust tree still keeps reduced
// per-text connection objects instead of full live `SCH_CONNECTION` ownership. It now picks the
// bus label and port from shared reduced connection owners on the subgraph instead of rescanning
// schematic items, and still compares direct shared member `Name()` values from those reduced
// member objects instead of bare repo-local string rescans. It also follows `RunERC()`-style
// reused-screen driver de-duplication through the shared reduced graph owner. Remaining divergence
// is fuller resolved member-object ownership beyond this reduced direct-member-name overlap check.
pub fn check_bus_to_bus_conflicts(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let graph = project.reduced_project_net_graph(false);

    for subgraph in graph_run_erc_subgraphs(&graph) {
        let Some(sheet_path) = project
            .sheet_paths
            .iter()
            .find(|sheet_path| sheet_path.instance_path == subgraph.sheet_instance_path)
        else {
            continue;
        };
        let mut label_members = None::<Vec<String>>;
        let mut port_members = None::<Vec<String>>;
        let mut label_at = None::<[f64; 2]>;
        if let Some(connection) = subgraph.label_links.iter().find_map(|label| {
            matches!(
                label.connection.connection_type,
                crate::connectivity::ReducedProjectConnectionType::Bus
                    | crate::connectivity::ReducedProjectConnectionType::BusGroup
            )
            .then_some(&label.connection)
        }) {
            label_members = Some(
                connection
                    .members
                    .iter()
                    .map(|member| member.name.clone())
                    .collect(),
            );
        }
        if let Some(connection) = subgraph
            .hier_sheet_pins
            .iter()
            .map(|pin| &pin.connection)
            .chain(subgraph.hier_ports.iter().map(|port| &port.connection))
            .find(|connection| {
                matches!(
                    connection.connection_type,
                    crate::connectivity::ReducedProjectConnectionType::Bus
                        | crate::connectivity::ReducedProjectConnectionType::BusGroup
                )
            })
        {
            port_members = Some(
                connection
                    .members
                    .iter()
                    .map(|member| member.name.clone())
                    .collect(),
            );
        }
        if label_members.is_some() {
            label_at = subgraph.label_links.iter().find_map(|label| {
                (matches!(
                    label.connection.connection_type,
                    crate::connectivity::ReducedProjectConnectionType::Bus
                        | crate::connectivity::ReducedProjectConnectionType::BusGroup
                ) && matches!(label.kind, LabelKind::Local | LabelKind::Global))
                .then_some([f64::from_bits(label.at.0), f64::from_bits(label.at.1)])
            });
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

    diagnostics
}

// Upstream parity: reduced local analogue for `CONNECTION_GRAPH::ercCheckBusToBusEntryConflicts()`.
// This is not a 1:1 KiCad driver/subgraph pass because the Rust tree still lacks live
// `SCH_CONNECTION` plus connected-bus-item ownership, but the exercised real graph path now reads
// bus, member, and fallback net-name state through the graph-owned reduced `driver_connection`
// owners instead of mixing those checks across parallel reduced boundary carriers. It still
// compares flattened reduced `FullLocalName()` values and still diverges on fuller resolved
// bus-object ownership plus cached live driver connections.
pub fn check_bus_to_bus_entry_conflicts(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let graph = project.reduced_project_net_graph(false);

    for subgraph in graph_run_erc_subgraphs(&graph)
        .into_iter()
        .filter(|subgraph| subgraph.wire_items.iter().any(|item| item.is_bus_entry))
    {
        let Some(sheet_path) = project
            .sheet_paths
            .iter()
            .find(|sheet_path| sheet_path.instance_path == subgraph.sheet_instance_path)
        else {
            continue;
        };
        let Some(bus_entry) = subgraph.wire_items.iter().find(|item| item.is_bus_entry) else {
            continue;
        };
        let entry_at = [
            f64::from_bits(bus_entry.start.0),
            f64::from_bits(bus_entry.start.1),
        ];
        let bus_connection =
            crate::connectivity::reduced_project_connected_bus_subgraph_for_wire_item(
                &graph, &subgraph, bus_entry,
            )
            .map(|bus_subgraph| &bus_subgraph.driver_connection)
            .unwrap_or(&subgraph.driver_connection);
        if !matches!(
            bus_connection.connection_type,
            crate::connectivity::ReducedProjectConnectionType::Bus
                | crate::connectivity::ReducedProjectConnectionType::BusGroup
        ) {
            continue;
        }
        let bus_name = bus_connection.local_name.clone();
        let bus_members = reduced_bus_member_full_local_names(&bus_connection.members);
        if bus_members.is_empty() {
            continue;
        }
        let mut test_names = Vec::new();
        if let Some(non_bus_driver) = subgraph.drivers.iter().find(|driver| {
            !crate::connectivity::reduced_project_strong_driver_full_name(driver).is_empty()
                && crate::connectivity::reduced_project_strong_driver_full_name(driver)
                    != bus_connection.full_local_name
                && crate::connectivity::reduced_project_strong_driver_name(driver)
                    != bus_connection.local_name
        }) {
            test_names.push(
                crate::connectivity::reduced_project_strong_driver_full_name(non_bus_driver)
                    .to_string(),
            );
        }
        let driver_connection = &subgraph.driver_connection;
        if driver_connection.connection_type
            == crate::connectivity::ReducedProjectConnectionType::Net
            && !driver_connection.full_local_name.is_empty()
            && !bus_members
                .iter()
                .any(|member| member == &driver_connection.full_local_name)
            && !test_names
                .iter()
                .any(|existing| existing == &driver_connection.full_local_name)
        {
            test_names.push(driver_connection.full_local_name.clone());
        }
        for connection in subgraph
            .label_links
            .iter()
            .map(|link| &link.connection)
            .chain(subgraph.hier_sheet_pins.iter().map(|pin| &pin.connection))
            .chain(subgraph.hier_ports.iter().map(|port| &port.connection))
            .filter(|connection| {
                connection.connection_type == crate::connectivity::ReducedProjectConnectionType::Net
            })
        {
            if !test_names
                .iter()
                .any(|existing| existing == &connection.full_local_name)
                && !bus_members
                    .iter()
                    .any(|member| member == &connection.full_local_name)
            {
                test_names.push(connection.full_local_name.clone());
            }
        }
        if test_names.is_empty() {
            test_names.extend(
                subgraph
                    .drivers
                    .iter()
                    .map(crate::connectivity::reduced_project_strong_driver_full_name)
                    .map(str::to_string)
                    .filter(|name| {
                        !bus_members.iter().any(|member| member == name)
                            && name != &bus_connection.full_local_name
                            && name != &bus_connection.local_name
                    }),
            );
        }
        if test_names.is_empty() {
            if !driver_connection.full_local_name.is_empty() {
                test_names.push(driver_connection.full_local_name.clone());
            }
        }

        let suppress_conflict =
            crate::connectivity::reduced_project_subgraph_non_bus_driver_priority(&subgraph)
                .is_some_and(|priority| priority >= 6);

        if test_names
            .iter()
            .any(|name| bus_members.iter().any(|member| member == name))
        {
            continue;
        }

        if suppress_conflict {
            continue;
        }

        let net_name = test_names.first().cloned().unwrap_or_else(|| {
            if !driver_connection.full_local_name.is_empty() {
                return driver_connection.full_local_name.clone();
            }

            subgraph.driver_connection.name.clone()
        });
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

    diagnostics
}

// Upstream parity: reduced local analogue for `ERC_TESTER::TestPinToPin()`. This is not a 1:1
// KiCad pin-matrix runner because the Rust tree still lacks full `ERC_SETTINGS`, marker
// placement/ranking exactness, and the fuller live `SCH_CONNECTION` / `CONNECTION_SUBGRAPH`
// ownership model. It now runs over the shared reduced project net map like upstream `m_nets`
// instead of per-sheet connection components, applies the typed companion-project `erc.pin_map`
// override slice on top of the upstream default matrix instead of hard-coding only the defaults,
// reads exercised per-pin ERC context from shared graph-owned pin payload instead of re-walking
// symbols at report time, prefers visible non-power pins for the reduced `needsDriver` report
// target, skips same-symbol stacked pins before pin-map conflict checks, reports the conflicting
// pin-type pair like upstream instead of dropping that branch detail into a generic point-only
// message, and now also emits separate reduced missing-driver codes for ordinary versus power-input
// nets so severity policy follows KiCad's owning ERC item split instead of message text. Remaining
// divergence is richer settings, multi-marker emission, and the fuller live graph ownership behind
// the reduced carrier.
pub fn check_pin_to_pin(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for net in collect_reduced_project_net_map(project, false) {
        struct PinMismatch {
            lhs: usize,
            rhs: usize,
            conflict: PinConflict,
        }

        let mut pins = net
            .base_pins
            .iter()
            .filter_map(reduced_erc_pin_context_from_base_pin)
            .collect::<Vec<_>>();

        if pins.is_empty() {
            continue;
        }

        pins.sort_by(|lhs, rhs| {
            let mut ordering = reduced_str_num_cmp_ignore_case(&lhs.reference, &rhs.reference);
            if ordering == std::cmp::Ordering::Equal {
                ordering = reduced_str_num_cmp_ignore_case(&lhs.pin_number, &rhs.pin_number);
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
        let preferred_missing_driver = reduced_preferred_missing_driver_pin(&pins, is_power_net);
        let mut mismatches = Vec::<PinMismatch>::new();
        let mut mismatch_weights = BTreeMap::<usize, usize>::new();

        if pins.len() >= 2 {
            for (index, lhs_pin) in pins.iter().enumerate() {
                for (rhs_index, rhs_pin) in pins.iter().enumerate().skip(index + 1) {
                    if reduced_erc_pins_are_stacked(lhs_pin, rhs_pin) {
                        continue;
                    }

                    let conflict =
                        configured_pin_conflict(project, lhs_pin.pin_type, rhs_pin.pin_type);
                    if conflict == PinConflict::Ok {
                        continue;
                    }

                    mismatches.push(PinMismatch {
                        lhs: index,
                        rhs: rhs_index,
                        conflict,
                    });
                    mismatch_weights.insert(index, reduced_pin_type_weight(lhs_pin.pin_type));
                    mismatch_weights.insert(rhs_index, reduced_pin_type_weight(rhs_pin.pin_type));
                }
            }
        }

        let mut ranked_pins = mismatch_weights.into_iter().collect::<Vec<_>>();
        ranked_pins.sort_by(|(lhs_index, lhs_weight), (rhs_index, rhs_weight)| {
            rhs_weight.cmp(lhs_weight).then(lhs_index.cmp(rhs_index))
        });

        for (pin_index, _) in ranked_pins {
            if mismatches.is_empty() {
                break;
            }

            let pin = &pins[pin_index];
            let mut nearest_conflict: Option<(PinConflict, usize, bool, f64)> = None;

            mismatches.retain(|mismatch| {
                let other_index = if mismatch.lhs == pin_index {
                    mismatch.rhs
                } else if mismatch.rhs == pin_index {
                    mismatch.lhs
                } else {
                    return true;
                };

                let other = &pins[other_index];
                let same_path = pin.path == other.path;
                let distance = if same_path {
                    let dx = pin.at[0] - other.at[0];
                    let dy = pin.at[1] - other.at[1];
                    (dx * dx + dy * dy).sqrt()
                } else {
                    f64::INFINITY
                };

                match &nearest_conflict {
                    Some((_, _, best_same_path, _)) if *best_same_path && !same_path => {}
                    Some((_, _, best_same_path, best_distance))
                        if *best_same_path == same_path && *best_distance <= distance => {}
                    _ => {
                        nearest_conflict =
                            Some((mismatch.conflict, other_index, same_path, distance));
                    }
                }

                false
            });

            let Some((conflict, other_index, _, _)) = nearest_conflict else {
                continue;
            };
            let other = &pins[other_index];

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
                    "Pins of type {} and {} are connected",
                    reduced_pin_type_text(pin.pin_type),
                    reduced_pin_type_text(other.pin_type)
                ),
                path: Some(pin.path.clone()),
                span: None,
                line: None,
                column: None,
            });
        }

        if has_driver || net.has_no_connect {
            continue;
        }

        if let Some(pin) = preferred_missing_driver {
            let article = if pin.pin_type == ReducedPinType::PowerIn {
                "Input Power pin not driven by any Output Power pins"
            } else {
                "Input pin not driven by any Output pins"
            };

            diagnostics.push(Diagnostic {
                severity: Severity::Warning,
                code: if pin.pin_type == ReducedPinType::PowerIn {
                    "erc-power-pin-not-driven"
                } else {
                    "erc-pin-not-driven"
                },
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
// and marker-owned item identity, but it now reads reduced strong-driver conflicts from the shared
// project subgraph owner instead of rebuilding them from per-sheet connection components, now also
// preserves KiCad's "labels and power pins only" secondary-driver filter instead of warning on
// sheet-pin-only name differences, and now also follows `RunERC()`-style reused-screen driver
// de-duplication through the shared reduced driver owner. Remaining divergence is fuller bus/power
// subgraph coverage, driver-item identity, and exact marker attachment.
pub fn check_driver_conflicts(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let graph = project.reduced_project_net_graph(false);

    for subgraph in graph_run_erc_subgraphs(&graph) {
        let Some(primary_driver) = subgraph.drivers.first() else {
            continue;
        };
        let Some(secondary_driver) = subgraph.drivers.iter().skip(1).find(|driver| {
            matches!(
                driver.kind,
                ReducedProjectDriverKind::Label | ReducedProjectDriverKind::PowerPin
            ) && crate::connectivity::reduced_project_strong_driver_name(driver)
                != crate::connectivity::reduced_project_strong_driver_name(primary_driver)
        }) else {
            continue;
        };
        let primary_name =
            crate::connectivity::reduced_project_strong_driver_name(primary_driver).to_string();
        let secondary_name =
            crate::connectivity::reduced_project_strong_driver_name(secondary_driver).to_string();
        let path = project
            .sheet_paths
            .iter()
            .find(|sheet_path| sheet_path.instance_path == subgraph.sheet_instance_path)
            .map(|sheet_path| sheet_path.schematic_path.clone())
            .unwrap_or_else(|| project.root_path.clone());

        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            code: "erc-driver-conflict",
            kind: crate::diagnostic::DiagnosticKind::Validation,
            message: format!(
                "Both {primary_name} and {secondary_name} are attached to the same items; {primary_name} will be used in the netlist"
            ),
            path: Some(path),
            span: None,
            line: None,
            column: None,
        });
    }

    diagnostics
}

// Upstream parity: reduced local analogue for `ERC_TESTER::TestMultUnitPinConflicts()`. This is
// not a 1:1 KiCad marker pass because the Rust tree still uses reduced graph-owned symbol-pin
// inventory plus reduced subgraph ownership instead of live `CONNECTION_SUBGRAPH`-owned `SCH_PIN`
// items. It now consumes the shared graph-owned symbol-pin inventory, including unconnected pins,
// instead of re-projecting lib pins at report time. Remaining divergence is fuller graph
// ownership and KiCad marker attachment.
pub fn check_mult_unit_pin_conflicts(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut pin_to_net = BTreeMap::<String, String>::new();
    let graph = project.reduced_project_net_graph(false);

    for sheet_path in &project.sheet_paths {
        for pin_inventory in
            collect_reduced_project_symbol_pin_inventories_in_sheet(&graph, sheet_path)
        {
            if pin_inventory.unit_count < 2 || pin_inventory.unit.is_none() {
                continue;
            }

            let Some(reference) = pin_inventory
                .pins
                .iter()
                .find_map(|pin| pin.reference.clone())
            else {
                continue;
            };

            for pin in &pin_inventory.pins {
                let Some(ref pin_number) = pin.number else {
                    continue;
                };

                let net_name = reduced_project_symbol_pin_net_name(&graph, pin);
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
// 1:1 KiCad marker pass because the Rust tree still groups reduced graph-owned symbol-pin
// inventory instead of live `SCH_PIN::Connection()` objects, but it now consumes the shared
// graph-owned symbol-pin inventory, including unconnected pins, instead of re-projecting lib pins
// at report time. It preserves the exercised rule: duplicate numbered pins on the same placed
// symbol must not resolve to different nets unless the lib symbol explicitly treats duplicate
// numbers as jumper pins. Remaining divergence is fuller connection-graph ownership and KiCad
// marker/item attachment.
pub fn check_duplicate_pin_nets(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let graph = project.reduced_project_net_graph(false);

    for sheet_path in &project.sheet_paths {
        for pin_inventory in
            collect_reduced_project_symbol_pin_inventories_in_sheet(&graph, sheet_path)
        {
            if pin_inventory.duplicate_pin_numbers_are_jumpers {
                continue;
            }

            let reference = pin_inventory
                .pins
                .iter()
                .find_map(|pin| pin.reference.clone())
                .unwrap_or_else(|| "?".to_string());

            let mut pins_by_number = BTreeMap::<String, Vec<(Option<String>, String)>>::new();

            for pin in &pin_inventory.pins {
                let Some(ref pin_number) = pin.number else {
                    continue;
                };

                let net_name = reduced_project_symbol_pin_net_name(&graph, pin);

                pins_by_number
                    .entry(pin_number.clone())
                    .or_default()
                    .push((pin.name.clone(), net_name));
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
                    path: pin_inventory
                        .pins
                        .first()
                        .map(|pin| pin.schematic_path.clone())
                        .or_else(|| Some(sheet_path.schematic_path.clone())),
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
                message: "Local and global labels have same name".to_string(),
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
// a 1:1 KiCad marker pass because the Rust tree still validates reduced graph-owned pin payloads
// instead of live `SCH_PIN` objects, but it now consumes the shared graph-owned per-symbol pin
// inventory instead of re-projecting symbol pins ad hoc on the ERC path, preserves the exercised
// bracketed stacked-pin syntax rule, and only warns on numbers that resemble stacked notation but
// do not parse like KiCad's helper. Remaining divergence is the fuller live pin object layer and
// marker attachment path.
pub fn check_stacked_pin_notation(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let graph = project.reduced_project_net_graph(false);

    for sheet_path in &project.sheet_paths {
        for pin_inventory in
            collect_reduced_project_symbol_pin_inventories_in_sheet(&graph, sheet_path)
        {
            for pin in &pin_inventory.pins {
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
                    path: Some(pin.schematic_path.clone()),
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
// KiCad marker pass because the Rust tree still checks reduced graph-owned symbol-pin inventory
// through reduced subgraph ownership instead of live `SCH_PIN` connections, but it now consumes
// the shared graph-owned symbol-pin inventory, including unconnected pins, instead of
// re-projecting lib pins at report time. It preserves the exercised rule: once a symbol has a
// real ground net, any `GND`-named power pin on a different net is an ERC error. Remaining
// divergence is fuller connection-graph ownership and richer pin metadata.
pub fn check_ground_pins(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let graph = project.reduced_project_net_graph(false);

    for sheet_path in &project.sheet_paths {
        for pin_inventory in
            collect_reduced_project_symbol_pin_inventories_in_sheet(&graph, sheet_path)
        {
            let mut has_ground_net = false;
            let mut mismatched_pins = Vec::new();

            for pin in &pin_inventory.pins {
                let Some(pin_type) = pin.electrical_type.as_deref() else {
                    continue;
                };

                if !matches!(pin_type, "power_in" | "power_out") {
                    continue;
                }

                let net_name = reduced_project_symbol_pin_net_name(&graph, pin);
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
                    mismatched_pins.push((
                        pin.name.clone().unwrap_or_default(),
                        [f64::from_bits(pin.at.0), f64::from_bits(pin.at.1)],
                    ));
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
                    path: pin_inventory
                        .pins
                        .first()
                        .map(|pin| pin.schematic_path.clone())
                        .or_else(|| Some(sheet_path.schematic_path.clone())),
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
// graph-owned symbol-pin inventory in millimeter coordinates instead of live schematic items in
// KiCad IU, but it preserves the exercised rule: connectable wire endpoints, bus-entry endpoints,
// and non-NC symbol pins must land on the typed schematic connection grid from companion project
// settings. Remaining divergence is fuller item coverage and KiCad marker attachment.
pub fn check_off_grid_endpoints(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let graph = project.reduced_project_net_graph(false);
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
                    if line
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
                        .is_some()
                    {
                        diagnostics.push(Diagnostic {
                            severity: Severity::Warning,
                            code: "erc-endpoint-off-grid",
                            kind: crate::diagnostic::DiagnosticKind::Validation,
                            message: "Symbol pin or wire end off connection grid".to_string(),
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
                            message: "Symbol pin or wire end off connection grid".to_string(),
                            path: Some(schematic.path.clone()),
                            span: None,
                            line: None,
                            column: None,
                        });
                    }
                }
                SchItem::Symbol(symbol) => {
                    let _ = symbol;
                }
                _ => {}
            }
        }

        for pin_inventory in
            collect_reduced_project_symbol_pin_inventories_in_sheet(&graph, sheet_path)
        {
            if pin_inventory
                .pins
                .iter()
                .find(|pin| {
                    pin.electrical_type.as_deref() != Some("no_connect")
                        && !point_is_on_grid(
                            [f64::from_bits(pin.at.0), f64::from_bits(pin.at.1)],
                            grid_size_mm,
                        )
                })
                .is_some()
            {
                diagnostics.push(Diagnostic {
                    severity: Severity::Warning,
                    code: "erc-endpoint-off-grid",
                    kind: crate::diagnostic::DiagnosticKind::Validation,
                    message: "Symbol pin or wire end off connection grid".to_string(),
                    path: pin_inventory
                        .pins
                        .first()
                        .map(|pin| pin.schematic_path.clone())
                        .or_else(|| Some(sheet_path.schematic_path.clone())),
                    span: None,
                    line: None,
                    column: None,
                });
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

#[cfg(test)]
mod tests {
    use super::{ReducedErcPinContext, ReducedPinType, reduced_preferred_missing_driver_pin};
    fn test_pin(
        reference: &str,
        pin_number: &str,
        pin_type: ReducedPinType,
        visible: bool,
        is_power_symbol: bool,
    ) -> ReducedErcPinContext {
        ReducedErcPinContext {
            at: [0.0, 0.0],
            visible,
            is_power_symbol,
            path: std::path::PathBuf::from("root.kicad_sch"),
            sheet_instance_path: "/".to_string(),
            reference: reference.to_string(),
            pin_number: pin_number.to_string(),
            pin_name: Some(pin_number.to_string()),
            symbol_uuid: Some("sym".to_string()),
            pin_type,
        }
    }

    #[test]
    fn missing_driver_prefers_visible_non_power_pin() {
        let pins = vec![
            test_pin("U1", "1", ReducedPinType::Input, false, false),
            test_pin("U2", "1", ReducedPinType::Input, true, false),
            test_pin("#PWR1", "1", ReducedPinType::PowerIn, true, true),
        ];

        let chosen =
            reduced_preferred_missing_driver_pin(&pins, false).expect("preferred missing driver");

        assert_eq!(chosen.reference, "U2");
        assert_eq!(chosen.pin_number, "1");
        assert!(chosen.visible);
        assert!(!chosen.is_power_symbol);
    }

    #[test]
    fn stacked_pin_helper_only_matches_same_symbol_name_type_and_position() {
        let lhs = ReducedErcPinContext {
            at: [10.0, 20.0],
            visible: true,
            is_power_symbol: false,
            path: std::path::PathBuf::from("root.kicad_sch"),
            sheet_instance_path: "/".to_string(),
            reference: "U1".to_string(),
            pin_number: "1".to_string(),
            pin_name: Some("IO".to_string()),
            symbol_uuid: Some("sym".to_string()),
            pin_type: ReducedPinType::Output,
        };
        let mut rhs = ReducedErcPinContext {
            at: lhs.at,
            visible: lhs.visible,
            is_power_symbol: lhs.is_power_symbol,
            path: lhs.path.clone(),
            sheet_instance_path: lhs.sheet_instance_path.clone(),
            reference: lhs.reference.clone(),
            pin_number: "2".to_string(),
            pin_name: lhs.pin_name.clone(),
            symbol_uuid: lhs.symbol_uuid.clone(),
            pin_type: lhs.pin_type,
        };

        assert!(super::reduced_erc_pins_are_stacked(&lhs, &rhs));

        rhs.pin_name = Some("ALT".to_string());
        assert!(!super::reduced_erc_pins_are_stacked(&lhs, &rhs));
    }

    #[test]
    fn stacked_pin_helper_keeps_reused_screen_occurrences_distinct() {
        let lhs = ReducedErcPinContext {
            at: [10.0, 20.0],
            visible: true,
            is_power_symbol: false,
            path: std::path::PathBuf::from("shared_child.kicad_sch"),
            sheet_instance_path: "/A".to_string(),
            reference: "U1".to_string(),
            pin_number: "2".to_string(),
            pin_name: Some("OUT".to_string()),
            symbol_uuid: Some("shared-sym".to_string()),
            pin_type: ReducedPinType::Output,
        };
        let rhs = ReducedErcPinContext {
            at: lhs.at,
            visible: lhs.visible,
            is_power_symbol: lhs.is_power_symbol,
            path: lhs.path.clone(),
            sheet_instance_path: "/B".to_string(),
            reference: "U2".to_string(),
            pin_number: lhs.pin_number.clone(),
            pin_name: lhs.pin_name.clone(),
            symbol_uuid: lhs.symbol_uuid.clone(),
            pin_type: lhs.pin_type,
        };

        assert!(!super::reduced_erc_pins_are_stacked(&lhs, &rhs));
    }
}
