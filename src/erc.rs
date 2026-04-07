use crate::core::SchematicProject;
use crate::diagnostic::{Diagnostic, Severity};
use crate::loader::{
    collect_wire_segments, point_on_wire_segment, points_equal, resolve_cross_reference_text_var,
    resolve_label_connectivity_text_var, resolve_label_text_token_without_connectivity,
    resolve_sheet_text_var, resolve_text_variables, resolved_sheet_text_state,
    resolved_symbol_text_state,
};
use crate::model::{LabelKind, MirrorAxis, Property, PropertyKind, SchItem, Schematic, Symbol};
use std::collections::BTreeMap;

// Upstream parity: local entrypoint for the implemented `ERC_TESTER` slice. This is not a 1:1
// KiCad ERC runner because the current tree still lacks markers, the full pin-conflict matrix, and
// full `CONNECTION_GRAPH` ownership. It exists so ERC work can proceed in upstream routine order
// against real loaded schematic state instead of ad-hoc checks. Remaining divergence is the
// broader unported `ERC_TESTER` surface beyond the reduced rules currently implemented here.
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
    diagnostics.extend(check_no_connect_pins(project));
    diagnostics.extend(check_pin_to_pin(project));
    diagnostics.extend(check_similar_labels(project));
    diagnostics.extend(check_same_local_global_label(project));
    diagnostics.extend(check_field_name_whitespace(project));
    diagnostics
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct PointKey(u64, u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConnectionMemberKind {
    SymbolPin,
    SheetPin,
    Wire,
    Label,
    Junction,
    NoConnectMarker,
}

#[derive(Clone, Debug)]
struct ConnectionMember {
    kind: ConnectionMemberKind,
    at: [f64; 2],
    symbol_uuid: Option<String>,
    visible: bool,
    electrical_type: Option<String>,
}

#[derive(Clone, Debug)]
struct ConnectionPointSnapshot {
    at: [f64; 2],
    members: Vec<ConnectionMember>,
}

#[derive(Clone, Debug)]
struct ConnectionComponent {
    anchor: [f64; 2],
    members: Vec<ConnectionMember>,
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

fn point_key(at: [f64; 2]) -> PointKey {
    PointKey(at[0].to_bits(), at[1].to_bits())
}

fn rotate_point(point: [f64; 2], angle_degrees: f64) -> [f64; 2] {
    let radians = angle_degrees.to_radians();
    let (sin, cos) = radians.sin_cos();
    [
        (point[0] * cos) - (point[1] * sin),
        (point[0] * sin) + (point[1] * cos),
    ]
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

// Upstream parity: reduced local helper for `SCH_SYMBOL::GetPins( &sheet )` point projection. This
// is not a 1:1 KiCad pin object path because the Rust tree still lacks live `SCH_PIN` instances on
// placed symbols. It exists so the reduced ERC connection snapshot can include placed symbol pins
// from linked lib-pin draw items instead of falling back to wire-only geometry.
fn projected_symbol_pins(symbol: &Symbol) -> Vec<ConnectionMember> {
    let Some(lib_symbol) = symbol.lib_symbol.as_ref() else {
        return Vec::new();
    };

    let unit_number = symbol.unit.unwrap_or(1);
    let body_style = symbol.body_style.unwrap_or(1);
    let symbol_uuid = symbol.uuid.clone();

    lib_symbol
        .units
        .iter()
        .filter(|unit| unit.unit_number == unit_number && unit.body_style == body_style)
        .flat_map(|unit| unit.draw_items.iter())
        .filter(|item| item.kind == "pin")
        .map(|pin| {
            let mut local_at = pin.at.unwrap_or([0.0, 0.0]);

            match symbol.mirror {
                Some(MirrorAxis::X) => local_at[1] = -local_at[1],
                Some(MirrorAxis::Y) => local_at[0] = -local_at[0],
                None => {}
            }

            let rotated = rotate_point(local_at, symbol.angle);
            let at = [symbol.at[0] + rotated[0], symbol.at[1] + rotated[1]];

            ConnectionMember {
                kind: ConnectionMemberKind::SymbolPin,
                at,
                symbol_uuid: symbol_uuid.clone(),
                visible: pin.visible,
                electrical_type: pin.electrical_type.clone(),
            }
        })
        .collect()
}

fn push_connection_member(
    snapshot: &mut BTreeMap<PointKey, ConnectionPointSnapshot>,
    member: ConnectionMember,
) {
    let key = point_key(member.at);
    let entry = snapshot
        .entry(key)
        .or_insert_with(|| ConnectionPointSnapshot {
            at: member.at,
            members: Vec::new(),
        });

    if member.kind == ConnectionMemberKind::SymbolPin {
        if let Some(existing) = entry.members.iter_mut().find(|existing| {
            existing.kind == ConnectionMemberKind::SymbolPin
                && existing.symbol_uuid == member.symbol_uuid
        }) {
            if member.visible && !existing.visible {
                *existing = member;
            }

            return;
        }
    }

    entry.members.push(member);
}

// Upstream parity: reduced local analogue for the connection-point map built inside
// `ERC_TESTER::TestFourWayJunction()` / `TestNoConnectPins()`. This is not a 1:1
// `CONNECTION_GRAPH` port because the Rust tree still lacks KiCad's full subgraph ownership, but
// it is needed so the remaining connection-driven ERC rules can run on pins, wires, sheet pins,
// labels, junctions, and no-connect markers together instead of repeating isolated geometry scans.
// Remaining divergence is fuller graph/subgraph ownership and item-class coverage beyond the
// current ERC slice.
fn collect_connection_points(schematic: &Schematic) -> BTreeMap<PointKey, ConnectionPointSnapshot> {
    let mut snapshot = BTreeMap::new();

    for item in &schematic.screen.items {
        match item {
            SchItem::Symbol(symbol) => {
                for pin in projected_symbol_pins(symbol) {
                    push_connection_member(&mut snapshot, pin);
                }
            }
            SchItem::Sheet(sheet) => {
                for pin in &sheet.pins {
                    push_connection_member(
                        &mut snapshot,
                        ConnectionMember {
                            kind: ConnectionMemberKind::SheetPin,
                            at: pin.at,
                            symbol_uuid: None,
                            visible: pin.visible,
                            electrical_type: None,
                        },
                    );
                }
            }
            SchItem::Wire(line) => {
                for point in &line.points {
                    push_connection_member(
                        &mut snapshot,
                        ConnectionMember {
                            kind: ConnectionMemberKind::Wire,
                            at: *point,
                            symbol_uuid: None,
                            visible: true,
                            electrical_type: None,
                        },
                    );
                }
            }
            SchItem::Label(label) => {
                push_connection_member(
                    &mut snapshot,
                    ConnectionMember {
                        kind: ConnectionMemberKind::Label,
                        at: label.at,
                        symbol_uuid: None,
                        visible: true,
                        electrical_type: None,
                    },
                );
            }
            SchItem::Junction(junction) => {
                push_connection_member(
                    &mut snapshot,
                    ConnectionMember {
                        kind: ConnectionMemberKind::Junction,
                        at: junction.at,
                        symbol_uuid: None,
                        visible: true,
                        electrical_type: None,
                    },
                );
            }
            SchItem::NoConnect(no_connect) => {
                push_connection_member(
                    &mut snapshot,
                    ConnectionMember {
                        kind: ConnectionMemberKind::NoConnectMarker,
                        at: no_connect.at,
                        symbol_uuid: None,
                        visible: true,
                        electrical_type: None,
                    },
                );
            }
            _ => {}
        }
    }

    snapshot
}

fn segment_components(segments: &[[[f64; 2]; 2]], junctions: &[[f64; 2]]) -> Vec<Vec<usize>> {
    let mut components = Vec::new();
    let mut seen = vec![false; segments.len()];

    for start in 0..segments.len() {
        if seen[start] {
            continue;
        }

        let mut stack = vec![start];
        let mut component = Vec::new();
        seen[start] = true;

        while let Some(current) = stack.pop() {
            component.push(current);
            let current_segment = segments[current];

            for (candidate, other) in segments.iter().enumerate() {
                if seen[candidate] {
                    continue;
                }

                let shares_endpoint = points_equal(current_segment[0], other[0])
                    || points_equal(current_segment[0], other[1])
                    || points_equal(current_segment[1], other[0])
                    || points_equal(current_segment[1], other[1]);
                let joined_by_junction = junctions.iter().copied().any(|junction| {
                    point_on_wire_segment(junction, current_segment[0], current_segment[1])
                        && point_on_wire_segment(junction, other[0], other[1])
                });

                if !shares_endpoint && !joined_by_junction {
                    continue;
                }

                seen[candidate] = true;
                stack.push(candidate);
            }
        }

        components.push(component);
    }

    components
}

struct DisjointSet {
    parent: Vec<usize>,
}

impl DisjointSet {
    fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
        }
    }

    fn find(&mut self, index: usize) -> usize {
        if self.parent[index] != index {
            let root = self.find(self.parent[index]);
            self.parent[index] = root;
        }

        self.parent[index]
    }

    fn union(&mut self, lhs: usize, rhs: usize) {
        let lhs_root = self.find(lhs);
        let rhs_root = self.find(rhs);

        if lhs_root != rhs_root {
            self.parent[rhs_root] = lhs_root;
        }
    }
}

// Upstream parity: reduced local analogue for the sheet-local connectivity grouping behind
// `CONNECTION_GRAPH` subgraphs. This is not a 1:1 graph owner because the Rust tree still lacks
// KiCad's full subgraph/netcode/driver objects. It exists so reduced ERC work can ask "which
// points/items are on the same net-like component?" instead of repeatedly walking raw wire
// geometry. Remaining divergence is fuller connection-driver ownership and bus/subgraph semantics.
fn collect_connection_components(schematic: &Schematic) -> Vec<ConnectionComponent> {
    let point_snapshot = collect_connection_points(schematic);
    let points = point_snapshot.into_values().collect::<Vec<_>>();
    let segments = collect_wire_segments(schematic);
    let junctions = schematic
        .screen
        .items
        .iter()
        .filter_map(|item| match item {
            SchItem::Junction(junction) => Some(junction.at),
            _ => None,
        })
        .collect::<Vec<_>>();
    let segment_components = segment_components(&segments, &junctions);

    let mut dsu = DisjointSet::new(points.len());
    let mut component_points = BTreeMap::<usize, Vec<usize>>::new();

    for (segment_component_index, segment_component) in segment_components.iter().enumerate() {
        for (point_index, point) in points.iter().enumerate() {
            if segment_component.iter().copied().any(|segment_index| {
                let segment = segments[segment_index];
                point_on_wire_segment(point.at, segment[0], segment[1])
            }) {
                component_points
                    .entry(segment_component_index)
                    .or_default()
                    .push(point_index);
            }
        }
    }

    for point_indexes in component_points.values() {
        for pair in point_indexes.windows(2) {
            dsu.union(pair[0], pair[1]);
        }
    }

    let mut groups = BTreeMap::<usize, ConnectionComponent>::new();

    for (index, point) in points.into_iter().enumerate() {
        let root = dsu.find(index);
        let entry = groups.entry(root).or_insert_with(|| ConnectionComponent {
            anchor: point.at,
            members: Vec::new(),
        });
        entry.members.extend(point.members);
    }

    groups.into_values().collect()
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
// symbol pins and wire endpoints instead of a wire-only geometry shortcut. Remaining divergence is
// fuller connection-graph ownership and broader item-class participation beyond the exercised ERC
// slice.
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

// Upstream parity: reduced local analogue for `ERC_TESTER::TestPinToPin()`. This is not a 1:1
// KiCad pin-matrix runner because the Rust tree still lacks `ERC_SETTINGS`, graph-owned pin
// contexts, marker placement heuristics, and the full connection graph. It exists so the current
// ERC path can start using upstream default pin-type conflict semantics on reduced same-net
// components instead of stopping at point-local checks. Remaining divergence is richer settings,
// driver-missing reporting, and full subgraph ownership.
pub fn check_pin_to_pin(project: &SchematicProject) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for sheet_path in &project.sheet_paths {
        let Some(schematic) = project
            .schematics
            .iter()
            .find(|schematic| schematic.path == sheet_path.schematic_path)
        else {
            continue;
        };

        for component in collect_connection_components(schematic) {
            let pins = component
                .members
                .iter()
                .filter(|member| member.kind == ConnectionMemberKind::SymbolPin)
                .filter_map(|member| {
                    parse_reduced_pin_type(member.electrical_type.as_deref()?)
                        .map(|pin_type| (member.at, pin_type))
                })
                .collect::<Vec<_>>();

            if pins.len() < 2 {
                continue;
            }

            let is_power_net = pins
                .iter()
                .any(|(_, pin_type)| *pin_type == ReducedPinType::PowerIn);
            let has_driver = pins.iter().any(|(_, pin_type)| {
                if is_power_net {
                    is_power_driver_pin_type(*pin_type)
                } else {
                    is_normal_driver_pin_type(*pin_type)
                }
            });
            let has_noconnect = component
                .members
                .iter()
                .any(|member| member.kind == ConnectionMemberKind::NoConnectMarker);

            for (index, (_at, lhs_type)) in pins.iter().enumerate() {
                for (_, rhs_type) in pins.iter().skip(index + 1) {
                    let conflict = pin_conflict(*lhs_type, *rhs_type);
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
                            component.anchor[0], component.anchor[1]
                        ),
                        path: Some(schematic.path.clone()),
                        span: None,
                        line: None,
                        column: None,
                    });
                    break;
                }
            }

            if has_driver || has_noconnect {
                continue;
            }

            if let Some((_, pin_type)) = pins
                .iter()
                .find(|(_, pin_type)| is_driven_pin_type(*pin_type))
            {
                let article = if *pin_type == ReducedPinType::PowerIn {
                    "Power input pin is not driven"
                } else {
                    "Input pin is not driven"
                };

                diagnostics.push(Diagnostic {
                    severity: Severity::Warning,
                    code: "erc-missing-driver",
                    kind: crate::diagnostic::DiagnosticKind::Validation,
                    message: article.to_string(),
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
