use std::cell::{Ref, RefCell, RefMut};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::rc::{Rc, Weak};

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
    pub(crate) pin_number: Option<String>,
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
    pub(crate) number: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ReducedProjectBasePin {
    pub(crate) key: ReducedNetBasePinKey,
    pub(crate) number: Option<String>,
    pub(crate) electrical_type: Option<String>,
    pub(crate) connection: ReducedProjectConnection,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReducedNetSubgraph {
    pub(crate) anchor: [f64; 2],
    pub(crate) class: String,
    pub(crate) has_no_connect: bool,
    pub(crate) points: Vec<PointKey>,
    pub(crate) nodes: Vec<ReducedNetNode>,
    pub(crate) base_pins: Vec<ReducedProjectBasePin>,
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
    pub(crate) chosen_driver_identity: Option<ReducedProjectDriverIdentity>,
    pub(crate) drivers: Vec<ReducedProjectStrongDriver>,
    pub(crate) class: String,
    pub(crate) has_no_connect: bool,
    pub(crate) sheet_instance_path: String,
    pub(crate) anchor: PointKey,
    pub(crate) points: Vec<PointKey>,
    pub(crate) nodes: Vec<ReducedNetNode>,
    pub(crate) base_pins: Vec<ReducedProjectBasePin>,
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
        pin_number: Option<String>,
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
    pub(crate) connection: ReducedProjectConnection,
    pub(crate) identity: Option<ReducedProjectDriverIdentity>,
}

#[derive(Clone, Debug)]
struct LiveProjectStrongDriver {
    owner: LiveProjectStrongDriverOwner,
}

type LiveProjectStrongDriverHandle = Rc<RefCell<LiveProjectStrongDriver>>;

#[derive(Clone, Debug)]
#[allow(dead_code)]
enum LiveProjectStrongDriverOwner {
    Floating {
        identity: Option<ReducedProjectDriverIdentity>,
        connection: LiveReducedConnection,
        kind: ReducedProjectDriverKind,
        priority: i32,
    },
    Label {
        owner: Weak<RefCell<LiveReducedLabelLink>>,
        kind: ReducedProjectDriverKind,
        priority: i32,
    },
    SheetPin {
        owner: Weak<RefCell<LiveReducedHierSheetPinLink>>,
        kind: ReducedProjectDriverKind,
        priority: i32,
    },
    HierPort {
        owner: Weak<RefCell<LiveReducedHierPortLink>>,
        kind: ReducedProjectDriverKind,
        priority: i32,
    },
    SymbolPin {
        owner: Weak<RefCell<LiveReducedBasePin>>,
        kind: ReducedProjectDriverKind,
        priority: i32,
    },
}

impl From<ReducedProjectStrongDriver> for LiveProjectStrongDriver {
    fn from(driver: ReducedProjectStrongDriver) -> Self {
        Self {
            owner: LiveProjectStrongDriverOwner::Floating {
                identity: driver.identity,
                connection: LiveReducedConnection::new(driver.connection),
                kind: driver.kind,
                priority: driver.priority,
            },
        }
    }
}

impl LiveProjectStrongDriver {
    fn snapshot(&self) -> ReducedProjectStrongDriver {
        let connection = live_project_strong_driver_connection(self).snapshot();
        ReducedProjectStrongDriver {
            kind: live_project_strong_driver_kind(self),
            priority: live_project_strong_driver_priority(self),
            connection,
            identity: live_project_strong_driver_identity(self),
        }
    }
}

pub(crate) fn reduced_project_strong_driver_name(driver: &ReducedProjectStrongDriver) -> &str {
    &driver.connection.local_name
}

pub(crate) fn reduced_project_strong_driver_full_name(driver: &ReducedProjectStrongDriver) -> &str {
    &driver.connection.name
}

// Upstream parity: reduced project-side owner for the chosen `CONNECTION_SUBGRAPH` driver item
// identity after `ResolveDrivers()`. This now keeps the exact chosen driver identity projected out
// of the reduced graph build instead of reconstructing it later from the chosen driver name, which
// still diverges from KiCad's live object ownership but avoids same-name driver collapse above the
// shared graph boundary.
pub(crate) fn reduced_project_subgraph_driver_identity(
    subgraph: &ReducedProjectSubgraphEntry,
) -> Option<&ReducedProjectDriverIdentity> {
    subgraph.chosen_driver_identity.as_ref()
}

pub(crate) fn reduced_project_subgraph_non_bus_driver_priority(
    subgraph: &ReducedProjectSubgraphEntry,
) -> Option<i32> {
    subgraph
        .drivers
        .iter()
        .find(|driver| {
            !matches!(
                driver.connection.connection_type,
                ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
            )
        })
        .map(|driver| driver.priority)
}

fn reduced_strong_drivers_into_live_handles(
    drivers: Vec<ReducedProjectStrongDriver>,
) -> Vec<LiveProjectStrongDriverHandle> {
    drivers
        .into_iter()
        .map(|driver| Rc::new(RefCell::new(driver.into())))
        .collect()
}

fn live_strong_driver_handles_to_snapshots(
    drivers: &[LiveProjectStrongDriverHandle],
) -> Vec<ReducedProjectStrongDriver> {
    drivers
        .iter()
        .map(|driver| driver.borrow().snapshot())
        .collect()
}

fn live_project_strong_driver_kind(driver: &LiveProjectStrongDriver) -> ReducedProjectDriverKind {
    match &driver.owner {
        LiveProjectStrongDriverOwner::Floating { kind, .. }
        | LiveProjectStrongDriverOwner::Label { kind, .. }
        | LiveProjectStrongDriverOwner::SheetPin { kind, .. }
        | LiveProjectStrongDriverOwner::HierPort { kind, .. }
        | LiveProjectStrongDriverOwner::SymbolPin { kind, .. } => *kind,
    }
}

fn live_project_strong_driver_priority(driver: &LiveProjectStrongDriver) -> i32 {
    match &driver.owner {
        LiveProjectStrongDriverOwner::Floating { priority, .. }
        | LiveProjectStrongDriverOwner::Label { priority, .. }
        | LiveProjectStrongDriverOwner::SheetPin { priority, .. }
        | LiveProjectStrongDriverOwner::HierPort { priority, .. }
        | LiveProjectStrongDriverOwner::SymbolPin { priority, .. } => *priority,
    }
}

fn live_project_strong_driver_identity(
    driver: &LiveProjectStrongDriver,
) -> Option<ReducedProjectDriverIdentity> {
    match &driver.owner {
        LiveProjectStrongDriverOwner::Floating { identity, .. } => identity.clone(),
        LiveProjectStrongDriverOwner::Label { owner, .. } => owner
            .upgrade()
            .and_then(|owner| owner.borrow().identity.clone()),
        LiveProjectStrongDriverOwner::SheetPin { owner, .. } => owner
            .upgrade()
            .and_then(|owner| owner.borrow().identity.clone()),
        LiveProjectStrongDriverOwner::HierPort { owner, .. } => owner
            .upgrade()
            .and_then(|owner| owner.borrow().identity.clone()),
        LiveProjectStrongDriverOwner::SymbolPin { owner, .. } => owner
            .upgrade()
            .and_then(|owner| owner.borrow().pin.identity.clone()),
    }
}

fn live_project_strong_driver_connection(
    driver: &LiveProjectStrongDriver,
) -> LiveReducedConnection {
    match &driver.owner {
        LiveProjectStrongDriverOwner::Floating { connection, .. } => Some(connection.clone()),
        LiveProjectStrongDriverOwner::Label { owner, .. } => owner
            .upgrade()
            .map(|owner| owner.borrow().connection.clone()),
        LiveProjectStrongDriverOwner::SheetPin { owner, .. } => owner
            .upgrade()
            .map(|owner| owner.borrow().connection.clone()),
        LiveProjectStrongDriverOwner::HierPort { owner, .. } => owner
            .upgrade()
            .map(|owner| owner.borrow().connection.clone()),
        LiveProjectStrongDriverOwner::SymbolPin { owner, .. } => owner
            .upgrade()
            .map(|owner| owner.borrow().connection.clone()),
    }
    .expect("live strong driver owner requires an attached connection owner")
}

fn live_project_strong_driver_full_name(driver: &LiveProjectStrongDriver) -> String {
    live_project_strong_driver_connection(driver).name()
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ReducedProjectPinIdentityKey {
    sheet_instance_path: String,
    symbol_uuid: Option<String>,
    at: PointKey,
    number: Option<String>,
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
    pin_driver_connections: BTreeMap<ReducedNetBasePinKey, ReducedProjectConnection>,
    pin_driver_connections_by_location:
        BTreeMap<ReducedProjectPinIdentityKey, ReducedProjectConnection>,
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

fn parse_alphanumeric_pin(pin: &str) -> (String, Option<i64>) {
    let mut number_start = pin.len();

    for (index, ch) in pin.char_indices().rev() {
        if !ch.is_ascii_digit() {
            number_start = index + ch.len_utf8();
            break;
        }

        if index == 0 {
            number_start = 0;
        }
    }

    if number_start >= pin.len() {
        return (String::new(), None);
    }

    let prefix = pin[..number_start].to_string();
    let number = pin[number_start..].parse::<i64>().ok();
    (prefix, number)
}

// Upstream parity: reduced local analogue for `ExpandStackedPinNotation()`. This now mirrors the
// exercised bracket/range branches KiCad uses for stacked pins, including `[1-3]` and `[A1-A3]`,
// instead of only splitting comma lists. Remaining divergence is broader upstream validation and
// any stacked-pin syntax not yet exercised in the local graph/export paths.
fn expand_stacked_pin_notation(pin: &str) -> (Vec<String>, bool) {
    let trimmed = pin.trim();
    let has_open_bracket = trimmed.contains('[');
    let has_close_bracket = trimmed.contains(']');

    if has_open_bracket || has_close_bracket {
        if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
            return (vec![pin.to_string()], false);
        }
    }

    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return (vec![pin.to_string()], true);
    }

    let inner = &trimmed[1..trimmed.len() - 1];
    let mut numbers = Vec::new();

    for part in inner
        .split(',')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
    {
        if let Some(dash_index) = part.find('-') {
            let start = part[..dash_index].trim();
            let end = part[dash_index + 1..].trim();
            let (start_prefix, start_value) = parse_alphanumeric_pin(start);
            let (end_prefix, end_value) = parse_alphanumeric_pin(end);

            let Some(start_value) = start_value else {
                return (vec![pin.to_string()], false);
            };
            let Some(end_value) = end_value else {
                return (vec![pin.to_string()], false);
            };
            if start_prefix != end_prefix || start_value > end_value {
                return (vec![pin.to_string()], false);
            }

            for value in start_value..=end_value {
                if start_prefix.is_empty() {
                    numbers.push(value.to_string());
                } else {
                    numbers.push(format!("{start_prefix}{value}"));
                }
            }
        } else {
            numbers.push(part.to_string());
        }
    }

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

fn reduced_connection_is_bus(connection_type: ReducedProjectConnectionType) -> bool {
    matches!(
        connection_type,
        ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
    )
}

fn reduced_connection_is_net(connection_type: ReducedProjectConnectionType) -> bool {
    matches!(connection_type, ReducedProjectConnectionType::Net)
}

fn reduced_connection_kind_mismatch(
    driver_connection: ReducedProjectConnectionType,
    item_connection: ReducedProjectConnectionType,
) -> bool {
    (reduced_connection_is_bus(driver_connection) && reduced_connection_is_net(item_connection))
        || (reduced_connection_is_net(driver_connection)
            && reduced_connection_is_bus(item_connection))
}

// Upstream parity: reduced local analogue for `SCH_PIN::GetEffectivePadNumber()`. This still
// uses reduced stacked-pin text instead of live `SCH_PIN` state, but it now applies the same
// exercised "smallest logical number" branch to default-net naming instead of always using the
// raw shown pin number. Remaining divergence is fuller stacked-pin parsing beyond the reduced
// bracketed notation helper.
fn reduced_effective_pad_number(pin_number: &str) -> String {
    let (expanded_numbers, valid) = expand_stacked_pin_notation(pin_number);

    if valid {
        expanded_numbers
            .into_iter()
            .next()
            .unwrap_or_else(|| pin_number.to_string())
    } else {
        pin_number.to_string()
    }
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

fn build_reduced_project_driver_connection(
    schematic: &Schematic,
    sheet_instance_path: impl Into<String>,
    local_name: impl Into<String>,
    full_name: impl Into<String>,
    member_sheet_prefix: &str,
) -> ReducedProjectConnection {
    let local_name = local_name.into();
    let full_name = full_name.into();
    let members = if reduced_text_is_bus(schematic, &local_name) {
        collect_reduced_bus_member_objects_inner(
            schematic,
            &local_name,
            "",
            member_sheet_prefix,
            &mut BTreeSet::new(),
        )
    } else {
        Vec::new()
    };

    build_reduced_project_connection(
        schematic,
        sheet_instance_path,
        full_name.clone(),
        local_name,
        full_name,
        members,
    )
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
#[cfg_attr(not(test), allow(dead_code))]
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

// Upstream parity: local live-graph analogue for `CONNECTION_GRAPH::matchBusMember()`. This now
// matches against the active live connection-member payload instead of round-tripping through a
// reduced member vector during graph propagation. Remaining divergence is fuller pointer-shared
// member identity across every attached item and subgraph relationship.
#[cfg_attr(not(test), allow(dead_code))]
fn match_live_bus_member<'a>(
    bus_members: &'a [LiveProjectBusMemberHandle],
    search: &ReducedBusMember,
) -> Option<LiveProjectBusMemberHandle> {
    for member in bus_members {
        let member_ref = member.borrow();
        if let Some(search_index) = search.vector_index {
            if member_ref.vector_index == Some(search_index) {
                return Some(member.clone());
            }
        }

        if member_ref.kind == ReducedBusMemberKind::Bus {
            if let Some(found) = match_live_bus_member(&member_ref.members, search) {
                return Some(found);
            }
        } else if member_ref.local_name == search.local_name {
            return Some(member.clone());
        }
    }

    None
}

fn match_live_bus_member_live<'a>(
    bus_members: &'a [LiveProjectBusMemberHandle],
    search: &LiveProjectBusMember,
) -> Option<LiveProjectBusMemberHandle> {
    for member in bus_members {
        let member_ref = member.borrow();
        if let Some(search_index) = search.vector_index {
            if member_ref.vector_index == Some(search_index) {
                return Some(member.clone());
            }
        }

        if member_ref.kind == ReducedBusMemberKind::Bus {
            if let Some(found) = match_live_bus_member_live(&member_ref.members, search) {
                return Some(found);
            }
        } else if member_ref.local_name == search.local_name {
            return Some(member.clone());
        }
    }

    None
}

fn match_live_bus_member_connection<'a>(
    bus_members: &'a [LiveProjectBusMemberHandle],
    search: &LiveProjectConnection,
) -> Option<LiveProjectBusMemberHandle> {
    for member in bus_members {
        let member_ref = member.borrow();

        if member_ref.kind == ReducedBusMemberKind::Bus {
            if let Some(found) = match_live_bus_member_connection(&member_ref.members, search) {
                return Some(found);
            }
        } else if member_ref.local_name == search.local_name {
            return Some(member.clone());
        }
    }

    None
}

// Upstream parity: mutable live-graph analogue for `CONNECTION_GRAPH::matchBusMember()`. This now
// mutates the active live connection-member payload directly instead of reduced member snapshots.
// Remaining divergence is fuller pointer-shared member identity across every attached item and
// subgraph relationship.
#[cfg_attr(not(test), allow(dead_code))]
fn match_live_bus_member_mut<'a>(
    bus_members: &'a mut [LiveProjectBusMemberHandle],
    search: &ReducedBusMember,
) -> Option<LiveProjectBusMemberHandle> {
    for member in bus_members {
        let member_ref = member.borrow();
        if let Some(search_index) = search.vector_index {
            if member_ref.vector_index == Some(search_index) {
                return Some(member.clone());
            }
        }

        if member_ref.kind == ReducedBusMemberKind::Bus {
            drop(member_ref);
            if let Some(found) = match_live_bus_member_mut(&mut member.borrow_mut().members, search)
            {
                return Some(found);
            }
        } else if member_ref.local_name == search.local_name {
            return Some(member.clone());
        }
    }

    None
}

