use std::collections::{BTreeMap, BTreeSet};

use crate::core::SchematicProject;
use crate::loader::{
    LoadedProjectSettings, LoadedSheetPath, SymbolPinTextVarKind, collect_wire_segments,
    point_on_wire_segment, points_equal, reduced_net_name_sheet_path_prefix,
    resolve_point_connectivity_text_var, resolved_sheet_text_state,
    resolved_symbol_text_property_value, shown_label_text_without_connectivity,
    shown_sheet_pin_text,
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ReducedBusMemberKind {
    Net,
    Bus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ReducedProjectConnectionType {
    None,
    Net,
    Bus,
    BusGroup,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ReducedBusMember {
    pub(crate) net_code: usize,
    pub(crate) name: String,
    pub(crate) local_name: String,
    pub(crate) full_local_name: String,
    pub(crate) vector_index: Option<usize>,
    pub(crate) kind: ReducedBusMemberKind,
    pub(crate) members: Vec<ReducedBusMember>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ReducedProjectConnection {
    pub(crate) net_code: usize,
    pub(crate) connection_type: ReducedProjectConnectionType,
    pub(crate) name: String,
    pub(crate) local_name: String,
    pub(crate) full_local_name: String,
    pub(crate) sheet_instance_path: String,
    pub(crate) members: Vec<ReducedBusMember>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ReducedProjectBusNeighborLink {
    pub(crate) member: ReducedBusMember,
    pub(crate) subgraph_index: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReducedLabelLink {
    pub(crate) at: PointKey,
    pub(crate) kind: LabelKind,
    pub(crate) connection: ReducedProjectConnection,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ReducedHierSheetPinLink {
    pub(crate) at: PointKey,
    pub(crate) child_sheet_uuid: Option<String>,
    pub(crate) connection: ReducedProjectConnection,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ReducedHierPortLink {
    pub(crate) at: PointKey,
    pub(crate) connection: ReducedProjectConnection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReducedProjectSubgraphEntry {
    pub(crate) subgraph_code: usize,
    pub(crate) code: usize,
    pub(crate) name: String,
    pub(crate) resolved_connection: ReducedProjectConnection,
    pub(crate) driver_connection: Option<ReducedProjectConnection>,
    pub(crate) driver_identity: Option<ReducedProjectDriverIdentity>,
    pub(crate) drivers: Vec<ReducedProjectStrongDriver>,
    pub(crate) non_bus_driver_priority: Option<i32>,
    pub(crate) class: String,
    pub(crate) has_no_connect: bool,
    pub(crate) sheet_instance_path: String,
    pub(crate) anchor: PointKey,
    pub(crate) points: Vec<PointKey>,
    pub(crate) nodes: Vec<ReducedNetNode>,
    pub(crate) base_pins: Vec<ReducedNetBasePinKey>,
    pub(crate) label_links: Vec<ReducedLabelLink>,
    pub(crate) no_connect_points: Vec<PointKey>,
    pub(crate) hier_sheet_pins: Vec<ReducedHierSheetPinLink>,
    pub(crate) hier_ports: Vec<ReducedHierPortLink>,
    pub(crate) bus_members: Vec<ReducedBusMember>,
    pub(crate) bus_items: Vec<ReducedSubgraphWireItem>,
    pub(crate) wire_items: Vec<ReducedSubgraphWireItem>,
    pub(crate) bus_neighbor_links: Vec<ReducedProjectBusNeighborLink>,
    pub(crate) bus_parent_links: Vec<ReducedProjectBusNeighborLink>,
    pub(crate) bus_parent_indexes: Vec<usize>,
    pub(crate) hier_parent_index: Option<usize>,
    pub(crate) hier_child_indexes: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ReducedProjectDriverIdentity {
    Label {
        schematic_path: std::path::PathBuf,
        at: PointKey,
        kind: u8,
    },
    SheetPin {
        schematic_path: std::path::PathBuf,
        at: PointKey,
    },
    SymbolPin {
        schematic_path: std::path::PathBuf,
        symbol_uuid: Option<String>,
        at: PointKey,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ReducedProjectDriverKind {
    Label,
    SheetPin,
    PowerPin,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ReducedProjectStrongDriver {
    pub(crate) kind: ReducedProjectDriverKind,
    pub(crate) priority: i32,
    pub(crate) name: String,
    pub(crate) full_name: String,
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
    pub(crate) connected_bus_subgraph_index: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReducedProjectNetGraph {
    subgraphs: Vec<ReducedProjectSubgraphEntry>,
    subgraphs_by_name: BTreeMap<String, Vec<usize>>,
    subgraphs_by_sheet_and_name: BTreeMap<(String, String), Vec<usize>>,
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

// Upstream parity: reduced local analogue for the connection-kind portion of
// `SCH_CONNECTION::Type()` after `ConfigureFromLabel()`. This is not a 1:1 connection-object
// query because the Rust tree still derives type from reduced text plus alias parsing instead of
// cached `SCH_CONNECTION` state, but it now centralizes the shared graph's reduced net/bus/group
// discrimination on the same owner that stores local/full-local/resolved names. Remaining
// divergence is fuller live connection-object caching beyond this reduced type carrier.
fn reduced_project_connection_type(
    schematic: &Schematic,
    text: &str,
) -> ReducedProjectConnectionType {
    if text.is_empty() {
        ReducedProjectConnectionType::None
    } else if !reduced_text_is_bus(schematic, text) {
        ReducedProjectConnectionType::Net
    } else if text.contains('{') || text.contains('}') {
        ReducedProjectConnectionType::BusGroup
    } else {
        ReducedProjectConnectionType::Bus
    }
}

// Upstream parity: reduced local helper for the `SCH_CONNECTION` name/type/member state the
// shared graph still lacks as a live object. This is not a 1:1 upstream routine because the Rust
// tree still stores cloned reduced data on the subgraph owner instead of live `SCH_CONNECTION`
// instances, but it now keeps the exercised resolved/local/full-local/member tuple together on one
// owner rather than spreading it across unrelated string fields. Remaining divergence is fuller
// clone/update behavior and parent-neighbor ownership on live connection objects.
fn build_reduced_project_connection(
    schematic: &Schematic,
    sheet_instance_path: impl Into<String>,
    resolved_name: impl Into<String>,
    local_name: impl Into<String>,
    full_local_name: impl Into<String>,
    members: Vec<ReducedBusMember>,
) -> ReducedProjectConnection {
    let sheet_instance_path = sheet_instance_path.into();
    let resolved_name = resolved_name.into();
    let local_name = local_name.into();
    let full_local_name = full_local_name.into();
    let type_name = if !local_name.is_empty() {
        local_name.as_str()
    } else {
        resolved_name.as_str()
    };

    ReducedProjectConnection {
        net_code: 0,
        connection_type: reduced_project_connection_type(schematic, type_name),
        name: resolved_name,
        local_name,
        full_local_name,
        sheet_instance_path,
        members,
    }
}

fn reduced_bus_members_inner(
    schematic: &Schematic,
    text: &str,
    active_aliases: &mut BTreeSet<String>,
) -> Vec<String> {
    reduced_bus_member_full_local_names(&collect_reduced_bus_member_objects_inner(
        schematic,
        text,
        "",
        "",
        active_aliases,
    ))
}

fn split_reduced_bus_group_members(inner: &str) -> Vec<String> {
    let mut members = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut escaped = false;

    for ch in inner.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }

        match ch {
            '\\' => {
                current.push(ch);
                escaped = true;
            }
            '{' => {
                depth += 1;
                current.push(ch);
            }
            '}' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            _ if ch.is_whitespace() && depth == 0 => {
                if !current.is_empty() {
                    members.push(current);
                    current = String::new();
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        members.push(current);
    }

    members
}

fn reduced_bus_vector_members(text: &str) -> Option<Vec<String>> {
    let (prefix, suffix) = text.split_once('[')?;
    let range = suffix.strip_suffix(']')?;
    let (start, end) = range.split_once("..")?;
    let start = start.parse::<i32>().ok()?;
    let end = end.parse::<i32>().ok()?;
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

    Some(members)
}

pub(crate) fn reduced_bus_member_full_local_names(members: &[ReducedBusMember]) -> Vec<String> {
    let mut names = Vec::new();

    fn collect(members: &[ReducedBusMember], names: &mut Vec<String>) {
        for member in members {
            if member.kind == ReducedBusMemberKind::Bus {
                collect(&member.members, names);
            } else {
                names.push(member.full_local_name.clone());
            }
        }
    }

    collect(members, &mut names);
    names
}

fn reduced_bus_member_leaf_objects(members: &[ReducedBusMember]) -> Vec<ReducedBusMember> {
    let mut leaves = Vec::new();

    fn collect(members: &[ReducedBusMember], leaves: &mut Vec<ReducedBusMember>) {
        for member in members {
            if member.kind == ReducedBusMemberKind::Bus {
                collect(&member.members, leaves);
            } else {
                leaves.push(member.clone());
            }
        }
    }

    collect(members, &mut leaves);
    leaves
}

#[cfg_attr(not(test), allow(dead_code))]
// Upstream parity: reduced local analogue for `CONNECTION_GRAPH::matchBusMember()`. This is not a
// 1:1 KiCad live-connection remap because the Rust tree still matches reduced member snapshots
// instead of `SCH_CONNECTION*`, but it now preserves the same exercised rule split:
// - vector buses remap by vector index even if the visible member name changes
// - bus groups remap by local member name
// Remaining divergence is live cloned-member refresh after hierarchy propagation.
fn match_reduced_bus_member<'a>(
    bus_members: &'a [ReducedBusMember],
    search: &ReducedBusMember,
) -> Option<&'a ReducedBusMember> {
    for member in bus_members {
        if let Some(search_index) = search.vector_index {
            if member.vector_index == Some(search_index) {
                return Some(member);
            }
        }

        if member.kind == ReducedBusMemberKind::Bus {
            if let Some(found) = match_reduced_bus_member(&member.members, search) {
                return Some(found);
            }
        } else if member.local_name == search.local_name {
            return Some(member);
        }
    }

    None
}

// Upstream parity: reduced local analogue for the mutable half of
// `CONNECTION_GRAPH::matchBusMember()` during graph propagation updates. This is not a 1:1 live
// `SCH_CONNECTION*` matcher because the Rust tree still mutates reduced member snapshots instead of
// real connection objects, but it preserves the same exercised vector-index vs local-name matching
// split before later reduced recache passes consume the updated member. Remaining divergence is the
// still-missing in-place mutation of live connection objects and their cached parent pointers.
fn match_reduced_bus_member_mut<'a>(
    bus_members: &'a mut [ReducedBusMember],
    search: &ReducedBusMember,
) -> Option<&'a mut ReducedBusMember> {
    for member in bus_members {
        if let Some(search_index) = search.vector_index {
            if member.vector_index == Some(search_index) {
                return Some(member);
            }
        }

        if member.kind == ReducedBusMemberKind::Bus {
            if let Some(found) = match_reduced_bus_member_mut(&mut member.members, search) {
                return Some(found);
            }
        } else if member.local_name == search.local_name {
            return Some(member);
        }
    }

    None
}

// Upstream parity: reduced local analogue for `SCH_CONNECTION::Clone()` when a propagated member
// net replaces an older bus member name. This is not a 1:1 live clone because the Rust tree still
// copies reduced connection snapshots into `ReducedBusMember`, but it preserves the exercised name
// and type refresh while keeping the reduced vector index metadata needed by later member remap
// passes. Remaining divergence is the still-missing live pointer/cache mutation on real connection
// objects.
fn clone_reduced_connection_into_bus_member(
    member: &mut ReducedBusMember,
    connection: &ReducedProjectConnection,
) {
    member.net_code = connection.net_code;
    member.name = connection.local_name.clone();
    member.local_name = connection.local_name.clone();
    member.full_local_name = connection.full_local_name.clone();
    member.kind = match connection.connection_type {
        ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup => {
            ReducedBusMemberKind::Bus
        }
        _ => ReducedBusMemberKind::Net,
    };
    member.members = connection.members.clone();
}

// Upstream parity: reduced local analogue for the `UpdateItemConnections()`-visible name refresh
// KiCad applies after `m_driver_connection->Clone()` / `recacheSubgraphName()`. This is not a 1:1
// item update because the Rust tree still mutates reduced link snapshots instead of live
// `SCH_ITEM`-owned connections, but it keeps the exercised subgraph-resolved connection names and
// text-link connection owners aligned once the shared graph renames a net subgraph. Remaining
// divergence is the still-missing live item connection cache on real subgraph objects.
fn clone_reduced_connection_into_subgraph(
    subgraph: &mut ReducedProjectSubgraphEntry,
    connection: &ReducedProjectConnection,
) {
    subgraph.name = connection.name.clone();
    subgraph.resolved_connection = connection.clone();

    if let Some(driver_connection) = &mut subgraph.driver_connection {
        *driver_connection = connection.clone();
    }

    for link in &mut subgraph.label_links {
        link.connection = connection.clone();
    }

    for pin in &mut subgraph.hier_sheet_pins {
        pin.connection = connection.clone();
    }

    for port in &mut subgraph.hier_ports {
        port.connection = connection.clone();
    }
}

// Upstream parity: reduced local analogue for `assignNewNetCode()` / `assignNetCodesToBus()`.
// This is not a 1:1 live `SCH_CONNECTION` mutation because the Rust tree still assigns onto
// reduced snapshot connections after propagation settles, but it moves netcode ownership onto the
// shared reduced connection objects and their non-bus bus members instead of keeping it only on
// detached whole-net entries. Remaining divergence is the still-missing in-place live clone/update
// timing on real connection objects during propagation.
fn assign_reduced_connection_net_codes(
    connection: &mut ReducedProjectConnection,
    net_codes: &mut BTreeMap<String, usize>,
) {
    if !connection.name.is_empty() {
        let next_code = net_codes.len() + 1;
        connection.net_code = *net_codes
            .entry(connection.name.clone())
            .or_insert(next_code);
    } else {
        connection.net_code = 0;
    }

    assign_reduced_bus_member_net_codes(&mut connection.members, net_codes);
}

fn assign_reduced_bus_member_net_codes(
    members: &mut [ReducedBusMember],
    net_codes: &mut BTreeMap<String, usize>,
) {
    for member in members {
        if member.kind == ReducedBusMemberKind::Bus {
            member.net_code = 0;
            assign_reduced_bus_member_net_codes(&mut member.members, net_codes);
            continue;
        }

        if !member.full_local_name.is_empty() {
            let next_code = net_codes.len() + 1;
            member.net_code = *net_codes
                .entry(member.full_local_name.clone())
                .or_insert(next_code);
        } else {
            member.net_code = 0;
        }
    }
}

fn reduced_sheet_path_depth(sheet_instance_path: &str) -> usize {
    if sheet_instance_path.is_empty() {
        0
    } else {
        sheet_instance_path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .count()
    }
}

fn reduced_strong_driver_priority(subgraph: &ReducedProjectSubgraphEntry) -> Option<i32> {
    subgraph.drivers.first().map(|driver| driver.priority)
}

fn reduced_subgraph_driver_priority(subgraph: &ReducedProjectSubgraphEntry) -> i32 {
    reduced_strong_driver_priority(subgraph)
        .or_else(|| subgraph.driver_connection.as_ref().map(|_| 1))
        .unwrap_or(0)
}

fn reduced_subgraph_driver_connection(
    subgraph: &ReducedProjectSubgraphEntry,
) -> ReducedProjectConnection {
    subgraph
        .driver_connection
        .clone()
        .unwrap_or_else(|| subgraph.resolved_connection.clone())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LiveReducedConnection {
    connection: ReducedProjectConnection,
}

impl LiveReducedConnection {
    fn new(connection: ReducedProjectConnection) -> Self {
        Self { connection }
    }

    // Upstream parity: reduced local analogue for `SCH_CONNECTION::Clone()`. This is not a 1:1
    // live KiCad connection because the Rust tree still wraps a reduced connection carrier rather
    // than mutating the real `SCH_CONNECTION` object graph, but it starts moving propagation onto
    // a dedicated live owner with in-place clone semantics instead of cloning reduced snapshots at
    // every caller. Remaining divergence is fuller member-pointer sharing and live item ownership.
    fn clone_from(&mut self, other: &LiveReducedConnection) {
        self.connection = other.connection.clone();
    }

    fn name(&self) -> &str {
        &self.connection.name
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LiveReducedSubgraph {
    source_index: usize,
    driver_connection: LiveReducedConnection,
    driver_priority: i32,
    driver_identity: Option<ReducedProjectDriverIdentity>,
    strong_driver_count: usize,
    sheet_instance_path: String,
    bus_neighbor_links: Vec<ReducedProjectBusNeighborLink>,
    bus_parent_links: Vec<ReducedProjectBusNeighborLink>,
    base_pin_count: usize,
    hier_parent_index: Option<usize>,
    hier_child_indexes: Vec<usize>,
    has_hier_pins: bool,
    has_hier_ports: bool,
    dirty: bool,
}

fn build_live_reduced_subgraphs(
    reduced_subgraphs: &[ReducedProjectSubgraphEntry],
) -> Vec<LiveReducedSubgraph> {
    reduced_subgraphs
        .iter()
        .enumerate()
        .map(|(index, subgraph)| LiveReducedSubgraph {
            source_index: index,
            driver_connection: LiveReducedConnection::new(reduced_subgraph_driver_connection(
                subgraph,
            )),
            driver_priority: reduced_subgraph_driver_priority(subgraph),
            driver_identity: subgraph.driver_identity.clone(),
            strong_driver_count: subgraph.drivers.len(),
            sheet_instance_path: subgraph.sheet_instance_path.clone(),
            bus_neighbor_links: subgraph.bus_neighbor_links.clone(),
            bus_parent_links: subgraph.bus_parent_links.clone(),
            base_pin_count: subgraph.base_pins.len(),
            hier_parent_index: subgraph.hier_parent_index,
            hier_child_indexes: subgraph.hier_child_indexes.clone(),
            has_hier_pins: !subgraph.hier_sheet_pins.is_empty(),
            has_hier_ports: !subgraph.hier_ports.is_empty(),
            dirty: true,
        })
        .collect()
}

fn apply_live_reduced_driver_connections(
    reduced_subgraphs: &mut [ReducedProjectSubgraphEntry],
    live_subgraphs: &[LiveReducedSubgraph],
) {
    for live in live_subgraphs {
        clone_reduced_connection_into_subgraph(
            &mut reduced_subgraphs[live.source_index],
            &live.driver_connection.connection,
        );
        reduced_subgraphs[live.source_index].bus_neighbor_links = live.bus_neighbor_links.clone();
        reduced_subgraphs[live.source_index].bus_parent_links = live.bus_parent_links.clone();
    }
}

fn build_live_reduced_name_caches(
    live_subgraphs: &[LiveReducedSubgraph],
) -> (
    BTreeMap<String, Vec<usize>>,
    BTreeMap<(String, String), Vec<usize>>,
) {
    let mut subgraphs_by_name = BTreeMap::<String, Vec<usize>>::new();
    let mut subgraphs_by_sheet_and_name = BTreeMap::<(String, String), Vec<usize>>::new();

    for (index, subgraph) in live_subgraphs.iter().enumerate() {
        let name = subgraph.driver_connection.connection.name.clone();
        subgraphs_by_name
            .entry(name.clone())
            .or_default()
            .push(index);

        if name.contains('[') {
            let prefix_only = format!("{}[]", name.split('[').next().unwrap_or(""));
            subgraphs_by_name
                .entry(prefix_only)
                .or_default()
                .push(index);
        }

        subgraphs_by_sheet_and_name
            .entry((subgraph.sheet_instance_path.clone(), name))
            .or_default()
            .push(index);
    }

    (subgraphs_by_name, subgraphs_by_sheet_and_name)
}

fn recache_live_reduced_subgraph_name(
    live_subgraphs: &[LiveReducedSubgraph],
    subgraphs_by_name: &mut BTreeMap<String, Vec<usize>>,
    subgraphs_by_sheet_and_name: &mut BTreeMap<(String, String), Vec<usize>>,
    subgraph_index: usize,
    old_name: &str,
) {
    if let Some(indexes) = subgraphs_by_name.get_mut(old_name) {
        indexes.retain(|index| *index != subgraph_index);
    }

    if old_name.contains('[') {
        let old_prefix_only = format!("{}[]", old_name.split('[').next().unwrap_or(""));
        if let Some(indexes) = subgraphs_by_name.get_mut(&old_prefix_only) {
            indexes.retain(|index| *index != subgraph_index);
        }
    }

    let sheet_key = (
        live_subgraphs[subgraph_index].sheet_instance_path.clone(),
        old_name.to_string(),
    );
    if let Some(indexes) = subgraphs_by_sheet_and_name.get_mut(&sheet_key) {
        indexes.retain(|index| *index != subgraph_index);
    }

    let new_name = live_subgraphs[subgraph_index]
        .driver_connection
        .connection
        .name
        .clone();
    subgraphs_by_name
        .entry(new_name.clone())
        .or_default()
        .push(subgraph_index);

    if new_name.contains('[') {
        let new_prefix_only = format!("{}[]", new_name.split('[').next().unwrap_or(""));
        subgraphs_by_name
            .entry(new_prefix_only)
            .or_default()
            .push(subgraph_index);
    }

    subgraphs_by_sheet_and_name
        .entry((
            live_subgraphs[subgraph_index].sheet_instance_path.clone(),
            new_name,
        ))
        .or_default()
        .push(subgraph_index);
}

fn reduced_connection_from_bus_member(
    member: &ReducedBusMember,
    sheet_instance_path: &str,
) -> ReducedProjectConnection {
    ReducedProjectConnection {
        net_code: member.net_code,
        connection_type: match member.kind {
            ReducedBusMemberKind::Net => ReducedProjectConnectionType::Net,
            ReducedBusMemberKind::Bus => ReducedProjectConnectionType::Bus,
        },
        name: member.full_local_name.clone(),
        local_name: member.local_name.clone(),
        full_local_name: member.full_local_name.clone(),
        sheet_instance_path: sheet_instance_path.to_string(),
        members: member.members.clone(),
    }
}

// Upstream parity: reduced local analogue for the `matchBusMember()`-driven member refresh KiCad
// performs after parent-bus propagation. This is not a 1:1 live graph update because the Rust tree
// still stores static reduced link snapshots instead of mutating live `SCH_CONNECTION` objects, but
// it now remaps stored bus parent/neighbor link members onto the parent's current reduced member
// tree so later consumers do not keep stale pre-remap member names forever. Remaining divergence is
// the still-missing in-place connection clone/recache cycle on the subgraphs themselves.
fn refresh_reduced_bus_link_members(reduced_subgraphs: &mut [ReducedProjectSubgraphEntry]) {
    let mut refreshed_parent_links =
        vec![Vec::<ReducedProjectBusNeighborLink>::new(); reduced_subgraphs.len()];

    for subgraph in reduced_subgraphs.iter() {
        for link in &subgraph.bus_parent_links {
            let refreshed_member = reduced_subgraphs
                .get(link.subgraph_index)
                .and_then(|parent| {
                    match_reduced_bus_member(&parent.resolved_connection.members, &link.member)
                })
                .cloned()
                .unwrap_or_else(|| link.member.clone());
            refreshed_parent_links[subgraph.subgraph_code - 1].push(
                ReducedProjectBusNeighborLink {
                    member: refreshed_member,
                    subgraph_index: link.subgraph_index,
                },
            );
        }
    }

    let mut refreshed_neighbor_links =
        vec![Vec::<ReducedProjectBusNeighborLink>::new(); reduced_subgraphs.len()];

    for (child_index, links) in refreshed_parent_links.iter().enumerate() {
        for link in links {
            if let Some(parent_links) = refreshed_neighbor_links.get_mut(link.subgraph_index) {
                parent_links.push(ReducedProjectBusNeighborLink {
                    member: link.member.clone(),
                    subgraph_index: child_index,
                });
            }
        }
    }

    for (index, subgraph) in reduced_subgraphs.iter_mut().enumerate() {
        subgraph.bus_parent_links = refreshed_parent_links[index].clone();
        subgraph.bus_parent_links.sort();
        subgraph.bus_parent_links.dedup();
        subgraph.bus_neighbor_links = refreshed_neighbor_links[index].clone();
        subgraph.bus_neighbor_links.sort();
        subgraph.bus_neighbor_links.dedup();
    }
}

// Upstream parity: reduced local analogue for the "multiple bus parents" rename/recache pass in
// `CONNECTION_GRAPH::Recalculate()`. This is not a 1:1 live graph mutation because the Rust tree
// still rewrites reduced subgraph snapshots instead of mutating real `SCH_CONNECTION` /
// `CONNECTION_SUBGRAPH` objects in place, but it now preserves the exercised static behavior:
// when a net subgraph with multiple bus parents wins a final name, parent bus members and any
// same-name subgraphs are updated to that propagated name before the final graph caches are built.
// Remaining divergence is the still-missing live stale-member refresh and pointer-based recache
// cycle on real connection objects.
fn refresh_reduced_multiple_bus_parent_names(
    reduced_subgraphs: &mut [ReducedProjectSubgraphEntry],
) {
    for subgraph_index in 0..reduced_subgraphs.len() {
        if reduced_subgraphs[subgraph_index].bus_parent_links.len() < 2 {
            continue;
        }

        let connection = reduced_subgraphs[subgraph_index]
            .driver_connection
            .clone()
            .unwrap_or_else(|| {
                reduced_subgraphs[subgraph_index]
                    .resolved_connection
                    .clone()
            });

        if connection.connection_type != ReducedProjectConnectionType::Net {
            continue;
        }

        let parent_links = reduced_subgraphs[subgraph_index].bus_parent_links.clone();

        for link in parent_links {
            let Some(parent) = reduced_subgraphs.get_mut(link.subgraph_index) else {
                continue;
            };

            let old_name = if let Some(member) =
                match_reduced_bus_member_mut(&mut parent.resolved_connection.members, &link.member)
            {
                if member.full_local_name == connection.full_local_name {
                    continue;
                }

                let old_name = member.full_local_name.clone();
                clone_reduced_connection_into_bus_member(member, &connection);
                old_name
            } else {
                continue;
            };

            if let Some(driver_connection) = &mut parent.driver_connection {
                if let Some(member) =
                    match_reduced_bus_member_mut(&mut driver_connection.members, &link.member)
                {
                    clone_reduced_connection_into_bus_member(member, &connection);
                }
            }

            for candidate in reduced_subgraphs.iter_mut() {
                if candidate.name == old_name {
                    clone_reduced_connection_into_subgraph(candidate, &connection);
                }
            }
        }
    }
}

// Upstream parity: reduced local analogue for the stale-member update KiCad performs inside
// `propagate_bus_neighbors()` after a child net finishes propagation. This is not a 1:1 live graph
// mutation because the Rust tree still rewrites reduced member/link snapshots instead of mutating
// real `SCH_CONNECTION` pointers in place, but it now refreshes each parent bus member from the
// child subgraph's final connection owner once that child net has settled. Remaining divergence is
// the still-missing recursive live dirty/repropagate cycle on real subgraph objects.
fn refresh_reduced_bus_members_from_neighbor_connections(
    reduced_subgraphs: &mut [ReducedProjectSubgraphEntry],
) {
    let mut refreshed_parent_links =
        vec![Vec::<ReducedProjectBusNeighborLink>::new(); reduced_subgraphs.len()];

    for child_index in 0..reduced_subgraphs.len() {
        let child_sheet_instance_path = reduced_subgraphs[child_index].sheet_instance_path.clone();
        let child_connection = reduced_subgraphs[child_index]
            .driver_connection
            .clone()
            .unwrap_or_else(|| reduced_subgraphs[child_index].resolved_connection.clone());

        if child_connection.connection_type != ReducedProjectConnectionType::Net {
            refreshed_parent_links[child_index] =
                reduced_subgraphs[child_index].bus_parent_links.clone();
            continue;
        }

        let existing_links = reduced_subgraphs[child_index].bus_parent_links.clone();

        for link in existing_links {
            let Some(parent) = reduced_subgraphs.get_mut(link.subgraph_index) else {
                continue;
            };

            if child_connection.sheet_instance_path != child_sheet_instance_path
                && child_connection.sheet_instance_path != parent.sheet_instance_path
            {
                refreshed_parent_links[child_index].push(link);
                continue;
            }

            let search = ReducedBusMember {
                net_code: 0,
                name: child_connection.local_name.clone(),
                local_name: child_connection.local_name.clone(),
                full_local_name: child_connection.full_local_name.clone(),
                vector_index: None,
                kind: ReducedBusMemberKind::Net,
                members: child_connection.members.clone(),
            };

            if let Some(found) =
                match_reduced_bus_member(&parent.resolved_connection.members, &search)
            {
                refreshed_parent_links[child_index].push(ReducedProjectBusNeighborLink {
                    member: found.clone(),
                    subgraph_index: link.subgraph_index,
                });
                continue;
            }

            let refreshed_member = if let Some(member) =
                match_reduced_bus_member_mut(&mut parent.resolved_connection.members, &link.member)
            {
                clone_reduced_connection_into_bus_member(member, &child_connection);
                member.clone()
            } else {
                link.member.clone()
            };

            if let Some(driver_connection) = &mut parent.driver_connection {
                if let Some(member) =
                    match_reduced_bus_member_mut(&mut driver_connection.members, &link.member)
                {
                    clone_reduced_connection_into_bus_member(member, &child_connection);
                }
            }

            refreshed_parent_links[child_index].push(ReducedProjectBusNeighborLink {
                member: refreshed_member,
                subgraph_index: link.subgraph_index,
            });
        }
    }

    for (child_index, subgraph) in reduced_subgraphs.iter_mut().enumerate() {
        if !refreshed_parent_links[child_index].is_empty() || subgraph.bus_parent_links.is_empty() {
            subgraph.bus_parent_links = refreshed_parent_links[child_index].clone();
            subgraph.bus_parent_links.sort();
            subgraph.bus_parent_links.dedup();
        }
    }
}

// Upstream parity: reduced local analogue for the repeated `propagateToNeighbors()` /
// stale-member refresh settling KiCad performs before item connections and caches are finalized.
// This is not a 1:1 live propagation loop because the Rust tree still converges cloned reduced
// subgraph snapshots instead of mutating real `CONNECTION_SUBGRAPH` / `SCH_CONNECTION` objects in
// place, but it now keeps running the reduced rename/member refresh passes until the shared graph
// stops changing instead of assuming one static pass is sufficient. Remaining divergence is the
// still-missing live dirty graph walk and pointer-based stale-member cache.
fn refresh_reduced_bus_propagation_fixpoint(reduced_subgraphs: &mut [ReducedProjectSubgraphEntry]) {
    let max_passes = reduced_subgraphs.len().saturating_add(1).max(1);

    for _ in 0..max_passes {
        let before = reduced_subgraphs.to_vec();

        refresh_reduced_bus_link_members(reduced_subgraphs);
        refresh_reduced_multiple_bus_parent_names(reduced_subgraphs);
        refresh_reduced_bus_members_from_neighbor_connections(reduced_subgraphs);
        refresh_reduced_bus_link_members(reduced_subgraphs);

        if *reduced_subgraphs == before {
            break;
        }
    }
}

// Upstream parity: reduced local analogue for the hierarchy-chain portion of
// `CONNECTION_GRAPH::propagateToNeighbors()`. This is no longer a purely static settle pass: it
// now walks a dedicated live reduced subgraph/connection layer with dirty-state and in-place
// clone semantics before projecting the chosen driver connection back onto the shared reduced
// graph. Remaining divergence is that the current live layer still covers only the hierarchy-chain
// slice and does not yet include KiCad's bus-neighbor recursion, stale bus-member replay, or live
// item-owned connection updates on the same objects.
fn propagate_reduced_live_hierarchy_chain(
    start: usize,
    live_subgraphs: &mut [LiveReducedSubgraph],
    force: bool,
) {
    if !force && live_subgraphs[start].has_hier_ports && live_subgraphs[start].has_hier_pins {
        return;
    } else if !live_subgraphs[start].has_hier_ports && !live_subgraphs[start].has_hier_pins {
        live_subgraphs[start].dirty = false;
        return;
    }

    let mut stack = vec![start];
    let mut visited = Vec::<usize>::new();
    let mut visited_set = BTreeSet::<usize>::new();

    visited_set.insert(start);

    while let Some(index) = stack.pop() {
        visited.push(index);

        if let Some(parent_index) = live_subgraphs[index].hier_parent_index {
            if visited_set.insert(parent_index) {
                stack.push(parent_index);
            }
        }

        let child_indexes = live_subgraphs[index].hier_child_indexes.clone();

        for child_index in child_indexes {
            if visited_set.insert(child_index) {
                stack.push(child_index);
            }
        }
    }

    let mut best_index = start;
    let mut highest = live_subgraphs[best_index].driver_priority;
    let mut best_is_strong = highest >= 3;
    let mut best_name = live_subgraphs[best_index]
        .driver_connection
        .name()
        .to_string();

    if highest < 6 {
        for &index in visited.iter().filter(|index| **index != start) {
            let priority = live_subgraphs[index].driver_priority;
            let candidate_strong = priority >= 3;
            let candidate_name = live_subgraphs[index].driver_connection.name();
            let shorter_path = reduced_sheet_path_depth(&live_subgraphs[index].sheet_instance_path)
                < reduced_sheet_path_depth(&live_subgraphs[best_index].sheet_instance_path);
            let as_good_path = reduced_sheet_path_depth(&live_subgraphs[index].sheet_instance_path)
                <= reduced_sheet_path_depth(&live_subgraphs[best_index].sheet_instance_path);

            if (priority >= 6)
                || (!best_is_strong && candidate_strong)
                || (priority > highest && candidate_strong)
                || (priority == highest && candidate_strong && shorter_path)
                || ((best_is_strong == candidate_strong)
                    && as_good_path
                    && (priority == highest)
                    && (candidate_name < best_name.as_str()))
            {
                best_index = index;
                highest = priority;
                best_is_strong = candidate_strong;
                best_name = candidate_name.to_string();
            }
        }
    }

    let chosen_connection = live_subgraphs[best_index].driver_connection.clone();

    for index in visited {
        live_subgraphs[index]
            .driver_connection
            .clone_from(&chosen_connection);
        live_subgraphs[index].dirty = false;
    }
}

fn refresh_reduced_hierarchy_driver_chains_on_live_subgraphs(
    live_subgraphs: &mut [LiveReducedSubgraph],
) {
    for start in 0..live_subgraphs.len() {
        if !live_subgraphs[start].dirty {
            continue;
        }

        let has_hierarchy_links = live_subgraphs[start].hier_parent_index.is_some()
            || !live_subgraphs[start].hier_child_indexes.is_empty();

        if !has_hierarchy_links {
            live_subgraphs[start].dirty = false;
            continue;
        }

        propagate_reduced_live_hierarchy_chain(start, live_subgraphs, false);
    }

    for start in 0..live_subgraphs.len() {
        if live_subgraphs[start].dirty {
            propagate_reduced_live_hierarchy_chain(start, live_subgraphs, true);
        }
    }
}

fn refresh_reduced_hierarchy_driver_chains(reduced_subgraphs: &mut [ReducedProjectSubgraphEntry]) {
    let mut live_subgraphs = build_live_reduced_subgraphs(reduced_subgraphs);
    refresh_reduced_hierarchy_driver_chains_on_live_subgraphs(&mut live_subgraphs);
    apply_live_reduced_driver_connections(reduced_subgraphs, &live_subgraphs);
}

// Upstream parity: reduced local analogue for the bus-neighbor branch inside
// `CONNECTION_GRAPH::propagateToNeighbors()`. This is still not a 1:1 live KiCad graph walk
// because the Rust tree does not yet recurse stale members and item-owned connections on the same
// live objects, but it now mutates chosen bus-member and neighbor driver connections on a shared
// live subgraph owner before projecting them back onto the reduced graph. Remaining divergence is
// the later stale-member replay / recache recursion that still falls back to the reduced fixpoint.
fn refresh_reduced_live_bus_neighbor_drivers_on_live_subgraphs(
    live_subgraphs: &mut [LiveReducedSubgraph],
) {
    for parent_index in 0..live_subgraphs.len() {
        if !matches!(
            live_subgraphs[parent_index]
                .driver_connection
                .connection
                .connection_type,
            ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
        ) {
            continue;
        }

        let mut sorted_links = live_subgraphs[parent_index].bus_neighbor_links.clone();
        sorted_links.sort_by(|left, right| {
            left.member
                .name
                .cmp(&right.member.name)
                .then(left.subgraph_index.cmp(&right.subgraph_index))
        });

        for link_index in 0..sorted_links.len() {
            let link = sorted_links[link_index].clone();
            let Some(parent_member) = match_reduced_bus_member(
                &live_subgraphs[parent_index]
                    .driver_connection
                    .connection
                    .members,
                &link.member,
            )
            .cloned() else {
                continue;
            };

            let neighbor_index = link.subgraph_index;
            let neighbor_name = live_subgraphs[neighbor_index]
                .driver_connection
                .connection
                .name
                .clone();

            if neighbor_name == parent_member.full_local_name {
                continue;
            }

            let parent_sheet_instance_path =
                live_subgraphs[parent_index].sheet_instance_path.clone();
            let neighbor_sheet_instance_path =
                live_subgraphs[neighbor_index].sheet_instance_path.clone();
            let neighbor_connection_sheet = live_subgraphs[neighbor_index]
                .driver_connection
                .connection
                .sheet_instance_path
                .clone();

            if neighbor_connection_sheet != neighbor_sheet_instance_path {
                if neighbor_connection_sheet != parent_sheet_instance_path {
                    continue;
                }

                let search = ReducedBusMember {
                    net_code: 0,
                    name: live_subgraphs[neighbor_index]
                        .driver_connection
                        .connection
                        .local_name
                        .clone(),
                    local_name: live_subgraphs[neighbor_index]
                        .driver_connection
                        .connection
                        .local_name
                        .clone(),
                    full_local_name: live_subgraphs[neighbor_index]
                        .driver_connection
                        .connection
                        .full_local_name
                        .clone(),
                    vector_index: None,
                    kind: match live_subgraphs[neighbor_index]
                        .driver_connection
                        .connection
                        .connection_type
                    {
                        ReducedProjectConnectionType::Bus
                        | ReducedProjectConnectionType::BusGroup => ReducedBusMemberKind::Bus,
                        _ => ReducedBusMemberKind::Net,
                    },
                    members: live_subgraphs[neighbor_index]
                        .driver_connection
                        .connection
                        .members
                        .clone(),
                };

                if match_reduced_bus_member(
                    &live_subgraphs[parent_index]
                        .driver_connection
                        .connection
                        .members,
                    &search,
                )
                .is_some()
                {
                    continue;
                }
            }

            if live_subgraphs[neighbor_index].driver_priority >= 6 {
                let promoted = live_subgraphs[neighbor_index]
                    .driver_connection
                    .connection
                    .clone();
                let old_member = link.member.clone();
                if let Some(member) = match_reduced_bus_member_mut(
                    &mut live_subgraphs[parent_index]
                        .driver_connection
                        .connection
                        .members,
                    &link.member,
                ) {
                    clone_reduced_connection_into_bus_member(member, &promoted);
                    let refreshed_member = member.clone();

                    for candidate_link in &mut sorted_links {
                        if candidate_link.member == old_member {
                            candidate_link.member = refreshed_member.clone();
                        }
                    }

                    for candidate_link in &mut live_subgraphs[parent_index].bus_neighbor_links {
                        if candidate_link.member == old_member {
                            candidate_link.member = refreshed_member.clone();
                        }
                    }

                    for candidate_link in &mut live_subgraphs[parent_index].bus_parent_links {
                        if candidate_link.member == old_member {
                            candidate_link.member = refreshed_member.clone();
                        }
                    }
                }
                continue;
            }

            live_subgraphs[neighbor_index].driver_connection =
                LiveReducedConnection::new(reduced_connection_from_bus_member(
                    &parent_member,
                    &live_subgraphs[neighbor_index].sheet_instance_path,
                ));
        }
    }
}

fn refresh_reduced_live_bus_neighbor_drivers(
    reduced_subgraphs: &mut [ReducedProjectSubgraphEntry],
) {
    let mut live_subgraphs = build_live_reduced_subgraphs(reduced_subgraphs);
    refresh_reduced_live_bus_neighbor_drivers_on_live_subgraphs(&mut live_subgraphs);
    apply_live_reduced_driver_connections(reduced_subgraphs, &live_subgraphs);
}

// Upstream parity: reduced local analogue for the stale-member update KiCad performs after a bus
// neighbor or hierarchy child settles on a final net connection. This still stops short of the
// full live `stale_bus_members` replay because it does not recursively revisit every affected bus
// subgraph on the same object graph, but it does move the direct child-net -> parent-bus member
// mutation onto the shared live subgraph owner before the reduced cleanup passes.
fn refresh_reduced_live_bus_parent_members_on_live_subgraphs(
    live_subgraphs: &mut [LiveReducedSubgraph],
) {
    for child_index in 0..live_subgraphs.len() {
        let child_connection = live_subgraphs[child_index]
            .driver_connection
            .connection
            .clone();

        if child_connection.connection_type != ReducedProjectConnectionType::Net {
            continue;
        }

        let child_sheet_instance_path = live_subgraphs[child_index].sheet_instance_path.clone();
        let parent_links = live_subgraphs[child_index].bus_parent_links.clone();

        for link in parent_links {
            let parent_index = link.subgraph_index;
            let parent_sheet_instance_path =
                live_subgraphs[parent_index].sheet_instance_path.clone();

            if child_connection.sheet_instance_path != child_sheet_instance_path
                && child_connection.sheet_instance_path != parent_sheet_instance_path
            {
                continue;
            }

            let search = ReducedBusMember {
                net_code: 0,
                name: child_connection.local_name.clone(),
                local_name: child_connection.local_name.clone(),
                full_local_name: child_connection.full_local_name.clone(),
                vector_index: None,
                kind: ReducedBusMemberKind::Net,
                members: child_connection.members.clone(),
            };

            if match_reduced_bus_member(
                &live_subgraphs[parent_index]
                    .driver_connection
                    .connection
                    .members,
                &search,
            )
            .is_some()
            {
                continue;
            }

            if let Some(member) = match_reduced_bus_member_mut(
                &mut live_subgraphs[parent_index]
                    .driver_connection
                    .connection
                    .members,
                &link.member,
            ) {
                clone_reduced_connection_into_bus_member(member, &child_connection);
            }
        }
    }
}

fn refresh_reduced_live_bus_parent_members(reduced_subgraphs: &mut [ReducedProjectSubgraphEntry]) {
    let mut live_subgraphs = build_live_reduced_subgraphs(reduced_subgraphs);
    refresh_reduced_live_bus_parent_members_on_live_subgraphs(&mut live_subgraphs);
    apply_live_reduced_driver_connections(reduced_subgraphs, &live_subgraphs);
}

// Upstream parity: reduced local analogue for the multiple-parent rename/recache branch KiCad
// runs before the final graph caches are rebuilt. This still projects back onto the reduced graph
// instead of mutating live name indexes in place, but it moves the parent-member clone and
// same-name subgraph rename onto the shared live subgraph owner before the reduced cache rebuild.
fn refresh_reduced_live_multiple_bus_parent_names_on_live_subgraphs(
    live_subgraphs: &mut [LiveReducedSubgraph],
) {
    let (mut subgraphs_by_name, mut subgraphs_by_sheet_and_name) =
        build_live_reduced_name_caches(&live_subgraphs);

    for subgraph_index in 0..live_subgraphs.len() {
        if live_subgraphs[subgraph_index].bus_parent_links.len() < 2 {
            continue;
        }

        let connection = live_subgraphs[subgraph_index]
            .driver_connection
            .connection
            .clone();

        if connection.connection_type != ReducedProjectConnectionType::Net {
            continue;
        }

        let parent_links = live_subgraphs[subgraph_index].bus_parent_links.clone();

        for link in parent_links {
            let old_name = {
                let Some(member) = match_reduced_bus_member_mut(
                    &mut live_subgraphs[link.subgraph_index]
                        .driver_connection
                        .connection
                        .members,
                    &link.member,
                ) else {
                    continue;
                };

                if member.full_local_name == connection.full_local_name {
                    continue;
                }

                let old_name = member.full_local_name.clone();
                clone_reduced_connection_into_bus_member(member, &connection);
                old_name
            };

            let candidate_indexes = subgraphs_by_name
                .get(&old_name)
                .cloned()
                .unwrap_or_default();

            for candidate_index in candidate_indexes {
                let old_candidate_name = live_subgraphs[candidate_index]
                    .driver_connection
                    .connection
                    .name
                    .clone();
                if old_candidate_name == old_name {
                    live_subgraphs[candidate_index].driver_connection.connection =
                        connection.clone();
                    recache_live_reduced_subgraph_name(
                        &live_subgraphs,
                        &mut subgraphs_by_name,
                        &mut subgraphs_by_sheet_and_name,
                        candidate_index,
                        &old_candidate_name,
                    );
                }
            }
        }
    }
}

fn refresh_reduced_live_multiple_bus_parent_names(
    reduced_subgraphs: &mut [ReducedProjectSubgraphEntry],
) {
    let mut live_subgraphs = build_live_reduced_subgraphs(reduced_subgraphs);
    refresh_reduced_live_multiple_bus_parent_names_on_live_subgraphs(&mut live_subgraphs);
    apply_live_reduced_driver_connections(reduced_subgraphs, &live_subgraphs);
}

// Upstream parity: reduced local analogue for the post-remap bus-link refresh KiCad gets from
// matching live bus members on the same propagated subgraph objects. This still projects the
// remapped links back onto reduced vectors, but it moves the member rematch step onto the shared
// live driver connections before the reduced cache rebuild.
fn refresh_reduced_live_bus_link_members_on_live_subgraphs(
    live_subgraphs: &mut [LiveReducedSubgraph],
) {
    let mut refreshed_parent_links =
        vec![Vec::<ReducedProjectBusNeighborLink>::new(); live_subgraphs.len()];

    for child_index in 0..live_subgraphs.len() {
        let parent_links = live_subgraphs[child_index].bus_parent_links.clone();

        for link in parent_links {
            let refreshed_member = match_reduced_bus_member(
                &live_subgraphs[link.subgraph_index]
                    .driver_connection
                    .connection
                    .members,
                &link.member,
            )
            .cloned()
            .unwrap_or(link.member);

            refreshed_parent_links[child_index].push(ReducedProjectBusNeighborLink {
                member: refreshed_member,
                subgraph_index: link.subgraph_index,
            });
        }
    }

    let mut refreshed_neighbor_links =
        vec![Vec::<ReducedProjectBusNeighborLink>::new(); live_subgraphs.len()];

    for (child_index, links) in refreshed_parent_links.iter().enumerate() {
        for link in links {
            refreshed_neighbor_links[link.subgraph_index].push(ReducedProjectBusNeighborLink {
                member: link.member.clone(),
                subgraph_index: child_index,
            });
        }
    }

    for (index, live) in live_subgraphs.iter_mut().enumerate() {
        live.bus_parent_links = refreshed_parent_links[index].clone();
        live.bus_parent_links.sort();
        live.bus_parent_links.dedup();
        live.bus_neighbor_links = refreshed_neighbor_links[index].clone();
        live.bus_neighbor_links.sort();
        live.bus_neighbor_links.dedup();
    }
}

fn refresh_reduced_live_bus_link_members(reduced_subgraphs: &mut [ReducedProjectSubgraphEntry]) {
    let mut live_subgraphs = build_live_reduced_subgraphs(reduced_subgraphs);
    refresh_reduced_live_bus_link_members_on_live_subgraphs(&mut live_subgraphs);
    apply_live_reduced_driver_connections(reduced_subgraphs, &live_subgraphs);
}

// Upstream parity: reduced local analogue for the repeated bus-neighbor dirty propagation KiCad
// gets by revisiting the same live subgraphs until bus-driven renames stop changing. This is still
// not the final `m_dirty` / recursive `propagateToNeighbors()` engine, but it moves the repeat
// loop itself onto the live bus-owner slices instead of leaving all multi-step bus propagation to
// the reduced snapshot fixpoint.
fn refresh_reduced_live_bus_propagation_fixpoint(
    reduced_subgraphs: &mut [ReducedProjectSubgraphEntry],
) {
    let max_passes = reduced_subgraphs.len().saturating_add(1).max(1);
    let mut live_subgraphs = build_live_reduced_subgraphs(reduced_subgraphs);

    for _ in 0..max_passes {
        let before = live_subgraphs.clone();

        refresh_reduced_live_bus_neighbor_drivers_on_live_subgraphs(&mut live_subgraphs);
        refresh_reduced_live_bus_parent_members_on_live_subgraphs(&mut live_subgraphs);
        refresh_reduced_live_multiple_bus_parent_names_on_live_subgraphs(&mut live_subgraphs);
        refresh_reduced_live_bus_link_members_on_live_subgraphs(&mut live_subgraphs);

        if live_subgraphs == before {
            break;
        }
    }

    apply_live_reduced_driver_connections(reduced_subgraphs, &live_subgraphs);
}

// Upstream parity: reduced local bridge toward one live `propagateToNeighbors()` owner during
// graph build. This is still not the full KiCad visited-set recursion because it keeps the
// hierarchy-chain walk and bus follow-up as separate local steps, but those steps now mutate one
// shared live subgraph set together with post-propagation item updates before the reduced graph is
// projected back. Remaining divergence is the still-missing single visited/stale-member walk.
fn refresh_reduced_live_graph_propagation(reduced_subgraphs: &mut [ReducedProjectSubgraphEntry]) {
    let mut live_subgraphs = build_live_reduced_subgraphs(reduced_subgraphs);
    refresh_reduced_hierarchy_driver_chains_on_live_subgraphs(&mut live_subgraphs);

    let max_passes = reduced_subgraphs.len().saturating_add(1).max(1);

    for _ in 0..max_passes {
        let before = live_subgraphs.clone();

        refresh_reduced_live_bus_neighbor_drivers_on_live_subgraphs(&mut live_subgraphs);
        refresh_reduced_live_bus_parent_members_on_live_subgraphs(&mut live_subgraphs);
        refresh_reduced_live_multiple_bus_parent_names_on_live_subgraphs(&mut live_subgraphs);
        refresh_reduced_live_bus_link_members_on_live_subgraphs(&mut live_subgraphs);

        if live_subgraphs == before {
            break;
        }
    }

    refresh_reduced_live_post_propagation_item_connections_on_live_subgraphs(&mut live_subgraphs);
    apply_live_reduced_driver_connections(reduced_subgraphs, &live_subgraphs);
}

// Upstream parity: reduced local analogue for the post-propagation item-connection update KiCad
// performs after subgraph names settle. This still projects back onto reduced subgraph snapshots
// instead of mutating live item-owned `SCH_CONNECTION` objects, but it moves the exercised
// `UpdateItemConnections()`-visible branches onto the shared live subgraph owner before the final
// reduced fallback pass.
fn refresh_reduced_live_post_propagation_item_connections(
    reduced_subgraphs: &mut [ReducedProjectSubgraphEntry],
) {
    let mut live_subgraphs = build_live_reduced_subgraphs(reduced_subgraphs);
    refresh_reduced_live_post_propagation_item_connections_on_live_subgraphs(&mut live_subgraphs);
    apply_live_reduced_driver_connections(reduced_subgraphs, &live_subgraphs);
}

fn refresh_reduced_live_post_propagation_item_connections_on_live_subgraphs(
    live_subgraphs: &mut [LiveReducedSubgraph],
) {
    for index in 0..live_subgraphs.len() {
        if live_subgraphs[index].strong_driver_count == 0
            && live_subgraphs[index].base_pin_count == 1
            && matches!(
                live_subgraphs[index].driver_identity,
                Some(ReducedProjectDriverIdentity::SymbolPin { .. })
            )
            && live_subgraphs[index]
                .driver_connection
                .connection
                .name
                .contains("Net-(")
        {
            let connection = &mut live_subgraphs[index].driver_connection.connection;
            connection.name = reduced_force_no_connect_net_name(&connection.name);
            connection.local_name = reduced_force_no_connect_net_name(&connection.local_name);
            connection.full_local_name =
                reduced_force_no_connect_net_name(&connection.full_local_name);
        }

        if matches!(
            live_subgraphs[index].driver_identity,
            Some(ReducedProjectDriverIdentity::SheetPin { .. })
        ) && matches!(
            live_subgraphs[index]
                .driver_connection
                .connection
                .connection_type,
            ReducedProjectConnectionType::Net
        ) {
            if let Some((connection_type, members)) = live_subgraphs[index]
                .hier_child_indexes
                .iter()
                .find_map(|child_index| {
                    let child_connection =
                        &live_subgraphs[*child_index].driver_connection.connection;

                    matches!(
                        child_connection.connection_type,
                        ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
                    )
                    .then_some((
                        child_connection.connection_type,
                        child_connection.members.clone(),
                    ))
                })
            {
                live_subgraphs[index]
                    .driver_connection
                    .connection
                    .connection_type = connection_type;
                live_subgraphs[index].driver_connection.connection.members = members;
            }
        }
    }
}

// Upstream parity: reduced local analogue for the global-secondary-driver promotion branch inside
// `CONNECTION_GRAPH::buildConnectionGraph()`. This is not a 1:1 live propagation because the Rust
// tree still rewrites reduced subgraph snapshots instead of mutating live dirty
// `CONNECTION_SUBGRAPH` objects before calling `propagateToNeighbors()`, but it preserves the
// exercised static behavior:
// - only non-local chosen drivers are eligible
// - only subgraphs with more than one strong driver are considered
// - non-chosen global secondary drivers can promote matching global subgraphs on any sheet
// - non-chosen local/power secondary drivers only promote matching global subgraphs on the same
//   sheet
// Remaining divergence is the still-missing live recursive propagation immediately after each
// promotion on the actual visited subgraph objects.
fn refresh_reduced_global_secondary_driver_promotions(
    reduced_subgraphs: &mut [ReducedProjectSubgraphEntry],
) {
    let global_indexes = reduced_subgraphs
        .iter()
        .enumerate()
        .filter(|(_, subgraph)| reduced_subgraph_driver_priority(subgraph) >= 6)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();

    for subgraph_index in 0..reduced_subgraphs.len() {
        let chosen_connection = reduced_subgraphs[subgraph_index]
            .driver_connection
            .clone()
            .unwrap_or_else(|| {
                reduced_subgraphs[subgraph_index]
                    .resolved_connection
                    .clone()
            });
        let chosen_priority = reduced_subgraph_driver_priority(&reduced_subgraphs[subgraph_index]);

        if chosen_priority < 6 || reduced_subgraphs[subgraph_index].drivers.len() < 2 {
            continue;
        }

        for secondary_driver in reduced_subgraphs[subgraph_index].drivers.clone() {
            if secondary_driver.full_name == chosen_connection.name {
                continue;
            }

            let secondary_is_global = secondary_driver.priority >= 6;

            for &candidate_index in &global_indexes {
                if candidate_index == subgraph_index {
                    continue;
                }

                if !secondary_is_global
                    && reduced_subgraphs[candidate_index].sheet_instance_path
                        != reduced_subgraphs[subgraph_index].sheet_instance_path
                {
                    continue;
                }

                if !reduced_subgraphs[candidate_index]
                    .drivers
                    .iter()
                    .any(|candidate_driver| {
                        candidate_driver.full_name == secondary_driver.full_name
                    })
                {
                    continue;
                }

                clone_reduced_connection_into_subgraph(
                    &mut reduced_subgraphs[candidate_index],
                    &chosen_connection,
                );
            }
        }
    }
}

// Upstream parity: reduced local analogue for the bus-entry connected-bus item ownership KiCad
// records during `updateItemConnectivity()`. This is not a 1:1 item-pointer owner because the Rust
// tree still resolves bus-entry attachment onto reduced subgraph indexes instead of live
// `SCH_BUS_WIRE_ENTRY` / `SCH_LINE*` pointers, but it preserves the shared graph boundary where a
// bus entry knows which bus item it is graphically attached to instead of making ERC infer that
// from the containing subgraph alone. Remaining divergence is the still-missing live item pointer
// ownership and connection-map updates on schematic items.
fn attach_reduced_connected_bus_items(reduced_subgraphs: &mut [ReducedProjectSubgraphEntry]) {
    let bus_subgraphs = reduced_subgraphs
        .iter()
        .enumerate()
        .filter(|(_, subgraph)| !subgraph.bus_items.is_empty())
        .map(|(index, subgraph)| {
            (
                index,
                subgraph.sheet_instance_path.clone(),
                subgraph.bus_items.clone(),
            )
        })
        .collect::<Vec<_>>();

    for subgraph in reduced_subgraphs.iter_mut() {
        for item in &mut subgraph.wire_items {
            if !item.is_bus_entry {
                continue;
            }

            item.connected_bus_subgraph_index = bus_subgraphs
                .iter()
                .find(|(_, sheet_instance_path, bus_items)| {
                    *sheet_instance_path == subgraph.sheet_instance_path
                        && bus_items.iter().any(|bus_item| {
                            point_on_wire_segment(
                                [f64::from_bits(item.start.0), f64::from_bits(item.start.1)],
                                [
                                    f64::from_bits(bus_item.start.0),
                                    f64::from_bits(bus_item.start.1),
                                ],
                                [
                                    f64::from_bits(bus_item.end.0),
                                    f64::from_bits(bus_item.end.1),
                                ],
                            ) || point_on_wire_segment(
                                [f64::from_bits(item.end.0), f64::from_bits(item.end.1)],
                                [
                                    f64::from_bits(bus_item.start.0),
                                    f64::from_bits(bus_item.start.1),
                                ],
                                [
                                    f64::from_bits(bus_item.end.0),
                                    f64::from_bits(bus_item.end.1),
                                ],
                            )
                        })
                })
                .map(|(index, _, _)| *index);
        }
    }
}

// Upstream parity: reduced local analogue for the shared graph-name recache KiCad performs through
// `recacheSubgraphName()` plus later netcode assignment. This is not a 1:1 cache owner because
// the Rust tree still rebuilds reduced lookup maps from snapshots instead of mutating live graph
// maps as names change, but it keeps the final shared `(name, sheet+name)` indexes and first-seen
// net codes aligned with the post-propagation reduced subgraph names instead of stale pre-rename
// values. Remaining divergence is the still-missing live cache mutation on real subgraph objects.
fn rebuild_reduced_project_graph_name_caches(
    reduced_subgraphs: &mut [ReducedProjectSubgraphEntry],
) -> (
    BTreeMap<String, Vec<usize>>,
    BTreeMap<(String, String), Vec<usize>>,
) {
    let mut net_codes = BTreeMap::<String, usize>::new();
    let mut subgraphs_by_name = BTreeMap::<String, Vec<usize>>::new();
    let mut subgraphs_by_sheet_and_name = BTreeMap::<(String, String), Vec<usize>>::new();

    for (index, subgraph) in reduced_subgraphs.iter_mut().enumerate() {
        if !subgraph.name.is_empty() {
            let next_code = net_codes.len() + 1;
            let code = *net_codes.entry(subgraph.name.clone()).or_insert(next_code);
            subgraph.code = code;
            assign_reduced_connection_net_codes(&mut subgraph.resolved_connection, &mut net_codes);

            if let Some(driver_connection) = &mut subgraph.driver_connection {
                assign_reduced_connection_net_codes(driver_connection, &mut net_codes);
            }

            for link in &mut subgraph.label_links {
                assign_reduced_connection_net_codes(&mut link.connection, &mut net_codes);
            }

            for pin in &mut subgraph.hier_sheet_pins {
                assign_reduced_connection_net_codes(&mut pin.connection, &mut net_codes);
            }

            for port in &mut subgraph.hier_ports {
                assign_reduced_connection_net_codes(&mut port.connection, &mut net_codes);
            }
            subgraphs_by_name
                .entry(subgraph.name.clone())
                .or_default()
                .push(index);
            if subgraph.name.contains('[') {
                let prefix_only = format!("{}[]", subgraph.name.split('[').next().unwrap_or(""));
                subgraphs_by_name
                    .entry(prefix_only)
                    .or_default()
                    .push(index);
            }
            subgraphs_by_sheet_and_name
                .entry((subgraph.sheet_instance_path.clone(), subgraph.name.clone()))
                .or_default()
                .push(index);
        } else {
            subgraph.code = 0;
        }
    }

    (subgraphs_by_name, subgraphs_by_sheet_and_name)
}

fn make_reduced_bus_member(
    text: &str,
    local_prefix: &str,
    sheet_prefix: &str,
    vector_index: Option<usize>,
    kind: ReducedBusMemberKind,
    members: Vec<ReducedBusMember>,
) -> ReducedBusMember {
    let local_name = format!("{local_prefix}{text}");

    ReducedBusMember {
        name: text.to_string(),
        net_code: 0,
        local_name: local_name.clone(),
        full_local_name: format!("{sheet_prefix}{local_name}"),
        vector_index,
        kind,
        members,
    }
}

fn parse_reduced_bus_member_object(
    schematic: &Schematic,
    text: &str,
    local_prefix: &str,
    sheet_prefix: &str,
    active_aliases: &mut BTreeSet<String>,
) -> ReducedBusMember {
    if let Some(members) = reduced_bus_vector_members(text) {
        return make_reduced_bus_member(
            text,
            local_prefix,
            sheet_prefix,
            None,
            ReducedBusMemberKind::Bus,
            members
                .into_iter()
                .enumerate()
                .map(|(index, member)| {
                    make_reduced_bus_member(
                        &member,
                        local_prefix,
                        sheet_prefix,
                        Some(index),
                        ReducedBusMemberKind::Net,
                        Vec::new(),
                    )
                })
                .collect(),
        );
    }

    if let Some(inner) = text
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
    {
        let members = split_reduced_bus_group_members(inner)
            .into_iter()
            .filter(|member| !member.is_empty())
            .flat_map(|member| {
                expand_reduced_bus_member_entry(
                    schematic,
                    &member,
                    local_prefix,
                    sheet_prefix,
                    active_aliases,
                )
            })
            .collect::<Vec<_>>();

        return make_reduced_bus_member(
            text,
            local_prefix,
            sheet_prefix,
            None,
            ReducedBusMemberKind::Bus,
            members,
        );
    }

    if let Some((prefix, suffix)) = text.split_once('{')
        && let Some(inner) = suffix.strip_suffix('}')
    {
        let child_prefix = if prefix.is_empty() {
            local_prefix.to_string()
        } else {
            format!("{local_prefix}{prefix}.")
        };
        let members = split_reduced_bus_group_members(inner)
            .into_iter()
            .filter(|member| !member.is_empty())
            .flat_map(|member| {
                expand_reduced_bus_member_entry(
                    schematic,
                    &member,
                    &child_prefix,
                    sheet_prefix,
                    active_aliases,
                )
            })
            .collect::<Vec<_>>();

        return make_reduced_bus_member(
            text,
            local_prefix,
            sheet_prefix,
            None,
            ReducedBusMemberKind::Bus,
            members,
        );
    }

    make_reduced_bus_member(
        text,
        local_prefix,
        sheet_prefix,
        None,
        ReducedBusMemberKind::Net,
        Vec::new(),
    )
}

fn expand_reduced_bus_member_entry(
    schematic: &Schematic,
    text: &str,
    local_prefix: &str,
    sheet_prefix: &str,
    active_aliases: &mut BTreeSet<String>,
) -> Vec<ReducedBusMember> {
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
                expand_reduced_bus_member_entry(
                    schematic,
                    member,
                    local_prefix,
                    sheet_prefix,
                    active_aliases,
                )
            })
            .collect::<Vec<_>>();

        active_aliases.remove(&alias_key);
        return members;
    }

    if reduced_text_is_bus(schematic, text) {
        return vec![parse_reduced_bus_member_object(
            schematic,
            text,
            local_prefix,
            sheet_prefix,
            active_aliases,
        )];
    }

    vec![make_reduced_bus_member(
        text,
        local_prefix,
        sheet_prefix,
        None,
        ReducedBusMemberKind::Net,
        Vec::new(),
    )]
}

fn collect_reduced_bus_member_objects_inner(
    schematic: &Schematic,
    text: &str,
    local_prefix: &str,
    sheet_prefix: &str,
    active_aliases: &mut BTreeSet<String>,
) -> Vec<ReducedBusMember> {
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
                expand_reduced_bus_member_entry(
                    schematic,
                    member,
                    local_prefix,
                    sheet_prefix,
                    active_aliases,
                )
            })
            .collect::<Vec<_>>();

        active_aliases.remove(&alias_key);
        return members;
    }

    if let Some(members) = reduced_bus_vector_members(text) {
        return members
            .into_iter()
            .enumerate()
            .map(|(index, member)| {
                make_reduced_bus_member(
                    &member,
                    local_prefix,
                    sheet_prefix,
                    Some(index),
                    ReducedBusMemberKind::Net,
                    Vec::new(),
                )
            })
            .collect();
    }

    if let Some(inner) = text
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
    {
        return split_reduced_bus_group_members(inner)
            .into_iter()
            .filter(|member| !member.is_empty())
            .flat_map(|member| {
                expand_reduced_bus_member_entry(
                    schematic,
                    &member,
                    local_prefix,
                    sheet_prefix,
                    active_aliases,
                )
            })
            .collect();
    }

    if let Some((prefix, suffix)) = text.split_once('{')
        && let Some(inner) = suffix.strip_suffix('}')
    {
        let child_prefix = if prefix.is_empty() {
            local_prefix.to_string()
        } else {
            format!("{local_prefix}{prefix}.")
        };

        return split_reduced_bus_group_members(inner)
            .into_iter()
            .filter(|member| !member.is_empty())
            .flat_map(|member| {
                expand_reduced_bus_member_entry(
                    schematic,
                    &member,
                    &child_prefix,
                    sheet_prefix,
                    active_aliases,
                )
            })
            .collect();
    }

    expand_reduced_bus_member_entry(schematic, text, local_prefix, sheet_prefix, active_aliases)
}

// Upstream parity: reduced local analogue for the direct child objects KiCad exposes through
// `SCH_CONNECTION::Members()` after `ConfigureFromLabel()`. This is still narrower than a real
// `SCH_CONNECTION` tree because the Rust path builds from raw text plus aliases instead of cloned
// connection objects, but the shared graph owner now preserves member kind plus local/full-local
// naming instead of collapsing immediately to flat strings. Remaining divergence is fuller resolved
// `Name()` / `Clone()` ownership beyond this reduced member tree.
#[cfg(test)]
pub(crate) fn reduced_bus_member_objects(
    schematic: &Schematic,
    text: &str,
) -> Vec<ReducedBusMember> {
    collect_reduced_bus_member_objects_inner(schematic, text, "", "", &mut BTreeSet::new())
}

// Upstream parity: reduced local analogue for the member expansion KiCad exposes through
// `SCH_CONNECTION::Members()` after `ConfigureFromLabel()`. This is not a 1:1 member-object walk
// because the Rust tree still expands from raw text and bus aliases instead of live
// `SCH_CONNECTION` members. The shared connectivity owner now flattens from reduced member objects
// only at the comparison sites that still need name lists. Remaining divergence is fuller resolved
// member object ownership beyond this reduced tree.
pub(crate) fn reduced_bus_members(schematic: &Schematic, text: &str) -> Vec<String> {
    reduced_bus_members_inner(schematic, text, &mut BTreeSet::new())
}

// Upstream parity: reduced local analogue for the bus-superset ranking branch inside
// `CONNECTION_SUBGRAPH::ResolveDrivers()`. This is not a 1:1 `SCH_CONNECTION::IsSubsetOf()` call
// because the Rust tree still compares reduced member objects built from text instead of live
// connection objects, but it now compares direct shared member identities instead of reparsing
// flattened leaf strings at the driver-ranking site.
fn reduced_bus_subset_cmp(schematic: &Schematic, lhs: &str, rhs: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    if !reduced_text_is_bus(schematic, lhs) || !reduced_text_is_bus(schematic, rhs) {
        return Ordering::Equal;
    }

    let lhs_members =
        collect_reduced_bus_member_objects_inner(schematic, lhs, "", "", &mut BTreeSet::new());
    let rhs_members =
        collect_reduced_bus_member_objects_inner(schematic, rhs, "", "", &mut BTreeSet::new());
    let lhs_member_names = lhs_members
        .iter()
        .map(|member| member.full_local_name.clone())
        .collect::<Vec<_>>();
    let rhs_member_names = rhs_members
        .iter()
        .map(|member| member.full_local_name.clone())
        .collect::<Vec<_>>();

    if lhs_member_names.is_empty() || rhs_member_names.is_empty() {
        return Ordering::Equal;
    }

    let lhs_is_subset = lhs_member_names
        .iter()
        .all(|member| rhs_member_names.contains(member));
    let rhs_is_subset = rhs_member_names
        .iter()
        .all(|member| lhs_member_names.contains(member));

    match (lhs_is_subset, rhs_is_subset) {
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        _ => Ordering::Equal,
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
// more closely, and it keeps shared base-pin identity even for symbols excluded from node emission
// so graph item lookup stays available for ERC power-pin paths. It now also preserves first-seen
// net-name encounter order instead of reordering reduced subgraphs by net name before the shared
// graph owner assigns whole-net codes. Remaining divergence is the missing full subgraph object
// model and graph-owned netcode allocation beyond these grouped reduced subgraphs.
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
    let mut net_name_order = Vec::<String>::new();

    for component in collect_connection_components(schematic) {
        let Some(net_name) = resolve_net_name(component.anchor).filter(|name| !name.is_empty())
        else {
            continue;
        };
        if !net_map.contains_key(&net_name) {
            net_name_order.push(net_name.clone());
        }

        let mut nodes = BTreeMap::<(String, String), ReducedNetNode>::new();
        let mut base_pins = Vec::new();

        for item in &schematic.screen.items {
            let SchItem::Symbol(symbol) = item else {
                continue;
            };

            if !allow_symbol(symbol) {
                continue;
            }

            let reference = symbol
                .in_netlist
                .then(|| symbol_reference(symbol))
                .flatten();

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
                base_pins.push(base_pin_key);

                let Some(reference) = reference.as_ref() else {
                    continue;
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

    net_name_order
        .into_iter()
        .filter_map(|name| {
            net_map
                .remove(&name)
                .map(|subgraphs| ReducedNetMapEntry { name, subgraphs })
        })
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
// carrier, reduced label/sheet-pin/no-connect membership now rides on the shared subgraph owner
// for graph-side ERC rules instead of per-sheet component rescans, and reduced driver identity now
// rides on that same owner so `RunERC()`-style reused-screen de-duplication can happen above the
// shared graph boundary. The outward reduced node carrier is still narrower than a real
// `CONNECTION_SUBGRAPH` item owner.
pub(crate) fn collect_reduced_project_net_graph_from_inputs(
    inputs: ReducedProjectGraphInputs<'_>,
    for_board: bool,
) -> ReducedProjectNetGraph {
    struct PendingProjectSubgraph {
        name: String,
        driver_connection: Option<ReducedProjectConnection>,
        driver_identity: Option<ReducedProjectDriverIdentity>,
        drivers: Vec<ReducedProjectStrongDriver>,
        non_bus_driver_priority: Option<i32>,
        class: String,
        has_no_connect: bool,
        sheet_instance_path: String,
        anchor: PointKey,
        points: Vec<PointKey>,
        nodes: Vec<ReducedNetNode>,
        base_pins: Vec<ReducedNetBasePinKey>,
        label_links: Vec<ReducedLabelLink>,
        no_connect_points: Vec<PointKey>,
        hier_sheet_pins: Vec<ReducedHierSheetPinLink>,
        hier_ports: Vec<ReducedHierPortLink>,
        bus_members: Vec<ReducedBusMember>,
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
        let sheet_path_prefix = reduced_net_name_sheet_path_prefix(inputs.sheet_paths, sheet_path);

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
                    None,
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
                    None,
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

                let driver_candidate = resolve_reduced_driver_name_candidate_on_component(
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
                    |sheet, pin| {
                        let Some(child_sheet_path) =
                            child_sheet_path_for_sheet(inputs.sheet_paths, sheet_path, sheet)
                        else {
                            return pin.name.clone();
                        };

                        shown_sheet_pin_text(
                            inputs.schematics,
                            inputs.sheet_paths,
                            sheet_path,
                            child_sheet_path,
                            inputs.project,
                            inputs.current_variant,
                            None,
                            pin,
                        )
                    },
                );
                let driver_identity = driver_candidate.as_ref().and_then(|candidate| {
                    candidate
                        .identity
                        .as_ref()
                        .map(|identity| identity.to_project_identity(&sheet_path.schematic_path))
                });
                let strong_drivers = collect_reduced_strong_drivers(
                    schematic,
                    &connected_component,
                    &sheet_path_prefix,
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
                    |sheet, pin| {
                        let Some(child_sheet_path) =
                            child_sheet_path_for_sheet(inputs.sheet_paths, sheet_path, sheet)
                        else {
                            return pin.name.clone();
                        };

                        shown_sheet_pin_text(
                            inputs.schematics,
                            inputs.sheet_paths,
                            sheet_path,
                            child_sheet_path,
                            inputs.project,
                            inputs.current_variant,
                            None,
                            pin,
                        )
                    },
                );
                let non_bus_driver_priority = strong_drivers
                    .iter()
                    .find(|driver| !reduced_text_is_bus(schematic, &driver.name))
                    .map(|driver| driver.priority);
                let (label_links, no_connect_points, bus_items, wire_items) =
                    collect_reduced_subgraph_local_membership(
                        inputs.schematics,
                        inputs.sheet_paths,
                        sheet_path,
                        schematic,
                        inputs.project,
                        inputs.current_variant,
                        &connected_component,
                    );
                let (hier_sheet_pins, hier_ports) = collect_reduced_subgraph_hierarchy_membership(
                    inputs.schematics,
                    inputs.sheet_paths,
                    sheet_path,
                    schematic,
                    inputs.project,
                    inputs.current_variant,
                    &connected_component,
                );
                let mut bus_members = label_links
                    .iter()
                    .filter(|link| {
                        matches!(
                            link.connection.connection_type,
                            ReducedProjectConnectionType::Bus
                                | ReducedProjectConnectionType::BusGroup
                        )
                    })
                    .flat_map(|link| link.connection.members.clone())
                    .collect::<Vec<_>>();
                bus_members.extend(
                    hier_sheet_pins
                        .iter()
                        .filter(|pin| {
                            matches!(
                                pin.connection.connection_type,
                                ReducedProjectConnectionType::Bus
                                    | ReducedProjectConnectionType::BusGroup
                            )
                        })
                        .flat_map(|pin| pin.connection.members.clone()),
                );
                bus_members.extend(
                    hier_ports
                        .iter()
                        .filter(|port| {
                            matches!(
                                port.connection.connection_type,
                                ReducedProjectConnectionType::Bus
                                    | ReducedProjectConnectionType::BusGroup
                            )
                        })
                        .flat_map(|port| port.connection.members.clone()),
                );
                bus_members.sort();
                bus_members.dedup();
                let driver_connection = driver_candidate.as_ref().map(|candidate| {
                    build_reduced_project_connection(
                        schematic,
                        sheet_path.instance_path.clone(),
                        entry.name.clone(),
                        candidate.text.clone(),
                        reduced_driver_candidate_full_name(candidate, &sheet_path_prefix),
                        if reduced_text_is_bus(schematic, &candidate.text) {
                            bus_members.clone()
                        } else {
                            Vec::new()
                        },
                    )
                });

                pending_subgraphs.push(PendingProjectSubgraph {
                    name: entry.name.clone(),
                    driver_connection,
                    driver_identity,
                    drivers: strong_drivers.clone(),
                    non_bus_driver_priority,
                    class: class.clone(),
                    has_no_connect,
                    sheet_instance_path: sheet_path.instance_path.clone(),
                    anchor: point_key(connected_component.anchor),
                    points: points.clone(),
                    nodes: nodes.clone(),
                    base_pins: base_pins.clone(),
                    label_links,
                    no_connect_points,
                    hier_sheet_pins,
                    hier_ports,
                    bus_members,
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
                None,
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

            let (label_links, no_connect_points, bus_items, wire_items) =
                collect_reduced_subgraph_local_membership(
                    inputs.schematics,
                    inputs.sheet_paths,
                    sheet_path,
                    schematic,
                    inputs.project,
                    inputs.current_variant,
                    &connected_component,
                );
            let (hier_sheet_pins, hier_ports) = collect_reduced_subgraph_hierarchy_membership(
                inputs.schematics,
                inputs.sheet_paths,
                sheet_path,
                schematic,
                inputs.project,
                inputs.current_variant,
                &connected_component,
            );

            pending_subgraphs.push(PendingProjectSubgraph {
                name: String::new(),
                driver_connection: None,
                driver_identity: None,
                drivers: Vec::new(),
                non_bus_driver_priority: None,
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
                label_links,
                no_connect_points,
                hier_sheet_pins,
                hier_ports,
                bus_members: Vec::new(),
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

    let mut reduced_subgraphs = Vec::new();
    let mut subgraphs_by_name = BTreeMap::<String, Vec<usize>>::new();
    let mut subgraphs_by_sheet_and_name = BTreeMap::<(String, String), Vec<usize>>::new();
    let mut pin_subgraph_identities = BTreeMap::new();
    let mut pin_subgraph_identities_by_location = BTreeMap::new();
    let mut point_subgraph_identities = BTreeMap::new();
    let mut label_subgraph_identities = BTreeMap::new();
    let mut no_connect_subgraph_identities = BTreeMap::new();
    let mut net_identities_by_name = BTreeMap::<String, ReducedProjectNetIdentity>::new();

    for (subgraph_index, pending) in pending_subgraphs.into_iter().enumerate() {
        if !pending.name.is_empty() && !net_identities_by_name.contains_key(&pending.name) {
            let (class, has_no_connect, _nodes, _base_pins) =
                nets.get(&pending.name).cloned().unwrap_or_default();
            let code = net_identities_by_name.len() + 1;
            net_identities_by_name.insert(
                pending.name.clone(),
                ReducedProjectNetIdentity {
                    code,
                    name: pending.name.clone(),
                    class,
                    has_no_connect,
                },
            );
        }
        let net_identity = net_identities_by_name.get(&pending.name);
        let subgraph_sheet = inputs
            .sheet_paths
            .iter()
            .find(|sheet| sheet.instance_path == pending.sheet_instance_path)
            .and_then(|sheet| {
                inputs
                    .schematics
                    .iter()
                    .find(|schematic| schematic.path == sheet.schematic_path)
            })
            .expect("pending reduced subgraph must resolve its source schematic");
        let resolved_name = net_identity
            .map(|net| net.name.clone())
            .unwrap_or_else(|| pending.name.clone());
        let resolved_local_name = if let Some(driver_connection) = &pending.driver_connection {
            driver_connection.local_name.clone()
        } else if let Some(connection) = pending
            .label_links
            .iter()
            .map(|link| &link.connection)
            .chain(pending.hier_sheet_pins.iter().map(|pin| &pin.connection))
            .chain(pending.hier_ports.iter().map(|port| &port.connection))
            .find(|connection| {
                matches!(
                    connection.connection_type,
                    ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
                )
            })
        {
            connection.local_name.clone()
        } else {
            reduced_short_net_name(&resolved_name)
        };
        let resolved_full_local_name = if let Some(driver_connection) = &pending.driver_connection {
            driver_connection.full_local_name.clone()
        } else if !resolved_name.is_empty() {
            resolved_name.clone()
        } else {
            resolved_local_name.clone()
        };
        let resolved_connection = build_reduced_project_connection(
            subgraph_sheet,
            pending.sheet_instance_path.clone(),
            resolved_name.clone(),
            resolved_local_name,
            resolved_full_local_name,
            pending.bus_members.clone(),
        );
        let net_identity = ReducedProjectSubgraphEntry {
            subgraph_code: subgraph_index + 1,
            code: net_identity.map(|net| net.code).unwrap_or_default(),
            name: resolved_name,
            resolved_connection,
            driver_connection: pending.driver_connection.clone(),
            driver_identity: pending.driver_identity.clone(),
            drivers: pending.drivers.clone(),
            non_bus_driver_priority: pending.non_bus_driver_priority,
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
            label_links: pending.label_links.clone(),
            no_connect_points: pending.no_connect_points.clone(),
            hier_sheet_pins: pending.hier_sheet_pins.clone(),
            hier_ports: pending.hier_ports.clone(),
            bus_members: pending.bus_members.clone(),
            bus_items: pending.bus_items.clone(),
            wire_items: pending.wire_items.clone(),
            bus_neighbor_links: Vec::new(),
            bus_parent_links: Vec::new(),
            bus_parent_indexes: Vec::new(),
            hier_parent_index: None,
            hier_child_indexes: Vec::new(),
        };

        let index = reduced_subgraphs.len();
        subgraphs_by_name
            .entry(net_identity.name.clone())
            .or_default()
            .push(index);
        if net_identity.name.contains('[') {
            let prefix_only = format!("{}[]", net_identity.name.split('[').next().unwrap_or(""));
            subgraphs_by_name
                .entry(prefix_only)
                .or_default()
                .push(index);
        }
        subgraphs_by_sheet_and_name
            .entry((
                net_identity.sheet_instance_path.clone(),
                net_identity.name.clone(),
            ))
            .or_default()
            .push(index);
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
        for label in &net_identity.label_links {
            label_subgraph_identities.insert(
                ReducedProjectLabelIdentityKey {
                    sheet_instance_path: net_identity.sheet_instance_path.clone(),
                    at: label.at,
                    kind: reduced_label_kind_sort_key(label.kind),
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

    let subgraphs_by_sheet = reduced_subgraphs.iter().enumerate().fold(
        BTreeMap::<String, Vec<usize>>::new(),
        |mut acc, (index, subgraph)| {
            acc.entry(subgraph.sheet_instance_path.clone())
                .or_default()
                .push(index);
            acc
        },
    );
    let child_sheet_by_parent_and_uuid = inputs
        .sheet_paths
        .iter()
        .filter_map(|sheet_path| {
            let sheet_uuid = sheet_path.sheet_uuid.clone()?;
            let parent_symbol_path = sheet_path
                .symbol_path
                .rsplit_once('/')
                .map(|(parent, _)| parent)
                .unwrap_or_default();
            let parent_instance_path = inputs
                .sheet_paths
                .iter()
                .filter(|candidate| {
                    parent_symbol_path == candidate.symbol_path
                        || parent_symbol_path.starts_with(&(candidate.symbol_path.clone() + "/"))
                })
                .max_by_key(|candidate| candidate.symbol_path.len())
                .map(|candidate| candidate.instance_path.clone())
                .unwrap_or_default();

            Some((
                (parent_instance_path, sheet_uuid),
                sheet_path.instance_path.clone(),
            ))
        })
        .collect::<BTreeMap<_, _>>();
    let mut hier_parent_indexes = vec![None; reduced_subgraphs.len()];
    let mut hier_child_indexes = vec![BTreeSet::<usize>::new(); reduced_subgraphs.len()];
    let mut bus_parent_indexes = vec![BTreeSet::<usize>::new(); reduced_subgraphs.len()];
    let mut bus_neighbor_links =
        vec![BTreeSet::<ReducedProjectBusNeighborLink>::new(); reduced_subgraphs.len()];
    let mut bus_parent_links =
        vec![BTreeSet::<ReducedProjectBusNeighborLink>::new(); reduced_subgraphs.len()];

    for (parent_index, subgraph) in reduced_subgraphs.iter().enumerate() {
        for hier_pin in &subgraph.hier_sheet_pins {
            let Some(child_sheet_uuid) = hier_pin.child_sheet_uuid.clone() else {
                continue;
            };
            let Some(child_sheet_instance_path) = child_sheet_by_parent_and_uuid
                .get(&(subgraph.sheet_instance_path.clone(), child_sheet_uuid))
            else {
                continue;
            };
            let Some(child_indexes) = subgraphs_by_sheet.get(child_sheet_instance_path) else {
                continue;
            };

            for child_index in child_indexes {
                let child = &reduced_subgraphs[*child_index];

                if !child.hier_ports.iter().any(|port| {
                    port.connection.local_name == hier_pin.connection.local_name
                        && port.connection.connection_type == hier_pin.connection.connection_type
                }) {
                    continue;
                }

                hier_parent_indexes[*child_index].get_or_insert(parent_index);
                hier_child_indexes[parent_index].insert(*child_index);
            }
        }
    }

    for (parent_index, subgraph) in reduced_subgraphs.iter().enumerate() {
        if subgraph.bus_members.is_empty() {
            continue;
        }

        let Some(same_sheet_indexes) = subgraphs_by_sheet.get(&subgraph.sheet_instance_path) else {
            continue;
        };
        let member_leaves = reduced_bus_member_leaf_objects(&subgraph.bus_members);

        for child_index in same_sheet_indexes {
            if *child_index == parent_index {
                continue;
            }

            let child = &reduced_subgraphs[*child_index];
            let mut child_names = child
                .label_links
                .iter()
                .map(|link| &link.connection)
                .chain(child.hier_sheet_pins.iter().map(|pin| &pin.connection))
                .chain(child.hier_ports.iter().map(|port| &port.connection))
                .filter(|connection| {
                    connection.connection_type == ReducedProjectConnectionType::Net
                })
                .map(|connection| connection.full_local_name.clone())
                .collect::<Vec<_>>();

            if let Some(driver_connection) = &child.driver_connection {
                if !driver_connection.full_local_name.is_empty() {
                    child_names.push(driver_connection.full_local_name.clone());
                }
            } else if !child.resolved_connection.name.is_empty() {
                child_names.push(child.resolved_connection.name.clone());
            }

            for member in &member_leaves {
                if child_names
                    .iter()
                    .any(|name| name == &member.full_local_name)
                {
                    bus_parent_indexes[*child_index].insert(parent_index);
                    bus_neighbor_links[parent_index].insert(ReducedProjectBusNeighborLink {
                        member: member.clone(),
                        subgraph_index: *child_index,
                    });
                    bus_parent_links[*child_index].insert(ReducedProjectBusNeighborLink {
                        member: member.clone(),
                        subgraph_index: parent_index,
                    });
                }
            }
        }
    }

    for (index, subgraph) in reduced_subgraphs.iter_mut().enumerate() {
        subgraph.hier_parent_index = hier_parent_indexes[index];
        subgraph.hier_child_indexes = hier_child_indexes[index].iter().copied().collect();
        subgraph.bus_neighbor_links = bus_neighbor_links[index].iter().cloned().collect();
        subgraph.bus_parent_links = bus_parent_links[index].iter().cloned().collect();
        subgraph.bus_parent_indexes = bus_parent_indexes[index].iter().copied().collect();
    }

    refresh_reduced_global_secondary_driver_promotions(&mut reduced_subgraphs);
    refresh_reduced_live_graph_propagation(&mut reduced_subgraphs);
    attach_reduced_connected_bus_items(&mut reduced_subgraphs);
    let (subgraphs_by_name, subgraphs_by_sheet_and_name) =
        rebuild_reduced_project_graph_name_caches(&mut reduced_subgraphs);

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
// and graph-owned resolved-name caches beyond this reduced project net map. It now also preserves
// the shared graph's reduced net codes for non-export callers instead of renumbering them a second
// time at the flattened whole-net layer; write-time exporters still do their own emitted-code
// assignment like KiCad `makeListOfNets()`.
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
            |((code, name), (class, has_no_connect, nodes, base_pins))| {
                let nodes = nodes.into_values().collect::<Vec<_>>();
                (!nodes.is_empty()).then_some((code, name, class, has_no_connect, nodes, base_pins))
            },
        )
        .map(
            |(code, name, class, has_no_connect, nodes, base_pins)| ReducedProjectNetEntry {
                code,
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
// Upstream parity: reduced local analogue for indexing into `CONNECTION_GRAPH` subgraph storage.
// This is not a 1:1 pointer owner because the Rust tree still stores cloned reduced subgraphs
// instead of live `CONNECTION_SUBGRAPH*`, but it keeps parent/child relation consumers on the
// shared graph owner instead of exposing the private storage directly. Remaining divergence is the
// still-missing live subgraph object model.
pub(crate) fn reduced_project_subgraph_by_index(
    graph: &ReducedProjectNetGraph,
    index: usize,
) -> Option<&ReducedProjectSubgraphEntry> {
    graph.subgraphs.get(index)
}

#[cfg_attr(not(test), allow(dead_code))]
// Upstream parity: reduced local analogue for locating a concrete `CONNECTION_SUBGRAPH*` inside
// graph-owned caches. This is not a 1:1 pointer lookup because the Rust tree still keys by
// reduced `(sheet instance path, subgraph code)` snapshots, but it keeps parent-link consumers on
// the shared graph owner instead of re-enumerating private storage. Remaining divergence is the
// still-missing live subgraph object model.
pub(crate) fn reduced_project_subgraph_index(
    graph: &ReducedProjectNetGraph,
    subgraph: &ReducedProjectSubgraphEntry,
) -> Option<usize> {
    graph.subgraphs.iter().position(|candidate| {
        candidate.sheet_instance_path == subgraph.sheet_instance_path
            && candidate.subgraph_code == subgraph.subgraph_code
    })
}

#[cfg_attr(not(test), allow(dead_code))]
// Upstream parity: reduced local analogue for `CONNECTION_GRAPH::FindSubgraphByName()`. This is
// not a 1:1 graph lookup because the Rust tree still lacks live `CONNECTION_SUBGRAPH` ownership,
// but it now preserves KiCad's `(sheet instance path, resolved net name)` lookup boundary and
// same-name multi-subgraph list shape instead of the old repo-local short-driver key. Remaining
// divergence is the fuller subgraph object model and exact driver-connection caching.
pub(crate) fn find_reduced_project_subgraph_by_name<'a>(
    graph: &'a ReducedProjectNetGraph,
    net_name: &str,
    sheet_path: &LoadedSheetPath,
) -> Option<&'a ReducedProjectSubgraphEntry> {
    graph
        .subgraphs_by_sheet_and_name
        .get(&(sheet_path.instance_path.clone(), net_name.to_string()))
        .and_then(|indexes| indexes.first())
        .and_then(|index| graph.subgraphs.get(*index))
}

#[cfg_attr(not(test), allow(dead_code))]
// Upstream parity: reduced local analogue for `CONNECTION_GRAPH::FindFirstSubgraphByName()`. This
// is not a 1:1 global lookup because the Rust tree still stores reduced subgraphs in the shared
// project graph instead of live `CONNECTION_SUBGRAPH*` objects, but it restores the owner
// boundary where graph/export/ERC callers can ask for the first resolved subgraph by full net name
// instead of flattening to whole-net facts only. It now also preserves KiCad's exercised vector
// bus `prefix[]` alias entries beside the full resolved bus name. Remaining divergence is the
// fuller subgraph object model and graph-owned resolved-name caches.
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
// Rust tree still lacks live `SCH_CONNECTION` instances, but it now reads the shared reduced
// driver connection owner instead of storing a separate short-name cache on the subgraph.
// Remaining divergence is fuller live connection-object caching and item ownership.
pub(crate) fn resolve_reduced_project_driver_name_at(
    graph: &ReducedProjectNetGraph,
    sheet_path: &LoadedSheetPath,
    at: [f64; 2],
) -> Option<String> {
    resolve_reduced_project_subgraph_at(graph, sheet_path, at).and_then(|subgraph| {
        subgraph
            .driver_connection
            .as_ref()
            .map(|connection| connection.local_name.clone())
    })
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
// Upstream parity: reduced local helper for the label/no-connect/wire membership KiCad keeps on
// `CONNECTION_SUBGRAPH`. This is not a 1:1 item-owner cache because the Rust tree still stores
// reduced snapshots instead of live `SCH_ITEM*`, but it now attaches reduced connection ownership
// directly to label links instead of splitting label point state from bus/net text state. Remaining
// divergence is fuller live item identity plus in-place connection updates.
fn collect_reduced_subgraph_local_membership(
    schematics: &[Schematic],
    sheet_paths: &[LoadedSheetPath],
    sheet_path: &LoadedSheetPath,
    schematic: &Schematic,
    project: Option<&LoadedProjectSettings>,
    current_variant: Option<&str>,
    connected_component: &ConnectionComponent,
) -> (
    Vec<ReducedLabelLink>,
    Vec<PointKey>,
    Vec<ReducedSubgraphWireItem>,
    Vec<ReducedSubgraphWireItem>,
) {
    let sheet_path_prefix = reduced_net_name_sheet_path_prefix(sheet_paths, sheet_path);
    let mut label_links = connected_component
        .members
        .iter()
        .filter_map(|member| {
            if member.kind != ConnectionMemberKind::Label {
                return None;
            }

            schematic.screen.items.iter().find_map(|item| match item {
                SchItem::Label(label) if points_equal(label.at, member.at) => {
                    let shown = shown_label_text_without_connectivity(
                        schematics,
                        sheet_paths,
                        sheet_path,
                        project,
                        current_variant,
                        label,
                    );
                    let full_name = match label.kind {
                        LabelKind::Global | LabelKind::Directive => shown.clone(),
                        LabelKind::Local | LabelKind::Hierarchical => {
                            format!("{sheet_path_prefix}{shown}")
                        }
                    };
                    let members = if reduced_text_is_bus(schematic, &shown) {
                        let member_sheet_prefix = if label.kind == LabelKind::Global {
                            ""
                        } else {
                            &sheet_path_prefix
                        };
                        collect_reduced_bus_member_objects_inner(
                            schematic,
                            &shown,
                            "",
                            member_sheet_prefix,
                            &mut BTreeSet::new(),
                        )
                    } else {
                        Vec::new()
                    };

                    Some(ReducedLabelLink {
                        at: point_key(label.at),
                        kind: label.kind,
                        connection: build_reduced_project_connection(
                            schematic,
                            sheet_path.instance_path.clone(),
                            full_name.clone(),
                            shown,
                            full_name,
                            members,
                        ),
                    })
                }
                _ => None,
            })
        })
        .collect::<Vec<_>>();
    label_links.sort_by_key(|link| {
        (
            link.at.0,
            link.at.1,
            reduced_label_kind_sort_key(link.kind),
            link.connection.connection_type,
            link.connection.full_local_name.clone(),
        )
    });
    label_links.dedup();

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
                    connected_bus_subgraph_index: None,
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
                    connected_bus_subgraph_index: None,
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
                    connected_bus_subgraph_index: None,
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

    (label_links, no_connect_points, bus_items, wire_items)
}

fn child_sheet_path_for_sheet<'a>(
    sheet_paths: &'a [LoadedSheetPath],
    parent_path: &LoadedSheetPath,
    sheet: &crate::model::Sheet,
) -> Option<&'a LoadedSheetPath> {
    sheet_paths
        .iter()
        .filter(|candidate| {
            candidate
                .instance_path
                .starts_with(&parent_path.instance_path)
                && candidate.instance_path != parent_path.instance_path
        })
        .find(|candidate| candidate.sheet_uuid == sheet.uuid)
}

// Upstream parity: reduced local helper for the bus-member state KiCad keeps on
// `SCH_CONNECTION` / `CONNECTION_SUBGRAPH`. This is not a 1:1 upstream routine because the Rust
// tree still derives member objects from shown text plus aliases instead of live cloned
// `SCH_CONNECTION` trees, but it now preserves reduced member kind plus local/full-local naming on
// the shared subgraph owner instead of collapsing immediately to flat member strings. Remaining
// divergence is fuller resolved member-object ownership beyond this reduced tree.
// Upstream parity: reduced local helper for the hierarchical pin/port membership caches KiCad
// keeps on `CONNECTION_SUBGRAPH`. This is not a 1:1 item-owner cache because the Rust tree still
// stores reduced shown-text snapshots instead of live `SCH_SHEET_PIN*` / `SCH_HIERLABEL*`, but it
// now attaches reduced connection ownership directly to those hierarchy links so parent-child
// matching and ERC do not need separate raw name/type caches. Remaining divergence is fuller
// item-pointer identity and live connection-type ownership.
fn collect_reduced_subgraph_hierarchy_membership(
    schematics: &[Schematic],
    sheet_paths: &[LoadedSheetPath],
    parent_sheet_path: &LoadedSheetPath,
    schematic: &Schematic,
    project: Option<&LoadedProjectSettings>,
    current_variant: Option<&str>,
    connected_component: &ConnectionComponent,
) -> (Vec<ReducedHierSheetPinLink>, Vec<ReducedHierPortLink>) {
    let sheet_path_prefix = reduced_net_name_sheet_path_prefix(sheet_paths, parent_sheet_path);
    let mut hier_sheet_pins = Vec::<ReducedHierSheetPinLink>::new();
    let mut hier_ports = Vec::<ReducedHierPortLink>::new();

    for item in &schematic.screen.items {
        match item {
            SchItem::Sheet(sheet) => {
                let Some(child_sheet_path) =
                    child_sheet_path_for_sheet(sheet_paths, parent_sheet_path, sheet)
                else {
                    continue;
                };

                for pin in &sheet.pins {
                    if !connected_component.members.iter().any(|member| {
                        member.kind == ConnectionMemberKind::SheetPin
                            && points_equal(member.at, pin.at)
                    }) {
                        continue;
                    }

                    let shown = shown_sheet_pin_text(
                        schematics,
                        sheet_paths,
                        parent_sheet_path,
                        child_sheet_path,
                        project,
                        current_variant,
                        None,
                        pin,
                    );

                    hier_sheet_pins.push(ReducedHierSheetPinLink {
                        at: point_key(pin.at),
                        child_sheet_uuid: sheet.uuid.clone(),
                        connection: build_reduced_project_connection(
                            schematic,
                            parent_sheet_path.instance_path.clone(),
                            format!("{sheet_path_prefix}{shown}"),
                            shown.clone(),
                            format!("{sheet_path_prefix}{shown}"),
                            if reduced_text_is_bus(schematic, &shown) {
                                collect_reduced_bus_member_objects_inner(
                                    schematic,
                                    &shown,
                                    "",
                                    &sheet_path_prefix,
                                    &mut BTreeSet::new(),
                                )
                            } else {
                                Vec::new()
                            },
                        ),
                    });
                }
            }
            SchItem::Label(label)
                if label.kind == LabelKind::Hierarchical
                    && connected_component.members.iter().any(|member| {
                        member.kind == ConnectionMemberKind::Label
                            && points_equal(member.at, label.at)
                    }) =>
            {
                let shown = shown_label_text_without_connectivity(
                    schematics,
                    sheet_paths,
                    parent_sheet_path,
                    project,
                    current_variant,
                    label,
                );

                hier_ports.push(ReducedHierPortLink {
                    at: point_key(label.at),
                    connection: build_reduced_project_connection(
                        schematic,
                        parent_sheet_path.instance_path.clone(),
                        format!("{sheet_path_prefix}{shown}"),
                        shown.clone(),
                        format!("{sheet_path_prefix}{shown}"),
                        if reduced_text_is_bus(schematic, &shown) {
                            collect_reduced_bus_member_objects_inner(
                                schematic,
                                &shown,
                                "",
                                &sheet_path_prefix,
                                &mut BTreeSet::new(),
                            )
                        } else {
                            Vec::new()
                        },
                    ),
                });
            }
            _ => {}
        }
    }

    hier_sheet_pins.sort();
    hier_sheet_pins.dedup();
    hier_ports.sort();
    hier_ports.dedup();

    (hier_sheet_pins, hier_ports)
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
// Rust tree still lacks live `SCH_CONNECTION` instances, but it now reads the shared reduced
// driver connection owner for pin text vars instead of keeping a duplicate short-name cache.
// Remaining divergence is fuller live connection-object caching and item ownership.
pub(crate) fn resolve_reduced_project_driver_name_for_symbol_pin(
    graph: &ReducedProjectNetGraph,
    sheet_path: &LoadedSheetPath,
    symbol: &Symbol,
    at: [f64; 2],
    pin_name: Option<&str>,
) -> Option<String> {
    resolve_reduced_project_subgraph_for_symbol_pin(graph, sheet_path, symbol, at, pin_name)
        .and_then(|subgraph| {
            subgraph
                .driver_connection
                .as_ref()
                .map(|connection| connection.local_name.clone())
        })
}

// Upstream parity: reduced local analogue for the connection-point half of
// `CONNECTION_GRAPH::GetSubgraphForItem()` / `GetResolvedSubgraphName()` on the project graph
// path. This is not a 1:1 KiCad item map because the Rust tree still keys the lookup by `(sheet
// instance path, reduced subgraph anchor)` instead of a live item-owned `CONNECTION_SUBGRAPH`,
// but it now derives the reported net name through the shared reduced resolved-connection owner
// instead of only the older flattened subgraph name field. Remaining divergence is fuller item
// identity for labels, wires, and markers plus the still-missing `CONNECTION_SUBGRAPH` object.
pub(crate) fn resolve_reduced_project_net_at(
    graph: &ReducedProjectNetGraph,
    sheet_path: &LoadedSheetPath,
    at: [f64; 2],
) -> Option<ReducedProjectNetIdentity> {
    resolve_reduced_project_subgraph_at(graph, sheet_path, at).map(|subgraph| {
        ReducedProjectNetIdentity {
            code: subgraph.code,
            name: subgraph.resolved_connection.name.clone(),
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
    force_no_connect: bool,
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
    let unconnected = force_no_connect || pin.electrical_type.as_deref() == Some("no_connect");
    let name_is_duplicated = pin_name.is_some_and(|name| {
        unit_pins.iter().any(|other| {
            other.number.as_deref() != Some(pin_number)
                && other.name.as_deref() == Some(name)
                && unconnected == (other.electrical_type.as_deref() == Some("no_connect"))
        })
    });

    let prefix = if unconnected {
        "unconnected-("
    } else {
        "Net-("
    };

    if let Some(pin_name) = pin_name {
        let mut name = format!("{prefix}{reference}-{pin_name}");

        if name_is_duplicated {
            name.push_str(&format!("-Pad{pin_number}"));
        }

        name.push(')');
        return Some(name);
    }

    Some(format!("{prefix}{reference}-Pad{pin_number})"))
}

fn label_uses_connectivity_dependent_text(label: &Label) -> bool {
    let text = label.text.to_ascii_uppercase();

    text.contains("NET_NAME")
        || text.contains("SHORT_NET_NAME")
        || text.contains("NET_CLASS")
        || text.contains("CONNECTION_TYPE")
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

fn reduced_driver_candidate_full_name(
    candidate: &ReducedDriverNameCandidate,
    sheet_path_prefix: &str,
) -> String {
    let prepend_path = matches!(
        candidate.source,
        ReducedNetNameSource::LocalLabel
            | ReducedNetNameSource::HierarchicalLabel
            | ReducedNetNameSource::SheetPin
            | ReducedNetNameSource::LocalPowerPin
    );

    if prepend_path {
        if candidate.text.starts_with('/') {
            candidate.text.clone()
        } else {
            format!("{sheet_path_prefix}{}", candidate.text)
        }
    } else {
        candidate.text.clone()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ReducedLocalDriverIdentity {
    Label {
        at: PointKey,
        kind: u8,
    },
    SheetPin {
        at: PointKey,
    },
    SymbolPin {
        symbol_uuid: Option<String>,
        at: PointKey,
    },
}

impl ReducedLocalDriverIdentity {
    fn to_project_identity(
        &self,
        schematic_path: &std::path::Path,
    ) -> ReducedProjectDriverIdentity {
        match self {
            ReducedLocalDriverIdentity::Label { at, kind } => ReducedProjectDriverIdentity::Label {
                schematic_path: schematic_path.to_path_buf(),
                at: *at,
                kind: *kind,
            },
            ReducedLocalDriverIdentity::SheetPin { at } => ReducedProjectDriverIdentity::SheetPin {
                schematic_path: schematic_path.to_path_buf(),
                at: *at,
            },
            ReducedLocalDriverIdentity::SymbolPin { symbol_uuid, at } => {
                ReducedProjectDriverIdentity::SymbolPin {
                    schematic_path: schematic_path.to_path_buf(),
                    symbol_uuid: symbol_uuid.clone(),
                    at: *at,
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReducedDriverNameCandidate {
    priority: i32,
    sheet_pin_rank: i32,
    text: String,
    source: ReducedNetNameSource,
    identity: Option<ReducedLocalDriverIdentity>,
}

// Upstream parity: reduced local analogue for the strong-driver collection inside
// `CONNECTION_SUBGRAPH::ResolveDrivers()`. This is not a 1:1 KiCad driver cache because the Rust
// tree still lacks live `SCH_CONNECTION` objects and full subgraph ownership, but it now keeps
// the shared graph's strong-driver names on the same shown-text owner KiCad uses for labels and
// sheet pins instead of leaving sheet-pin drivers on raw parser text, and now also preserves the
// reduced driver kind the shared graph needs for `ercCheckMultipleDrivers()`-style filtering
// instead of collapsing every strong driver to bare names. Remaining divergence is the still-
// missing live connection object plus fuller power/bus-parent driver ownership.
fn collect_reduced_strong_drivers<FLabel, FSheet>(
    schematic: &Schematic,
    connected_component: &ConnectionComponent,
    sheet_path_prefix: &str,
    mut shown_label_text: FLabel,
    mut shown_sheet_pin_text: FSheet,
) -> Vec<ReducedProjectStrongDriver>
where
    FLabel: FnMut(&Label) -> String,
    FSheet: FnMut(&crate::model::Sheet, &crate::model::SheetPin) -> String,
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
                let full_name = match label.kind {
                    LabelKind::Global => text.clone(),
                    LabelKind::Local | LabelKind::Hierarchical => {
                        format!("{sheet_path_prefix}{text}")
                    }
                    LabelKind::Directive => return None,
                };
                Some(ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: reduced_label_driver_priority(label),
                    name: text,
                    full_name,
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
                .map(|pin| {
                    let shown = shown_sheet_pin_text(sheet, pin);

                    ReducedProjectStrongDriver {
                        kind: ReducedProjectDriverKind::SheetPin,
                        priority: reduced_sheet_pin_driver_rank(pin.shape),
                        name: shown.clone(),
                        full_name: format!("{sheet_path_prefix}{shown}"),
                    }
                })
                .max_by(|lhs, rhs| {
                    lhs.priority
                        .cmp(&rhs.priority)
                        .then_with(|| rhs.name.cmp(&lhs.name))
                }),
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
                                symbol_value_text(symbol).map(|text| ReducedProjectStrongDriver {
                                    kind: ReducedProjectDriverKind::PowerPin,
                                    priority,
                                    full_name: if symbol
                                        .lib_symbol
                                        .as_ref()
                                        .is_some_and(|lib_symbol| lib_symbol.local_power)
                                    {
                                        format!("{sheet_path_prefix}{text}")
                                    } else {
                                        text.clone()
                                    },
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
// 1:1 KiCad driver owner because the Rust tree still lacks full subgraphs, fuller power-pin
// drivers, and cached `SCH_CONNECTION` objects. It exists so loader shown-text and export paths do
// not each pick the "first connected label" independently. The current reduced driver ranking is
// limited to the driver kinds the Rust tree can already model on one sheet:
// - global labels outrank global power pins
// - global power pins outrank local power pins
// - local power pins outrank local labels
// - local labels outrank hierarchical labels
// - sheet pins now participate through a caller-provided reduced `SCH_SHEET_PIN::GetShownText()`
//   analogue instead of raw parser pin names, with output pins preferred over non-output pins
// - ordinary symbol pins participate last through reduced `SCH_PIN::GetDefaultNetName()`-style
//   fallback names so unlabeled nets still get deterministic export/CLI names
// - equal-priority bus labels first prefer supersets over subsets to keep the widest connection
//   before falling back to sheet-pin rank / name quality / alphabetical order
// - labels whose raw text still depends on the reduced connectivity resolver are skipped so the
//   current reduced driver path does not recurse back into itself
// - the winning reduced driver now also carries enough stable local identity for the shared
//   project graph to mimic `RunERC()` driver-instance de-duplication across reused screens
// Remaining divergence is the still-missing live connection object plus fuller bus-parent/power
// driver ownership.
fn resolve_reduced_driver_name_candidate_on_component<FLabel, FSheet>(
    schematic: &Schematic,
    connected_component: &ConnectionComponent,
    mut shown_label_text: FLabel,
    mut shown_sheet_pin_text: FSheet,
) -> Option<ReducedDriverNameCandidate>
where
    FLabel: FnMut(&Label) -> String,
    FSheet: FnMut(&crate::model::Sheet, &crate::model::SheetPin) -> String,
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
                    identity: Some(ReducedLocalDriverIdentity::Label {
                        at: point_key(label.at),
                        kind: reduced_label_kind_sort_key(label.kind),
                    }),
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
                    text: shown_sheet_pin_text(sheet, pin),
                    source: ReducedNetNameSource::SheetPin,
                    identity: Some(ReducedLocalDriverIdentity::SheetPin {
                        at: point_key(pin.at),
                    }),
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
                                        identity: Some(ReducedLocalDriverIdentity::SymbolPin {
                                            symbol_uuid: symbol.uuid.clone(),
                                            at: point_key(pin.at),
                                        }),
                                    }
                                })
                            })
                            .or_else(|| {
                                reduced_symbol_pin_default_net_name(symbol, &pin, &unit_pins, false)
                                    .map(|text| ReducedDriverNameCandidate {
                                        priority: 1,
                                        sheet_pin_rank: 0,
                                        text,
                                        source: ReducedNetNameSource::SymbolPinDefault,
                                        identity: Some(ReducedLocalDriverIdentity::SymbolPin {
                                            symbol_uuid: symbol.uuid.clone(),
                                            at: point_key(pin.at),
                                        }),
                                    })
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

fn reduced_force_no_connect_net_name(name: &str) -> String {
    name.replacen("Net-(", "unconnected-(", 1)
}

// Upstream parity: reduced local analogue for the post-propagation
// `CONNECTION_SUBGRAPH::UpdateItemConnections()` follow-up inside `buildConnectionGraph()`. This
// is not a 1:1 live item-connection mutation because the Rust tree still rewrites reduced subgraph
// snapshots instead of mutating live `SCH_ITEM`-owned `SCH_CONNECTION` objects, but it preserves
// the exercised branches still reachable in the reduced graph:
// - weak single-pin default-net names are forced onto the `unconnected-(` path
// - self-driven sheet-pin nets adopt bus type/member shape from bus-typed child hierarchy
//   neighbors
// Remaining divergence is the still-missing live `UpdateItemConnections()` cache/update timing on
// real items.
fn refresh_reduced_post_propagation_item_connections(
    reduced_subgraphs: &mut [ReducedProjectSubgraphEntry],
) {
    #[derive(Clone)]
    enum PostPropagationUpdate {
        ForceNoConnectName {
            index: usize,
            connection: ReducedProjectConnection,
        },
        PromoteSheetPinBus {
            index: usize,
            connection_type: ReducedProjectConnectionType,
            members: Vec<ReducedBusMember>,
        },
    }

    let mut updates = Vec::<PostPropagationUpdate>::new();

    for (index, subgraph) in reduced_subgraphs.iter().enumerate() {
        if subgraph.drivers.is_empty()
            && subgraph.base_pins.len() == 1
            && matches!(
                subgraph.driver_identity,
                Some(ReducedProjectDriverIdentity::SymbolPin { .. })
            )
        {
            let mut connection = reduced_subgraph_driver_connection(subgraph);

            if connection.name.contains("Net-(") {
                connection.name = reduced_force_no_connect_net_name(&connection.name);
                connection.local_name = reduced_force_no_connect_net_name(&connection.local_name);
                connection.full_local_name =
                    reduced_force_no_connect_net_name(&connection.full_local_name);
                updates.push(PostPropagationUpdate::ForceNoConnectName { index, connection });
            }
        }

        if matches!(
            subgraph.driver_identity,
            Some(ReducedProjectDriverIdentity::SheetPin { .. })
        ) && subgraph
            .driver_connection
            .as_ref()
            .is_some_and(|connection| {
                matches!(
                    connection.connection_type,
                    ReducedProjectConnectionType::Net
                )
            })
        {
            if let Some((connection_type, members)) =
                subgraph.hier_child_indexes.iter().find_map(|child_index| {
                    reduced_subgraphs.get(*child_index).and_then(|child| {
                        let child_connection = child
                            .driver_connection
                            .as_ref()
                            .unwrap_or(&child.resolved_connection);

                        matches!(
                            child_connection.connection_type,
                            ReducedProjectConnectionType::Bus
                                | ReducedProjectConnectionType::BusGroup
                        )
                        .then_some((
                            child_connection.connection_type,
                            child_connection.members.clone(),
                        ))
                    })
                })
            {
                updates.push(PostPropagationUpdate::PromoteSheetPinBus {
                    index,
                    connection_type,
                    members,
                });
            }
        }
    }

    for update in updates {
        match update {
            PostPropagationUpdate::ForceNoConnectName { index, connection } => {
                clone_reduced_connection_into_subgraph(&mut reduced_subgraphs[index], &connection);
            }
            PostPropagationUpdate::PromoteSheetPinBus {
                index,
                connection_type,
                members,
            } => {
                let mut connection = reduced_subgraph_driver_connection(&reduced_subgraphs[index]);
                connection.connection_type = connection_type;
                connection.members = members;
                clone_reduced_connection_into_subgraph(&mut reduced_subgraphs[index], &connection);
            }
        }
    }
}

// Upstream parity: reduced local analogue for the driver-owned naming side of
// `CONNECTION_SUBGRAPH::driverName()` / `SCH_CONNECTION::Name(false)`. This is not a 1:1 KiCad
// current-sheet name resolver because callers without hierarchy context still fall back to raw
// sheet-pin names, but it now routes the reduced driver choice through the same shared owner used
// by the project graph instead of re-deriving the first visible label locally. Remaining
// divergence is fuller sheet-pin shown-text context plus the still-missing live connection object.
fn resolve_reduced_net_name_on_component<FLabel, FSheet>(
    schematic: &Schematic,
    connected_component: &ConnectionComponent,
    sheet_path_prefix: Option<&str>,
    shown_label_text: FLabel,
    shown_sheet_pin_text: FSheet,
) -> Option<String>
where
    FLabel: FnMut(&Label) -> String,
    FSheet: FnMut(&crate::model::Sheet, &crate::model::SheetPin) -> String,
{
    resolve_reduced_driver_name_candidate_on_component(
        schematic,
        connected_component,
        shown_label_text,
        shown_sheet_pin_text,
    )
    .map(|candidate| {
        if let Some(prefix) = sheet_path_prefix {
            reduced_driver_candidate_full_name(&candidate, prefix)
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
pub(crate) fn resolve_reduced_net_name_at<FLabel, FSheet>(
    schematic: &Schematic,
    at: [f64; 2],
    sheet_path_prefix: Option<&str>,
    shown_label_text: FLabel,
    shown_sheet_pin_text: FSheet,
) -> Option<String>
where
    FLabel: FnMut(&Label) -> String,
    FSheet: FnMut(&crate::model::Sheet, &crate::model::SheetPin) -> String,
{
    let connected_component = connection_component_at(schematic, at)?;
    resolve_reduced_net_name_on_component(
        schematic,
        &connected_component,
        sheet_path_prefix,
        shown_label_text,
        shown_sheet_pin_text,
    )
}

// Upstream parity: reduced local analogue for the symbol-pin item lookup half of
// `CONNECTION_GRAPH::GetSubgraphForItem()` on the net-name path. This is not a 1:1 KiCad item map
// because the Rust tree still identifies a placed pin by `(symbol uuid, projected pin at)` instead
// of a live `SCH_PIN*`, but it lets pin-owned ERC/shown-text paths resolve against a symbol-pin
// component owner instead of a raw point query.
pub(crate) fn resolve_reduced_net_name_for_symbol_pin<FLabel, FSheet>(
    schematic: &Schematic,
    symbol: &Symbol,
    at: [f64; 2],
    sheet_path_prefix: Option<&str>,
    shown_label_text: FLabel,
    shown_sheet_pin_text: FSheet,
) -> Option<String>
where
    FLabel: FnMut(&Label) -> String,
    FSheet: FnMut(&crate::model::Sheet, &crate::model::SheetPin) -> String,
{
    let connected_component = connection_component_for_symbol_pin(schematic, symbol, at)?;
    resolve_reduced_net_name_on_component(
        schematic,
        &connected_component,
        sheet_path_prefix,
        shown_label_text,
        shown_sheet_pin_text,
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
        resolve_reduced_net_name_on_component(
            schematic,
            component,
            None,
            |label| shown_label_text(label),
            |_sheet, pin| pin.name.clone(),
        )
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
        LiveReducedConnection, LiveReducedSubgraph, PointKey, ReducedBusMember,
        ReducedBusMemberKind, ReducedHierPortLink, ReducedHierSheetPinLink, ReducedLabelLink,
        ReducedProjectBusNeighborLink, ReducedProjectConnection, ReducedProjectConnectionType,
        ReducedProjectDriverKind, ReducedProjectStrongDriver, ReducedProjectSubgraphEntry,
        build_live_reduced_name_caches, find_first_reduced_project_subgraph_by_name,
        find_reduced_project_subgraph_by_name, rebuild_reduced_project_graph_name_caches,
        recache_live_reduced_subgraph_name, reduced_bus_member_objects,
        refresh_reduced_bus_link_members, refresh_reduced_bus_members_from_neighbor_connections,
        refresh_reduced_global_secondary_driver_promotions,
        refresh_reduced_hierarchy_driver_chains, refresh_reduced_live_bus_link_members,
        refresh_reduced_live_bus_neighbor_drivers, refresh_reduced_live_bus_parent_members,
        refresh_reduced_live_bus_propagation_fixpoint,
        refresh_reduced_live_multiple_bus_parent_names,
        refresh_reduced_live_post_propagation_item_connections,
        refresh_reduced_multiple_bus_parent_names,
        refresh_reduced_post_propagation_item_connections, resolve_reduced_net_name_at,
        resolve_reduced_project_net_at, resolve_reduced_project_subgraph_at,
        resolve_reduced_project_subgraph_for_label,
        resolve_reduced_project_subgraph_for_no_connect,
        resolve_reduced_project_subgraph_for_symbol_pin,
    };
    use crate::core::SchematicProject;
    use crate::loader::load_schematic_tree;
    use crate::model::{LabelKind, SchItem};
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
        let resolved = resolve_reduced_net_name_at(
            &schematic,
            [0.0, 0.0],
            None,
            |label| label.text.clone(),
            |_sheet, pin| pin.name.clone(),
        );

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
        assert_eq!(
            super::reduced_bus_members(&schematic, "USB{PAIR{DP DM} AUX}"),
            vec!["USB.PAIR.DP", "USB.PAIR.DM", "USB.AUX"]
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reduced_bus_member_objects_keep_nested_bus_children() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_bus_member_objects_{}.kicad_sch",
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
  (uuid "73050000-0000-0000-0000-000000000071")
  (paper "A4")
  (bus_alias "PAIR" (members DP DM))
)"#,
        )
        .expect("write schematic");

        let schematic = parse_schematic_file(&path).expect("parse schematic");
        let members = reduced_bus_member_objects(&schematic, "USB{PAIR{DP DM} AUX}");

        assert_eq!(members.len(), 2);
        assert_eq!(members[0].kind, ReducedBusMemberKind::Bus);
        assert_eq!(members[0].local_name, "USB.PAIR{DP DM}");
        assert_eq!(members[0].members.len(), 2);
        assert_eq!(members[0].members[0].vector_index, None);
        assert_eq!(members[0].members[0].full_local_name, "USB.PAIR.DP");
        assert_eq!(members[0].members[1].full_local_name, "USB.PAIR.DM");
        assert_eq!(members[1].kind, ReducedBusMemberKind::Net);
        assert_eq!(members[1].full_local_name, "USB.AUX");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reduced_bus_member_match_uses_vector_index_before_name() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_bus_member_match_vector_{}.kicad_sch",
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
  (uuid "73050000-0000-0000-0000-000000000073")
  (paper "A4")
)"#,
        )
        .expect("write schematic");

        let schematic = parse_schematic_file(&path).expect("parse schematic");
        let bus = reduced_bus_member_objects(&schematic, "DATA[0..3]");

        let search = ReducedBusMember {
            net_code: 0,
            name: "ALT9".to_string(),
            local_name: "ALT9".to_string(),
            full_local_name: "/ALT9".to_string(),
            vector_index: Some(2),
            kind: ReducedBusMemberKind::Net,
            members: Vec::new(),
        };

        let matched = super::match_reduced_bus_member(&bus, &search).expect("matched member");
        assert_eq!(matched.name, "DATA2");
        assert_eq!(matched.vector_index, Some(2));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reduced_bus_subset_cmp_uses_direct_member_objects() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_bus_subset_cmp_{}.kicad_sch",
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
  (uuid "73050000-0000-0000-0000-000000000072")
  (paper "A4")
)"#,
        )
        .expect("write schematic");

        let schematic = parse_schematic_file(&path).expect("parse schematic");

        assert_eq!(
            super::reduced_bus_subset_cmp(&schematic, "USB{PAIR{DP DM} AUX}", "USB{PAIR{DP DM}}"),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            super::reduced_bus_subset_cmp(&schematic, "USB{PAIR{DP DM}}", "USB{PAIR{DP DM} AUX}"),
            std::cmp::Ordering::Greater
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
        assert_eq!(by_sheet.name, "/Child/SIG");
        assert_eq!(by_sheet.resolved_connection.name, "/Child/SIG");
        assert_eq!(by_sheet.resolved_connection.local_name, "SIG");
        assert_eq!(
            by_sheet
                .driver_connection
                .as_ref()
                .expect("driver connection")
                .local_name,
            "SIG"
        );

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
        assert_eq!(by_full_name.resolved_connection.local_name, "SIG");

        let _ = fs::remove_file(root_path);
        let _ = fs::remove_file(child_path);
        let _ = fs::remove_dir(dir);
    }

    #[test]
    fn reduced_project_subgraph_lookup_keeps_first_same_sheet_duplicate_name() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_duplicate_sheet_name_{}.kicad_sch",
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
  (uuid "73050000-0000-0000-0000-000000000201")
  (paper "A4")
  (wire (pts (xy 0 0) (xy 10 0)))
  (label "SIG" (at 10 0 0) (effects (font (size 1 1))))
  (wire (pts (xy 0 20) (xy 10 20)))
  (label "SIG" (at 10 20 0) (effects (font (size 1 1)))))"#,
        )
        .expect("write schematic");

        let loaded = load_schematic_tree(&path).expect("load tree");
        let project = SchematicProject::from_load_result(loaded);
        let root_sheet = project
            .sheet_paths
            .iter()
            .find(|sheet_path| sheet_path.instance_path.is_empty())
            .expect("root sheet path");
        let graph = project.reduced_project_net_graph(false);

        let first_by_point = resolve_reduced_project_subgraph_at(&graph, root_sheet, [10.0, 0.0])
            .expect("first point subgraph");
        let second_by_point = resolve_reduced_project_subgraph_at(&graph, root_sheet, [10.0, 20.0])
            .expect("second point subgraph");
        assert_ne!(first_by_point.subgraph_code, second_by_point.subgraph_code);
        let by_name =
            find_reduced_project_subgraph_by_name(&graph, &first_by_point.name, root_sheet)
                .expect("same-sheet lookup");
        let by_first = find_first_reduced_project_subgraph_by_name(&graph, &first_by_point.name)
            .expect("global same-name lookup");
        assert_eq!(by_name.subgraph_code, first_by_point.subgraph_code);
        assert_eq!(by_first.subgraph_code, first_by_point.subgraph_code);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reduced_project_subgraph_driver_names_include_sheet_pins() {
        let dir = env::temp_dir().join(format!(
            "ki2_connectivity_sheet_pin_drivers_{}",
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
  (paper "A4"))"#,
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
    (uuid "73050000-0000-0000-0000-000000000301")
    (property "Sheetname" "Child" (id 0) (at 0 0 0) (effects (font (size 1 1))))
    (property "Sheetfile" "{}" (id 1) (at 0 0 0) (effects (font (size 1 1))))
    (pin "SIG" input (at 0 5 180) (uuid "73050000-0000-0000-0000-000000000302")))
  (wire (pts (xy 0 5) (xy 10 5))))"#,
                child_path.display()
            ),
        )
        .expect("write root");

        let loaded = load_schematic_tree(&root_path).expect("load tree");
        let project = SchematicProject::from_load_result(loaded);
        let root_sheet = project
            .sheet_paths
            .iter()
            .find(|sheet_path| sheet_path.instance_path.is_empty())
            .expect("root sheet path");
        let graph = project.reduced_project_net_graph(false);

        let by_point =
            resolve_reduced_project_subgraph_at(&graph, root_sheet, [0.0, 5.0]).expect("subgraph");
        assert_eq!(by_point.resolved_connection.local_name, "SIG");
        assert!(by_point.drivers.iter().any(|driver| driver.name == "SIG"));

        let _ = fs::remove_file(root_path);
        let _ = fs::remove_file(child_path);
        let _ = fs::remove_dir(dir);
    }

    #[test]
    fn reduced_project_subgraph_driver_names_use_sheet_pin_shown_text() {
        let dir = env::temp_dir().join(format!(
            "ki2_connectivity_sheet_pin_shown_text_{}",
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
  (paper "A4"))"#,
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
    (uuid "73050000-0000-0000-0000-000000000311")
    (property "Sheetname" "Child" (id 0) (at 0 0 0) (effects (font (size 1 1))))
    (property "Sheetfile" "{}" (id 1) (at 0 0 0) (effects (font (size 1 1))))
    (pin "${{SHEETPATH}}" input (at 0 5 180) (uuid "73050000-0000-0000-0000-000000000312")))
  (wire (pts (xy 0 5) (xy 10 5))))"#,
                child_path.display()
            ),
        )
        .expect("write root");

        let loaded = load_schematic_tree(&root_path).expect("load tree");
        let project = SchematicProject::from_load_result(loaded);
        let root_sheet = project
            .sheet_paths
            .iter()
            .find(|sheet_path| sheet_path.instance_path.is_empty())
            .expect("root sheet path");
        let root_schematic = project
            .schematic(&root_sheet.schematic_path)
            .expect("root schematic");
        let root_sheet_item = root_schematic
            .screen
            .items
            .iter()
            .find_map(|item| match item {
                SchItem::Sheet(sheet) => Some(sheet),
                _ => None,
            })
            .expect("root sheet item");
        let child_sheet =
            super::child_sheet_path_for_sheet(&project.sheet_paths, root_sheet, root_sheet_item)
                .expect("child sheet path");
        let graph = project.reduced_project_net_graph(false);

        let by_point =
            resolve_reduced_project_subgraph_at(&graph, root_sheet, [0.0, 5.0]).expect("subgraph");
        assert_eq!(
            by_point.resolved_connection.local_name,
            child_sheet.instance_path
        );
        assert_eq!(
            by_point
                .driver_connection
                .as_ref()
                .expect("driver connection")
                .full_local_name,
            child_sheet.instance_path
        );
        assert!(
            by_point
                .drivers
                .iter()
                .any(|driver| driver.name == child_sheet.instance_path)
        );

        let _ = fs::remove_file(root_path);
        let _ = fs::remove_file(child_path);
        let _ = fs::remove_dir(dir);
    }

    #[test]
    fn reduced_project_subgraphs_link_hierarchical_parent_chains() {
        let dir = env::temp_dir().join(format!(
            "ki2_connectivity_hier_parent_links_{}",
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
  (hierarchical_label "SIG" (shape input) (at 0 5 0) (effects (font (size 1 1))))
  (wire (pts (xy 0 5) (xy 10 5)))
  (label "SIG" (at 10 5 0) (effects (font (size 1 1)))))"#,
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
    (uuid "73050000-0000-0000-0000-000000000321")
    (property "Sheetname" "Child" (id 0) (at 0 0 0) (effects (font (size 1 1))))
    (property "Sheetfile" "{}" (id 1) (at 0 0 0) (effects (font (size 1 1))))
    (pin "SIG" input (at 0 5 180) (uuid "73050000-0000-0000-0000-000000000322")))
  (wire (pts (xy 0 5) (xy 10 5)))
  (label "SIG" (at 10 5 0) (effects (font (size 1 1)))))"#,
                child_path.display()
            ),
        )
        .expect("write root");

        let loaded = load_schematic_tree(&root_path).expect("load tree");
        let project = SchematicProject::from_load_result(loaded);
        let root_sheet = project
            .sheet_paths
            .iter()
            .find(|sheet_path| sheet_path.instance_path.is_empty())
            .expect("root sheet path");
        let child_sheet = project
            .sheet_paths
            .iter()
            .find(|sheet_path| sheet_path.instance_path != root_sheet.instance_path)
            .expect("child sheet path");
        let graph = project.reduced_project_net_graph(false);

        let root_subgraph = resolve_reduced_project_subgraph_at(&graph, root_sheet, [0.0, 5.0])
            .expect("root subgraph");
        let child_subgraph = resolve_reduced_project_subgraph_at(&graph, child_sheet, [0.0, 5.0])
            .expect("child subgraph");
        let root_index = graph
            .subgraphs
            .iter()
            .position(|candidate| {
                candidate.subgraph_code == root_subgraph.subgraph_code
                    && candidate.sheet_instance_path == root_subgraph.sheet_instance_path
            })
            .expect("root index");
        let child_index = graph
            .subgraphs
            .iter()
            .position(|candidate| {
                candidate.subgraph_code == child_subgraph.subgraph_code
                    && candidate.sheet_instance_path == child_subgraph.sheet_instance_path
            })
            .expect("child index");

        assert_eq!(
            graph.subgraphs[child_index].hier_parent_index,
            Some(root_index),
            "root hier_sheet_pins={:?} child hier_ports={:?} child sheet path={} root sheet path={}",
            graph.subgraphs[root_index].hier_sheet_pins,
            graph.subgraphs[child_index].hier_ports,
            graph.subgraphs[child_index].sheet_instance_path,
            graph.subgraphs[root_index].sheet_instance_path,
        );
        assert!(
            graph.subgraphs[root_index]
                .hier_child_indexes
                .iter()
                .any(|index| *index == child_index)
        );

        let _ = fs::remove_file(root_path);
        let _ = fs::remove_file(child_path);
        let _ = fs::remove_dir(dir);
    }

    #[test]
    fn reduced_project_subgraph_lookup_accepts_vector_bus_prefix_alias() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_bus_prefix_lookup_{}.kicad_sch",
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
  (paper "A4")
  (bus (pts (xy 0 0) (xy 10 0)))
  (global_label "DATA[0..7]" (shape input) (at 0 0 0) (effects (font (size 1 1)))))"#,
        )
        .expect("write schematic");

        let loaded = load_schematic_tree(&path).expect("load tree");
        let project = SchematicProject::from_load_result(loaded);
        let graph = project.reduced_project_net_graph(false);

        let by_full_name = find_first_reduced_project_subgraph_by_name(&graph, "DATA[0..7]")
            .expect("full-name bus subgraph");
        let by_prefix =
            find_first_reduced_project_subgraph_by_name(&graph, "DATA[]").expect("prefix bus");

        assert_eq!(by_prefix.subgraph_code, by_full_name.subgraph_code);
        assert_eq!(by_prefix.name, by_full_name.name);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reduced_project_net_codes_follow_first_seen_net_name_order() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_net_code_order_{}.kicad_sch",
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
  (paper "A4")
  (wire (pts (xy 0 0) (xy 10 0)))
  (global_label "NET10" (shape input) (at 10 0 0) (effects (font (size 1 1))))
  (wire (pts (xy 0 20) (xy 10 20)))
  (global_label "NET2" (shape input) (at 10 20 0) (effects (font (size 1 1)))))"#,
        )
        .expect("write schematic");

        let loaded = load_schematic_tree(&path).expect("load tree");
        let root_sheet = loaded
            .sheet_paths
            .iter()
            .find(|sheet_path| sheet_path.instance_path.is_empty())
            .cloned()
            .expect("root sheet");
        let project = SchematicProject::from_load_result(loaded);
        let graph = project.reduced_project_net_graph(false);

        let first_net =
            resolve_reduced_project_net_at(&graph, &root_sheet, [10.0, 0.0]).expect("first net");
        let second_net =
            resolve_reduced_project_net_at(&graph, &root_sheet, [10.0, 20.0]).expect("second net");

        assert_eq!(first_net.name, "NET10");
        assert_eq!(second_net.name, "NET2");
        assert_eq!(first_net.code, 1);
        assert_eq!(second_net.code, 2);
        let first_subgraph = graph
            .subgraphs
            .iter()
            .find(|subgraph| subgraph.name == "NET10")
            .expect("first subgraph");
        let second_subgraph = graph
            .subgraphs
            .iter()
            .find(|subgraph| subgraph.name == "NET2")
            .expect("second subgraph");
        assert_eq!(first_subgraph.resolved_connection.net_code, 1);
        assert_eq!(second_subgraph.resolved_connection.net_code, 2);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reduced_bus_members_get_shared_connection_net_codes() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_bus_member_net_codes_{}.kicad_sch",
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
  (paper "A4")
  (bus (pts (xy 0 0) (xy 10 0)))
  (global_label "DATA[0..1]" (shape input) (at 0 0 0) (effects (font (size 1 1)))))"#,
        )
        .expect("write schematic");

        let loaded = load_schematic_tree(&path).expect("load tree");
        let project = SchematicProject::from_load_result(loaded);
        let graph = project.reduced_project_net_graph(false);
        let bus_subgraph = graph
            .subgraphs
            .iter()
            .find(|subgraph| subgraph.name == "DATA[0..1]")
            .expect("bus subgraph");

        assert_eq!(bus_subgraph.resolved_connection.net_code, 1);
        assert_eq!(bus_subgraph.resolved_connection.members.len(), 2);
        assert_eq!(
            bus_subgraph.resolved_connection.members[0].full_local_name,
            "DATA0"
        );
        assert_eq!(bus_subgraph.resolved_connection.members[0].net_code, 2);
        assert_eq!(
            bus_subgraph.resolved_connection.members[1].full_local_name,
            "DATA1"
        );
        assert_eq!(bus_subgraph.resolved_connection.members[1].net_code, 3);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reduced_project_pin_identity_covers_multi_pin_power_symbols() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_power_pin_identity_{}.kicad_sch",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));

        fs::write(
            &path,
            r##"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (lib_symbols
    (symbol "power:SplitGround"
      (power)
      (property "Reference" "#PWR" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "SplitGround" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "SplitGround_1_1"
        (pin power_in line (at 0 0 180) (length 2.54)
          (name "GND" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1)))))
        (pin power_in line (at 10 0 0) (length 2.54)
          (name "AGND" (effects (font (size 1 1))))
          (number "2" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "power:SplitGround")
    (uuid "73050000-0000-0000-0000-000000000201")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "#PWR1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "SplitGround" (at 0 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy -10 0)))
  (global_label "VCC" (shape input) (at -10 0 0) (effects (font (size 1 1))))
  (wire (pts (xy 10 0) (xy 20 0)))
  (global_label "GND" (shape input) (at 20 0 0) (effects (font (size 1 1)))))"##,
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
        let symbol = schematic
            .screen
            .items
            .iter()
            .find_map(|item| match item {
                SchItem::Symbol(symbol) => Some(symbol),
                _ => None,
            })
            .expect("power symbol");

        let gnd_pin = crate::connectivity::resolve_reduced_project_net_for_symbol_pin(
            &graph,
            &sheet_path,
            symbol,
            [0.0, 0.0],
            Some("GND"),
        )
        .expect("gnd pin graph identity");
        let agnd_pin = crate::connectivity::resolve_reduced_project_net_for_symbol_pin(
            &graph,
            &sheet_path,
            symbol,
            [10.0, 0.0],
            Some("AGND"),
        )
        .expect("agnd pin graph identity");

        assert_eq!(gnd_pin.name, "VCC");
        assert_eq!(agnd_pin.name, "VCC");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reduced_project_subgraphs_track_bus_neighbor_links() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_bus_neighbor_links_{}.kicad_sch",
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
  (uuid "73050000-0000-0000-0000-000000000701")
  (paper "A4")
  (wire (pts (xy 0 0) (xy 10 0)))
  (label "DATA[1..0]" (at 10 0 0) (effects (font (size 1 1))))
  (wire (pts (xy 0 20) (xy 10 20)))
  (label "DATA0" (at 10 20 0) (effects (font (size 1 1)))))"#,
        )
        .expect("write schematic");

        let loaded = load_schematic_tree(&path).expect("load tree");
        let project = SchematicProject::from_load_result(loaded);
        let root_sheet = project
            .sheet_paths
            .iter()
            .find(|sheet_path| sheet_path.instance_path.is_empty())
            .expect("root sheet path");
        let graph = project.reduced_project_net_graph(false);

        let bus = resolve_reduced_project_subgraph_at(&graph, root_sheet, [10.0, 0.0])
            .expect("bus subgraph");
        let net = resolve_reduced_project_subgraph_at(&graph, root_sheet, [10.0, 20.0])
            .expect("net subgraph");

        assert!(bus.bus_neighbor_links.iter().any(|link| {
            link.member.full_local_name == "/DATA0" && link.subgraph_index == net.subgraph_code - 1
        }));
        assert!(net.bus_parent_links.iter().any(|link| {
            link.member.full_local_name == "/DATA0" && link.subgraph_index == bus.subgraph_code - 1
        }));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reduced_bus_link_refresh_matches_vector_members_by_index() {
        let mut parent = ReducedProjectSubgraphEntry {
            subgraph_code: 1,
            code: 1,
            name: "BUS".to_string(),
            resolved_connection: ReducedProjectConnection {
                net_code: 0,
                connection_type: ReducedProjectConnectionType::Bus,
                name: "BUS".to_string(),
                local_name: "BUS".to_string(),
                full_local_name: "/BUS".to_string(),
                sheet_instance_path: String::new(),
                members: vec![
                    ReducedBusMember {
                        net_code: 0,
                        name: "RENAMED0".to_string(),
                        local_name: "RENAMED0".to_string(),
                        full_local_name: "/RENAMED0".to_string(),
                        vector_index: Some(0),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    },
                    ReducedBusMember {
                        net_code: 0,
                        name: "RENAMED1".to_string(),
                        local_name: "RENAMED1".to_string(),
                        full_local_name: "/RENAMED1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    },
                ],
            },
            driver_connection: None,
            driver_identity: None,
            drivers: Vec::new(),
            non_bus_driver_priority: None,
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: String::new(),
            anchor: PointKey(0, 0),
            points: Vec::new(),
            nodes: Vec::new(),
            base_pins: Vec::new(),
            label_links: Vec::new(),
            no_connect_points: Vec::new(),
            hier_sheet_pins: Vec::new(),
            hier_ports: Vec::new(),
            bus_members: Vec::new(),
            bus_items: Vec::new(),
            wire_items: Vec::new(),
            bus_neighbor_links: vec![ReducedProjectBusNeighborLink {
                member: ReducedBusMember {
                    net_code: 0,
                    name: "OLD1".to_string(),
                    local_name: "OLD1".to_string(),
                    full_local_name: "/OLD1".to_string(),
                    vector_index: Some(1),
                    kind: ReducedBusMemberKind::Net,
                    members: Vec::new(),
                },
                subgraph_index: 1,
            }],
            bus_parent_links: Vec::new(),
            bus_parent_indexes: vec![1],
            hier_parent_index: None,
            hier_child_indexes: Vec::new(),
        };
        let child = ReducedProjectSubgraphEntry {
            subgraph_code: 2,
            code: 2,
            name: "/OLD1".to_string(),
            resolved_connection: ReducedProjectConnection {
                net_code: 0,
                connection_type: ReducedProjectConnectionType::Net,
                name: "/OLD1".to_string(),
                local_name: "OLD1".to_string(),
                full_local_name: "/OLD1".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
            driver_connection: None,
            driver_identity: None,
            drivers: Vec::new(),
            non_bus_driver_priority: None,
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: String::new(),
            anchor: PointKey(0, 0),
            points: Vec::new(),
            nodes: Vec::new(),
            base_pins: Vec::new(),
            label_links: Vec::new(),
            no_connect_points: Vec::new(),
            hier_sheet_pins: Vec::new(),
            hier_ports: Vec::new(),
            bus_members: Vec::new(),
            bus_items: Vec::new(),
            wire_items: Vec::new(),
            bus_neighbor_links: Vec::new(),
            bus_parent_links: vec![ReducedProjectBusNeighborLink {
                member: ReducedBusMember {
                    net_code: 0,
                    name: "OLD1".to_string(),
                    local_name: "OLD1".to_string(),
                    full_local_name: "/OLD1".to_string(),
                    vector_index: Some(1),
                    kind: ReducedBusMemberKind::Net,
                    members: Vec::new(),
                },
                subgraph_index: 0,
            }],
            bus_parent_indexes: vec![0],
            hier_parent_index: None,
            hier_child_indexes: Vec::new(),
        };

        let mut graph = vec![parent.clone(), child];
        refresh_reduced_bus_link_members(&mut graph);
        parent = graph.remove(0);

        assert_eq!(parent.bus_neighbor_links[0].member.name, "RENAMED1");
        assert_eq!(parent.bus_neighbor_links[0].member.vector_index, Some(1));
    }

    #[test]
    fn reduced_live_bus_link_refresh_matches_vector_members_by_index() {
        let mut parent = ReducedProjectSubgraphEntry {
            subgraph_code: 1,
            code: 1,
            name: "BUS".to_string(),
            resolved_connection: ReducedProjectConnection {
                net_code: 0,
                connection_type: ReducedProjectConnectionType::Bus,
                name: "BUS".to_string(),
                local_name: "BUS".to_string(),
                full_local_name: "/BUS".to_string(),
                sheet_instance_path: String::new(),
                members: vec![
                    ReducedBusMember {
                        net_code: 0,
                        name: "RENAMED0".to_string(),
                        local_name: "RENAMED0".to_string(),
                        full_local_name: "/RENAMED0".to_string(),
                        vector_index: Some(0),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    },
                    ReducedBusMember {
                        net_code: 0,
                        name: "RENAMED1".to_string(),
                        local_name: "RENAMED1".to_string(),
                        full_local_name: "/RENAMED1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    },
                ],
            },
            driver_connection: None,
            driver_identity: None,
            drivers: Vec::new(),
            non_bus_driver_priority: None,
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: String::new(),
            anchor: PointKey(0, 0),
            points: Vec::new(),
            nodes: Vec::new(),
            base_pins: Vec::new(),
            label_links: Vec::new(),
            no_connect_points: Vec::new(),
            hier_sheet_pins: Vec::new(),
            hier_ports: Vec::new(),
            bus_members: Vec::new(),
            bus_items: Vec::new(),
            wire_items: Vec::new(),
            bus_neighbor_links: vec![ReducedProjectBusNeighborLink {
                member: ReducedBusMember {
                    net_code: 0,
                    name: "OLD1".to_string(),
                    local_name: "OLD1".to_string(),
                    full_local_name: "/OLD1".to_string(),
                    vector_index: Some(1),
                    kind: ReducedBusMemberKind::Net,
                    members: Vec::new(),
                },
                subgraph_index: 1,
            }],
            bus_parent_links: Vec::new(),
            bus_parent_indexes: vec![1],
            hier_parent_index: None,
            hier_child_indexes: Vec::new(),
        };
        let child = ReducedProjectSubgraphEntry {
            subgraph_code: 2,
            code: 2,
            name: "/OLD1".to_string(),
            resolved_connection: ReducedProjectConnection {
                net_code: 0,
                connection_type: ReducedProjectConnectionType::Net,
                name: "/OLD1".to_string(),
                local_name: "OLD1".to_string(),
                full_local_name: "/OLD1".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
            driver_connection: None,
            driver_identity: None,
            drivers: Vec::new(),
            non_bus_driver_priority: None,
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: String::new(),
            anchor: PointKey(0, 0),
            points: Vec::new(),
            nodes: Vec::new(),
            base_pins: Vec::new(),
            label_links: Vec::new(),
            no_connect_points: Vec::new(),
            hier_sheet_pins: Vec::new(),
            hier_ports: Vec::new(),
            bus_members: Vec::new(),
            bus_items: Vec::new(),
            wire_items: Vec::new(),
            bus_neighbor_links: Vec::new(),
            bus_parent_links: vec![ReducedProjectBusNeighborLink {
                member: ReducedBusMember {
                    net_code: 0,
                    name: "OLD1".to_string(),
                    local_name: "OLD1".to_string(),
                    full_local_name: "/OLD1".to_string(),
                    vector_index: Some(1),
                    kind: ReducedBusMemberKind::Net,
                    members: Vec::new(),
                },
                subgraph_index: 0,
            }],
            bus_parent_indexes: vec![0],
            hier_parent_index: None,
            hier_child_indexes: Vec::new(),
        };

        let mut graph = vec![parent.clone(), child];
        refresh_reduced_live_bus_link_members(&mut graph);
        parent = graph.remove(0);

        assert_eq!(parent.bus_neighbor_links[0].member.name, "RENAMED1");
        assert_eq!(parent.bus_neighbor_links[0].member.vector_index, Some(1));
    }

    #[test]
    fn reduced_live_bus_fixpoint_replays_renamed_member_to_second_neighbor() {
        let mut graph = vec![
            ReducedProjectSubgraphEntry {
                subgraph_code: 1,
                code: 1,
                name: "/BUS".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS".to_string(),
                    local_name: "BUS".to_string(),
                    full_local_name: "/BUS".to_string(),
                    sheet_instance_path: String::new(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        vector_index: None,
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS".to_string(),
                    local_name: "BUS".to_string(),
                    full_local_name: "/BUS".to_string(),
                    sheet_instance_path: String::new(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        vector_index: None,
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                }),
                driver_identity: None,
                drivers: Vec::new(),
                non_bus_driver_priority: None,
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: vec![
                    ReducedProjectBusNeighborLink {
                        member: ReducedBusMember {
                            net_code: 0,
                            name: "OLD1".to_string(),
                            local_name: "OLD1".to_string(),
                            full_local_name: "/OLD1".to_string(),
                            vector_index: None,
                            kind: ReducedBusMemberKind::Net,
                            members: Vec::new(),
                        },
                        subgraph_index: 1,
                    },
                    ReducedProjectBusNeighborLink {
                        member: ReducedBusMember {
                            net_code: 0,
                            name: "OLD1".to_string(),
                            local_name: "OLD1".to_string(),
                            full_local_name: "/OLD1".to_string(),
                            vector_index: None,
                            kind: ReducedBusMemberKind::Net,
                            members: Vec::new(),
                        },
                        subgraph_index: 2,
                    },
                ],
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
            ReducedProjectSubgraphEntry {
                subgraph_code: 2,
                code: 2,
                name: "/PWR".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "/PWR".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "/PWR".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                }),
                driver_identity: None,
                drivers: vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: 6,
                    name: "PWR".to_string(),
                    full_name: "/PWR".to_string(),
                }],
                non_bus_driver_priority: Some(6),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(1, 1),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: vec![ReducedProjectBusNeighborLink {
                    member: ReducedBusMember {
                        net_code: 0,
                        name: "OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        vector_index: None,
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    },
                    subgraph_index: 0,
                }],
                bus_parent_indexes: vec![0],
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
            ReducedProjectSubgraphEntry {
                subgraph_code: 3,
                code: 3,
                name: "/OLD1".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/OLD1".to_string(),
                    local_name: "OLD1".to_string(),
                    full_local_name: "/OLD1".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/OLD1".to_string(),
                    local_name: "OLD1".to_string(),
                    full_local_name: "/OLD1".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                }),
                driver_identity: None,
                drivers: Vec::new(),
                non_bus_driver_priority: None,
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(2, 2),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: vec![ReducedProjectBusNeighborLink {
                    member: ReducedBusMember {
                        net_code: 0,
                        name: "OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        vector_index: None,
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    },
                    subgraph_index: 0,
                }],
                bus_parent_indexes: vec![0],
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
        ];

        refresh_reduced_live_bus_propagation_fixpoint(&mut graph);

        assert_eq!(
            graph[0].resolved_connection.members[0].full_local_name,
            "/PWR"
        );
        assert_eq!(graph[2].resolved_connection.full_local_name, "/PWR");
    }

    #[test]
    fn recache_live_reduced_subgraph_name_updates_live_name_indexes() {
        let mut live_subgraphs = vec![
            LiveReducedSubgraph {
                source_index: 0,
                driver_connection: LiveReducedConnection::new(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/OLD".to_string(),
                    local_name: "OLD".to_string(),
                    full_local_name: "/OLD".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                }),
                driver_priority: 0,
                driver_identity: None,
                strong_driver_count: 0,
                sheet_instance_path: String::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                base_pin_count: 0,
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
                has_hier_pins: false,
                has_hier_ports: false,
                dirty: true,
            },
            LiveReducedSubgraph {
                source_index: 1,
                driver_connection: LiveReducedConnection::new(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/KEEP".to_string(),
                    local_name: "KEEP".to_string(),
                    full_local_name: "/KEEP".to_string(),
                    sheet_instance_path: "/child".to_string(),
                    members: Vec::new(),
                }),
                driver_priority: 0,
                driver_identity: None,
                strong_driver_count: 0,
                sheet_instance_path: "/child".to_string(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                base_pin_count: 0,
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
                has_hier_pins: false,
                has_hier_ports: false,
                dirty: true,
            },
        ];

        let (mut by_name, mut by_sheet_and_name) = build_live_reduced_name_caches(&live_subgraphs);

        live_subgraphs[0].driver_connection.connection.name = "/NEW".to_string();
        live_subgraphs[0].driver_connection.connection.local_name = "NEW".to_string();
        live_subgraphs[0]
            .driver_connection
            .connection
            .full_local_name = "/NEW".to_string();

        recache_live_reduced_subgraph_name(
            &live_subgraphs,
            &mut by_name,
            &mut by_sheet_and_name,
            0,
            "/OLD",
        );

        assert_eq!(by_name.get("/OLD"), Some(&Vec::new()));
        assert_eq!(by_name.get("/NEW"), Some(&vec![0]));
        assert_eq!(
            by_sheet_and_name.get(&(String::new(), "/NEW".to_string())),
            Some(&vec![0])
        );
    }

    #[test]
    fn reduced_multiple_bus_parents_refresh_subgraph_names() {
        let connection = ReducedProjectConnection {
            net_code: 0,
            connection_type: ReducedProjectConnectionType::Net,
            name: "/RENAMED1".to_string(),
            local_name: "RENAMED1".to_string(),
            full_local_name: "/RENAMED1".to_string(),
            sheet_instance_path: String::new(),
            members: Vec::new(),
        };
        let mut graph = vec![
            ReducedProjectSubgraphEntry {
                subgraph_code: 1,
                code: 1,
                name: "/BUS_A".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS_A".to_string(),
                    local_name: "BUS_A".to_string(),
                    full_local_name: "/BUS_A".to_string(),
                    sheet_instance_path: String::new(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS_A".to_string(),
                    local_name: "BUS_A".to_string(),
                    full_local_name: "/BUS_A".to_string(),
                    sheet_instance_path: String::new(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                }),
                driver_identity: None,
                drivers: Vec::new(),
                non_bus_driver_priority: None,
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: vec![ReducedHierSheetPinLink {
                    at: PointKey(0, 0),
                    child_sheet_uuid: Some("child-sheet".to_string()),
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "/ROOT_SIG".to_string(),
                        local_name: "ROOT_SIG".to_string(),
                        full_local_name: "/ROOT_SIG".to_string(),
                        sheet_instance_path: String::new(),
                        members: Vec::new(),
                    },
                }],
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
            ReducedProjectSubgraphEntry {
                subgraph_code: 2,
                code: 2,
                name: "/BUS_B".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS_B".to_string(),
                    local_name: "BUS_B".to_string(),
                    full_local_name: "/BUS_B".to_string(),
                    sheet_instance_path: String::new(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS_B".to_string(),
                    local_name: "BUS_B".to_string(),
                    full_local_name: "/BUS_B".to_string(),
                    sheet_instance_path: String::new(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                }),
                driver_identity: None,
                drivers: Vec::new(),
                non_bus_driver_priority: None,
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: vec![ReducedHierPortLink {
                    at: PointKey(0, 0),
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "/Child/GLOBAL_SIG".to_string(),
                        local_name: "ROOT_SIG".to_string(),
                        full_local_name: "/Child/GLOBAL_SIG".to_string(),
                        sheet_instance_path: "/child".to_string(),
                        members: Vec::new(),
                    },
                }],
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
            ReducedProjectSubgraphEntry {
                subgraph_code: 3,
                code: 3,
                name: "/RENAMED1".to_string(),
                resolved_connection: connection.clone(),
                driver_connection: Some(connection.clone()),
                driver_identity: None,
                drivers: Vec::new(),
                non_bus_driver_priority: None,
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: vec![ReducedHierSheetPinLink {
                    at: PointKey(0, 0),
                    child_sheet_uuid: Some("child-sheet".to_string()),
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "/ROOT_SIG".to_string(),
                        local_name: "ROOT_SIG".to_string(),
                        full_local_name: "/ROOT_SIG".to_string(),
                        sheet_instance_path: String::new(),
                        members: Vec::new(),
                    },
                }],
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: vec![
                    ReducedProjectBusNeighborLink {
                        member: ReducedBusMember {
                            net_code: 0,
                            name: "OLD1".to_string(),
                            local_name: "OLD1".to_string(),
                            full_local_name: "/OLD1".to_string(),
                            vector_index: Some(1),
                            kind: ReducedBusMemberKind::Net,
                            members: Vec::new(),
                        },
                        subgraph_index: 0,
                    },
                    ReducedProjectBusNeighborLink {
                        member: ReducedBusMember {
                            net_code: 0,
                            name: "OLD1".to_string(),
                            local_name: "OLD1".to_string(),
                            full_local_name: "/OLD1".to_string(),
                            vector_index: Some(1),
                            kind: ReducedBusMemberKind::Net,
                            members: Vec::new(),
                        },
                        subgraph_index: 1,
                    },
                ],
                bus_parent_indexes: vec![0, 1],
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
            ReducedProjectSubgraphEntry {
                subgraph_code: 4,
                code: 4,
                name: "/OLD1".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/OLD1".to_string(),
                    local_name: "OLD1".to_string(),
                    full_local_name: "/OLD1".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/OLD1".to_string(),
                    local_name: "OLD1".to_string(),
                    full_local_name: "/OLD1".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                }),
                driver_identity: None,
                drivers: Vec::new(),
                non_bus_driver_priority: None,
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: vec![ReducedLabelLink {
                    at: PointKey(0, 0),
                    kind: LabelKind::Local,
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "/OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        sheet_instance_path: String::new(),
                        members: Vec::new(),
                    },
                }],
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: vec![ReducedHierPortLink {
                    at: PointKey(0, 0),
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "/Child/GLOBAL_SIG".to_string(),
                        local_name: "ROOT_SIG".to_string(),
                        full_local_name: "/Child/GLOBAL_SIG".to_string(),
                        sheet_instance_path: "/child".to_string(),
                        members: Vec::new(),
                    },
                }],
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
        ];

        refresh_reduced_multiple_bus_parent_names(&mut graph);

        assert_eq!(graph[0].resolved_connection.members[0].name, "RENAMED1");
        assert_eq!(
            graph[0].resolved_connection.members[0].full_local_name,
            "/RENAMED1"
        );
        assert_eq!(graph[1].resolved_connection.members[0].name, "RENAMED1");
        assert_eq!(
            graph[1].resolved_connection.members[0].full_local_name,
            "/RENAMED1"
        );
        assert_eq!(graph[3].name, "/RENAMED1");
        assert_eq!(graph[3].resolved_connection.name, "/RENAMED1");
        assert_eq!(
            graph[3].label_links[0].connection.full_local_name,
            "/RENAMED1"
        );

        let (by_name, by_sheet_and_name) = rebuild_reduced_project_graph_name_caches(&mut graph);
        assert_eq!(graph[2].code, graph[3].code);
        assert_eq!(by_name["/RENAMED1"], vec![2, 3]);
        assert_eq!(
            by_sheet_and_name[&(String::new(), "/RENAMED1".to_string())],
            vec![2, 3]
        );
    }

    #[test]
    fn reduced_bus_member_refresh_uses_child_final_connection() {
        let mut graph = vec![
            ReducedProjectSubgraphEntry {
                subgraph_code: 1,
                code: 1,
                name: "/BUS".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS".to_string(),
                    local_name: "BUS".to_string(),
                    full_local_name: "/BUS".to_string(),
                    sheet_instance_path: String::new(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS".to_string(),
                    local_name: "BUS".to_string(),
                    full_local_name: "/BUS".to_string(),
                    sheet_instance_path: String::new(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                }),
                driver_identity: None,
                drivers: Vec::new(),
                non_bus_driver_priority: None,
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: vec![ReducedHierSheetPinLink {
                    at: PointKey(0, 0),
                    child_sheet_uuid: Some("child-sheet".to_string()),
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "/ROOT_SIG".to_string(),
                        local_name: "ROOT_SIG".to_string(),
                        full_local_name: "/ROOT_SIG".to_string(),
                        sheet_instance_path: String::new(),
                        members: Vec::new(),
                    },
                }],
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
            ReducedProjectSubgraphEntry {
                subgraph_code: 2,
                code: 2,
                name: "/PWR".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "/PWR".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "/PWR".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                }),
                driver_identity: None,
                drivers: Vec::new(),
                non_bus_driver_priority: None,
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: vec![ReducedHierPortLink {
                    at: PointKey(0, 0),
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "/Child/GLOBAL_SIG".to_string(),
                        local_name: "ROOT_SIG".to_string(),
                        full_local_name: "/Child/GLOBAL_SIG".to_string(),
                        sheet_instance_path: "/child".to_string(),
                        members: Vec::new(),
                    },
                }],
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: vec![ReducedProjectBusNeighborLink {
                    member: ReducedBusMember {
                        net_code: 0,
                        name: "OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    },
                    subgraph_index: 0,
                }],
                bus_parent_indexes: vec![0],
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
        ];

        refresh_reduced_bus_members_from_neighbor_connections(&mut graph);
        refresh_reduced_bus_link_members(&mut graph);

        assert_eq!(graph[0].resolved_connection.members[0].name, "PWR");
        assert_eq!(
            graph[0].resolved_connection.members[0].full_local_name,
            "/PWR"
        );
        assert_eq!(graph[1].bus_parent_links[0].member.full_local_name, "/PWR");
        assert_eq!(
            graph[0].bus_neighbor_links[0].member.full_local_name,
            "/PWR"
        );
    }

    #[test]
    fn reduced_bus_member_refresh_preserves_other_sheet_override() {
        let mut graph = vec![
            ReducedProjectSubgraphEntry {
                subgraph_code: 1,
                code: 1,
                name: "/BUS".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS".to_string(),
                    local_name: "BUS".to_string(),
                    full_local_name: "/BUS".to_string(),
                    sheet_instance_path: String::new(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS".to_string(),
                    local_name: "BUS".to_string(),
                    full_local_name: "/BUS".to_string(),
                    sheet_instance_path: String::new(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                }),
                driver_identity: None,
                drivers: Vec::new(),
                non_bus_driver_priority: None,
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: vec![ReducedHierSheetPinLink {
                    at: PointKey(0, 0),
                    child_sheet_uuid: Some("child-sheet".to_string()),
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "/ROOT_SIG".to_string(),
                        local_name: "ROOT_SIG".to_string(),
                        full_local_name: "/ROOT_SIG".to_string(),
                        sheet_instance_path: String::new(),
                        members: Vec::new(),
                    },
                }],
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
            ReducedProjectSubgraphEntry {
                subgraph_code: 2,
                code: 2,
                name: "/PWR".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "/PWR".to_string(),
                    sheet_instance_path: "different-sheet".to_string(),
                    members: Vec::new(),
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "/PWR".to_string(),
                    sheet_instance_path: "different-sheet".to_string(),
                    members: Vec::new(),
                }),
                driver_identity: None,
                drivers: Vec::new(),
                non_bus_driver_priority: None,
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: vec![ReducedHierPortLink {
                    at: PointKey(0, 0),
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "/Child/GLOBAL_SIG".to_string(),
                        local_name: "ROOT_SIG".to_string(),
                        full_local_name: "/Child/GLOBAL_SIG".to_string(),
                        sheet_instance_path: "/child".to_string(),
                        members: Vec::new(),
                    },
                }],
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: vec![ReducedProjectBusNeighborLink {
                    member: ReducedBusMember {
                        net_code: 0,
                        name: "OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    },
                    subgraph_index: 0,
                }],
                bus_parent_indexes: vec![0],
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
        ];

        refresh_reduced_bus_members_from_neighbor_connections(&mut graph);
        refresh_reduced_bus_link_members(&mut graph);

        assert_eq!(graph[0].resolved_connection.members[0].name, "OLD1");
        assert_eq!(graph[1].bus_parent_links[0].member.full_local_name, "/OLD1");
    }

    #[test]
    fn reduced_live_bus_neighbors_clone_member_into_neighbor_driver() {
        let mut graph = vec![
            ReducedProjectSubgraphEntry {
                subgraph_code: 1,
                code: 1,
                name: "/BUS".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS".to_string(),
                    local_name: "BUS".to_string(),
                    full_local_name: "/BUS".to_string(),
                    sheet_instance_path: String::new(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "SIG1".to_string(),
                        local_name: "SIG1".to_string(),
                        full_local_name: "/SIG1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS".to_string(),
                    local_name: "BUS".to_string(),
                    full_local_name: "/BUS".to_string(),
                    sheet_instance_path: String::new(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "SIG1".to_string(),
                        local_name: "SIG1".to_string(),
                        full_local_name: "/SIG1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                }),
                driver_identity: None,
                drivers: Vec::new(),
                non_bus_driver_priority: None,
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: vec![ReducedProjectBusNeighborLink {
                    member: ReducedBusMember {
                        net_code: 0,
                        name: "SIG1".to_string(),
                        local_name: "SIG1".to_string(),
                        full_local_name: "/SIG1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    },
                    subgraph_index: 1,
                }],
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
            ReducedProjectSubgraphEntry {
                subgraph_code: 2,
                code: 2,
                name: "/OLD".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/OLD".to_string(),
                    local_name: "OLD".to_string(),
                    full_local_name: "/OLD".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/OLD".to_string(),
                    local_name: "OLD".to_string(),
                    full_local_name: "/OLD".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                }),
                driver_identity: None,
                drivers: Vec::new(),
                non_bus_driver_priority: None,
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(1, 1),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: vec![ReducedProjectBusNeighborLink {
                    member: ReducedBusMember {
                        net_code: 0,
                        name: "SIG1".to_string(),
                        local_name: "SIG1".to_string(),
                        full_local_name: "/SIG1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    },
                    subgraph_index: 0,
                }],
                bus_parent_indexes: vec![0],
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
        ];

        refresh_reduced_live_bus_neighbor_drivers(&mut graph);

        assert_eq!(graph[1].name, "/SIG1");
        assert_eq!(graph[1].resolved_connection.full_local_name, "/SIG1");
        assert_eq!(
            graph[1]
                .driver_connection
                .as_ref()
                .expect("neighbor driver")
                .full_local_name,
            "/SIG1"
        );
    }

    #[test]
    fn reduced_live_bus_neighbors_promote_global_neighbor_into_bus_member() {
        let mut graph = vec![
            ReducedProjectSubgraphEntry {
                subgraph_code: 1,
                code: 1,
                name: "/BUS".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS".to_string(),
                    local_name: "BUS".to_string(),
                    full_local_name: "/BUS".to_string(),
                    sheet_instance_path: String::new(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "SIG1".to_string(),
                        local_name: "SIG1".to_string(),
                        full_local_name: "/SIG1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS".to_string(),
                    local_name: "BUS".to_string(),
                    full_local_name: "/BUS".to_string(),
                    sheet_instance_path: String::new(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "SIG1".to_string(),
                        local_name: "SIG1".to_string(),
                        full_local_name: "/SIG1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                }),
                driver_identity: None,
                drivers: Vec::new(),
                non_bus_driver_priority: None,
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: vec![ReducedProjectBusNeighborLink {
                    member: ReducedBusMember {
                        net_code: 0,
                        name: "SIG1".to_string(),
                        local_name: "SIG1".to_string(),
                        full_local_name: "/SIG1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    },
                    subgraph_index: 1,
                }],
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
            ReducedProjectSubgraphEntry {
                subgraph_code: 2,
                code: 2,
                name: "/PWR".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "/PWR".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "/PWR".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                }),
                driver_identity: None,
                drivers: vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: 6,
                    name: "PWR".to_string(),
                    full_name: "/PWR".to_string(),
                }],
                non_bus_driver_priority: Some(6),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(1, 1),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: vec![ReducedProjectBusNeighborLink {
                    member: ReducedBusMember {
                        net_code: 0,
                        name: "SIG1".to_string(),
                        local_name: "SIG1".to_string(),
                        full_local_name: "/SIG1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    },
                    subgraph_index: 0,
                }],
                bus_parent_indexes: vec![0],
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
        ];

        refresh_reduced_live_bus_neighbor_drivers(&mut graph);

        assert_eq!(
            graph[0].resolved_connection.members[0].full_local_name,
            "/PWR"
        );
        assert_eq!(
            graph[0]
                .driver_connection
                .as_ref()
                .expect("bus driver")
                .members[0]
                .full_local_name,
            "/PWR"
        );
    }

    #[test]
    fn reduced_live_bus_parent_members_refresh_from_child_driver() {
        let mut graph = vec![
            ReducedProjectSubgraphEntry {
                subgraph_code: 1,
                code: 1,
                name: "/BUS".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS".to_string(),
                    local_name: "BUS".to_string(),
                    full_local_name: "/BUS".to_string(),
                    sheet_instance_path: String::new(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS".to_string(),
                    local_name: "BUS".to_string(),
                    full_local_name: "/BUS".to_string(),
                    sheet_instance_path: String::new(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                }),
                driver_identity: None,
                drivers: Vec::new(),
                non_bus_driver_priority: None,
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
            ReducedProjectSubgraphEntry {
                subgraph_code: 2,
                code: 2,
                name: "/PWR".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "/PWR".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "/PWR".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                }),
                driver_identity: None,
                drivers: Vec::new(),
                non_bus_driver_priority: None,
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(1, 1),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: vec![ReducedProjectBusNeighborLink {
                    member: ReducedBusMember {
                        net_code: 0,
                        name: "OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    },
                    subgraph_index: 0,
                }],
                bus_parent_indexes: vec![0],
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
        ];

        refresh_reduced_live_bus_parent_members(&mut graph);

        assert_eq!(
            graph[0].resolved_connection.members[0].full_local_name,
            "/PWR"
        );
        assert_eq!(
            graph[0]
                .driver_connection
                .as_ref()
                .expect("bus driver")
                .members[0]
                .full_local_name,
            "/PWR"
        );
    }

    #[test]
    fn reduced_live_bus_parent_members_preserve_other_sheet_override() {
        let mut graph = vec![
            ReducedProjectSubgraphEntry {
                subgraph_code: 1,
                code: 1,
                name: "/BUS".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS".to_string(),
                    local_name: "BUS".to_string(),
                    full_local_name: "/BUS".to_string(),
                    sheet_instance_path: String::new(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS".to_string(),
                    local_name: "BUS".to_string(),
                    full_local_name: "/BUS".to_string(),
                    sheet_instance_path: String::new(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                }),
                driver_identity: None,
                drivers: Vec::new(),
                non_bus_driver_priority: None,
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
            ReducedProjectSubgraphEntry {
                subgraph_code: 2,
                code: 2,
                name: "/PWR".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "/PWR".to_string(),
                    sheet_instance_path: "different-sheet".to_string(),
                    members: Vec::new(),
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "/PWR".to_string(),
                    sheet_instance_path: "different-sheet".to_string(),
                    members: Vec::new(),
                }),
                driver_identity: None,
                drivers: Vec::new(),
                non_bus_driver_priority: None,
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(1, 1),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: vec![ReducedProjectBusNeighborLink {
                    member: ReducedBusMember {
                        net_code: 0,
                        name: "OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    },
                    subgraph_index: 0,
                }],
                bus_parent_indexes: vec![0],
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
        ];

        refresh_reduced_live_bus_parent_members(&mut graph);

        assert_eq!(
            graph[0].resolved_connection.members[0].full_local_name,
            "/OLD1"
        );
        assert_eq!(
            graph[0]
                .driver_connection
                .as_ref()
                .expect("bus driver")
                .members[0]
                .full_local_name,
            "/OLD1"
        );
    }

    #[test]
    fn reduced_live_multiple_bus_parent_names_refresh_subgraph_names() {
        let connection = ReducedProjectConnection {
            net_code: 0,
            connection_type: ReducedProjectConnectionType::Net,
            name: "/RENAMED1".to_string(),
            local_name: "RENAMED1".to_string(),
            full_local_name: "/RENAMED1".to_string(),
            sheet_instance_path: String::new(),
            members: Vec::new(),
        };
        let mut graph = vec![
            ReducedProjectSubgraphEntry {
                subgraph_code: 1,
                code: 1,
                name: "/BUS_A".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS_A".to_string(),
                    local_name: "BUS_A".to_string(),
                    full_local_name: "/BUS_A".to_string(),
                    sheet_instance_path: String::new(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS_A".to_string(),
                    local_name: "BUS_A".to_string(),
                    full_local_name: "/BUS_A".to_string(),
                    sheet_instance_path: String::new(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                }),
                driver_identity: None,
                drivers: Vec::new(),
                non_bus_driver_priority: None,
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
            ReducedProjectSubgraphEntry {
                subgraph_code: 2,
                code: 2,
                name: "/BUS_B".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS_B".to_string(),
                    local_name: "BUS_B".to_string(),
                    full_local_name: "/BUS_B".to_string(),
                    sheet_instance_path: String::new(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS_B".to_string(),
                    local_name: "BUS_B".to_string(),
                    full_local_name: "/BUS_B".to_string(),
                    sheet_instance_path: String::new(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        vector_index: Some(1),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                }),
                driver_identity: None,
                drivers: Vec::new(),
                non_bus_driver_priority: None,
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
            ReducedProjectSubgraphEntry {
                subgraph_code: 3,
                code: 3,
                name: "/RENAMED1".to_string(),
                resolved_connection: connection.clone(),
                driver_connection: Some(connection.clone()),
                driver_identity: None,
                drivers: Vec::new(),
                non_bus_driver_priority: None,
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: vec![
                    ReducedProjectBusNeighborLink {
                        member: ReducedBusMember {
                            net_code: 0,
                            name: "OLD1".to_string(),
                            local_name: "OLD1".to_string(),
                            full_local_name: "/OLD1".to_string(),
                            vector_index: Some(1),
                            kind: ReducedBusMemberKind::Net,
                            members: Vec::new(),
                        },
                        subgraph_index: 0,
                    },
                    ReducedProjectBusNeighborLink {
                        member: ReducedBusMember {
                            net_code: 0,
                            name: "OLD1".to_string(),
                            local_name: "OLD1".to_string(),
                            full_local_name: "/OLD1".to_string(),
                            vector_index: Some(1),
                            kind: ReducedBusMemberKind::Net,
                            members: Vec::new(),
                        },
                        subgraph_index: 1,
                    },
                ],
                bus_parent_indexes: vec![0, 1],
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
            ReducedProjectSubgraphEntry {
                subgraph_code: 4,
                code: 4,
                name: "/OLD1".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/OLD1".to_string(),
                    local_name: "OLD1".to_string(),
                    full_local_name: "/OLD1".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/OLD1".to_string(),
                    local_name: "OLD1".to_string(),
                    full_local_name: "/OLD1".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                }),
                driver_identity: None,
                drivers: Vec::new(),
                non_bus_driver_priority: None,
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: vec![ReducedLabelLink {
                    at: PointKey(0, 0),
                    kind: LabelKind::Local,
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "/OLD1".to_string(),
                        local_name: "OLD1".to_string(),
                        full_local_name: "/OLD1".to_string(),
                        sheet_instance_path: String::new(),
                        members: Vec::new(),
                    },
                }],
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
        ];

        refresh_reduced_live_multiple_bus_parent_names(&mut graph);

        assert_eq!(
            graph[0].resolved_connection.members[0].full_local_name,
            "/RENAMED1"
        );
        assert_eq!(
            graph[1].resolved_connection.members[0].full_local_name,
            "/RENAMED1"
        );
        assert_eq!(graph[3].name, "/RENAMED1");
        assert_eq!(graph[3].resolved_connection.name, "/RENAMED1");
        assert_eq!(
            graph[3].label_links[0].connection.full_local_name,
            "/RENAMED1"
        );
    }

    #[test]
    fn reduced_hierarchy_driver_chain_uses_best_driver() {
        let mut graph = vec![
            ReducedProjectSubgraphEntry {
                subgraph_code: 1,
                code: 1,
                name: "/ROOT_SIG".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/ROOT_SIG".to_string(),
                    local_name: "ROOT_SIG".to_string(),
                    full_local_name: "/ROOT_SIG".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/ROOT_SIG".to_string(),
                    local_name: "ROOT_SIG".to_string(),
                    full_local_name: "/ROOT_SIG".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                }),
                driver_identity: None,
                drivers: vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: 4,
                    name: "ROOT_SIG".to_string(),
                    full_name: "/ROOT_SIG".to_string(),
                }],
                non_bus_driver_priority: Some(4),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: vec![ReducedHierSheetPinLink {
                    at: PointKey(0, 0),
                    child_sheet_uuid: Some("child-sheet".to_string()),
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "/ROOT_SIG".to_string(),
                        local_name: "ROOT_SIG".to_string(),
                        full_local_name: "/ROOT_SIG".to_string(),
                        sheet_instance_path: String::new(),
                        members: Vec::new(),
                    },
                }],
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: vec![1],
            },
            ReducedProjectSubgraphEntry {
                subgraph_code: 2,
                code: 2,
                name: "/Child/GLOBAL_SIG".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/Child/GLOBAL_SIG".to_string(),
                    local_name: "GLOBAL_SIG".to_string(),
                    full_local_name: "/Child/GLOBAL_SIG".to_string(),
                    sheet_instance_path: "/child".to_string(),
                    members: Vec::new(),
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/Child/GLOBAL_SIG".to_string(),
                    local_name: "GLOBAL_SIG".to_string(),
                    full_local_name: "/Child/GLOBAL_SIG".to_string(),
                    sheet_instance_path: "/child".to_string(),
                    members: Vec::new(),
                }),
                driver_identity: None,
                drivers: vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::PowerPin,
                    priority: 6,
                    name: "GLOBAL_SIG".to_string(),
                    full_name: "/Child/GLOBAL_SIG".to_string(),
                }],
                non_bus_driver_priority: Some(6),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: "/child".to_string(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: vec![ReducedHierPortLink {
                    at: PointKey(0, 0),
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "/Child/GLOBAL_SIG".to_string(),
                        local_name: "ROOT_SIG".to_string(),
                        full_local_name: "/Child/GLOBAL_SIG".to_string(),
                        sheet_instance_path: "/child".to_string(),
                        members: Vec::new(),
                    },
                }],
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: Some(0),
                hier_child_indexes: Vec::new(),
            },
        ];

        refresh_reduced_hierarchy_driver_chains(&mut graph);

        assert_eq!(graph[0].name, "/Child/GLOBAL_SIG");
        assert_eq!(graph[1].name, "/Child/GLOBAL_SIG");
        assert_eq!(
            graph[0]
                .driver_connection
                .as_ref()
                .expect("root driver")
                .full_local_name,
            "/Child/GLOBAL_SIG"
        );
    }

    #[test]
    fn reduced_global_secondary_driver_promotion_updates_matching_global_subgraph() {
        let chosen = ReducedProjectConnection {
            net_code: 0,
            connection_type: ReducedProjectConnectionType::Net,
            name: "VCC".to_string(),
            local_name: "VCC".to_string(),
            full_local_name: "VCC".to_string(),
            sheet_instance_path: String::new(),
            members: Vec::new(),
        };

        let mut graph = vec![
            ReducedProjectSubgraphEntry {
                subgraph_code: 1,
                code: 1,
                name: "VCC".to_string(),
                resolved_connection: chosen.clone(),
                driver_connection: Some(chosen.clone()),
                driver_identity: None,
                drivers: vec![
                    ReducedProjectStrongDriver {
                        kind: ReducedProjectDriverKind::PowerPin,
                        priority: 6,
                        name: "VCC".to_string(),
                        full_name: "VCC".to_string(),
                    },
                    ReducedProjectStrongDriver {
                        kind: ReducedProjectDriverKind::PowerPin,
                        priority: 6,
                        name: "PWR_ALT".to_string(),
                        full_name: "PWR_ALT".to_string(),
                    },
                ],
                non_bus_driver_priority: Some(6),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
            ReducedProjectSubgraphEntry {
                subgraph_code: 2,
                code: 2,
                name: "PWR_ALT".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "PWR_ALT".to_string(),
                    local_name: "PWR_ALT".to_string(),
                    full_local_name: "PWR_ALT".to_string(),
                    sheet_instance_path: "/other".to_string(),
                    members: Vec::new(),
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "PWR_ALT".to_string(),
                    local_name: "PWR_ALT".to_string(),
                    full_local_name: "PWR_ALT".to_string(),
                    sheet_instance_path: "/other".to_string(),
                    members: Vec::new(),
                }),
                driver_identity: None,
                drivers: vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::PowerPin,
                    priority: 6,
                    name: "PWR_ALT".to_string(),
                    full_name: "PWR_ALT".to_string(),
                }],
                non_bus_driver_priority: Some(6),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: "/other".to_string(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
        ];

        refresh_reduced_global_secondary_driver_promotions(&mut graph);

        assert_eq!(graph[1].name, "VCC");
        assert_eq!(
            graph[1]
                .driver_connection
                .as_ref()
                .expect("promoted driver")
                .name,
            "VCC"
        );
    }

    #[test]
    fn reduced_post_propagation_forces_weak_single_pin_names_to_unconnected() {
        let mut graph = vec![ReducedProjectSubgraphEntry {
            subgraph_code: 1,
            code: 1,
            name: "Net-(R1-Pad1)".to_string(),
            resolved_connection: ReducedProjectConnection {
                net_code: 0,
                connection_type: ReducedProjectConnectionType::Net,
                name: "Net-(R1-Pad1)".to_string(),
                local_name: "Net-(R1-Pad1)".to_string(),
                full_local_name: "Net-(R1-Pad1)".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
            driver_connection: Some(ReducedProjectConnection {
                net_code: 0,
                connection_type: ReducedProjectConnectionType::Net,
                name: "Net-(R1-Pad1)".to_string(),
                local_name: "Net-(R1-Pad1)".to_string(),
                full_local_name: "Net-(R1-Pad1)".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            }),
            driver_identity: Some(
                crate::connectivity::ReducedProjectDriverIdentity::SymbolPin {
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                    symbol_uuid: Some("r1".to_string()),
                    at: PointKey(0, 0),
                },
            ),
            drivers: Vec::new(),
            non_bus_driver_priority: None,
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: String::new(),
            anchor: PointKey(0, 0),
            points: Vec::new(),
            nodes: Vec::new(),
            base_pins: vec![crate::connectivity::ReducedNetBasePinKey {
                sheet_instance_path: String::new(),
                symbol_uuid: Some("r1".to_string()),
                at: PointKey(0, 0),
                name: Some("1".to_string()),
            }],
            label_links: Vec::new(),
            no_connect_points: Vec::new(),
            hier_sheet_pins: Vec::new(),
            hier_ports: Vec::new(),
            bus_members: Vec::new(),
            bus_items: Vec::new(),
            wire_items: Vec::new(),
            bus_neighbor_links: Vec::new(),
            bus_parent_links: Vec::new(),
            bus_parent_indexes: Vec::new(),
            hier_parent_index: None,
            hier_child_indexes: Vec::new(),
        }];

        refresh_reduced_post_propagation_item_connections(&mut graph);

        assert_eq!(graph[0].name, "unconnected-(R1-Pad1)");
        assert_eq!(
            graph[0]
                .driver_connection
                .as_ref()
                .expect("driver connection")
                .name,
            "unconnected-(R1-Pad1)"
        );
    }

    #[test]
    fn reduced_live_post_propagation_forces_weak_single_pin_names_to_unconnected() {
        let mut graph = vec![ReducedProjectSubgraphEntry {
            subgraph_code: 1,
            code: 1,
            name: "Net-(R1-Pad1)".to_string(),
            resolved_connection: ReducedProjectConnection {
                net_code: 0,
                connection_type: ReducedProjectConnectionType::Net,
                name: "Net-(R1-Pad1)".to_string(),
                local_name: "Net-(R1-Pad1)".to_string(),
                full_local_name: "Net-(R1-Pad1)".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
            driver_connection: Some(ReducedProjectConnection {
                net_code: 0,
                connection_type: ReducedProjectConnectionType::Net,
                name: "Net-(R1-Pad1)".to_string(),
                local_name: "Net-(R1-Pad1)".to_string(),
                full_local_name: "Net-(R1-Pad1)".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            }),
            driver_identity: Some(
                crate::connectivity::ReducedProjectDriverIdentity::SymbolPin {
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                    symbol_uuid: Some("r1".to_string()),
                    at: PointKey(0, 0),
                },
            ),
            drivers: Vec::new(),
            non_bus_driver_priority: None,
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: String::new(),
            anchor: PointKey(0, 0),
            points: Vec::new(),
            nodes: Vec::new(),
            base_pins: vec![crate::connectivity::ReducedNetBasePinKey {
                sheet_instance_path: String::new(),
                symbol_uuid: Some("r1".to_string()),
                at: PointKey(0, 0),
                name: Some("1".to_string()),
            }],
            label_links: Vec::new(),
            no_connect_points: Vec::new(),
            hier_sheet_pins: Vec::new(),
            hier_ports: Vec::new(),
            bus_members: Vec::new(),
            bus_items: Vec::new(),
            wire_items: Vec::new(),
            bus_neighbor_links: Vec::new(),
            bus_parent_links: Vec::new(),
            bus_parent_indexes: Vec::new(),
            hier_parent_index: None,
            hier_child_indexes: Vec::new(),
        }];

        refresh_reduced_live_post_propagation_item_connections(&mut graph);

        assert_eq!(graph[0].name, "unconnected-(R1-Pad1)");
        assert_eq!(
            graph[0]
                .driver_connection
                .as_ref()
                .expect("driver connection")
                .name,
            "unconnected-(R1-Pad1)"
        );
    }

    #[test]
    fn reduced_post_propagation_promotes_sheet_pin_nets_to_bus() {
        let mut graph = vec![
            ReducedProjectSubgraphEntry {
                subgraph_code: 1,
                code: 1,
                name: "/BUS".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/BUS".to_string(),
                    local_name: "BUS".to_string(),
                    full_local_name: "/BUS".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/BUS".to_string(),
                    local_name: "BUS".to_string(),
                    full_local_name: "/BUS".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                }),
                driver_identity: Some(
                    crate::connectivity::ReducedProjectDriverIdentity::SheetPin {
                        schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                        at: PointKey(0, 0),
                    },
                ),
                drivers: Vec::new(),
                non_bus_driver_priority: None,
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: vec![1],
            },
            ReducedProjectSubgraphEntry {
                subgraph_code: 2,
                code: 2,
                name: "/BUS".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS".to_string(),
                    local_name: "BUS".to_string(),
                    full_local_name: "/BUS".to_string(),
                    sheet_instance_path: "/child".to_string(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "BUS0".to_string(),
                        local_name: "BUS0".to_string(),
                        full_local_name: "/child/BUS0".to_string(),
                        vector_index: Some(0),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS".to_string(),
                    local_name: "BUS".to_string(),
                    full_local_name: "/BUS".to_string(),
                    sheet_instance_path: "/child".to_string(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "BUS0".to_string(),
                        local_name: "BUS0".to_string(),
                        full_local_name: "/child/BUS0".to_string(),
                        vector_index: Some(0),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                }),
                driver_identity: None,
                drivers: Vec::new(),
                non_bus_driver_priority: None,
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: "/child".to_string(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: Some(0),
                hier_child_indexes: Vec::new(),
            },
        ];

        refresh_reduced_post_propagation_item_connections(&mut graph);

        assert_eq!(
            graph[0]
                .driver_connection
                .as_ref()
                .expect("parent driver")
                .connection_type,
            ReducedProjectConnectionType::Bus
        );
        assert_eq!(
            graph[0]
                .driver_connection
                .as_ref()
                .expect("parent driver")
                .members[0]
                .full_local_name,
            "/child/BUS0"
        );
    }

    #[test]
    fn reduced_live_post_propagation_promotes_sheet_pin_nets_to_bus() {
        let mut graph = vec![
            ReducedProjectSubgraphEntry {
                subgraph_code: 1,
                code: 1,
                name: "/BUS".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/BUS".to_string(),
                    local_name: "BUS".to_string(),
                    full_local_name: "/BUS".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/BUS".to_string(),
                    local_name: "BUS".to_string(),
                    full_local_name: "/BUS".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                }),
                driver_identity: Some(
                    crate::connectivity::ReducedProjectDriverIdentity::SheetPin {
                        schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                        at: PointKey(0, 0),
                    },
                ),
                drivers: Vec::new(),
                non_bus_driver_priority: None,
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: vec![1],
            },
            ReducedProjectSubgraphEntry {
                subgraph_code: 2,
                code: 2,
                name: "/BUS".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS".to_string(),
                    local_name: "BUS".to_string(),
                    full_local_name: "/BUS".to_string(),
                    sheet_instance_path: "/child".to_string(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "BUS0".to_string(),
                        local_name: "BUS0".to_string(),
                        full_local_name: "/child/BUS0".to_string(),
                        vector_index: Some(0),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS".to_string(),
                    local_name: "BUS".to_string(),
                    full_local_name: "/BUS".to_string(),
                    sheet_instance_path: "/child".to_string(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "BUS0".to_string(),
                        local_name: "BUS0".to_string(),
                        full_local_name: "/child/BUS0".to_string(),
                        vector_index: Some(0),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                }),
                driver_identity: None,
                drivers: Vec::new(),
                non_bus_driver_priority: None,
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: "/child".to_string(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: Some(0),
                hier_child_indexes: Vec::new(),
            },
        ];

        refresh_reduced_live_post_propagation_item_connections(&mut graph);

        assert_eq!(
            graph[0]
                .driver_connection
                .as_ref()
                .expect("parent driver")
                .connection_type,
            ReducedProjectConnectionType::Bus
        );
        assert_eq!(
            graph[0]
                .driver_connection
                .as_ref()
                .expect("parent driver")
                .members[0]
                .full_local_name,
            "/child/BUS0"
        );
    }
}
