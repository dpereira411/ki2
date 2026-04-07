use std::collections::BTreeMap;

use crate::loader::{collect_wire_segments, point_on_wire_segment, points_equal};
use crate::model::{
    Label, LabelKind, MirrorAxis, SchItem, Schematic, Shape, ShapeKind, SheetPinShape, Symbol,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PointKey(pub(crate) u64, pub(crate) u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConnectionMemberKind {
    SymbolPin,
    SheetPin,
    Wire,
    BusEntry,
    Label,
    Junction,
    NoConnectMarker,
}

#[derive(Clone, Debug)]
pub(crate) struct ConnectionMember {
    pub(crate) kind: ConnectionMemberKind,
    pub(crate) at: [f64; 2],
    pub(crate) symbol_uuid: Option<String>,
    pub(crate) visible: bool,
    pub(crate) electrical_type: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct ConnectionPointSnapshot {
    pub(crate) at: [f64; 2],
    pub(crate) members: Vec<ConnectionMember>,
}

#[derive(Clone, Debug)]
pub(crate) struct ConnectionComponent {
    pub(crate) anchor: [f64; 2],
    pub(crate) members: Vec<ConnectionMember>,
}

#[derive(Clone, Debug)]
pub(crate) struct ReducedLabelComponentLabel {
    pub(crate) at: [f64; 2],
    pub(crate) kind: LabelKind,
    pub(crate) dangling: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ReducedLabelComponentSnapshot {
    pub(crate) anchor: [f64; 2],
    pub(crate) net_name: Option<String>,
    pub(crate) pin_count: usize,
    pub(crate) has_no_connect: bool,
    pub(crate) has_local_hierarchy: bool,
    pub(crate) labels: Vec<ReducedLabelComponentLabel>,
}

#[derive(Clone, Debug)]
pub(crate) struct ProjectedSymbolPin {
    pub(crate) at: [f64; 2],
    pub(crate) name: Option<String>,
    pub(crate) number: Option<String>,
    pub(crate) electrical_type: Option<String>,
}

fn connection_component_at(schematic: &Schematic, at: [f64; 2]) -> Option<ConnectionComponent> {
    collect_connection_components(schematic)
        .into_iter()
        .find(|component| {
            component
                .members
                .iter()
                .any(|member| points_equal(member.at, at))
        })
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

// Upstream parity: reduced local analogue for the placed-symbol `SCH_PIN` projection KiCad uses
// across ERC and export code. This is not a 1:1 live-pin object path because the Rust tree still
// stores pins only on linked lib draw items, but it preserves the exercised unit/body-style pin
// projection and pin text payload needed by ERC checks, shown-text resolution, and net export.
pub(crate) fn projected_symbol_pin_info(symbol: &Symbol) -> Vec<ProjectedSymbolPin> {
    let Some(lib_symbol) = symbol.lib_symbol.as_ref() else {
        return Vec::new();
    };

    let unit_number = symbol.unit.unwrap_or(1);
    let body_style = symbol.body_style.unwrap_or(1);
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

            ProjectedSymbolPin {
                at,
                name: pin.name.clone(),
                number: pin.number.clone(),
                electrical_type: pin.electrical_type.clone(),
            }
        })
        .collect()
}

// Upstream parity: reduced local helper for `SCH_SYMBOL::GetPins( &sheet )` point projection. This
// is not a 1:1 KiCad pin object path because the Rust tree still lacks live `SCH_PIN` instances on
// placed symbols. It exists so the shared reduced connectivity owner can include placed symbol pins
// from linked lib-pin draw items instead of falling back to wire-only geometry.
fn projected_symbol_pins(symbol: &Symbol) -> Vec<ConnectionMember> {
    projected_symbol_pin_info(symbol)
        .into_iter()
        .map(|pin| ConnectionMember {
            kind: ConnectionMemberKind::SymbolPin,
            at: pin.at,
            symbol_uuid: symbol.uuid.clone(),
            visible: true,
            electrical_type: pin.electrical_type,
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
// it is needed so ERC, shown-text, and net export can run on one shared connection owner instead of
// repeating isolated geometry scans in each caller.
pub(crate) fn collect_connection_points(
    schematic: &Schematic,
) -> BTreeMap<PointKey, ConnectionPointSnapshot> {
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
            SchItem::Wire(line) | SchItem::Bus(line) => {
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
            SchItem::BusEntry(entry) => {
                for point in [
                    entry.at,
                    [entry.at[0] + entry.size[0], entry.at[1] + entry.size[1]],
                ] {
                    push_connection_member(
                        &mut snapshot,
                        ConnectionMember {
                            kind: ConnectionMemberKind::BusEntry,
                            at: point,
                            symbol_uuid: entry.uuid.clone(),
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
// KiCad's full subgraph/netcode/driver objects. It exists so loader, ERC, and export can share
// one grouped connection carrier instead of each rebuilding its own net-like component queries.
pub(crate) fn collect_connection_components(schematic: &Schematic) -> Vec<ConnectionComponent> {
    let point_snapshot = collect_connection_points(schematic);
    let points = point_snapshot.into_values().collect::<Vec<_>>();
    let mut segments = collect_wire_segments(schematic);
    segments.extend(
        schematic
            .screen
            .items
            .iter()
            .filter_map(|item| match item {
                SchItem::Bus(line) => Some(
                    line.points
                        .windows(2)
                        .filter_map(|pair| {
                            (!points_equal(pair[0], pair[1])).then_some([pair[0], pair[1]])
                        })
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .flatten(),
    );
    segments.extend(schematic.screen.items.iter().filter_map(|item| match item {
        SchItem::BusEntry(entry) => Some([
            entry.at,
            [entry.at[0] + entry.size[0], entry.at[1] + entry.size[1]],
        ]),
        _ => None,
    }));
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

fn label_is_dangling_on_component(
    schematic: &Schematic,
    connected_component: &ConnectionComponent,
    at: [f64; 2],
) -> bool {
    if connected_component.members.iter().any(|member| {
        points_equal(member.at, at)
            && matches!(
                member.kind,
                ConnectionMemberKind::SymbolPin
                    | ConnectionMemberKind::SheetPin
                    | ConnectionMemberKind::NoConnectMarker
            )
    }) {
        return false;
    }

    !collect_wire_segments(schematic)
        .iter()
        .any(|segment| point_on_wire_segment(at, segment[0], segment[1]))
}

// Upstream parity: reduced local analogue for the label-bearing subgraph facts consumed by
// `CONNECTION_GRAPH::ercCheckLabels()`. This is not a 1:1 KiCad subgraph snapshot because the Rust
// tree still lacks global net-name neighbors, bus parents, and live `SCH_TEXT::IsDangling()`
// state. It exists so the shared reduced connectivity owner can provide label/pin/no-connect
// component facts to ERC instead of rebuilding them inside another local label scan.
pub(crate) fn collect_reduced_label_component_snapshots<F>(
    schematic: &Schematic,
    mut shown_label_text: F,
) -> Vec<ReducedLabelComponentSnapshot>
where
    F: FnMut(&Label) -> String,
{
    collect_connection_components(schematic)
        .into_iter()
        .filter_map(|connected_component| {
            let labels = schematic
                .screen
                .items
                .iter()
                .filter_map(|item| match item {
                    SchItem::Label(label)
                        if connected_component.members.iter().any(|member| {
                            member.kind == ConnectionMemberKind::Label
                                && points_equal(member.at, label.at)
                        }) =>
                    {
                        Some(ReducedLabelComponentLabel {
                            at: label.at,
                            kind: label.kind,
                            dangling: label_is_dangling_on_component(
                                schematic,
                                &connected_component,
                                label.at,
                            ),
                        })
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();

            if labels.is_empty() {
                return None;
            }

            Some(ReducedLabelComponentSnapshot {
                anchor: connected_component.anchor,
                net_name: resolve_reduced_net_name_at(
                    schematic,
                    connected_component.anchor,
                    |label| shown_label_text(label),
                ),
                pin_count: connected_component
                    .members
                    .iter()
                    .filter(|member| member.kind == ConnectionMemberKind::SymbolPin)
                    .count(),
                has_no_connect: connected_component
                    .members
                    .iter()
                    .any(|member| member.kind == ConnectionMemberKind::NoConnectMarker),
                has_local_hierarchy: connected_component
                    .members
                    .iter()
                    .any(|member| member.kind == ConnectionMemberKind::SheetPin)
                    || labels
                        .iter()
                        .any(|label| label.kind == LabelKind::Hierarchical),
                labels,
            })
        })
        .collect()
}

fn connected_wire_segment_indices(
    segments: &[[[f64; 2]; 2]],
    junctions: &[[f64; 2]],
    anchor: [f64; 2],
) -> Vec<usize> {
    let mut connected = Vec::new();
    let mut frontier = Vec::new();

    for (index, segment) in segments.iter().enumerate() {
        if point_on_wire_segment(anchor, segment[0], segment[1]) {
            connected.push(index);
            frontier.push(index);
        }
    }

    while let Some(current) = frontier.pop() {
        let segment = segments[current];

        for (index, other) in segments.iter().enumerate() {
            if connected.contains(&index) {
                continue;
            }

            let shares_endpoint = points_equal(segment[0], other[0])
                || points_equal(segment[0], other[1])
                || points_equal(segment[1], other[0])
                || points_equal(segment[1], other[1]);
            let joined_by_junction = junctions.iter().copied().any(|junction| {
                point_on_wire_segment(junction, segment[0], segment[1])
                    && point_on_wire_segment(junction, other[0], other[1])
            });

            if !shares_endpoint && !joined_by_junction {
                continue;
            }

            connected.push(index);
            frontier.push(index);
        }
    }

    connected.sort_unstable();
    connected.dedup();
    connected
}

fn points_share_segment(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    points_equal(a, c) || points_equal(a, d) || points_equal(b, c) || points_equal(b, d)
}

fn segment_orientation(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    ((b[0] - a[0]) * (c[1] - a[1])) - ((b[1] - a[1]) * (c[0] - a[0]))
}

fn segment_intersects_segment(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let o1 = segment_orientation(a, b, c);
    let o2 = segment_orientation(a, b, d);
    let o3 = segment_orientation(c, d, a);
    let o4 = segment_orientation(c, d, b);

    if o1.abs() < 1e-9 && point_on_wire_segment(c, a, b) {
        return true;
    }
    if o2.abs() < 1e-9 && point_on_wire_segment(d, a, b) {
        return true;
    }
    if o3.abs() < 1e-9 && point_on_wire_segment(a, c, d) {
        return true;
    }
    if o4.abs() < 1e-9 && point_on_wire_segment(b, c, d) {
        return true;
    }

    ((o1 > 0.0 && o2 < 0.0) || (o1 < 0.0 && o2 > 0.0))
        && ((o3 > 0.0 && o4 < 0.0) || (o3 < 0.0 && o4 > 0.0))
}

fn point_in_polygon(point: [f64; 2], polygon: &[[f64; 2]]) -> bool {
    if polygon.len() < 3 {
        return false;
    }

    let mut inside = false;

    for index in 0..polygon.len() {
        let start = polygon[index];
        let end = polygon[(index + 1) % polygon.len()];

        if point_on_wire_segment(point, start, end) {
            return true;
        }

        let intersects = ((start[1] > point[1]) != (end[1] > point[1]))
            && (point[0]
                < ((end[0] - start[0]) * (point[1] - start[1]) / (end[1] - start[1])) + start[0]);

        if intersects {
            inside = !inside;
        }
    }

    inside
}

// Upstream parity: reduced local analogue for the rule-area/netclass geometry half of
// `SCH_CONNECTION_GRAPH::GetNetclassesForDriver()`. This is not a 1:1 KiCad rule-area owner
// because the Rust tree still lacks cached rule-area membership and full subgraph objects. It is
// needed so the shared reduced connectivity owner can decide which directive/rule-area netclass
// providers apply to a point instead of leaving that ownership split across loader callers.
fn rule_area_contains_connected_component(
    rule_area: &Shape,
    at: [f64; 2],
    wire_segments: &[[[f64; 2]; 2]],
    connected_segments: &[usize],
) -> bool {
    if rule_area.kind != ShapeKind::RuleArea || rule_area.points.len() < 3 {
        return false;
    }

    if point_in_polygon(at, &rule_area.points) {
        return true;
    }

    connected_segments.iter().copied().any(|segment_index| {
        let segment = wire_segments[segment_index];

        if point_in_polygon(segment[0], &rule_area.points)
            || point_in_polygon(segment[1], &rule_area.points)
        {
            return true;
        }

        rule_area.points.iter().enumerate().any(|(index, start)| {
            let end = rule_area.points[(index + 1) % rule_area.points.len()];

            if points_share_segment(segment[0], segment[1], *start, end) {
                return false;
            }

            segment_intersects_segment(segment[0], segment[1], *start, end)
        })
    })
}

fn reduced_label_driver_priority(label: &Label) -> i32 {
    match label.kind {
        LabelKind::Global => 7,
        LabelKind::Local => 4,
        LabelKind::Hierarchical => 3,
        LabelKind::Directive => 0,
    }
}

fn reduced_sheet_pin_driver_rank(shape: SheetPinShape) -> i32 {
    match shape {
        SheetPinShape::Output => 1,
        SheetPinShape::Input
        | SheetPinShape::Bidirectional
        | SheetPinShape::TriState
        | SheetPinShape::Unspecified => 0,
    }
}

fn reduced_power_pin_driver_priority(
    symbol: &Symbol,
    electrical_type: Option<&str>,
) -> Option<i32> {
    let lib_symbol = symbol.lib_symbol.as_ref()?;

    if electrical_type != Some("power_in") || !lib_symbol.power {
        return None;
    }

    Some(if lib_symbol.local_power { 5 } else { 6 })
}

fn symbol_value_text(symbol: &Symbol) -> Option<String> {
    symbol
        .properties
        .iter()
        .find(|property| property.kind == crate::model::PropertyKind::SymbolValue)
        .map(|property| property.value.clone())
}

fn symbol_reference_text(symbol: &Symbol) -> Option<String> {
    symbol
        .properties
        .iter()
        .find(|property| property.kind == crate::model::PropertyKind::SymbolReference)
        .map(|property| property.value.clone())
}

fn reduced_symbol_pin_default_net_name(
    symbol: &Symbol,
    pin: &ProjectedSymbolPin,
    unit_pins: &[ProjectedSymbolPin],
) -> Option<String> {
    let reference = symbol_reference_text(symbol)?;
    let pin_number = pin.number.as_deref()?;

    if reference.ends_with('?') {
        let symbol_uuid = symbol.uuid.as_deref()?;
        return Some(format!("Net-({symbol_uuid}-Pad{pin_number})"));
    }

    let pin_name = pin
        .name
        .as_deref()
        .filter(|name| !name.is_empty() && *name != pin_number && *name != "~");
    let name_is_duplicated = pin_name.is_some_and(|name| {
        unit_pins.iter().any(|other| {
            other.number.as_deref() != Some(pin_number) && other.name.as_deref() == Some(name)
        })
    });

    if let Some(pin_name) = pin_name {
        let mut name = format!("Net-({reference}-{pin_name}");

        if name_is_duplicated {
            name.push_str(&format!("-Pad{pin_number}"));
        }

        name.push(')');
        return Some(name);
    }

    Some(format!("Net-({reference}-Pad{pin_number})"))
}

fn label_uses_connectivity_dependent_text(label: &Label) -> bool {
    let text = label.text.to_ascii_uppercase();

    text.contains("NET_NAME")
        || text.contains("SHORT_NET_NAME")
        || text.contains("NET_CLASS")
        || text.contains("CONNECTION_TYPE")
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReducedStrongDriver {
    priority: i32,
    name: String,
}

fn collect_reduced_strong_drivers<F>(
    schematic: &Schematic,
    connected_component: &ConnectionComponent,
    mut shown_label_text: F,
) -> Vec<ReducedStrongDriver>
where
    F: FnMut(&Label) -> String,
{
    let mut drivers = schematic
        .screen
        .items
        .iter()
        .filter_map(|item| match item {
            SchItem::Label(label)
                if label.kind != LabelKind::Directive
                    && !label_uses_connectivity_dependent_text(label)
                    && connected_component.members.iter().any(|member| {
                        member.kind == ConnectionMemberKind::Label
                            && points_equal(member.at, label.at)
                    }) =>
            {
                let text = shown_label_text(label);
                Some(ReducedStrongDriver {
                    priority: reduced_label_driver_priority(label),
                    name: text,
                })
            }
            SchItem::Symbol(symbol) => {
                let unit_pins = projected_symbol_pin_info(symbol);

                unit_pins
                    .iter()
                    .filter(|pin| {
                        connected_component.members.iter().any(|member| {
                            member.kind == ConnectionMemberKind::SymbolPin
                                && member.symbol_uuid == symbol.uuid
                                && points_equal(member.at, pin.at)
                        })
                    })
                    .find_map(|pin| {
                        reduced_power_pin_driver_priority(symbol, pin.electrical_type.as_deref())
                            .and_then(|priority| {
                                symbol_value_text(symbol).map(|text| ReducedStrongDriver {
                                    priority,
                                    name: text,
                                })
                            })
                    })
            }
            _ => None,
        })
        .filter(|driver| {
            !driver.name.is_empty() && !driver.name.contains("${") && !driver.name.starts_with('<')
        })
        .collect::<Vec<_>>();

    drivers.sort_by(|lhs, rhs| {
        rhs.priority
            .cmp(&lhs.priority)
            .then_with(|| lhs.name.cmp(&rhs.name))
    });
    drivers
}

// Upstream parity: reduced local analogue for the connected-driver naming part of
// `CONNECTION_SUBGRAPH::ResolveDrivers()` plus `driverName()/GetNameForDriver()`. This is not a
// 1:1 KiCad driver owner because the Rust tree still lacks full subgraphs, sheet pins, power-pin
// drivers, and cached `SCH_CONNECTION` objects. It exists so loader shown-text and export paths do
// not each pick the "first connected label" independently. The current reduced driver ranking is
// limited to the driver kinds the Rust tree can already model on one sheet:
// - global labels outrank global power pins
// - global power pins outrank local power pins
// - local power pins outrank local labels
// - local labels outrank hierarchical labels
// - sheet pins participate below labels, with output pins preferred over non-output pins
// - ordinary symbol pins participate last through reduced `SCH_PIN::GetDefaultNetName()`-style
//   fallback names so unlabeled nets still get deterministic export/CLI names
// - equal-priority drivers fall back to alphabetical shown text for deterministic parity instead
//   of file order
// - labels whose raw text still depends on the reduced connectivity resolver are skipped so the
//   current reduced driver path does not recurse back into itself
pub(crate) fn resolve_reduced_net_name_at<F>(
    schematic: &Schematic,
    at: [f64; 2],
    mut shown_label_text: F,
) -> Option<String>
where
    F: FnMut(&Label) -> String,
{
    let connected_component = connection_component_at(schematic, at)?;
    let mut candidates = schematic
        .screen
        .items
        .iter()
        .filter_map(|item| match item {
            SchItem::Label(label)
                if label.kind != LabelKind::Directive
                    && !label_uses_connectivity_dependent_text(label)
                    && connected_component.members.iter().any(|member| {
                        member.kind == ConnectionMemberKind::Label
                            && points_equal(member.at, label.at)
                    }) =>
            {
                let text = shown_label_text(label);
                Some((reduced_label_driver_priority(label), 0, text))
            }
            SchItem::Sheet(sheet) => sheet
                .pins
                .iter()
                .filter(|pin| {
                    connected_component.members.iter().any(|member| {
                        member.kind == ConnectionMemberKind::SheetPin
                            && points_equal(member.at, pin.at)
                    })
                })
                .map(|pin| {
                    (
                        0,
                        reduced_sheet_pin_driver_rank(pin.shape),
                        pin.name.clone(),
                    )
                })
                .max_by(|lhs, rhs| lhs.1.cmp(&rhs.1).then_with(|| rhs.2.cmp(&lhs.2))),
            SchItem::Symbol(symbol) => {
                let unit_pins = projected_symbol_pin_info(symbol);

                unit_pins
                    .iter()
                    .cloned()
                    .filter_map(|pin| {
                        connected_component
                            .members
                            .iter()
                            .any(|member| {
                                member.kind == ConnectionMemberKind::SymbolPin
                                    && member.symbol_uuid == symbol.uuid
                                    && points_equal(member.at, pin.at)
                            })
                            .then_some(pin)
                    })
                    .find_map(|pin| {
                        reduced_power_pin_driver_priority(symbol, pin.electrical_type.as_deref())
                            .and_then(|priority| {
                                symbol_value_text(symbol).map(|text| (priority, 0, text))
                            })
                            .or_else(|| {
                                reduced_symbol_pin_default_net_name(symbol, &pin, &unit_pins)
                                    .map(|text| (1, 0, text))
                            })
                    })
            }
            _ => None,
        })
        .filter(|(_, _, text)| !text.is_empty() && !text.contains("${") && !text.starts_with('<'))
        .collect::<Vec<_>>();

    candidates.sort_by(
        |(lhs_priority, lhs_sheet_pin_rank, lhs_text),
         (rhs_priority, rhs_sheet_pin_rank, rhs_text)| {
            rhs_priority
                .cmp(lhs_priority)
                .then_with(|| rhs_sheet_pin_rank.cmp(lhs_sheet_pin_rank))
                .then_with(|| lhs_text.cmp(rhs_text))
        },
    );

    candidates.into_iter().map(|(_, _, text)| text).next()
}

// Upstream parity: reduced local analogue for the strong-driver conflict part of
// `CONNECTION_SUBGRAPH::ResolveDrivers()` plus `ercCheckMultipleDrivers()`. This is not a 1:1
// KiCad subgraph conflict owner because the Rust tree still lacks full subgraphs, cached driver
// item identity, and marker placement. It exists so ERC can report the exercised case where two
// different strong driver names are attached to one reduced connected component and one wins the
// net name according to the shared driver ranking.
pub(crate) fn resolve_reduced_driver_conflict_at<F>(
    schematic: &Schematic,
    at: [f64; 2],
    shown_label_text: F,
) -> Option<(String, String)>
where
    F: FnMut(&Label) -> String,
{
    let connected_component = connection_component_at(schematic, at)?;
    let drivers = collect_reduced_strong_drivers(schematic, &connected_component, shown_label_text);
    let primary = drivers.first()?;
    let secondary = drivers
        .iter()
        .skip(1)
        .find(|driver| driver.name != primary.name)?;

    Some((primary.name.clone(), secondary.name.clone()))
}

// Upstream parity: reduced local analogue for the driver-netclass lookup side of
// `CONNECTION_SUBGRAPH::GetNetclassesForDriver()`. This is not a 1:1 KiCad graph query because the
// Rust tree still lacks cached rule-area ownership, child-item traversal, and full subgraph
// drivers. It exists so loader shown-text and export paths query one shared reduced connectivity
// owner for current-sheet `NET_CLASS` instead of rebuilding directive/rule-area scans locally.
pub(crate) fn resolve_reduced_netclass_at<F>(
    schematic: &Schematic,
    at: [f64; 2],
    mut directive_netclass: F,
) -> Option<String>
where
    F: FnMut(&Label) -> Option<String>,
{
    let wire_segments = collect_wire_segments(schematic);
    let junctions = schematic
        .screen
        .items
        .iter()
        .filter_map(|item| match item {
            SchItem::Junction(junction) => Some(junction.at),
            _ => None,
        })
        .collect::<Vec<_>>();
    let connected_segments = connected_wire_segment_indices(&wire_segments, &junctions, at);

    schematic
        .screen
        .items
        .iter()
        .filter_map(|item| match item {
            SchItem::Label(label) if label.kind == LabelKind::Directive => {
                if connected_segments.is_empty() {
                    points_equal(label.at, at).then_some(label)
                } else {
                    connected_segments
                        .iter()
                        .copied()
                        .any(|segment_index| {
                            let segment = wire_segments[segment_index];
                            point_on_wire_segment(label.at, segment[0], segment[1])
                        })
                        .then_some(label)
                }
            }
            _ => None,
        })
        .find_map(&mut directive_netclass)
        .or_else(|| {
            schematic
                .screen
                .items
                .iter()
                .filter_map(|item| match item {
                    SchItem::Shape(shape) if shape.kind == ShapeKind::RuleArea => Some(shape),
                    _ => None,
                })
                .filter(|rule_area| {
                    rule_area_contains_connected_component(
                        rule_area,
                        at,
                        &wire_segments,
                        &connected_segments,
                    )
                })
                .find_map(|rule_area| {
                    schematic
                        .screen
                        .items
                        .iter()
                        .filter_map(|item| match item {
                            SchItem::Label(label)
                                if label.kind == LabelKind::Directive
                                    && point_in_polygon(label.at, &rule_area.points) =>
                            {
                                Some(label)
                            }
                            _ => None,
                        })
                        .find_map(&mut directive_netclass)
                })
        })
}
