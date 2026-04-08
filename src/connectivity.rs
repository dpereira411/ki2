use std::collections::{BTreeMap, BTreeSet};

use crate::core::SchematicProject;
use crate::loader::{
    LoadedProjectSettings, LoadedSheetPath, SymbolPinTextVarKind, collect_wire_segments,
    point_on_wire_segment, points_equal, resolve_point_connectivity_text_var,
    resolved_sheet_text_state, resolved_symbol_text_property_value,
    shown_label_text_without_connectivity,
};
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
    Bus,
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
    pub(crate) labels: Vec<ReducedLabelComponentLabel>,
}

#[derive(Clone, Debug)]
pub(crate) struct ProjectedSymbolPin {
    pub(crate) at: [f64; 2],
    pub(crate) name: Option<String>,
    pub(crate) number: Option<String>,
    pub(crate) electrical_type: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReducedNetNode {
    pub(crate) reference: String,
    pub(crate) pin: String,
    pub(crate) pinfunction: Option<String>,
    pub(crate) pintype: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ReducedNetBasePinKey {
    pub(crate) sheet_instance_path: String,
    pub(crate) symbol_uuid: Option<String>,
    pub(crate) at: PointKey,
    pub(crate) name: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReducedNetSubgraph {
    pub(crate) anchor: [f64; 2],
    pub(crate) class: String,
    pub(crate) has_no_connect: bool,
    pub(crate) points: Vec<PointKey>,
    pub(crate) nodes: Vec<ReducedNetNode>,
    pub(crate) base_pins: Vec<ReducedNetBasePinKey>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReducedNetMapEntry {
    pub(crate) name: String,
    pub(crate) subgraphs: Vec<ReducedNetSubgraph>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReducedProjectNetEntry {
    pub(crate) code: usize,
    pub(crate) name: String,
    pub(crate) class: String,
    pub(crate) has_no_connect: bool,
    pub(crate) nodes: Vec<ReducedNetNode>,
    pub(crate) base_pins: Vec<ReducedNetBasePinKey>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReducedProjectNetIdentity {
    pub(crate) code: usize,
    pub(crate) name: String,
    pub(crate) class: String,
    pub(crate) has_no_connect: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReducedProjectSubgraphEntry {
    pub(crate) subgraph_code: usize,
    pub(crate) code: usize,
    pub(crate) name: String,
    pub(crate) driver_name: String,
    pub(crate) driver_names: Vec<String>,
    pub(crate) class: String,
    pub(crate) has_no_connect: bool,
    pub(crate) sheet_instance_path: String,
    pub(crate) anchor: PointKey,
    pub(crate) points: Vec<PointKey>,
    pub(crate) nodes: Vec<ReducedNetNode>,
    pub(crate) base_pins: Vec<ReducedNetBasePinKey>,
    pub(crate) label_points: Vec<(PointKey, LabelKind)>,
    pub(crate) sheet_pin_points: Vec<PointKey>,
    pub(crate) no_connect_points: Vec<PointKey>,
    pub(crate) bus_items: Vec<ReducedSubgraphWireItem>,
    pub(crate) wire_items: Vec<ReducedSubgraphWireItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ReducedProjectPinIdentityKey {
    sheet_instance_path: String,
    symbol_uuid: Option<String>,
    at: PointKey,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ReducedProjectPointIdentityKey {
    sheet_instance_path: String,
    at: PointKey,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ReducedProjectLabelIdentityKey {
    sheet_instance_path: String,
    at: PointKey,
    kind: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ReducedProjectNoConnectIdentityKey {
    sheet_instance_path: String,
    at: PointKey,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReducedSubgraphWireItem {
    pub(crate) start: PointKey,
    pub(crate) end: PointKey,
    pub(crate) is_bus_entry: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReducedProjectNetGraph {
    subgraphs: Vec<ReducedProjectSubgraphEntry>,
    subgraphs_by_name: BTreeMap<String, Vec<usize>>,
    subgraphs_by_sheet_and_name: BTreeMap<(String, String), usize>,
    pin_subgraph_identities: BTreeMap<ReducedNetBasePinKey, usize>,
    pin_subgraph_identities_by_location: BTreeMap<ReducedProjectPinIdentityKey, usize>,
    point_subgraph_identities: BTreeMap<ReducedProjectPointIdentityKey, usize>,
    label_subgraph_identities: BTreeMap<ReducedProjectLabelIdentityKey, usize>,
    no_connect_subgraph_identities: BTreeMap<ReducedProjectNoConnectIdentityKey, usize>,
}

pub(crate) struct ReducedProjectGraphInputs<'a> {
    pub(crate) schematics: &'a [Schematic],
    pub(crate) sheet_paths: &'a [LoadedSheetPath],
    pub(crate) project: Option<&'a LoadedProjectSettings>,
    pub(crate) current_variant: Option<&'a str>,
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

fn connection_component_for_symbol_pin(
    schematic: &Schematic,
    symbol: &Symbol,
    at: [f64; 2],
) -> Option<ConnectionComponent> {
    collect_connection_components(schematic)
        .into_iter()
        .find(|component| {
            component.members.iter().any(|member| {
                member.kind == ConnectionMemberKind::SymbolPin
                    && member.symbol_uuid == symbol.uuid
                    && points_equal(member.at, at)
            })
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

fn expand_stacked_pin_notation(pin: &str) -> (Vec<String>, bool) {
    let trimmed = pin.trim();

    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return (vec![pin.to_string()], false);
    }

    let inner = &trimmed[1..trimmed.len() - 1];
    let numbers = inner
        .split(',')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    if numbers.is_empty() {
        (vec![pin.to_string()], false)
    } else {
        (numbers, true)
    }
}

fn is_auto_generated_net_name(net_name: &str) -> bool {
    net_name.starts_with("unconnected-(") || net_name.starts_with("Net-(")
}

fn reduced_short_net_name(net_name: &str) -> String {
    net_name
        .rsplit(['.', '/'])
        .next()
        .unwrap_or(net_name)
        .to_string()
}

// Upstream parity: reduced local analogue for the bus-kind discrimination KiCad gets from
// `SCH_CONNECTION::Type()` after `ConfigureFromLabel()`. This is not a 1:1 connection-object
// query because the Rust tree still infers bus-ness from raw label text plus parsed aliases
// instead of cached `SCH_CONNECTION` state, but it lets the shared connectivity owner make one
// consistent bus-vs-net decision for ERC, naming, and export paths.
pub(crate) fn reduced_text_is_bus(schematic: &Schematic, text: &str) -> bool {
    text.contains('[')
        || text.contains(']')
        || text.contains('{')
        || text.contains('}')
        || schematic
            .screen
            .bus_aliases
            .iter()
            .any(|alias| alias.name.eq_ignore_ascii_case(text))
}

fn reduced_bus_members_inner(
    schematic: &Schematic,
    text: &str,
    active_aliases: &mut BTreeSet<String>,
) -> Vec<String> {
    if let Some(alias) = schematic
        .screen
        .bus_aliases
        .iter()
        .find(|alias| alias.name.eq_ignore_ascii_case(text))
    {
        let alias_key = alias.name.to_ascii_uppercase();

        if !active_aliases.insert(alias_key.clone()) {
            return Vec::new();
        }

        let members = alias
            .members
            .iter()
            .flat_map(|member| {
                let expanded = reduced_bus_members_inner(schematic, member, active_aliases);

                if expanded.is_empty() {
                    vec![member.clone()]
                } else {
                    expanded
                }
            })
            .collect::<Vec<_>>();

        active_aliases.remove(&alias_key);
        return members;
    }

    if let Some(inner) = text
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
    {
        return inner
            .split_whitespace()
            .filter(|member| !member.is_empty())
            .map(|member| member.to_string())
            .collect();
    }

    if let Some((prefix, suffix)) = text.split_once('{') {
        if let Some(inner) = suffix.strip_suffix('}') {
            return inner
                .split_whitespace()
                .filter(|member| !member.is_empty())
                .flat_map(|member| {
                    let expanded = reduced_bus_members_inner(schematic, member, active_aliases);

                    if expanded.is_empty() {
                        let name = if prefix.is_empty() {
                            member.to_string()
                        } else {
                            format!("{prefix}.{member}")
                        };
                        vec![name]
                    } else if prefix.is_empty() {
                        expanded
                    } else {
                        expanded
                            .into_iter()
                            .map(|expanded_member| format!("{prefix}.{expanded_member}"))
                            .collect::<Vec<_>>()
                    }
                })
                .collect();
        }
    }

    let Some((prefix, suffix)) = text.split_once('[') else {
        return Vec::new();
    };
    let Some(range) = suffix.strip_suffix(']') else {
        return Vec::new();
    };
    let Some((start, end)) = range.split_once("..") else {
        return Vec::new();
    };
    let Ok(start) = start.parse::<i32>() else {
        return Vec::new();
    };
    let Ok(end) = end.parse::<i32>() else {
        return Vec::new();
    };

    let step = if start <= end { 1 } else { -1 };
    let mut members = Vec::new();
    let mut current = start;

    loop {
        members.push(format!("{prefix}{current}"));

        if current == end {
            break;
        }

        current += step;
    }

    members
}

// Upstream parity: reduced local analogue for the member expansion KiCad exposes through
// `SCH_CONNECTION::Members()` after `ConfigureFromLabel()`. This is not a 1:1 member-object walk
// because the Rust tree still expands from raw text and bus aliases instead of live
// `SCH_CONNECTION` members, but the shared connectivity owner now reuses the same recursive alias,
// vector, and group expansion for ERC, driver naming, and export tie-breaking. Remaining
// divergence is fuller nested/member object ownership beyond reduced string expansion.
pub(crate) fn reduced_bus_members(schematic: &Schematic, text: &str) -> Vec<String> {
    reduced_bus_members_inner(schematic, text, &mut BTreeSet::new())
}

fn reduced_bus_subset_cmp(schematic: &Schematic, lhs: &str, rhs: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    if !reduced_text_is_bus(schematic, lhs) || !reduced_text_is_bus(schematic, rhs) {
        return Ordering::Equal;
    }

    let lhs_members = reduced_bus_members(schematic, lhs);
    let rhs_members = reduced_bus_members(schematic, rhs);

    if lhs_members.is_empty() || rhs_members.is_empty() {
        return Ordering::Equal;
    }

    let lhs_is_subset = lhs_members
        .iter()
        .all(|member| rhs_members.contains(member));
    let rhs_is_subset = rhs_members
        .iter()
        .all(|member| lhs_members.contains(member));

    match (lhs_is_subset, rhs_is_subset) {
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        _ => Ordering::Equal,
    }
}

fn reduced_str_num_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let mut a_chars = a.chars().peekable();
    let mut b_chars = b.chars().peekable();

    loop {
        match (a_chars.peek(), b_chars.peek()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(a_ch), Some(b_ch)) if a_ch.is_ascii_digit() && b_ch.is_ascii_digit() => {
                let mut a_num = String::new();
                let mut b_num = String::new();

                while let Some(ch) = a_chars.peek() {
                    if !ch.is_ascii_digit() {
                        break;
                    }

                    a_num.push(*ch);
                    a_chars.next();
                }

                while let Some(ch) = b_chars.peek() {
                    if !ch.is_ascii_digit() {
                        break;
                    }

                    b_num.push(*ch);
                    b_chars.next();
                }

                let a_trimmed = a_num.trim_start_matches('0');
                let b_trimmed = b_num.trim_start_matches('0');
                let a_cmp = if a_trimmed.is_empty() { "0" } else { a_trimmed };
                let b_cmp = if b_trimmed.is_empty() { "0" } else { b_trimmed };
                let ordering = a_cmp.len().cmp(&b_cmp.len()).then_with(|| a_cmp.cmp(b_cmp));

                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            (Some(a_ch), Some(b_ch)) => {
                let ordering = a_ch.cmp(b_ch);
                a_chars.next();
                b_chars.next();

                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
        }
    }
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
// repeating isolated geometry scans in each caller. Bus segments now stay distinct from wire
// segments in this shared carrier instead of collapsing into one local `Wire` kind, which keeps
// wire-only ERC branches closer to KiCad's bus-vs-wire item ownership.
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
            SchItem::Bus(line) => {
                for point in &line.points {
                    push_connection_member(
                        &mut snapshot,
                        ConnectionMember {
                            kind: ConnectionMemberKind::Bus,
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

// Upstream parity: reduced local analogue for the `ConnectionGraph()->GetNetMap()` ownership that
// KiCad's XML/KiCad exporters consume. This is not a 1:1 `CONNECTION_GRAPH` port because the Rust
// tree still lacks real `CONNECTION_SUBGRAPH` objects, graph-owned netcodes, and cached driver
// item identity, but it preserves one shared reduced net-map owner instead of rebuilding net
// grouping inside each exporter. It now also keeps node-less driver subgraphs instead of dropping
// them before the shared graph owner sees them, matching KiCad's graph-owned subgraph coverage
// more closely. Remaining divergence is the missing full subgraph object model and graph-owned
// netcode allocation beyond these grouped reduced subgraphs.
pub(crate) fn collect_reduced_net_map<FName, FClass, FAllow, FReference>(
    schematic: &Schematic,
    sheet_instance_path: &str,
    mut resolve_net_name: FName,
    mut resolve_net_class: FClass,
    mut allow_symbol: FAllow,
    mut symbol_reference: FReference,
) -> Vec<ReducedNetMapEntry>
where
    FName: FnMut([f64; 2]) -> Option<String>,
    FClass: FnMut([f64; 2]) -> Option<String>,
    FAllow: FnMut(&Symbol) -> bool,
    FReference: FnMut(&Symbol) -> Option<String>,
{
    let mut net_map = BTreeMap::<String, Vec<ReducedNetSubgraph>>::new();

    for component in collect_connection_components(schematic) {
        let Some(net_name) = resolve_net_name(component.anchor).filter(|name| !name.is_empty())
        else {
            continue;
        };

        let mut nodes = BTreeMap::<(String, String), ReducedNetNode>::new();
        let mut base_pins = Vec::new();

        for item in &schematic.screen.items {
            let SchItem::Symbol(symbol) = item else {
                continue;
            };

            if !symbol.in_netlist || !allow_symbol(symbol) {
                continue;
            }

            let Some(reference) = symbol_reference(symbol) else {
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
                let base_pin_key = ReducedNetBasePinKey {
                    sheet_instance_path: sheet_instance_path.to_string(),
                    symbol_uuid: symbol.uuid.clone(),
                    at: point_key(pin.at),
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

                    nodes
                        .entry((reference.clone(), pin_number.clone()))
                        .or_insert_with(|| ReducedNetNode {
                            reference: reference.clone(),
                            pin: pin_number,
                            pinfunction,
                            pintype: pin.electrical_type.clone().unwrap_or_default(),
                        });
                }

                base_pins.push(base_pin_key);
            }
        }

        net_map
            .entry(net_name)
            .or_default()
            .push(ReducedNetSubgraph {
                anchor: component.anchor,
                class: {
                    let mut seen_points = BTreeSet::new();
                    component
                        .members
                        .iter()
                        .filter(|member| seen_points.insert(point_key(member.at)))
                        .find_map(|member| resolve_net_class(member.at))
                        .unwrap_or_default()
                },
                has_no_connect: component
                    .members
                    .iter()
                    .any(|member| member.kind == ConnectionMemberKind::NoConnectMarker),
                points: component
                    .members
                    .iter()
                    .map(|member| point_key(member.at))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                nodes: nodes.into_values().collect(),
                base_pins,
            });
    }

    net_map
        .into_iter()
        .map(|(name, subgraphs)| ReducedNetMapEntry { name, subgraphs })
        .collect()
}

// Upstream parity: reduced local analogue for the project-wide `ConnectionGraph` owner behind
// `GetNetMap()` and `GetSubgraphForItem()`. This is not a 1:1 graph owner because the Rust tree
// still lacks real `CONNECTION_SUBGRAPH` objects, driver objects, and live item pointers, but it
// now owns one shared reduced project net map plus item lookup indexes instead of making ERC and
// export rebuild those facts independently. Remaining divergence is the missing full subgraph
// object model and graph-owned resolved-name caches beyond this reduced project graph; candidate
// ownership is now widened to `(sheet instance path, reference, pin)` so reused-sheet symbol-pin
// identity is not collapsed before pin net/class ownership is assigned, item-to-net facts now
// derive through the shared subgraph owner instead of duplicate item-to-whole-net side maps,
// whole-net views are derived from the shared subgraph owner instead of a second stored flattened
// carrier, and reduced label/sheet-pin/no-connect membership now rides on the shared subgraph
// owner for graph-side ERC rules instead of per-sheet component rescans. The outward reduced node
// carrier is still narrower than a real `CONNECTION_SUBGRAPH` item owner.
pub(crate) fn collect_reduced_project_net_graph_from_inputs(
    inputs: ReducedProjectGraphInputs<'_>,
    for_board: bool,
) -> ReducedProjectNetGraph {
    struct PendingProjectSubgraph {
        name: String,
        driver_name: String,
        driver_names: Vec<String>,
        class: String,
        has_no_connect: bool,
        sheet_instance_path: String,
        anchor: PointKey,
        points: Vec<PointKey>,
        nodes: Vec<ReducedNetNode>,
        base_pins: Vec<ReducedNetBasePinKey>,
        label_points: Vec<(PointKey, LabelKind)>,
        sheet_pin_points: Vec<PointKey>,
        no_connect_points: Vec<PointKey>,
        bus_items: Vec<ReducedSubgraphWireItem>,
        wire_items: Vec<ReducedSubgraphWireItem>,
    }

    let mut all_base_pins_by_net = BTreeMap::<String, Vec<ReducedNetBasePinKey>>::new();
    let mut pending_subgraphs = Vec::<PendingProjectSubgraph>::new();
    let mut candidates = BTreeMap::<
        (String, String, String),
        (String, String, bool, ReducedNetNode, ReducedNetBasePinKey),
    >::new();
    let mut nets = BTreeMap::<
        String,
        (
            String,
            bool,
            BTreeMap<(String, String), ReducedNetNode>,
            Vec<ReducedNetBasePinKey>,
        ),
    >::new();

    for sheet_path in inputs.sheet_paths {
        if for_board
            && !resolved_sheet_text_state(
                inputs.schematics,
                inputs.sheet_paths,
                sheet_path,
                inputs.current_variant,
            )
            .map(|state| state.on_board)
            .unwrap_or(true)
        {
            continue;
        }

        let Some(schematic) = inputs
            .schematics
            .iter()
            .find(|schematic| schematic.path == sheet_path.schematic_path)
        else {
            continue;
        };

        for entry in collect_reduced_net_map(
            schematic,
            &sheet_path.instance_path,
            |at| {
                resolve_point_connectivity_text_var(
                    inputs.schematics,
                    inputs.sheet_paths,
                    sheet_path,
                    inputs.project,
                    inputs.current_variant,
                    at,
                    SymbolPinTextVarKind::NetName,
                )
            },
            |at| {
                resolve_point_connectivity_text_var(
                    inputs.schematics,
                    inputs.sheet_paths,
                    sheet_path,
                    inputs.project,
                    inputs.current_variant,
                    at,
                    SymbolPinTextVarKind::NetClass,
                )
            },
            |symbol| !for_board || symbol.on_board,
            |symbol| {
                resolved_symbol_text_property_value(
                    inputs.schematics,
                    sheet_path,
                    inputs.project,
                    inputs.current_variant,
                    symbol,
                    "Reference",
                )
            },
        ) {
            for subgraph in entry.subgraphs {
                let ReducedNetSubgraph {
                    class,
                    has_no_connect,
                    points,
                    nodes,
                    base_pins,
                    ..
                } = subgraph;

                let connected_component = connection_component_at(
                    schematic,
                    [f64::from_bits(points[0].0), f64::from_bits(points[0].1)],
                )
                .expect("project reduced subgraph must keep its source component");

                let driver_name = resolve_reduced_driver_name_on_component(
                    schematic,
                    &connected_component,
                    |label| {
                        shown_label_text_without_connectivity(
                            inputs.schematics,
                            inputs.sheet_paths,
                            sheet_path,
                            inputs.project,
                            inputs.current_variant,
                            label,
                        )
                    },
                )
                .unwrap_or_else(|| reduced_short_net_name(&entry.name));
                let driver_names = {
                    let mut names =
                        collect_reduced_strong_drivers(schematic, &connected_component, |label| {
                            shown_label_text_without_connectivity(
                                inputs.schematics,
                                inputs.sheet_paths,
                                sheet_path,
                                inputs.project,
                                inputs.current_variant,
                                label,
                            )
                        })
                        .into_iter()
                        .map(|driver| driver.name)
                        .collect::<Vec<_>>();
                    names.dedup();
                    names
                };
                let (label_points, sheet_pin_points, no_connect_points, bus_items, wire_items) =
                    collect_reduced_subgraph_local_membership(schematic, &connected_component);

                pending_subgraphs.push(PendingProjectSubgraph {
                    name: entry.name.clone(),
                    driver_name,
                    driver_names,
                    class: class.clone(),
                    has_no_connect,
                    sheet_instance_path: sheet_path.instance_path.clone(),
                    anchor: point_key(connected_component.anchor),
                    points: points.clone(),
                    nodes: nodes.clone(),
                    base_pins: base_pins.clone(),
                    label_points,
                    sheet_pin_points,
                    no_connect_points,
                    bus_items,
                    wire_items,
                });

                nets.entry(entry.name.clone()).or_insert_with(|| {
                    (
                        class.clone(),
                        has_no_connect,
                        BTreeMap::new(),
                        all_base_pins_by_net
                            .get(&entry.name)
                            .cloned()
                            .unwrap_or_default(),
                    )
                });

                all_base_pins_by_net
                    .entry(entry.name.clone())
                    .or_default()
                    .extend(base_pins.iter().cloned());

                for node in nodes {
                    let key = (
                        sheet_path.instance_path.clone(),
                        node.reference.clone(),
                        node.pin.clone(),
                    );
                    let base_pin_key = base_pins
                        .iter()
                        .find(|base_pin| {
                            base_pin.symbol_uuid.is_some()
                                && node
                                    .pinfunction
                                    .as_ref()
                                    .map(|pinfunction| {
                                        base_pin
                                            .name
                                            .as_ref()
                                            .is_some_and(|name| pinfunction.starts_with(name))
                                    })
                                    .unwrap_or(base_pin.name.is_none())
                        })
                        .cloned()
                        .or_else(|| base_pins.first().cloned())
                        .expect("shared reduced net map must keep base pin identity");

                    let candidate = (
                        entry.name.clone(),
                        class.clone(),
                        has_no_connect,
                        node,
                        base_pin_key,
                    );

                    match candidates.get(&key) {
                        Some(existing)
                            if is_auto_generated_net_name(&existing.0)
                                && !is_auto_generated_net_name(&candidate.0) =>
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

        for connected_component in collect_connection_components(schematic) {
            let net_name = resolve_point_connectivity_text_var(
                inputs.schematics,
                inputs.sheet_paths,
                sheet_path,
                inputs.project,
                inputs.current_variant,
                connected_component.anchor,
                SymbolPinTextVarKind::NetName,
            )
            .unwrap_or_default();

            if !net_name.is_empty() {
                continue;
            }

            let keeps_local_subgraph = connected_component.members.iter().any(|member| {
                matches!(
                    member.kind,
                    ConnectionMemberKind::Wire
                        | ConnectionMemberKind::BusEntry
                        | ConnectionMemberKind::NoConnectMarker
                )
            });

            if !keeps_local_subgraph {
                continue;
            }

            let (label_points, sheet_pin_points, no_connect_points, bus_items, wire_items) =
                collect_reduced_subgraph_local_membership(schematic, &connected_component);

            pending_subgraphs.push(PendingProjectSubgraph {
                name: String::new(),
                driver_name: String::new(),
                driver_names: Vec::new(),
                class: String::new(),
                has_no_connect: true,
                sheet_instance_path: sheet_path.instance_path.clone(),
                anchor: point_key(connected_component.anchor),
                points: connected_component
                    .members
                    .iter()
                    .map(|member| point_key(member.at))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_points,
                sheet_pin_points,
                no_connect_points,
                bus_items,
                wire_items,
            });
        }
    }

    for (
        (_sheet_instance_path, reference, pin_number),
        (net_name, net_class, has_no_connect, node, base_pin_key),
    ) in candidates
    {
        let net_nodes = nets.entry(net_name.clone()).or_insert_with(|| {
            (
                net_class.clone(),
                false,
                BTreeMap::new(),
                all_base_pins_by_net.remove(&net_name).unwrap_or_default(),
            )
        });
        if net_nodes.0.is_empty() && !net_class.is_empty() {
            net_nodes.0 = net_class.clone();
        }
        net_nodes.1 |= has_no_connect;
        net_nodes.2.insert((reference, pin_number), node);
        if !net_nodes.3.contains(&base_pin_key) {
            net_nodes.3.push(base_pin_key);
        }
    }

    let mut nets = nets.into_iter().collect::<Vec<_>>();
    nets.sort_by(|(a_name, _), (b_name, _)| reduced_str_num_cmp(a_name, b_name));

    let mut reduced_subgraphs = Vec::new();
    let mut subgraphs_by_name = BTreeMap::<String, Vec<usize>>::new();
    let mut subgraphs_by_sheet_and_name = BTreeMap::<(String, String), usize>::new();
    let mut pin_subgraph_identities = BTreeMap::new();
    let mut pin_subgraph_identities_by_location = BTreeMap::new();
    let mut point_subgraph_identities = BTreeMap::new();
    let mut label_subgraph_identities = BTreeMap::new();
    let mut no_connect_subgraph_identities = BTreeMap::new();
    let mut net_identities_by_name = BTreeMap::<String, ReducedProjectNetIdentity>::new();

    for (index, (name, (class, has_no_connect, _nodes, _base_pins))) in nets.into_iter().enumerate()
    {
        net_identities_by_name.insert(
            name.clone(),
            ReducedProjectNetIdentity {
                code: index + 1,
                name: name.clone(),
                class: class.clone(),
                has_no_connect,
            },
        );
    }

    for (subgraph_index, pending) in pending_subgraphs.into_iter().enumerate() {
        let net_identity = net_identities_by_name.get(&pending.name);
        let net_identity = ReducedProjectSubgraphEntry {
            subgraph_code: subgraph_index + 1,
            code: net_identity.map(|net| net.code).unwrap_or_default(),
            name: net_identity
                .map(|net| net.name.clone())
                .unwrap_or_else(|| pending.name.clone()),
            driver_name: pending.driver_name.clone(),
            driver_names: pending.driver_names.clone(),
            class: if pending.class.is_empty() {
                net_identity
                    .map(|net| net.class.clone())
                    .unwrap_or_default()
            } else {
                pending.class.clone()
            },
            has_no_connect: pending.has_no_connect,
            sheet_instance_path: pending.sheet_instance_path.clone(),
            anchor: pending.anchor,
            points: pending.points.clone(),
            nodes: pending.nodes.clone(),
            base_pins: pending.base_pins.clone(),
            label_points: pending.label_points.clone(),
            sheet_pin_points: pending.sheet_pin_points.clone(),
            no_connect_points: pending.no_connect_points.clone(),
            bus_items: pending.bus_items.clone(),
            wire_items: pending.wire_items.clone(),
        };

        let index = reduced_subgraphs.len();
        subgraphs_by_name
            .entry(net_identity.name.clone())
            .or_default()
            .push(index);
        subgraphs_by_sheet_and_name.insert(
            (
                net_identity.sheet_instance_path.clone(),
                net_identity.name.clone(),
            ),
            index,
        );
        for base_pin in &net_identity.base_pins {
            pin_subgraph_identities.insert(base_pin.clone(), index);
            pin_subgraph_identities_by_location.insert(
                ReducedProjectPinIdentityKey {
                    sheet_instance_path: base_pin.sheet_instance_path.clone(),
                    symbol_uuid: base_pin.symbol_uuid.clone(),
                    at: base_pin.at,
                },
                index,
            );
        }
        for point in &net_identity.points {
            point_subgraph_identities.insert(
                ReducedProjectPointIdentityKey {
                    sheet_instance_path: net_identity.sheet_instance_path.clone(),
                    at: *point,
                },
                index,
            );
        }
        for (point, kind) in &net_identity.label_points {
            label_subgraph_identities.insert(
                ReducedProjectLabelIdentityKey {
                    sheet_instance_path: net_identity.sheet_instance_path.clone(),
                    at: *point,
                    kind: reduced_label_kind_sort_key(*kind),
                },
                index,
            );
        }
        for point in &net_identity.no_connect_points {
            no_connect_subgraph_identities.insert(
                ReducedProjectNoConnectIdentityKey {
                    sheet_instance_path: net_identity.sheet_instance_path.clone(),
                    at: *point,
                },
                index,
            );
        }
        reduced_subgraphs.push(net_identity);
    }

    ReducedProjectNetGraph {
        subgraphs: reduced_subgraphs,
        subgraphs_by_name,
        subgraphs_by_sheet_and_name,
        pin_subgraph_identities,
        pin_subgraph_identities_by_location,
        point_subgraph_identities,
        label_subgraph_identities,
        no_connect_subgraph_identities,
    }
}

// Upstream parity: reduced local analogue for the project-wide `ConnectionGraph()->GetNetMap()`
// owner boundary. This wrapper exists because `SchematicProject` is currently the main cached
// graph owner, but the underlying reduced graph construction now accepts raw loaded inputs so
// loader-side hierarchy passes can reuse the same owner path instead of rebuilding connectivity via
// per-label current-sheet scans. Remaining divergence is the still-missing full subgraph object
// model behind both callers.
pub(crate) fn collect_reduced_project_net_graph(
    schematics: &[Schematic],
    sheet_paths: &[LoadedSheetPath],
    project: Option<&LoadedProjectSettings>,
    current_variant: Option<&str>,
    for_board: bool,
) -> ReducedProjectNetGraph {
    collect_reduced_project_net_graph_from_inputs(
        ReducedProjectGraphInputs {
            schematics,
            sheet_paths,
            project,
            current_variant,
        },
        for_board,
    )
}

#[allow(dead_code)]
// Upstream parity: reduced local analogue for the project-wide `ConnectionGraph()->GetNetMap()`
// consumer path used by KiCad's net exporters. This is not a 1:1 graph owner because the Rust
// tree still lacks real `CONNECTION_SUBGRAPH` objects and graph-owned item identity, but it now
// derives whole-net entries from the shared reduced subgraph owner instead of storing a second
// flattened net vector beside it. Remaining divergence is the missing full subgraph object model
// and graph-owned resolved-name caches beyond this reduced project net map.
pub(crate) fn collect_reduced_project_net_map(
    project: &SchematicProject,
    for_board: bool,
) -> Vec<ReducedProjectNetEntry> {
    let mut grouped = BTreeMap::<
        (usize, String),
        (
            String,
            bool,
            BTreeMap<(String, String), ReducedNetNode>,
            Vec<ReducedNetBasePinKey>,
        ),
    >::new();
    let mut candidates = BTreeMap::<
        (String, String),
        (
            usize,
            String,
            String,
            bool,
            ReducedNetNode,
            Option<ReducedNetBasePinKey>,
        ),
    >::new();

    for subgraph in project.reduced_project_net_graph(for_board).subgraphs {
        let entry = grouped
            .entry((subgraph.code, subgraph.name.clone()))
            .or_insert_with(|| {
                (
                    subgraph.class.clone(),
                    false,
                    BTreeMap::new(),
                    Vec::<ReducedNetBasePinKey>::new(),
                )
            });

        if entry.0.is_empty() && !subgraph.class.is_empty() {
            entry.0 = subgraph.class.clone();
        }

        entry.1 |= subgraph.has_no_connect;

        for node in subgraph.nodes {
            let base_pin_key = subgraph
                .base_pins
                .iter()
                .find(|base_pin| {
                    base_pin.symbol_uuid.is_some()
                        && node
                            .pinfunction
                            .as_ref()
                            .map(|pinfunction| {
                                base_pin
                                    .name
                                    .as_ref()
                                    .is_some_and(|name| pinfunction.starts_with(name))
                            })
                            .unwrap_or(base_pin.name.is_none())
                })
                .cloned()
                .or_else(|| subgraph.base_pins.first().cloned());
            let key = (node.reference.clone(), node.pin.clone());
            let candidate = (
                subgraph.code,
                subgraph.name.clone(),
                subgraph.class.clone(),
                subgraph.has_no_connect,
                node,
                base_pin_key,
            );

            match candidates.get(&key) {
                Some(existing)
                    if is_auto_generated_net_name(&existing.1)
                        && !is_auto_generated_net_name(&candidate.1) =>
                {
                    candidates.insert(key, candidate);
                }
                None => {
                    candidates.insert(key, candidate);
                }
                _ => {}
            }
        }

        for base_pin in subgraph.base_pins {
            if !entry.3.contains(&base_pin) {
                entry.3.push(base_pin);
            }
        }
    }

    for ((_reference, _pin), (code, name, class, has_no_connect, node, base_pin_key)) in candidates
    {
        let entry = grouped
            .entry((code, name))
            .or_insert_with(|| (class.clone(), has_no_connect, BTreeMap::new(), Vec::new()));

        if entry.0.is_empty() && !class.is_empty() {
            entry.0 = class;
        }

        entry.1 |= has_no_connect;
        entry
            .2
            .insert((node.reference.clone(), node.pin.clone()), node);
        if let Some(base_pin_key) = base_pin_key {
            if !entry.3.contains(&base_pin_key) {
                entry.3.push(base_pin_key);
            }
        }
    }

    grouped
        .into_iter()
        .filter_map(
            |((_code, name), (class, has_no_connect, nodes, base_pins))| {
                let nodes = nodes.into_values().collect::<Vec<_>>();
                (!nodes.is_empty()).then_some((name, class, has_no_connect, nodes, base_pins))
            },
        )
        .enumerate()
        .map(
            |(index, (name, class, has_no_connect, nodes, base_pins))| ReducedProjectNetEntry {
                code: index + 1,
                name,
                class,
                has_no_connect,
                nodes,
                base_pins,
            },
        )
        .collect()
}

#[allow(dead_code)]
// Upstream parity: reduced local analogue for iterating `ConnectionGraph()->GetNetMap()` subgraph
// members on the project graph path. This is not a 1:1 KiCad container because the Rust tree
// still stores reduced cloned subgraph snapshots instead of live `CONNECTION_SUBGRAPH*` objects,
// but it preserves the graph-owned subgraph boundary for exporter/ERC callers instead of forcing
// every consumer through pre-flattened whole-net entries only. Remaining divergence is the still-
// missing live subgraph object model and cached driver connections.
pub(crate) fn collect_reduced_project_subgraphs(
    project: &SchematicProject,
    for_board: bool,
) -> Vec<ReducedProjectSubgraphEntry> {
    project.reduced_project_net_graph(for_board).subgraphs
}

#[cfg_attr(not(test), allow(dead_code))]
// Upstream parity: reduced local analogue for `CONNECTION_GRAPH::FindSubgraphByName()`. This is
// not a 1:1 graph lookup because the Rust tree still lacks live `CONNECTION_SUBGRAPH` ownership,
// but it now preserves KiCad's `(sheet instance path, resolved net name)` lookup boundary instead
// of the old repo-local short-driver key. Remaining divergence is the fuller subgraph object model
// and exact driver-connection caching.
pub(crate) fn find_reduced_project_subgraph_by_name<'a>(
    graph: &'a ReducedProjectNetGraph,
    net_name: &str,
    sheet_path: &LoadedSheetPath,
) -> Option<&'a ReducedProjectSubgraphEntry> {
    graph
        .subgraphs_by_sheet_and_name
        .get(&(sheet_path.instance_path.clone(), net_name.to_string()))
        .and_then(|index| graph.subgraphs.get(*index))
}

#[cfg_attr(not(test), allow(dead_code))]
// Upstream parity: reduced local analogue for `CONNECTION_GRAPH::FindFirstSubgraphByName()`. This
// is not a 1:1 global lookup because the Rust tree still stores reduced subgraphs in the shared
// project graph instead of live `CONNECTION_SUBGRAPH*` objects, but it restores the owner
// boundary where graph/export/ERC callers can ask for the first resolved subgraph by full net name
// instead of flattening to whole-net facts only. Remaining divergence is the fuller subgraph
// object model and graph-owned resolved-name caches.
pub(crate) fn find_first_reduced_project_subgraph_by_name<'a>(
    graph: &'a ReducedProjectNetGraph,
    net_name: &str,
) -> Option<&'a ReducedProjectSubgraphEntry> {
    graph
        .subgraphs_by_name
        .get(net_name)
        .and_then(|indexes| indexes.first())
        .and_then(|index| graph.subgraphs.get(*index))
}

#[cfg_attr(not(test), allow(dead_code))]
// Upstream parity: reduced local analogue for `CONNECTION_GRAPH::GetAllSubgraphs()`. This is not
// a 1:1 shared cache because the Rust tree still stores cloned reduced subgraphs instead of live
// `CONNECTION_SUBGRAPH*` objects, but it preserves the graph-owned "all resolved subgraphs for one
// name" lookup boundary so ERC/export callers do not rebuild per-net neighbor lists locally.
// Remaining divergence is the fuller subgraph object model and graph-owned cache lifetime.
pub(crate) fn collect_reduced_project_subgraphs_by_name<'a>(
    graph: &'a ReducedProjectNetGraph,
    net_name: &str,
) -> Vec<&'a ReducedProjectSubgraphEntry> {
    graph
        .subgraphs_by_name
        .get(net_name)
        .into_iter()
        .flat_map(|indexes| indexes.iter())
        .filter_map(|index| graph.subgraphs.get(*index))
        .collect()
}

#[cfg_attr(not(test), allow(dead_code))]
// Upstream parity: reduced local analogue for the connection-point half of
// `CONNECTION_GRAPH::GetSubgraphForItem()` on the project graph path. This is not a 1:1 KiCad
// item map because the Rust tree still keys lookups by `(sheet instance path, point)` instead of
// live item pointers, but it preserves shared item-to-subgraph identity instead of flattening
// directly to whole-net identity. Remaining divergence is fuller item ownership for labels, wires,
// and markers plus the still-missing live `CONNECTION_SUBGRAPH` object.
pub(crate) fn resolve_reduced_project_subgraph_at<'a>(
    graph: &'a ReducedProjectNetGraph,
    sheet_path: &LoadedSheetPath,
    at: [f64; 2],
) -> Option<&'a ReducedProjectSubgraphEntry> {
    graph
        .point_subgraph_identities
        .get(&reduced_project_point_identity_key(sheet_path, at))
        .and_then(|index| graph.subgraphs.get(*index))
}

#[cfg_attr(not(test), allow(dead_code))]
// Upstream parity: reduced local analogue for the label-item half of
// `CONNECTION_GRAPH::GetSubgraphForItem()` on the project graph path. This is not a 1:1 KiCad
// item map because the Rust tree still keys labels by `(sheet instance path, point, kind)`
// instead of live `SCH_LABEL_BASE*`, but it preserves shared label-to-subgraph identity instead
// of making ERC recover label membership from per-subgraph point lists. Remaining divergence is
// fuller item identity for overlapping same-kind labels plus the still-missing live
// `CONNECTION_SUBGRAPH` object.
pub(crate) fn resolve_reduced_project_subgraph_for_label<'a>(
    graph: &'a ReducedProjectNetGraph,
    sheet_path: &LoadedSheetPath,
    label: &Label,
) -> Option<&'a ReducedProjectSubgraphEntry> {
    graph
        .label_subgraph_identities
        .get(&reduced_project_label_identity_key(sheet_path, label))
        .and_then(|index| graph.subgraphs.get(*index))
}

#[cfg_attr(not(test), allow(dead_code))]
// Upstream parity: reduced local analogue for the no-connect marker half of
// `CONNECTION_GRAPH::GetSubgraphForItem()` on the project graph path. This is not a 1:1 KiCad
// item map because the Rust tree still keys markers by `(sheet instance path, point)` instead of
// live `SCH_NO_CONNECT*`, but it preserves shared marker-to-subgraph identity instead of making
// ERC infer marker ownership from subgraph point sets. Remaining divergence is fuller marker item
// identity for overlapping markers plus the still-missing live `CONNECTION_SUBGRAPH` object.
pub(crate) fn resolve_reduced_project_subgraph_for_no_connect<'a>(
    graph: &'a ReducedProjectNetGraph,
    sheet_path: &LoadedSheetPath,
    at: [f64; 2],
) -> Option<&'a ReducedProjectSubgraphEntry> {
    graph
        .no_connect_subgraph_identities
        .get(&reduced_project_no_connect_identity_key(sheet_path, at))
        .and_then(|index| graph.subgraphs.get(*index))
}

// Upstream parity: reduced local analogue for the connection-point `Name(true)` path via
// `CONNECTION_GRAPH::GetSubgraphForItem()`. This is not a 1:1 KiCad connection object because the
// Rust tree still lacks live `SCH_CONNECTION` objects, but it preserves shared local driver-name
// ownership instead of deriving short names by trimming the full resolved net name after the fact.
// Remaining divergence is the still-missing live connection object and fuller subgraph ownership.
pub(crate) fn resolve_reduced_project_driver_name_at(
    graph: &ReducedProjectNetGraph,
    sheet_path: &LoadedSheetPath,
    at: [f64; 2],
) -> Option<String> {
    resolve_reduced_project_subgraph_at(graph, sheet_path, at)
        .map(|subgraph| subgraph.driver_name.clone())
}

fn reduced_project_pin_identity_key(
    sheet_path: &LoadedSheetPath,
    symbol: &Symbol,
    at: [f64; 2],
) -> ReducedProjectPinIdentityKey {
    ReducedProjectPinIdentityKey {
        sheet_instance_path: sheet_path.instance_path.clone(),
        symbol_uuid: symbol.uuid.clone(),
        at: point_key(at),
    }
}

fn reduced_project_point_identity_key(
    sheet_path: &LoadedSheetPath,
    at: [f64; 2],
) -> ReducedProjectPointIdentityKey {
    ReducedProjectPointIdentityKey {
        sheet_instance_path: sheet_path.instance_path.clone(),
        at: point_key(at),
    }
}

fn reduced_project_label_identity_key(
    sheet_path: &LoadedSheetPath,
    label: &Label,
) -> ReducedProjectLabelIdentityKey {
    ReducedProjectLabelIdentityKey {
        sheet_instance_path: sheet_path.instance_path.clone(),
        at: point_key(label.at),
        kind: reduced_label_kind_sort_key(label.kind),
    }
}

fn reduced_project_no_connect_identity_key(
    sheet_path: &LoadedSheetPath,
    at: [f64; 2],
) -> ReducedProjectNoConnectIdentityKey {
    ReducedProjectNoConnectIdentityKey {
        sheet_instance_path: sheet_path.instance_path.clone(),
        at: point_key(at),
    }
}

// Upstream parity: local helper for deterministic reduced subgraph snapshot ordering. KiCad keeps
// live item sets on `CONNECTION_SUBGRAPH`, so it does not need this tuple-sort helper. The Rust
// reduced graph still stores cloned label-point snapshots, and this helper keeps that carrier
// stable without broadening `LabelKind` itself just for ordering.
fn reduced_label_kind_sort_key(kind: LabelKind) -> u8 {
    match kind {
        LabelKind::Local => 0,
        LabelKind::Global => 1,
        LabelKind::Hierarchical => 2,
        LabelKind::Directive => 3,
    }
}

// Upstream parity: local helper for the reduced per-subgraph item membership KiCad keeps directly
// on `CONNECTION_SUBGRAPH`. This is not a 1:1 upstream routine because the Rust tree still starts
// from cloned connected-component members plus a schematic item scan, but it exists so project-
// graph ERC paths can share one reduced local label/sheet-pin/no-connect/wire-item membership
// snapshot instead of rebuilding those facts differently per rule.
fn collect_reduced_subgraph_local_membership(
    schematic: &Schematic,
    connected_component: &ConnectionComponent,
) -> (
    Vec<(PointKey, LabelKind)>,
    Vec<PointKey>,
    Vec<PointKey>,
    Vec<ReducedSubgraphWireItem>,
    Vec<ReducedSubgraphWireItem>,
) {
    let mut label_points = connected_component
        .members
        .iter()
        .filter_map(|member| {
            if member.kind != ConnectionMemberKind::Label {
                return None;
            }

            schematic.screen.items.iter().find_map(|item| match item {
                SchItem::Label(label) if points_equal(label.at, member.at) => {
                    Some((point_key(label.at), label.kind))
                }
                _ => None,
            })
        })
        .collect::<Vec<_>>();
    label_points
        .sort_by_key(|(point, kind)| (point.0, point.1, reduced_label_kind_sort_key(*kind)));
    label_points.dedup();

    let mut sheet_pin_points = connected_component
        .members
        .iter()
        .filter(|member| member.kind == ConnectionMemberKind::SheetPin)
        .map(|member| point_key(member.at))
        .collect::<Vec<_>>();
    sheet_pin_points.sort();
    sheet_pin_points.dedup();

    let mut no_connect_points = connected_component
        .members
        .iter()
        .filter(|member| member.kind == ConnectionMemberKind::NoConnectMarker)
        .map(|member| point_key(member.at))
        .collect::<Vec<_>>();
    no_connect_points.sort();
    no_connect_points.dedup();

    let component_points = connected_component
        .members
        .iter()
        .map(|member| point_key(member.at))
        .collect::<BTreeSet<_>>();
    let mut bus_items = schematic
        .screen
        .items
        .iter()
        .filter_map(|item| match item {
            SchItem::Bus(line) => {
                let start = line.points.first().copied()?;
                let end = line.points.last().copied()?;
                (component_points.contains(&point_key(start))
                    || component_points.contains(&point_key(end)))
                .then_some(ReducedSubgraphWireItem {
                    start: point_key(start),
                    end: point_key(end),
                    is_bus_entry: false,
                })
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    bus_items.sort_by_key(|item| {
        (
            item.start.0,
            item.start.1,
            item.end.0,
            item.end.1,
            item.is_bus_entry,
        )
    });
    bus_items.dedup();

    let mut wire_items = schematic
        .screen
        .items
        .iter()
        .filter_map(|item| match item {
            SchItem::Wire(line) => {
                let start = line.points.first().copied()?;
                let end = line.points.last().copied()?;
                (component_points.contains(&point_key(start))
                    || component_points.contains(&point_key(end)))
                .then_some(ReducedSubgraphWireItem {
                    start: point_key(start),
                    end: point_key(end),
                    is_bus_entry: false,
                })
            }
            SchItem::BusEntry(entry) => {
                let start = entry.at;
                let end = [entry.at[0] + entry.size[0], entry.at[1] + entry.size[1]];
                (component_points.contains(&point_key(start))
                    || component_points.contains(&point_key(end)))
                .then_some(ReducedSubgraphWireItem {
                    start: point_key(start),
                    end: point_key(end),
                    is_bus_entry: true,
                })
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    wire_items.sort_by_key(|item| {
        (
            item.start.0,
            item.start.1,
            item.end.0,
            item.end.1,
            item.is_bus_entry,
        )
    });
    wire_items.dedup();

    (
        label_points,
        sheet_pin_points,
        no_connect_points,
        bus_items,
        wire_items,
    )
}

fn reduced_project_base_pin_key(
    sheet_path: &LoadedSheetPath,
    symbol: &Symbol,
    at: [f64; 2],
    pin_name: &str,
) -> ReducedNetBasePinKey {
    ReducedNetBasePinKey {
        sheet_instance_path: sheet_path.instance_path.clone(),
        symbol_uuid: symbol.uuid.clone(),
        at: point_key(at),
        name: Some(pin_name.to_string()),
    }
}

// Upstream parity: reduced local analogue for the symbol-pin item half of
// `CONNECTION_GRAPH::GetSubgraphForItem()` on the project graph path. This is not a 1:1 KiCad
// item map because the Rust tree still uses `(sheet instance path, symbol uuid, projected pin
// point)` instead of a live `SCH_PIN*`, but it now derives shared net identity through the stored
// pin-to-subgraph owner instead of keeping a second pin-to-net side map. Remaining divergence is
// fuller item identity for non-pin items and the still-missing `CONNECTION_SUBGRAPH` object.
pub(crate) fn resolve_reduced_project_net_for_symbol_pin(
    graph: &ReducedProjectNetGraph,
    sheet_path: &LoadedSheetPath,
    symbol: &Symbol,
    at: [f64; 2],
    pin_name: Option<&str>,
) -> Option<ReducedProjectNetIdentity> {
    resolve_reduced_project_subgraph_for_symbol_pin(graph, sheet_path, symbol, at, pin_name).map(
        |subgraph| ReducedProjectNetIdentity {
            code: subgraph.code,
            name: subgraph.name.clone(),
            class: subgraph.class.clone(),
            has_no_connect: subgraph.has_no_connect,
        },
    )
}

#[cfg_attr(not(test), allow(dead_code))]
// Upstream parity: reduced local analogue for the symbol-pin half of
// `CONNECTION_GRAPH::GetSubgraphForItem()` on the project graph path. This is not a 1:1 KiCad
// item map because the Rust tree still uses `(sheet instance path, symbol uuid, projected pin
// point)` instead of a live `SCH_PIN*`, but it preserves shared pin-to-subgraph identity instead
// of flattening all pin lookups straight to whole-net identity. Remaining divergence is fuller
// item ownership for non-pin items and the still-missing live `CONNECTION_SUBGRAPH` object.
pub(crate) fn resolve_reduced_project_subgraph_for_symbol_pin<'a>(
    graph: &'a ReducedProjectNetGraph,
    sheet_path: &LoadedSheetPath,
    symbol: &Symbol,
    at: [f64; 2],
    pin_name: Option<&str>,
) -> Option<&'a ReducedProjectSubgraphEntry> {
    pin_name
        .and_then(|pin_name| {
            graph
                .pin_subgraph_identities
                .get(&reduced_project_base_pin_key(
                    sheet_path, symbol, at, pin_name,
                ))
        })
        .and_then(|index| graph.subgraphs.get(*index))
        .or_else(|| {
            graph
                .pin_subgraph_identities_by_location
                .get(&reduced_project_pin_identity_key(sheet_path, symbol, at))
                .and_then(|index| graph.subgraphs.get(*index))
        })
}

// Upstream parity: reduced local analogue for the symbol-pin `Name(true)` path via
// `CONNECTION_GRAPH::GetSubgraphForItem()`. This is not a 1:1 KiCad connection object because the
// Rust tree still lacks live `SCH_CONNECTION` objects, but it preserves shared local driver-name
// ownership for pin text vars instead of trimming the full resolved net name after graph lookup.
// Remaining divergence is the still-missing live connection object and fuller subgraph ownership.
pub(crate) fn resolve_reduced_project_driver_name_for_symbol_pin(
    graph: &ReducedProjectNetGraph,
    sheet_path: &LoadedSheetPath,
    symbol: &Symbol,
    at: [f64; 2],
    pin_name: Option<&str>,
) -> Option<String> {
    resolve_reduced_project_subgraph_for_symbol_pin(graph, sheet_path, symbol, at, pin_name)
        .map(|subgraph| subgraph.driver_name.clone())
}

// Upstream parity: reduced local analogue for the connection-point half of
// `CONNECTION_GRAPH::GetSubgraphForItem()` / `GetResolvedSubgraphName()` on the project graph
// path. This is not a 1:1 KiCad item map because the Rust tree still keys the lookup by `(sheet
// instance path, reduced subgraph anchor)` instead of a live item-owned `CONNECTION_SUBGRAPH`,
// but it now derives shared net identity through the stored point-to-subgraph owner instead of a
// duplicate point-to-net side map. Remaining divergence is fuller item identity for labels, wires,
// and markers plus the still-missing `CONNECTION_SUBGRAPH` object.
pub(crate) fn resolve_reduced_project_net_at(
    graph: &ReducedProjectNetGraph,
    sheet_path: &LoadedSheetPath,
    at: [f64; 2],
) -> Option<ReducedProjectNetIdentity> {
    resolve_reduced_project_subgraph_at(graph, sheet_path, at).map(|subgraph| {
        ReducedProjectNetIdentity {
            code: subgraph.code,
            name: subgraph.name.clone(),
            class: subgraph.class.clone(),
            has_no_connect: subgraph.has_no_connect,
        }
    })
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

// Upstream parity: reduced local analogue for the label-item `IsDangling()` facts consumed by
// `CONNECTION_GRAPH::ercCheckLabels()` / `ercCheckDirectiveLabels()`. This is not a 1:1 KiCad
// subgraph snapshot because the Rust tree still lacks live `SCH_TEXT*` objects and graph-owned
// dangling state. It now exists only for the remaining per-label dangling probe while the shared
// project subgraph owner carries the broader label/pin/no-connect grouping facts.
pub(crate) fn collect_reduced_label_component_snapshots(
    schematic: &Schematic,
) -> Vec<ReducedLabelComponentSnapshot> {
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

            Some(ReducedLabelComponentSnapshot { labels })
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReducedNetNameSource {
    GlobalLabel,
    LocalLabel,
    HierarchicalLabel,
    SheetPin,
    GlobalPowerPin,
    LocalPowerPin,
    SymbolPinDefault,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReducedDriverNameCandidate {
    priority: i32,
    sheet_pin_rank: i32,
    text: String,
    source: ReducedNetNameSource,
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

// Upstream parity: reduced local analogue for the non-bus winning-driver-priority query implied by
// `CONNECTION_SUBGRAPH::GetDriverPriority( m_driver )` after `ResolveDrivers()` on the bus-member
// side of `ercCheckBusToBusEntryConflicts()`. This is not a 1:1 KiCad subgraph owner because the
// Rust tree still lacks cached `CONNECTION_SUBGRAPH` objects and separate bus-vs-member subgraphs.
// The local helper exists because the reduced component currently merges both sides of a bus entry,
// so the ERC pass needs one shared graph query that ignores bus labels and returns only the
// strongest non-bus driver priority instead of re-ranking labels and power pins locally. Remaining
// divergence is fuller subgraph ownership and non-strong driver participation.
pub(crate) fn resolve_reduced_non_bus_driver_priority_at<F>(
    schematic: &Schematic,
    at: [f64; 2],
    shown_label_text: F,
) -> Option<i32>
where
    F: FnMut(&Label) -> String,
{
    let connected_component = connection_component_at(schematic, at)?;
    collect_reduced_strong_drivers(schematic, &connected_component, shown_label_text)
        .into_iter()
        .find(|driver| !reduced_text_is_bus(schematic, &driver.name))
        .map(|driver| driver.priority)
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
// - equal-priority bus labels first prefer supersets over subsets to keep the widest connection
//   before falling back to sheet-pin rank / name quality / alphabetical order
// - labels whose raw text still depends on the reduced connectivity resolver are skipped so the
//   current reduced driver path does not recurse back into itself
fn resolve_reduced_driver_name_candidate_on_component<F>(
    schematic: &Schematic,
    connected_component: &ConnectionComponent,
    mut shown_label_text: F,
) -> Option<ReducedDriverNameCandidate>
where
    F: FnMut(&Label) -> String,
{
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
                let source = match label.kind {
                    LabelKind::Global => ReducedNetNameSource::GlobalLabel,
                    LabelKind::Local => ReducedNetNameSource::LocalLabel,
                    LabelKind::Hierarchical => ReducedNetNameSource::HierarchicalLabel,
                    LabelKind::Directive => return None,
                };
                Some(ReducedDriverNameCandidate {
                    priority: reduced_label_driver_priority(label),
                    sheet_pin_rank: 0,
                    text,
                    source,
                })
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
                .map(|pin| ReducedDriverNameCandidate {
                    priority: 0,
                    sheet_pin_rank: reduced_sheet_pin_driver_rank(pin.shape),
                    text: pin.name.clone(),
                    source: ReducedNetNameSource::SheetPin,
                })
                .max_by(|lhs, rhs| {
                    lhs.sheet_pin_rank
                        .cmp(&rhs.sheet_pin_rank)
                        .then_with(|| rhs.text.cmp(&lhs.text))
                }),
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
                                symbol_value_text(symbol).map(|text| {
                                    let source = if symbol
                                        .lib_symbol
                                        .as_ref()
                                        .is_some_and(|lib_symbol| lib_symbol.local_power)
                                    {
                                        ReducedNetNameSource::LocalPowerPin
                                    } else {
                                        ReducedNetNameSource::GlobalPowerPin
                                    };

                                    ReducedDriverNameCandidate {
                                        priority,
                                        sheet_pin_rank: 0,
                                        text,
                                        source,
                                    }
                                })
                            })
                            .or_else(|| {
                                reduced_symbol_pin_default_net_name(symbol, &pin, &unit_pins).map(
                                    |text| ReducedDriverNameCandidate {
                                        priority: 1,
                                        sheet_pin_rank: 0,
                                        text,
                                        source: ReducedNetNameSource::SymbolPinDefault,
                                    },
                                )
                            })
                    })
            }
            _ => None,
        })
        .filter(|candidate| {
            !candidate.text.is_empty()
                && !candidate.text.contains("${")
                && !candidate.text.starts_with('<')
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|lhs, rhs| {
        let lhs_low_quality_name = lhs.text.contains("-Pad");
        let rhs_low_quality_name = rhs.text.contains("-Pad");

        rhs.priority
            .cmp(&lhs.priority)
            .then_with(|| reduced_bus_subset_cmp(schematic, &lhs.text, &rhs.text))
            .then_with(|| rhs.sheet_pin_rank.cmp(&lhs.sheet_pin_rank))
            .then_with(|| lhs_low_quality_name.cmp(&rhs_low_quality_name))
            .then_with(|| lhs.text.cmp(&rhs.text))
    });

    candidates.into_iter().next()
}

fn resolve_reduced_driver_name_on_component<F>(
    schematic: &Schematic,
    connected_component: &ConnectionComponent,
    shown_label_text: F,
) -> Option<String>
where
    F: FnMut(&Label) -> String,
{
    resolve_reduced_driver_name_candidate_on_component(
        schematic,
        connected_component,
        shown_label_text,
    )
    .map(|candidate| candidate.text)
}

fn resolve_reduced_net_name_on_component<F>(
    schematic: &Schematic,
    connected_component: &ConnectionComponent,
    sheet_path_prefix: Option<&str>,
    shown_label_text: F,
) -> Option<String>
where
    F: FnMut(&Label) -> String,
{
    resolve_reduced_driver_name_candidate_on_component(
        schematic,
        connected_component,
        shown_label_text,
    )
    .map(|candidate| {
        let prepend_path = matches!(
            candidate.source,
            ReducedNetNameSource::LocalLabel
                | ReducedNetNameSource::HierarchicalLabel
                | ReducedNetNameSource::SheetPin
                | ReducedNetNameSource::LocalPowerPin
        );

        if prepend_path {
            match sheet_path_prefix {
                Some(prefix) => format!("{prefix}{}", candidate.text),
                None => candidate.text,
            }
        } else {
            candidate.text
        }
    })
}

// Upstream parity: reduced local analogue for the current-sheet `CONNECTION_GRAPH::GetSubgraphForItem()`
// + `SCH_CONNECTION::Name(false)` path used by label text, ERC, and exporters. This is not a 1:1
// KiCad owner because the Rust tree still lacks real `CONNECTION_SUBGRAPH` / `SCH_CONNECTION`
// objects, but the shared reduced owner now distinguishes path-qualified full net names from short
// driver names using the same driver-kind split KiCad applies in `SCH_CONNECTION::recacheName()`.
// Remaining divergence is fuller bus/subgraph/item identity beyond the current reduced component
// carrier.
pub(crate) fn resolve_reduced_net_name_at<F>(
    schematic: &Schematic,
    at: [f64; 2],
    sheet_path_prefix: Option<&str>,
    shown_label_text: F,
) -> Option<String>
where
    F: FnMut(&Label) -> String,
{
    let connected_component = connection_component_at(schematic, at)?;
    resolve_reduced_net_name_on_component(
        schematic,
        &connected_component,
        sheet_path_prefix,
        shown_label_text,
    )
}

// Upstream parity: reduced local analogue for the symbol-pin item lookup half of
// `CONNECTION_GRAPH::GetSubgraphForItem()` on the net-name path. This is not a 1:1 KiCad item map
// because the Rust tree still identifies a placed pin by `(symbol uuid, projected pin at)` instead
// of a live `SCH_PIN*`, but it lets pin-owned ERC/shown-text paths resolve against a symbol-pin
// component owner instead of a raw point query.
pub(crate) fn resolve_reduced_net_name_for_symbol_pin<F>(
    schematic: &Schematic,
    symbol: &Symbol,
    at: [f64; 2],
    sheet_path_prefix: Option<&str>,
    shown_label_text: F,
) -> Option<String>
where
    F: FnMut(&Label) -> String,
{
    let connected_component = connection_component_for_symbol_pin(schematic, symbol, at)?;
    resolve_reduced_net_name_on_component(
        schematic,
        &connected_component,
        sheet_path_prefix,
        shown_label_text,
    )
}

// Upstream parity: reduced local analogue for the driver-netclass lookup side of
// `CONNECTION_SUBGRAPH::GetNetclassesForDriver()`. This is not a 1:1 KiCad graph query because the
// Rust tree still lacks cached rule-area ownership, child-item traversal, and full subgraph
// drivers. It exists so loader shown-text and export paths query one shared reduced connectivity
// owner for current-sheet `NET_CLASS` instead of rebuilding directive/rule-area scans locally. The
// shared owner now also propagates bus-label netclass assignments down to included bus members
// instead of leaving that expansion loader-local. Remaining divergence is fuller rule-area and
// subgraph-owned netclass state.
pub(crate) fn resolve_reduced_netclass_at<F, G, H>(
    schematic: &Schematic,
    at: [f64; 2],
    mut directive_netclass: F,
    mut shown_label_text: G,
    mut label_netclass: H,
) -> Option<String>
where
    F: FnMut(&Label) -> Option<String>,
    G: FnMut(&Label) -> String,
    H: FnMut(&Label) -> Option<String>,
{
    let connected_component = connection_component_at(schematic, at);
    let component_net_name = connected_component.as_ref().and_then(|component| {
        resolve_reduced_net_name_on_component(schematic, component, None, |label| {
            shown_label_text(label)
        })
    });
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
        .or_else(|| {
            let connected_component = connected_component.as_ref()?;
            let mut labels = schematic
                .screen
                .items
                .iter()
                .filter_map(|item| match item {
                    SchItem::Label(label)
                        if label.kind != LabelKind::Directive
                            && connected_component.members.iter().any(|member| {
                                member.kind == ConnectionMemberKind::Label
                                    && points_equal(member.at, label.at)
                            }) =>
                    {
                        Some(label)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();

            labels.sort_by(|lhs, rhs| {
                reduced_label_driver_priority(rhs)
                    .cmp(&reduced_label_driver_priority(lhs))
                    .then_with(|| lhs.text.cmp(&rhs.text))
            });

            labels.into_iter().find_map(&mut label_netclass)
        })
        .or_else(|| {
            let net_name = component_net_name.as_ref()?;
            let mut labels = schematic
                .screen
                .items
                .iter()
                .filter_map(|item| match item {
                    SchItem::Label(label) if label.kind != LabelKind::Directive => {
                        let shown = shown_label_text(label);
                        let members = reduced_bus_members(schematic, &shown);

                        (!members.is_empty() && members.iter().any(|member| member == net_name))
                            .then_some(label)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();

            labels.sort_by(|lhs, rhs| {
                reduced_label_driver_priority(rhs)
                    .cmp(&reduced_label_driver_priority(lhs))
                    .then_with(|| shown_label_text(lhs).cmp(&shown_label_text(rhs)))
            });

            labels.into_iter().find_map(&mut label_netclass)
        })
}

#[cfg(test)]
mod tests {
    use super::{
        find_first_reduced_project_subgraph_by_name, find_reduced_project_subgraph_by_name,
        resolve_reduced_net_name_at, resolve_reduced_project_net_at,
        resolve_reduced_project_subgraph_at, resolve_reduced_project_subgraph_for_label,
        resolve_reduced_project_subgraph_for_no_connect,
        resolve_reduced_project_subgraph_for_symbol_pin,
    };
    use crate::core::SchematicProject;
    use crate::loader::load_schematic_tree;
    use crate::model::SchItem;
    use crate::parser::parse_schematic_file;
    use std::env;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn reduced_net_name_prefers_wider_bus_driver() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_bus_driver_{}.kicad_sch",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));

        fs::write(
            &path,
            r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (uuid "73050000-0000-0000-0000-000000000001")
  (paper "A4")
  (bus (pts (xy 0 0) (xy 20 0)))
  (global_label "DATA[0..3]" (shape input) (at 0 0 0) (effects (font (size 1 1))))
  (global_label "DATA[0..7]" (shape input) (at 20 0 0) (effects (font (size 1 1)))))"#,
        )
        .expect("write schematic");

        let schematic = parse_schematic_file(&path).expect("parse schematic");
        let resolved =
            resolve_reduced_net_name_at(&schematic, [0.0, 0.0], None, |label| label.text.clone());

        assert_eq!(resolved.as_deref(), Some("DATA[0..7]"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reduced_bus_members_expand_top_level_alias_vectors() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_bus_alias_{}.kicad_sch",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));

        fs::write(
            &path,
            r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (uuid "73050000-0000-0000-0000-000000000002")
  (paper "A4")
  (bus_alias "DATA" (members D[0..3]))
  (bus_alias "PAIR" (members DP DM))
  (bus_alias "USBPAIR" (members PAIR))
)"#,
        )
        .expect("write schematic");

        let schematic = parse_schematic_file(&path).expect("parse schematic");

        assert_eq!(
            super::reduced_bus_members(&schematic, "DATA"),
            vec!["D0", "D1", "D2", "D3"]
        );
        assert_eq!(
            super::reduced_bus_members(&schematic, "USBPAIR"),
            vec!["DP", "DM"]
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reduced_project_net_identity_covers_non_anchor_label_points() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_project_points_{}.kicad_sch",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));

        fs::write(
            &path,
            r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (uuid "73050000-0000-0000-0000-000000000003")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "Reference" "R" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "R" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "R_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:R")
    (uuid "73050000-0000-0000-0000-000000000004")
    (at 0 0 0)
    (property "Reference" "R1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "10k" (at 0 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy 10 0)))
  (global_label "NET_A" (shape input) (at 10 0 0) (effects (font (size 1 1)))))"#,
        )
        .expect("write schematic");

        let loaded = load_schematic_tree(&path).expect("load tree");
        let sheet_path = loaded
            .sheet_paths
            .iter()
            .find(|sheet_path| sheet_path.instance_path.is_empty())
            .cloned()
            .expect("root sheet path");
        let project = SchematicProject::from_load_result(loaded);

        let graph = project.reduced_project_net_graph(false);
        let net = resolve_reduced_project_net_at(&graph, &sheet_path, [10.0, 0.0])
            .expect("project net at label point");

        assert_eq!(net.name, "NET_A");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reduced_project_item_identity_covers_labels_and_no_connects() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_project_items_{}.kicad_sch",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));

        fs::write(
            &path,
            r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (uuid "73050000-0000-0000-0000-000000000103")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "Reference" "R" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "R" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "R_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:R")
    (uuid "73050000-0000-0000-0000-000000000104")
    (at 0 0 0)
    (property "Reference" "R1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "10k" (at 0 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy 10 0)))
  (global_label "NET_A" (shape input) (at 10 0 0) (effects (font (size 1 1))))
  (no_connect (at 10 0)))"#,
        )
        .expect("write schematic");

        let loaded = load_schematic_tree(&path).expect("load tree");
        let sheet_path = loaded
            .sheet_paths
            .iter()
            .find(|sheet_path| sheet_path.instance_path.is_empty())
            .cloned()
            .expect("root sheet path");
        let project = SchematicProject::from_load_result(loaded);
        let graph = project.reduced_project_net_graph(false);
        let schematic = project
            .schematic(&sheet_path.schematic_path)
            .expect("root schematic");

        let label = schematic
            .screen
            .items
            .iter()
            .find_map(|item| match item {
                SchItem::Label(label) => Some(label),
                _ => None,
            })
            .expect("label");
        let no_connect = schematic
            .screen
            .items
            .iter()
            .find_map(|item| match item {
                SchItem::NoConnect(no_connect) => Some(no_connect),
                _ => None,
            })
            .expect("no-connect");

        let by_label = resolve_reduced_project_subgraph_for_label(&graph, &sheet_path, label)
            .expect("label subgraph");
        let by_no_connect =
            resolve_reduced_project_subgraph_for_no_connect(&graph, &sheet_path, no_connect.at)
                .expect("no-connect subgraph");
        let by_point = resolve_reduced_project_subgraph_at(&graph, &sheet_path, [10.0, 0.0])
            .expect("point subgraph");

        assert_eq!(by_label.subgraph_code, by_point.subgraph_code);
        assert_eq!(by_no_connect.subgraph_code, by_point.subgraph_code);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reduced_project_subgraph_lookup_uses_sheet_and_full_net_name() {
        let dir = env::temp_dir().join(format!(
            "ki2_connectivity_project_subgraphs_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("mkdir");
        let root_path = dir.join("root.kicad_sch");
        let child_path = dir.join("child.kicad_sch");

        fs::write(
            &child_path,
            r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "Device:R"
      (property "Reference" "R" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "R" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "R_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "~" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:R")
    (uuid "73050000-0000-0000-0000-000000000014")
    (at 0 0 0)
    (property "Reference" "R1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "10k" (at 0 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy 10 0)))
  (label "SIG" (at 10 0 0) (effects (font (size 1 1)))))"#,
        )
        .expect("write child");

        fs::write(
            &root_path,
            format!(
                r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (sheet
    (at 0 0)
    (size 20 10)
    (uuid "73050000-0000-0000-0000-000000000013")
    (property "Sheetname" "Child" (id 0) (at 0 0 0) (effects (font (size 1 1))))
    (property "Sheetfile" "{}" (id 1) (at 0 0 0) (effects (font (size 1 1)))))
  (sheet_instances
    (path "" (page "1"))
    (path "/73050000-0000-0000-0000-000000000013" (page "2"))))"#,
                child_path.display()
            ),
        )
        .expect("write root");

        let loaded = load_schematic_tree(&root_path).expect("load tree");
        let project = SchematicProject::from_load_result(loaded);
        let child_sheet = project
            .sheet_paths
            .iter()
            .find(|sheet_path| !sheet_path.instance_path.is_empty())
            .expect("child sheet path");
        let graph = project.reduced_project_net_graph(false);

        let by_sheet = find_reduced_project_subgraph_by_name(&graph, "/Child/SIG", child_sheet)
            .expect("sheet-local subgraph");
        assert_eq!(by_sheet.subgraph_code, 1);
        assert_eq!(by_sheet.code, 1);
        assert_eq!(by_sheet.driver_name, "SIG");
        assert_eq!(by_sheet.name, "/Child/SIG");

        let by_point = resolve_reduced_project_subgraph_at(&graph, child_sheet, [10.0, 0.0])
            .expect("point subgraph");
        assert_eq!(by_point.subgraph_code, by_sheet.subgraph_code);
        assert_eq!(by_point.name, "/Child/SIG");

        let child_schematic = project
            .schematic(&child_sheet.schematic_path)
            .expect("child schematic");
        let child_symbol = child_schematic
            .screen
            .items
            .iter()
            .find_map(|item| match item {
                SchItem::Symbol(symbol) => Some(symbol),
                _ => None,
            })
            .expect("child symbol");
        let by_pin = resolve_reduced_project_subgraph_for_symbol_pin(
            &graph,
            child_sheet,
            child_symbol,
            [0.0, 0.0],
            Some("~"),
        )
        .expect("pin subgraph");
        assert_eq!(by_pin.subgraph_code, by_sheet.subgraph_code);
        assert_eq!(by_pin.name, "/Child/SIG");

        let by_full_name = find_first_reduced_project_subgraph_by_name(&graph, "/Child/SIG")
            .expect("full-name subgraph");
        assert_eq!(by_full_name.subgraph_code, by_sheet.subgraph_code);
        assert_eq!(by_full_name.sheet_instance_path, child_sheet.instance_path);
        assert_eq!(by_full_name.driver_name, "SIG");

        let _ = fs::remove_file(root_path);
        let _ = fs::remove_file(child_path);
        let _ = fs::remove_dir(dir);
    }
}