fn match_live_bus_member_mut_live<'a>(
    bus_members: &'a mut [LiveProjectBusMemberHandle],
    search: &LiveProjectBusMember,
) -> Option<LiveProjectBusMemberHandle> {
    for member in bus_members {
        let member_ref = member.borrow();
        if let Some(search_index) = search.vector_index {
            if member_ref.vector_index == Some(search_index) {
                return Some(member.clone());
            }
        }

        if member_ref.kind == ReducedBusMemberKind::Bus {
            drop(member_ref);
            if let Some(found) =
                match_live_bus_member_mut_live(&mut member.borrow_mut().members, search)
            {
                return Some(found);
            }
        } else if member_ref.local_name == search.local_name {
            return Some(member.clone());
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
#[cfg_attr(not(test), allow(dead_code))]
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
#[cfg(test)]
fn clone_reduced_connection_into_subgraph(
    subgraph: &mut ReducedProjectSubgraphEntry,
    connection: &ReducedProjectConnection,
) {
    subgraph.name = connection.name.clone();
    clone_reduced_connection_into_live_connection(&mut subgraph.resolved_connection, connection);

    if let Some(driver_connection) = &mut subgraph.driver_connection {
        clone_reduced_connection_into_live_connection(driver_connection, connection);
    }

    for link in &mut subgraph.label_links {
        clone_reduced_connection_into_live_connection(&mut link.connection, connection);
    }

    for pin in &mut subgraph.hier_sheet_pins {
        clone_reduced_connection_into_live_connection(&mut pin.connection, connection);
    }

    for port in &mut subgraph.hier_ports {
        clone_reduced_connection_into_live_connection(&mut port.connection, connection);
    }
}

// Upstream parity: local live-graph analogue for `SCH_CONNECTION::Clone()` when a propagated
// member net replaces an older bus member name. This mutates the active live bus-member payload
// directly instead of round-tripping through a reduced member vector inside the live graph.
#[cfg(test)]
fn clone_reduced_connection_into_live_bus_member(
    member: &mut LiveProjectBusMember,
    connection: &ReducedProjectConnection,
) {
    let existing_local_name = member.local_name.clone();
    let existing_vector_index = member.vector_index;
    member.net_code = connection.net_code;
    member.name = connection.local_name.clone();
    if existing_local_name.is_empty() {
        member.local_name = connection.local_name.clone();
    }
    member.full_local_name = connection.full_local_name.clone();
    member.kind = match connection.connection_type {
        ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup => {
            ReducedBusMemberKind::Bus
        }
        _ => ReducedBusMemberKind::Net,
    };
    if existing_vector_index.is_some() {
        member.vector_index = existing_vector_index;
    }

    if member.kind == ReducedBusMemberKind::Bus {
        member.members = reduced_bus_members_into_live_handles(connection.members.clone());
    } else {
        member.members.clear();
    }
}

fn clone_live_bus_member_into_live_bus_member(
    target: &mut LiveProjectBusMember,
    source: &LiveProjectBusMember,
) {
    let existing_local_name = target.local_name.clone();
    let existing_vector_index = target.vector_index;
    let existing_members = target.members.clone();

    target.net_code = source.net_code;
    target.name = source.name.clone();
    if existing_local_name.is_empty() {
        target.local_name = source.local_name.clone();
    }
    target.full_local_name = source.full_local_name.clone();
    target.kind = source.kind.clone();
    if existing_vector_index.is_some() {
        target.vector_index = existing_vector_index;
    } else {
        target.vector_index = source.vector_index;
    }

    if matches!(target.kind, ReducedBusMemberKind::Bus)
        && matches!(source.kind, ReducedBusMemberKind::Bus)
    {
        if existing_members.is_empty() {
            target.members = source.members.clone();
        } else {
            target.members = existing_members;

            let clone_limit = target.members.len().min(source.members.len());
            for index in 0..clone_limit {
                if Rc::ptr_eq(&target.members[index], &source.members[index]) {
                    continue;
                }
                clone_live_bus_member_into_live_bus_member(
                    &mut target.members[index].borrow_mut(),
                    &source.members[index].borrow(),
                );
            }

            if target.members.len() > source.members.len() {
                target.members.truncate(source.members.len());
            } else if target.members.len() < source.members.len() {
                target
                    .members
                    .extend(source.members[target.members.len()..].iter().cloned());
            }
        }
    } else {
        target.members = source.members.clone();
    }
}

fn clone_live_bus_member_handle_into_live_bus_member_handle(
    target: &LiveProjectBusMemberHandle,
    source: &LiveProjectBusMemberHandle,
) {
    if Rc::ptr_eq(target, source) {
        return;
    }

    // Upstream parity: local live member analogue for the recursive `SCH_CONNECTION::Clone()`
    // member path. This still snapshots one source member before mutating the target handle so
    // the reduced live graph can avoid aliasing the same RefCell-backed member tree through two
    // recursive paths. Remaining divergence is fuller live member/pointer ownership on the shared
    // connection/subgraph graph.
    let source_snapshot = live_bus_member_handle_snapshot(source);
    clone_reduced_bus_member_into_live_bus_member(&mut target.borrow_mut(), &source_snapshot);
}

fn clone_live_bus_member_into_live_connection_owner(
    target: &mut LiveProjectConnection,
    source: &LiveProjectBusMember,
    sheet_instance_path: &str,
) {
    let existing_local_name = target.local_name.clone();
    let existing_members = target.members.clone();

    target.net_code = source.net_code;
    target.connection_type = match source.kind {
        ReducedBusMemberKind::Net => ReducedProjectConnectionType::Net,
        ReducedBusMemberKind::Bus => ReducedProjectConnectionType::Bus,
    };
    target.name = source.full_local_name.clone();
    if existing_local_name.is_empty() {
        target.local_name = source.local_name.clone();
    }
    target.full_local_name = source.full_local_name.clone();
    target.sheet_instance_path = sheet_instance_path.to_string();

    if matches!(
        target.connection_type,
        ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
    ) && matches!(source.kind, ReducedBusMemberKind::Bus)
    {
        if existing_members.is_empty() {
            target.members = source.members.clone();
        } else {
            target.members = existing_members;

            let clone_limit = target.members.len().min(source.members.len());
            for index in 0..clone_limit {
                clone_live_bus_member_handle_into_live_bus_member_handle(
                    &target.members[index],
                    &source.members[index],
                );
            }

            if target.members.len() > source.members.len() {
                target.members.truncate(source.members.len());
            } else if target.members.len() < source.members.len() {
                target
                    .members
                    .extend(source.members[target.members.len()..].iter().cloned());
            }
        }
    } else {
        target.members = source.members.clone();
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

fn reduced_subgraph_driver_connection(
    subgraph: &ReducedProjectSubgraphEntry,
) -> ReducedProjectConnection {
    subgraph
        .driver_connection
        .clone()
        .unwrap_or_else(|| subgraph.resolved_connection.clone())
}

#[cfg(test)]
fn reduced_strong_driver_priority(subgraph: &ReducedProjectSubgraphEntry) -> Option<i32> {
    subgraph.drivers.first().map(|driver| driver.priority)
}

#[cfg(test)]
fn reduced_subgraph_driver_priority(subgraph: &ReducedProjectSubgraphEntry) -> i32 {
    reduced_strong_driver_priority(subgraph)
        .or_else(|| subgraph.driver_connection.as_ref().map(|_| 1))
        .unwrap_or(0)
}

#[cfg_attr(not(test), allow(dead_code))]
fn clone_reduced_bus_member_into_live_member(
    target: &mut ReducedBusMember,
    source: &ReducedBusMember,
) {
    let existing_local_name = target.local_name.clone();
    let existing_vector_index = target.vector_index;
    let existing_members = target.members.clone();

    target.net_code = source.net_code;
    target.name = source.name.clone();
    if existing_local_name.is_empty() {
        target.local_name = source.local_name.clone();
    }
    target.full_local_name = source.full_local_name.clone();
    target.kind = source.kind.clone();

    if matches!(target.kind, ReducedBusMemberKind::Bus)
        && matches!(source.kind, ReducedBusMemberKind::Bus)
    {
        if existing_members.is_empty() {
            target.members = source.members.clone();
        } else {
            target.members = source.members.clone();

            for member in &mut target.members {
                if let Some(existing) = match_reduced_bus_member(&existing_members, member) {
                    if !existing.local_name.is_empty() {
                        member.local_name = existing.local_name.clone();
                    }
                    if existing.vector_index.is_some() {
                        member.vector_index = existing.vector_index;
                    }
                }
            }
        }
    } else {
        target.members = source.members.clone();
    }

    if existing_vector_index.is_some() {
        target.vector_index = existing_vector_index;
    }
}

#[cfg(test)]
fn clone_reduced_connection_into_live_connection(
    target: &mut ReducedProjectConnection,
    source: &ReducedProjectConnection,
) {
    let existing_local_name = target.local_name.clone();
    let existing_members = target.members.clone();

    target.net_code = source.net_code;
    target.connection_type = source.connection_type.clone();
    target.name = source.name.clone();
    if existing_local_name.is_empty() {
        target.local_name = source.local_name.clone();
    }
    target.full_local_name = source.full_local_name.clone();
    target.sheet_instance_path = source.sheet_instance_path.clone();

    if matches!(
        target.connection_type,
        ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
    ) && matches!(
        source.connection_type,
        ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
    ) {
        if existing_members.is_empty() {
            target.members = source.members.clone();
        } else {
            target.members = existing_members.clone();

            let clone_limit = target.members.len().min(source.members.len());
            for index in 0..clone_limit {
                clone_reduced_bus_member_into_live_member(
                    &mut target.members[index],
                    &source.members[index],
                );
            }

            if target.members.len() > source.members.len() {
                target.members.truncate(source.members.len());
            } else if target.members.len() < source.members.len() {
                target
                    .members
                    .extend(source.members[target.members.len()..].iter().cloned());
            }
        }
    } else {
        target.members = source.members.clone();
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct LiveProjectBusMember {
    net_code: usize,
    name: String,
    local_name: String,
    full_local_name: String,
    vector_index: Option<usize>,
    kind: ReducedBusMemberKind,
    members: Vec<LiveProjectBusMemberHandle>,
}

type LiveProjectBusMemberHandle = Rc<RefCell<LiveProjectBusMember>>;

fn live_bus_member_handle_snapshot(handle: &LiveProjectBusMemberHandle) -> ReducedBusMember {
    handle.borrow().snapshot()
}

fn live_bus_member_handles_eq(
    left: &LiveProjectBusMemberHandle,
    right: &LiveProjectBusMemberHandle,
) -> bool {
    live_bus_member_handle_snapshot(left) == live_bus_member_handle_snapshot(right)
}

fn live_bus_member_handles_cmp(
    left: &LiveProjectBusMemberHandle,
    right: &LiveProjectBusMemberHandle,
) -> Ordering {
    live_bus_member_handle_snapshot(left).cmp(&live_bus_member_handle_snapshot(right))
}

fn live_bus_member_handles_clone_eq(
    target: &[LiveProjectBusMemberHandle],
    source: &[LiveProjectBusMemberHandle],
) -> bool {
    if target.len() != source.len() {
        return false;
    }

    target
        .iter()
        .zip(source.iter())
        .all(|(target, source)| live_bus_member_handle_clone_eq(target, source))
}

fn live_bus_member_handle_clone_eq(
    target: &LiveProjectBusMemberHandle,
    source: &LiveProjectBusMemberHandle,
) -> bool {
    if Rc::ptr_eq(target, source) {
        return true;
    }

    live_bus_member_clone_eq(&target.borrow(), &source.borrow())
}

fn reduced_bus_members_into_live_handles(
    members: Vec<ReducedBusMember>,
) -> Vec<LiveProjectBusMemberHandle> {
    members
        .into_iter()
        .map(|member| Rc::new(RefCell::new(member.into())))
        .collect()
}

fn live_bus_member_handles_to_snapshots(
    members: &[LiveProjectBusMemberHandle],
) -> Vec<ReducedBusMember> {
    members
        .iter()
        .map(live_bus_member_handle_snapshot)
        .collect()
}

impl From<ReducedBusMember> for LiveProjectBusMember {
    fn from(member: ReducedBusMember) -> Self {
        Self {
            net_code: member.net_code,
            name: member.name,
            local_name: member.local_name,
            full_local_name: member.full_local_name,
            vector_index: member.vector_index,
            kind: member.kind,
            members: reduced_bus_members_into_live_handles(member.members),
        }
    }
}

impl LiveProjectBusMember {
    fn snapshot(&self) -> ReducedBusMember {
        ReducedBusMember {
            net_code: self.net_code,
            name: self.name.clone(),
            local_name: self.local_name.clone(),
            full_local_name: self.full_local_name.clone(),
            vector_index: self.vector_index,
            kind: self.kind.clone(),
            members: live_bus_member_handles_to_snapshots(&self.members),
        }
    }
}

fn live_bus_member_clone_eq(target: &LiveProjectBusMember, source: &LiveProjectBusMember) -> bool {
    if target.net_code != source.net_code
        || target.name != source.name
        || target.full_local_name != source.full_local_name
        || target.kind != source.kind
    {
        return false;
    }

    if target.local_name.is_empty() && target.local_name != source.local_name {
        return false;
    }

    if target.vector_index.is_none() && target.vector_index != source.vector_index {
        return false;
    }

    if matches!(target.kind, ReducedBusMemberKind::Bus)
        && matches!(source.kind, ReducedBusMemberKind::Bus)
    {
        if target.members.is_empty() {
            live_bus_member_handles_clone_eq(&target.members, &source.members)
        } else {
            target.members.len() == source.members.len()
                && target.members.iter().zip(source.members.iter()).all(
                    |(target_member, source_member)| {
                        live_bus_member_handle_clone_eq(target_member, source_member)
                    },
                )
        }
    } else {
        live_bus_member_handles_clone_eq(&target.members, &source.members)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct LiveProjectConnection {
    net_code: usize,
    connection_type: ReducedProjectConnectionType,
    name: String,
    local_name: String,
    full_local_name: String,
    sheet_instance_path: String,
    members: Vec<LiveProjectBusMemberHandle>,
}

impl From<ReducedProjectConnection> for LiveProjectConnection {
    fn from(connection: ReducedProjectConnection) -> Self {
        Self {
            net_code: connection.net_code,
            connection_type: connection.connection_type,
            name: connection.name,
            local_name: connection.local_name,
            full_local_name: connection.full_local_name,
            sheet_instance_path: connection.sheet_instance_path,
            members: reduced_bus_members_into_live_handles(connection.members),
        }
    }
}

impl LiveProjectConnection {
    fn snapshot(&self) -> ReducedProjectConnection {
        ReducedProjectConnection {
            net_code: self.net_code,
            connection_type: self.connection_type,
            name: self.name.clone(),
            local_name: self.local_name.clone(),
            full_local_name: self.full_local_name.clone(),
            sheet_instance_path: self.sheet_instance_path.clone(),
            members: live_bus_member_handles_to_snapshots(&self.members),
        }
    }
}

fn live_connection_clone_eq(
    target: &LiveProjectConnection,
    source: &LiveProjectConnection,
) -> bool {
    if target.net_code != source.net_code
        || target.connection_type != source.connection_type
        || target.name != source.name
        || target.full_local_name != source.full_local_name
        || target.sheet_instance_path != source.sheet_instance_path
    {
        return false;
    }

    if target.local_name.is_empty() && target.local_name != source.local_name {
        return false;
    }

    if matches!(
        target.connection_type,
        ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
    ) && matches!(
        source.connection_type,
        ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
    ) {
        if target.members.is_empty() {
            live_bus_member_handles_clone_eq(&target.members, &source.members)
        } else {
            target.members.len() == source.members.len()
                && target.members.iter().zip(source.members.iter()).all(
                    |(target_member, source_member)| {
                        live_bus_member_handle_clone_eq(target_member, source_member)
                    },
                )
        }
    } else {
        live_bus_member_handles_clone_eq(&target.members, &source.members)
    }
}

fn live_connection_handle_clone_eq(
    target: &LiveReducedConnection,
    source: &LiveReducedConnection,
) -> bool {
    if Rc::ptr_eq(&target.connection, &source.connection) {
        return true;
    }

    live_connection_clone_eq(&target.borrow(), &source.borrow())
}

fn live_bus_member_clone_eq_to_connection(
    target: &LiveProjectBusMember,
    source: &LiveProjectConnection,
) -> bool {
    if target.net_code != source.net_code
        || target.name != source.name
        || target.full_local_name != source.full_local_name
        || target.kind
            != match source.connection_type {
                ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup => {
                    ReducedBusMemberKind::Bus
                }
                _ => ReducedBusMemberKind::Net,
            }
    {
        return false;
    }

    if target.local_name.is_empty() && target.local_name != source.local_name {
        return false;
    }

    if matches!(target.kind, ReducedBusMemberKind::Bus)
        && matches!(
            source.connection_type,
            ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
        )
    {
        if target.members.is_empty() {
            live_bus_member_handles_clone_eq(&target.members, &source.members)
        } else {
            target.members.len() == source.members.len()
                && target.members.iter().zip(source.members.iter()).all(
                    |(target_member, source_member)| {
                        live_bus_member_handle_clone_eq(target_member, source_member)
                    },
                )
        }
    } else {
        live_bus_member_handles_clone_eq(&target.members, &source.members)
    }
}

fn clone_reduced_bus_member_into_live_bus_member(
    target: &mut LiveProjectBusMember,
    source: &ReducedBusMember,
) {
    let existing_local_name = target.local_name.clone();
    let existing_vector_index = target.vector_index;
    let existing_members = target.members.clone();

    target.net_code = source.net_code;
    target.name = source.name.clone();
    if existing_local_name.is_empty() {
        target.local_name = source.local_name.clone();
    }
    target.full_local_name = source.full_local_name.clone();
    if existing_vector_index.is_some() {
        target.vector_index = existing_vector_index;
    } else {
        target.vector_index = source.vector_index;
    }
    target.kind = source.kind.clone();

    if matches!(target.kind, ReducedBusMemberKind::Bus)
        && matches!(source.kind, ReducedBusMemberKind::Bus)
    {
        if existing_members.is_empty() {
            target.members = reduced_bus_members_into_live_handles(source.members.clone());
        } else {
            target.members = existing_members;

            let clone_limit = target.members.len().min(source.members.len());
            for index in 0..clone_limit {
                clone_reduced_bus_member_into_live_bus_member(
                    &mut target.members[index].borrow_mut(),
                    &source.members[index],
                );
            }

            if target.members.len() > source.members.len() {
                target.members.truncate(source.members.len());
            } else if target.members.len() < source.members.len() {
                target.members.extend(
                    source.members[target.members.len()..]
                        .iter()
                        .cloned()
                        .map(|member| Rc::new(RefCell::new(member.into()))),
                );
            }
        }
    } else {
        target.members = reduced_bus_members_into_live_handles(source.members.clone());
    }
}

fn clone_live_bus_member_into_reduced_bus_member(
    target: &mut ReducedBusMember,
    source: &LiveProjectBusMember,
) {
    let existing_local_name = target.local_name.clone();
    let existing_vector_index = target.vector_index;
    let existing_members = target.members.clone();

    target.net_code = source.net_code;
    target.name = source.name.clone();
    if existing_local_name.is_empty() {
        target.local_name = source.local_name.clone();
    }
    target.full_local_name = source.full_local_name.clone();
    if existing_vector_index.is_some() {
        target.vector_index = existing_vector_index;
    } else {
        target.vector_index = source.vector_index;
    }
    target.kind = source.kind.clone();

    if matches!(target.kind, ReducedBusMemberKind::Bus)
        && matches!(source.kind, ReducedBusMemberKind::Bus)
    {
        if existing_members.is_empty() {
            target.members = live_bus_member_handles_to_snapshots(&source.members);
        } else {
            target.members = existing_members;

            let clone_limit = target.members.len().min(source.members.len());
            for index in 0..clone_limit {
                clone_live_bus_member_into_reduced_bus_member(
                    &mut target.members[index],
                    &source.members[index].borrow(),
                );
            }

            if target.members.len() > source.members.len() {
                target.members.truncate(source.members.len());
            } else if target.members.len() < source.members.len() {
                target.members.extend(
                    source.members[target.members.len()..]
                        .iter()
                        .map(live_bus_member_handle_snapshot),
                );
            }
        }
    } else {
        target.members = live_bus_member_handles_to_snapshots(&source.members);
    }
}

fn clone_live_connection_owner_into_live_bus_member(
    target: &mut LiveProjectBusMember,
    source: &LiveProjectConnection,
) {
    let existing_local_name = target.local_name.clone();
    let existing_vector_index = target.vector_index;
    let existing_members = target.members.clone();

    target.net_code = source.net_code;
    target.name = source.name.clone();
    if existing_local_name.is_empty() {
        target.local_name = source.local_name.clone();
    }
    target.full_local_name = source.full_local_name.clone();
    if existing_vector_index.is_some() {
        target.vector_index = existing_vector_index;
    }
    target.kind = match source.connection_type {
        ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup => {
            ReducedBusMemberKind::Bus
        }
        _ => ReducedBusMemberKind::Net,
    };

    if matches!(target.kind, ReducedBusMemberKind::Bus)
        && matches!(
            source.connection_type,
            ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
        )
    {
        if existing_members.is_empty() {
            target.members = source.members.clone();
        } else {
            target.members = existing_members;

            let clone_limit = target.members.len().min(source.members.len());
            for index in 0..clone_limit {
                clone_live_bus_member_handle_into_live_bus_member_handle(
                    &target.members[index],
                    &source.members[index],
                );
            }

            if target.members.len() > source.members.len() {
                target.members.truncate(source.members.len());
            } else if target.members.len() < source.members.len() {
                target
                    .members
                    .extend(source.members[target.members.len()..].iter().cloned());
            }
        }
    } else {
        target.members = source.members.clone();
    }
}

#[cfg(test)]
fn clone_reduced_connection_into_live_connection_owner(
    target: &mut LiveProjectConnection,
    source: &ReducedProjectConnection,
) {
    let existing_local_name = target.local_name.clone();
    let existing_members = target.members.clone();

    target.net_code = source.net_code;
    target.connection_type = source.connection_type;
    target.name = source.name.clone();
    if existing_local_name.is_empty() {
        target.local_name = source.local_name.clone();
    }
    target.full_local_name = source.full_local_name.clone();
    target.sheet_instance_path = source.sheet_instance_path.clone();

    if matches!(
        target.connection_type,
        ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
    ) && matches!(
        source.connection_type,
        ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
    ) {
        if existing_members.is_empty() {
            target.members = reduced_bus_members_into_live_handles(source.members.clone());
        } else {
            target.members = existing_members;

            let clone_limit = target.members.len().min(source.members.len());
            for index in 0..clone_limit {
                clone_reduced_bus_member_into_live_bus_member(
                    &mut target.members[index].borrow_mut(),
                    &source.members[index],
                );
            }

            if target.members.len() > source.members.len() {
                target.members.truncate(source.members.len());
            } else if target.members.len() < source.members.len() {
                target.members.extend(
                    source.members[target.members.len()..]
                        .iter()
                        .cloned()
                        .map(|member| Rc::new(RefCell::new(member.into()))),
                );
            }
        }
    } else {
        target.members = reduced_bus_members_into_live_handles(source.members.clone());
    }
}

fn clone_live_connection_owner_into_live_connection_owner(
    target: &mut LiveProjectConnection,
    source: &LiveProjectConnection,
) {
    let existing_local_name = target.local_name.clone();
    let existing_members = target.members.clone();

    target.net_code = source.net_code;
    target.connection_type = source.connection_type;
    target.name = source.name.clone();
    if existing_local_name.is_empty() {
        target.local_name = source.local_name.clone();
    }
    target.full_local_name = source.full_local_name.clone();
    target.sheet_instance_path = source.sheet_instance_path.clone();

    if matches!(
        target.connection_type,
        ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
    ) && matches!(
        source.connection_type,
        ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
    ) {
        if existing_members.is_empty() {
            target.members = source.members.clone();
        } else {
            target.members = existing_members;

            let clone_limit = target.members.len().min(source.members.len());
            for index in 0..clone_limit {
                clone_live_bus_member_handle_into_live_bus_member_handle(
                    &target.members[index],
                    &source.members[index],
                );
            }

            if target.members.len() > source.members.len() {
                target.members.truncate(source.members.len());
            } else if target.members.len() < source.members.len() {
                target
                    .members
                    .extend(source.members[target.members.len()..].iter().cloned());
            }
        }
    } else {
        target.members = source.members.clone();
    }
}

fn clone_live_connection_owner_into_reduced_connection(
    target: &mut ReducedProjectConnection,
    source: &LiveProjectConnection,
) {
    let existing_local_name = target.local_name.clone();
    let existing_members = target.members.clone();

    target.net_code = source.net_code;
    target.connection_type = source.connection_type;
    target.name = source.name.clone();
    if existing_local_name.is_empty() {
        target.local_name = source.local_name.clone();
    }
    target.full_local_name = source.full_local_name.clone();
    target.sheet_instance_path = source.sheet_instance_path.clone();

    if matches!(
        target.connection_type,
        ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
    ) && matches!(
        source.connection_type,
        ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
    ) {
        if existing_members.is_empty() {
            target.members = live_bus_member_handles_to_snapshots(&source.members);
        } else {
            target.members = existing_members;

            let clone_limit = target.members.len().min(source.members.len());
            for index in 0..clone_limit {
                clone_live_bus_member_into_reduced_bus_member(
                    &mut target.members[index],
                    &source.members[index].borrow(),
                );
            }

            if target.members.len() > source.members.len() {
                target.members.truncate(source.members.len());
            } else if target.members.len() < source.members.len() {
                target.members.extend(
                    source.members[target.members.len()..]
                        .iter()
                        .map(live_bus_member_handle_snapshot),
                );
            }
        }
    } else {
        target.members = live_bus_member_handles_to_snapshots(&source.members);
    }
}

#[derive(Clone)]
struct LiveReducedConnection {
    connection: Rc<RefCell<LiveProjectConnection>>,
}

impl LiveReducedConnection {
    fn new(connection: ReducedProjectConnection) -> Self {
        Self {
            connection: Rc::new(RefCell::new(connection.into())),
        }
    }

    // Upstream parity: reduced local analogue for `SCH_CONNECTION::Clone()`. This still operates
    // on a reduced local connection carrier, but active cloning now mutates one shared live
    // connection owner directly instead of round-tripping through a reduced snapshot. Remaining
    // divergence is fuller item/subgraph pointer topology beyond these local live connection
    // owners.
    fn clone_from(&self, other: &LiveReducedConnection) {
        if Rc::ptr_eq(&self.connection, &other.connection) {
            return;
        }
        let source = other.borrow();
        clone_live_connection_owner_into_live_connection_owner(&mut self.borrow_mut(), &source);
    }

    fn borrow(&self) -> Ref<'_, LiveProjectConnection> {
        self.connection.borrow()
    }

    fn borrow_mut(&self) -> RefMut<'_, LiveProjectConnection> {
        self.connection.borrow_mut()
    }

    fn snapshot(&self) -> ReducedProjectConnection {
        self.borrow().snapshot()
    }

    fn name(&self) -> String {
        self.borrow().name.clone()
    }
}

impl std::fmt::Debug for LiveReducedConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.borrow().fmt(f)
    }
}

impl PartialEq for LiveReducedConnection {
    fn eq(&self, other: &Self) -> bool {
        self.snapshot() == other.snapshot()
    }
}

impl Eq for LiveReducedConnection {}

impl PartialOrd for LiveReducedConnection {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LiveReducedConnection {
    fn cmp(&self, other: &Self) -> Ordering {
        self.snapshot().cmp(&other.snapshot())
    }
}

#[derive(Clone, Debug)]
struct LiveReducedLabelLink {
    at: PointKey,
    kind: LabelKind,
    identity: Option<ReducedProjectDriverIdentity>,
    connection: LiveReducedConnection,
    driver: Option<LiveProjectStrongDriverHandle>,
}
type LiveReducedLabelLinkHandle = Rc<RefCell<LiveReducedLabelLink>>;

#[derive(Clone, Debug)]
struct LiveReducedHierSheetPinLink {
    at: PointKey,
    child_sheet_uuid: Option<String>,
    identity: Option<ReducedProjectDriverIdentity>,
    connection: LiveReducedConnection,
    driver: Option<LiveProjectStrongDriverHandle>,
}
type LiveReducedHierSheetPinLinkHandle = Rc<RefCell<LiveReducedHierSheetPinLink>>;

#[derive(Clone, Debug)]
struct LiveReducedHierPortLink {
    at: PointKey,
    identity: Option<ReducedProjectDriverIdentity>,
    connection: LiveReducedConnection,
    driver: Option<LiveProjectStrongDriverHandle>,
}
type LiveReducedHierPortLinkHandle = Rc<RefCell<LiveReducedHierPortLink>>;

#[derive(Clone, Debug)]
struct LiveReducedBasePinPayload {
    key: ReducedNetBasePinKey,
    identity: Option<ReducedProjectDriverIdentity>,
}

#[derive(Clone, Debug)]
// Upstream parity: reduced local live pin-item payload under the shared graph. This still keeps a
// reduced projected pin payload instead of a live `SCH_PIN*`, but it now separates immutable pin
// identity/type data from the shared live connection owner so the active live pin carrier stops
// shadowing a second copied reduced connection beside the real live connection handle.
struct LiveReducedBasePin {
    pin: LiveReducedBasePinPayload,
    connection: LiveReducedConnection,
    driver: Option<LiveProjectStrongDriverHandle>,
}

type LiveReducedBasePinHandle = Rc<RefCell<LiveReducedBasePin>>;

fn live_strong_driver_handle_snapshot(
    driver: &LiveProjectStrongDriverHandle,
) -> ReducedProjectStrongDriver {
    driver.borrow().snapshot()
}

fn live_optional_driver_snapshot(
    driver: &Option<LiveProjectStrongDriverHandle>,
) -> Option<ReducedProjectStrongDriver> {
    driver.as_ref().map(live_strong_driver_handle_snapshot)
}

impl PartialEq for LiveReducedHierSheetPinLink {
    fn eq(&self, other: &Self) -> bool {
        (
            self.at,
            &self.child_sheet_uuid,
            &self.connection,
            live_optional_driver_snapshot(&self.driver),
        ) == (
            other.at,
            &other.child_sheet_uuid,
            &other.connection,
            live_optional_driver_snapshot(&other.driver),
        )
    }
}

impl Eq for LiveReducedHierSheetPinLink {}

impl PartialOrd for LiveReducedHierSheetPinLink {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LiveReducedHierSheetPinLink {
    fn cmp(&self, other: &Self) -> Ordering {
        (
            self.at,
            &self.child_sheet_uuid,
            &self.connection,
            live_optional_driver_snapshot(&self.driver),
        )
            .cmp(&(
                other.at,
                &other.child_sheet_uuid,
                &other.connection,
                live_optional_driver_snapshot(&other.driver),
            ))
    }
}

impl PartialEq for LiveReducedHierPortLink {
    fn eq(&self, other: &Self) -> bool {
        (
            self.at,
            &self.connection,
            live_optional_driver_snapshot(&self.driver),
        ) == (
            other.at,
            &other.connection,
            live_optional_driver_snapshot(&other.driver),
        )
    }
}

impl Eq for LiveReducedHierPortLink {}

impl PartialOrd for LiveReducedHierPortLink {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LiveReducedHierPortLink {
    fn cmp(&self, other: &Self) -> Ordering {
        (
            self.at,
            &self.connection,
            live_optional_driver_snapshot(&self.driver),
        )
            .cmp(&(
                other.at,
                &other.connection,
                live_optional_driver_snapshot(&other.driver),
            ))
    }
}

impl PartialEq for LiveReducedBasePin {
    fn eq(&self, other: &Self) -> bool {
        (
            self.pin.key.clone(),
            self.connection.snapshot(),
            live_optional_driver_snapshot(&self.driver),
        ) == (
            other.pin.key.clone(),
            other.connection.snapshot(),
            live_optional_driver_snapshot(&other.driver),
        )
    }
}

impl Eq for LiveReducedBasePin {}

impl PartialOrd for LiveReducedBasePin {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LiveReducedBasePin {
    fn cmp(&self, other: &Self) -> Ordering {
        (
            self.pin.key.clone(),
            self.connection.snapshot(),
            live_optional_driver_snapshot(&self.driver),
        )
            .cmp(&(
                other.pin.key.clone(),
                other.connection.snapshot(),
                live_optional_driver_snapshot(&other.driver),
            ))
    }
}

#[derive(Clone, Debug)]
struct LiveReducedSubgraphWireItem {
    start: PointKey,
    end: PointKey,
    is_bus_entry: bool,
    connection: Option<LiveReducedConnection>,
    #[cfg(test)]
    connected_bus_connection: Option<LiveReducedConnection>,
    connected_bus_item_handle: Option<Weak<RefCell<LiveReducedSubgraphWireItem>>>,
    parent_subgraph_handle: Option<Weak<RefCell<LiveReducedSubgraph>>>,
}

impl PartialEq for LiveReducedSubgraphWireItem {
    fn eq(&self, other: &Self) -> bool {
        self.start == other.start
            && self.end == other.end
            && self.is_bus_entry == other.is_bus_entry
            && live_reduced_subgraph_wire_item_extra_eq(self, other)
    }
}

impl Eq for LiveReducedSubgraphWireItem {}

#[cfg(test)]
fn live_reduced_subgraph_wire_item_extra_eq(
    left: &LiveReducedSubgraphWireItem,
    right: &LiveReducedSubgraphWireItem,
) -> bool {
    left.connected_bus_connection == right.connected_bus_connection
}

#[cfg(not(test))]
fn live_reduced_subgraph_wire_item_extra_eq(
    _left: &LiveReducedSubgraphWireItem,
    _right: &LiveReducedSubgraphWireItem,
) -> bool {
    true
}

type LiveReducedSubgraphWireItemHandle = Rc<RefCell<LiveReducedSubgraphWireItem>>;

#[derive(Clone, Debug)]
struct LiveReducedSubgraphLink {
    member: LiveProjectBusMemberHandle,
    #[cfg(test)]
    subgraph_index: usize,
    subgraph_handle: Option<Weak<RefCell<LiveReducedSubgraph>>>,
}

#[cfg(test)]
fn live_reduced_subgraph_link_extra_projection_eq(
    left: &LiveReducedSubgraphLink,
    right: &LiveReducedSubgraphLink,
) -> bool {
    left.subgraph_index == right.subgraph_index
}

#[cfg(not(test))]
fn live_reduced_subgraph_link_extra_projection_eq(
    _left: &LiveReducedSubgraphLink,
    _right: &LiveReducedSubgraphLink,
) -> bool {
    true
}

impl PartialEq for LiveReducedSubgraphLink {
    fn eq(&self, other: &Self) -> bool {
        live_bus_member_handles_eq(&self.member, &other.member)
            && live_reduced_subgraph_link_extra_projection_eq(self, other)
    }
}

impl Eq for LiveReducedSubgraphLink {}

type LiveReducedSubgraphLinkHandle = Rc<RefCell<LiveReducedSubgraphLink>>;

impl PartialOrd for LiveReducedSubgraphLink {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LiveReducedSubgraphLink {
    fn cmp(&self, other: &Self) -> Ordering {
        let member_cmp = live_bus_member_handles_cmp(&self.member, &other.member);
        if member_cmp != Ordering::Equal {
            return member_cmp;
        }

        #[cfg(test)]
        {
            self.subgraph_index.cmp(&other.subgraph_index)
        }

        #[cfg(not(test))]
        {
            Ordering::Equal
        }
    }
}

#[derive(Clone, Debug)]
struct LiveReducedSubgraph {
    #[cfg(test)]
    source_index: usize,
    driver_connection: LiveReducedConnection,
    drivers: Vec<LiveProjectStrongDriverHandle>,
    chosen_driver: Option<LiveProjectStrongDriverHandle>,
    sheet_instance_path: String,
    bus_neighbor_links: Vec<LiveReducedSubgraphLinkHandle>,
    bus_parent_links: Vec<LiveReducedSubgraphLinkHandle>,
    #[cfg(test)]
    bus_parent_indexes: Vec<usize>,
    bus_parent_handles: Vec<Weak<RefCell<LiveReducedSubgraph>>>,
    base_pins: Vec<LiveReducedBasePinHandle>,
    #[cfg(test)]
    hier_parent_index: Option<usize>,
    #[cfg(test)]
    hier_child_indexes: Vec<usize>,
    hier_parent_handle: Option<Weak<RefCell<LiveReducedSubgraph>>>,
    hier_child_handles: Vec<Weak<RefCell<LiveReducedSubgraph>>>,
    label_links: Vec<LiveReducedLabelLinkHandle>,
    hier_sheet_pins: Vec<LiveReducedHierSheetPinLinkHandle>,
    hier_ports: Vec<LiveReducedHierPortLinkHandle>,
    bus_items: Vec<LiveReducedSubgraphWireItemHandle>,
    wire_items: Vec<LiveReducedSubgraphWireItemHandle>,
    dirty: bool,
}

#[cfg(test)]
fn live_reduced_subgraph_extra_projection_eq(
    left: &LiveReducedSubgraph,
    right: &LiveReducedSubgraph,
) -> bool {
    left.source_index == right.source_index
        && left.bus_parent_indexes == right.bus_parent_indexes
        && left.hier_parent_index == right.hier_parent_index
        && left.hier_child_indexes == right.hier_child_indexes
}

#[cfg(not(test))]
fn live_reduced_subgraph_extra_projection_eq(
    _left: &LiveReducedSubgraph,
    _right: &LiveReducedSubgraph,
) -> bool {
    true
}

fn live_base_pin_handles_to_snapshots(
    base_pins: &[LiveReducedBasePinHandle],
) -> Vec<ReducedNetBasePinKey> {
    base_pins
        .iter()
        .map(|base_pin| base_pin.borrow().pin.key.clone())
        .collect()
}

impl PartialEq for LiveReducedSubgraph {
    fn eq(&self, other: &Self) -> bool {
        self.driver_connection == other.driver_connection
            && live_reduced_subgraph_driver_priority(self)
                == live_reduced_subgraph_driver_priority(other)
            && live_strong_driver_handles_to_snapshots(&self.drivers)
                == live_strong_driver_handles_to_snapshots(&other.drivers)
            && self.sheet_instance_path == other.sheet_instance_path
            && self.bus_neighbor_links == other.bus_neighbor_links
            && self.bus_parent_links == other.bus_parent_links
            && live_reduced_subgraph_extra_projection_eq(self, other)
            && live_base_pin_handles_to_snapshots(&self.base_pins)
                == live_base_pin_handles_to_snapshots(&other.base_pins)
            && self.label_links == other.label_links
            && self.hier_sheet_pins == other.hier_sheet_pins
            && self.hier_ports == other.hier_ports
            && self.bus_items == other.bus_items
            && self.wire_items == other.wire_items
            && self.dirty == other.dirty
    }
}

impl Eq for LiveReducedSubgraph {}

type LiveReducedSubgraphHandle = Rc<RefCell<LiveReducedSubgraph>>;

fn live_subgraph_handle_id(handle: &LiveReducedSubgraphHandle) -> usize {
    Rc::as_ptr(handle) as usize
}

fn live_subgraph_projection_index(
    live_subgraphs: &[LiveReducedSubgraphHandle],
    target: &LiveReducedSubgraphHandle,
) -> usize {
    live_subgraphs
        .iter()
        .position(|handle| Rc::ptr_eq(handle, target))
        .expect("live subgraph handle must belong to active graph")
}

fn live_subgraph_link_handle_cmp(
    left: &LiveReducedSubgraphLinkHandle,
    right: &LiveReducedSubgraphLinkHandle,
) -> Ordering {
    let left = left.borrow();
    let right = right.borrow();
    left.cmp(&right)
}

fn sort_dedup_live_subgraph_link_handles(links: &mut Vec<LiveReducedSubgraphLinkHandle>) {
    links.sort_by(live_subgraph_link_handle_cmp);
    links.dedup_by(|left, right| {
        let left = left.borrow();
        let right = right.borrow();
        *left == *right
    });
}

fn live_subgraph_strong_driver_count(subgraph: &LiveReducedSubgraph) -> usize {
    subgraph.drivers.len()
}

fn live_subgraph_has_local_driver(subgraph: &LiveReducedSubgraph) -> bool {
    live_reduced_subgraph_driver_priority(subgraph) < 6
}

#[cfg(test)]
fn live_subgraph_has_hier_pins(subgraph: &LiveReducedSubgraph) -> bool {
    !subgraph.hier_sheet_pins.is_empty()
}

#[cfg(test)]
fn live_subgraph_has_hier_ports(subgraph: &LiveReducedSubgraph) -> bool {
    !subgraph.hier_ports.is_empty()
}

fn live_subgraph_base_pin_count(subgraph: &LiveReducedSubgraph) -> usize {
    subgraph.base_pins.len()
}

fn live_reduced_subgraph_driver_priority(subgraph: &LiveReducedSubgraph) -> i32 {
    subgraph
        .drivers
        .first()
        .map(|driver| live_project_strong_driver_priority(&driver.borrow()))
        .or_else(|| {
            (!matches!(
                subgraph.driver_connection.borrow().connection_type,
                ReducedProjectConnectionType::None
            ))
            .then_some(1)
        })
        .unwrap_or(0)
}

fn live_subgraph_is_self_driven_symbol_pin(subgraph: &LiveReducedSubgraph) -> bool {
    live_subgraph_strong_driver_count(subgraph) == 0
        && live_subgraph_base_pin_count(subgraph) == 1
        && subgraph.hier_sheet_pins.is_empty()
}

fn live_subgraph_is_self_driven_sheet_pin(subgraph: &LiveReducedSubgraph) -> bool {
    live_subgraph_strong_driver_count(subgraph) == 0
        && subgraph.base_pins.is_empty()
        && (!subgraph.hier_sheet_pins.is_empty()
            || {
                #[cfg(test)]
                {
                    subgraph.hier_parent_index.is_some() || !subgraph.hier_child_indexes.is_empty()
                }
                #[cfg(not(test))]
                {
                    false
                }
            }
            || subgraph
                .hier_parent_handle
                .as_ref()
                .and_then(Weak::upgrade)
                .is_some()
            || !subgraph.hier_child_handles.is_empty())
}

// Upstream parity: local bridge toward shared mutable `CONNECTION_SUBGRAPH` ownership during live
// graph propagation. This still wraps the existing reduced live subgraph carrier instead of a full
// local `CONNECTION_SUBGRAPH` analogue, but it moves the active recursive graph build off whole
// value-owned subgraph moves and onto shared live handles.
fn build_live_reduced_subgraph_handles(
    reduced_subgraphs: &[ReducedProjectSubgraphEntry],
) -> Vec<LiveReducedSubgraphHandle> {
    let handles = build_live_reduced_subgraphs(reduced_subgraphs)
        .into_iter()
        .map(|subgraph| Rc::new(RefCell::new(subgraph)))
        .collect::<Vec<_>>();
    attach_live_subgraph_links_to_handles(&handles, reduced_subgraphs);
    attach_live_bus_parent_handles_to_handles(&handles, reduced_subgraphs);
    attach_live_hierarchy_links_to_handles(&handles, reduced_subgraphs);
    attach_live_strong_driver_owners_to_handles(&handles, reduced_subgraphs);
    for handle in &handles {
        sync_live_reduced_base_pin_connections_from_driver_handle(handle);
    }
    attach_live_wire_item_parent_handles_to_handles(&handles);
    attach_live_connected_bus_items_to_handles(&handles);
    handles
}

// Upstream parity: local bridge toward pointer-style bus parent/neighbor topology on the shared
// live subgraph graph. The non-test live payload no longer keeps copied reduced target indexes for
// active traversal; handle attachment is seeded directly from the reduced graph during
// construction and reduced indexes are rebuilt only when projecting back out. Bus links are now
// shared live owners too, so active propagation mutates attached link state instead of copied
// value links on each subgraph pass.
fn attach_live_subgraph_links_to_handles(
    live_subgraphs: &[LiveReducedSubgraphHandle],
    reduced_subgraphs: &[ReducedProjectSubgraphEntry],
) {
    for (index, handle) in live_subgraphs.iter().enumerate() {
        let mut subgraph = handle.borrow_mut();

        for (link, reduced_link) in subgraph
            .bus_neighbor_links
            .iter_mut()
            .zip(reduced_subgraphs[index].bus_neighbor_links.iter())
        {
            link.borrow_mut().subgraph_handle = live_subgraphs
                .get(reduced_link.subgraph_index)
                .map(Rc::downgrade);
        }

        for (link, reduced_link) in subgraph
            .bus_parent_links
            .iter_mut()
            .zip(reduced_subgraphs[index].bus_parent_links.iter())
        {
            link.borrow_mut().subgraph_handle = live_subgraphs
                .get(reduced_link.subgraph_index)
                .map(Rc::downgrade);
        }
    }
}

// Upstream parity: local bridge toward item-pointer topology on the shared live graph. Wire and
// bus items now keep a live reference back to their owning subgraph handle instead of only
// existing as detached value payload on that subgraph, and bus items also keep a shared live
// connection owner from that parent subgraph so attached bus-entry items can follow an item-owned
// bus connection path before projection collapses back to reduced subgraph indexes.
fn attach_live_wire_item_parent_handles_to_handles(live_subgraphs: &[LiveReducedSubgraphHandle]) {
    for handle in live_subgraphs {
        let subgraph = handle.borrow_mut();
        for item in &subgraph.bus_items {
            let mut item_ref = item.borrow_mut();
            item_ref.parent_subgraph_handle = Some(Rc::downgrade(handle));
            item_ref.connection = Some(subgraph.driver_connection.clone());
        }
        for item in &subgraph.wire_items {
            item.borrow_mut().parent_subgraph_handle = Some(Rc::downgrade(handle));
        }
    }
}

// Upstream parity: local bridge toward pointer-style plain bus-parent topology on the shared live
// subgraph graph. This still seeds from reduced parent indexes during construction, but the active
// traversal can now follow attached parent-bus handles even when no current member link exists.
fn attach_live_bus_parent_handles_to_handles(
    live_subgraphs: &[LiveReducedSubgraphHandle],
    reduced_subgraphs: &[ReducedProjectSubgraphEntry],
) {
    for (index, handle) in live_subgraphs.iter().enumerate() {
        let mut subgraph = handle.borrow_mut();
        subgraph.bus_parent_handles = reduced_subgraphs[index]
            .bus_parent_indexes
            .iter()
            .filter_map(|index| live_subgraphs.get(*index).map(Rc::downgrade))
            .collect();
    }
}

// Upstream parity: local bridge toward pointer-style hierarchy topology on the shared live
// subgraph graph. This still seeds from reduced hierarchy indexes during construction, but the
// active live traversal can now follow attached parent/child handles instead of copied indexes.
fn attach_live_hierarchy_links_to_handles(
    live_subgraphs: &[LiveReducedSubgraphHandle],
    reduced_subgraphs: &[ReducedProjectSubgraphEntry],
) {
    for (index, handle) in live_subgraphs.iter().enumerate() {
        let mut subgraph = handle.borrow_mut();
        subgraph.hier_parent_handle = reduced_subgraphs[index]
            .hier_parent_index
            .and_then(|index| live_subgraphs.get(index).map(Rc::downgrade));
        subgraph.hier_child_handles = reduced_subgraphs[index]
            .hier_child_indexes
            .iter()
            .filter_map(|index| live_subgraphs.get(*index).map(Rc::downgrade))
            .collect();
    }
}

// Upstream parity: local bridge toward live driver-item ownership on the shared live graph.
// KiCad's strong-driver selection ultimately points back to live items; this reduced bridge still
// keeps copied driver records, but it now attaches the exercised label, sheet-pin, and symbol-pin
// drivers to shared live item owners on the active graph instead of leaving every strong driver as
// a detached copied struct, and now also attaches those item owners back onto the same shared live
// strong-driver owners used by the subgraph driver list. Active strong-driver connection reads now
// prefer those shared item owners, and symbol-pin strong drivers now read their connection through
// the attached base-pin owner instead of carrying a second driver-side connection cache. The
// attached base pin still seeds its own live connection owner from the floating strong-driver
// connection so later per-pin item branches have a real owner to update without borrowing the
// whole subgraph driver. Once the chosen driver is attached, the active live subgraph now also
// points its `driver_connection` at that same chosen item-owned live connection owner, matching
// KiCad's `m_driver_connection = m_driver->Connection(...)` branch more closely instead of keeping
// a parallel subgraph-owned connection copy on the active path. Remaining divergence is the fuller
// live driver-item object graph and the still-missing live `SCH_CONNECTION` /
// `CONNECTION_SUBGRAPH` object graph behind these handles.
fn attach_live_strong_driver_owners_to_handles(
    live_subgraphs: &[LiveReducedSubgraphHandle],
    reduced_subgraphs: &[ReducedProjectSubgraphEntry],
) {
    for (index, handle) in live_subgraphs.iter().enumerate() {
        let mut subgraph = handle.borrow_mut();
        let chosen_identity =
            reduced_project_subgraph_driver_identity(&reduced_subgraphs[index]).cloned();
        let chosen_connection = reduced_subgraph_driver_connection(&reduced_subgraphs[index]);
        let live_drivers = subgraph.drivers.clone();

        for (driver, reduced_driver) in live_drivers
            .iter()
            .zip(reduced_subgraphs[index].drivers.iter())
        {
            let identity = reduced_driver.identity.clone();
            let driver_kind = reduced_driver.kind;
            let priority = reduced_driver.priority;
            let floating_connection = match &driver.borrow().owner {
                LiveProjectStrongDriverOwner::Floating { connection, .. } => connection.clone(),
                _ => live_project_strong_driver_connection(&driver.borrow()),
            };
            let owner = match identity {
                Some(ReducedProjectDriverIdentity::Label { at, kind, .. }) => {
                    if kind == reduced_label_kind_sort_key(LabelKind::Hierarchical) {
                        subgraph
                            .hier_ports
                            .iter()
                            .find(|port| port.borrow().at == at)
                            .map(|port| {
                                let mut port_ref = port.borrow_mut();
                                port_ref.identity = identity.clone();
                                port_ref.driver = Some(driver.clone());
                                LiveProjectStrongDriverOwner::HierPort {
                                    owner: Rc::downgrade(port),
                                    kind: driver_kind,
                                    priority,
                                }
                            })
                            .unwrap_or(LiveProjectStrongDriverOwner::Floating {
                                identity: identity.clone(),
                                connection: floating_connection.clone(),
                                kind: driver_kind,
                                priority,
                            })
                    } else {
                        subgraph
                            .label_links
                            .iter()
                            .find(|link| {
                                let link = link.borrow();
                                link.at == at && reduced_label_kind_sort_key(link.kind) == kind
                            })
                            .map(|link| {
                                let mut link_ref = link.borrow_mut();
                                link_ref.identity = identity.clone();
                                link_ref.driver = Some(driver.clone());
                                LiveProjectStrongDriverOwner::Label {
                                    owner: Rc::downgrade(link),
                                    kind: driver_kind,
                                    priority,
                                }
                            })
                            .unwrap_or(LiveProjectStrongDriverOwner::Floating {
                                identity: identity.clone(),
                                connection: floating_connection.clone(),
                                kind: driver_kind,
                                priority,
                            })
                    }
                }
                Some(ReducedProjectDriverIdentity::SheetPin { at, .. }) => subgraph
                    .hier_sheet_pins
                    .iter()
                    .find(|pin| pin.borrow().at == at)
                    .map(|pin| {
                        let mut pin_ref = pin.borrow_mut();
                        pin_ref.identity = identity.clone();
                        pin_ref.driver = Some(driver.clone());
                        LiveProjectStrongDriverOwner::SheetPin {
                            owner: Rc::downgrade(pin),
                            kind: driver_kind,
                            priority,
                        }
                    })
                    .unwrap_or(LiveProjectStrongDriverOwner::Floating {
                        identity: identity.clone(),
                        connection: floating_connection.clone(),
                        kind: driver_kind,
                        priority,
                    }),
                Some(ReducedProjectDriverIdentity::SymbolPin {
                    ref symbol_uuid,
                    at,
                    ref pin_number,
                    ..
                }) => subgraph
                    .base_pins
                    .iter()
                    .find(|base_pin| {
                        let key = &base_pin.borrow().pin.key;
                        key.symbol_uuid.as_ref() == symbol_uuid.as_ref()
                            && key.at == at
                            && key.number.as_ref() == pin_number.as_ref()
                    })
                    .map(|base_pin| {
                        {
                            let base_pin_ref = base_pin.borrow();
                            base_pin_ref.connection.clone_from(&floating_connection);
                        }
                        let mut base_pin_ref = base_pin.borrow_mut();
                        base_pin_ref.pin.identity = identity.clone();
                        base_pin_ref.driver = Some(driver.clone());
                        LiveProjectStrongDriverOwner::SymbolPin {
                            owner: Rc::downgrade(base_pin),
                            kind: driver_kind,
                            priority,
                        }
                    })
                    .unwrap_or(LiveProjectStrongDriverOwner::Floating {
                        identity: identity.clone(),
                        connection: floating_connection.clone(),
                        kind: driver_kind,
                        priority,
                    }),
                None => LiveProjectStrongDriverOwner::Floating {
                    identity,
                    connection: floating_connection,
                    kind: driver_kind,
                    priority,
                },
            };
            let mut driver_ref = driver.borrow_mut();
            driver_ref.owner = owner;
            drop(driver_ref);

            let is_chosen_driver = chosen_identity
                .as_ref()
                .map(|identity| reduced_driver.identity.as_ref() == Some(identity))
                .unwrap_or_else(|| reduced_driver.connection == chosen_connection);

            if is_chosen_driver {
                subgraph.chosen_driver = Some(driver.clone());
                subgraph.driver_connection =
                    live_project_strong_driver_connection(&driver.borrow());
            }
        }
    }
}

// Upstream parity: local bridge for the self-driven symbol-pin item branch on the shared live
// subgraph owner. KiCad keeps pin-owned connection state under the same live graph; this reduced
// bridge only does that for the exercised single-pin self-driven symbol branch so weak
// `Net-(` -> `unconnected-(` renames and later driver updates stop bypassing the base-pin owner.
// Base pins now always carry a live connection owner instead of an optional carrier, but
// remaining divergence is fuller per-pin live connection ownership for multi-pin symbol/power-pin
// branches.
fn sync_live_reduced_base_pin_connections_from_driver_handle(handle: &LiveReducedSubgraphHandle) {
    let driver_connection = handle.borrow().driver_connection.clone();
    let subgraph = handle.borrow_mut();

    if !live_subgraph_is_self_driven_symbol_pin(&subgraph) {
        return;
    }

    for base_pin in &subgraph.base_pins {
        base_pin.borrow().connection.clone_from(&driver_connection);
    }
}

// Upstream parity: local bridge for item-owned connection refresh against the shared live
// subgraph owner. This still mutates reduced live item carriers instead of real `SCH_ITEM`
// pointers, but labels, sheet pins, hierarchy ports, and the exercised self-driven single-pin
// base-pin branch now all sit on shared live item owners under that subgraph graph instead of
// copied per-pass wrapper values. Base pins now always carry a live connection owner even before
// the fuller multi-pin pin-owned branch is ported, and item refresh now also preserves KiCad's
// exercised bus/net mismatch skip plus the `item != m_driver` guard by reading the already-chosen
// live strong-driver handle from the subgraph owner instead of rediscovering it from names after
// attachment.
fn sync_live_reduced_item_connections_from_driver_handle(handle: &LiveReducedSubgraphHandle) {
    let driver_connection = handle.borrow().driver_connection.clone();
    let driver_connection_type = driver_connection.borrow().connection_type;
    let subgraph = handle.borrow_mut();
    let chosen_driver = subgraph.chosen_driver.clone();

    for link in &subgraph.label_links {
        if chosen_driver
            .as_ref()
            .zip(link.borrow().driver.as_ref())
            .is_some_and(|(chosen, owner)| Rc::ptr_eq(chosen, owner))
        {
            continue;
        }

        if reduced_connection_kind_mismatch(
            driver_connection_type,
            link.borrow().connection.borrow().connection_type,
        ) {
            continue;
        }

        link.borrow_mut().connection.clone_from(&driver_connection);
    }
    for pin in &subgraph.hier_sheet_pins {
        if chosen_driver
            .as_ref()
            .zip(pin.borrow().driver.as_ref())
            .is_some_and(|(chosen, owner)| Rc::ptr_eq(chosen, owner))
        {
            continue;
        }

        if reduced_connection_kind_mismatch(
            driver_connection_type,
            pin.borrow().connection.borrow().connection_type,
        ) {
            continue;
        }

        pin.borrow_mut().connection.clone_from(&driver_connection);
    }
    for port in &subgraph.hier_ports {
        if chosen_driver
            .as_ref()
            .zip(port.borrow().driver.as_ref())
            .is_some_and(|(chosen, owner)| Rc::ptr_eq(chosen, owner))
        {
            continue;
        }

        if reduced_connection_kind_mismatch(
            driver_connection_type,
            port.borrow().connection.borrow().connection_type,
        ) {
            continue;
        }

        port.borrow_mut().connection.clone_from(&driver_connection);
    }

    drop(subgraph);
    sync_live_reduced_base_pin_connections_from_driver_handle(handle);
}

// Upstream parity: reduced local builder for live graph-owned text and hierarchy link connection
// carriers. This is still not pointer-shared `SCH_ITEM` ownership, but it lets the live graph keep
// per-item connection state on shared local item owners instead of projecting every
// label/sheet-pin/hier-port/wire-item from the chosen subgraph driver only at the end of
// propagation. The active payload now also keeps shared live base-pin payload directly instead of
// a copied base-pin count summary, seeds every base pin from an `updatePinConnectivity()`-style
// net connection at build time, derives driver priority from the shared live driver owner instead
// of caching one more copied summary field, and attaches the exercised label/sheet-pin/symbol-pin
// strong drivers back onto shared live item owners via the same shared live strong-driver owners
// used by the subgraph driver list. Those live strong-driver owners now also keep a live
// connection carrier instead of parallel live name fields. Remaining divergence is the
// still-missing fuller live driver-item object graph.
fn build_live_reduced_subgraphs(
    reduced_subgraphs: &[ReducedProjectSubgraphEntry],
) -> Vec<LiveReducedSubgraph> {
    let live_subgraphs = reduced_subgraphs
        .iter()
        .enumerate()
        .map(|(_index, subgraph)| LiveReducedSubgraph {
            #[cfg(test)]
            source_index: _index,
            driver_connection: LiveReducedConnection::new(reduced_subgraph_driver_connection(
                subgraph,
            )),
            drivers: reduced_strong_drivers_into_live_handles(subgraph.drivers.clone()),
            chosen_driver: None,
            sheet_instance_path: subgraph.sheet_instance_path.clone(),
            bus_neighbor_links: subgraph
                .bus_neighbor_links
                .iter()
                .cloned()
                .map(|link| {
                    Rc::new(RefCell::new(LiveReducedSubgraphLink {
                        member: Rc::new(RefCell::new(link.member.into())),
                        #[cfg(test)]
                        subgraph_index: link.subgraph_index,
                        subgraph_handle: None,
                    }))
                })
                .collect(),
            bus_parent_links: subgraph
                .bus_parent_links
                .iter()
                .cloned()
                .map(|link| {
                    Rc::new(RefCell::new(LiveReducedSubgraphLink {
                        member: Rc::new(RefCell::new(link.member.into())),
                        #[cfg(test)]
                        subgraph_index: link.subgraph_index,
                        subgraph_handle: None,
                    }))
                })
                .collect(),
            #[cfg(test)]
            bus_parent_indexes: subgraph.bus_parent_indexes.clone(),
            bus_parent_handles: Vec::new(),
            base_pins: subgraph
                .base_pins
                .iter()
                .cloned()
                .map(|pin| {
                    Rc::new(RefCell::new(LiveReducedBasePin {
                        pin: LiveReducedBasePinPayload {
                            key: pin.key.clone(),
                            identity: None,
                        },
                        connection: LiveReducedConnection::new(pin.connection.clone()),
                        driver: None,
                    }))
                })
                .collect(),
            #[cfg(test)]
            hier_parent_index: subgraph.hier_parent_index,
            #[cfg(test)]
            hier_child_indexes: subgraph.hier_child_indexes.clone(),
            hier_parent_handle: None,
            hier_child_handles: Vec::new(),
            label_links: subgraph
                .label_links
                .iter()
                .cloned()
                .map(|link| {
                    Rc::new(RefCell::new(LiveReducedLabelLink {
                        at: link.at,
                        kind: link.kind,
                        identity: None,
                        connection: LiveReducedConnection::new(link.connection),
                        driver: None,
                    }))
                })
                .collect(),
            hier_sheet_pins: subgraph
                .hier_sheet_pins
                .iter()
                .cloned()
                .map(|pin| {
                    Rc::new(RefCell::new(LiveReducedHierSheetPinLink {
                        at: pin.at,
                        child_sheet_uuid: pin.child_sheet_uuid,
                        identity: None,
                        connection: LiveReducedConnection::new(pin.connection),
                        driver: None,
                    }))
                })
                .collect(),
            hier_ports: subgraph
                .hier_ports
                .iter()
                .cloned()
                .map(|port| {
                    Rc::new(RefCell::new(LiveReducedHierPortLink {
                        at: port.at,
                        identity: None,
                        connection: LiveReducedConnection::new(port.connection),
                        driver: None,
                    }))
                })
                .collect(),
            bus_items: subgraph
                .bus_items
                .iter()
                .cloned()
                .map(|item| {
                    Rc::new(RefCell::new(LiveReducedSubgraphWireItem {
                        start: item.start,
                        end: item.end,
                        is_bus_entry: item.is_bus_entry,
                        connection: None,
                        #[cfg(test)]
                        connected_bus_connection: None,
                        connected_bus_item_handle: None,
                        parent_subgraph_handle: None,
                    }))
                })
                .collect(),
            wire_items: subgraph
                .wire_items
                .iter()
                .cloned()
                .map(|item| {
                    Rc::new(RefCell::new(LiveReducedSubgraphWireItem {
                        start: item.start,
                        end: item.end,
                        is_bus_entry: item.is_bus_entry,
                        connection: None,
                        #[cfg(test)]
                        connected_bus_connection: None,
                        connected_bus_item_handle: None,
                        parent_subgraph_handle: None,
                    }))
                })
                .collect(),
            dirty: true,
        })
        .collect::<Vec<_>>();

    #[cfg(test)]
    {
        let mut live_subgraphs = live_subgraphs;
        attach_live_connected_bus_items(&mut live_subgraphs);
        live_subgraphs
    }
    #[cfg(not(test))]
    live_subgraphs
}

// Upstream parity: local bridge for connected-bus ownership on the shared live subgraph graph.
// This still uses reduced wire geometry instead of real `SCH_BUS_WIRE_ENTRY` / `SCH_LINE*`
// pointers, but bus and wire items on the active live graph now keep shared local item owners,
// and bus entries keep a live reference to the attached bus item owner instead of carrying a
// copied reduced bus index as active ownership. Projection can derive the parent subgraph from
// that bus item owner when callers still need reduced subgraph indexes.
fn attach_live_connected_bus_items_to_handles(live_subgraphs: &[LiveReducedSubgraphHandle]) {
    let bus_subgraphs = live_subgraphs
        .iter()
        .filter_map(|handle| {
            let subgraph = handle.borrow();
            (!subgraph.bus_items.is_empty()).then(|| {
                (
                    subgraph.sheet_instance_path.clone(),
                    subgraph.bus_items.clone(),
                )
            })
        })
        .collect::<Vec<_>>();

    for handle in live_subgraphs {
        let sheet_instance_path = handle.borrow().sheet_instance_path.clone();
        let subgraph = handle.borrow_mut();

        for item_handle in &subgraph.wire_items {
            if !item_handle.borrow().is_bus_entry {
                continue;
            }
            let item = item_handle.borrow();

            let attached_bus = bus_subgraphs
                .iter()
                .find(|(bus_sheet_path, bus_items)| {
                    *bus_sheet_path == sheet_instance_path
                        && bus_items.iter().any(|bus_item| {
                            let bus_item = bus_item.borrow();
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
                .and_then(|(_, bus_items)| {
                    bus_items
                        .iter()
                        .find(|bus_item| {
                            let bus_item = bus_item.borrow();
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
                        .map(Rc::downgrade)
                });

            drop(item);
            let mut item = item_handle.borrow_mut();
            item.connected_bus_item_handle = attached_bus;
        }
    }
}

// Upstream parity: reduced local analogue for the bus-entry connected-bus item owner during live
// graph build. This still identifies the attached bus from reduced wire geometry and stores a live
// local connection owner instead of real `SCH_LINE*` item pointers, but it
// moves connected-bus ownership onto the live graph objects so later live graph updates can follow
// the attached bus connection owner directly. Live wire-item ownership is now shared on the active
// live graph instead of copied per-subgraph wrapper state. Remaining divergence is that the
// wire-item payload itself is still a reduced carrier rather than a fuller live item/pointer
// object.
#[cfg(test)]
fn attach_live_connected_bus_items(live_subgraphs: &mut [LiveReducedSubgraph]) {
    let bus_subgraphs = live_subgraphs
        .iter()
        .enumerate()
        .filter(|(_, subgraph)| !subgraph.bus_items.is_empty())
        .map(|(index, subgraph)| {
            (
                index,
                subgraph.sheet_instance_path.clone(),
                subgraph.bus_items.clone(),
                subgraph.driver_connection.clone(),
            )
        })
        .collect::<Vec<_>>();

    for subgraph in live_subgraphs.iter_mut() {
        for item_handle in &subgraph.wire_items {
            if !item_handle.borrow().is_bus_entry {
                continue;
            }
            let item = item_handle.borrow();

            #[cfg(test)]
            let attached_bus = bus_subgraphs
                .iter()
                .find(|(_, sheet_instance_path, bus_items, _)| {
                    *sheet_instance_path == subgraph.sheet_instance_path
                        && bus_items.iter().any(|bus_item| {
                            let bus_item = bus_item.borrow();
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
                .map(|(index, _, _, connection)| (*index, connection.clone()));

            drop(item);
            #[cfg(test)]
            {
                item_handle.borrow_mut().connected_bus_connection =
                    attached_bus.map(|(_, connection)| connection);
            }
        }
    }
}

fn reduced_project_bus_link_from_live(
    live_subgraphs: &[LiveReducedSubgraphHandle],
    link: &LiveReducedSubgraphLinkHandle,
) -> ReducedProjectBusNeighborLink {
    let link = link.borrow();
    ReducedProjectBusNeighborLink {
        member: live_bus_member_handle_snapshot(&link.member),
        subgraph_index: link
            .subgraph_handle
            .as_ref()
            .and_then(Weak::upgrade)
            .map(|subgraph| live_subgraph_projection_index(live_subgraphs, &subgraph))
            .unwrap_or_else(|| {
                #[cfg(test)]
                {
                    link.subgraph_index
                }

                #[cfg(not(test))]
                {
                    unreachable!("live bus link projection requires an attached subgraph handle")
                }
            }),
    }
}

fn live_subgraph_link_index(
    live_subgraphs: &[LiveReducedSubgraphHandle],
    link: &LiveReducedSubgraphLinkHandle,
) -> usize {
    let link = link.borrow();
    link.subgraph_handle
        .as_ref()
        .and_then(Weak::upgrade)
        .map(|subgraph| live_subgraph_projection_index(live_subgraphs, &subgraph))
        .unwrap_or_else(|| {
            #[cfg(test)]
            {
                link.subgraph_index
            }

            #[cfg(not(test))]
            {
                unreachable!("active live bus link lookup requires an attached subgraph handle")
            }
        })
}

// Upstream parity: active live graph traversal should follow the attached live subgraph topology,
// not copied reduced indexes. This helper is now handle-only on the active path; copied reduced
// target indexes remain only in the test build and at projection boundaries.
fn live_subgraph_handle_for_link(
    _live_subgraphs: &[LiveReducedSubgraphHandle],
    link: &LiveReducedSubgraphLinkHandle,
) -> Option<LiveReducedSubgraphHandle> {
    link.borrow()
        .subgraph_handle
        .as_ref()
        .and_then(Weak::upgrade)
}

// Upstream parity: active hierarchy traversal should follow the shared live parent handle. The
// non-test live payload now keeps that topology only on attached handles; reduced parent indexes
// are reconstructed only at the projection boundary.
fn live_subgraph_parent_handle(
    _live_subgraphs: &[LiveReducedSubgraphHandle],
    subgraph: &LiveReducedSubgraph,
) -> Option<LiveReducedSubgraphHandle> {
    subgraph.hier_parent_handle.as_ref().and_then(Weak::upgrade)
}

// Upstream parity: active hierarchy traversal now follows shared live child handles. The non-test
// live payload no longer needs copied reduced child indexes; reduced child indexes are only
// reconstructed when projecting the live graph back out.
fn live_subgraph_child_handles(
    _live_subgraphs: &[LiveReducedSubgraphHandle],
    subgraph: &LiveReducedSubgraph,
) -> Vec<LiveReducedSubgraphHandle> {
    subgraph
        .hier_child_handles
        .iter()
        .filter_map(Weak::upgrade)
        .collect()
}

fn live_subgraph_parent_handle_from_handle(
    handle: &LiveReducedSubgraphHandle,
) -> Option<LiveReducedSubgraphHandle> {
    handle
        .borrow()
        .hier_parent_handle
        .as_ref()
        .and_then(Weak::upgrade)
}

fn live_subgraph_child_handles_from_handle(
    handle: &LiveReducedSubgraphHandle,
) -> Vec<LiveReducedSubgraphHandle> {
    handle
        .borrow()
        .hier_child_handles
        .iter()
        .filter_map(Weak::upgrade)
        .collect()
}

fn live_subgraph_handle_from_wire_item(
    item: &LiveReducedSubgraphWireItemHandle,
) -> Option<LiveReducedSubgraphHandle> {
    item.borrow()
        .parent_subgraph_handle
        .as_ref()
        .and_then(Weak::upgrade)
}

// Upstream parity: active plain bus-parent traversal now follows shared live parent handles. The
// non-test live payload no longer needs copied reduced parent indexes; those are reconstructed
// only when projecting the live graph back into the reduced shared graph view.
fn live_subgraph_bus_parent_handles(
    _live_subgraphs: &[LiveReducedSubgraphHandle],
    subgraph: &LiveReducedSubgraph,
) -> Vec<LiveReducedSubgraphHandle> {
    subgraph
        .bus_parent_handles
        .iter()
        .filter_map(Weak::upgrade)
        .collect()
}

fn live_subgraph_bus_parent_handles_from_handle(
    handle: &LiveReducedSubgraphHandle,
) -> Vec<LiveReducedSubgraphHandle> {
    handle
        .borrow()
        .bus_parent_handles
        .iter()
        .filter_map(Weak::upgrade)
        .collect()
}

fn live_subgraph_bus_neighbor_links_from_handle(
    handle: &LiveReducedSubgraphHandle,
) -> Vec<LiveReducedSubgraphLinkHandle> {
    handle.borrow().bus_neighbor_links.clone()
}

fn live_subgraph_has_hierarchy_handles_from_handle(handle: &LiveReducedSubgraphHandle) -> bool {
    let subgraph = handle.borrow();
    subgraph
        .hier_parent_handle
        .as_ref()
        .and_then(Weak::upgrade)
        .is_some()
        || !subgraph.hier_child_handles.is_empty()
}

fn reduced_project_bus_parent_indexes_from_live_subgraph(
    live_subgraphs: &[LiveReducedSubgraphHandle],
    subgraph: &LiveReducedSubgraph,
) -> Vec<usize> {
    let mut parent_indexes = live_subgraph_bus_parent_handles(live_subgraphs, subgraph)
        .into_iter()
        .map(|handle| live_subgraph_projection_index(live_subgraphs, &handle))
        .collect::<Vec<_>>();
    parent_indexes.sort_unstable();
    parent_indexes.dedup();
    parent_indexes
}

fn reduced_project_hierarchy_indexes_from_live_subgraph(
    live_subgraphs: &[LiveReducedSubgraphHandle],
    subgraph: &LiveReducedSubgraph,
) -> (Option<usize>, Vec<usize>) {
    let parent_index = live_subgraph_parent_handle(live_subgraphs, subgraph)
        .map(|handle| live_subgraph_projection_index(live_subgraphs, &handle));
    let mut child_indexes = live_subgraph_child_handles(live_subgraphs, subgraph)
        .into_iter()
        .map(|handle| live_subgraph_projection_index(live_subgraphs, &handle))
        .collect::<Vec<_>>();
    child_indexes.sort_unstable();
    child_indexes.dedup();
    (parent_index, child_indexes)
}

fn collect_reduced_project_pin_driver_connections_from_live_handles(
    live_subgraphs: &[LiveReducedSubgraphHandle],
) -> (
    BTreeMap<ReducedNetBasePinKey, ReducedProjectConnection>,
    BTreeMap<ReducedProjectPinIdentityKey, ReducedProjectConnection>,
) {
    let mut by_pin = BTreeMap::new();
    let mut by_location = BTreeMap::new();

    for handle in live_subgraphs {
        let subgraph = handle.borrow();
        for base_pin in &subgraph.base_pins {
            let base_pin = base_pin.borrow();
            let connection = base_pin.connection.snapshot();
            if connection.connection_type == ReducedProjectConnectionType::None {
                continue;
            }

            by_pin.insert(base_pin.pin.key.clone(), connection.clone());
            by_location
                .entry(ReducedProjectPinIdentityKey {
                    sheet_instance_path: base_pin.pin.key.sheet_instance_path.clone(),
                    symbol_uuid: base_pin.pin.key.symbol_uuid.clone(),
                    at: base_pin.pin.key.at,
                    number: base_pin.pin.key.number.clone(),
                })
                .or_insert(connection);
        }
    }

    (by_pin, by_location)
}

// Upstream parity: reduced local analogue for the item-owned `SCH_CONNECTION::Clone()` refresh
// KiCad performs after a subgraph's chosen connection changes. This still uses reduced live
// wrappers instead of shared item pointers, but it keeps label/sheet-pin/hier-port connection
// owners synchronized with the current live subgraph driver before the final reduced projection
// and now also preserves KiCad's exercised bus/net mismatch skip plus the `item != m_driver`
// guard by reading the already-chosen live strong-driver handle from the subgraph owner instead of
// rediscovering it from names after attachment.
#[cfg(test)]
fn sync_live_reduced_item_connections_from_driver(subgraph: &mut LiveReducedSubgraph) {
    let driver_connection = subgraph.driver_connection.clone();
    let driver_connection_type = driver_connection.borrow().connection_type;
    let chosen_driver = subgraph.chosen_driver.clone();

    for link in &subgraph.label_links {
        if chosen_driver
            .as_ref()
            .zip(link.borrow().driver.as_ref())
            .is_some_and(|(chosen, owner)| Rc::ptr_eq(chosen, owner))
        {
            continue;
        }

        if reduced_connection_kind_mismatch(
            driver_connection_type,
            link.borrow().connection.borrow().connection_type,
        ) {
            continue;
        }

        link.borrow_mut().connection.clone_from(&driver_connection);
    }
    for pin in &subgraph.hier_sheet_pins {
        if chosen_driver
            .as_ref()
            .zip(pin.borrow().driver.as_ref())
            .is_some_and(|(chosen, owner)| Rc::ptr_eq(chosen, owner))
        {
            continue;
        }

        if reduced_connection_kind_mismatch(
            driver_connection_type,
            pin.borrow().connection.borrow().connection_type,
        ) {
            continue;
        }

        pin.borrow_mut().connection.clone_from(&driver_connection);
    }
    for port in &subgraph.hier_ports {
        if chosen_driver
            .as_ref()
            .zip(port.borrow().driver.as_ref())
            .is_some_and(|(chosen, owner)| Rc::ptr_eq(chosen, owner))
        {
            continue;
        }

        if reduced_connection_kind_mismatch(
            driver_connection_type,
            port.borrow().connection.borrow().connection_type,
        ) {
            continue;
        }

        port.borrow_mut().connection.clone_from(&driver_connection);
    }
}

// Upstream parity: reduced local bridge for pushing live graph-owned connection state back onto the
// reduced project graph query surface. This is still a projection step because the repo does not
// yet keep live item-owned connection pointers, but it now writes the live per-link connection
// owners instead of blasting the chosen driver connection onto every label/sheet-pin/hier-port.
#[cfg(test)]
fn apply_live_reduced_driver_connections(
    reduced_subgraphs: &mut [ReducedProjectSubgraphEntry],
    live_subgraphs: &[LiveReducedSubgraph],
) {
    for (index, live) in live_subgraphs.iter().enumerate() {
        let reduced = &mut reduced_subgraphs[index];
        let live_driver = live.driver_connection.snapshot();
        reduced.name = live_driver.name.clone();
        clone_reduced_connection_into_live_connection(
            &mut reduced.resolved_connection,
            &live_driver,
        );

        if let Some(driver_connection) = &mut reduced.driver_connection {
            clone_reduced_connection_into_live_connection(driver_connection, &live_driver);
        }

        for (target, source) in reduced.label_links.iter_mut().zip(live.label_links.iter()) {
            let source = source.borrow();
            clone_reduced_connection_into_live_connection(
                &mut target.connection,
                &source.connection.snapshot(),
            );
        }

        for (target, source) in reduced
            .hier_sheet_pins
            .iter_mut()
            .zip(live.hier_sheet_pins.iter())
        {
            let source = source.borrow();
            clone_reduced_connection_into_live_connection(
                &mut target.connection,
                &source.connection.snapshot(),
            );
        }

        for (target, source) in reduced.hier_ports.iter_mut().zip(live.hier_ports.iter()) {
            let source = source.borrow();
            clone_reduced_connection_into_live_connection(
                &mut target.connection,
                &source.connection.snapshot(),
            );
        }

        reduced.bus_neighbor_links = live
            .bus_neighbor_links
            .iter()
            .map(|link| reduced_project_bus_link_from_live(&[], link))
            .collect();
        reduced.bus_parent_links = live
            .bus_parent_links
            .iter()
            .map(|link| reduced_project_bus_link_from_live(&[], link))
            .collect();
        reduced.bus_parent_indexes = live.bus_parent_indexes.clone();
        for (target, source) in reduced.wire_items.iter_mut().zip(live.wire_items.iter()) {
            let source = source.borrow();
            target.connected_bus_subgraph_index = source
                .connected_bus_item_handle
                .as_ref()
                .and_then(Weak::upgrade)
                .and_then(|bus| live_subgraph_handle_from_wire_item(&bus))
                .map(|bus| bus.borrow().source_index);
        }
    }
}

// Upstream parity: local bridge for projecting the active shared live subgraph owner back onto
// the reduced graph query surface. This still ends in a reduced projection because consumers do
// not yet keep live item/subgraph pointers, but the active recursive graph build now mutates one
// shared live subgraph object graph before that projection. Remaining divergence is that callers
// still consume reduced indices instead of live item/subgraph pointers, so this projection must
// collapse bus-entry attachment back to source indexes at the edge. Active live bus-entry items no
// longer carry a copied reduced bus index alongside that live owner, and those wire-item owners
// are now shared handles on the live graph instead of copied value wrappers.
fn apply_live_reduced_driver_connections_from_handles(
    reduced_subgraphs: &mut [ReducedProjectSubgraphEntry],
    live_subgraphs: &[LiveReducedSubgraphHandle],
) {
    for (index, handle) in live_subgraphs.iter().enumerate() {
        let live = handle.borrow();
        let reduced = &mut reduced_subgraphs[index];
        let live_driver = live.driver_connection.borrow();
        reduced.name = live_driver.name.clone();
        clone_live_connection_owner_into_reduced_connection(
            &mut reduced.resolved_connection,
            &live_driver,
        );

        if let Some(driver_connection) = &mut reduced.driver_connection {
            clone_live_connection_owner_into_reduced_connection(driver_connection, &live_driver);
        }

        for (target, source) in reduced.label_links.iter_mut().zip(live.label_links.iter()) {
            let source = source.borrow();
            let source_connection = source.connection.borrow();
            clone_live_connection_owner_into_reduced_connection(
                &mut target.connection,
                &source_connection,
            );
        }

        for (target, source) in reduced
            .hier_sheet_pins
            .iter_mut()
            .zip(live.hier_sheet_pins.iter())
        {
            let source = source.borrow();
            let source_connection = source.connection.borrow();
            clone_live_connection_owner_into_reduced_connection(
                &mut target.connection,
                &source_connection,
            );
        }

        for (target, source) in reduced.hier_ports.iter_mut().zip(live.hier_ports.iter()) {
            let source = source.borrow();
            let source_connection = source.connection.borrow();
            clone_live_connection_owner_into_reduced_connection(
                &mut target.connection,
                &source_connection,
            );
        }

        reduced.bus_neighbor_links = live
            .bus_neighbor_links
            .iter()
            .map(|link| reduced_project_bus_link_from_live(live_subgraphs, link))
            .collect();
        reduced.bus_parent_links = live
            .bus_parent_links
            .iter()
            .map(|link| reduced_project_bus_link_from_live(live_subgraphs, link))
            .collect();
        let (hier_parent_index, hier_child_indexes) =
            reduced_project_hierarchy_indexes_from_live_subgraph(live_subgraphs, &live);
        reduced.hier_parent_index = hier_parent_index;
        reduced.hier_child_indexes = hier_child_indexes;
        reduced.bus_parent_indexes =
            reduced_project_bus_parent_indexes_from_live_subgraph(live_subgraphs, &live);
        for (target, source) in reduced.wire_items.iter_mut().zip(live.wire_items.iter()) {
            let source = source.borrow();
            target.connected_bus_subgraph_index = source
                .connected_bus_item_handle
                .as_ref()
                .and_then(Weak::upgrade)
                .and_then(|bus| live_subgraph_handle_from_wire_item(&bus))
                .map(|bus| live_subgraph_projection_index(live_subgraphs, &bus));
        }
    }
}

#[cfg(test)]
fn build_live_reduced_name_caches(
    live_subgraphs: &[LiveReducedSubgraph],
) -> (
    BTreeMap<String, Vec<usize>>,
    BTreeMap<(String, String), Vec<usize>>,
) {
    let mut subgraphs_by_name = BTreeMap::<String, Vec<usize>>::new();
    let mut subgraphs_by_sheet_and_name = BTreeMap::<(String, String), Vec<usize>>::new();

    for (index, subgraph) in live_subgraphs.iter().enumerate() {
        let name = subgraph.driver_connection.name();
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

// Upstream parity: local bridge for live graph name-cache ownership on shared subgraph handles.
// This still caches reduced connection names instead of full `CONNECTION_SUBGRAPH` objects, but
// it keeps same-name recache attached to the shared live subgraph graph build path.
#[cfg(test)]
fn build_live_reduced_name_caches_from_handles(
    live_subgraphs: &[LiveReducedSubgraphHandle],
) -> (
    BTreeMap<String, Vec<usize>>,
    BTreeMap<(String, String), Vec<usize>>,
) {
    let mut subgraphs_by_name = BTreeMap::<String, Vec<usize>>::new();
    let mut subgraphs_by_sheet_and_name = BTreeMap::<(String, String), Vec<usize>>::new();

    for (index, handle) in live_subgraphs.iter().enumerate() {
        let subgraph = handle.borrow();
        let name = subgraph.driver_connection.name();
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

fn build_live_reduced_name_handle_caches_from_handles(
    live_subgraphs: &[LiveReducedSubgraphHandle],
) -> (
    BTreeMap<String, Vec<LiveReducedSubgraphHandle>>,
    BTreeMap<(String, String), Vec<LiveReducedSubgraphHandle>>,
) {
    let mut subgraphs_by_name = BTreeMap::<String, Vec<LiveReducedSubgraphHandle>>::new();
    let mut subgraphs_by_sheet_and_name =
        BTreeMap::<(String, String), Vec<LiveReducedSubgraphHandle>>::new();

    for handle in live_subgraphs {
        let subgraph = handle.borrow();
        let name = subgraph.driver_connection.name();
        subgraphs_by_name
            .entry(name.clone())
            .or_default()
            .push(handle.clone());

        if name.contains('[') {
            let prefix_only = format!("{}[]", name.split('[').next().unwrap_or(""));
            subgraphs_by_name
                .entry(prefix_only)
                .or_default()
                .push(handle.clone());
        }

        subgraphs_by_sheet_and_name
            .entry((subgraph.sheet_instance_path.clone(), name))
            .or_default()
            .push(handle.clone());
    }

    (subgraphs_by_name, subgraphs_by_sheet_and_name)
}

#[cfg(test)]
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

    let new_name = live_subgraphs[subgraph_index].driver_connection.name();
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

// Upstream parity: local bridge for same-name recache on the shared live subgraph owner. This
// still keys by reduced resolved names instead of full live `CONNECTION_SUBGRAPH` identity, but it
// keeps recache/update tied to the shared active graph object graph.
#[cfg(test)]
fn recache_live_reduced_subgraph_name_from_handles(
    live_subgraphs: &[LiveReducedSubgraphHandle],
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

    let sheet_path = live_subgraphs[subgraph_index]
        .borrow()
        .sheet_instance_path
        .clone();
    let sheet_key = (sheet_path, old_name.to_string());
    if let Some(indexes) = subgraphs_by_sheet_and_name.get_mut(&sheet_key) {
        indexes.retain(|index| *index != subgraph_index);
    }

    let subgraph = live_subgraphs[subgraph_index].borrow();
    let new_name = subgraph.driver_connection.name();
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
        .entry((subgraph.sheet_instance_path.clone(), new_name))
        .or_default()
        .push(subgraph_index);
}

fn recache_live_reduced_subgraph_name_handle_cache_from_handles(
    subgraphs_by_name: &mut BTreeMap<String, Vec<LiveReducedSubgraphHandle>>,
    subgraphs_by_sheet_and_name: &mut BTreeMap<(String, String), Vec<LiveReducedSubgraphHandle>>,
    subgraph_handle: &LiveReducedSubgraphHandle,
    old_name: &str,
) {
    if let Some(handles) = subgraphs_by_name.get_mut(old_name) {
        handles.retain(|handle| !Rc::ptr_eq(handle, subgraph_handle));
    }

    if old_name.contains('[') {
        let old_prefix_only = format!("{}[]", old_name.split('[').next().unwrap_or(""));
        if let Some(handles) = subgraphs_by_name.get_mut(&old_prefix_only) {
            handles.retain(|handle| !Rc::ptr_eq(handle, subgraph_handle));
        }
    }

    let subgraph = subgraph_handle.borrow();
    let sheet_key = (subgraph.sheet_instance_path.clone(), old_name.to_string());
    if let Some(handles) = subgraphs_by_sheet_and_name.get_mut(&sheet_key) {
        handles.retain(|handle| !Rc::ptr_eq(handle, subgraph_handle));
    }

    let new_name = subgraph.driver_connection.name();
    subgraphs_by_name
        .entry(new_name.clone())
        .or_default()
        .push(subgraph_handle.clone());

    if new_name.contains('[') {
        let new_prefix_only = format!("{}[]", new_name.split('[').next().unwrap_or(""));
        subgraphs_by_name
            .entry(new_prefix_only)
            .or_default()
            .push(subgraph_handle.clone());
    }

    subgraphs_by_sheet_and_name
        .entry((subgraph.sheet_instance_path.clone(), new_name))
        .or_default()
        .push(subgraph_handle.clone());
}

#[cfg(test)]
fn replay_reduced_live_stale_bus_members_on_live_subgraphs(
    live_subgraphs: &mut [LiveReducedSubgraph],
    stale_members: &[ReducedBusMember],
) {
    for stale_member in stale_members {
        for subgraph in live_subgraphs.iter_mut() {
            let is_bus = {
                let connection = subgraph.driver_connection.borrow();
                matches!(
                    connection.connection_type,
                    ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
                )
            };
            if !is_bus {
                continue;
            }

            let mut connection = subgraph.driver_connection.borrow_mut();
            if let Some(member) = match_live_bus_member_mut(&mut connection.members, stale_member) {
                let old_member = member.borrow().clone();
                clone_reduced_connection_into_live_bus_member(
                    &mut member.borrow_mut(),
                    &reduced_connection_from_bus_member(
                        stale_member,
                        &subgraph.sheet_instance_path,
                    ),
                );
                if *member.borrow() != old_member {
                    subgraph.dirty = true;
                }
            }
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
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

#[allow(dead_code)]
fn reduced_connection_from_live_bus_member(
    member: &LiveProjectBusMember,
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
        members: live_bus_member_handles_to_snapshots(&member.members),
    }
}

// Upstream parity: reduced local analogue for the `matchBusMember()`-driven member refresh KiCad
// performs after parent-bus propagation. This is not a 1:1 live graph update because the Rust tree
// still stores static reduced link snapshots instead of mutating live `SCH_CONNECTION` objects, but
// it now remaps stored bus parent/neighbor link members onto the parent's current reduced member
// tree so later consumers do not keep stale pre-remap member names forever. Remaining divergence is
// the still-missing in-place connection clone/recache cycle on the subgraphs themselves.
#[cfg(test)]
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
#[cfg(test)]
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
#[cfg(test)]
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
                link.member.clone().into()
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
// Upstream parity: reduced local analogue for the hierarchy-chain portion of
// `CONNECTION_GRAPH::propagateToNeighbors()`. This is no longer a purely static settle pass: it
// now walks a dedicated live reduced subgraph/connection layer with dirty-state and in-place
// clone semantics before projecting the chosen driver connection back onto the shared reduced
// graph. Remaining divergence is that the current live layer still covers only the hierarchy-chain
// slice and does not yet include KiCad's bus-neighbor recursion, stale bus-member replay, or live
// item-owned connection updates on the same objects.
#[cfg(test)]
fn propagate_reduced_live_hierarchy_chain(
    start: usize,
    live_subgraphs: &mut [LiveReducedSubgraph],
    force: bool,
) {
    if !force
        && live_subgraph_has_hier_ports(&live_subgraphs[start])
        && live_subgraph_has_hier_pins(&live_subgraphs[start])
    {
        return;
    } else if !live_subgraph_has_hier_ports(&live_subgraphs[start])
        && !live_subgraph_has_hier_pins(&live_subgraphs[start])
    {
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
    let mut highest = live_reduced_subgraph_driver_priority(&live_subgraphs[best_index]);
    let mut best_is_strong = highest >= 3;
    let mut best_name = live_subgraphs[best_index]
        .driver_connection
        .name()
        .to_string();

    if highest < 6 {
        for &index in visited.iter().filter(|index| **index != start) {
            let priority = live_reduced_subgraph_driver_priority(&live_subgraphs[index]);
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
                    && (candidate_name < best_name))
            {
                best_index = index;
                highest = priority;
                best_is_strong = candidate_strong;
                best_name = candidate_name;
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

#[cfg(test)]
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

#[cfg(test)]
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
#[cfg(test)]
fn refresh_reduced_live_bus_neighbor_drivers_on_live_subgraphs(
    live_subgraphs: &mut [LiveReducedSubgraph],
    stale_members: &mut Vec<ReducedBusMember>,
) {
    for parent_index in 0..live_subgraphs.len() {
        if !live_subgraphs[parent_index].dirty {
            continue;
        }
        let is_bus = {
            let parent_connection = live_subgraphs[parent_index].driver_connection.borrow();
            matches!(
                parent_connection.connection_type,
                ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
            )
        };
        if !is_bus {
            continue;
        }

        let mut sorted_links = live_subgraphs[parent_index].bus_neighbor_links.clone();
        sorted_links.sort_by(live_subgraph_link_handle_cmp);

        for link_index in 0..sorted_links.len() {
            let link = sorted_links[link_index].clone();
            let link_member = link.borrow().member.clone();
            let neighbor_index = link.borrow().subgraph_index;
            let parent_member = {
                let parent_connection = live_subgraphs[parent_index].driver_connection.borrow();
                match_live_bus_member_live(&parent_connection.members, &link_member.borrow())
            };
            let Some(parent_member) = parent_member else {
                continue;
            };

            let neighbor_connection = live_subgraphs[neighbor_index].driver_connection.snapshot();
            let neighbor_name = neighbor_connection.name.clone();

            if neighbor_name == parent_member.borrow().full_local_name {
                continue;
            }

            let parent_sheet_instance_path =
                live_subgraphs[parent_index].sheet_instance_path.clone();
            let neighbor_sheet_instance_path =
                live_subgraphs[neighbor_index].sheet_instance_path.clone();
            let neighbor_connection_sheet = neighbor_connection.sheet_instance_path.clone();

            if neighbor_connection_sheet != neighbor_sheet_instance_path {
                if neighbor_connection_sheet != parent_sheet_instance_path {
                    continue;
                }

                let search = ReducedBusMember {
                    net_code: 0,
                    name: neighbor_connection.local_name.clone(),
                    local_name: neighbor_connection.local_name.clone(),
                    full_local_name: neighbor_connection.full_local_name.clone(),
                    vector_index: None,
                    kind: match neighbor_connection.connection_type {
                        ReducedProjectConnectionType::Bus
                        | ReducedProjectConnectionType::BusGroup => ReducedBusMemberKind::Bus,
                        _ => ReducedBusMemberKind::Net,
                    },
                    members: neighbor_connection.members.clone(),
                };

                let parent_has_search = {
                    let parent_connection = live_subgraphs[parent_index].driver_connection.borrow();
                    match_live_bus_member(&parent_connection.members, &search).is_some()
                };
                if parent_has_search {
                    continue;
                }
            }

            if live_reduced_subgraph_driver_priority(&live_subgraphs[neighbor_index]) >= 6 {
                let promoted = neighbor_connection;
                let old_member = link_member.clone();
                let mut parent_connection_mut =
                    live_subgraphs[parent_index].driver_connection.borrow_mut();
                if let Some(member) = match_live_bus_member_mut_live(
                    &mut parent_connection_mut.members,
                    &link_member.borrow(),
                ) {
                    clone_reduced_connection_into_live_bus_member(
                        &mut member.borrow_mut(),
                        &promoted,
                    );
                    let refreshed_member = member.clone();
                    let refreshed_member_snapshot =
                        live_bus_member_handle_snapshot(&refreshed_member);

                    for candidate_link in &mut sorted_links {
                        let mut candidate_link = candidate_link.borrow_mut();
                        if live_bus_member_handles_eq(&candidate_link.member, &old_member) {
                            candidate_link.member = refreshed_member.clone();
                        }
                    }

                    for candidate_link in &mut live_subgraphs[parent_index].bus_neighbor_links {
                        let mut candidate_link = candidate_link.borrow_mut();
                        if live_bus_member_handles_eq(&candidate_link.member, &old_member) {
                            candidate_link.member = refreshed_member.clone();
                        }
                    }

                    for candidate_link in &mut live_subgraphs[parent_index].bus_parent_links {
                        let mut candidate_link = candidate_link.borrow_mut();
                        if live_bus_member_handles_eq(&candidate_link.member, &old_member) {
                            candidate_link.member = refreshed_member.clone();
                        }
                    }

                    if !stale_members.contains(&refreshed_member_snapshot) {
                        stale_members.push(refreshed_member_snapshot);
                    }

                    live_subgraphs[parent_index].dirty = true;
                }
                continue;
            }

            clone_live_bus_member_into_live_connection_owner(
                &mut live_subgraphs[neighbor_index]
                    .driver_connection
                    .borrow_mut(),
                &parent_member.borrow(),
                &live_subgraphs[neighbor_index].sheet_instance_path,
            );
            sync_live_reduced_item_connections_from_driver(&mut live_subgraphs[neighbor_index]);
            live_subgraphs[neighbor_index].dirty = true;
        }
    }
}

#[cfg(test)]
fn refresh_reduced_live_bus_neighbor_drivers(
    reduced_subgraphs: &mut [ReducedProjectSubgraphEntry],
) {
    let mut live_subgraphs = build_live_reduced_subgraphs(reduced_subgraphs);
    let mut stale_members = Vec::new();
    refresh_reduced_live_bus_neighbor_drivers_on_live_subgraphs(
        &mut live_subgraphs,
        &mut stale_members,
    );
    replay_reduced_live_stale_bus_members_on_live_subgraphs(&mut live_subgraphs, &stale_members);
    apply_live_reduced_driver_connections(reduced_subgraphs, &live_subgraphs);
}

// Upstream parity: reduced local analogue for the stale-member update KiCad performs after a bus
// neighbor or hierarchy child settles on a final net connection. This still stops short of the
// full live `stale_bus_members` replay because it does not recursively revisit every affected bus
// subgraph on the same object graph, but it does move the direct child-net -> parent-bus member
// mutation onto the shared live subgraph owner before the reduced cleanup passes.
#[cfg(test)]
fn refresh_reduced_live_bus_parent_members_on_live_subgraphs(
    live_subgraphs: &mut [LiveReducedSubgraph],
) {
    for child_index in 0..live_subgraphs.len() {
        if !live_subgraphs[child_index].dirty {
            continue;
        }
        let child_connection = live_subgraphs[child_index].driver_connection.snapshot();

        if child_connection.connection_type != ReducedProjectConnectionType::Net {
            continue;
        }

        let child_sheet_instance_path = live_subgraphs[child_index].sheet_instance_path.clone();
        let parent_links = live_subgraphs[child_index].bus_parent_links.clone();

        for link in parent_links {
            let parent_index = link.borrow().subgraph_index;
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

            if match_live_bus_member(
                &live_subgraphs[parent_index]
                    .driver_connection
                    .borrow()
                    .members,
                &search,
            )
            .is_some()
            {
                continue;
            }

            let mut parent_connection = live_subgraphs[parent_index].driver_connection.borrow_mut();
            let link_member = live_bus_member_handle_snapshot(&link.borrow().member);
            if let Some(member) =
                match_live_bus_member_mut(&mut parent_connection.members, &link_member)
            {
                let old_member = member.borrow().clone();
                clone_reduced_connection_into_live_bus_member(
                    &mut member.borrow_mut(),
                    &child_connection,
                );
                if *member.borrow() != old_member {
                    live_subgraphs[parent_index].dirty = true;
                }
            }
        }
    }
}

#[cfg(test)]
fn refresh_reduced_live_bus_parent_members(reduced_subgraphs: &mut [ReducedProjectSubgraphEntry]) {
    let mut live_subgraphs = build_live_reduced_subgraphs(reduced_subgraphs);
    refresh_reduced_live_bus_parent_members_on_live_subgraphs(&mut live_subgraphs);
    apply_live_reduced_driver_connections(reduced_subgraphs, &live_subgraphs);
}

// Upstream parity: reduced local analogue for the multiple-parent rename/recache branch KiCad
// runs before the final graph caches are rebuilt. This still projects back onto the reduced graph
// instead of mutating live name indexes in place, but it moves the parent-member clone and
// same-name subgraph rename onto the shared live subgraph owner before the reduced cache rebuild.
#[cfg(test)]
fn refresh_reduced_live_multiple_bus_parent_names_on_live_subgraphs(
    live_subgraphs: &mut [LiveReducedSubgraph],
) {
    let (mut subgraphs_by_name, mut subgraphs_by_sheet_and_name) =
        build_live_reduced_name_caches(&live_subgraphs);

    for subgraph_index in 0..live_subgraphs.len() {
        if live_subgraphs[subgraph_index].bus_parent_links.len() < 2 {
            continue;
        }

        let connection = live_subgraphs[subgraph_index].driver_connection.snapshot();

        if connection.connection_type != ReducedProjectConnectionType::Net {
            continue;
        }

        let parent_links = live_subgraphs[subgraph_index].bus_parent_links.clone();

        for link in parent_links {
            let parent_index = link.borrow().subgraph_index;
            let link_member = live_bus_member_handle_snapshot(&link.borrow().member);
            let old_name = {
                let mut parent_connection =
                    live_subgraphs[parent_index].driver_connection.borrow_mut();
                let Some(member) =
                    match_live_bus_member_mut(&mut parent_connection.members, &link_member)
                else {
                    continue;
                };

                if member.borrow().full_local_name == connection.full_local_name {
                    continue;
                }

                let old_name = member.borrow().full_local_name.clone();
                clone_reduced_connection_into_live_bus_member(
                    &mut member.borrow_mut(),
                    &connection,
                );
                old_name
            };

            let candidate_indexes = subgraphs_by_name
                .get(&old_name)
                .cloned()
                .unwrap_or_default();

            for candidate_index in candidate_indexes {
                let old_candidate_name = live_subgraphs[candidate_index].driver_connection.name();
                if old_candidate_name == old_name {
                    *live_subgraphs[candidate_index]
                        .driver_connection
                        .borrow_mut() = connection.clone().into();
                    sync_live_reduced_item_connections_from_driver(
                        &mut live_subgraphs[candidate_index],
                    );
                    live_subgraphs[candidate_index].dirty = true;
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

#[cfg(test)]
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
#[cfg(test)]
fn refresh_reduced_live_bus_link_members_on_live_subgraphs(
    live_subgraphs: &mut [LiveReducedSubgraph],
) {
    let mut refreshed_parent_links =
        vec![Vec::<LiveReducedSubgraphLinkHandle>::new(); live_subgraphs.len()];

    for child_index in 0..live_subgraphs.len() {
        let child_connection = live_subgraphs[child_index].driver_connection.snapshot();
        let existing_parent_links = live_subgraphs[child_index].bus_parent_links.clone();

        for &parent_index in &live_subgraphs[child_index].bus_parent_indexes {
            let search = existing_parent_links
                .iter()
                .find(|link| link.borrow().subgraph_index == parent_index)
                .map(|link| link.borrow().member.borrow().clone())
                .unwrap_or_else(|| {
                    LiveProjectBusMember::from(ReducedBusMember {
                        net_code: child_connection.net_code,
                        name: child_connection.local_name.clone(),
                        local_name: child_connection.local_name.clone(),
                        full_local_name: child_connection.full_local_name.clone(),
                        vector_index: None,
                        kind: match child_connection.connection_type {
                            ReducedProjectConnectionType::Bus
                            | ReducedProjectConnectionType::BusGroup => ReducedBusMemberKind::Bus,
                            _ => ReducedBusMemberKind::Net,
                        },
                        members: child_connection.members.clone(),
                    })
                });

            let Some(refreshed_member) = match_live_bus_member(
                &live_subgraphs[parent_index]
                    .driver_connection
                    .borrow()
                    .members,
                &search.snapshot(),
            ) else {
                continue;
            };

            refreshed_parent_links[child_index].push(Rc::new(RefCell::new(
                LiveReducedSubgraphLink {
                    member: refreshed_member,
                    subgraph_index: parent_index,
                    subgraph_handle: None,
                },
            )));
        }
    }

    let mut refreshed_neighbor_links =
        vec![Vec::<LiveReducedSubgraphLinkHandle>::new(); live_subgraphs.len()];

    for (child_index, links) in refreshed_parent_links.iter().enumerate() {
        for link in links {
            let neighbor_index = link.borrow().subgraph_index;
            refreshed_neighbor_links[neighbor_index].push(Rc::new(RefCell::new(
                LiveReducedSubgraphLink {
                    member: link.borrow().member.clone(),
                    subgraph_index: child_index,
                    subgraph_handle: None,
                },
            )));
        }
    }

    for (index, live) in live_subgraphs.iter_mut().enumerate() {
        live.bus_parent_links = refreshed_parent_links[index].clone();
        sort_dedup_live_subgraph_link_handles(&mut live.bus_parent_links);
        live.bus_neighbor_links = refreshed_neighbor_links[index].clone();
        sort_dedup_live_subgraph_link_handles(&mut live.bus_neighbor_links);
    }
}

#[cfg(test)]
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
// Upstream parity: local bridge for the hierarchy-chain portion of live graph propagation on the
// shared live subgraph owner. This still mutates reduced live carriers instead of full local
// `CONNECTION_SUBGRAPH` objects, but the active recursive graph build now walks shared subgraph
// handles and uses handle identity plus narrow live handle reads instead of cloning whole live
// subgraph wrappers for traversal.
fn propagate_reduced_live_hierarchy_chain_on_handles(
    start: &LiveReducedSubgraphHandle,
    _live_subgraphs: &[LiveReducedSubgraphHandle],
    force: bool,
) {
    let start_has_hier_ports = !start.borrow().hier_ports.is_empty();
    let start_has_hier_pins = !start.borrow().hier_sheet_pins.is_empty();
    if !force && start_has_hier_ports && start_has_hier_pins {
        return;
    } else if !start_has_hier_ports && !start_has_hier_pins {
        start.borrow_mut().dirty = false;
        return;
    }

    let mut stack = vec![start.clone()];
    let mut visited = Vec::<LiveReducedSubgraphHandle>::new();
    let mut visited_set = BTreeSet::<usize>::new();

    visited_set.insert(live_subgraph_handle_id(start));

    while let Some(handle) = stack.pop() {
        visited.push(handle.clone());
        if let Some(parent_handle) = live_subgraph_parent_handle_from_handle(&handle) {
            if visited_set.insert(live_subgraph_handle_id(&parent_handle)) {
                stack.push(parent_handle);
            }
        }

        for child_handle in live_subgraph_child_handles_from_handle(&handle) {
            if visited_set.insert(live_subgraph_handle_id(&child_handle)) {
                stack.push(child_handle);
            }
        }
    }

    let mut best_handle = start.clone();
    let mut highest = live_reduced_subgraph_driver_priority(&start.borrow());
    let mut best_is_strong = highest >= 3;
    let mut best_name = start.borrow().driver_connection.name().to_string();

    if highest < 6 {
        for handle in visited.iter().filter(|handle| !Rc::ptr_eq(handle, start)) {
            let priority = live_reduced_subgraph_driver_priority(&handle.borrow());
            let candidate_strong = priority >= 3;
            let candidate_name = handle.borrow().driver_connection.name();
            let candidate_depth = reduced_sheet_path_depth(&handle.borrow().sheet_instance_path);
            let best_depth = reduced_sheet_path_depth(&best_handle.borrow().sheet_instance_path);
            let shorter_path = candidate_depth < best_depth;
            let as_good_path = candidate_depth <= best_depth;

            if (priority >= 6)
                || (!best_is_strong && candidate_strong)
                || (priority > highest && candidate_strong)
                || (priority == highest && candidate_strong && shorter_path)
                || ((best_is_strong == candidate_strong)
                    && as_good_path
                    && (priority == highest)
                    && (candidate_name < best_name))
            {
                best_handle = handle.clone();
                highest = priority;
                best_is_strong = candidate_strong;
                best_name = candidate_name;
            }
        }
    }

    let chosen_connection = best_handle.borrow().driver_connection.clone();

    for handle in visited {
        let mut subgraph = handle.borrow_mut();
        let changed =
            !live_connection_handle_clone_eq(&subgraph.driver_connection, &chosen_connection);
        subgraph.driver_connection.clone_from(&chosen_connection);
        subgraph.dirty = changed;
    }
}

// Upstream parity: local bridge for the connected live propagation slice on the shared subgraph
// owner. Active component discovery now follows live handle identity across hierarchy and bus
// links instead of using reduced subgraph indexes as traversal identity.
fn collect_live_reduced_propagation_component_handles_from_handles(
    start: &LiveReducedSubgraphHandle,
    live_subgraphs: &[LiveReducedSubgraphHandle],
) -> Vec<LiveReducedSubgraphHandle> {
    let mut queue = VecDeque::from([start.clone()]);
    let mut visited = BTreeSet::from([live_subgraph_handle_id(start)]);
    let mut component = Vec::new();

    while let Some(handle) = queue.pop_front() {
        component.push(handle.clone());
        if let Some(parent_handle) = live_subgraph_parent_handle_from_handle(&handle) {
            if visited.insert(live_subgraph_handle_id(&parent_handle)) {
                queue.push_back(parent_handle);
            }
        }

        for child_handle in live_subgraph_child_handles_from_handle(&handle) {
            if visited.insert(live_subgraph_handle_id(&child_handle)) {
                queue.push_back(child_handle);
            }
        }

        for parent_handle in live_subgraph_bus_parent_handles_from_handle(&handle) {
            if visited.insert(live_subgraph_handle_id(&parent_handle)) {
                queue.push_back(parent_handle);
            }
        }

        for link in live_subgraph_bus_neighbor_links_from_handle(&handle) {
            let Some(neighbor_handle) = live_subgraph_handle_for_link(live_subgraphs, &link) else {
                continue;
            };
            if visited.insert(live_subgraph_handle_id(&neighbor_handle)) {
                queue.push_back(neighbor_handle);
            }
        }
    }

    component.sort_by_key(live_subgraph_handle_id);
    component
}

#[cfg(test)]
fn collect_live_reduced_propagation_component_from_handles(
    start: usize,
    live_subgraphs: &[LiveReducedSubgraphHandle],
) -> Vec<usize> {
    collect_live_reduced_propagation_component_handles_from_handles(
        &live_subgraphs[start],
        live_subgraphs,
    )
    .into_iter()
    .map(|handle| handle.borrow().source_index)
    .collect()
}

// Upstream parity: local bridge for the global-secondary-driver promotion branch on the shared
// live subgraph owner. The active recursion now promotes the shared live connection owner itself
// instead of snapshotting the chosen connection through reduced carriers, while still revisiting
// shared subgraph handles by handle identity and narrow live-owner reads instead of cloning whole
// live subgraph wrappers. Active equality checks now also compare clone-equivalent live connection
// owners directly instead of snapshotting reduced connections.
fn refresh_reduced_live_global_secondary_driver_promotions_for_handle(
    start: &LiveReducedSubgraphHandle,
    live_subgraphs: &[LiveReducedSubgraphHandle],
) -> Vec<LiveReducedSubgraphHandle> {
    if live_subgraph_has_local_driver(&start.borrow())
        || live_subgraph_strong_driver_count(&start.borrow()) < 2
    {
        return Vec::new();
    }

    let chosen_connection = start.borrow().driver_connection.clone();
    let start_sheet = start.borrow().sheet_instance_path.clone();
    let secondary_drivers = start.borrow().drivers.clone();
    let mut promoted = Vec::new();

    for secondary_driver in secondary_drivers {
        if live_project_strong_driver_full_name(&secondary_driver.borrow())
            == chosen_connection.name()
        {
            continue;
        }

        let secondary_is_global =
            live_project_strong_driver_priority(&secondary_driver.borrow()) >= 6;

        for handle in live_subgraphs.iter() {
            if Rc::ptr_eq(handle, start) {
                continue;
            }

            if !secondary_is_global && handle.borrow().sheet_instance_path != start_sheet {
                continue;
            }

            if !handle.borrow().drivers.iter().any(|candidate_driver| {
                live_project_strong_driver_full_name(&candidate_driver.borrow())
                    == live_project_strong_driver_full_name(&secondary_driver.borrow())
            }) {
                continue;
            }

            let same_connection = {
                let handle_ref = handle.borrow();
                live_connection_handle_clone_eq(&handle_ref.driver_connection, &chosen_connection)
            };
            if same_connection {
                continue;
            }

            handle
                .borrow()
                .driver_connection
                .clone_from(&chosen_connection);
            sync_live_reduced_item_connections_from_driver_handle(handle);
            handle.borrow_mut().dirty = true;
            promoted.push(handle.clone());
        }
    }

    promoted.sort_by_key(live_subgraph_handle_id);
    promoted.dedup_by(|left, right| Rc::ptr_eq(left, right));
    promoted
}

fn propagate_reduced_hierarchy_driver_chains_on_live_subgraph_handles_for_component(
    live_subgraphs: &[LiveReducedSubgraphHandle],
    component: &[LiveReducedSubgraphHandle],
    force: bool,
) {
    // Upstream parity: local live-handle analogue for the hierarchy-chain propagation branch
    // inside `propagateToNeighbors()`. This now consumes the recursive walk's explicit dirty
    // subset instead of re-reading dirty state inside the helper, and active dirty checks now
    // compare clone-equivalent live connection owners directly instead of snapshotting before and
    // after mutation. Remaining divergence is upstream's fuller `CONNECTION_SUBGRAPH` object graph
    // and item-pointer topology.
    for handle in component {
        let has_hierarchy_links = live_subgraph_has_hierarchy_handles_from_handle(handle);

        if !has_hierarchy_links {
            continue;
        }

        propagate_reduced_live_hierarchy_chain_on_handles(handle, live_subgraphs, force);
    }
}

fn refresh_reduced_live_bus_neighbor_drivers_on_handles_for_component(
    live_subgraphs: &[LiveReducedSubgraphHandle],
    component: &[LiveReducedSubgraphHandle],
    stale_members: &mut Vec<LiveProjectBusMemberHandle>,
) {
    // Upstream parity: local live-handle analogue for the bus-neighbor driver/member propagation
    // KiCad runs through connected `CONNECTION_SUBGRAPH` neighbors. This still keeps reduced
    // member snapshots, but active traversal now consumes the recursive walk's explicit dirty
    // subset and matches/mutates through attached live parent/neighbor handles plus narrow
    // live-owner reads instead of reduced subgraph indexes or whole live subgraph clones.
    for parent_handle in component {
        let is_bus = matches!(
            parent_handle
                .borrow()
                .driver_connection
                .borrow()
                .connection_type,
            ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
        );
        if !is_bus {
            continue;
        }

        let mut sorted_links = live_subgraph_bus_neighbor_links_from_handle(parent_handle);
        sorted_links.sort_by(|left, right| {
            left.borrow()
                .member
                .borrow()
                .name
                .cmp(&right.borrow().member.borrow().name)
                .then(
                    live_subgraph_link_index(live_subgraphs, left)
                        .cmp(&live_subgraph_link_index(live_subgraphs, right)),
                )
        });

        for link in sorted_links.clone() {
            let Some(neighbor_handle) = live_subgraph_handle_for_link(live_subgraphs, &link) else {
                continue;
            };
            let current_link_member = {
                let parent = parent_handle.borrow();
                parent
                    .bus_neighbor_links
                    .iter()
                    .find(|candidate| {
                        live_subgraph_link_index(live_subgraphs, candidate)
                            == live_subgraph_link_index(live_subgraphs, &link)
                    })
                    .map(|candidate| candidate.borrow().member.clone())
                    .unwrap_or_else(|| link.borrow().member.clone())
            };
            let parent_member = {
                let parent = parent_handle.borrow();
                let parent_connection = parent.driver_connection.borrow();
                match_live_bus_member_live(
                    &parent_connection.members,
                    &current_link_member.borrow(),
                )
            };
            let Some(parent_member) = parent_member else {
                continue;
            };

            let (neighbor_name, neighbor_connection_sheet, promoted_connection) = {
                let neighbor = neighbor_handle.borrow();
                let neighbor_connection = neighbor.driver_connection.borrow();
                (
                    neighbor_connection.name.clone(),
                    neighbor_connection.sheet_instance_path.clone(),
                    neighbor_connection.clone(),
                )
            };
            if neighbor_name == parent_member.borrow().full_local_name {
                continue;
            }

            let parent_sheet_instance_path = parent_handle.borrow().sheet_instance_path.clone();
            let neighbor_sheet_instance_path = neighbor_handle.borrow().sheet_instance_path.clone();

            if neighbor_connection_sheet != neighbor_sheet_instance_path {
                if neighbor_connection_sheet != parent_sheet_instance_path {
                    continue;
                }

                let parent_has_search = {
                    let parent = parent_handle.borrow();
                    let parent_connection = parent.driver_connection.borrow();
                    match_live_bus_member_connection(
                        &parent_connection.members,
                        &promoted_connection,
                    )
                    .is_some()
                };
                if parent_has_search {
                    continue;
                }
            }

            if live_reduced_subgraph_driver_priority(&neighbor_handle.borrow()) >= 6 {
                let old_member = current_link_member.clone();
                let refreshed_member = {
                    let parent = parent_handle.borrow();
                    let mut parent_connection = parent.driver_connection.borrow_mut();
                    let Some(member_handle) = match_live_bus_member_mut_live(
                        &mut parent_connection.members,
                        &current_link_member.borrow(),
                    ) else {
                        continue;
                    };
                    clone_live_connection_owner_into_live_bus_member(
                        &mut member_handle.borrow_mut(),
                        &promoted_connection,
                    );
                    member_handle
                };

                {
                    let mut parent = parent_handle.borrow_mut();
                    let refreshed_member = refreshed_member.clone();
                    for candidate_link in &mut parent.bus_neighbor_links {
                        let mut candidate_link = candidate_link.borrow_mut();
                        if Rc::ptr_eq(&candidate_link.member, &old_member)
                            || live_bus_member_handles_eq(&candidate_link.member, &old_member)
                        {
                            candidate_link.member = refreshed_member.clone();
                        }
                    }
                    for candidate_link in &mut parent.bus_parent_links {
                        let mut candidate_link = candidate_link.borrow_mut();
                        if Rc::ptr_eq(&candidate_link.member, &old_member)
                            || live_bus_member_handles_eq(&candidate_link.member, &old_member)
                        {
                            candidate_link.member = refreshed_member.clone();
                        }
                    }
                    parent.dirty = true;
                }

                if !stale_members
                    .iter()
                    .any(|candidate| live_bus_member_handles_eq(candidate, &refreshed_member))
                {
                    stale_members.push(refreshed_member);
                }
                continue;
            }

            clone_live_bus_member_into_live_connection_owner(
                &mut neighbor_handle.borrow().driver_connection.borrow_mut(),
                &parent_member.borrow(),
                &neighbor_sheet_instance_path,
            );
            sync_live_reduced_item_connections_from_driver_handle(&neighbor_handle);
            neighbor_handle.borrow_mut().dirty = true;
        }
    }
}

#[cfg(test)]
fn refresh_reduced_live_bus_neighbor_drivers_on_handles_for_indexes(
    live_subgraphs: &[LiveReducedSubgraphHandle],
    indexes: &[usize],
    stale_members: &mut Vec<LiveProjectBusMemberHandle>,
) {
    let component = indexes
        .iter()
        .filter_map(|index| live_subgraphs.get(*index).cloned())
        .collect::<Vec<_>>();
    refresh_reduced_live_bus_neighbor_drivers_on_handles_for_component(
        live_subgraphs,
        &component,
        stale_members,
    );
}

fn refresh_reduced_live_bus_parent_members_on_handles_for_component(
    live_subgraphs: &[LiveReducedSubgraphHandle],
    component: &[LiveReducedSubgraphHandle],
) {
    // Upstream parity: local live-handle analogue for refreshing parent bus members from a dirty
    // child net. This still stores reduced bus members on the live subgraph owner, but it now
    // consumes the recursive walk's explicit dirty child subset and follows the attached live
    // parent handle plus shared live connection owner instead of recovering the parent through
    // copied source indexes or whole live subgraph clones first. Active dirty checks now compare the
    // target member against the incoming live connection by clone semantics instead of snapshotting
    // reduced before/after members.
    for child_handle in component {
        let child_connection = child_handle.borrow().driver_connection.clone();
        if child_connection.borrow().connection_type != ReducedProjectConnectionType::Net {
            continue;
        }

        let child_sheet_instance_path = child_handle.borrow().sheet_instance_path.clone();
        let parent_links = child_handle.borrow().bus_parent_links.clone();

        for link in parent_links {
            let Some(parent_handle) = live_subgraph_handle_for_link(live_subgraphs, &link) else {
                continue;
            };
            let child_connection_sheet = child_connection.borrow().sheet_instance_path.clone();
            let parent_sheet_instance_path = parent_handle.borrow().sheet_instance_path.clone();
            if child_connection_sheet != child_sheet_instance_path
                && child_connection_sheet != parent_sheet_instance_path
            {
                continue;
            }

            if match_live_bus_member_connection(
                &parent_handle.borrow().driver_connection.borrow().members,
                &child_connection.borrow(),
            )
            .is_some()
            {
                continue;
            }

            let changed = {
                let parent = parent_handle.borrow();
                let mut parent_connection = parent.driver_connection.borrow_mut();
                let link_member = link.borrow().member.clone();
                let Some(member) = match_live_bus_member_mut_live(
                    &mut parent_connection.members,
                    &link_member.borrow(),
                ) else {
                    continue;
                };
                let changed = !live_bus_member_clone_eq_to_connection(
                    &member.borrow(),
                    &child_connection.borrow(),
                );
                clone_live_connection_owner_into_live_bus_member(
                    &mut member.borrow_mut(),
                    &child_connection.borrow(),
                );
                changed
            };

            if changed {
                parent_handle.borrow_mut().dirty = true;
            }
        }
    }
}

fn replay_reduced_live_stale_bus_members_on_handles_for_component(
    component: &[LiveReducedSubgraphHandle],
    stale_members: &[LiveProjectBusMemberHandle],
) {
    for stale_member in stale_members {
        for handle in component {
            let is_bus = {
                let subgraph = handle.borrow();
                let connection = subgraph.driver_connection.borrow();
                matches!(
                    connection.connection_type,
                    ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
                )
            };
            if !is_bus {
                continue;
            }

            let changed = {
                let subgraph = handle.borrow();
                let mut connection = subgraph.driver_connection.borrow_mut();
                let Some(member) =
                    match_live_bus_member_mut_live(&mut connection.members, &stale_member.borrow())
                else {
                    continue;
                };
                if Rc::ptr_eq(&member, stale_member) {
                    continue;
                }
                let changed = !live_bus_member_handle_clone_eq(&member, stale_member);
                clone_live_bus_member_into_live_bus_member(
                    &mut member.borrow_mut(),
                    &stale_member.borrow(),
                );
                changed
            };

            if changed {
                handle.borrow_mut().dirty = true;
            }
        }
    }
}

#[cfg(test)]
fn replay_reduced_live_stale_bus_members_on_handles_for_indexes(
    live_subgraphs: &[LiveReducedSubgraphHandle],
    indexes: &[usize],
    stale_members: &[LiveProjectBusMemberHandle],
) {
    let component = indexes
        .iter()
        .filter_map(|index| live_subgraphs.get(*index).cloned())
        .collect::<Vec<_>>();
    replay_reduced_live_stale_bus_members_on_handles_for_component(&component, stale_members);
}

fn refresh_reduced_live_bus_link_members_on_handles_for_component(
    live_subgraphs: &[LiveReducedSubgraphHandle],
    component: &[LiveReducedSubgraphHandle],
) {
    // Upstream parity: local live-handle analogue for rematching bus parent/neighbor links after
    // propagation. Active refresh now prefers the attached live parent/child handles, shared live
    // connection owners, and handle-keyed refresh state over copied link or subgraph indexes.
    // This helper still rebuilds reduced link carriers because the repo does not yet keep a fuller
    // local `CONNECTION_SUBGRAPH` payload for neighbor topology, but it now marks the live owner
    // dirty when those attached links change so recursive revisits follow live dirty-state directly
    // instead of whole-subgraph equality checks.
    let mut refreshed_parent_links = BTreeMap::<usize, Vec<LiveReducedSubgraphLinkHandle>>::new();

    for child_handle in component {
        let child_id = live_subgraph_handle_id(child_handle);
        let child_connection = child_handle.borrow().driver_connection.clone();
        let existing_parent_links = child_handle.borrow().bus_parent_links.clone();

        let mut parent_handles = live_subgraph_bus_parent_handles_from_handle(child_handle)
            .into_iter()
            .collect::<Vec<_>>();
        for link in &existing_parent_links {
            let Some(parent_handle) = live_subgraph_handle_for_link(live_subgraphs, link) else {
                continue;
            };
            if !parent_handles
                .iter()
                .any(|candidate| Rc::ptr_eq(candidate, &parent_handle))
            {
                parent_handles.push(parent_handle);
            }
        }

        for parent_handle in parent_handles {
            let existing_member = existing_parent_links
                .iter()
                .find(|link| {
                    live_subgraph_handle_for_link(live_subgraphs, link)
                        .as_ref()
                        .is_some_and(|candidate| Rc::ptr_eq(candidate, &parent_handle))
                })
                .map(|link| link.borrow().member.clone());

            let parent_neighbor_member = parent_handle
                .borrow()
                .bus_neighbor_links
                .iter()
                .find(|link| {
                    live_subgraph_handle_for_link(live_subgraphs, link)
                        .as_ref()
                        .is_some_and(|candidate| Rc::ptr_eq(candidate, child_handle))
                })
                .map(|link| link.borrow().member.clone());

            let refreshed_member = existing_member
                .as_ref()
                .and_then(|search| {
                    match_live_bus_member_live(
                        &parent_handle.borrow().driver_connection.borrow().members,
                        &search.borrow(),
                    )
                })
                .or_else(|| {
                    parent_neighbor_member.as_ref().and_then(|search| {
                        match_live_bus_member_live(
                            &parent_handle.borrow().driver_connection.borrow().members,
                            &search.borrow(),
                        )
                    })
                })
                .or_else(|| {
                    match_live_bus_member_connection(
                        &parent_handle.borrow().driver_connection.borrow().members,
                        &child_connection.borrow(),
                    )
                });

            let Some(refreshed_member) = refreshed_member else {
                continue;
            };

            refreshed_parent_links
                .entry(child_id)
                .or_default()
                .push(Rc::new(RefCell::new(LiveReducedSubgraphLink {
                    member: refreshed_member,
                    #[cfg(test)]
                    subgraph_index: parent_handle.borrow().source_index,
                    subgraph_handle: Some(Rc::downgrade(&parent_handle)),
                })));
        }
    }

    let mut refreshed_neighbor_links = BTreeMap::<usize, Vec<LiveReducedSubgraphLinkHandle>>::new();

    for child_handle in component {
        let child_id = live_subgraph_handle_id(child_handle);
        let child_index = live_subgraph_projection_index(live_subgraphs, child_handle);
        for link in refreshed_parent_links.get(&child_id).into_iter().flatten() {
            let Some(neighbor_handle) = live_subgraph_handle_for_link(live_subgraphs, link) else {
                continue;
            };
            refreshed_neighbor_links
                .entry(live_subgraph_handle_id(&neighbor_handle))
                .or_default()
                .push(Rc::new(RefCell::new(LiveReducedSubgraphLink {
                    member: link.borrow().member.clone(),
                    #[cfg(test)]
                    subgraph_index: child_index,
                    subgraph_handle: live_subgraphs.get(child_index).map(Rc::downgrade),
                })));
        }
    }

    for handle in component {
        let handle_id = live_subgraph_handle_id(handle);
        let mut live = handle.borrow_mut();
        let mut next_parent_links = refreshed_parent_links
            .get(&handle_id)
            .cloned()
            .unwrap_or_default();
        sort_dedup_live_subgraph_link_handles(&mut next_parent_links);
        let mut next_neighbor_links = refreshed_neighbor_links
            .get(&handle_id)
            .cloned()
            .unwrap_or_default();
        sort_dedup_live_subgraph_link_handles(&mut next_neighbor_links);
        let links_changed = live.bus_parent_links != next_parent_links
            || live.bus_neighbor_links != next_neighbor_links;
        live.bus_parent_links = next_parent_links;
        live.bus_neighbor_links = next_neighbor_links;
        if links_changed {
            live.dirty = true;
        }
    }
}

fn refresh_reduced_live_bus_link_members_on_handles_for_indexes(
    live_subgraphs: &[LiveReducedSubgraphHandle],
    indexes: &[usize],
) {
    let component = indexes
        .iter()
        .filter_map(|index| live_subgraphs.get(*index).cloned())
        .collect::<Vec<_>>();
    refresh_reduced_live_bus_link_members_on_handles_for_component(live_subgraphs, &component);
}

fn refresh_reduced_live_multiple_bus_parent_names_on_handles(
    live_subgraphs: &[LiveReducedSubgraphHandle],
) {
    let (mut subgraphs_by_name, mut subgraphs_by_sheet_and_name) =
        build_live_reduced_name_handle_caches_from_handles(live_subgraphs);

    for subgraph_handle in live_subgraphs {
        let bus_parent_links = subgraph_handle.borrow().bus_parent_links.clone();
        if bus_parent_links.len() < 2 {
            continue;
        }

        let connection = {
            let subgraph = subgraph_handle.borrow();
            let connection = subgraph.driver_connection.borrow();
            if connection.connection_type != ReducedProjectConnectionType::Net {
                continue;
            }
            subgraph.driver_connection.clone()
        };

        for link in bus_parent_links {
            let Some(parent_handle) = live_subgraph_handle_for_link(live_subgraphs, &link) else {
                continue;
            };
            let old_name = {
                let parent = parent_handle.borrow();
                let mut parent_connection = parent.driver_connection.borrow_mut();
                let link_member = link.borrow().member.clone();
                let Some(member) = match_live_bus_member_mut_live(
                    &mut parent_connection.members,
                    &link_member.borrow(),
                ) else {
                    continue;
                };

                if member.borrow().full_local_name == connection.borrow().full_local_name {
                    continue;
                }

                let old_name = member.borrow().full_local_name.clone();
                clone_live_connection_owner_into_live_bus_member(
                    &mut member.borrow_mut(),
                    &connection.borrow(),
                );
                old_name
            };
            parent_handle.borrow_mut().dirty = true;

            let candidate_handles = subgraphs_by_name
                .get(&old_name)
                .cloned()
                .unwrap_or_default();

            for candidate_handle in candidate_handles {
                let old_candidate_name = candidate_handle.borrow().driver_connection.name();
                if old_candidate_name == old_name {
                    let changed = {
                        let candidate = candidate_handle.borrow();
                        let changed = !live_connection_handle_clone_eq(
                            &candidate.driver_connection,
                            &connection,
                        );
                        candidate.driver_connection.clone_from(&connection);
                        changed
                    };
                    if changed {
                        sync_live_reduced_item_connections_from_driver_handle(&candidate_handle);
                        candidate_handle.borrow_mut().dirty = true;
                        recache_live_reduced_subgraph_name_handle_cache_from_handles(
                            &mut subgraphs_by_name,
                            &mut subgraphs_by_sheet_and_name,
                            &candidate_handle,
                            &old_candidate_name,
                        );
                    }
                }
            }
        }
    }
}

fn run_reduced_live_graph_roots_on_handles(
    live_subgraphs: &[LiveReducedSubgraphHandle],
    force: bool,
) {
    let max_roots = live_subgraphs
        .len()
        .saturating_mul(live_subgraphs.len().max(1));
    let mut roots = 0;

    for start in live_subgraphs {
        if !start.borrow().dirty {
            continue;
        }

        roots += 1;
        if roots > max_roots {
            break;
        }

        let mut stale_members = Vec::new();
        let mut visiting = BTreeSet::new();
        propagate_reduced_live_graph_neighbors_on_handles(
            start,
            live_subgraphs,
            force,
            &mut visiting,
            &mut stale_members,
        );
    }
}

fn refresh_reduced_live_post_propagation_item_connections_on_handles(
    live_subgraphs: &[LiveReducedSubgraphHandle],
) {
    for handle in live_subgraphs {
        sync_live_reduced_item_connections_from_driver_handle(handle);

        if {
            let subgraph = handle.borrow();
            live_subgraph_is_self_driven_symbol_pin(&subgraph)
                && subgraph.driver_connection.borrow().name.contains("Net-(")
        } {
            let subgraph = handle.borrow();
            let mut connection = subgraph.driver_connection.borrow_mut();
            connection.name = reduced_force_no_connect_net_name(&connection.name);
            connection.local_name = reduced_force_no_connect_net_name(&connection.local_name);
            connection.full_local_name =
                reduced_force_no_connect_net_name(&connection.full_local_name);
            drop(connection);
            drop(subgraph);
            sync_live_reduced_item_connections_from_driver_handle(handle);
        }

        if {
            let subgraph = handle.borrow();
            live_subgraph_is_self_driven_sheet_pin(&subgraph)
                && matches!(
                    subgraph.driver_connection.borrow().connection_type,
                    ReducedProjectConnectionType::Net
                )
        } {
            if let Some((connection_type, members)) =
                live_subgraph_child_handles_from_handle(handle)
                    .into_iter()
                    .find_map(|child_handle| {
                        let child = child_handle.borrow();
                        let child_connection = child.driver_connection.borrow();

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
            {
                let subgraph = handle.borrow();
                let mut connection = subgraph.driver_connection.borrow_mut();
                connection.connection_type = connection_type;
                connection.members = members;
                drop(connection);
                drop(subgraph);
                sync_live_reduced_item_connections_from_driver_handle(handle);
            }
        }
    }
}

// Upstream parity: local bridge toward one shared live `propagateToNeighbors()` owner during
// graph build. This still runs on reduced live carriers instead of the final local
// `CONNECTION_SUBGRAPH` analogue, but it moves the active recursion, dirty-state, and stale-member
// replay onto shared subgraph handles rather than value-owned subgraphs. This is materially
// closer to upstream now that the recursive walk consumes one explicit dirty-handle subset before
// each pass and any in-pass mutation can immediately requeue the same live subgraph for another
// recursive visit without a whole-subgraph compatibility compare. Remaining divergence is the
// still-missing fuller local `CONNECTION_SUBGRAPH` / item-pointer topology.
fn propagate_reduced_live_graph_neighbors_on_handles(
    start: &LiveReducedSubgraphHandle,
    live_subgraphs: &[LiveReducedSubgraphHandle],
    force: bool,
    visiting: &mut BTreeSet<usize>,
    stale_members: &mut Vec<LiveProjectBusMemberHandle>,
) {
    let start_id = live_subgraph_handle_id(start);
    if !start.borrow().dirty || !visiting.insert(start_id) {
        return;
    }

    if !force {
        let promoted = refresh_reduced_live_global_secondary_driver_promotions_for_handle(
            start,
            live_subgraphs,
        );

        for promoted_handle in promoted {
            propagate_reduced_live_graph_neighbors_on_handles(
                &promoted_handle,
                live_subgraphs,
                false,
                visiting,
                stale_members,
            );
        }
    }

    let active =
        collect_live_reduced_propagation_component_handles_from_handles(start, live_subgraphs);
    let dirty_active = active
        .iter()
        .filter(|handle| handle.borrow().dirty)
        .cloned()
        .collect::<Vec<_>>();

    for handle in &dirty_active {
        handle.borrow_mut().dirty = false;
    }

    propagate_reduced_hierarchy_driver_chains_on_live_subgraph_handles_for_component(
        live_subgraphs,
        &dirty_active,
        force,
    );
    refresh_reduced_live_bus_neighbor_drivers_on_handles_for_component(
        live_subgraphs,
        &dirty_active,
        stale_members,
    );
    refresh_reduced_live_bus_parent_members_on_handles_for_component(live_subgraphs, &dirty_active);
    replay_reduced_live_stale_bus_members_on_handles_for_component(&active, stale_members);
    refresh_reduced_live_bus_link_members_on_handles_for_component(live_subgraphs, &active);

    let recurse_targets = live_subgraphs
        .iter()
        .filter(|handle| !Rc::ptr_eq(handle, start) && handle.borrow().dirty)
        .cloned()
        .collect::<Vec<_>>();

    visiting.remove(&start_id);
    for handle in recurse_targets {
        propagate_reduced_live_graph_neighbors_on_handles(
            &handle,
            live_subgraphs,
            force,
            visiting,
            stale_members,
        );
    }

    if start.borrow().dirty {
        propagate_reduced_live_graph_neighbors_on_handles(
            start,
            live_subgraphs,
            force,
            visiting,
            stale_members,
        );
    }
}

// Upstream parity: reduced local bridge toward one live `propagateToNeighbors()` owner during
// graph build. This now follows KiCad's two-pass caller shape more closely by recursively
// traversing dirty live subgraphs with a shared stale-member bag per root before running the
// post-propagation multi-parent rename and item-update steps. This active graph-build path now
// runs on shared live subgraph handles. Hierarchy and link updates now consume one explicit dirty
// handle subset per recursive visit and requeue through dirty ownership directly. Remaining
// divergence is the still-missing fuller local `CONNECTION_SUBGRAPH` / item-pointer topology
// behind those handles.
#[cfg_attr(not(test), allow(dead_code))]
fn refresh_reduced_live_graph_propagation(reduced_subgraphs: &mut [ReducedProjectSubgraphEntry]) {
    let _ = refresh_reduced_live_graph_propagation_with_handles(reduced_subgraphs);
}

fn refresh_reduced_live_graph_propagation_with_handles(
    reduced_subgraphs: &mut [ReducedProjectSubgraphEntry],
) -> Vec<LiveReducedSubgraphHandle> {
    let live_subgraphs = build_live_reduced_subgraph_handles(reduced_subgraphs);
    run_reduced_live_graph_roots_on_handles(&live_subgraphs, false);
    run_reduced_live_graph_roots_on_handles(&live_subgraphs, true);

    let all_indexes = (0..live_subgraphs.len()).collect::<Vec<_>>();
    refresh_reduced_live_multiple_bus_parent_names_on_handles(&live_subgraphs);
    run_reduced_live_graph_roots_on_handles(&live_subgraphs, false);
    run_reduced_live_graph_roots_on_handles(&live_subgraphs, true);
    refresh_reduced_live_bus_link_members_on_handles_for_indexes(&live_subgraphs, &all_indexes);
    refresh_reduced_live_post_propagation_item_connections_on_handles(&live_subgraphs);
    apply_live_reduced_driver_connections_from_handles(reduced_subgraphs, &live_subgraphs);
    live_subgraphs
}

// Upstream parity: reduced local analogue for the post-propagation item-connection update KiCad
// performs after subgraph names settle. This still projects back onto reduced subgraph snapshots
// instead of mutating live item-owned `SCH_CONNECTION` objects, but it now refreshes live
// label/sheet-pin/hier-port connection owners before the final reduced projection instead of
// treating every item connection as an end-of-pass clone of the chosen driver. The active live
// path now derives the exercised self-driven symbol-pin and sheet-pin branches from shared live
// base-pin and sheet-pin payload instead of a copied live driver-identity summary.
#[cfg(test)]
fn refresh_reduced_live_post_propagation_item_connections(
    reduced_subgraphs: &mut [ReducedProjectSubgraphEntry],
) {
    let mut live_subgraphs = build_live_reduced_subgraphs(reduced_subgraphs);
    refresh_reduced_live_post_propagation_item_connections_on_live_subgraphs(&mut live_subgraphs);
    apply_live_reduced_driver_connections(reduced_subgraphs, &live_subgraphs);
}

#[cfg(test)]
fn refresh_reduced_live_post_propagation_item_connections_on_live_subgraphs(
    live_subgraphs: &mut [LiveReducedSubgraph],
) {
    for index in 0..live_subgraphs.len() {
        sync_live_reduced_item_connections_from_driver(&mut live_subgraphs[index]);

        if live_subgraph_is_self_driven_symbol_pin(&live_subgraphs[index])
            && live_subgraphs[index]
                .driver_connection
                .borrow()
                .name
                .contains("Net-(")
        {
            let mut connection = live_subgraphs[index].driver_connection.borrow_mut();
            connection.name = reduced_force_no_connect_net_name(&connection.name);
            connection.local_name = reduced_force_no_connect_net_name(&connection.local_name);
            connection.full_local_name =
                reduced_force_no_connect_net_name(&connection.full_local_name);
            drop(connection);
            sync_live_reduced_item_connections_from_driver(&mut live_subgraphs[index]);
        }

        if live_subgraph_is_self_driven_sheet_pin(&live_subgraphs[index])
            && matches!(
                live_subgraphs[index]
                    .driver_connection
                    .borrow()
                    .connection_type,
                ReducedProjectConnectionType::Net
            )
        {
            if let Some((connection_type, members)) = live_subgraphs[index]
                .hier_child_indexes
                .iter()
                .find_map(|child_index| {
                    let child_connection = live_subgraphs[*child_index].driver_connection.borrow();

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
                let mut connection = live_subgraphs[index].driver_connection.borrow_mut();
                connection.connection_type = connection_type;
                connection.members = members;
                drop(connection);
                sync_live_reduced_item_connections_from_driver(&mut live_subgraphs[index]);
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
#[cfg(test)]
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
            if reduced_project_strong_driver_full_name(&secondary_driver) == chosen_connection.name
            {
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
                        reduced_project_strong_driver_full_name(candidate_driver)
                            == reduced_project_strong_driver_full_name(&secondary_driver)
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

// Upstream parity: reduced local analogue for the global-secondary-driver promotion branch in
// `CONNECTION_GRAPH::Recalculate()` immediately before `propagateToNeighbors()`. This still stops
// short of pointer-owned driver/item mutation, but it now mutates the shared live subgraph owner
// instead of promoting disconnected candidates on reduced snapshots before the live graph runs.
// Remaining divergence is the still-missing pointer-owned driver/item mutation on the promoted
// subgraph itself.
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
// from linked lib-pin draw items instead of falling back to wire-only geometry, and now also keeps
// pin-number identity so stacked pins on the same symbol/point are not collapsed before the shared
// graph sees them.
fn projected_symbol_pins(symbol: &Symbol) -> Vec<ConnectionMember> {
    projected_symbol_pin_info(symbol)
        .into_iter()
        .map(|pin| ConnectionMember {
            kind: ConnectionMemberKind::SymbolPin,
            at: pin.at,
            symbol_uuid: symbol.uuid.clone(),
            pin_number: pin.number,
            visible: true,
            electrical_type: pin.electrical_type,
        })
        .collect()
}

// Upstream parity: reduced local connection-point owner used before subgraph grouping. This still
// stores reduced members instead of live `SCH_ITEM*`, but symbol-pin dedup now keys by both symbol
// UUID and pin number so stacked pins stay distinct the way separate `SCH_PIN` items do upstream.
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
                && existing.pin_number == member.pin_number
        }) {
            if member.visible && !existing.visible {
                *existing = member;
            }

            return;
        }
    }

    entry.members.push(member);
}

fn connection_member_matches_projected_symbol_pin(
    member: &ConnectionMember,
    symbol: &Symbol,
    pin: &ProjectedSymbolPin,
) -> bool {
    member.kind == ConnectionMemberKind::SymbolPin
        && member.symbol_uuid == symbol.uuid
        && member.pin_number == pin.number
        && points_equal(member.at, pin.at)
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
                            pin_number: None,
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
                            pin_number: None,
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
                            pin_number: None,
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
                            pin_number: None,
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
                        pin_number: None,
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
                        pin_number: None,
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
                        pin_number: None,
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

fn union_point_indexes(point_indexes: &[usize], dsu: &mut DisjointSet) {
    for pair in point_indexes.windows(2) {
        dsu.union(pair[0], pair[1]);
    }
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
// Connected symbol pins now also mirror KiCad's `updateSymbolConnectivity()` jumper linking:
// duplicate pin numbers marked as jumpers and explicit `jumper_pin_groups` are unioned before
// component extraction instead of being left to later ERC-only special cases.
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
        union_point_indexes(point_indexes, &mut dsu);
    }

    for item in &schematic.screen.items {
        let SchItem::Symbol(symbol) = item else {
            continue;
        };
        let Some(lib_symbol) = symbol.lib_symbol.as_ref() else {
            continue;
        };
        let projected_pins = projected_symbol_pin_info(symbol);
        let mut point_indexes_by_number = BTreeMap::<String, Vec<usize>>::new();

        for pin in &projected_pins {
            let Some(pin_number) = pin.number.as_ref() else {
                continue;
            };
            let point_indexes = points
                .iter()
                .enumerate()
                .filter_map(|(point_index, point)| {
                    point
                        .members
                        .iter()
                        .any(|member| {
                            connection_member_matches_projected_symbol_pin(member, symbol, pin)
                        })
                        .then_some(point_index)
                })
                .collect::<Vec<_>>();

            if point_indexes.is_empty() {
                continue;
            }

            point_indexes_by_number
                .entry(pin_number.clone())
                .or_default()
                .extend(point_indexes);
        }

        if lib_symbol.duplicate_pin_numbers_are_jumpers {
            for point_indexes in point_indexes_by_number.values() {
                union_point_indexes(point_indexes, &mut dsu);
            }
        }

        for group in &lib_symbol.jumper_pin_groups {
            let point_indexes = group
                .iter()
                .filter_map(|pin_number| point_indexes_by_number.get(pin_number))
                .flatten()
                .copied()
                .collect::<Vec<_>>();
            union_point_indexes(&point_indexes, &mut dsu);
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

            let unit_pins = projected_symbol_pin_info(symbol);

            for pin in unit_pins.iter() {
                let Some(base_pin_number) = pin.number.clone() else {
                    continue;
                };

                if !component.members.iter().any(|member| {
                    connection_member_matches_projected_symbol_pin(member, symbol, &pin)
                }) {
                    continue;
                }

                let pinfunction_base = pin.name.clone().and_then(|name| {
                    let trimmed = name.trim();
                    (!trimmed.is_empty() && trimmed != "~").then_some(name)
                });
                let (expanded_numbers, _) = expand_stacked_pin_notation(&base_pin_number);
                let base_pin = ReducedProjectBasePin {
                    key: ReducedNetBasePinKey {
                        sheet_instance_path: sheet_instance_path.to_string(),
                        symbol_uuid: symbol.uuid.clone(),
                        at: point_key(pin.at),
                        name: pin.name.clone(),
                        number: pin.number.clone(),
                    },
                    number: pin.number.clone(),
                    electrical_type: pin.electrical_type.clone(),
                    connection: reduced_seeded_symbol_pin_connection(
                        symbol,
                        pin,
                        &unit_pins,
                        sheet_instance_path,
                    ),
                };
                base_pins.push(base_pin);

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
        chosen_driver_identity: Option<ReducedProjectDriverIdentity>,
        drivers: Vec<ReducedProjectStrongDriver>,
        class: String,
        has_no_connect: bool,
        sheet_instance_path: String,
        anchor: PointKey,
        points: Vec<PointKey>,
        nodes: Vec<ReducedNetNode>,
        base_pins: Vec<ReducedProjectBasePin>,
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
                let strong_drivers = collect_reduced_strong_drivers(
                    schematic,
                    &sheet_path.schematic_path,
                    &sheet_path.instance_path,
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
                let chosen_driver_identity = driver_candidate
                    .as_ref()
                    .and_then(|candidate| candidate.identity.as_ref())
                    .map(|identity| {
                        reduced_local_driver_identity_to_project_identity(
                            &sheet_path.schematic_path,
                            identity,
                        )
                    });

                pending_subgraphs.push(PendingProjectSubgraph {
                    name: entry.name.clone(),
                    driver_connection,
                    chosen_driver_identity,
                    drivers: strong_drivers.clone(),
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
                    .extend(base_pins.iter().map(|base_pin| base_pin.key.clone()));

                for node in nodes {
                    let key = (
                        sheet_path.instance_path.clone(),
                        node.reference.clone(),
                        node.pin.clone(),
                    );
                    let base_pin_key = base_pins
                        .iter()
                        .find(|base_pin| {
                            base_pin.key.symbol_uuid.is_some()
                                && node
                                    .pinfunction
                                    .as_ref()
                                    .map(|pinfunction| {
                                        base_pin
                                            .key
                                            .name
                                            .as_ref()
                                            .is_some_and(|name| pinfunction.starts_with(name))
                                    })
                                    .unwrap_or(base_pin.key.name.is_none())
                        })
                        .map(|base_pin| base_pin.key.clone())
                        .or_else(|| base_pins.first().map(|base_pin| base_pin.key.clone()))
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
            chosen_driver_identity: pending.chosen_driver_identity.clone(),
            drivers: pending.drivers.clone(),
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
            pin_subgraph_identities.insert(base_pin.key.clone(), index);
            pin_subgraph_identities_by_location.insert(
                ReducedProjectPinIdentityKey {
                    sheet_instance_path: base_pin.key.sheet_instance_path.clone(),
                    symbol_uuid: base_pin.key.symbol_uuid.clone(),
                    at: base_pin.key.at,
                    number: base_pin.key.number.clone(),
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

    let live_subgraphs =
        refresh_reduced_live_graph_propagation_with_handles(&mut reduced_subgraphs);
    attach_reduced_connected_bus_items(&mut reduced_subgraphs);
    let (subgraphs_by_name, subgraphs_by_sheet_and_name) =
        rebuild_reduced_project_graph_name_caches(&mut reduced_subgraphs);
    let (pin_driver_connections, pin_driver_connections_by_location) =
        collect_reduced_project_pin_driver_connections_from_live_handles(&live_subgraphs);

    ReducedProjectNetGraph {
        subgraphs: reduced_subgraphs,
        subgraphs_by_name,
        subgraphs_by_sheet_and_name,
        pin_subgraph_identities,
        pin_subgraph_identities_by_location,
        pin_driver_connections,
        pin_driver_connections_by_location,
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
                    base_pin.key.symbol_uuid.is_some()
                        && node
                            .pinfunction
                            .as_ref()
                            .map(|pinfunction| {
                                base_pin
                                    .key
                                    .name
                                    .as_ref()
                                    .is_some_and(|name| pinfunction.starts_with(name))
                            })
                            .unwrap_or(base_pin.key.name.is_none())
                })
                .map(|base_pin| base_pin.key.clone())
                .or_else(|| {
                    subgraph
                        .base_pins
                        .first()
                        .map(|base_pin| base_pin.key.clone())
                });
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
            let base_pin_key = base_pin.key;
            if !entry.3.contains(&base_pin_key) {
                entry.3.push(base_pin_key);
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
    pin_number: Option<&str>,
) -> ReducedProjectPinIdentityKey {
    ReducedProjectPinIdentityKey {
        sheet_instance_path: sheet_path.instance_path.clone(),
        symbol_uuid: symbol.uuid.clone(),
        at: point_key(at),
        number: pin_number.map(str::to_string),
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
                    let source = match label.kind {
                        LabelKind::Global => ReducedNetNameSource::GlobalLabel,
                        LabelKind::Local => ReducedNetNameSource::LocalLabel,
                        LabelKind::Hierarchical => ReducedNetNameSource::HierarchicalLabel,
                        LabelKind::Directive => return None,
                    };
                    let full_name = reduced_driver_full_name(&shown, source, &sheet_path_prefix);
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
                    let full_name = reduced_driver_full_name(
                        &shown,
                        ReducedNetNameSource::SheetPin,
                        &sheet_path_prefix,
                    );

                    hier_sheet_pins.push(ReducedHierSheetPinLink {
                        at: point_key(pin.at),
                        child_sheet_uuid: sheet.uuid.clone(),
                        connection: build_reduced_project_connection(
                            schematic,
                            parent_sheet_path.instance_path.clone(),
                            full_name.clone(),
                            shown.clone(),
                            full_name,
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
                let full_name = reduced_driver_full_name(
                    &shown,
                    ReducedNetNameSource::HierarchicalLabel,
                    &sheet_path_prefix,
                );

                hier_ports.push(ReducedHierPortLink {
                    at: point_key(label.at),
                    connection: build_reduced_project_connection(
                        schematic,
                        parent_sheet_path.instance_path.clone(),
                        full_name.clone(),
                        shown.clone(),
                        full_name,
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

// Upstream parity: reduced local key for the symbol-pin branch of
// `CONNECTION_GRAPH::GetSubgraphForItem()`. This is not a 1:1 KiCad item key because the Rust
// tree still has no live `SCH_PIN*`, but it now includes projected pin number so stacked same-name
// pins can stay distinct at the reduced project-graph lookup boundary. Remaining divergence is the
// still-missing full live pin item owner.
fn reduced_project_base_pin_key(
    sheet_path: &LoadedSheetPath,
    symbol: &Symbol,
    at: [f64; 2],
    pin_name: &str,
    pin_number: Option<&str>,
) -> ReducedNetBasePinKey {
    ReducedNetBasePinKey {
        sheet_instance_path: sheet_path.instance_path.clone(),
        symbol_uuid: symbol.uuid.clone(),
        at: point_key(at),
        name: Some(pin_name.to_string()),
        number: pin_number.map(str::to_string),
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
    pin_number: Option<&str>,
) -> Option<ReducedProjectNetIdentity> {
    resolve_reduced_project_subgraph_for_symbol_pin(
        graph, sheet_path, symbol, at, pin_name, pin_number,
    )
    .map(|subgraph| ReducedProjectNetIdentity {
        code: subgraph.code,
        name: subgraph.name.clone(),
        class: subgraph.class.clone(),
        has_no_connect: subgraph.has_no_connect,
    })
}

#[cfg_attr(not(test), allow(dead_code))]
// Upstream parity: reduced local analogue for the symbol-pin half of
// `CONNECTION_GRAPH::GetSubgraphForItem()` on the project graph path. This is not a 1:1 KiCad
// item map because the Rust tree still uses reduced projected pin identity instead of a live
// `SCH_PIN*`, but it preserves shared pin-to-subgraph identity, and both the named and by-location
// lookup edges now include projected pin number so stacked pins do not collapse before the shared
// graph owner sees them. Remaining divergence is fuller item ownership for non-pin items and the
// still-missing live `CONNECTION_SUBGRAPH` object.
pub(crate) fn resolve_reduced_project_subgraph_for_symbol_pin<'a>(
    graph: &'a ReducedProjectNetGraph,
    sheet_path: &LoadedSheetPath,
    symbol: &Symbol,
    at: [f64; 2],
    pin_name: Option<&str>,
    pin_number: Option<&str>,
) -> Option<&'a ReducedProjectSubgraphEntry> {
    pin_name
        .and_then(|pin_name| {
            graph
                .pin_subgraph_identities
                .get(&reduced_project_base_pin_key(
                    sheet_path, symbol, at, pin_name, pin_number,
                ))
        })
        .and_then(|index| graph.subgraphs.get(*index))
        .or_else(|| {
            graph
                .pin_subgraph_identities_by_location
                .get(&reduced_project_pin_identity_key(
                    sheet_path, symbol, at, pin_number,
                ))
                .and_then(|index| graph.subgraphs.get(*index))
        })
}

// Upstream parity: reduced local analogue for the symbol-pin `Name(true)` path via
// `CONNECTION_GRAPH::GetSubgraphForItem()`. This is not a 1:1 KiCad connection object because the
// Rust tree still lacks live `SCH_CONNECTION` instances, but the project graph now preserves
// graph-owned per-pin driver connections projected from the live base-pin owners, and both the
// named and by-location lookup edges now include projected pin number so stacked pins do not
// collapse to one driver-name query. Base-pin owners now also start with
// `updatePinConnectivity()`-style seeded names, so this lookup ignores auto-generated pin-owned
// names when the graph has a better chosen driver and only reports the pin-owned name directly for
// exercised power-pin and pin-default branches. Remaining divergence is fuller live
// connection-object caching and item ownership.
pub(crate) fn resolve_reduced_project_driver_name_for_symbol_pin(
    graph: &ReducedProjectNetGraph,
    sheet_path: &LoadedSheetPath,
    symbol: &Symbol,
    at: [f64; 2],
    pin_name: Option<&str>,
    pin_number: Option<&str>,
) -> Option<String> {
    pin_name
        .and_then(|pin_name| {
            graph
                .pin_driver_connections
                .get(&reduced_project_base_pin_key(
                    sheet_path, symbol, at, pin_name, pin_number,
                ))
        })
        .or_else(|| {
            graph
                .pin_driver_connections_by_location
                .get(&reduced_project_pin_identity_key(
                    sheet_path, symbol, at, pin_number,
                ))
        })
        .and_then(|connection| {
            (!connection.local_name.is_empty()
                && !is_auto_generated_net_name(&connection.local_name))
            .then(|| connection.local_name.clone())
        })
        .or_else(|| {
            resolve_reduced_project_subgraph_for_symbol_pin(
                graph, sheet_path, symbol, at, pin_name, pin_number,
            )
            .and_then(|subgraph| {
                subgraph
                    .driver_connection
                    .as_ref()
                    .map(|connection| connection.local_name.clone())
            })
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

// Upstream parity: reduced local analogue for the per-pin `SCH_CONNECTION` seeding KiCad performs
// in `updateSymbolConnectivity()` / `updatePinConnectivity()` before `ResolveDrivers()`. This is
// still reduced projected pin state rather than a live `SCH_PIN`, but it now gives every base-pin
// owner a net-typed connection plus the exercised power-pin or default-net seeded name from setup
// time, matching the upstream per-pin seed path more closely than starting most base pins as
// `CONNECTION_TYPE::NONE` or leaving ordinary pins unnamed until later graph ownership attaches.
fn reduced_seeded_symbol_pin_connection(
    symbol: &Symbol,
    pin: &ProjectedSymbolPin,
    unit_pins: &[ProjectedSymbolPin],
    sheet_instance_path: &str,
) -> ReducedProjectConnection {
    let mut connection = ReducedProjectConnection {
        net_code: 0,
        connection_type: ReducedProjectConnectionType::Net,
        name: String::new(),
        local_name: String::new(),
        full_local_name: String::new(),
        sheet_instance_path: sheet_instance_path.to_string(),
        members: Vec::new(),
    };

    if reduced_power_pin_driver_priority(symbol, pin.electrical_type.as_deref()) == Some(6) {
        if let Some(name) = symbol_value_text(symbol) {
            connection.name = name.clone();
            connection.local_name = name.clone();
            connection.full_local_name = name;
        }
    } else if let Some(name) = reduced_symbol_pin_default_net_name(symbol, pin, unit_pins, false) {
        connection.name = name.clone();
        connection.local_name = name.clone();
        connection.full_local_name = name;
    }

    connection
}

// Upstream parity: reduced local analogue for `SCH_PIN::GetDefaultNetName()`. This still runs on
// reduced projected pin data instead of live `SCH_PIN` items, but it now applies the exercised
// stacked-pin effective-pad-number branch instead of always naming from the raw shown pin number.
// Remaining divergence is fuller stacked-pin parsing and the still-missing live pin object/cache
// behavior around this naming path.
fn reduced_symbol_pin_default_net_name(
    symbol: &Symbol,
    pin: &ProjectedSymbolPin,
    unit_pins: &[ProjectedSymbolPin],
    force_no_connect: bool,
) -> Option<String> {
    let reference = symbol_reference_text(symbol)?;
    let pin_number = pin.number.as_deref()?;
    let effective_pad_number = reduced_effective_pad_number(pin_number);

    if reference.ends_with('?') {
        let symbol_uuid = symbol.uuid.as_deref()?;
        return Some(format!("Net-({symbol_uuid}-Pad{effective_pad_number})"));
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
            name.push_str(&format!("-Pad{effective_pad_number}"));
        }

        name.push(')');
        return Some(name);
    }

    Some(format!("{prefix}{reference}-Pad{effective_pad_number})"))
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
    reduced_driver_full_name(&candidate.text, candidate.source, sheet_path_prefix)
}

fn reduced_driver_full_name(
    text: &str,
    source: ReducedNetNameSource,
    sheet_path_prefix: &str,
) -> String {
    let prepend_path = matches!(
        source,
        ReducedNetNameSource::LocalLabel
            | ReducedNetNameSource::HierarchicalLabel
            | ReducedNetNameSource::SheetPin
            | ReducedNetNameSource::LocalPowerPin
    );

    if prepend_path {
        if text.starts_with('/') {
            text.to_string()
        } else {
            format!("{sheet_path_prefix}{text}")
        }
    } else {
        text.to_string()
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
        pin_number: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReducedDriverNameCandidate {
    priority: i32,
    sheet_pin_rank: i32,
    text: String,
    source: ReducedNetNameSource,
    identity: Option<ReducedLocalDriverIdentity>,
}

// Upstream parity: reduced bridge from local `ResolveDrivers()` candidate identity into the
// project-graph driver identity owner. This helper still exists because the reduced graph does not
// yet project live `SCH_ITEM*` / `SCH_CONNECTION*` identity directly across the graph boundary.
fn reduced_local_driver_identity_to_project_identity(
    schematic_path: &std::path::Path,
    identity: &ReducedLocalDriverIdentity,
) -> ReducedProjectDriverIdentity {
    match identity {
        ReducedLocalDriverIdentity::Label { at, kind } => ReducedProjectDriverIdentity::Label {
            schematic_path: schematic_path.to_path_buf(),
            at: *at,
            kind: *kind,
        },
        ReducedLocalDriverIdentity::SheetPin { at } => ReducedProjectDriverIdentity::SheetPin {
            schematic_path: schematic_path.to_path_buf(),
            at: *at,
        },
        ReducedLocalDriverIdentity::SymbolPin {
            symbol_uuid,
            at,
            pin_number,
        } => ReducedProjectDriverIdentity::SymbolPin {
            schematic_path: schematic_path.to_path_buf(),
            symbol_uuid: symbol_uuid.clone(),
            at: *at,
            pin_number: pin_number.clone(),
        },
    }
}

// Upstream parity: reduced local analogue for the strong-driver collection inside
// `CONNECTION_SUBGRAPH::ResolveDrivers()`. This is not a 1:1 KiCad driver cache because the Rust
// tree still lacks live `SCH_CONNECTION` objects and full subgraph ownership, but it now keeps
// the shared graph's strong-driver names on the same shown-text owner KiCad uses for labels and
// sheet pins instead of leaving sheet-pin drivers on raw parser text, and now also preserves the
// reduced driver kind the shared graph needs for `ercCheckMultipleDrivers()`-style filtering
// instead of collapsing every strong driver to bare names, and now also carries stable reduced
// driver identity plus a reduced connection owner so the later live graph can widen this payload
// into fuller driver-item ownership without keeping driver names as a parallel string-only cache.
// Connected symbol pins and sheet pins are now emitted one projected item at a time instead of
// collapsing each symbol or sheet to one local winner before ranking, which is closer to KiCad's
// per-item `m_drivers` collection, and symbol-pin driver identity now also carries pin number so
// stacked same-position pins do not collapse before live owner attachment. Remaining divergence is
// the still-missing live connection object plus fuller power/bus-parent driver ownership.
fn collect_reduced_strong_drivers<FLabel, FSheet>(
    schematic: &Schematic,
    schematic_path: &std::path::Path,
    sheet_instance_path: &str,
    connected_component: &ConnectionComponent,
    sheet_path_prefix: &str,
    mut shown_label_text: FLabel,
    mut shown_sheet_pin_text: FSheet,
) -> Vec<ReducedProjectStrongDriver>
where
    FLabel: FnMut(&Label) -> String,
    FSheet: FnMut(&crate::model::Sheet, &crate::model::SheetPin) -> String,
{
    let mut drivers = Vec::new();

    for item in &schematic.screen.items {
        match item {
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
                    LabelKind::Directive => continue,
                };
                let full_name = reduced_driver_full_name(&text, source, sheet_path_prefix);

                drivers.push(ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: reduced_label_driver_priority(label),
                    connection: build_reduced_project_driver_connection(
                        schematic,
                        sheet_instance_path,
                        text.clone(),
                        full_name,
                        if label.kind == LabelKind::Global {
                            ""
                        } else {
                            sheet_path_prefix
                        },
                    ),
                    identity: Some(ReducedProjectDriverIdentity::Label {
                        schematic_path: schematic_path.to_path_buf(),
                        at: point_key(label.at),
                        kind: reduced_label_kind_sort_key(label.kind),
                    }),
                });
            }
            SchItem::Sheet(sheet) => {
                for pin in sheet.pins.iter().filter(|pin| {
                    connected_component.members.iter().any(|member| {
                        member.kind == ConnectionMemberKind::SheetPin
                            && points_equal(member.at, pin.at)
                    })
                }) {
                    let shown = shown_sheet_pin_text(sheet, pin);
                    let full_name = reduced_driver_full_name(
                        &shown,
                        ReducedNetNameSource::SheetPin,
                        sheet_path_prefix,
                    );

                    drivers.push(ReducedProjectStrongDriver {
                        kind: ReducedProjectDriverKind::SheetPin,
                        priority: reduced_sheet_pin_driver_rank(pin.shape),
                        connection: build_reduced_project_driver_connection(
                            schematic,
                            sheet_instance_path,
                            shown.clone(),
                            full_name,
                            sheet_path_prefix,
                        ),
                        identity: Some(ReducedProjectDriverIdentity::SheetPin {
                            schematic_path: schematic_path.to_path_buf(),
                            at: point_key(pin.at),
                        }),
                    });
                }
            }
            SchItem::Symbol(symbol) => {
                let unit_pins = projected_symbol_pin_info(symbol);

                for pin in unit_pins.iter().filter(|pin| {
                    connected_component.members.iter().any(|member| {
                        connection_member_matches_projected_symbol_pin(member, symbol, pin)
                    })
                }) {
                    if let Some(priority) =
                        reduced_power_pin_driver_priority(symbol, pin.electrical_type.as_deref())
                    {
                        if let Some(text) = symbol_value_text(symbol) {
                            let local_power = symbol
                                .lib_symbol
                                .as_ref()
                                .is_some_and(|lib_symbol| lib_symbol.local_power);
                            let source = if local_power {
                                ReducedNetNameSource::LocalPowerPin
                            } else {
                                ReducedNetNameSource::GlobalPowerPin
                            };
                            let full_name =
                                reduced_driver_full_name(&text, source, sheet_path_prefix);

                            drivers.push(ReducedProjectStrongDriver {
                                kind: ReducedProjectDriverKind::PowerPin,
                                priority,
                                connection: build_reduced_project_driver_connection(
                                    schematic,
                                    sheet_instance_path,
                                    text.clone(),
                                    full_name,
                                    if local_power { sheet_path_prefix } else { "" },
                                ),
                                identity: Some(ReducedProjectDriverIdentity::SymbolPin {
                                    schematic_path: schematic_path.to_path_buf(),
                                    symbol_uuid: symbol.uuid.clone(),
                                    at: point_key(pin.at),
                                    pin_number: pin.number.clone(),
                                }),
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    drivers.retain(|driver| {
        !reduced_project_strong_driver_name(driver).is_empty()
            && !reduced_project_strong_driver_name(driver).contains("${")
            && !reduced_project_strong_driver_name(driver).starts_with('<')
    });

    drivers.sort_by(|lhs, rhs| {
        rhs.priority.cmp(&lhs.priority).then_with(|| {
            reduced_project_strong_driver_name(lhs).cmp(reduced_project_strong_driver_name(rhs))
        })
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
// - connected symbol pins and sheet pins now contribute one candidate per projected item before
//   ranking instead of collapsing each symbol or sheet to one local winner, which is closer to
//   KiCad's per-item `ResolveDrivers()` candidate ordering, and symbol-pin identity now carries
//   pin number so stacked same-position pins stay distinct through reduced candidate ranking
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
    let mut candidates = Vec::new();

    for item in &schematic.screen.items {
        match item {
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
                    LabelKind::Directive => continue,
                };
                candidates.push(ReducedDriverNameCandidate {
                    priority: reduced_label_driver_priority(label),
                    sheet_pin_rank: 0,
                    text,
                    source,
                    identity: Some(ReducedLocalDriverIdentity::Label {
                        at: point_key(label.at),
                        kind: reduced_label_kind_sort_key(label.kind),
                    }),
                });
            }
            SchItem::Sheet(sheet) => {
                for pin in sheet.pins.iter().filter(|pin| {
                    connected_component.members.iter().any(|member| {
                        member.kind == ConnectionMemberKind::SheetPin
                            && points_equal(member.at, pin.at)
                    })
                }) {
                    candidates.push(ReducedDriverNameCandidate {
                        priority: 0,
                        sheet_pin_rank: reduced_sheet_pin_driver_rank(pin.shape),
                        text: shown_sheet_pin_text(sheet, pin),
                        source: ReducedNetNameSource::SheetPin,
                        identity: Some(ReducedLocalDriverIdentity::SheetPin {
                            at: point_key(pin.at),
                        }),
                    });
                }
            }
            SchItem::Symbol(symbol) => {
                let unit_pins = projected_symbol_pin_info(symbol);

                for pin in unit_pins.iter().filter(|pin| {
                    connected_component.members.iter().any(|member| {
                        connection_member_matches_projected_symbol_pin(member, symbol, pin)
                    })
                }) {
                    let candidate =
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
                                            pin_number: pin.number.clone(),
                                        }),
                                    }
                                })
                            })
                            .or_else(|| {
                                reduced_symbol_pin_default_net_name(symbol, pin, &unit_pins, false)
                                    .map(|text| ReducedDriverNameCandidate {
                                        priority: 1,
                                        sheet_pin_rank: 0,
                                        text,
                                        source: ReducedNetNameSource::SymbolPinDefault,
                                        identity: Some(ReducedLocalDriverIdentity::SymbolPin {
                                            symbol_uuid: symbol.uuid.clone(),
                                            at: point_key(pin.at),
                                            pin_number: pin.number.clone(),
                                        }),
                                    })
                            });

                    if let Some(candidate) = candidate {
                        candidates.push(candidate);
                    }
                }
            }
            _ => {}
        }
    }

    candidates.retain(|candidate| {
        !candidate.text.is_empty()
            && !candidate.text.contains("${")
            && !candidate.text.starts_with('<')
    });

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

#[cfg(test)]
fn reduced_subgraph_is_self_driven_symbol_pin(subgraph: &ReducedProjectSubgraphEntry) -> bool {
    subgraph.drivers.is_empty()
        && subgraph.base_pins.len() == 1
        && subgraph.hier_sheet_pins.is_empty()
}

#[cfg(test)]
fn reduced_subgraph_is_self_driven_sheet_pin(subgraph: &ReducedProjectSubgraphEntry) -> bool {
    subgraph.drivers.is_empty()
        && subgraph.base_pins.is_empty()
        && (!subgraph.hier_sheet_pins.is_empty()
            || subgraph.hier_parent_index.is_some()
            || !subgraph.hier_child_indexes.is_empty())
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
#[cfg(test)]
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
        if reduced_subgraph_is_self_driven_symbol_pin(subgraph) {
            let mut connection = reduced_subgraph_driver_connection(subgraph);

            if connection.name.contains("Net-(") {
                connection.name = reduced_force_no_connect_net_name(&connection.name);
                connection.local_name = reduced_force_no_connect_net_name(&connection.local_name);
                connection.full_local_name =
                    reduced_force_no_connect_net_name(&connection.full_local_name);
                updates.push(PostPropagationUpdate::ForceNoConnectName { index, connection });
            }
        }

        if reduced_subgraph_is_self_driven_sheet_pin(subgraph)
            && subgraph
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
        LiveReducedConnection, LiveReducedSubgraph, LiveReducedSubgraphHandle, PointKey,
        ReducedBusMember, ReducedBusMemberKind, ReducedHierPortLink, ReducedHierSheetPinLink,
        ReducedLabelLink, ReducedProjectBusNeighborLink, ReducedProjectConnection,
        ReducedProjectConnectionType, ReducedProjectDriverKind, ReducedProjectStrongDriver,
        ReducedProjectSubgraphEntry, ReducedSubgraphWireItem,
        apply_live_reduced_driver_connections_from_handles,
        build_live_reduced_name_caches_from_handles, build_live_reduced_subgraph_handles,
        clone_reduced_connection_into_live_connection_owner,
        clone_reduced_connection_into_subgraph,
        collect_live_reduced_propagation_component_from_handles,
        find_first_reduced_project_subgraph_by_name, find_reduced_project_subgraph_by_name,
        rebuild_reduced_project_graph_name_caches, recache_live_reduced_subgraph_name_from_handles,
        recache_live_reduced_subgraph_name_handle_cache_from_handles, reduced_bus_member_objects,
        refresh_reduced_bus_link_members, refresh_reduced_bus_members_from_neighbor_connections,
        refresh_reduced_global_secondary_driver_promotions,
        refresh_reduced_hierarchy_driver_chains, refresh_reduced_live_bus_link_members,
        refresh_reduced_live_bus_neighbor_drivers,
        refresh_reduced_live_bus_neighbor_drivers_on_handles_for_indexes,
        refresh_reduced_live_bus_parent_members, refresh_reduced_live_graph_propagation,
        refresh_reduced_live_multiple_bus_parent_names,
        refresh_reduced_live_post_propagation_item_connections,
        refresh_reduced_multiple_bus_parent_names,
        refresh_reduced_post_propagation_item_connections,
        replay_reduced_live_stale_bus_members_on_handles_for_indexes, resolve_reduced_net_name_at,
        resolve_reduced_project_net_at, resolve_reduced_project_subgraph_at,
        resolve_reduced_project_subgraph_for_label,
        resolve_reduced_project_subgraph_for_no_connect,
        resolve_reduced_project_subgraph_for_symbol_pin,
    };
    use crate::core::SchematicProject;
    use crate::loader::load_schematic_tree;
    use crate::model::{LabelKind, SchItem};
    use crate::parser::parse_schematic_file;
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::env;
    use std::fs;
    use std::rc::{Rc, Weak};
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
    fn live_reduced_connection_clone_preserves_existing_bus_member_identity() {
        let target = LiveReducedConnection::new(ReducedProjectConnection {
            net_code: 0,
            connection_type: ReducedProjectConnectionType::Bus,
            name: "/OLD".to_string(),
            local_name: "OLD".to_string(),
            full_local_name: "/OLD".to_string(),
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
        });
        let source = LiveReducedConnection::new(ReducedProjectConnection {
            net_code: 0,
            connection_type: ReducedProjectConnectionType::Bus,
            name: "/RENAMED".to_string(),
            local_name: "RENAMED".to_string(),
            full_local_name: "/RENAMED".to_string(),
            sheet_instance_path: "/child".to_string(),
            members: vec![ReducedBusMember {
                net_code: 7,
                name: "RENAMED1".to_string(),
                local_name: "RENAMED1".to_string(),
                full_local_name: "/RENAMED1".to_string(),
                vector_index: None,
                kind: ReducedBusMemberKind::Net,
                members: Vec::new(),
            }],
        });

        target.clone_from(&source);

        let target_connection = target.snapshot();
        assert_eq!(target_connection.name, "/RENAMED");
        assert_eq!(target_connection.local_name, "OLD");
        assert_eq!(target_connection.sheet_instance_path, "/child");
        assert_eq!(target_connection.members[0].name, "RENAMED1");
        assert_eq!(target_connection.members[0].local_name, "OLD1");
        assert_eq!(target_connection.members[0].vector_index, Some(1));
    }

    #[test]
    fn clone_reduced_connection_into_subgraph_preserves_item_bus_member_identity() {
        let mut subgraph = ReducedProjectSubgraphEntry {
            subgraph_code: 1,
            code: 1,
            name: "/OLD".to_string(),
            resolved_connection: ReducedProjectConnection {
                net_code: 0,
                connection_type: ReducedProjectConnectionType::Bus,
                name: "/OLD".to_string(),
                local_name: "OLD".to_string(),
                full_local_name: "/OLD".to_string(),
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
            driver_connection: None,
            chosen_driver_identity: None,
            drivers: Vec::new(),
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
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/OLD".to_string(),
                    local_name: "OLD".to_string(),
                    full_local_name: "/OLD".to_string(),
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
        };

        let source = ReducedProjectConnection {
            net_code: 0,
            connection_type: ReducedProjectConnectionType::Bus,
            name: "/RENAMED".to_string(),
            local_name: "RENAMED".to_string(),
            full_local_name: "/RENAMED".to_string(),
            sheet_instance_path: "/child".to_string(),
            members: vec![ReducedBusMember {
                net_code: 7,
                name: "RENAMED1".to_string(),
                local_name: "RENAMED1".to_string(),
                full_local_name: "/RENAMED1".to_string(),
                vector_index: None,
                kind: ReducedBusMemberKind::Net,
                members: Vec::new(),
            }],
        };

        clone_reduced_connection_into_subgraph(&mut subgraph, &source);

        assert_eq!(subgraph.resolved_connection.name, "/RENAMED");
        assert_eq!(subgraph.resolved_connection.local_name, "OLD");
        assert_eq!(
            subgraph.resolved_connection.members[0].vector_index,
            Some(1)
        );
        assert_eq!(subgraph.label_links[0].connection.name, "/RENAMED");
        assert_eq!(subgraph.label_links[0].connection.local_name, "OLD");
        assert_eq!(
            subgraph.label_links[0].connection.members[0].local_name,
            "OLD1"
        );
        assert_eq!(
            subgraph.label_links[0].connection.members[0].vector_index,
            Some(1)
        );
    }

    #[test]
    fn handle_projection_resolves_bus_entry_owner_from_live_handle() {
        let mut reduced = vec![
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
                    members: Vec::new(),
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS".to_string(),
                    local_name: "BUS".to_string(),
                    full_local_name: "/BUS".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                }),
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                bus_items: vec![ReducedSubgraphWireItem {
                    start: PointKey(0, 0),
                    end: PointKey(10, 0),
                    is_bus_entry: false,
                    connected_bus_subgraph_index: None,
                }],
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
                name: "/ENTRY".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/ENTRY".to_string(),
                    local_name: "ENTRY".to_string(),
                    full_local_name: "/ENTRY".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/ENTRY".to_string(),
                    local_name: "ENTRY".to_string(),
                    full_local_name: "/ENTRY".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                }),
                chosen_driver_identity: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(5, 5),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: vec![ReducedSubgraphWireItem {
                    start: PointKey(0, 0),
                    end: PointKey(5, 5),
                    is_bus_entry: true,
                    connected_bus_subgraph_index: Some(999),
                }],
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
        ];

        let handles = build_live_reduced_subgraph_handles(&reduced);
        handles[1].borrow_mut().wire_items[0]
            .borrow_mut()
            .connected_bus_item_handle = Some(Rc::downgrade(&handles[0].borrow().bus_items[0]));

        apply_live_reduced_driver_connections_from_handles(&mut reduced, &handles);

        assert_eq!(
            reduced[1].wire_items[0].connected_bus_subgraph_index,
            Some(0)
        );
    }

    #[test]
    fn handle_projection_resolves_bus_parent_indexes_from_live_handles() {
        let mut reduced = vec![
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
                        name: "SIG0".to_string(),
                        local_name: "SIG0".to_string(),
                        full_local_name: "/SIG0".to_string(),
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
                    sheet_instance_path: String::new(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "SIG0".to_string(),
                        local_name: "SIG0".to_string(),
                        full_local_name: "/SIG0".to_string(),
                        vector_index: Some(0),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                }),
                chosen_driver_identity: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: vec![ReducedSubgraphWireItem {
                    start: PointKey(0, 0),
                    end: PointKey(10, 0),
                    is_bus_entry: false,
                    connected_bus_subgraph_index: None,
                }],
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
                base_pins: Vec::new(),
            },
            ReducedProjectSubgraphEntry {
                subgraph_code: 2,
                code: 2,
                name: "/SIG0".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/SIG0".to_string(),
                    local_name: "SIG0".to_string(),
                    full_local_name: "/SIG0".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/SIG0".to_string(),
                    local_name: "SIG0".to_string(),
                    full_local_name: "/SIG0".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                }),
                chosen_driver_identity: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(1, 1),
                points: Vec::new(),
                nodes: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: vec![0],
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
                base_pins: Vec::new(),
            },
        ];

        let handles = build_live_reduced_subgraph_handles(&reduced);
        handles[1].borrow_mut().bus_parent_indexes.clear();

        apply_live_reduced_driver_connections_from_handles(&mut reduced, &handles);

        assert_eq!(reduced[1].bus_parent_indexes, vec![0]);
    }

    #[test]
    fn handle_projection_resolves_hierarchy_indexes_from_live_handles() {
        let mut reduced = vec![
            ReducedProjectSubgraphEntry {
                subgraph_code: 1,
                code: 1,
                name: "/TOP".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/TOP".to_string(),
                    local_name: "TOP".to_string(),
                    full_local_name: "/TOP".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/TOP".to_string(),
                    local_name: "TOP".to_string(),
                    full_local_name: "/TOP".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                }),
                chosen_driver_identity: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: vec![ReducedHierPortLink {
                    at: PointKey(0, 0),
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "/TOP".to_string(),
                        local_name: "TOP".to_string(),
                        full_local_name: "/TOP".to_string(),
                        sheet_instance_path: String::new(),
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
                hier_child_indexes: vec![1],
                base_pins: Vec::new(),
            },
            ReducedProjectSubgraphEntry {
                subgraph_code: 2,
                code: 2,
                name: "/CHILD".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/CHILD".to_string(),
                    local_name: "CHILD".to_string(),
                    full_local_name: "/CHILD".to_string(),
                    sheet_instance_path: "/child".to_string(),
                    members: Vec::new(),
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/CHILD".to_string(),
                    local_name: "CHILD".to_string(),
                    full_local_name: "/CHILD".to_string(),
                    sheet_instance_path: "/child".to_string(),
                    members: Vec::new(),
                }),
                chosen_driver_identity: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: "/child".to_string(),
                anchor: PointKey(1, 1),
                points: Vec::new(),
                nodes: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: vec![ReducedHierSheetPinLink {
                    at: PointKey(1, 1),
                    child_sheet_uuid: None,
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "/CHILD".to_string(),
                        local_name: "CHILD".to_string(),
                        full_local_name: "/CHILD".to_string(),
                        sheet_instance_path: "/child".to_string(),
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
                hier_parent_index: Some(0),
                hier_child_indexes: Vec::new(),
                base_pins: Vec::new(),
            },
        ];

        let handles = build_live_reduced_subgraph_handles(&reduced);
        {
            let mut parent = handles[0].borrow_mut();
            parent.hier_child_indexes.clear();
        }
        {
            let mut child = handles[1].borrow_mut();
            child.hier_parent_index = None;
        }

        apply_live_reduced_driver_connections_from_handles(&mut reduced, &handles);

        assert_eq!(reduced[0].hier_child_indexes, vec![1]);
        assert_eq!(reduced[1].hier_parent_index, Some(0));
    }

    #[test]
    fn apply_live_reduced_driver_connections_preserves_live_link_connections() {
        let mut reduced = vec![ReducedProjectSubgraphEntry {
            subgraph_code: 1,
            code: 1,
            name: "/OLD".to_string(),
            resolved_connection: ReducedProjectConnection {
                net_code: 0,
                connection_type: ReducedProjectConnectionType::Bus,
                name: "/OLD".to_string(),
                local_name: "OLD".to_string(),
                full_local_name: "/OLD".to_string(),
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
                name: "/OLD".to_string(),
                local_name: "OLD".to_string(),
                full_local_name: "/OLD".to_string(),
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
            chosen_driver_identity: None,
            drivers: Vec::new(),
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
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/OLD".to_string(),
                    local_name: "OLD".to_string(),
                    full_local_name: "/OLD".to_string(),
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
        }];

        let live = build_live_reduced_subgraph_handles(&reduced);
        {
            let mut subgraph = live[0].borrow_mut();
            subgraph.driver_connection = LiveReducedConnection::new(ReducedProjectConnection {
                net_code: 0,
                connection_type: ReducedProjectConnectionType::Bus,
                name: "/RENAMED".to_string(),
                local_name: "RENAMED".to_string(),
                full_local_name: "/RENAMED".to_string(),
                sheet_instance_path: "/child".to_string(),
                members: vec![ReducedBusMember {
                    net_code: 0,
                    name: "RENAMED1".to_string(),
                    local_name: "RENAMED1".to_string(),
                    full_local_name: "/RENAMED1".to_string(),
                    vector_index: None,
                    kind: ReducedBusMemberKind::Net,
                    members: Vec::new(),
                }],
            });
            subgraph.label_links[0].borrow_mut().connection =
                LiveReducedConnection::new(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/LABEL".to_string(),
                    local_name: "LABEL".to_string(),
                    full_local_name: "/LABEL".to_string(),
                    sheet_instance_path: "/child".to_string(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "LABEL1".to_string(),
                        local_name: "LABEL1".to_string(),
                        full_local_name: "/LABEL1".to_string(),
                        vector_index: None,
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                });
        }

        apply_live_reduced_driver_connections_from_handles(&mut reduced, &live);

        assert_eq!(reduced[0].name, "/RENAMED");
        assert_eq!(reduced[0].resolved_connection.name, "/RENAMED");
        assert_eq!(reduced[0].label_links[0].connection.name, "/LABEL");
        assert_eq!(reduced[0].label_links[0].connection.local_name, "OLD");
        assert_eq!(
            reduced[0].label_links[0].connection.members[0].local_name,
            "OLD1"
        );
        assert_eq!(
            reduced[0].label_links[0].connection.members[0].vector_index,
            Some(1)
        );
    }

    #[test]
    fn live_bus_entry_connected_bus_owner_tracks_in_place_bus_updates() {
        let reduced = vec![
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                bus_items: vec![ReducedSubgraphWireItem {
                    start: PointKey(0, 0),
                    end: PointKey(10, 0),
                    is_bus_entry: false,
                    connected_bus_subgraph_index: None,
                }],
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(5, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: vec![ReducedSubgraphWireItem {
                    start: PointKey(5, 0),
                    end: PointKey(6, 1),
                    is_bus_entry: true,
                    connected_bus_subgraph_index: Some(0),
                }],
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

        let live = build_live_reduced_subgraph_handles(&reduced);
        refresh_reduced_live_bus_neighbor_drivers_on_handles_for_indexes(
            &live,
            &[0],
            &mut Vec::new(),
        );

        let live_bus_entry = live[1].borrow();
        let connected_bus_item = live_bus_entry.wire_items[0]
            .borrow()
            .connected_bus_item_handle
            .as_ref()
            .and_then(Weak::upgrade)
            .expect("connected bus item owner");
        let connected_bus_connection = connected_bus_item
            .borrow()
            .connection
            .clone()
            .expect("connected bus item connection");
        assert_eq!(
            connected_bus_connection.borrow().members[0]
                .borrow()
                .full_local_name,
            "/OLD1"
        );

        clone_reduced_connection_into_live_connection_owner(
            &mut live[0].borrow().driver_connection.borrow_mut(),
            &ReducedProjectConnection {
                net_code: 0,
                connection_type: ReducedProjectConnectionType::Bus,
                name: "/BUS".to_string(),
                local_name: "BUS".to_string(),
                full_local_name: "/BUS".to_string(),
                sheet_instance_path: String::new(),
                members: vec![ReducedBusMember {
                    net_code: 0,
                    name: "PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "/PWR".to_string(),
                    vector_index: Some(1),
                    kind: ReducedBusMemberKind::Net,
                    members: Vec::new(),
                }],
            },
        );

        assert_eq!(
            connected_bus_connection.borrow().members[0]
                .borrow()
                .full_local_name,
            "/PWR"
        );
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
            Some("1"),
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
        assert!(by_point.drivers.iter().any(|driver| {
            super::reduced_project_strong_driver_name(driver) == "SIG"
                && matches!(
                    driver.identity,
                    Some(super::ReducedProjectDriverIdentity::SheetPin { .. })
                )
        }));

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
        assert!(by_point.drivers.iter().any(|driver| {
            super::reduced_project_strong_driver_name(driver) == child_sheet.instance_path
        }));

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
            Some("1"),
        )
        .expect("gnd pin graph identity");
        let agnd_pin = crate::connectivity::resolve_reduced_project_net_for_symbol_pin(
            &graph,
            &sheet_path,
            symbol,
            [10.0, 0.0],
            Some("AGND"),
            Some("2"),
        )
        .expect("agnd pin graph identity");

        assert_eq!(gnd_pin.name, "VCC");
        assert_eq!(agnd_pin.name, "VCC");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reduced_project_driver_name_for_power_pin_uses_pin_owned_connection() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_power_pin_driver_name_{}.kicad_sch",
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
    (symbol "power:VCC"
      (power)
      (property "Reference" "#PWR" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "VCC" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "VCC_1_1"
        (pin power_in line (at 0 0 180) (length 2.54)
          (name "VCC" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "power:VCC")
    (uuid "73050000-0000-0000-0000-000000000301")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "#PWR1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "VCC" (at 0 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy -10 0)))
  (global_label "SIG" (shape input) (at -10 0 0) (effects (font (size 1 1)))))"##,
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

        let driver_name = crate::connectivity::resolve_reduced_project_driver_name_for_symbol_pin(
            &graph,
            &sheet_path,
            symbol,
            [0.0, 0.0],
            Some("VCC"),
            Some("1"),
        )
        .expect("driver name");
        let net_name = crate::connectivity::resolve_reduced_project_net_for_symbol_pin(
            &graph,
            &sheet_path,
            symbol,
            [0.0, 0.0],
            Some("VCC"),
            Some("1"),
        )
        .expect("net identity");

        assert_eq!(driver_name, "VCC");
        assert_eq!(net_name.name, "SIG");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reduced_project_driver_name_by_location_distinguishes_stacked_pin_numbers() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_stacked_pin_location_driver_{}.kicad_sch",
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
    (symbol "device:Stacked"
      (property "Reference" "U" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "Stacked" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "Stacked_1_1"
        (pin input line (at 0 0 180) (length 2.54)
          (name "PWR" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1)))))
        (pin input line (at 0 0 180) (length 2.54)
          (name "PWR" (effects (font (size 1 1))))
          (number "2" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "device:Stacked")
    (uuid "73050000-0000-0000-0000-000000000302")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "U1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "Stacked" (at 0 0 0) (effects (font (size 1 1))))))"##,
        )
        .expect("write schematic");

        let loaded = load_schematic_tree(&path).expect("load tree");
        let sheet_path = loaded
            .sheet_paths
            .iter()
            .find(|sheet_path| sheet_path.instance_path.is_empty())
            .cloned()
            .expect("root sheet path");
        let schematic = loaded
            .schematics
            .iter()
            .find(|schematic| schematic.path == sheet_path.schematic_path)
            .expect("root schematic");
        let symbol = schematic
            .screen
            .items
            .iter()
            .find_map(|item| match item {
                SchItem::Symbol(symbol) => Some(symbol),
                _ => None,
            })
            .expect("symbol");
        let graph = super::ReducedProjectNetGraph {
            subgraphs: Vec::new(),
            subgraphs_by_name: BTreeMap::new(),
            subgraphs_by_sheet_and_name: BTreeMap::new(),
            pin_subgraph_identities: BTreeMap::new(),
            pin_subgraph_identities_by_location: BTreeMap::new(),
            pin_driver_connections: BTreeMap::new(),
            pin_driver_connections_by_location: BTreeMap::from([
                (
                    super::ReducedProjectPinIdentityKey {
                        sheet_instance_path: sheet_path.instance_path.clone(),
                        symbol_uuid: symbol.uuid.clone(),
                        at: super::point_key([0.0, 0.0]),
                        number: Some("1".to_string()),
                    },
                    super::ReducedProjectConnection {
                        net_code: 0,
                        connection_type: super::ReducedProjectConnectionType::Net,
                        name: "VCC".to_string(),
                        local_name: "VCC".to_string(),
                        full_local_name: "VCC".to_string(),
                        sheet_instance_path: sheet_path.instance_path.clone(),
                        members: Vec::new(),
                    },
                ),
                (
                    super::ReducedProjectPinIdentityKey {
                        sheet_instance_path: sheet_path.instance_path.clone(),
                        symbol_uuid: symbol.uuid.clone(),
                        at: super::point_key([0.0, 0.0]),
                        number: Some("2".to_string()),
                    },
                    super::ReducedProjectConnection {
                        net_code: 0,
                        connection_type: super::ReducedProjectConnectionType::Net,
                        name: "GND".to_string(),
                        local_name: "GND".to_string(),
                        full_local_name: "GND".to_string(),
                        sheet_instance_path: sheet_path.instance_path.clone(),
                        members: Vec::new(),
                    },
                ),
            ]),
            point_subgraph_identities: BTreeMap::new(),
            label_subgraph_identities: BTreeMap::new(),
            no_connect_subgraph_identities: BTreeMap::new(),
        };

        assert_eq!(
            super::resolve_reduced_project_driver_name_for_symbol_pin(
                &graph,
                &sheet_path,
                symbol,
                [0.0, 0.0],
                None,
                Some("1"),
            )
            .as_deref(),
            Some("VCC")
        );
        assert_eq!(
            super::resolve_reduced_project_driver_name_for_symbol_pin(
                &graph,
                &sheet_path,
                symbol,
                [0.0, 0.0],
                None,
                Some("2"),
            )
            .as_deref(),
            Some("GND")
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reduced_project_driver_name_for_symbol_pin_prefers_resolved_driver_over_seeded_default() {
        let mut symbol = crate::model::Symbol::new();
        symbol.uuid = Some("73050000-0000-0000-0000-000000000901".to_string());

        let sheet_path = crate::loader::LoadedSheetPath {
            instance_path: String::new(),
            schematic_path: std::path::PathBuf::from("root.kicad_sch"),
            symbol_path: String::new(),
            sheet_uuid: Some("root-sheet".to_string()),
            sheet_name: None,
            page: None,
            sheet_number: 1,
            sheet_count: 1,
        };
        let graph = super::ReducedProjectNetGraph {
            subgraphs: vec![super::ReducedProjectSubgraphEntry {
                subgraph_code: 1,
                code: 1,
                name: "SIG".to_string(),
                resolved_connection: super::ReducedProjectConnection {
                    net_code: 1,
                    connection_type: super::ReducedProjectConnectionType::Net,
                    name: "SIG".to_string(),
                    local_name: "SIG".to_string(),
                    full_local_name: "SIG".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                driver_connection: Some(super::ReducedProjectConnection {
                    net_code: 1,
                    connection_type: super::ReducedProjectConnectionType::Net,
                    name: "SIG".to_string(),
                    local_name: "SIG".to_string(),
                    full_local_name: "SIG".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                }),
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
            }],
            subgraphs_by_name: BTreeMap::new(),
            subgraphs_by_sheet_and_name: BTreeMap::new(),
            pin_subgraph_identities: BTreeMap::new(),
            pin_subgraph_identities_by_location: BTreeMap::from([(
                super::ReducedProjectPinIdentityKey {
                    sheet_instance_path: String::new(),
                    symbol_uuid: symbol.uuid.clone(),
                    at: PointKey(0, 0),
                    number: Some("1".to_string()),
                },
                0,
            )]),
            pin_driver_connections: BTreeMap::new(),
            pin_driver_connections_by_location: BTreeMap::from([(
                super::ReducedProjectPinIdentityKey {
                    sheet_instance_path: String::new(),
                    symbol_uuid: symbol.uuid.clone(),
                    at: PointKey(0, 0),
                    number: Some("1".to_string()),
                },
                super::ReducedProjectConnection {
                    net_code: 1,
                    connection_type: super::ReducedProjectConnectionType::Net,
                    name: "Net-(U1-Pad1)".to_string(),
                    local_name: "Net-(U1-Pad1)".to_string(),
                    full_local_name: "Net-(U1-Pad1)".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
            )]),
            point_subgraph_identities: BTreeMap::new(),
            label_subgraph_identities: BTreeMap::new(),
            no_connect_subgraph_identities: BTreeMap::new(),
        };

        assert_eq!(
            super::resolve_reduced_project_driver_name_for_symbol_pin(
                &graph,
                &sheet_path,
                &symbol,
                [0.0, 0.0],
                None,
                Some("1"),
            )
            .as_deref(),
            Some("SIG")
        );
    }

    #[test]
    fn reduced_symbol_pin_default_net_name_uses_effective_stacked_pad_number() {
        let mut symbol = crate::model::Symbol::new();
        symbol.uuid = Some("73050000-0000-0000-0000-000000000777".to_string());
        symbol.set_field_text(
            crate::model::PropertyKind::SymbolReference,
            "U1".to_string(),
        );

        let pin = super::ProjectedSymbolPin {
            at: [0.0, 0.0],
            name: Some("A".to_string()),
            number: Some("[2-3]".to_string()),
            electrical_type: Some("input".to_string()),
        };
        let unit_pins = vec![
            pin.clone(),
            super::ProjectedSymbolPin {
                at: [10.0, 0.0],
                name: Some("A".to_string()),
                number: Some("[4-5]".to_string()),
                electrical_type: Some("input".to_string()),
            },
        ];

        let name = super::reduced_symbol_pin_default_net_name(&symbol, &pin, &unit_pins, false)
            .expect("default net name");

        assert_eq!(name, "Net-(U1-A-Pad2)");
    }

    #[test]
    fn reduced_seeded_symbol_pin_connection_uses_default_net_name_for_ordinary_pin() {
        let mut symbol = crate::model::Symbol::new();
        symbol.uuid = Some("73050000-0000-0000-0000-000000000778".to_string());
        symbol.set_field_text(
            crate::model::PropertyKind::SymbolReference,
            "U1".to_string(),
        );

        let pin = super::ProjectedSymbolPin {
            at: [0.0, 0.0],
            name: Some("A".to_string()),
            number: Some("1".to_string()),
            electrical_type: Some("input".to_string()),
        };
        let unit_pins = vec![pin.clone()];

        let connection = super::reduced_seeded_symbol_pin_connection(&symbol, &pin, &unit_pins, "");

        assert_eq!(
            connection.connection_type,
            super::ReducedProjectConnectionType::Net
        );
        assert_eq!(connection.name, "Net-(U1-A)");
        assert_eq!(connection.local_name, "Net-(U1-A)");
        assert_eq!(connection.full_local_name, "Net-(U1-A)");
    }

    #[test]
    fn reduced_driver_name_candidate_ranks_connected_symbol_pins_individually() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_symbol_pin_driver_candidates_{}.kicad_sch",
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
    (symbol "device:TwoPin"
      (property "Reference" "U" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "TwoPin" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "TwoPin_1_1"
        (pin input line (at 0 0 180) (length 2.54)
          (name "Z" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1)))))
        (pin input line (at 10 0 0) (length 2.54)
          (name "A" (effects (font (size 1 1))))
          (number "2" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "device:TwoPin")
    (uuid "73050000-0000-0000-0000-000000000401")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "U1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "TwoPin" (at 0 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy 10 0))))"##,
        )
        .expect("write schematic");

        let loaded = load_schematic_tree(&path).expect("load tree");
        let sheet_path = loaded
            .sheet_paths
            .iter()
            .find(|sheet_path| sheet_path.instance_path.is_empty())
            .cloned()
            .expect("root sheet path");
        let schematic = loaded
            .schematics
            .iter()
            .find(|schematic| schematic.path == sheet_path.schematic_path)
            .expect("root schematic");
        let symbol = schematic
            .screen
            .items
            .iter()
            .find_map(|item| match item {
                SchItem::Symbol(symbol) => Some(symbol),
                _ => None,
            })
            .expect("symbol");
        let component = super::connection_component_for_symbol_pin(schematic, symbol, [0.0, 0.0])
            .expect("component");

        let candidate = super::resolve_reduced_driver_name_candidate_on_component(
            schematic,
            &component,
            |label| label.text.clone(),
            |_sheet, pin| pin.name.clone(),
        )
        .expect("driver candidate");

        assert_eq!(candidate.text, "Net-(U1-A)");
        assert_eq!(
            candidate.identity,
            Some(super::ReducedLocalDriverIdentity::SymbolPin {
                symbol_uuid: symbol.uuid.clone(),
                at: super::point_key([10.0, 0.0]),
                pin_number: Some("2".to_string()),
            })
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reduced_driver_name_candidate_ranks_connected_sheet_pins_individually() {
        let root_path = env::temp_dir().join(format!(
            "ki2_connectivity_sheet_pin_driver_candidates_{}.kicad_sch",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let child_path = env::temp_dir().join(format!(
            "ki2_connectivity_sheet_pin_driver_candidates_child_{}.kicad_sch",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));

        fs::write(
            &root_path,
            format!(
                r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4")
  (wire (pts (xy 0 5) (xy 20 5)))
  (sheet (at 0 0) (size 20 10)
    (uuid "73050000-0000-0000-0000-000000000501")
    (property "Sheetname" "Child" (id 0) (at 0 0 0) (effects (font (size 1 1))))
    (property "Sheetfile" "{}" (id 1) (at 0 0 0) (effects (font (size 1 1))))
    (pin "Z" input (at 0 5 180) (uuid "73050000-0000-0000-0000-000000000502"))
    (pin "A" input (at 20 5 0) (uuid "73050000-0000-0000-0000-000000000503"))))"#,
                child_path.display()
            ),
        )
        .expect("write root schematic");
        fs::write(
            &child_path,
            r#"(kicad_sch
  (version 20260306)
  (generator "ki2")
  (paper "A4"))"#,
        )
        .expect("write child schematic");

        let loaded = load_schematic_tree(&root_path).expect("load tree");
        let sheet_path = loaded
            .sheet_paths
            .iter()
            .find(|sheet_path| sheet_path.instance_path.is_empty())
            .cloned()
            .expect("root sheet path");
        let schematic = loaded
            .schematics
            .iter()
            .find(|schematic| schematic.path == sheet_path.schematic_path)
            .expect("root schematic");
        let component = super::connection_component_at(schematic, [20.0, 5.0]).expect("component");
        let candidate = super::resolve_reduced_driver_name_candidate_on_component(
            schematic,
            &component,
            |label| label.text.clone(),
            |_sheet, pin| pin.name.clone(),
        )
        .expect("driver candidate");

        assert_eq!(candidate.text, "A");
        assert_eq!(
            candidate.identity,
            Some(super::ReducedLocalDriverIdentity::SheetPin {
                at: super::point_key([20.0, 5.0]),
            })
        );

        let _ = fs::remove_file(&root_path);
        let _ = fs::remove_file(&child_path);
    }

    #[test]
    fn collect_connection_components_links_duplicate_jumper_pin_numbers() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_duplicate_jumper_pin_numbers_{}.kicad_sch",
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
  (lib_symbols
    (symbol "Device:JumperDup"
      (duplicate_pin_numbers_are_jumpers yes)
      (property "Reference" "JP" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "JumperDup" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "JumperDup_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "A" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1)))))
        (pin passive line (at 10 0 0) (length 2.54)
          (name "B" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:JumperDup")
    (uuid "73050000-0000-0000-0000-000000000601")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "JP1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "JumperDup" (at 0 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy -10 0)))
  (global_label "NET_A" (shape input) (at -10 0 0) (effects (font (size 1 1))))
  (wire (pts (xy 10 0) (xy 20 0)))
  (global_label "NET_B" (shape input) (at 20 0 0) (effects (font (size 1 1)))))"#,
        )
        .expect("write schematic");

        let schematic = crate::parser::parse_schematic_file(&path).expect("parse schematic");
        let components = super::collect_connection_components(&schematic);

        assert_eq!(components.len(), 1);
        assert_eq!(
            components[0]
                .members
                .iter()
                .filter(|member| member.kind == super::ConnectionMemberKind::SymbolPin)
                .count(),
            2
        );
        assert!(
            components[0]
                .members
                .iter()
                .any(|member| member.kind == super::ConnectionMemberKind::Label
                    && crate::loader::points_equal(member.at, [-10.0, 0.0]))
        );
        assert!(
            components[0]
                .members
                .iter()
                .any(|member| member.kind == super::ConnectionMemberKind::Label
                    && crate::loader::points_equal(member.at, [20.0, 0.0]))
        );

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn collect_connection_components_links_jumper_pin_groups() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_jumper_pin_groups_{}.kicad_sch",
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
  (lib_symbols
    (symbol "Device:JumperGroup"
      (jumper_pin_groups ("1" "2"))
      (property "Reference" "JP" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "JumperGroup" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "JumperGroup_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "A" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1)))))
        (pin passive line (at 10 0 0) (length 2.54)
          (name "B" (effects (font (size 1 1))))
          (number "2" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:JumperGroup")
    (uuid "73050000-0000-0000-0000-000000000602")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "JP1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "JumperGroup" (at 0 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy -10 0)))
  (global_label "NET_A" (shape input) (at -10 0 0) (effects (font (size 1 1))))
  (wire (pts (xy 10 0) (xy 20 0)))
  (global_label "NET_B" (shape input) (at 20 0 0) (effects (font (size 1 1)))))"#,
        )
        .expect("write schematic");

        let schematic = crate::parser::parse_schematic_file(&path).expect("parse schematic");
        let components = super::collect_connection_components(&schematic);

        assert_eq!(components.len(), 1);
        assert_eq!(
            components[0]
                .members
                .iter()
                .filter(|member| member.kind == super::ConnectionMemberKind::SymbolPin)
                .count(),
            2
        );
        assert!(
            components[0]
                .members
                .iter()
                .any(|member| member.kind == super::ConnectionMemberKind::Label
                    && crate::loader::points_equal(member.at, [-10.0, 0.0]))
        );
        assert!(
            components[0]
                .members
                .iter()
                .any(|member| member.kind == super::ConnectionMemberKind::Label
                    && crate::loader::points_equal(member.at, [20.0, 0.0]))
        );

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn collect_connection_points_keeps_stacked_symbol_pins_distinct() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_stacked_symbol_pins_{}.kicad_sch",
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
  (lib_symbols
    (symbol "Device:Stacked"
      (property "Reference" "U" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "Stacked" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "Stacked_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "A" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1)))))
        (pin passive line (at 0 0 180) (length 2.54)
          (name "B" (effects (font (size 1 1))))
          (number "2" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "Device:Stacked")
    (uuid "73050000-0000-0000-0000-000000000603")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "U1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "Stacked" (at 0 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy -10 0)))
  (global_label "NET" (shape input) (at -10 0 0) (effects (font (size 1 1)))))"#,
        )
        .expect("write schematic");

        let schematic = crate::parser::parse_schematic_file(&path).expect("parse schematic");
        let points = super::collect_connection_points(&schematic);
        let point = points
            .get(&super::point_key([0.0, 0.0]))
            .expect("stacked pin point");

        assert_eq!(
            point
                .members
                .iter()
                .filter(|member| member.kind == super::ConnectionMemberKind::SymbolPin)
                .count(),
            2
        );
        assert!(point.members.iter().any(|member| {
            member.kind == super::ConnectionMemberKind::SymbolPin
                && member.pin_number.as_deref() == Some("1")
        }));
        assert!(point.members.iter().any(|member| {
            member.kind == super::ConnectionMemberKind::SymbolPin
                && member.pin_number.as_deref() == Some("2")
        }));

        let _ = fs::remove_file(&path);
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
            chosen_driver_identity: None,
            drivers: Vec::new(),
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
            chosen_driver_identity: None,
            drivers: Vec::new(),
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
            chosen_driver_identity: None,
            drivers: Vec::new(),
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
            chosen_driver_identity: None,
            drivers: Vec::new(),
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                chosen_driver_identity: None,
                drivers: vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: 6,
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "/PWR".to_string(),
                        local_name: "PWR".to_string(),
                        full_local_name: "/PWR".to_string(),
                        sheet_instance_path: String::new(),
                        members: Vec::new(),
                    },
                    identity: None,
                }],
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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

        refresh_reduced_live_graph_propagation(&mut graph);

        assert_eq!(
            graph[0].resolved_connection.members[0].full_local_name,
            "/PWR"
        );
        assert_eq!(graph[2].resolved_connection.full_local_name, "/PWR");
    }

    #[test]
    fn recache_live_reduced_subgraph_name_updates_live_name_indexes() {
        let live_subgraphs = vec![
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
                drivers: Vec::new(),
                chosen_driver: None,
                sheet_instance_path: String::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                bus_parent_handles: Vec::new(),
                base_pins: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
                hier_parent_handle: None,
                hier_child_handles: Vec::new(),
                label_links: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
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
                drivers: Vec::new(),
                chosen_driver: None,
                sheet_instance_path: "/child".to_string(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                bus_parent_handles: Vec::new(),
                base_pins: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
                hier_parent_handle: None,
                hier_child_handles: Vec::new(),
                label_links: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                dirty: true,
            },
        ];

        let handles = live_subgraphs
            .into_iter()
            .map(|subgraph| Rc::new(RefCell::new(subgraph)))
            .collect::<Vec<_>>();
        let (mut by_name, mut by_sheet_and_name) =
            build_live_reduced_name_caches_from_handles(&handles);

        {
            let subgraph = handles[0].borrow();
            let mut connection = subgraph.driver_connection.borrow_mut();
            connection.name = "/NEW".to_string();
            connection.local_name = "NEW".to_string();
            connection.full_local_name = "/NEW".to_string();
        }

        recache_live_reduced_subgraph_name_from_handles(
            &handles,
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
    fn recache_live_reduced_subgraph_name_handle_cache_keeps_shared_identity() {
        let live_subgraphs = vec![LiveReducedSubgraph {
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
            drivers: Vec::new(),
            chosen_driver: None,
            sheet_instance_path: String::new(),
            bus_neighbor_links: Vec::new(),
            bus_parent_links: Vec::new(),
            bus_parent_indexes: Vec::new(),
            bus_parent_handles: Vec::new(),
            base_pins: Vec::new(),
            hier_parent_index: None,
            hier_child_indexes: Vec::new(),
            hier_parent_handle: None,
            hier_child_handles: Vec::new(),
            label_links: Vec::new(),
            hier_sheet_pins: Vec::new(),
            hier_ports: Vec::new(),
            bus_items: Vec::new(),
            wire_items: Vec::new(),
            dirty: true,
        }];

        let handles = live_subgraphs
            .into_iter()
            .map(|subgraph| Rc::new(RefCell::new(subgraph)))
            .collect::<Vec<_>>();
        let mut by_name = BTreeMap::<String, Vec<LiveReducedSubgraphHandle>>::new();
        let mut by_sheet_and_name =
            BTreeMap::<(String, String), Vec<LiveReducedSubgraphHandle>>::new();
        by_name
            .entry("/OLD".to_string())
            .or_default()
            .push(handles[0].clone());
        by_sheet_and_name
            .entry((String::new(), "/OLD".to_string()))
            .or_default()
            .push(handles[0].clone());

        {
            let subgraph = handles[0].borrow();
            let mut connection = subgraph.driver_connection.borrow_mut();
            connection.name = "/NEW".to_string();
            connection.local_name = "NEW".to_string();
            connection.full_local_name = "/NEW".to_string();
        }

        recache_live_reduced_subgraph_name_handle_cache_from_handles(
            &mut by_name,
            &mut by_sheet_and_name,
            &handles[0],
            "/OLD",
        );

        assert_eq!(by_name.get("/OLD"), Some(&Vec::new()));
        assert_eq!(by_name.get("/NEW").map(Vec::len), Some(1));
        assert!(
            by_name
                .get("/NEW")
                .into_iter()
                .flatten()
                .any(|handle| Rc::ptr_eq(handle, &handles[0]))
        );
    }

    #[test]
    fn build_live_reduced_subgraph_handles_preserves_shared_subgraph_identity() {
        let reduced = vec![ReducedProjectSubgraphEntry {
            subgraph_code: 1,
            code: 1,
            name: "/BUS".to_string(),
            resolved_connection: ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Bus,
                name: "/BUS".to_string(),
                local_name: "BUS".to_string(),
                full_local_name: "/BUS".to_string(),
                sheet_instance_path: String::new(),
                members: vec![ReducedBusMember {
                    net_code: 1,
                    name: "BUS0".to_string(),
                    local_name: "BUS0".to_string(),
                    full_local_name: "/BUS0".to_string(),
                    vector_index: Some(0),
                    kind: ReducedBusMemberKind::Net,
                    members: Vec::new(),
                }],
            },
            driver_connection: None,
            chosen_driver_identity: None,
            drivers: Vec::new(),
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: String::new(),
            anchor: PointKey(0, 0),
            points: Vec::new(),
            nodes: Vec::new(),
            label_links: Vec::new(),
            no_connect_points: Vec::new(),
            hier_sheet_pins: Vec::new(),
            hier_ports: Vec::new(),
            bus_members: Vec::new(),
            bus_neighbor_links: Vec::new(),
            bus_parent_links: Vec::new(),
            bus_parent_indexes: Vec::new(),
            hier_parent_index: None,
            hier_child_indexes: Vec::new(),
            bus_items: vec![ReducedSubgraphWireItem {
                start: PointKey(0, 0),
                end: PointKey(10, 0),
                is_bus_entry: false,
                connected_bus_subgraph_index: None,
            }],
            wire_items: vec![ReducedSubgraphWireItem {
                start: PointKey(0, 0),
                end: PointKey(5, 5),
                is_bus_entry: true,
                connected_bus_subgraph_index: Some(0),
            }],
            base_pins: Vec::new(),
        }];

        let handles = build_live_reduced_subgraph_handles(&reduced);
        let shared = handles[0].clone();

        {
            let handle = handles[0].borrow();
            let mut connection = handle.driver_connection.borrow_mut();
            connection.name = "/RENAMED".to_string();
            connection.local_name = "RENAMED".to_string();
            connection.full_local_name = "/RENAMED".to_string();
        }
        handles[0].borrow_mut().dirty = false;

        assert_eq!(shared.borrow().driver_connection.name(), "/RENAMED");
        assert!(!shared.borrow().dirty);
        let attached_bus_item = shared.borrow().wire_items[0]
            .borrow()
            .connected_bus_item_handle
            .as_ref()
            .and_then(Weak::upgrade)
            .expect("attached live bus item");
        assert!(Rc::ptr_eq(
            &attached_bus_item,
            &shared.borrow().bus_items[0]
        ));
        let attached_bus_connection = attached_bus_item
            .borrow()
            .connection
            .clone()
            .expect("attached live bus connection");
        assert!(super::live_connection_handle_clone_eq(
            &attached_bus_connection,
            &shared.borrow().driver_connection
        ));
    }

    #[test]
    fn build_live_reduced_subgraph_handles_attach_sheet_pin_driver_owners() {
        let reduced = vec![ReducedProjectSubgraphEntry {
            subgraph_code: 1,
            code: 1,
            name: "/SIG".to_string(),
            resolved_connection: ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Net,
                name: "/SIG".to_string(),
                local_name: "SIG".to_string(),
                full_local_name: "/SIG".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
            driver_connection: Some(ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Net,
                name: "/SIG".to_string(),
                local_name: "SIG".to_string(),
                full_local_name: "/SIG".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            }),
            chosen_driver_identity: None,
            drivers: vec![ReducedProjectStrongDriver {
                kind: ReducedProjectDriverKind::SheetPin,
                priority: 1,
                connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/SIG".to_string(),
                    local_name: "SIG".to_string(),
                    full_local_name: "/SIG".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                identity: Some(super::ReducedProjectDriverIdentity::SheetPin {
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                    at: PointKey(10, 20),
                }),
            }],
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: String::new(),
            anchor: PointKey(10, 20),
            points: Vec::new(),
            nodes: Vec::new(),
            base_pins: Vec::new(),
            label_links: Vec::new(),
            no_connect_points: Vec::new(),
            hier_sheet_pins: vec![ReducedHierSheetPinLink {
                at: PointKey(10, 20),
                child_sheet_uuid: Some("child".to_string()),
                connection: ReducedProjectConnection {
                    net_code: 1,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/SIG".to_string(),
                    local_name: "SIG".to_string(),
                    full_local_name: "/SIG".to_string(),
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
        }];

        let handles = build_live_reduced_subgraph_handles(&reduced);
        let subgraph = handles[0].borrow();
        let owner = subgraph.drivers[0].borrow().owner.clone();

        match owner {
            super::LiveProjectStrongDriverOwner::SheetPin { owner, .. } => {
                let owner = owner.upgrade().expect("sheet pin owner");
                assert!(Rc::ptr_eq(&owner, &subgraph.hier_sheet_pins[0]));
                let driver = owner
                    .borrow()
                    .driver
                    .clone()
                    .expect("sheet pin driver owner");
                assert!(!matches!(
                    driver.borrow().owner,
                    super::LiveProjectStrongDriverOwner::Floating { .. }
                ));
                let driver = driver.borrow().snapshot();
                assert_eq!(driver.connection.local_name, "SIG");
                assert_eq!(driver.connection.name, "/SIG");
                assert_eq!(driver.priority, 1);
                assert!(matches!(
                    driver.identity,
                    Some(super::ReducedProjectDriverIdentity::SheetPin {
                        at: PointKey(10, 20),
                        ..
                    })
                ));
            }
            _ => panic!("expected sheet pin strong-driver owner"),
        }
    }

    #[test]
    fn build_live_reduced_subgraph_handles_attach_symbol_pin_driver_owners() {
        let reduced = vec![ReducedProjectSubgraphEntry {
            subgraph_code: 1,
            code: 1,
            name: "PWR".to_string(),
            resolved_connection: ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Net,
                name: "PWR".to_string(),
                local_name: "PWR".to_string(),
                full_local_name: "PWR".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
            driver_connection: Some(ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Net,
                name: "PWR".to_string(),
                local_name: "PWR".to_string(),
                full_local_name: "PWR".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            }),
            chosen_driver_identity: Some(super::ReducedProjectDriverIdentity::SymbolPin {
                schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                symbol_uuid: Some("sym".to_string()),
                at: PointKey(10, 20),
                pin_number: Some("1".to_string()),
            }),
            drivers: vec![ReducedProjectStrongDriver {
                kind: ReducedProjectDriverKind::PowerPin,
                priority: 6,
                connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "PWR".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                identity: Some(super::ReducedProjectDriverIdentity::SymbolPin {
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                    symbol_uuid: Some("sym".to_string()),
                    at: PointKey(10, 20),
                    pin_number: Some("1".to_string()),
                }),
            }],
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: String::new(),
            anchor: PointKey(10, 20),
            points: Vec::new(),
            nodes: Vec::new(),
            base_pins: vec![super::ReducedProjectBasePin {
                key: super::ReducedNetBasePinKey {
                    sheet_instance_path: String::new(),
                    symbol_uuid: Some("sym".to_string()),
                    at: PointKey(10, 20),
                    name: Some("1".to_string()),
                    number: Some("1".to_string()),
                },
                number: Some("1".to_string()),
                electrical_type: Some("power_in".to_string()),
                connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "PWR".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
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

        let handles = build_live_reduced_subgraph_handles(&reduced);
        let subgraph = handles[0].borrow();
        let owner = subgraph.drivers[0].borrow().owner.clone();

        match owner {
            super::LiveProjectStrongDriverOwner::SymbolPin { owner, .. } => {
                let owner = owner.upgrade().expect("symbol pin owner");
                assert_eq!(owner.borrow().pin.key.symbol_uuid.as_deref(), Some("sym"));
                assert_eq!(owner.borrow().pin.key.at, PointKey(10, 20));
                assert_eq!(owner.borrow().pin.key.number.as_deref(), Some("1"));
                assert_eq!(
                    owner.borrow().connection.borrow().connection_type,
                    super::ReducedProjectConnectionType::Net
                );
                assert_eq!(owner.borrow().connection.name(), "PWR");
                let driver = owner
                    .borrow()
                    .driver
                    .clone()
                    .expect("symbol pin driver owner");
                assert!(!matches!(
                    driver.borrow().owner,
                    super::LiveProjectStrongDriverOwner::Floating { .. }
                ));
                let driver = driver.borrow().snapshot();
                assert_eq!(driver.connection.local_name, "PWR");
                assert_eq!(driver.connection.name, "PWR");
                assert_eq!(driver.priority, 6);
                assert!(matches!(
                    driver.identity,
                    Some(super::ReducedProjectDriverIdentity::SymbolPin {
                        symbol_uuid: Some(ref uuid),
                        at: PointKey(10, 20),
                        pin_number: Some(ref pin_number),
                        ..
                    }) if uuid == "sym" && pin_number == "1"
                ));
            }
            _ => panic!("expected symbol pin strong-driver owner"),
        }

        drop(subgraph);
        {
            let subgraph = handles[0].borrow();
            let mut connection = subgraph.driver_connection.borrow_mut();
            connection.name = "RENAMED".to_string();
            connection.local_name = "RENAMED".to_string();
            connection.full_local_name = "RENAMED".to_string();
        }

        let subgraph = handles[0].borrow();
        let owner = match subgraph.drivers[0].borrow().owner.clone() {
            super::LiveProjectStrongDriverOwner::SymbolPin { owner, .. } => {
                owner.upgrade().expect("symbol pin owner")
            }
            _ => panic!("expected symbol pin strong-driver owner"),
        };
        assert_eq!(owner.borrow().connection.name(), "RENAMED");
    }

    #[test]
    fn build_live_reduced_subgraph_handles_keep_stacked_symbol_pin_driver_owners_distinct() {
        let reduced = vec![ReducedProjectSubgraphEntry {
            subgraph_code: 1,
            code: 1,
            name: "PWR".to_string(),
            resolved_connection: ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Net,
                name: "PWR".to_string(),
                local_name: "PWR".to_string(),
                full_local_name: "PWR".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
            driver_connection: Some(ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Net,
                name: "PWR".to_string(),
                local_name: "PWR".to_string(),
                full_local_name: "PWR".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            }),
            chosen_driver_identity: None,
            drivers: vec![
                ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::PowerPin,
                    priority: 6,
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "VCC".to_string(),
                        local_name: "VCC".to_string(),
                        full_local_name: "VCC".to_string(),
                        sheet_instance_path: String::new(),
                        members: Vec::new(),
                    },
                    identity: Some(super::ReducedProjectDriverIdentity::SymbolPin {
                        schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                        symbol_uuid: Some("sym".to_string()),
                        at: PointKey(10, 20),
                        pin_number: Some("1".to_string()),
                    }),
                },
                ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::PowerPin,
                    priority: 6,
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "GND".to_string(),
                        local_name: "GND".to_string(),
                        full_local_name: "GND".to_string(),
                        sheet_instance_path: String::new(),
                        members: Vec::new(),
                    },
                    identity: Some(super::ReducedProjectDriverIdentity::SymbolPin {
                        schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                        symbol_uuid: Some("sym".to_string()),
                        at: PointKey(10, 20),
                        pin_number: Some("2".to_string()),
                    }),
                },
            ],
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: String::new(),
            anchor: PointKey(10, 20),
            points: Vec::new(),
            nodes: Vec::new(),
            base_pins: vec![
                super::ReducedProjectBasePin {
                    key: super::ReducedNetBasePinKey {
                        sheet_instance_path: String::new(),
                        symbol_uuid: Some("sym".to_string()),
                        at: PointKey(10, 20),
                        name: Some("PWR".to_string()),
                        number: Some("1".to_string()),
                    },
                    number: Some("1".to_string()),
                    electrical_type: Some("power_in".to_string()),
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "VCC".to_string(),
                        local_name: "VCC".to_string(),
                        full_local_name: "VCC".to_string(),
                        sheet_instance_path: String::new(),
                        members: Vec::new(),
                    },
                },
                super::ReducedProjectBasePin {
                    key: super::ReducedNetBasePinKey {
                        sheet_instance_path: String::new(),
                        symbol_uuid: Some("sym".to_string()),
                        at: PointKey(10, 20),
                        name: Some("PWR".to_string()),
                        number: Some("2".to_string()),
                    },
                    number: Some("2".to_string()),
                    electrical_type: Some("power_in".to_string()),
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "GND".to_string(),
                        local_name: "GND".to_string(),
                        full_local_name: "GND".to_string(),
                        sheet_instance_path: String::new(),
                        members: Vec::new(),
                    },
                },
            ],
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

        let handles = build_live_reduced_subgraph_handles(&reduced);
        let subgraph = handles[0].borrow();
        let first_owner = match &subgraph.drivers[0].borrow().owner {
            super::LiveProjectStrongDriverOwner::SymbolPin { owner, .. } => {
                owner.upgrade().expect("first symbol pin owner")
            }
            _ => panic!("expected first symbol pin strong-driver owner"),
        };
        let second_owner = match &subgraph.drivers[1].borrow().owner {
            super::LiveProjectStrongDriverOwner::SymbolPin { owner, .. } => {
                owner.upgrade().expect("second symbol pin owner")
            }
            _ => panic!("expected second symbol pin strong-driver owner"),
        };

        assert_eq!(first_owner.borrow().pin.key.number.as_deref(), Some("1"));
        assert_eq!(second_owner.borrow().pin.key.number.as_deref(), Some("2"));
        assert_eq!(first_owner.borrow().connection.name(), "VCC");
        assert_eq!(second_owner.borrow().connection.name(), "GND");
    }

    #[test]
    fn build_live_reduced_subgraph_handles_attach_bus_link_handles() {
        let reduced = vec![
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
                        name: "SIG0".to_string(),
                        local_name: "SIG0".to_string(),
                        full_local_name: "/SIG0".to_string(),
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
                    sheet_instance_path: String::new(),
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "SIG0".to_string(),
                        local_name: "SIG0".to_string(),
                        full_local_name: "/SIG0".to_string(),
                        vector_index: Some(0),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                }),
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                bus_items: vec![ReducedSubgraphWireItem {
                    start: PointKey(0, 0),
                    end: PointKey(10, 0),
                    is_bus_entry: false,
                    connected_bus_subgraph_index: None,
                }],
                wire_items: Vec::new(),
                bus_neighbor_links: vec![ReducedProjectBusNeighborLink {
                    member: ReducedBusMember {
                        net_code: 0,
                        name: "SIG0".to_string(),
                        local_name: "SIG0".to_string(),
                        full_local_name: "/SIG0".to_string(),
                        vector_index: Some(0),
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
                name: "/SIG0".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/SIG0".to_string(),
                    local_name: "SIG0".to_string(),
                    full_local_name: "/SIG0".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/SIG0".to_string(),
                    local_name: "SIG0".to_string(),
                    full_local_name: "/SIG0".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                }),
                chosen_driver_identity: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 10),
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
                        name: "SIG0".to_string(),
                        local_name: "SIG0".to_string(),
                        full_local_name: "/SIG0".to_string(),
                        vector_index: Some(0),
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

        let handles = build_live_reduced_subgraph_handles(&reduced);
        let bus = handles[0].borrow();
        let net = handles[1].borrow();

        let bus_neighbor = bus.bus_neighbor_links[0]
            .borrow()
            .subgraph_handle
            .as_ref()
            .and_then(Weak::upgrade)
            .expect("bus neighbor handle");
        let net_parent = net.bus_parent_links[0]
            .borrow()
            .subgraph_handle
            .as_ref()
            .and_then(Weak::upgrade)
            .expect("net parent handle");

        assert!(Rc::ptr_eq(&bus_neighbor, &handles[1]));
        assert!(Rc::ptr_eq(&net_parent, &handles[0]));
        assert_eq!(net.bus_parent_handles.len(), 1);
    }

    #[test]
    fn collect_live_reduced_propagation_component_prefers_hierarchy_handles() {
        let reduced = vec![
            ReducedProjectSubgraphEntry {
                subgraph_code: 1,
                code: 1,
                name: "/TOP".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/TOP".to_string(),
                    local_name: "TOP".to_string(),
                    full_local_name: "/TOP".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                driver_connection: None,
                chosen_driver_identity: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: vec![ReducedHierPortLink {
                    at: PointKey(0, 0),
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "/TOP".to_string(),
                        local_name: "TOP".to_string(),
                        full_local_name: "/TOP".to_string(),
                        sheet_instance_path: String::new(),
                        members: Vec::new(),
                    },
                }],
                bus_members: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: vec![1],
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                base_pins: Vec::new(),
            },
            ReducedProjectSubgraphEntry {
                subgraph_code: 2,
                code: 2,
                name: "/CHILD".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/CHILD".to_string(),
                    local_name: "CHILD".to_string(),
                    full_local_name: "/CHILD".to_string(),
                    sheet_instance_path: "/child".to_string(),
                    members: Vec::new(),
                },
                driver_connection: None,
                chosen_driver_identity: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: "/child".to_string(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: vec![ReducedHierSheetPinLink {
                    at: PointKey(0, 0),
                    child_sheet_uuid: None,
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "/CHILD".to_string(),
                        local_name: "CHILD".to_string(),
                        full_local_name: "/CHILD".to_string(),
                        sheet_instance_path: "/child".to_string(),
                        members: Vec::new(),
                    },
                }],
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: Some(0),
                hier_child_indexes: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                base_pins: Vec::new(),
            },
        ];

        let handles = build_live_reduced_subgraph_handles(&reduced);

        {
            let mut parent = handles[0].borrow_mut();
            parent.hier_child_indexes.clear();
        }
        {
            let mut child = handles[1].borrow_mut();
            child.hier_parent_index = None;
        }

        let mut component = collect_live_reduced_propagation_component_from_handles(0, &handles);
        component.sort_unstable();

        assert_eq!(component, vec![0, 1]);
        let child_parent = handles[1]
            .borrow()
            .hier_parent_handle
            .as_ref()
            .and_then(Weak::upgrade)
            .expect("live hierarchy parent handle");
        assert!(Rc::ptr_eq(&child_parent, &handles[0]));
    }

    #[test]
    fn collect_live_reduced_propagation_component_prefers_bus_parent_handles() {
        let reduced = vec![
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
                        name: "SIG0".to_string(),
                        local_name: "SIG0".to_string(),
                        full_local_name: "/SIG0".to_string(),
                        vector_index: Some(0),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                },
                driver_connection: None,
                chosen_driver_identity: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
                bus_items: vec![ReducedSubgraphWireItem {
                    start: PointKey(0, 0),
                    end: PointKey(10, 0),
                    is_bus_entry: false,
                    connected_bus_subgraph_index: None,
                }],
                wire_items: Vec::new(),
                base_pins: Vec::new(),
            },
            ReducedProjectSubgraphEntry {
                subgraph_code: 2,
                code: 2,
                name: "/SIG0".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/SIG0".to_string(),
                    local_name: "SIG0".to_string(),
                    full_local_name: "/SIG0".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                driver_connection: None,
                chosen_driver_identity: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(1, 1),
                points: Vec::new(),
                nodes: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: vec![0],
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                base_pins: Vec::new(),
            },
        ];

        let handles = build_live_reduced_subgraph_handles(&reduced);
        let mut component = collect_live_reduced_propagation_component_from_handles(1, &handles);
        component.sort_unstable();

        assert_eq!(component, vec![0, 1]);
        let child_parent = handles[1]
            .borrow()
            .bus_parent_handles
            .first()
            .and_then(Weak::upgrade)
            .expect("live bus parent handle");
        assert!(Rc::ptr_eq(&child_parent, &handles[0]));
    }

    #[test]
    fn replay_reduced_live_stale_bus_members_updates_other_bus_subgraphs() {
        let live_subgraphs = vec![
            LiveReducedSubgraph {
                source_index: 0,
                driver_connection: LiveReducedConnection::new(ReducedProjectConnection {
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
                drivers: Vec::new(),
                chosen_driver: None,
                sheet_instance_path: String::new(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                bus_parent_handles: Vec::new(),
                base_pins: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
                hier_parent_handle: None,
                hier_child_handles: Vec::new(),
                label_links: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                dirty: true,
            },
            LiveReducedSubgraph {
                source_index: 1,
                driver_connection: LiveReducedConnection::new(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS_B".to_string(),
                    local_name: "BUS_B".to_string(),
                    full_local_name: "/BUS_B".to_string(),
                    sheet_instance_path: "/child".to_string(),
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
                drivers: Vec::new(),
                chosen_driver: None,
                sheet_instance_path: "/child".to_string(),
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                bus_parent_handles: Vec::new(),
                base_pins: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
                hier_parent_handle: None,
                hier_child_handles: Vec::new(),
                label_links: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_items: Vec::new(),
                wire_items: Vec::new(),
                dirty: true,
            },
        ];

        let handles = live_subgraphs
            .into_iter()
            .map(|subgraph| Rc::new(RefCell::new(subgraph)))
            .collect::<Vec<_>>();

        replay_reduced_live_stale_bus_members_on_handles_for_indexes(
            &handles,
            &[0, 1],
            &[Rc::new(RefCell::new(super::LiveProjectBusMember::from(
                ReducedBusMember {
                    net_code: 0,
                    name: "PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "/PWR".to_string(),
                    vector_index: Some(1),
                    kind: ReducedBusMemberKind::Net,
                    members: Vec::new(),
                },
            )))],
        );

        assert_eq!(
            handles[0].borrow().driver_connection.borrow().members[0]
                .borrow()
                .full_local_name,
            "/PWR"
        );
        assert_eq!(
            handles[1].borrow().driver_connection.borrow().members[0]
                .borrow()
                .full_local_name,
            "/PWR"
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                chosen_driver_identity: None,
                drivers: vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: 6,
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "/PWR".to_string(),
                        local_name: "PWR".to_string(),
                        full_local_name: "/PWR".to_string(),
                        sheet_instance_path: String::new(),
                        members: Vec::new(),
                    },
                    identity: None,
                }],
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                chosen_driver_identity: None,
                drivers: vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: 4,
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "/ROOT_SIG".to_string(),
                        local_name: "ROOT_SIG".to_string(),
                        full_local_name: "/ROOT_SIG".to_string(),
                        sheet_instance_path: String::new(),
                        members: Vec::new(),
                    },
                    identity: None,
                }],
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
                chosen_driver_identity: None,
                drivers: vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::PowerPin,
                    priority: 6,
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "/Child/GLOBAL_SIG".to_string(),
                        local_name: "GLOBAL_SIG".to_string(),
                        full_local_name: "/Child/GLOBAL_SIG".to_string(),
                        sheet_instance_path: "/child".to_string(),
                        members: Vec::new(),
                    },
                    identity: None,
                }],
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
    fn reduced_project_subgraph_driver_identity_keeps_exact_chosen_identity() {
        let chosen_connection = ReducedProjectConnection {
            net_code: 0,
            connection_type: ReducedProjectConnectionType::Net,
            name: "/SIG".to_string(),
            local_name: "SIG".to_string(),
            full_local_name: "/SIG".to_string(),
            sheet_instance_path: String::new(),
            members: Vec::new(),
        };
        let label_identity = super::ReducedProjectDriverIdentity::Label {
            schematic_path: std::path::PathBuf::from("root.kicad_sch"),
            at: PointKey(0, 0),
            kind: super::reduced_label_kind_sort_key(LabelKind::Local),
        };
        let sheet_pin_identity = super::ReducedProjectDriverIdentity::SheetPin {
            schematic_path: std::path::PathBuf::from("root.kicad_sch"),
            at: PointKey(10, 0),
        };
        let subgraph = ReducedProjectSubgraphEntry {
            subgraph_code: 1,
            code: 1,
            name: "/SIG".to_string(),
            resolved_connection: chosen_connection.clone(),
            driver_connection: Some(chosen_connection.clone()),
            chosen_driver_identity: Some(sheet_pin_identity.clone()),
            drivers: vec![
                ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: 7,
                    connection: chosen_connection.clone(),
                    identity: Some(label_identity),
                },
                ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::SheetPin,
                    priority: 1,
                    connection: chosen_connection.clone(),
                    identity: Some(sheet_pin_identity.clone()),
                },
            ],
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
        };

        assert_eq!(
            super::reduced_project_subgraph_driver_identity(&subgraph),
            Some(&sheet_pin_identity)
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
                chosen_driver_identity: None,
                drivers: vec![
                    ReducedProjectStrongDriver {
                        kind: ReducedProjectDriverKind::PowerPin,
                        priority: 6,
                        connection: chosen.clone(),
                        identity: None,
                    },
                    ReducedProjectStrongDriver {
                        kind: ReducedProjectDriverKind::PowerPin,
                        priority: 6,
                        connection: ReducedProjectConnection {
                            net_code: 0,
                            connection_type: ReducedProjectConnectionType::Net,
                            name: "PWR_ALT".to_string(),
                            local_name: "PWR_ALT".to_string(),
                            full_local_name: "PWR_ALT".to_string(),
                            sheet_instance_path: String::new(),
                            members: Vec::new(),
                        },
                        identity: None,
                    },
                ],
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
                chosen_driver_identity: None,
                drivers: vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::PowerPin,
                    priority: 6,
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "PWR_ALT".to_string(),
                        local_name: "PWR_ALT".to_string(),
                        full_local_name: "PWR_ALT".to_string(),
                        sheet_instance_path: "/other".to_string(),
                        members: Vec::new(),
                    },
                    identity: None,
                }],
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
    fn reduced_live_graph_propagation_promotes_secondary_globals_on_live_owner() {
        let mut graph = vec![
            ReducedProjectSubgraphEntry {
                subgraph_code: 1,
                code: 1,
                name: "VCC".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "VCC".to_string(),
                    local_name: "VCC".to_string(),
                    full_local_name: "VCC".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                driver_connection: Some(ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "VCC".to_string(),
                    local_name: "VCC".to_string(),
                    full_local_name: "VCC".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                }),
                chosen_driver_identity: None,
                drivers: vec![
                    ReducedProjectStrongDriver {
                        kind: ReducedProjectDriverKind::PowerPin,
                        priority: 6,
                        connection: ReducedProjectConnection {
                            net_code: 0,
                            connection_type: ReducedProjectConnectionType::Net,
                            name: "VCC".to_string(),
                            local_name: "VCC".to_string(),
                            full_local_name: "VCC".to_string(),
                            sheet_instance_path: String::new(),
                            members: Vec::new(),
                        },
                        identity: None,
                    },
                    ReducedProjectStrongDriver {
                        kind: ReducedProjectDriverKind::PowerPin,
                        priority: 6,
                        connection: ReducedProjectConnection {
                            net_code: 0,
                            connection_type: ReducedProjectConnectionType::Net,
                            name: "PWR_ALT".to_string(),
                            local_name: "PWR_ALT".to_string(),
                            full_local_name: "PWR_ALT".to_string(),
                            sheet_instance_path: String::new(),
                            members: Vec::new(),
                        },
                        identity: None,
                    },
                ],
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
                chosen_driver_identity: None,
                drivers: vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::PowerPin,
                    priority: 6,
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "PWR_ALT".to_string(),
                        local_name: "PWR_ALT".to_string(),
                        full_local_name: "PWR_ALT".to_string(),
                        sheet_instance_path: "/other".to_string(),
                        members: Vec::new(),
                    },
                    identity: None,
                }],
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

        refresh_reduced_live_graph_propagation(&mut graph);

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
            chosen_driver_identity: None,
            drivers: Vec::new(),
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: String::new(),
            anchor: PointKey(0, 0),
            points: Vec::new(),
            nodes: Vec::new(),
            base_pins: vec![crate::connectivity::ReducedProjectBasePin {
                key: crate::connectivity::ReducedNetBasePinKey {
                    sheet_instance_path: String::new(),
                    symbol_uuid: Some("r1".to_string()),
                    at: PointKey(0, 0),
                    name: Some("1".to_string()),
                    number: Some("1".to_string()),
                },
                number: Some("1".to_string()),
                electrical_type: Some("passive".to_string()),
                connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: String::new(),
                    local_name: String::new(),
                    full_local_name: String::new(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
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
            chosen_driver_identity: None,
            drivers: Vec::new(),
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: String::new(),
            anchor: PointKey(0, 0),
            points: Vec::new(),
            nodes: Vec::new(),
            base_pins: vec![crate::connectivity::ReducedProjectBasePin {
                key: crate::connectivity::ReducedNetBasePinKey {
                    sheet_instance_path: String::new(),
                    symbol_uuid: Some("r1".to_string()),
                    at: PointKey(0, 0),
                    name: Some("1".to_string()),
                    number: Some("1".to_string()),
                },
                number: Some("1".to_string()),
                electrical_type: Some("passive".to_string()),
                connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: String::new(),
                    local_name: String::new(),
                    full_local_name: String::new(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
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
    fn live_post_propagation_updates_self_driven_symbol_pin_base_pin_connections() {
        let reduced = vec![ReducedProjectSubgraphEntry {
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
            chosen_driver_identity: None,
            drivers: Vec::new(),
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: String::new(),
            anchor: PointKey(0, 0),
            points: Vec::new(),
            nodes: Vec::new(),
            base_pins: vec![crate::connectivity::ReducedProjectBasePin {
                key: crate::connectivity::ReducedNetBasePinKey {
                    sheet_instance_path: String::new(),
                    symbol_uuid: Some("r1".to_string()),
                    at: PointKey(0, 0),
                    name: Some("1".to_string()),
                    number: Some("1".to_string()),
                },
                number: Some("1".to_string()),
                electrical_type: Some("passive".to_string()),
                connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: String::new(),
                    local_name: String::new(),
                    full_local_name: String::new(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
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

        let handles = build_live_reduced_subgraph_handles(&reduced);
        super::refresh_reduced_live_post_propagation_item_connections_on_handles(&handles);

        let connection = handles[0].borrow().base_pins[0].borrow().connection.clone();

        assert_eq!(connection.name(), "unconnected-(R1-Pad1)");
    }

    #[test]
    fn live_item_connection_refresh_skips_bus_net_type_mismatches() {
        let reduced = vec![ReducedProjectSubgraphEntry {
            subgraph_code: 1,
            code: 1,
            name: "BUS".to_string(),
            resolved_connection: ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Bus,
                name: "BUS".to_string(),
                local_name: "BUS".to_string(),
                full_local_name: "BUS".to_string(),
                sheet_instance_path: String::new(),
                members: vec![ReducedBusMember {
                    net_code: 2,
                    name: "BUS0".to_string(),
                    local_name: "BUS0".to_string(),
                    full_local_name: "BUS0".to_string(),
                    vector_index: Some(0),
                    kind: ReducedBusMemberKind::Net,
                    members: Vec::new(),
                }],
            },
            driver_connection: Some(ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Bus,
                name: "BUS".to_string(),
                local_name: "BUS".to_string(),
                full_local_name: "BUS".to_string(),
                sheet_instance_path: String::new(),
                members: vec![ReducedBusMember {
                    net_code: 2,
                    name: "BUS0".to_string(),
                    local_name: "BUS0".to_string(),
                    full_local_name: "BUS0".to_string(),
                    vector_index: Some(0),
                    kind: ReducedBusMemberKind::Net,
                    members: Vec::new(),
                }],
            }),
            chosen_driver_identity: None,
            drivers: Vec::new(),
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
                    net_code: 3,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "SIG".to_string(),
                    local_name: "SIG".to_string(),
                    full_local_name: "SIG".to_string(),
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
        }];

        let handles = build_live_reduced_subgraph_handles(&reduced);
        super::sync_live_reduced_item_connections_from_driver_handle(&handles[0]);

        let connection = handles[0].borrow().label_links[0]
            .borrow()
            .connection
            .clone();

        assert_eq!(
            connection.borrow().connection_type,
            ReducedProjectConnectionType::Net
        );
        assert_eq!(connection.name(), "SIG");
    }

    #[test]
    fn live_item_connection_refresh_skips_chosen_driver_item() {
        let reduced = vec![ReducedProjectSubgraphEntry {
            subgraph_code: 1,
            code: 1,
            name: "/SIG".to_string(),
            resolved_connection: ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Net,
                name: "/SIG".to_string(),
                local_name: "SIG".to_string(),
                full_local_name: "/SIG".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
            driver_connection: Some(ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Net,
                name: "/SIG".to_string(),
                local_name: "SIG".to_string(),
                full_local_name: "/SIG".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            }),
            chosen_driver_identity: None,
            drivers: vec![ReducedProjectStrongDriver {
                kind: ReducedProjectDriverKind::Label,
                priority: 7,
                connection: ReducedProjectConnection {
                    net_code: 1,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/SIG".to_string(),
                    local_name: "SIG".to_string(),
                    full_local_name: "/SIG".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                identity: Some(super::ReducedProjectDriverIdentity::Label {
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                    at: PointKey(0, 0),
                    kind: super::reduced_label_kind_sort_key(LabelKind::Local),
                }),
            }],
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
                    net_code: 9,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/LOCAL".to_string(),
                    local_name: "LOCAL".to_string(),
                    full_local_name: "/LOCAL".to_string(),
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
        }];

        let handles = build_live_reduced_subgraph_handles(&reduced);
        super::sync_live_reduced_item_connections_from_driver_handle(&handles[0]);

        let connection = handles[0].borrow().label_links[0]
            .borrow()
            .connection
            .clone();

        assert_eq!(
            connection.borrow().connection_type,
            ReducedProjectConnectionType::Net
        );
        assert_eq!(connection.name(), "/LOCAL");
        assert_eq!(connection.borrow().local_name, "LOCAL");
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
                chosen_driver_identity: None,
                drivers: Vec::new(),
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
impl PartialEq for LiveReducedLabelLink {
    fn eq(&self, other: &Self) -> bool {
        (
            self.at,
            self.kind,
            &self.connection,
            live_optional_driver_snapshot(&self.driver),
        ) == (
            other.at,
            other.kind,
            &other.connection,
            live_optional_driver_snapshot(&other.driver),
        )
    }
}

impl Eq for LiveReducedLabelLink {}
