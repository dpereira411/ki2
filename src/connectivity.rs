use std::cell::RefCell;
#[cfg(test)]
use std::cell::{Ref, RefMut};
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
    pub(crate) schematic_path: std::path::PathBuf,
    pub(crate) key: ReducedNetBasePinKey,
    pub(crate) number: Option<String>,
    pub(crate) electrical_type: Option<String>,
    pub(crate) connection: ReducedProjectConnection,
    pub(crate) driver_connection: ReducedProjectConnection,
    pub(crate) preserve_local_name_on_refresh: bool,
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
    pub(crate) schematic_path: std::path::PathBuf,
    pub(crate) at: PointKey,
    pub(crate) kind: LabelKind,
    pub(crate) connection: ReducedProjectConnection,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ReducedHierSheetPinLink {
    pub(crate) schematic_path: std::path::PathBuf,
    pub(crate) at: PointKey,
    pub(crate) child_sheet_uuid: Option<String>,
    pub(crate) connection: ReducedProjectConnection,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ReducedHierPortLink {
    pub(crate) schematic_path: std::path::PathBuf,
    pub(crate) at: PointKey,
    pub(crate) connection: ReducedProjectConnection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReducedProjectSubgraphEntry {
    pub(crate) subgraph_code: usize,
    pub(crate) code: usize,
    pub(crate) name: String,
    pub(crate) resolved_connection: ReducedProjectConnection,
    pub(crate) driver_connection: ReducedProjectConnection,
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

type LiveProjectStrongDriverHandle = Rc<RefCell<LiveProjectStrongDriverOwner>>;

#[derive(Clone, Debug)]
#[allow(dead_code)]
enum LiveProjectStrongDriverOwner {
    Floating {
        identity: Option<ReducedProjectDriverIdentity>,
        connection: LiveProjectConnectionHandle,
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

impl From<ReducedProjectStrongDriver> for LiveProjectStrongDriverOwner {
    fn from(driver: ReducedProjectStrongDriver) -> Self {
        LiveProjectStrongDriverOwner::Floating {
            identity: driver.identity,
            connection: Rc::new(RefCell::new(driver.connection.into())),
            kind: driver.kind,
            priority: driver.priority,
        }
    }
}

impl LiveProjectStrongDriverOwner {
    // Upstream parity: local owner-side analogue for the exercised strong-driver metadata/identity
    // reads KiCad gets from the chosen live driver item and its attached `SCH_CONNECTION`. This
    // still runs on reduced live owners instead of the fuller live driver-item object graph, but
    // it moves active driver reads onto the shared owner itself instead of routing them through
    // parallel free helper accessors.
    fn kind(&self) -> ReducedProjectDriverKind {
        match self {
            LiveProjectStrongDriverOwner::Floating { kind, .. }
            | LiveProjectStrongDriverOwner::Label { kind, .. }
            | LiveProjectStrongDriverOwner::SheetPin { kind, .. }
            | LiveProjectStrongDriverOwner::HierPort { kind, .. }
            | LiveProjectStrongDriverOwner::SymbolPin { kind, .. } => *kind,
        }
    }

    fn priority(&self) -> i32 {
        match self {
            LiveProjectStrongDriverOwner::Floating { priority, .. }
            | LiveProjectStrongDriverOwner::Label { priority, .. }
            | LiveProjectStrongDriverOwner::SheetPin { priority, .. }
            | LiveProjectStrongDriverOwner::HierPort { priority, .. }
            | LiveProjectStrongDriverOwner::SymbolPin { priority, .. } => *priority,
        }
    }

    fn identity(&self) -> Option<ReducedProjectDriverIdentity> {
        match self {
            LiveProjectStrongDriverOwner::Floating { identity, .. } => identity.clone(),
            LiveProjectStrongDriverOwner::Label { owner, .. } => owner.upgrade().map(|owner| {
                let owner = owner.borrow();
                ReducedProjectDriverIdentity::Label {
                    schematic_path: owner.schematic_path.clone(),
                    at: owner.at,
                    kind: reduced_label_kind_sort_key(owner.kind),
                }
            }),
            LiveProjectStrongDriverOwner::SheetPin { owner, .. } => owner.upgrade().map(|owner| {
                let owner = owner.borrow();
                ReducedProjectDriverIdentity::SheetPin {
                    schematic_path: owner.schematic_path.clone(),
                    at: owner.at,
                }
            }),
            LiveProjectStrongDriverOwner::HierPort { owner, .. } => owner.upgrade().map(|owner| {
                let owner = owner.borrow();
                ReducedProjectDriverIdentity::Label {
                    schematic_path: owner.schematic_path.clone(),
                    at: owner.at,
                    kind: reduced_label_kind_sort_key(LabelKind::Hierarchical),
                }
            }),
            LiveProjectStrongDriverOwner::SymbolPin { owner, .. } => {
                owner.upgrade().and_then(|owner| {
                    let owner = owner.borrow();
                    owner.pin.key.symbol_uuid.as_ref().map(|symbol_uuid| {
                        ReducedProjectDriverIdentity::SymbolPin {
                            schematic_path: owner.pin.schematic_path.clone(),
                            symbol_uuid: Some(symbol_uuid.clone()),
                            at: owner.pin.key.at,
                            pin_number: owner.pin.number.clone(),
                        }
                    })
                })
            }
        }
    }

    fn connection_handle(&self) -> LiveProjectConnectionHandle {
        match self {
            LiveProjectStrongDriverOwner::Floating { connection, .. } => Some(connection.clone()),
            LiveProjectStrongDriverOwner::Label { owner, .. } => owner
                .upgrade()
                .map(|owner| owner.borrow().driver_connection.clone()),
            LiveProjectStrongDriverOwner::SheetPin { owner, .. } => owner
                .upgrade()
                .map(|owner| owner.borrow().driver_connection.clone()),
            LiveProjectStrongDriverOwner::HierPort { owner, .. } => owner
                .upgrade()
                .map(|owner| owner.borrow().driver_connection.clone()),
            LiveProjectStrongDriverOwner::SymbolPin { owner, .. } => owner
                .upgrade()
                .map(|owner| owner.borrow().driver_connection.clone()),
        }
        .expect("live strong driver owner requires an attached connection owner")
    }
}

fn empty_live_project_connection_handle() -> LiveProjectConnectionHandle {
    Rc::new(RefCell::new(LiveProjectConnection {
        net_code: 0,
        connection_type: ReducedProjectConnectionType::None,
        name: String::new(),
        local_name: String::new(),
        full_local_name: String::new(),
        sheet_instance_path: String::new(),
        members: Vec::new(),
    }))
}

impl LiveProjectStrongDriverOwner {
    fn full_name(&self) -> String {
        self.connection_handle().borrow().name.clone()
    }

    // Upstream parity: local live-driver analogue for reduced projection at the graph boundary.
    // Consumers still read reduced strong-driver records, but the shared live driver owner now
    // projects itself onto that boundary instead of leaving driver snapshot assembly in free
    // helper functions outside the owner graph.
    fn project_onto_reduced(&self, target: &mut ReducedProjectStrongDriver) {
        target.kind = self.kind();
        target.priority = self.priority();
        self.connection_handle()
            .borrow()
            .project_onto_reduced(&mut target.connection);
        target.identity = self.identity();
    }

    fn snapshot(&self) -> ReducedProjectStrongDriver {
        let mut target = ReducedProjectStrongDriver {
            kind: self.kind(),
            priority: self.priority(),
            connection: ReducedProjectConnection {
                net_code: 0,
                connection_type: ReducedProjectConnectionType::None,
                name: String::new(),
                local_name: String::new(),
                full_local_name: String::new(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
            identity: None,
        };
        self.project_onto_reduced(&mut target);
        target
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
    pin_number: Option<&str>,
) -> Option<ConnectionComponent> {
    // Upstream parity: reduced local analogue for the symbol-pin item lookup KiCad performs
    // through live `SCH_PIN*` identity. The Rust tree still resolves against reduced connection
    // members instead of real pin items, but the fallback owner path now includes projected pin
    // number so stacked pins do not alias one another just because they share `(symbol UUID, at)`.
    collect_connection_components(schematic)
        .into_iter()
        .find(|component| {
            component.members.iter().any(|member| {
                member.kind == ConnectionMemberKind::SymbolPin
                    && member.symbol_uuid == symbol.uuid
                    && points_equal(member.at, at)
                    && (pin_number.is_none() || member.pin_number.as_deref() == pin_number)
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
    // member path. Active clone now reads the shared live source member directly instead of
    // snapshotting it into a temporary reduced member first, which keeps recursive replay on the
    // same live owner graph through member refresh. Remaining divergence is fuller live
    // member/pointer ownership on the shared connection/subgraph graph.
    let source = source.borrow();
    clone_live_bus_member_into_live_bus_member(&mut target.borrow_mut(), &source);
}

fn clone_live_bus_member_into_live_connection_owner(
    target: &mut LiveProjectConnection,
    source: &LiveProjectBusMember,
    sheet_instance_path: &str,
) {
    target.clone_from_live_bus_member(source, sheet_instance_path);
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
    subgraph.driver_connection.clone()
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

    // Upstream parity: local bus-member-owner analogue for the reduced projection boundary after
    // live graph mutation. Consumers still read reduced member trees, but the shared live
    // bus-member owner now projects itself onto that boundary instead of leaving reduced member
    // cloning in a free helper outside the owner graph.
    fn project_onto_reduced(&self, target: &mut ReducedBusMember) {
        let existing_local_name = target.local_name.clone();
        let existing_vector_index = target.vector_index;
        let existing_members = target.members.clone();

        target.net_code = self.net_code;
        target.name = self.name.clone();
        if existing_local_name.is_empty() {
            target.local_name = self.local_name.clone();
        }
        target.full_local_name = self.full_local_name.clone();
        if existing_vector_index.is_some() {
            target.vector_index = existing_vector_index;
        } else {
            target.vector_index = self.vector_index;
        }
        target.kind = self.kind.clone();

        if matches!(target.kind, ReducedBusMemberKind::Bus)
            && matches!(self.kind, ReducedBusMemberKind::Bus)
        {
            if existing_members.is_empty() {
                target.members = live_bus_member_handles_to_snapshots(&self.members);
            } else {
                target.members = existing_members;

                let clone_limit = target.members.len().min(self.members.len());
                for index in 0..clone_limit {
                    self.members[index]
                        .borrow()
                        .project_onto_reduced(&mut target.members[index]);
                }

                if target.members.len() > self.members.len() {
                    target.members.truncate(self.members.len());
                } else if target.members.len() < self.members.len() {
                    target.members.extend(
                        self.members[target.members.len()..]
                            .iter()
                            .map(live_bus_member_handle_snapshot),
                    );
                }
            }
        } else {
            target.members = live_bus_member_handles_to_snapshots(&self.members);
        }
    }

    // Upstream parity: local live bus-member analogue for the exercised recursive member matching
    // KiCad performs through `SCH_CONNECTION` member trees. This still walks reduced live member
    // owners instead of a fuller live connection/member graph, but the recursion now belongs to
    // the member owner instead of free helper functions around the owner graph.
    fn matches_live_member(&self, search: &LiveProjectBusMember) -> bool {
        if let Some(search_index) = search.vector_index {
            if self.vector_index == Some(search_index) {
                return true;
            }
        }

        self.kind != ReducedBusMemberKind::Bus && self.local_name == search.local_name
    }

    fn matches_connection_member(&self, search: &LiveProjectConnection) -> bool {
        self.kind != ReducedBusMemberKind::Bus && self.local_name == search.local_name
    }

    fn find_descendant_live(
        &self,
        search: &LiveProjectBusMember,
    ) -> Option<LiveProjectBusMemberHandle> {
        for member in &self.members {
            let member_ref = member.borrow();
            if member_ref.matches_live_member(search) {
                return Some(member.clone());
            }

            if member_ref.kind == ReducedBusMemberKind::Bus {
                if let Some(found) = member_ref.find_descendant_live(search) {
                    return Some(found);
                }
            }
        }

        None
    }

    fn find_descendant_for_connection(
        &self,
        search: &LiveProjectConnection,
    ) -> Option<LiveProjectBusMemberHandle> {
        for member in &self.members {
            let member_ref = member.borrow();
            if member_ref.matches_connection_member(search) {
                return Some(member.clone());
            }

            if member_ref.kind == ReducedBusMemberKind::Bus {
                if let Some(found) = member_ref.find_descendant_for_connection(search) {
                    return Some(found);
                }
            }
        }

        None
    }

    fn find_descendant_mut_live(
        &mut self,
        search: &LiveProjectBusMember,
    ) -> Option<LiveProjectBusMemberHandle> {
        for member in &self.members {
            let member_ref = member.borrow();
            if member_ref.matches_live_member(search) {
                return Some(member.clone());
            }

            if member_ref.kind == ReducedBusMemberKind::Bus {
                drop(member_ref);
                if let Some(found) = member.borrow_mut().find_descendant_mut_live(search) {
                    return Some(found);
                }
            }
        }

        None
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

type LiveProjectConnectionHandle = Rc<RefCell<LiveProjectConnection>>;

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
    // Upstream parity: local live-owner bridge toward `SCH_CONNECTION::Clone()` on the shared live
    // graph. This still mutates a reduced local connection carrier instead of a full live
    // `SCH_CONNECTION`, but the active graph now routes live-to-live connection cloning through
    // the connection owner itself instead of open-coding field updates at each call site.
    // Owner-specific shown-text preservation for net connections now lives on item owners instead
    // of this shared connection clone, while bus clones still preserve existing local/member
    // identity on the shared owner.
    fn clone_from_live_connection(&mut self, source: &LiveProjectConnection) {
        let existing_local_name = self.local_name.clone();
        let existing_members = self.members.clone();

        self.net_code = source.net_code;
        self.connection_type = source.connection_type;
        self.name = source.name.clone();
        if matches!(
            self.connection_type,
            ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
        ) {
            if existing_local_name.is_empty() {
                self.local_name = source.local_name.clone();
            }
        } else {
            self.local_name = source.local_name.clone();
        }
        self.full_local_name = source.full_local_name.clone();
        self.sheet_instance_path = source.sheet_instance_path.clone();

        if matches!(
            self.connection_type,
            ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
        ) && matches!(
            source.connection_type,
            ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
        ) {
            if existing_members.is_empty() {
                self.members = source.members.clone();
            } else {
                self.members = existing_members;

                let clone_limit = self.members.len().min(source.members.len());
                for index in 0..clone_limit {
                    clone_live_bus_member_handle_into_live_bus_member_handle(
                        &self.members[index],
                        &source.members[index],
                    );
                }

                if self.members.len() > source.members.len() {
                    self.members.truncate(source.members.len());
                } else if self.members.len() < source.members.len() {
                    self.members
                        .extend(source.members[self.members.len()..].iter().cloned());
                }
            }
        } else {
            self.members = source.members.clone();
        }
    }

    // Upstream parity: local live-owner bridge toward cloning one propagated bus member into an
    // attached `SCH_CONNECTION`. This keeps the exercised bus-member-to-connection mutation on the
    // connection owner instead of open-coding it at each propagation site.
    fn clone_from_live_bus_member(
        &mut self,
        source: &LiveProjectBusMember,
        sheet_instance_path: &str,
    ) {
        let existing_local_name = self.local_name.clone();
        let existing_members = self.members.clone();

        self.net_code = source.net_code;
        self.connection_type = match source.kind {
            ReducedBusMemberKind::Net => ReducedProjectConnectionType::Net,
            ReducedBusMemberKind::Bus => ReducedProjectConnectionType::Bus,
        };
        self.name = source.full_local_name.clone();
        if existing_local_name.is_empty() {
            self.local_name = source.local_name.clone();
        }
        self.full_local_name = source.full_local_name.clone();
        self.sheet_instance_path = sheet_instance_path.to_string();

        if matches!(
            self.connection_type,
            ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
        ) && matches!(source.kind, ReducedBusMemberKind::Bus)
        {
            if existing_members.is_empty() {
                self.members = source.members.clone();
            } else {
                self.members = existing_members;

                let clone_limit = self.members.len().min(source.members.len());
                for index in 0..clone_limit {
                    clone_live_bus_member_handle_into_live_bus_member_handle(
                        &self.members[index],
                        &source.members[index],
                    );
                }

                if self.members.len() > source.members.len() {
                    self.members.truncate(source.members.len());
                } else if self.members.len() < source.members.len() {
                    self.members
                        .extend(source.members[self.members.len()..].iter().cloned());
                }
            }
        } else {
            self.members = source.members.clone();
        }
    }

    // Upstream parity: local live-owner bridge toward the exercised
    // `CONNECTION_SUBGRAPH::UpdateItemConnections()` no-connect rename branch. This keeps the
    // `Net-(` -> `unconnected-(` rename on the shared live connection owner instead of mutating
    // the three name fields separately at each call site.
    fn force_no_connect_name(&mut self) {
        self.name = reduced_force_no_connect_net_name(&self.name);
        self.local_name = reduced_force_no_connect_net_name(&self.local_name);
        self.full_local_name = reduced_force_no_connect_net_name(&self.full_local_name);
    }

    // Upstream parity: local connection-owner analogue for the exercised bus-member lookup paths
    // inside live graph propagation. This still walks reduced live member owners instead of a
    // fuller `SCH_CONNECTION` object graph, but active callers now ask the connection owner for
    // member matches instead of reaching through raw `.members` vectors.
    #[cfg_attr(not(test), allow(dead_code))]
    fn find_member_live(
        &self,
        search: &LiveProjectBusMember,
    ) -> Option<LiveProjectBusMemberHandle> {
        for member in &self.members {
            let member_ref = member.borrow();
            if member_ref.matches_live_member(search) {
                return Some(member.clone());
            }

            if member_ref.kind == ReducedBusMemberKind::Bus {
                if let Some(found) = member_ref.find_descendant_live(search) {
                    return Some(found);
                }
            }
        }

        None
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn find_member_for_connection(
        &self,
        search: &LiveProjectConnection,
    ) -> Option<LiveProjectBusMemberHandle> {
        for member in &self.members {
            let member_ref = member.borrow();
            if member_ref.matches_connection_member(search) {
                return Some(member.clone());
            }

            if member_ref.kind == ReducedBusMemberKind::Bus {
                if let Some(found) = member_ref.find_descendant_for_connection(search) {
                    return Some(found);
                }
            }
        }

        None
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn find_member_mut_live(
        &mut self,
        search: &LiveProjectBusMember,
    ) -> Option<LiveProjectBusMemberHandle> {
        for member in &self.members {
            let member_ref = member.borrow();
            if member_ref.matches_live_member(search) {
                return Some(member.clone());
            }

            if member_ref.kind == ReducedBusMemberKind::Bus {
                drop(member_ref);
                if let Some(found) = member.borrow_mut().find_descendant_mut_live(search) {
                    return Some(found);
                }
            }
        }

        None
    }

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

    // Upstream parity: local connection-owner analogue for the reduced projection boundary after
    // live graph mutation. Consumers still read reduced connections, but the shared live
    // connection owner now projects itself onto that boundary instead of leaving reduced
    // connection cloning in a free helper outside the owner graph.
    fn project_onto_reduced(&self, target: &mut ReducedProjectConnection) {
        let existing_local_name = target.local_name.clone();
        let existing_members = target.members.clone();

        target.net_code = self.net_code;
        target.connection_type = self.connection_type;
        target.name = self.name.clone();
        if existing_local_name.is_empty() {
            target.local_name = self.local_name.clone();
        }
        target.full_local_name = self.full_local_name.clone();
        target.sheet_instance_path = self.sheet_instance_path.clone();

        if matches!(
            target.connection_type,
            ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
        ) && matches!(
            self.connection_type,
            ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
        ) {
            if existing_members.is_empty() {
                target.members = live_bus_member_handles_to_snapshots(&self.members);
            } else {
                target.members = existing_members;

                let clone_limit = target.members.len().min(self.members.len());
                for index in 0..clone_limit {
                    self.members[index]
                        .borrow()
                        .project_onto_reduced(&mut target.members[index]);
                }

                if target.members.len() > self.members.len() {
                    target.members.truncate(self.members.len());
                } else if target.members.len() < self.members.len() {
                    target.members.extend(
                        self.members[target.members.len()..]
                            .iter()
                            .map(live_bus_member_handle_snapshot),
                    );
                }
            }
        } else {
            target.members = live_bus_member_handles_to_snapshots(&self.members);
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

fn clone_live_connection_owner_into_live_connection_owner(
    target: &mut LiveProjectConnection,
    source: &LiveProjectConnection,
) {
    target.clone_from_live_connection(source);
}

// Upstream parity: reduced local analogue for the item-owned `SCH_CONNECTION::Clone()` behavior
// the exercised label/sheet-pin/hier-port branches need during `UpdateItemConnections()`. This
// still operates on the reduced local connection carrier, but shown-text preservation now belongs
// to the item-owner clone path instead of the shared connection clone itself.
fn clone_live_connection_owner_into_live_item_connection_owner(
    target: &mut LiveProjectConnection,
    source: &LiveProjectConnection,
) {
    let existing_local_name = target.local_name.clone();
    clone_live_connection_owner_into_live_connection_owner(target, source);
    if !existing_local_name.is_empty() {
        target.local_name = existing_local_name;
    }
}

#[cfg(test)]
fn clone_reduced_connection_into_live_connection_owner(
    target: &mut LiveProjectConnection,
    source: &ReducedProjectConnection,
) {
    let source = LiveProjectConnection::from(source.clone());
    target.clone_from_live_connection(&source);
}

// Upstream parity: reduced local analogue for the per-pin `SCH_CONNECTION::Clone()` behavior the
// live graph needs after a chosen driver changes. This still operates on the reduced local
// connection carrier, but the live base-pin owner now decides whether pin-local text is preserved
// instead of re-deriving that choice from connection strings at every clone site. Remaining
// divergence is the still-missing fuller live pin object and item-owned `SCH_CONNECTION` cache.
fn clone_live_connection_owner_into_live_base_pin_connection_owner(
    target: &mut LiveProjectConnection,
    source: &LiveProjectConnection,
    preserve_local_name: bool,
) {
    let existing_local_name = target.local_name.clone();

    clone_live_connection_owner_into_live_connection_owner(target, source);

    if preserve_local_name {
        target.local_name = existing_local_name;
    } else {
        target.local_name = source.local_name.clone();
    }
}

#[cfg(test)]
#[derive(Clone)]
struct LiveReducedConnection {
    connection: Rc<RefCell<LiveProjectConnection>>,
}

#[cfg(test)]
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
}

#[cfg(test)]
impl std::fmt::Debug for LiveReducedConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.borrow().fmt(f)
    }
}

#[cfg(test)]
impl PartialEq for LiveReducedConnection {
    fn eq(&self, other: &Self) -> bool {
        self.snapshot() == other.snapshot()
    }
}

#[cfg(test)]
impl Eq for LiveReducedConnection {}

#[cfg(test)]
impl PartialOrd for LiveReducedConnection {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
impl Ord for LiveReducedConnection {
    fn cmp(&self, other: &Self) -> Ordering {
        self.snapshot().cmp(&other.snapshot())
    }
}

#[derive(Clone, Debug)]
struct LiveReducedLabelLink {
    schematic_path: std::path::PathBuf,
    at: PointKey,
    kind: LabelKind,
    connection: LiveProjectConnectionHandle,
    driver_connection: LiveProjectConnectionHandle,
    driver: Option<LiveProjectStrongDriverHandle>,
    parent_subgraph_handle: Weak<RefCell<LiveReducedSubgraph>>,
}
type LiveReducedLabelLinkHandle = Rc<RefCell<LiveReducedLabelLink>>;

#[derive(Clone, Debug)]
struct LiveReducedHierSheetPinLink {
    schematic_path: std::path::PathBuf,
    at: PointKey,
    child_sheet_uuid: Option<String>,
    connection: LiveProjectConnectionHandle,
    driver_connection: LiveProjectConnectionHandle,
    driver: Option<LiveProjectStrongDriverHandle>,
    parent_subgraph_handle: Weak<RefCell<LiveReducedSubgraph>>,
}
type LiveReducedHierSheetPinLinkHandle = Rc<RefCell<LiveReducedHierSheetPinLink>>;

#[derive(Clone, Debug)]
struct LiveReducedHierPortLink {
    schematic_path: std::path::PathBuf,
    at: PointKey,
    connection: LiveProjectConnectionHandle,
    driver_connection: LiveProjectConnectionHandle,
    driver: Option<LiveProjectStrongDriverHandle>,
    parent_subgraph_handle: Weak<RefCell<LiveReducedSubgraph>>,
}
type LiveReducedHierPortLinkHandle = Rc<RefCell<LiveReducedHierPortLink>>;

#[derive(Clone, Debug)]
struct LiveReducedBasePinPayload {
    schematic_path: std::path::PathBuf,
    key: ReducedNetBasePinKey,
    number: Option<String>,
    electrical_type: Option<String>,
}

#[derive(Clone, Debug)]
// Upstream parity: reduced local live pin-item payload under the shared graph. This still keeps a
// reduced projected pin payload instead of a live `SCH_PIN*`, but it now separates immutable pin
// identity/type data from the shared live connection owner so the active live pin carrier stops
// shadowing a second copied reduced connection beside the real live connection handle. Symbol-pin
// driver identity now derives from this owner payload instead of living as copied side state on
// the live pin carrier.
struct LiveReducedBasePin {
    pin: LiveReducedBasePinPayload,
    connection: LiveProjectConnectionHandle,
    driver_connection: LiveProjectConnectionHandle,
    driver: Option<LiveProjectStrongDriverHandle>,
    preserve_local_name_on_refresh: bool,
    parent_subgraph_handle: Weak<RefCell<LiveReducedSubgraph>>,
}

type LiveReducedBasePinHandle = Rc<RefCell<LiveReducedBasePin>>;

impl LiveReducedLabelLink {
    // Upstream parity: local item-owner analogue for the exercised
    // `CONNECTION_SUBGRAPH::UpdateItemConnections()` label branch. This still mutates a reduced
    // live item carrier instead of a real `SCH_LABEL`, but the label owner now decides whether to
    // adopt the chosen live connection and keeps the `item != m_driver` skip on the owner path
    // instead of leaving that policy in a separate helper loop.
    fn refresh_from_driver_connection(
        &mut self,
        chosen_driver: Option<&LiveProjectStrongDriverHandle>,
        driver_connection: &LiveProjectConnectionHandle,
        driver_connection_type: ReducedProjectConnectionType,
    ) {
        if chosen_driver
            .zip(self.driver.as_ref())
            .is_some_and(|(chosen, owner)| Rc::ptr_eq(chosen, owner))
        {
            return;
        }

        if reduced_connection_kind_mismatch(
            driver_connection_type,
            self.connection.borrow().connection_type,
        ) {
            return;
        }

        clone_live_connection_owner_into_live_item_connection_owner(
            &mut self.connection.borrow_mut(),
            &driver_connection.borrow(),
        );
    }

    // Upstream parity: local live label-owner analogue for binding one exercised strong driver
    // back onto the chosen item owner. This still returns a reduced local driver-owner variant
    // instead of a fuller live driver-item object, but it moves label attachment state and the
    // label-owned driver connection onto the label owner instead of open-coding both inside the
    // subgraph builder.
    fn attach_strong_driver(
        &mut self,
        owner: &LiveReducedLabelLinkHandle,
        source_driver_connection: &LiveProjectConnectionHandle,
        driver: &LiveProjectStrongDriverHandle,
        kind: ReducedProjectDriverKind,
        priority: i32,
    ) -> LiveProjectStrongDriverOwner {
        self.driver = Some(driver.clone());
        clone_live_connection_owner_into_live_connection_owner(
            &mut self.driver_connection.borrow_mut(),
            &source_driver_connection.borrow(),
        );
        LiveProjectStrongDriverOwner::Label {
            owner: Rc::downgrade(owner),
            kind,
            priority,
        }
    }
}

impl LiveReducedHierSheetPinLink {
    // Upstream parity: local item-owner analogue for the exercised sheet-pin branch of
    // `CONNECTION_SUBGRAPH::UpdateItemConnections()`. This still runs on a reduced live link owner
    // instead of a real `SCH_SHEET_PIN`, but the owner now applies the chosen-driver skip and
    // connection-kind guard directly.
    fn refresh_from_driver_connection(
        &mut self,
        chosen_driver: Option<&LiveProjectStrongDriverHandle>,
        driver_connection: &LiveProjectConnectionHandle,
        driver_connection_type: ReducedProjectConnectionType,
    ) {
        if chosen_driver
            .zip(self.driver.as_ref())
            .is_some_and(|(chosen, owner)| Rc::ptr_eq(chosen, owner))
        {
            return;
        }

        if reduced_connection_kind_mismatch(
            driver_connection_type,
            self.connection.borrow().connection_type,
        ) {
            return;
        }

        clone_live_connection_owner_into_live_item_connection_owner(
            &mut self.connection.borrow_mut(),
            &driver_connection.borrow(),
        );
    }

    // Upstream parity: local live sheet-pin-owner analogue for exercised strong-driver binding.
    // The fuller live driver-item graph is still missing, but the shared sheet-pin owner now owns
    // both the driver attachment and the sheet-pin-owned driver connection instead of leaving
    // them in surrounding builder logic or reduced snapshots.
    fn attach_strong_driver(
        &mut self,
        owner: &LiveReducedHierSheetPinLinkHandle,
        source_driver_connection: &LiveProjectConnectionHandle,
        driver: &LiveProjectStrongDriverHandle,
        kind: ReducedProjectDriverKind,
        priority: i32,
    ) -> LiveProjectStrongDriverOwner {
        self.driver = Some(driver.clone());
        clone_live_connection_owner_into_live_connection_owner(
            &mut self.driver_connection.borrow_mut(),
            &source_driver_connection.borrow(),
        );
        LiveProjectStrongDriverOwner::SheetPin {
            owner: Rc::downgrade(owner),
            kind,
            priority,
        }
    }
}

impl LiveReducedHierPortLink {
    // Upstream parity: local item-owner analogue for the exercised hierarchical-port branch of
    // `CONNECTION_SUBGRAPH::UpdateItemConnections()`. The live owner still wraps reduced payload,
    // but it now owns the exercised update decision instead of helper-side branch duplication.
    fn refresh_from_driver_connection(
        &mut self,
        chosen_driver: Option<&LiveProjectStrongDriverHandle>,
        driver_connection: &LiveProjectConnectionHandle,
        driver_connection_type: ReducedProjectConnectionType,
    ) {
        if chosen_driver
            .zip(self.driver.as_ref())
            .is_some_and(|(chosen, owner)| Rc::ptr_eq(chosen, owner))
        {
            return;
        }

        if reduced_connection_kind_mismatch(
            driver_connection_type,
            self.connection.borrow().connection_type,
        ) {
            return;
        }

        clone_live_connection_owner_into_live_item_connection_owner(
            &mut self.connection.borrow_mut(),
            &driver_connection.borrow(),
        );
    }

    // Upstream parity: local live hierarchical-port-owner analogue for exercised strong-driver
    // binding on the shared graph. The live port owner now keeps its own driver connection so
    // chosen-driver matching can stay on live owners for exercised hierarchical-label branches.
    fn attach_strong_driver(
        &mut self,
        owner: &LiveReducedHierPortLinkHandle,
        source_driver_connection: &LiveProjectConnectionHandle,
        driver: &LiveProjectStrongDriverHandle,
        kind: ReducedProjectDriverKind,
        priority: i32,
    ) -> LiveProjectStrongDriverOwner {
        self.driver = Some(driver.clone());
        clone_live_connection_owner_into_live_connection_owner(
            &mut self.driver_connection.borrow_mut(),
            &source_driver_connection.borrow(),
        );
        LiveProjectStrongDriverOwner::HierPort {
            owner: Rc::downgrade(owner),
            kind,
            priority,
        }
    }
}

impl LiveReducedBasePin {
    // Upstream parity: local pin-owner analogue for the exercised pin branch of
    // `CONNECTION_SUBGRAPH::UpdateItemConnections()`. This still updates a reduced live base-pin
    // owner instead of a real `SCH_PIN`, but the owner now decides whether to preserve pin-local
    // text, skip the chosen driver, and adopt the chosen live connection. Attached strong-driver
    // pins now widen their dedicated pin-driver connection owner onto that same chosen net
    // identity while preserving explicit pin-owned local driver text through owner state instead
    // of a clone-time string heuristic, so active symbol-pin driver reads stop staying on
    // pre-propagation setup snapshots after graph updates.
    fn refresh_from_driver_connection(
        &mut self,
        chosen_driver: Option<&LiveProjectStrongDriverHandle>,
        driver_connection: &LiveProjectConnectionHandle,
        driver_connection_type: ReducedProjectConnectionType,
        refresh_attached_strong_driver_pins: bool,
    ) {
        if chosen_driver
            .zip(self.driver.as_ref())
            .is_some_and(|(chosen, owner)| Rc::ptr_eq(chosen, owner))
        {
            return;
        }

        if !refresh_attached_strong_driver_pins && self.driver.is_some() {
            return;
        }

        if reduced_connection_kind_mismatch(
            driver_connection_type,
            self.connection.borrow().connection_type,
        ) {
            return;
        }

        clone_live_connection_owner_into_live_base_pin_connection_owner(
            &mut self.connection.borrow_mut(),
            &driver_connection.borrow(),
            self.preserve_local_name_on_refresh,
        );

        if refresh_attached_strong_driver_pins && self.driver.is_some() {
            clone_live_connection_owner_into_live_base_pin_connection_owner(
                &mut self.driver_connection.borrow_mut(),
                &driver_connection.borrow(),
                self.preserve_local_name_on_refresh,
            );
        }
    }

    // Upstream parity: local base-pin owner analogue for the exercised chosen-driver self-update
    // KiCad gets because `m_driver_connection` and the chosen pin's `Connection()` are the same
    // live object. Chosen symbol-pin owners now alias their item connection onto that live driver
    // handle on the active path; this fallback keeps the older reduced split-owner path safe until
    // the fuller live pin object exists.
    fn refresh_from_owned_driver_connection(&mut self) {
        if Rc::ptr_eq(&self.connection, &self.driver_connection) {
            return;
        }
        let driver_connection = self.driver_connection.clone();
        clone_live_connection_owner_into_live_base_pin_connection_owner(
            &mut self.connection.borrow_mut(),
            &driver_connection.borrow(),
            false,
        );
    }

    // Upstream parity: local base-pin-owner analogue for the chosen symbol-pin path where KiCad's
    // pin `Connection()` and `m_driver_connection` are the same live object. This keeps the
    // exercised chosen symbol-pin branch on one shared live connection handle instead of relying
    // on later self-refresh to copy names across split local owners.
    fn adopt_driver_connection_as_item_connection(&mut self) {
        if !Rc::ptr_eq(&self.connection, &self.driver_connection) {
            self.connection = self.driver_connection.clone();
        }
    }

    // Upstream parity: local live base-pin-owner analogue for exercised symbol-pin/power-pin
    // strong-driver binding. This still uses the reduced live base-pin owner instead of a fuller
    // live `SCH_PIN`, but the base-pin owner now owns the driver attachment itself, keeps the
    // pre-seeded pin-driver connection owner, and marks explicit pin-local text preservation on
    // the owner instead of re-deriving it from connection strings during later refresh.
    fn attach_strong_driver(
        &mut self,
        owner: &LiveReducedBasePinHandle,
        driver: &LiveProjectStrongDriverHandle,
        kind: ReducedProjectDriverKind,
        priority: i32,
    ) -> LiveProjectStrongDriverOwner {
        self.driver = Some(driver.clone());
        self.preserve_local_name_on_refresh = true;
        LiveProjectStrongDriverOwner::SymbolPin {
            owner: Rc::downgrade(owner),
            kind,
            priority,
        }
    }
}

fn live_optional_driver_snapshot(
    driver: &Option<LiveProjectStrongDriverHandle>,
) -> Option<ReducedProjectStrongDriver> {
    driver.as_ref().map(|driver| driver.borrow().snapshot())
}

impl PartialEq for LiveReducedHierSheetPinLink {
    fn eq(&self, other: &Self) -> bool {
        (
            &self.schematic_path,
            self.at,
            &self.child_sheet_uuid,
            self.connection.borrow().snapshot(),
            self.driver_connection.borrow().snapshot(),
            live_optional_driver_snapshot(&self.driver),
        ) == (
            &other.schematic_path,
            other.at,
            &other.child_sheet_uuid,
            other.connection.borrow().snapshot(),
            other.driver_connection.borrow().snapshot(),
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
            &self.schematic_path,
            self.at,
            &self.child_sheet_uuid,
            self.connection.borrow().snapshot(),
            self.driver_connection.borrow().snapshot(),
            live_optional_driver_snapshot(&self.driver),
        )
            .cmp(&(
                &other.schematic_path,
                other.at,
                &other.child_sheet_uuid,
                other.connection.borrow().snapshot(),
                other.driver_connection.borrow().snapshot(),
                live_optional_driver_snapshot(&other.driver),
            ))
    }
}

impl PartialEq for LiveReducedHierPortLink {
    fn eq(&self, other: &Self) -> bool {
        (
            &self.schematic_path,
            self.at,
            self.connection.borrow().snapshot(),
            self.driver_connection.borrow().snapshot(),
            live_optional_driver_snapshot(&self.driver),
        ) == (
            &other.schematic_path,
            other.at,
            other.connection.borrow().snapshot(),
            other.driver_connection.borrow().snapshot(),
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
            &self.schematic_path,
            self.at,
            self.connection.borrow().snapshot(),
            self.driver_connection.borrow().snapshot(),
            live_optional_driver_snapshot(&self.driver),
        )
            .cmp(&(
                &other.schematic_path,
                other.at,
                other.connection.borrow().snapshot(),
                other.driver_connection.borrow().snapshot(),
                live_optional_driver_snapshot(&other.driver),
            ))
    }
}

impl PartialEq for LiveReducedBasePin {
    fn eq(&self, other: &Self) -> bool {
        (
            self.pin.schematic_path.clone(),
            self.pin.key.clone(),
            self.pin.number.clone(),
            self.pin.electrical_type.clone(),
            self.connection.borrow().snapshot(),
            self.driver_connection.borrow().snapshot(),
            live_optional_driver_snapshot(&self.driver),
        ) == (
            other.pin.schematic_path.clone(),
            other.pin.key.clone(),
            other.pin.number.clone(),
            other.pin.electrical_type.clone(),
            other.connection.borrow().snapshot(),
            other.driver_connection.borrow().snapshot(),
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
            self.pin.schematic_path.clone(),
            self.pin.key.clone(),
            self.pin.number.clone(),
            self.pin.electrical_type.clone(),
            self.connection.borrow().snapshot(),
            self.driver_connection.borrow().snapshot(),
            live_optional_driver_snapshot(&self.driver),
        )
            .cmp(&(
                other.pin.schematic_path.clone(),
                other.pin.key.clone(),
                other.pin.number.clone(),
                other.pin.electrical_type.clone(),
                other.connection.borrow().snapshot(),
                other.driver_connection.borrow().snapshot(),
                live_optional_driver_snapshot(&other.driver),
            ))
    }
}

#[derive(Clone, Debug)]
struct LiveReducedSubgraphWireItem {
    start: PointKey,
    end: PointKey,
    is_bus_entry: bool,
    connection: LiveProjectConnectionHandle,
    connected_bus_item_handle: Option<Weak<RefCell<LiveReducedSubgraphWireItem>>>,
    parent_subgraph_handle: Weak<RefCell<LiveReducedSubgraph>>,
}

impl LiveReducedSubgraphWireItem {
    // Upstream parity: local wire-item analogue for the exercised connected-bus attachment KiCad
    // keeps on bus entries during graph build. This still identifies the attached bus from reduced
    // wire geometry instead of real `SCH_LINE*` pointers, but the shared wire-item owner now owns
    // the geometric match and attached-bus write instead of leaving that decision in a free graph
    // builder loop.
    fn attach_connected_bus_item(
        &mut self,
        sheet_instance_path: &str,
        bus_subgraphs: &[(String, Vec<LiveReducedSubgraphWireItemHandle>)],
    ) {
        if !self.is_bus_entry {
            return;
        }

        let attached_bus = bus_subgraphs
            .iter()
            .find(|(bus_sheet_path, bus_items)| {
                *bus_sheet_path == sheet_instance_path
                    && bus_items.iter().any(|bus_item| {
                        let bus_item = bus_item.borrow();
                        point_on_wire_segment(
                            [f64::from_bits(self.start.0), f64::from_bits(self.start.1)],
                            [
                                f64::from_bits(bus_item.start.0),
                                f64::from_bits(bus_item.start.1),
                            ],
                            [
                                f64::from_bits(bus_item.end.0),
                                f64::from_bits(bus_item.end.1),
                            ],
                        ) || point_on_wire_segment(
                            [f64::from_bits(self.end.0), f64::from_bits(self.end.1)],
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
                            [f64::from_bits(self.start.0), f64::from_bits(self.start.1)],
                            [
                                f64::from_bits(bus_item.start.0),
                                f64::from_bits(bus_item.start.1),
                            ],
                            [
                                f64::from_bits(bus_item.end.0),
                                f64::from_bits(bus_item.end.1),
                            ],
                        ) || point_on_wire_segment(
                            [f64::from_bits(self.end.0), f64::from_bits(self.end.1)],
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

        self.connected_bus_item_handle = attached_bus;
    }
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
    _left: &LiveReducedSubgraphWireItem,
    _right: &LiveReducedSubgraphWireItem,
) -> bool {
    true
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
    subgraph_handle: Weak<RefCell<LiveReducedSubgraph>>,
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

impl LiveReducedSubgraphLink {
    // Upstream parity: local live-link analogue for the reduced bus-link projection boundary after
    // graph mutation. Consumers still read reduced parent/neighbor links, but the shared live link
    // owner now decides which member and target subgraph index it projects instead of leaving that
    // boundary logic in a free helper around the owner graph.
    fn projection_index(&self, live_subgraphs: &[LiveReducedSubgraphHandle]) -> usize {
        Weak::upgrade(&self.subgraph_handle)
            .map(|subgraph| live_subgraph_projection_index(live_subgraphs, &subgraph))
            .unwrap_or_else(|| {
                #[cfg(test)]
                {
                    self.subgraph_index
                }

                #[cfg(not(test))]
                {
                    unreachable!("active live bus link lookup requires an attached subgraph handle")
                }
            })
    }

    fn project_onto_reduced(
        &self,
        live_subgraphs: &[LiveReducedSubgraphHandle],
    ) -> ReducedProjectBusNeighborLink {
        ReducedProjectBusNeighborLink {
            member: live_bus_member_handle_snapshot(&self.member),
            subgraph_index: self.projection_index(live_subgraphs),
        }
    }
}

#[derive(Clone, Debug)]
struct LiveReducedSubgraph {
    #[cfg(test)]
    source_index: usize,
    driver_connection: LiveProjectConnectionHandle,
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

impl LiveReducedSubgraph {
    // Upstream parity: local live-subgraph analogue for the exercised build-time attachment KiCad
    // performs while constructing one live `CONNECTION_SUBGRAPH`. The active handle path now lets
    // the shared subgraph owner attach its own topology, strong drivers, initial pin refresh, and
    // parent item handles instead of sequencing those steps through free functions after handle
    // allocation.
    fn attach_from_reduced(
        &mut self,
        handle: &LiveReducedSubgraphHandle,
        reduced_subgraph: &ReducedProjectSubgraphEntry,
        live_subgraphs: &[LiveReducedSubgraphHandle],
    ) {
        self.attach_topology_from_reduced(reduced_subgraph, live_subgraphs);

        let chosen_identity = reduced_project_subgraph_driver_identity(reduced_subgraph).cloned();
        let chosen_connection = self.driver_connection.clone();
        let live_drivers = self.drivers.clone();

        for (driver, reduced_driver) in live_drivers.iter().zip(reduced_subgraph.drivers.iter()) {
            let identity = reduced_driver.identity.clone();
            let driver_kind = reduced_driver.kind;
            let priority = reduced_driver.priority;
            let floating_connection = match &*driver.borrow() {
                LiveProjectStrongDriverOwner::Floating { connection, .. } => connection.clone(),
                _ => driver.borrow().connection_handle(),
            };
            let owner = self.attach_driver_owner_for_identity(
                identity,
                driver,
                &floating_connection,
                driver_kind,
                priority,
            );
            let mut driver_ref = driver.borrow_mut();
            *driver_ref = owner;
            drop(driver_ref);

            self.attach_strong_driver(driver, chosen_identity.as_ref(), &chosen_connection);
        }

        self.refresh_base_pin_connections_from_driver(false);
        self.attach_item_parent_handles(handle);
    }

    // Upstream parity: local live-subgraph analogue for attaching bus-entry connected-bus
    // ownership during graph build. The active handle path now lets the shared subgraph owner
    // drive bus-entry attachment across the live graph instead of an outer free loop.
    fn attach_connected_bus_items(live_subgraphs: &[LiveReducedSubgraphHandle]) {
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
                item_handle
                    .borrow_mut()
                    .attach_connected_bus_item(&sheet_instance_path, &bus_subgraphs);
            }
        }
    }

    // Upstream parity: local live-subgraph analogue for binding one exercised strong driver onto
    // the shared subgraph owner during driver resolution. This still seeds from reduced projected
    // identities instead of a fuller live `ResolveDrivers()` object graph, but the subgraph owner
    // now owns chosen-driver adoption and chosen-driver-connection attachment instead of leaving
    // that branch open-coded in the surrounding builder. Symbol-pin and text-item branches now
    // compare through attached live owner-side driver connections against the already-seeded live
    // subgraph driver handle. Chosen symbol-pin owners now also alias their item connection onto
    // that chosen driver handle, closer to KiCad's shared `SCH_PIN::Connection()` /
    // `m_driver_connection` path. Remaining divergence is the still-missing fuller live
    // driver-item object graph, not reduced chosen-connection snapshot matching on the active
    // path.
    fn attach_strong_driver(
        &mut self,
        driver: &LiveProjectStrongDriverHandle,
        chosen_identity: Option<&ReducedProjectDriverIdentity>,
        chosen_connection: &LiveProjectConnectionHandle,
    ) {
        let is_chosen_driver = chosen_identity
            .map(|identity| driver.borrow().identity().as_ref() == Some(identity))
            .unwrap_or_else(|| {
                let driver_connection = driver.borrow().connection_handle();
                *driver_connection.borrow() == *chosen_connection.borrow()
            });

        if is_chosen_driver {
            self.chosen_driver = Some(driver.clone());
            self.driver_connection = driver.borrow().connection_handle();
            if let LiveProjectStrongDriverOwner::SymbolPin { owner, .. } = &*driver.borrow() {
                if let Some(base_pin) = owner.upgrade() {
                    base_pin
                        .borrow_mut()
                        .adopt_driver_connection_as_item_connection();
                }
            }
        }
    }

    // Upstream parity: local live-subgraph analogue for binding one reduced strong-driver
    // identity back onto the exercised live item owner it belongs to before chosen-driver
    // selection. This still returns reduced local owner variants instead of fuller live driver
    // items, but the shared subgraph owner now owns the item-match and attachment flow instead of
    // leaving that selection as one large free-function match around the graph.
    fn attach_driver_owner_for_identity(
        &mut self,
        identity: Option<ReducedProjectDriverIdentity>,
        driver: &LiveProjectStrongDriverHandle,
        floating_connection: &LiveProjectConnectionHandle,
        driver_kind: ReducedProjectDriverKind,
        priority: i32,
    ) -> LiveProjectStrongDriverOwner {
        let fallback_identity = identity.clone();
        match identity {
            Some(ReducedProjectDriverIdentity::Label { at, kind, .. }) => {
                if kind == reduced_label_kind_sort_key(LabelKind::Hierarchical) {
                    self.hier_ports
                        .iter()
                        .find(|port| port.borrow().at == at)
                        .map(|port| {
                            port.borrow_mut().attach_strong_driver(
                                port,
                                floating_connection,
                                driver,
                                driver_kind,
                                priority,
                            )
                        })
                        .unwrap_or(LiveProjectStrongDriverOwner::Floating {
                            identity: fallback_identity,
                            connection: floating_connection.clone(),
                            kind: driver_kind,
                            priority,
                        })
                } else {
                    self.label_links
                        .iter()
                        .find(|link| {
                            let link = link.borrow();
                            link.at == at && reduced_label_kind_sort_key(link.kind) == kind
                        })
                        .map(|link| {
                            link.borrow_mut().attach_strong_driver(
                                link,
                                floating_connection,
                                driver,
                                driver_kind,
                                priority,
                            )
                        })
                        .unwrap_or(LiveProjectStrongDriverOwner::Floating {
                            identity: fallback_identity,
                            connection: floating_connection.clone(),
                            kind: driver_kind,
                            priority,
                        })
                }
            }
            Some(ReducedProjectDriverIdentity::SheetPin { at, .. }) => self
                .hier_sheet_pins
                .iter()
                .find(|pin| pin.borrow().at == at)
                .map(|pin| {
                    pin.borrow_mut().attach_strong_driver(
                        pin,
                        floating_connection,
                        driver,
                        driver_kind,
                        priority,
                    )
                })
                .unwrap_or(LiveProjectStrongDriverOwner::Floating {
                    identity: fallback_identity,
                    connection: floating_connection.clone(),
                    kind: driver_kind,
                    priority,
                }),
            Some(ReducedProjectDriverIdentity::SymbolPin {
                symbol_uuid,
                at,
                pin_number,
                ..
            }) => self
                .base_pins
                .iter()
                .find(|base_pin| {
                    let key = &base_pin.borrow().pin.key;
                    key.symbol_uuid.as_ref() == symbol_uuid.as_ref()
                        && key.at == at
                        && key.number.as_ref() == pin_number.as_ref()
                })
                .map(|base_pin| {
                    base_pin.borrow_mut().attach_strong_driver(
                        base_pin,
                        driver,
                        driver_kind,
                        priority,
                    )
                })
                .unwrap_or(LiveProjectStrongDriverOwner::Floating {
                    identity: fallback_identity,
                    connection: floating_connection.clone(),
                    kind: driver_kind,
                    priority,
                }),
            None => LiveProjectStrongDriverOwner::Floating {
                identity: None,
                connection: floating_connection.clone(),
                kind: driver_kind,
                priority,
            },
        }
    }

    // Upstream parity: local live-subgraph analogue for the exercised topology seeding KiCad
    // performs while building one `CONNECTION_SUBGRAPH`. This still seeds from reduced graph
    // indexes instead of fuller pointer-owned items, but the shared subgraph owner now owns its
    // initial parent/neighbor/hierarchy handle attachment instead of leaving that setup in free
    // builder loops around the graph.
    fn attach_topology_from_reduced(
        &mut self,
        reduced_subgraph: &ReducedProjectSubgraphEntry,
        live_subgraphs: &[LiveReducedSubgraphHandle],
    ) {
        for (link, reduced_link) in self
            .bus_neighbor_links
            .iter_mut()
            .zip(reduced_subgraph.bus_neighbor_links.iter())
        {
            link.borrow_mut().subgraph_handle = live_subgraphs
                .get(reduced_link.subgraph_index)
                .map(Rc::downgrade)
                .expect("active live bus-neighbor link requires an attached subgraph handle");
        }

        for (link, reduced_link) in self
            .bus_parent_links
            .iter_mut()
            .zip(reduced_subgraph.bus_parent_links.iter())
        {
            link.borrow_mut().subgraph_handle = live_subgraphs
                .get(reduced_link.subgraph_index)
                .map(Rc::downgrade)
                .expect("active live bus-parent link requires an attached subgraph handle");
        }

        self.bus_parent_handles = reduced_subgraph
            .bus_parent_indexes
            .iter()
            .filter_map(|index| live_subgraphs.get(*index).map(Rc::downgrade))
            .collect();

        self.hier_parent_handle = reduced_subgraph
            .hier_parent_index
            .and_then(|index| live_subgraphs.get(index).map(Rc::downgrade));
        self.hier_child_handles = reduced_subgraph
            .hier_child_indexes
            .iter()
            .filter_map(|index| live_subgraphs.get(*index).map(Rc::downgrade))
            .collect();
    }

    // Upstream parity: local live-subgraph analogue for attaching exercised wire/bus item owners
    // back onto their parent subgraph during graph build. The item payload is still reduced, but
    // the shared subgraph owner now owns the parent-handle and bus-item connection attachment
    // instead of leaving that setup in a separate builder loop.
    fn attach_item_parent_handles(&mut self, handle: &LiveReducedSubgraphHandle) {
        for base_pin in &self.base_pins {
            base_pin.borrow_mut().parent_subgraph_handle = Rc::downgrade(handle);
        }
        for link in &self.label_links {
            link.borrow_mut().parent_subgraph_handle = Rc::downgrade(handle);
        }
        for pin in &self.hier_sheet_pins {
            pin.borrow_mut().parent_subgraph_handle = Rc::downgrade(handle);
        }
        for port in &self.hier_ports {
            port.borrow_mut().parent_subgraph_handle = Rc::downgrade(handle);
        }
        for item in &self.bus_items {
            let mut item_ref = item.borrow_mut();
            item_ref.parent_subgraph_handle = Rc::downgrade(handle);
            item_ref.connection = self.driver_connection.clone();
        }
        for item in &self.wire_items {
            let mut item_ref = item.borrow_mut();
            item_ref.parent_subgraph_handle = Rc::downgrade(handle);
            item_ref.connection = self.driver_connection.clone();
        }
    }

    // Upstream parity: local live-subgraph analogue for the exercised pin portion of
    // `CONNECTION_SUBGRAPH::UpdateItemConnections()`. This still mutates reduced live pin owners
    // instead of real `SCH_PIN`s, but the shared live subgraph owner now drives the current pin
    // refresh flow for both active and compatibility paths.
    fn refresh_base_pin_connections_from_driver(
        &mut self,
        refresh_attached_strong_driver_pins: bool,
    ) {
        let driver_connection = self.driver_connection.clone();
        let driver_connection_type = driver_connection.borrow().connection_type;
        let chosen_driver = self.chosen_driver.clone();

        for base_pin in &self.base_pins {
            base_pin.borrow_mut().refresh_from_driver_connection(
                chosen_driver.as_ref(),
                &driver_connection,
                driver_connection_type,
                refresh_attached_strong_driver_pins,
            );
        }
    }

    // Upstream parity: local live-subgraph analogue for the exercised
    // `CONNECTION_SUBGRAPH::UpdateItemConnections()` item refresh. This still mutates reduced live
    // item owners instead of real `SCH_ITEM*`, but the shared live subgraph owner now drives the
    // label/sheet-pin/hier-port/base-pin refresh flow for both active and compatibility paths.
    fn refresh_item_connections_from_driver(&mut self) {
        let driver_connection = self.driver_connection.clone();
        let driver_connection_type = driver_connection.borrow().connection_type;
        let chosen_driver = self.chosen_driver.clone();

        for link in &self.label_links {
            link.borrow_mut().refresh_from_driver_connection(
                chosen_driver.as_ref(),
                &driver_connection,
                driver_connection_type,
            );
        }
        for pin in &self.hier_sheet_pins {
            pin.borrow_mut().refresh_from_driver_connection(
                chosen_driver.as_ref(),
                &driver_connection,
                driver_connection_type,
            );
        }
        for port in &self.hier_ports {
            port.borrow_mut().refresh_from_driver_connection(
                chosen_driver.as_ref(),
                &driver_connection,
                driver_connection_type,
            );
        }

        self.refresh_base_pin_connections_from_driver(true);
    }

    // Upstream parity: local live-subgraph analogue for the exercised reduced projection boundary
    // after graph mutation. Consumers still read reduced graph state, but the shared live subgraph
    // owner now pushes its resolved connection, chosen-driver state, strong drivers, and item/pin
    // connection owners onto that boundary instead of leaving those projection loops duplicated
    // across active and compatibility paths.
    fn project_driver_and_item_state_onto_reduced(
        &self,
        reduced: &mut ReducedProjectSubgraphEntry,
    ) {
        let live_driver = self.driver_connection.borrow();
        reduced.name = live_driver.name.clone();
        live_driver.project_onto_reduced(&mut reduced.resolved_connection);
        live_driver.project_onto_reduced(&mut reduced.driver_connection);
        reduced.drivers = live_strong_driver_handles_to_snapshots(&self.drivers);
        reduced.chosen_driver_identity = self
            .chosen_driver
            .as_ref()
            .and_then(|driver| driver.borrow().identity());

        for (target, source) in reduced.label_links.iter_mut().zip(self.label_links.iter()) {
            let source = source.borrow();
            source
                .connection
                .borrow()
                .project_onto_reduced(&mut target.connection);
        }

        for (target, source) in reduced
            .hier_sheet_pins
            .iter_mut()
            .zip(self.hier_sheet_pins.iter())
        {
            let source = source.borrow();
            source
                .connection
                .borrow()
                .project_onto_reduced(&mut target.connection);
        }

        for (target, source) in reduced.hier_ports.iter_mut().zip(self.hier_ports.iter()) {
            let source = source.borrow();
            source
                .connection
                .borrow()
                .project_onto_reduced(&mut target.connection);
        }

        for (target, source) in reduced.base_pins.iter_mut().zip(self.base_pins.iter()) {
            let source = source.borrow();
            source
                .connection
                .borrow()
                .project_onto_reduced(&mut target.connection);
            source
                .driver_connection
                .borrow()
                .project_onto_reduced(&mut target.driver_connection);
        }
    }

    // Upstream parity: local live-subgraph analogue for the reduced projection boundary after the
    // active live graph finishes mutating. The shared subgraph owner now also projects its
    // topology and bus-entry attachment back onto the reduced graph instead of leaving those edge
    // loops outside the owner.
    fn project_onto_reduced(
        &self,
        reduced: &mut ReducedProjectSubgraphEntry,
        live_subgraphs: &[LiveReducedSubgraphHandle],
    ) {
        self.project_driver_and_item_state_onto_reduced(reduced);

        reduced.bus_neighbor_links = self
            .bus_neighbor_links
            .iter()
            .map(|link| link.borrow().project_onto_reduced(live_subgraphs))
            .collect();
        reduced.bus_parent_links = self
            .bus_parent_links
            .iter()
            .map(|link| link.borrow().project_onto_reduced(live_subgraphs))
            .collect();
        let (hier_parent_index, hier_child_indexes) =
            reduced_project_hierarchy_indexes_from_live_subgraph(live_subgraphs, self);
        reduced.hier_parent_index = hier_parent_index;
        reduced.hier_child_indexes = hier_child_indexes;
        reduced.bus_parent_indexes =
            reduced_project_bus_parent_indexes_from_live_subgraph(live_subgraphs, self);
        for (target, source) in reduced.wire_items.iter_mut().zip(self.wire_items.iter()) {
            let source = source.borrow();
            target.connected_bus_subgraph_index = source
                .connected_bus_item_handle
                .as_ref()
                .and_then(Weak::upgrade)
                .and_then(|bus| live_subgraph_handle_from_wire_item(&bus))
                .map(|bus| live_subgraph_projection_index(live_subgraphs, &bus));
        }
    }

    // Upstream parity: local live-subgraph analogue for the exercised post-propagation
    // `UpdateItemConnections()` follow-up branches KiCad runs after names settle. This still
    // operates on reduced live owners instead of fuller item pointers, but the shared subgraph
    // owner now owns the self-driven symbol-pin no-connect rename refresh and the self-driven
    // sheet-pin child-bus promotion branch instead of leaving those branches open-coded in the
    // outer handle loop.
    fn refresh_post_propagation_item_connections(handle: &LiveReducedSubgraphHandle) {
        sync_live_reduced_item_connections_from_driver_handle(handle);

        if {
            let subgraph = handle.borrow();
            live_subgraph_is_self_driven_symbol_pin(&subgraph)
                && subgraph.driver_connection.borrow().name.contains("Net-(")
        } {
            let chosen_driver = {
                let subgraph = handle.borrow();
                subgraph
                    .driver_connection
                    .borrow_mut()
                    .force_no_connect_name();
                subgraph.chosen_driver.clone()
            };
            if let Some(driver) = chosen_driver {
                if let LiveProjectStrongDriverOwner::SymbolPin { owner, .. } = &*driver.borrow() {
                    if let Some(base_pin) = owner.upgrade() {
                        base_pin.borrow_mut().refresh_from_owned_driver_connection();
                    }
                }
            }
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

    // Upstream parity: local live-subgraph analogue for the hierarchy-chain slice inside
    // `propagateToNeighbors()`. This still mutates reduced live carriers instead of full local
    // `CONNECTION_SUBGRAPH` objects, but the shared subgraph owner now owns the traversal and
    // chosen-driver rewrite for one hierarchy-connected component, and that rewrite now stays on
    // the chosen live driver handle instead of snapshotting a reduced-shaped chosen connection
    // through the active propagation path.
    fn propagate_hierarchy_chain(start: &LiveReducedSubgraphHandle, force: bool) {
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
        let mut best_name = start.borrow().driver_connection.borrow().name.clone();

        if highest < 6 {
            for handle in visited.iter().filter(|handle| !Rc::ptr_eq(handle, start)) {
                let priority = live_reduced_subgraph_driver_priority(&handle.borrow());
                let candidate_strong = priority >= 3;
                let candidate_name = handle.borrow().driver_connection.borrow().name.clone();
                let candidate_depth =
                    reduced_sheet_path_depth(&handle.borrow().sheet_instance_path);
                let best_depth =
                    reduced_sheet_path_depth(&best_handle.borrow().sheet_instance_path);
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
            let changed = {
                let chosen_connection_ref = chosen_connection.borrow();
                !live_connection_clone_eq(
                    &subgraph.driver_connection.borrow(),
                    &chosen_connection_ref,
                )
            };
            if changed {
                clone_live_connection_owner_into_live_connection_owner(
                    &mut subgraph.driver_connection.borrow_mut(),
                    &chosen_connection.borrow(),
                );
            }
            subgraph.dirty = changed;
        }
    }

    // Upstream parity: local live-subgraph analogue for collecting the connected propagation
    // component around one dirty subgraph. Active component discovery now belongs to the shared
    // subgraph owner and follows attached hierarchy and bus handles directly instead of leaving
    // traversal identity in an outer free helper keyed by reduced indexes.
    fn collect_propagation_component_handles(
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
                let Some(neighbor_handle) = live_subgraph_handle_for_link(live_subgraphs, &link)
                else {
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

    // Upstream parity: local live-subgraph analogue for the global-secondary-driver promotion
    // branch KiCad runs before neighbor propagation. The active path now keeps the promotion walk
    // on the shared subgraph owner instead of an outer free helper around the handle graph.
    fn refresh_global_secondary_driver_promotions(
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
            if secondary_driver.borrow().full_name() == chosen_connection.borrow().name {
                continue;
            }

            let secondary_is_global = secondary_driver.borrow().priority() >= 6;

            for handle in live_subgraphs.iter() {
                if Rc::ptr_eq(handle, start) {
                    continue;
                }

                if !secondary_is_global && handle.borrow().sheet_instance_path != start_sheet {
                    continue;
                }

                if !handle.borrow().drivers.iter().any(|candidate_driver| {
                    candidate_driver.borrow().full_name() == secondary_driver.borrow().full_name()
                }) {
                    continue;
                }

                let same_connection = {
                    let handle_ref = handle.borrow();
                    live_connection_clone_eq(
                        &handle_ref.driver_connection.borrow(),
                        &chosen_connection.borrow(),
                    )
                };
                if same_connection {
                    continue;
                }

                clone_live_connection_owner_into_live_connection_owner(
                    &mut handle.borrow().driver_connection.borrow_mut(),
                    &chosen_connection.borrow(),
                );
                sync_live_reduced_item_connections_from_driver_handle(handle);
                handle.borrow_mut().dirty = true;
                promoted.push(handle.clone());
            }
        }

        promoted.sort_by_key(live_subgraph_handle_id);
        promoted.dedup_by(|left, right| Rc::ptr_eq(left, right));
        promoted
    }

    // Upstream parity: local live-subgraph analogue for the active recursive
    // `propagateToNeighbors()` walk. This still runs on reduced live carriers instead of the final
    // local `CONNECTION_SUBGRAPH` analogue, but the shared subgraph owner now owns recursive dirty
    // traversal, component discovery, hierarchy propagation, secondary-driver promotion, and
    // revisit scheduling instead of coordinating those steps from free helpers around the graph.
    fn propagate_neighbors(
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
            let promoted = Self::refresh_global_secondary_driver_promotions(start, live_subgraphs);

            for promoted_handle in promoted {
                Self::propagate_neighbors(
                    &promoted_handle,
                    live_subgraphs,
                    false,
                    visiting,
                    stale_members,
                );
            }
        }

        let active = Self::collect_propagation_component_handles(start, live_subgraphs);
        let dirty_active = active
            .iter()
            .filter(|handle| handle.borrow().dirty)
            .cloned()
            .collect::<Vec<_>>();

        for handle in &dirty_active {
            handle.borrow_mut().dirty = false;
        }

        for handle in &dirty_active {
            let has_hierarchy_links = live_subgraph_has_hierarchy_handles_from_handle(handle);

            if !has_hierarchy_links {
                continue;
            }

            Self::propagate_hierarchy_chain(handle, force);
        }
        Self::refresh_bus_neighbor_drivers(live_subgraphs, &dirty_active, stale_members);
        Self::refresh_bus_parent_members(live_subgraphs, &dirty_active);
        Self::replay_stale_bus_members(&active, stale_members);
        Self::refresh_bus_link_members(live_subgraphs, &active);

        let recurse_targets = live_subgraphs
            .iter()
            .filter(|handle| !Rc::ptr_eq(handle, start) && handle.borrow().dirty)
            .cloned()
            .collect::<Vec<_>>();

        visiting.remove(&start_id);
        for handle in recurse_targets {
            Self::propagate_neighbors(&handle, live_subgraphs, force, visiting, stale_members);
        }

        if start.borrow().dirty {
            Self::propagate_neighbors(start, live_subgraphs, force, visiting, stale_members);
        }
    }

    // Upstream parity: local live-subgraph analogue for the repeated dirty-root walk KiCad drives
    // from live subgraphs during graph build. The active handle path now keeps that root loop on
    // the shared subgraph owner instead of a free outer coordinator around the graph.
    fn run_dirty_roots(live_subgraphs: &[LiveReducedSubgraphHandle], force: bool) {
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
            Self::propagate_neighbors(
                start,
                live_subgraphs,
                force,
                &mut visiting,
                &mut stale_members,
            );
        }
    }

    // Upstream parity: local live-subgraph analogue for the bus-neighbor mutation branch inside
    // `propagateToNeighbors()`. The active recursive walk now keeps this driver/member promotion
    // step on the shared subgraph owner instead of a free helper around the handle graph.
    fn refresh_bus_neighbor_drivers(
        live_subgraphs: &[LiveReducedSubgraphHandle],
        component: &[LiveReducedSubgraphHandle],
        stale_members: &mut Vec<LiveProjectBusMemberHandle>,
    ) {
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
                let Some(neighbor_handle) = live_subgraph_handle_for_link(live_subgraphs, &link)
                else {
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
                    parent_connection.find_member_live(&current_link_member.borrow())
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
                let neighbor_sheet_instance_path =
                    neighbor_handle.borrow().sheet_instance_path.clone();

                if neighbor_connection_sheet != neighbor_sheet_instance_path {
                    if neighbor_connection_sheet != parent_sheet_instance_path {
                        continue;
                    }

                    let parent_has_search = {
                        let parent = parent_handle.borrow();
                        let parent_connection = parent.driver_connection.borrow();
                        parent_connection
                            .find_member_for_connection(&promoted_connection)
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
                        let Some(member_handle) =
                            parent_connection.find_member_mut_live(&current_link_member.borrow())
                        else {
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

    // Upstream parity: local live-subgraph analogue for refreshing parent-bus members from dirty
    // child net connections during the active recursive walk.
    fn refresh_bus_parent_members(
        live_subgraphs: &[LiveReducedSubgraphHandle],
        component: &[LiveReducedSubgraphHandle],
    ) {
        for child_handle in component {
            let child_connection = child_handle.borrow().driver_connection.clone();
            if child_connection.borrow().connection_type != ReducedProjectConnectionType::Net {
                continue;
            }

            let child_sheet_instance_path = child_handle.borrow().sheet_instance_path.clone();
            let parent_links = child_handle.borrow().bus_parent_links.clone();

            for link in parent_links {
                let Some(parent_handle) = live_subgraph_handle_for_link(live_subgraphs, &link)
                else {
                    continue;
                };
                let child_connection_sheet = child_connection.borrow().sheet_instance_path.clone();
                let parent_sheet_instance_path = parent_handle.borrow().sheet_instance_path.clone();
                if child_connection_sheet != child_sheet_instance_path
                    && child_connection_sheet != parent_sheet_instance_path
                {
                    continue;
                }

                if parent_handle
                    .borrow()
                    .driver_connection
                    .borrow()
                    .find_member_for_connection(&child_connection.borrow())
                    .is_some()
                {
                    continue;
                }

                let changed = {
                    let parent = parent_handle.borrow();
                    let mut parent_connection = parent.driver_connection.borrow_mut();
                    let link_member = link.borrow().member.clone();
                    let Some(member) =
                        parent_connection.find_member_mut_live(&link_member.borrow())
                    else {
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

    // Upstream parity: local live-subgraph analogue for replaying stale bus members across the
    // active recursive live graph after neighbor and parent refresh mutate bus members in place.
    fn replay_stale_bus_members(
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
                    let Some(member) = connection.find_member_mut_live(&stale_member.borrow())
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

    // Upstream parity: local live-subgraph analogue for rematching bus parent/neighbor links
    // after propagation. The active handle path already mutates shared link owners in place; the
    // shared subgraph owner now also owns that post-pass rematch control flow instead of leaving
    // it in a free helper around the graph.
    fn refresh_bus_link_members(
        live_subgraphs: &[LiveReducedSubgraphHandle],
        component: &[LiveReducedSubgraphHandle],
    ) {
        let mut refreshed_parent_links =
            BTreeMap::<usize, Vec<LiveReducedSubgraphLinkHandle>>::new();

        for child_handle in component {
            let child_id = live_subgraph_handle_id(child_handle);
            let child_connection = child_handle.borrow().driver_connection.clone();
            let existing_parent_links = child_handle.borrow().bus_parent_links.clone();

            let mut parent_handles = live_subgraph_bus_parent_handles_from_handle(child_handle)
                .into_iter()
                .collect::<Vec<_>>();
            for link in &existing_parent_links {
                let Some(parent_handle) = live_subgraph_handle_for_link(live_subgraphs, link)
                else {
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
                        parent_handle
                            .borrow()
                            .driver_connection
                            .borrow()
                            .find_member_live(&search.borrow())
                    })
                    .or_else(|| {
                        parent_neighbor_member.as_ref().and_then(|search| {
                            parent_handle
                                .borrow()
                                .driver_connection
                                .borrow()
                                .find_member_live(&search.borrow())
                        })
                    })
                    .or_else(|| {
                        parent_handle
                            .borrow()
                            .driver_connection
                            .borrow()
                            .find_member_for_connection(&child_connection.borrow())
                    });

                let Some(refreshed_member) = refreshed_member else {
                    continue;
                };

                let existing_link = existing_parent_links.iter().find(|link| {
                    live_subgraph_handle_for_link(live_subgraphs, link)
                        .as_ref()
                        .is_some_and(|candidate| Rc::ptr_eq(candidate, &parent_handle))
                });

                refreshed_parent_links.entry(child_id).or_default().push(
                    update_live_subgraph_link_handle(
                        existing_link,
                        refreshed_member,
                        &parent_handle,
                    ),
                );
            }
        }

        let mut refreshed_neighbor_links =
            BTreeMap::<usize, Vec<LiveReducedSubgraphLinkHandle>>::new();

        for child_handle in component {
            let child_id = live_subgraph_handle_id(child_handle);
            for link in refreshed_parent_links.get(&child_id).into_iter().flatten() {
                let Some(neighbor_handle) = live_subgraph_handle_for_link(live_subgraphs, link)
                else {
                    continue;
                };
                let existing_neighbor_links = neighbor_handle.borrow().bus_neighbor_links.clone();
                let existing_link = existing_neighbor_links.iter().find(|candidate| {
                    live_subgraph_handle_for_link(live_subgraphs, candidate)
                        .as_ref()
                        .is_some_and(|existing_child| Rc::ptr_eq(existing_child, child_handle))
                });
                refreshed_neighbor_links
                    .entry(live_subgraph_handle_id(&neighbor_handle))
                    .or_default()
                    .push(update_live_subgraph_link_handle(
                        existing_link,
                        link.borrow().member.clone(),
                        child_handle,
                    ));
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

    // Upstream parity: local live-subgraph analogue for the multiple-parent rename/recache pass
    // KiCad runs before rebuilding final caches. The shared subgraph owner now also owns that
    // post-pass rename control flow instead of leaving it in a free helper around the handle graph.
    fn refresh_multiple_bus_parent_names(live_subgraphs: &[LiveReducedSubgraphHandle]) {
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
                let Some(parent_handle) = live_subgraph_handle_for_link(live_subgraphs, &link)
                else {
                    continue;
                };
                let old_name = {
                    let parent = parent_handle.borrow();
                    let mut parent_connection = parent.driver_connection.borrow_mut();
                    let link_member = link.borrow().member.clone();
                    let Some(member) =
                        parent_connection.find_member_mut_live(&link_member.borrow())
                    else {
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
                    let old_candidate_name = candidate_handle
                        .borrow()
                        .driver_connection
                        .borrow()
                        .name
                        .clone();
                    if old_candidate_name == old_name {
                        let changed = {
                            let candidate = candidate_handle.borrow();
                            let changed = !live_connection_clone_eq(
                                &candidate.driver_connection.borrow(),
                                &connection.borrow(),
                            );
                            clone_live_connection_owner_into_live_connection_owner(
                                &mut candidate.driver_connection.borrow_mut(),
                                &connection.borrow(),
                            );
                            changed
                        };
                        if changed {
                            sync_live_reduced_item_connections_from_driver_handle(
                                &candidate_handle,
                            );
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

    // Upstream parity: local live-subgraph analogue for the same-name cache keys KiCad rebuilds
    // around propagated `CONNECTION_SUBGRAPH`s. The graph still keeps reduced cache maps instead
    // of full live subgraph objects, but the shared live subgraph owner now decides which name and
    // prefix entries belong in those caches instead of free helpers rebuilding keys around it.
    fn cache_name(&self) -> String {
        self.driver_connection.borrow().name.clone()
    }

    fn cache_prefix_name(&self) -> Option<String> {
        let name = self.cache_name();
        name.contains('[')
            .then(|| format!("{}[]", name.split('[').next().unwrap_or("")))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn insert_into_index_name_caches(
        &self,
        subgraphs_by_name: &mut BTreeMap<String, Vec<usize>>,
        subgraphs_by_sheet_and_name: &mut BTreeMap<(String, String), Vec<usize>>,
        subgraph_index: usize,
    ) {
        let name = self.cache_name();
        subgraphs_by_name
            .entry(name.clone())
            .or_default()
            .push(subgraph_index);

        if let Some(prefix_only) = self.cache_prefix_name() {
            subgraphs_by_name
                .entry(prefix_only)
                .or_default()
                .push(subgraph_index);
        }

        subgraphs_by_sheet_and_name
            .entry((self.sheet_instance_path.clone(), name))
            .or_default()
            .push(subgraph_index);
    }

    fn insert_into_handle_name_caches(
        &self,
        subgraphs_by_name: &mut BTreeMap<String, Vec<LiveReducedSubgraphHandle>>,
        subgraphs_by_sheet_and_name: &mut BTreeMap<
            (String, String),
            Vec<LiveReducedSubgraphHandle>,
        >,
        subgraph_handle: &LiveReducedSubgraphHandle,
    ) {
        let name = self.cache_name();
        subgraphs_by_name
            .entry(name.clone())
            .or_default()
            .push(subgraph_handle.clone());

        if let Some(prefix_only) = self.cache_prefix_name() {
            subgraphs_by_name
                .entry(prefix_only)
                .or_default()
                .push(subgraph_handle.clone());
        }

        subgraphs_by_sheet_and_name
            .entry((self.sheet_instance_path.clone(), name))
            .or_default()
            .push(subgraph_handle.clone());
    }
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

// Upstream parity: reduced live-pin snapshot helper for the shared graph owner. This still
// projects back to reduced pin payload instead of a live `SCH_PIN`, but it now keeps the fuller
// reduced base-pin payload on the live owner, including schematic-path identity, instead of
// collapsing the active pin carrier down to only its key before comparison or projection helpers
// use it.
fn live_base_pin_handle_snapshot(base_pin: &LiveReducedBasePinHandle) -> ReducedProjectBasePin {
    let base_pin = base_pin.borrow();
    ReducedProjectBasePin {
        schematic_path: base_pin.pin.schematic_path.clone(),
        key: base_pin.pin.key.clone(),
        number: base_pin.pin.number.clone(),
        electrical_type: base_pin.pin.electrical_type.clone(),
        connection: base_pin.connection.borrow().snapshot(),
        driver_connection: base_pin.driver_connection.borrow().snapshot(),
        preserve_local_name_on_refresh: base_pin.preserve_local_name_on_refresh,
    }
}

fn live_base_pin_handles_to_snapshots(
    base_pins: &[LiveReducedBasePinHandle],
) -> Vec<ReducedProjectBasePin> {
    base_pins
        .iter()
        .map(live_base_pin_handle_snapshot)
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

// Upstream parity: local live-owner helper for preserving one shared link carrier while the
// active graph rematches parent/neighbor topology after propagation. KiCad mutates one
// `CONNECTION_SUBGRAPH` link graph in place; this reduced bridge still carries a narrower link
// wrapper, but it now updates that shared wrapper instead of recreating fresh link values on each
// rematch pass.
fn update_live_subgraph_link_handle(
    existing: Option<&LiveReducedSubgraphLinkHandle>,
    member: LiveProjectBusMemberHandle,
    target_handle: &LiveReducedSubgraphHandle,
) -> LiveReducedSubgraphLinkHandle {
    if let Some(existing) = existing {
        let existing_handle = existing.clone();
        let mut existing = existing_handle.borrow_mut();
        existing.member = member;
        #[cfg(test)]
        {
            existing.subgraph_index = target_handle.borrow().source_index;
        }
        existing.subgraph_handle = Rc::downgrade(target_handle);
        drop(existing);
        return existing_handle;
    }

    Rc::new(RefCell::new(LiveReducedSubgraphLink {
        member,
        #[cfg(test)]
        subgraph_index: target_handle.borrow().source_index,
        subgraph_handle: Rc::downgrade(target_handle),
    }))
}

fn live_subgraph_strong_driver_count(subgraph: &LiveReducedSubgraph) -> usize {
    subgraph.drivers.len()
}

fn live_subgraph_has_local_driver(subgraph: &LiveReducedSubgraph) -> bool {
    live_reduced_subgraph_driver_priority(subgraph) < 6
}

fn live_subgraph_base_pin_count(subgraph: &LiveReducedSubgraph) -> usize {
    subgraph.base_pins.len()
}

fn live_reduced_subgraph_driver_priority(subgraph: &LiveReducedSubgraph) -> i32 {
    subgraph
        .drivers
        .first()
        .map(|driver| driver.borrow().priority())
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
// local `CONNECTION_SUBGRAPH` analogue, but it now constructs the active graph directly as shared
// live handles instead of first building a temporary value-owned live-subgraph vector. Base-pin
// item and pin-driver connection owners now also seed from their distinct reduced owners here
// instead of collapsing the pin-driver side back onto the item connection during handle
// construction.
fn build_live_reduced_subgraph_handles(
    reduced_subgraphs: &[ReducedProjectSubgraphEntry],
) -> Vec<LiveReducedSubgraphHandle> {
    let handles = reduced_subgraphs
        .iter()
        .enumerate()
        .map(|(_index, subgraph)| {
            let live_driver_connection = Rc::new(RefCell::new(
                reduced_subgraph_driver_connection(subgraph).into(),
            ));
            Rc::new(RefCell::new(LiveReducedSubgraph {
                #[cfg(test)]
                source_index: _index,
                driver_connection: live_driver_connection.clone(),
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
                            subgraph_handle: Weak::new(),
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
                            subgraph_handle: Weak::new(),
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
                                schematic_path: pin.schematic_path.clone(),
                                key: pin.key.clone(),
                                number: pin.number.clone(),
                                electrical_type: pin.electrical_type.clone(),
                            },
                            connection: Rc::new(RefCell::new(pin.connection.clone().into())),
                            driver_connection: Rc::new(RefCell::new(
                                pin.driver_connection.clone().into(),
                            )),
                            driver: None,
                            preserve_local_name_on_refresh: pin.preserve_local_name_on_refresh,
                            parent_subgraph_handle: Weak::new(),
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
                            schematic_path: link.schematic_path.clone(),
                            at: link.at,
                            kind: link.kind,
                            connection: Rc::new(RefCell::new(link.connection.clone().into())),
                            driver_connection: empty_live_project_connection_handle(),
                            driver: None,
                            parent_subgraph_handle: Weak::new(),
                        }))
                    })
                    .collect(),
                hier_sheet_pins: subgraph
                    .hier_sheet_pins
                    .iter()
                    .cloned()
                    .map(|pin| {
                        Rc::new(RefCell::new(LiveReducedHierSheetPinLink {
                            schematic_path: pin.schematic_path.clone(),
                            at: pin.at,
                            child_sheet_uuid: pin.child_sheet_uuid,
                            connection: Rc::new(RefCell::new(pin.connection.clone().into())),
                            driver_connection: empty_live_project_connection_handle(),
                            driver: None,
                            parent_subgraph_handle: Weak::new(),
                        }))
                    })
                    .collect(),
                hier_ports: subgraph
                    .hier_ports
                    .iter()
                    .cloned()
                    .map(|port| {
                        Rc::new(RefCell::new(LiveReducedHierPortLink {
                            schematic_path: port.schematic_path.clone(),
                            at: port.at,
                            connection: Rc::new(RefCell::new(port.connection.clone().into())),
                            driver_connection: empty_live_project_connection_handle(),
                            driver: None,
                            parent_subgraph_handle: Weak::new(),
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
                            connection: live_driver_connection.clone(),
                            connected_bus_item_handle: None,
                            parent_subgraph_handle: Weak::new(),
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
                            connection: live_driver_connection.clone(),
                            connected_bus_item_handle: None,
                            parent_subgraph_handle: Weak::new(),
                        }))
                    })
                    .collect(),
                dirty: true,
            }))
        })
        .collect::<Vec<_>>();
    for (index, handle) in handles.iter().enumerate() {
        handle
            .borrow_mut()
            .attach_from_reduced(handle, &reduced_subgraphs[index], &handles);
    }
    LiveReducedSubgraph::attach_connected_bus_items(&handles);
    handles
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
    handle.borrow_mut().refresh_item_connections_from_driver();
}

fn live_subgraph_link_index(
    live_subgraphs: &[LiveReducedSubgraphHandle],
    link: &LiveReducedSubgraphLinkHandle,
) -> usize {
    link.borrow().projection_index(live_subgraphs)
}

// Upstream parity: active live graph traversal should follow the attached live subgraph topology,
// not copied reduced indexes. This helper is now handle-only on the active path; copied reduced
// target indexes remain only in the test build and at projection boundaries.
fn live_subgraph_handle_for_link(
    _live_subgraphs: &[LiveReducedSubgraphHandle],
    link: &LiveReducedSubgraphLinkHandle,
) -> Option<LiveReducedSubgraphHandle> {
    Weak::upgrade(&link.borrow().subgraph_handle)
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
    Weak::upgrade(&item.borrow().parent_subgraph_handle)
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

// Upstream parity: local bridge for projecting the active shared live subgraph owner back onto
// the reduced graph query surface. This still ends in a reduced projection because consumers do
// not yet keep live item/subgraph pointers, but the active recursive graph build now mutates one
// shared live subgraph object graph before that projection. Base-pin live connection owners now
// also project back onto the reduced subgraph payload directly, so the reduced graph boundary no
// longer drops exercised per-pin connection updates from the active live graph. Remaining
// divergence is that callers still consume reduced indices instead of live
// item/subgraph pointers, so this projection must collapse bus-entry attachment back to source
// indexes at the edge. Active live bus-entry items no longer carry a copied reduced bus index
// alongside that live owner, and those wire-item owners are now shared handles on the live graph
// instead of copied value wrappers. Reduced strong-driver snapshots now also refresh from the
// bound live owners here, and any already-seeded chosen-driver identity now stays attached to the
// chosen live owner instead of being left behind on the pre-live reduced subgraph. That keeps ERC
// and graph queries from reading stale reduced driver metadata after the active graph has already
// attached item owners.
fn apply_live_reduced_driver_connections_from_handles(
    reduced_subgraphs: &mut [ReducedProjectSubgraphEntry],
    live_subgraphs: &[LiveReducedSubgraphHandle],
) {
    for (index, handle) in live_subgraphs.iter().enumerate() {
        let live = handle.borrow();
        let reduced = &mut reduced_subgraphs[index];
        live.project_onto_reduced(reduced, live_subgraphs);
    }
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
        subgraph.insert_into_index_name_caches(
            &mut subgraphs_by_name,
            &mut subgraphs_by_sheet_and_name,
            index,
        );
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
        subgraph.insert_into_handle_name_caches(
            &mut subgraphs_by_name,
            &mut subgraphs_by_sheet_and_name,
            handle,
        );
    }

    (subgraphs_by_name, subgraphs_by_sheet_and_name)
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
    subgraph.insert_into_index_name_caches(
        subgraphs_by_name,
        subgraphs_by_sheet_and_name,
        subgraph_index,
    );
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

    subgraph.insert_into_handle_name_caches(
        subgraphs_by_name,
        subgraphs_by_sheet_and_name,
        subgraph_handle,
    );
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
// Upstream parity: reduced local analogue for the bus-neighbor branch inside
// `CONNECTION_GRAPH::propagateToNeighbors()`. This is still not a 1:1 live KiCad graph walk
// because the Rust tree does not yet recurse stale members and item-owned connections on the same
// live objects, but it now mutates chosen bus-member and neighbor driver connections on a shared
// live subgraph owner before projecting them back onto the reduced graph. Remaining divergence is
// the later stale-member replay / recache recursion that still falls back to the reduced fixpoint.
#[cfg(test)]
fn refresh_reduced_live_bus_neighbor_drivers(
    reduced_subgraphs: &mut [ReducedProjectSubgraphEntry],
) {
    let live_subgraphs = build_live_reduced_subgraph_handles(reduced_subgraphs);
    let all_indexes = (0..live_subgraphs.len()).collect::<Vec<_>>();
    let mut stale_members = Vec::new();
    refresh_reduced_live_bus_neighbor_drivers_on_handles_for_indexes(
        &live_subgraphs,
        &all_indexes,
        &mut stale_members,
    );
    replay_reduced_live_stale_bus_members_on_handles_for_indexes(
        &live_subgraphs,
        &all_indexes,
        &stale_members,
    );
    apply_live_reduced_driver_connections_from_handles(reduced_subgraphs, &live_subgraphs);
}

// Upstream parity: reduced local analogue for the stale-member update KiCad performs after a bus
// neighbor or hierarchy child settles on a final net connection. This still stops short of the
// full live `stale_bus_members` replay because it does not recursively revisit every affected bus
// subgraph on the same object graph, but it does move the direct child-net -> parent-bus member
// mutation onto the shared live subgraph owner before the reduced cleanup passes.
#[cfg(test)]
fn refresh_reduced_live_bus_parent_members(reduced_subgraphs: &mut [ReducedProjectSubgraphEntry]) {
    let live_subgraphs = build_live_reduced_subgraph_handles(reduced_subgraphs);
    let component = live_subgraphs.iter().cloned().collect::<Vec<_>>();
    refresh_reduced_live_bus_parent_members_on_handles_for_component(&live_subgraphs, &component);
    apply_live_reduced_driver_connections_from_handles(reduced_subgraphs, &live_subgraphs);
}

// Upstream parity: reduced local analogue for the multiple-parent rename/recache branch KiCad
// runs before the final graph caches are rebuilt. This still projects back onto the reduced graph
// instead of mutating live name indexes in place, but it moves the parent-member clone and
// same-name subgraph rename onto the shared live subgraph owner before the reduced cache rebuild,
// and now mutates the existing live connection owner on that compatibility path instead of
// swapping in a fresh rebuilt connection value.
#[cfg(test)]
fn refresh_reduced_live_multiple_bus_parent_names(
    reduced_subgraphs: &mut [ReducedProjectSubgraphEntry],
) {
    let live_subgraphs = build_live_reduced_subgraph_handles(reduced_subgraphs);
    LiveReducedSubgraph::refresh_multiple_bus_parent_names(&live_subgraphs);
    apply_live_reduced_driver_connections_from_handles(reduced_subgraphs, &live_subgraphs);
}

// Upstream parity: reduced local analogue for the post-remap bus-link refresh KiCad gets from
// matching live bus members on the same propagated subgraph objects. This still projects the
// remapped links back onto reduced vectors, but it moves the member rematch step onto the shared
// live driver connections before the reduced cache rebuild.
#[cfg(test)]
fn refresh_reduced_live_bus_link_members(reduced_subgraphs: &mut [ReducedProjectSubgraphEntry]) {
    let live_subgraphs = build_live_reduced_subgraph_handles(reduced_subgraphs);
    let all_indexes = (0..live_subgraphs.len()).collect::<Vec<_>>();
    let component = all_indexes
        .iter()
        .filter_map(|index| live_subgraphs.get(*index).cloned())
        .collect::<Vec<_>>();
    LiveReducedSubgraph::refresh_bus_link_members(&live_subgraphs, &component);
    apply_live_reduced_driver_connections_from_handles(reduced_subgraphs, &live_subgraphs);
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
#[cfg(test)]
fn collect_live_reduced_propagation_component_from_handles(
    start: usize,
    live_subgraphs: &[LiveReducedSubgraphHandle],
) -> Vec<usize> {
    LiveReducedSubgraph::collect_propagation_component_handles(
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
#[cfg(test)]
fn refresh_reduced_live_bus_neighbor_drivers_on_handles_for_component(
    live_subgraphs: &[LiveReducedSubgraphHandle],
    component: &[LiveReducedSubgraphHandle],
    stale_members: &mut Vec<LiveProjectBusMemberHandle>,
) {
    LiveReducedSubgraph::refresh_bus_neighbor_drivers(live_subgraphs, component, stale_members);
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

#[cfg(test)]
fn refresh_reduced_live_bus_parent_members_on_handles_for_component(
    live_subgraphs: &[LiveReducedSubgraphHandle],
    component: &[LiveReducedSubgraphHandle],
) {
    LiveReducedSubgraph::refresh_bus_parent_members(live_subgraphs, component);
}

#[cfg(test)]
fn replay_reduced_live_stale_bus_members_on_handles_for_component(
    component: &[LiveReducedSubgraphHandle],
    stale_members: &[LiveProjectBusMemberHandle],
) {
    LiveReducedSubgraph::replay_stale_bus_members(component, stale_members);
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

#[cfg(test)]
fn refresh_reduced_live_bus_link_members_on_handles_for_component(
    live_subgraphs: &[LiveReducedSubgraphHandle],
    component: &[LiveReducedSubgraphHandle],
) {
    // Upstream parity: local live-handle analogue for rematching bus parent/neighbor links after
    // propagation. Active refresh now prefers the attached live parent/child handles, shared live
    // connection owners, handle-keyed refresh state, and the existing shared live link owners over
    // copied link or subgraph indexes. This helper still uses a narrower local link wrapper than a
    // full `CONNECTION_SUBGRAPH` neighbor object, but active rematch now mutates those shared link
    // owners in place instead of rebuilding fresh link wrappers on each pass, and still marks the
    // live owner dirty when attached topology changes so recursive revisits follow live dirty-state
    // directly instead of whole-subgraph equality checks.
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
                    parent_handle
                        .borrow()
                        .driver_connection
                        .borrow()
                        .find_member_live(&search.borrow())
                })
                .or_else(|| {
                    parent_neighbor_member.as_ref().and_then(|search| {
                        parent_handle
                            .borrow()
                            .driver_connection
                            .borrow()
                            .find_member_live(&search.borrow())
                    })
                })
                .or_else(|| {
                    parent_handle
                        .borrow()
                        .driver_connection
                        .borrow()
                        .find_member_for_connection(&child_connection.borrow())
                });

            let Some(refreshed_member) = refreshed_member else {
                continue;
            };

            let existing_link = existing_parent_links.iter().find(|link| {
                live_subgraph_handle_for_link(live_subgraphs, link)
                    .as_ref()
                    .is_some_and(|candidate| Rc::ptr_eq(candidate, &parent_handle))
            });

            refreshed_parent_links.entry(child_id).or_default().push(
                update_live_subgraph_link_handle(existing_link, refreshed_member, &parent_handle),
            );
        }
    }

    let mut refreshed_neighbor_links = BTreeMap::<usize, Vec<LiveReducedSubgraphLinkHandle>>::new();

    for child_handle in component {
        let child_id = live_subgraph_handle_id(child_handle);
        for link in refreshed_parent_links.get(&child_id).into_iter().flatten() {
            let Some(neighbor_handle) = live_subgraph_handle_for_link(live_subgraphs, link) else {
                continue;
            };
            let existing_neighbor_links = neighbor_handle.borrow().bus_neighbor_links.clone();
            let existing_link = existing_neighbor_links.iter().find(|candidate| {
                live_subgraph_handle_for_link(live_subgraphs, candidate)
                    .as_ref()
                    .is_some_and(|existing_child| Rc::ptr_eq(existing_child, child_handle))
            });
            refreshed_neighbor_links
                .entry(live_subgraph_handle_id(&neighbor_handle))
                .or_default()
                .push(update_live_subgraph_link_handle(
                    existing_link,
                    link.borrow().member.clone(),
                    child_handle,
                ));
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

#[cfg(test)]
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

fn refresh_reduced_live_post_propagation_item_connections_on_handles(
    live_subgraphs: &[LiveReducedSubgraphHandle],
) {
    for handle in live_subgraphs {
        LiveReducedSubgraph::refresh_post_propagation_item_connections(handle);
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
    LiveReducedSubgraph::run_dirty_roots(&live_subgraphs, false);
    LiveReducedSubgraph::run_dirty_roots(&live_subgraphs, true);

    let all_indexes = (0..live_subgraphs.len()).collect::<Vec<_>>();
    LiveReducedSubgraph::refresh_multiple_bus_parent_names(&live_subgraphs);
    LiveReducedSubgraph::run_dirty_roots(&live_subgraphs, false);
    LiveReducedSubgraph::run_dirty_roots(&live_subgraphs, true);
    let component = all_indexes
        .iter()
        .filter_map(|index| live_subgraphs.get(*index).cloned())
        .collect::<Vec<_>>();
    LiveReducedSubgraph::refresh_bus_link_members(&live_subgraphs, &component);
    refresh_reduced_live_post_propagation_item_connections_on_handles(&live_subgraphs);
    apply_live_reduced_driver_connections_from_handles(reduced_subgraphs, &live_subgraphs);
    live_subgraphs
}

// Upstream parity: reduced local analogue for the post-propagation item-connection update KiCad
// performs after subgraph names settle. The active and compatibility test paths now both run this
// through the shared-handle live graph instead of a second value-owned live-subgraph pass.
// Remaining divergence is the still-missing fuller live item/pointer graph behind those handles.
#[cfg(test)]
fn refresh_reduced_live_post_propagation_item_connections(
    reduced_subgraphs: &mut [ReducedProjectSubgraphEntry],
) {
    let live_subgraphs = build_live_reduced_subgraph_handles(reduced_subgraphs);
    refresh_reduced_live_post_propagation_item_connections_on_handles(&live_subgraphs);
    apply_live_reduced_driver_connections_from_handles(reduced_subgraphs, &live_subgraphs);
}

// Upstream parity: reduced local analogue for the global-secondary-driver promotion branch in
// `CONNECTION_GRAPH::Recalculate()` immediately before `propagateToNeighbors()`. This still stops
// short of pointer-owned driver/item mutation, but it now mutates the shared live subgraph owner
// instead of promoting disconnected candidates on reduced snapshots before the live graph runs.
// Remaining divergence is the still-missing pointer-owned driver/item mutation on the promoted
// subgraph itself.
// Upstream parity: reduced local analogue for the shared graph-name recache KiCad performs through
// `recacheSubgraphName()` plus later netcode assignment. This is not a 1:1 cache owner because
// the Rust tree still rebuilds reduced lookup maps from snapshots instead of mutating live graph
// maps as names change, but it keeps the final shared `(name, sheet+name)` indexes and first-seen
// net codes aligned with the post-propagation reduced subgraph names instead of stale pre-rename
// values. Outward reduced `resolved_connection` state is now re-derived from the required reduced
// `driver_connection` owner during this cache rebuild instead of assigning both in parallel, and
// production cache/code assignment now also reads the reduced subgraph name from that same owner
// instead of treating `subgraph.name` as a second source of truth. Remaining divergence is the
// still-missing live cache mutation on real subgraph objects.
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
        let owner_name = subgraph.driver_connection.name.clone();

        if !owner_name.is_empty() {
            subgraph.name = owner_name.clone();
            let next_code = net_codes.len() + 1;
            let code = *net_codes.entry(owner_name.clone()).or_insert(next_code);
            subgraph.code = code;
            assign_reduced_connection_net_codes(&mut subgraph.driver_connection, &mut net_codes);
            subgraph.resolved_connection = subgraph.driver_connection.clone();
            subgraph.resolved_connection.name = owner_name.clone();

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
                .entry(owner_name.clone())
                .or_default()
                .push(index);
            if owner_name.contains('[') {
                let prefix_only = format!("{}[]", owner_name.split('[').next().unwrap_or(""));
                subgraphs_by_name
                    .entry(prefix_only)
                    .or_default()
                    .push(index);
            }
            subgraphs_by_sheet_and_name
                .entry((subgraph.sheet_instance_path.clone(), owner_name))
                .or_default()
                .push(index);
        } else {
            subgraph.name.clear();
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
// so graph item lookup stays available for ERC power-pin paths. Reduced base-pin payload now also
// preserves schematic-path identity so later live symbol-pin owners can derive their own driver
// identity without a copied side cache. It now also preserves first-seen net-name encounter order
// instead of reordering reduced subgraphs by net name before the shared graph owner assigns
// whole-net codes. Remaining divergence is the missing full subgraph object model and graph-owned
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
                    schematic_path: schematic.path.clone(),
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
                    driver_connection: reduced_seeded_symbol_pin_connection(
                        symbol,
                        pin,
                        &unit_pins,
                        sheet_instance_path,
                    ),
                    preserve_local_name_on_refresh: reduced_power_pin_driver_priority(
                        symbol,
                        pin.electrical_type.as_deref(),
                    )
                    .is_some()
                        && reduced_power_pin_driver_text(symbol, pin).is_some(),
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
// outward `resolved_connection` state is now also derived from the required reduced
// `driver_connection` owner instead of being rebuilt from parallel raw fields during final graph
// assembly, whole-net views are derived from the shared subgraph owner instead of a second stored
// flattened carrier, reduced label/sheet-pin/no-connect membership now rides on the shared
// subgraph owner for graph-side ERC rules instead of per-sheet component rescans, reduced driver
// identity now rides on that same owner so `RunERC()`-style reused-screen de-duplication can
// happen above the shared graph boundary, and final reduced subgraph names now also derive from
// the required reduced `driver_connection` owner instead of treating `name` as an independent
// production owner. The outward reduced node carrier is still narrower than a real
// `CONNECTION_SUBGRAPH` item owner.
pub(crate) fn collect_reduced_project_net_graph_from_inputs(
    inputs: ReducedProjectGraphInputs<'_>,
    for_board: bool,
) -> ReducedProjectNetGraph {
    struct PendingProjectSubgraph {
        name: String,
        driver_connection: ReducedProjectConnection,
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
                let driver_connection = build_pending_reduced_subgraph_driver_connection(
                    schematic,
                    &sheet_path.instance_path,
                    &entry.name,
                    driver_candidate.as_ref(),
                    &sheet_path_prefix,
                    &bus_members,
                    &label_links,
                    &hier_sheet_pins,
                    &hier_ports,
                );
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

            let driver_connection = build_pending_reduced_subgraph_driver_connection(
                schematic,
                &sheet_path.instance_path,
                "",
                None,
                &sheet_path_prefix,
                &[],
                &label_links,
                &hier_sheet_pins,
                &hier_ports,
            );

            pending_subgraphs.push(PendingProjectSubgraph {
                name: String::new(),
                driver_connection,
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
        let resolved_name = net_identity
            .map(|net| net.name.clone())
            .unwrap_or_else(|| pending.name.clone());
        let mut driver_connection = pending.driver_connection.clone();
        driver_connection.name = resolved_name.clone();
        let mut resolved_connection = driver_connection.clone();
        resolved_connection.name = resolved_name.clone();
        let net_identity = ReducedProjectSubgraphEntry {
            subgraph_code: subgraph_index + 1,
            code: net_identity.map(|net| net.code).unwrap_or_default(),
            name: resolved_name,
            resolved_connection,
            driver_connection,
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

            let driver_connection = &child.driver_connection;
            if !driver_connection.full_local_name.is_empty() {
                child_names.push(driver_connection.full_local_name.clone());
            } else if !driver_connection.name.is_empty() {
                child_names.push(driver_connection.name.clone());
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

    let _live_subgraphs =
        refresh_reduced_live_graph_propagation_with_handles(&mut reduced_subgraphs);
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
// time at the flattened whole-net layer, and whole-net grouping now reads net names from the
// required reduced `driver_connection` owner instead of a parallel reduced subgraph `name` field.
// Write-time exporters still do their own emitted-code assignment like KiCad `makeListOfNets()`.
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
        let owner_name = subgraph.driver_connection.name.clone();
        let entry = grouped
            .entry((subgraph.code, owner_name.clone()))
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
                owner_name.clone(),
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
    resolve_reduced_project_subgraph_at(graph, sheet_path, at)
        .map(|subgraph| subgraph.driver_connection.local_name.clone())
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
// directly to label links instead of splitting label point state from bus/net text state, and the
// reduced label payload now also keeps schematic-path identity so later live driver owners can
// derive label identity from the owner itself. Remaining divergence is fuller live item identity
// plus in-place connection updates.
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
                        schematic_path: schematic.path.clone(),
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
// matching and ERC do not need separate raw name/type caches, and the reduced sheet-pin payload
// now also keeps schematic-path identity so live driver owners can derive sheet-pin identity from
// the owner itself. Remaining divergence is fuller item-pointer identity and live connection-type
// ownership.
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
                        schematic_path: schematic.path.clone(),
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
                    schematic_path: schematic.path.clone(),
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
// Upstream parity: reduced local analogue for the symbol-pin half of
// `CONNECTION_GRAPH::GetNetFromItem()` on the project graph path. This still returns reduced net
// identity instead of a live `CONNECTION_SUBGRAPH`, but it now reports the symbol pin's net name
// from the required reduced `driver_connection` owner instead of a parallel reduced subgraph
// `name` field. Remaining divergence is fuller item identity and the still-missing live
// `CONNECTION_SUBGRAPH` object.
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
        name: subgraph.driver_connection.name.clone(),
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
// graph-owned per-pin driver connections projected on the reduced base-pin payload itself, and
// both the named and by-location subgraph lookup edges now include projected pin number so
// stacked pins do not collapse to one driver-name query. Base-pin owners now also project
// explicit pin-local-name preservation state from the live pin owner, so this lookup no longer
// guesses from `Net-(` string shapes when deciding whether to report the pin-owned name or the
// chosen subgraph driver name. Remaining divergence is fuller live connection-object caching and
// item ownership.
pub(crate) fn resolve_reduced_project_driver_name_for_symbol_pin(
    graph: &ReducedProjectNetGraph,
    sheet_path: &LoadedSheetPath,
    symbol: &Symbol,
    at: [f64; 2],
    pin_name: Option<&str>,
    pin_number: Option<&str>,
) -> Option<String> {
    resolve_reduced_project_subgraph_for_symbol_pin(
        graph, sheet_path, symbol, at, pin_name, pin_number,
    )
    .and_then(|subgraph| {
        pin_name
            .and_then(|pin_name| {
                subgraph.base_pins.iter().find(|base_pin| {
                    base_pin.key
                        == reduced_project_base_pin_key(
                            sheet_path, symbol, at, pin_name, pin_number,
                        )
                })
            })
            .or_else(|| {
                subgraph.base_pins.iter().find(|base_pin| {
                    base_pin.key.symbol_uuid == symbol.uuid
                        && base_pin.key.at == point_key(at)
                        && (pin_number.is_none() || base_pin.key.number.as_deref() == pin_number)
                })
            })
            .map(|base_pin| {
                (
                    base_pin.preserve_local_name_on_refresh,
                    &base_pin.driver_connection,
                )
            })
    })
    .and_then(|(preserve_local_name_on_refresh, connection)| {
        preserve_local_name_on_refresh.then(|| connection.local_name.clone())
    })
    .or_else(|| {
        resolve_reduced_project_subgraph_for_symbol_pin(
            graph, sheet_path, symbol, at, pin_name, pin_number,
        )
        .map(|subgraph| subgraph.driver_connection.local_name.clone())
    })
}

// Upstream parity: reduced local analogue for the connection-point half of
// `CONNECTION_GRAPH::GetSubgraphForItem()` / `GetResolvedSubgraphName()` on the project graph
// path. This is not a 1:1 KiCad item map because the Rust tree still keys the lookup by `(sheet
// instance path, reduced subgraph anchor)` instead of a live item-owned `CONNECTION_SUBGRAPH`,
// but it now reports the graph-owned subgraph name from the required reduced driver owner instead
// of re-deriving the point net name from reduced connection boundary state or treating
// `subgraph.name` as a second owner. Remaining divergence is fuller item identity for labels,
// wires, and markers plus the still-missing `CONNECTION_SUBGRAPH` object.
pub(crate) fn resolve_reduced_project_net_at(
    graph: &ReducedProjectNetGraph,
    sheet_path: &LoadedSheetPath,
    at: [f64; 2],
) -> Option<ReducedProjectNetIdentity> {
    resolve_reduced_project_subgraph_at(graph, sheet_path, at).map(|subgraph| {
        ReducedProjectNetIdentity {
            code: subgraph.code,
            name: subgraph.driver_connection.name.clone(),
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

// Upstream parity: reduced local analogue for the shown-name text KiCad seeds onto power-pin
// `SCH_CONNECTION`s. This still runs on projected pin data instead of a live `SCH_PIN`, but it
// now prefers the projected pin shown name before falling back to the symbol value, which keeps
// exercised multi-pin power symbols on per-pin names instead of collapsing every power pin on the
// symbol to one shared value string.
fn reduced_power_pin_driver_text(symbol: &Symbol, pin: &ProjectedSymbolPin) -> Option<String> {
    pin.name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty() && *name != "~")
        .map(str::to_string)
        .or_else(|| symbol_value_text(symbol))
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
// time. Power-pin seeding now prefers the projected pin shown name before the symbol value so
// multi-pin power symbols keep per-pin names on their live base-pin owners, which matches the
// upstream per-pin seed path more closely than starting most base pins as `CONNECTION_TYPE::NONE`
// or leaving ordinary pins unnamed until later graph ownership attaches.
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
        if let Some(name) = reduced_power_pin_driver_text(symbol, pin) {
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

// Upstream parity: reduced local bridge for the pending subgraph driver connection KiCad keeps on
// the owning graph object while building one connection subgraph. The Rust tree still assembles a
// reduced driver connection before the fuller live object graph exists, but pending reduced graph
// build now materializes that owner directly instead of carrying an optional driver connection and
// reconstructing fallback local/full-local names later.
fn build_pending_reduced_subgraph_driver_connection(
    schematic: &Schematic,
    sheet_instance_path: &str,
    resolved_name: &str,
    driver_candidate: Option<&ReducedDriverNameCandidate>,
    sheet_path_prefix: &str,
    bus_members: &[ReducedBusMember],
    label_links: &[ReducedLabelLink],
    hier_sheet_pins: &[ReducedHierSheetPinLink],
    hier_ports: &[ReducedHierPortLink],
) -> ReducedProjectConnection {
    if let Some(candidate) = driver_candidate {
        return build_reduced_project_connection(
            schematic,
            sheet_instance_path.to_string(),
            resolved_name.to_string(),
            candidate.text.clone(),
            reduced_driver_candidate_full_name(candidate, sheet_path_prefix),
            if reduced_text_is_bus(schematic, &candidate.text) {
                bus_members.to_vec()
            } else {
                Vec::new()
            },
        );
    }

    if let Some(connection) = label_links
        .iter()
        .map(|link| &link.connection)
        .chain(hier_sheet_pins.iter().map(|pin| &pin.connection))
        .chain(hier_ports.iter().map(|port| &port.connection))
        .find(|connection| {
            matches!(
                connection.connection_type,
                ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
            )
        })
    {
        return build_reduced_project_connection(
            schematic,
            sheet_instance_path.to_string(),
            resolved_name.to_string(),
            connection.local_name.clone(),
            connection.full_local_name.clone(),
            bus_members.to_vec(),
        );
    }

    let local_name = reduced_short_net_name(resolved_name);
    let full_local_name = if !resolved_name.is_empty() {
        resolved_name.to_string()
    } else {
        local_name.clone()
    };

    build_reduced_project_connection(
        schematic,
        sheet_instance_path.to_string(),
        resolved_name.to_string(),
        local_name,
        full_local_name,
        bus_members.to_vec(),
    )
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
// stacked same-position pins do not collapse before live owner attachment. Reduced power-pin
// drivers now also prefer the projected pin shown name before the symbol value so multi-pin power
// symbols keep per-pin driver text through the shared graph owner. Remaining divergence is the
// still-missing live connection object plus fuller power/bus-parent driver ownership.
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
                        if let Some(text) = reduced_power_pin_driver_text(symbol, pin) {
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
// - reduced power-pin drivers now prefer the projected pin shown name before the symbol value so
//   multi-pin power symbols keep per-pin driver text through reduced ranking
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
                                reduced_power_pin_driver_text(symbol, pin).map(|text| {
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
// because the Rust tree still identifies a placed pin by reduced projected identity instead of a
// live `SCH_PIN*`, but it now includes projected pin number so stacked pins on one symbol point do
// not alias one another on the fallback owner path, and it lets pin-owned ERC/shown-text paths
// resolve against a symbol-pin component owner instead of a raw point query.
pub(crate) fn resolve_reduced_net_name_for_symbol_pin<FLabel, FSheet>(
    schematic: &Schematic,
    symbol: &Symbol,
    at: [f64; 2],
    pin_number: Option<&str>,
    sheet_path_prefix: Option<&str>,
    shown_label_text: FLabel,
    shown_sheet_pin_text: FSheet,
) -> Option<String>
where
    FLabel: FnMut(&Label) -> String,
    FSheet: FnMut(&crate::model::Sheet, &crate::model::SheetPin) -> String,
{
    let connected_component =
        connection_component_for_symbol_pin(schematic, symbol, at, pin_number)?;
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
        collect_live_reduced_propagation_component_from_handles,
        find_first_reduced_project_subgraph_by_name, find_reduced_project_subgraph_by_name,
        recache_live_reduced_subgraph_name_from_handles,
        recache_live_reduced_subgraph_name_handle_cache_from_handles, reduced_bus_member_objects,
        refresh_reduced_live_bus_link_members,
        refresh_reduced_live_bus_link_members_on_handles_for_indexes,
        refresh_reduced_live_bus_neighbor_drivers,
        refresh_reduced_live_bus_neighbor_drivers_on_handles_for_indexes,
        refresh_reduced_live_bus_parent_members, refresh_reduced_live_graph_propagation,
        refresh_reduced_live_multiple_bus_parent_names,
        refresh_reduced_live_post_propagation_item_connections,
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Bus,
                    name: "/BUS".to_string(),
                    local_name: "BUS".to_string(),
                    full_local_name: "/BUS".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/ENTRY".to_string(),
                    local_name: "ENTRY".to_string(),
                    full_local_name: "/ENTRY".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
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
                driver_connection: ReducedProjectConnection {
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/SIG0".to_string(),
                    local_name: "SIG0".to_string(),
                    full_local_name: "/SIG0".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/TOP".to_string(),
                    local_name: "TOP".to_string(),
                    full_local_name: "/TOP".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
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
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/CHILD".to_string(),
                    local_name: "CHILD".to_string(),
                    full_local_name: "/CHILD".to_string(),
                    sheet_instance_path: "/child".to_string(),
                    members: Vec::new(),
                },
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
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
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
            driver_connection: ReducedProjectConnection {
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
                schematic_path: std::path::PathBuf::from("root.kicad_sch"),
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
            subgraph.driver_connection = Rc::new(RefCell::new(
                ReducedProjectConnection {
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
                }
                .into(),
            ));
            subgraph.label_links[0].borrow_mut().connection = Rc::new(RefCell::new(
                ReducedProjectConnection {
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
                }
                .into(),
            ));
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
                driver_connection: ReducedProjectConnection {
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/OLD1".to_string(),
                    local_name: "OLD1".to_string(),
                    full_local_name: "/OLD1".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
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
        let connected_bus_connection = connected_bus_item.borrow().connection.clone();
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
        assert_eq!(by_sheet.driver_connection.local_name, "SIG");

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
            by_point.driver_connection.full_local_name,
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
    fn reduced_project_driver_name_for_multi_pin_power_symbol_uses_per_pin_name() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_multi_power_pin_driver_name_{}.kicad_sch",
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
    (uuid "73050000-0000-0000-0000-000000000311")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "#PWR1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "SplitGround" (at 0 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy -10 0)))
  (global_label "VCC" (shape input) (at -10 0 0) (effects (font (size 1 1))))
  (wire (pts (xy 10 0) (xy 20 0)))
  (global_label "SIG" (shape input) (at 20 0 0) (effects (font (size 1 1)))))"##,
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

        let gnd_driver = crate::connectivity::resolve_reduced_project_driver_name_for_symbol_pin(
            &graph,
            &sheet_path,
            symbol,
            [0.0, 0.0],
            Some("GND"),
            Some("1"),
        )
        .expect("gnd driver name");
        let agnd_driver = crate::connectivity::resolve_reduced_project_driver_name_for_symbol_pin(
            &graph,
            &sheet_path,
            symbol,
            [10.0, 0.0],
            Some("AGND"),
            Some("2"),
        )
        .expect("agnd driver name");

        assert_eq!(gnd_driver, "GND");
        assert_eq!(agnd_driver, "AGND");

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
            subgraphs: vec![super::ReducedProjectSubgraphEntry {
                subgraph_code: 1,
                code: 1,
                name: "PWR".to_string(),
                resolved_connection: super::ReducedProjectConnection {
                    net_code: 0,
                    connection_type: super::ReducedProjectConnectionType::Net,
                    name: "PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "PWR".to_string(),
                    sheet_instance_path: sheet_path.instance_path.clone(),
                    members: Vec::new(),
                },
                driver_connection: super::ReducedProjectConnection {
                    net_code: 0,
                    connection_type: super::ReducedProjectConnectionType::Net,
                    name: "PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "PWR".to_string(),
                    sheet_instance_path: sheet_path.instance_path.clone(),
                    members: Vec::new(),
                },
                chosen_driver_identity: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: sheet_path.instance_path.clone(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: vec![
                    super::ReducedProjectBasePin {
                        schematic_path: sheet_path.schematic_path.clone(),
                        key: super::ReducedNetBasePinKey {
                            sheet_instance_path: sheet_path.instance_path.clone(),
                            symbol_uuid: symbol.uuid.clone(),
                            at: super::point_key([0.0, 0.0]),
                            name: Some("PWR".to_string()),
                            number: Some("1".to_string()),
                        },
                        number: Some("1".to_string()),
                        electrical_type: None,
                        connection: super::ReducedProjectConnection {
                            net_code: 0,
                            connection_type: super::ReducedProjectConnectionType::Net,
                            name: "VCC".to_string(),
                            local_name: "VCC".to_string(),
                            full_local_name: "VCC".to_string(),
                            sheet_instance_path: sheet_path.instance_path.clone(),
                            members: Vec::new(),
                        },
                        driver_connection: super::ReducedProjectConnection {
                            net_code: 0,
                            connection_type: super::ReducedProjectConnectionType::Net,
                            name: "VCC".to_string(),
                            local_name: "VCC".to_string(),
                            full_local_name: "VCC".to_string(),
                            sheet_instance_path: sheet_path.instance_path.clone(),
                            members: Vec::new(),
                        },
                        preserve_local_name_on_refresh: true,
                    },
                    super::ReducedProjectBasePin {
                        schematic_path: sheet_path.schematic_path.clone(),
                        key: super::ReducedNetBasePinKey {
                            sheet_instance_path: sheet_path.instance_path.clone(),
                            symbol_uuid: symbol.uuid.clone(),
                            at: super::point_key([0.0, 0.0]),
                            name: Some("PWR".to_string()),
                            number: Some("2".to_string()),
                        },
                        number: Some("2".to_string()),
                        electrical_type: None,
                        connection: super::ReducedProjectConnection {
                            net_code: 0,
                            connection_type: super::ReducedProjectConnectionType::Net,
                            name: "GND".to_string(),
                            local_name: "GND".to_string(),
                            full_local_name: "GND".to_string(),
                            sheet_instance_path: sheet_path.instance_path.clone(),
                            members: Vec::new(),
                        },
                        driver_connection: super::ReducedProjectConnection {
                            net_code: 0,
                            connection_type: super::ReducedProjectConnectionType::Net,
                            name: "GND".to_string(),
                            local_name: "GND".to_string(),
                            full_local_name: "GND".to_string(),
                            sheet_instance_path: sheet_path.instance_path.clone(),
                            members: Vec::new(),
                        },
                        preserve_local_name_on_refresh: true,
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
            }],
            subgraphs_by_name: BTreeMap::new(),
            subgraphs_by_sheet_and_name: BTreeMap::new(),
            pin_subgraph_identities: BTreeMap::new(),
            pin_subgraph_identities_by_location: BTreeMap::from([
                (
                    super::ReducedProjectPinIdentityKey {
                        sheet_instance_path: sheet_path.instance_path.clone(),
                        symbol_uuid: symbol.uuid.clone(),
                        at: super::point_key([0.0, 0.0]),
                        number: Some("1".to_string()),
                    },
                    0,
                ),
                (
                    super::ReducedProjectPinIdentityKey {
                        sheet_instance_path: sheet_path.instance_path.clone(),
                        symbol_uuid: symbol.uuid.clone(),
                        at: super::point_key([0.0, 0.0]),
                        number: Some("2".to_string()),
                    },
                    0,
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
                driver_connection: super::ReducedProjectConnection {
                    net_code: 1,
                    connection_type: super::ReducedProjectConnectionType::Net,
                    name: "SIG".to_string(),
                    local_name: "SIG".to_string(),
                    full_local_name: "SIG".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
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
        let component =
            super::connection_component_for_symbol_pin(schematic, symbol, [10.0, 0.0], Some("2"))
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
    fn connection_component_for_symbol_pin_uses_stacked_pin_number_identity() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_stacked_pin_component_identity_{}.kicad_sch",
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
    (uuid "73050000-0000-0000-0000-000000000604")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "U1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "Stacked" (at 0 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy -10 0)))
  (global_label "NET" (shape input) (at -10 0 0) (effects (font (size 1 1)))))"#,
        )
        .expect("write schematic");

        let schematic = crate::parser::parse_schematic_file(&path).expect("parse schematic");
        let symbol = schematic
            .screen
            .items
            .iter()
            .find_map(|item| match item {
                SchItem::Symbol(symbol) => Some(symbol),
                _ => None,
            })
            .expect("symbol");

        assert!(
            super::connection_component_for_symbol_pin(&schematic, symbol, [0.0, 0.0], Some("2"))
                .is_some()
        );
        assert!(
            super::connection_component_for_symbol_pin(&schematic, symbol, [0.0, 0.0], Some("3"))
                .is_none()
        );

        let _ = fs::remove_file(&path);
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
            driver_connection: ReducedProjectConnection {
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
            driver_connection: ReducedProjectConnection {
                net_code: 0,
                connection_type: ReducedProjectConnectionType::Net,
                name: "/OLD1".to_string(),
                local_name: "OLD1".to_string(),
                full_local_name: "/OLD1".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
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
                driver_connection: ReducedProjectConnection {
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "/PWR".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/OLD1".to_string(),
                    local_name: "OLD1".to_string(),
                    full_local_name: "/OLD1".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
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
                driver_connection: Rc::new(RefCell::new(
                    ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "/OLD".to_string(),
                        local_name: "OLD".to_string(),
                        full_local_name: "/OLD".to_string(),
                        sheet_instance_path: String::new(),
                        members: Vec::new(),
                    }
                    .into(),
                )),
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
                driver_connection: Rc::new(RefCell::new(
                    ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "/KEEP".to_string(),
                        local_name: "KEEP".to_string(),
                        full_local_name: "/KEEP".to_string(),
                        sheet_instance_path: "/child".to_string(),
                        members: Vec::new(),
                    }
                    .into(),
                )),
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
            driver_connection: Rc::new(RefCell::new(
                ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/OLD".to_string(),
                    local_name: "OLD".to_string(),
                    full_local_name: "/OLD".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                }
                .into(),
            )),
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
            driver_connection: ReducedProjectConnection {
                net_code: 0,
                connection_type: ReducedProjectConnectionType::Bus,
                name: "/BUS".to_string(),
                local_name: "BUS".to_string(),
                full_local_name: "/BUS".to_string(),
                sheet_instance_path: String::new(),
                members: vec![ReducedBusMember {
                    net_code: 0,
                    name: "BUS0".to_string(),
                    local_name: "BUS0".to_string(),
                    full_local_name: "/BUS0".to_string(),
                    vector_index: Some(0),
                    kind: ReducedBusMemberKind::Net,
                    members: Vec::new(),
                }],
            },
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

        assert_eq!(shared.borrow().driver_connection.borrow().name, "/RENAMED");
        assert!(!shared.borrow().dirty);
        let wire_item_connection = shared.borrow().wire_items[0].borrow().connection.clone();
        assert!(Rc::ptr_eq(
            &wire_item_connection,
            &shared.borrow().driver_connection
        ));
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
        let attached_bus_connection = attached_bus_item.borrow().connection.clone();
        assert!(super::live_connection_clone_eq(
            &attached_bus_connection.borrow(),
            &shared.borrow().driver_connection.borrow()
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
            driver_connection: ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Net,
                name: "/SIG".to_string(),
                local_name: "SIG".to_string(),
                full_local_name: "/SIG".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
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
                schematic_path: std::path::PathBuf::from("root.kicad_sch"),
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
        let owner = subgraph.drivers[0].borrow().clone();

        match owner {
            super::LiveProjectStrongDriverOwner::SheetPin { owner, .. } => {
                let owner = owner.upgrade().expect("sheet pin owner");
                assert!(Rc::ptr_eq(&owner, &subgraph.hier_sheet_pins[0]));
                let owner_ref = owner.borrow();
                let driver_connection = owner_ref.driver_connection.clone();
                let item_connection = owner_ref.connection.clone();
                drop(owner_ref);
                assert!(!Rc::ptr_eq(&driver_connection, &item_connection));
                let driver = owner
                    .borrow()
                    .driver
                    .clone()
                    .expect("sheet pin driver owner");
                assert!(Rc::ptr_eq(
                    &driver.borrow().connection_handle(),
                    &driver_connection
                ));
                assert!(!matches!(
                    *driver.borrow(),
                    super::LiveProjectStrongDriverOwner::Floating { .. }
                ));
                let driver = driver.borrow().snapshot();
                assert_eq!(driver.connection.local_name, "SIG");
                assert_eq!(driver.connection.name, "/SIG");
                assert_eq!(driver.priority, 1);
                assert!(matches!(
                    driver.identity,
                    Some(super::ReducedProjectDriverIdentity::SheetPin {
                        schematic_path,
                        at: PointKey(10, 20),
                    }) if schematic_path == std::path::PathBuf::from("root.kicad_sch")
                ));
            }
            _ => panic!("expected sheet pin strong-driver owner"),
        }
    }

    #[test]
    fn build_live_reduced_subgraph_handles_attach_label_driver_owners() {
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
            driver_connection: ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Net,
                name: "/SIG".to_string(),
                local_name: "SIG".to_string(),
                full_local_name: "/SIG".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
            chosen_driver_identity: None,
            drivers: vec![ReducedProjectStrongDriver {
                kind: ReducedProjectDriverKind::Label,
                priority: 6,
                connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/SIG".to_string(),
                    local_name: "SIG".to_string(),
                    full_local_name: "/SIG".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                identity: Some(super::ReducedProjectDriverIdentity::Label {
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                    at: PointKey(5, 6),
                    kind: super::reduced_label_kind_sort_key(LabelKind::Global),
                }),
            }],
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: String::new(),
            anchor: PointKey(5, 6),
            points: Vec::new(),
            nodes: Vec::new(),
            base_pins: Vec::new(),
            label_links: vec![ReducedLabelLink {
                schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                at: PointKey(5, 6),
                kind: LabelKind::Global,
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
        let owner = subgraph.drivers[0].borrow().clone();

        match owner {
            super::LiveProjectStrongDriverOwner::Label { owner, .. } => {
                let owner = owner.upgrade().expect("label owner");
                assert!(Rc::ptr_eq(&owner, &subgraph.label_links[0]));
                let parent_subgraph = owner
                    .borrow()
                    .parent_subgraph_handle
                    .upgrade()
                    .expect("label parent subgraph");
                assert!(Rc::ptr_eq(&parent_subgraph, &handles[0]));
                let owner_ref = owner.borrow();
                let driver_connection = owner_ref.driver_connection.clone();
                let item_connection = owner_ref.connection.clone();
                drop(owner_ref);
                assert!(!Rc::ptr_eq(&driver_connection, &item_connection));
                let driver = owner.borrow().driver.clone().expect("label driver owner");
                assert!(Rc::ptr_eq(
                    &driver.borrow().connection_handle(),
                    &driver_connection
                ));
                let driver = driver.borrow().snapshot();
                assert_eq!(driver.connection.local_name, "SIG");
                assert_eq!(driver.connection.name, "/SIG");
                assert_eq!(driver.priority, 6);
                assert!(matches!(
                    driver.identity,
                    Some(super::ReducedProjectDriverIdentity::Label {
                        schematic_path,
                        at: PointKey(5, 6),
                        kind,
                    }) if schematic_path == std::path::PathBuf::from("root.kicad_sch")
                        && kind == super::reduced_label_kind_sort_key(LabelKind::Global)
                ));
            }
            _ => panic!("expected label strong-driver owner"),
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
            driver_connection: ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Net,
                name: "PWR".to_string(),
                local_name: "PWR".to_string(),
                full_local_name: "PWR".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
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
                schematic_path: std::path::PathBuf::from("root.kicad_sch"),
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "PWR".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                preserve_local_name_on_refresh: true,
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
        let owner = subgraph.drivers[0].borrow().clone();

        match owner {
            super::LiveProjectStrongDriverOwner::SymbolPin { owner, .. } => {
                let owner = owner.upgrade().expect("symbol pin owner");
                let parent_subgraph = owner
                    .borrow()
                    .parent_subgraph_handle
                    .upgrade()
                    .expect("symbol pin parent subgraph");
                assert!(Rc::ptr_eq(&parent_subgraph, &handles[0]));
                assert_eq!(owner.borrow().pin.key.symbol_uuid.as_deref(), Some("sym"));
                assert_eq!(owner.borrow().pin.key.at, PointKey(10, 20));
                assert_eq!(owner.borrow().pin.key.number.as_deref(), Some("1"));
                assert_eq!(
                    owner.borrow().connection.borrow().connection_type,
                    super::ReducedProjectConnectionType::Net
                );
                assert_eq!(owner.borrow().connection.borrow().name, "PWR");
                let driver = owner
                    .borrow()
                    .driver
                    .clone()
                    .expect("symbol pin driver owner");
                assert!(!matches!(
                    *driver.borrow(),
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
        let owner = match subgraph.drivers[0].borrow().clone() {
            super::LiveProjectStrongDriverOwner::SymbolPin { owner, .. } => {
                owner.upgrade().expect("symbol pin owner")
            }
            _ => panic!("expected symbol pin strong-driver owner"),
        };
        assert_eq!(owner.borrow().driver_connection.borrow().name, "RENAMED");
        assert!(Rc::ptr_eq(
            &owner.borrow().connection,
            &owner.borrow().driver_connection
        ));
        assert_eq!(owner.borrow().connection.borrow().name, "RENAMED");
    }

    #[test]
    fn build_live_reduced_subgraph_handles_preserve_seeded_symbol_pin_driver_owner() {
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
            driver_connection: ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Net,
                name: "PWR".to_string(),
                local_name: "PWR".to_string(),
                full_local_name: "PWR".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
            chosen_driver_identity: None,
            drivers: vec![ReducedProjectStrongDriver {
                kind: ReducedProjectDriverKind::PowerPin,
                priority: 6,
                connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "FLOATING".to_string(),
                    local_name: "FLOATING".to_string(),
                    full_local_name: "FLOATING".to_string(),
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
                schematic_path: std::path::PathBuf::from("root.kicad_sch"),
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
                    name: "ITEM".to_string(),
                    local_name: "ITEM".to_string(),
                    full_local_name: "ITEM".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "SEEDED".to_string(),
                    local_name: "SEEDED".to_string(),
                    full_local_name: "SEEDED".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                preserve_local_name_on_refresh: true,
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
        let owner = match subgraph.drivers[0].borrow().clone() {
            super::LiveProjectStrongDriverOwner::SymbolPin { owner, .. } => {
                owner.upgrade().expect("symbol pin owner")
            }
            _ => panic!("expected symbol pin strong-driver owner"),
        };

        assert!(!Rc::ptr_eq(
            &owner.borrow().connection,
            &owner.borrow().driver_connection
        ));
        assert_eq!(owner.borrow().connection.borrow().name, "ITEM");
        assert_eq!(owner.borrow().driver_connection.borrow().name, "SEEDED");
        assert_eq!(
            subgraph.drivers[0]
                .borrow()
                .connection_handle()
                .borrow()
                .name,
            "SEEDED"
        );
    }

    #[test]
    fn build_live_reduced_subgraph_handles_choose_driver_from_attached_owner() {
        let chosen = ReducedProjectConnection {
            net_code: 1,
            connection_type: ReducedProjectConnectionType::Net,
            name: "SEEDED".to_string(),
            local_name: "SEEDED".to_string(),
            full_local_name: "SEEDED".to_string(),
            sheet_instance_path: String::new(),
            members: Vec::new(),
        };

        let reduced = vec![ReducedProjectSubgraphEntry {
            subgraph_code: 1,
            code: 1,
            name: "SEEDED".to_string(),
            resolved_connection: chosen.clone(),
            driver_connection: chosen.clone(),
            chosen_driver_identity: None,
            drivers: vec![ReducedProjectStrongDriver {
                kind: ReducedProjectDriverKind::PowerPin,
                priority: 6,
                connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "FLOATING".to_string(),
                    local_name: "FLOATING".to_string(),
                    full_local_name: "FLOATING".to_string(),
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
                schematic_path: std::path::PathBuf::from("root.kicad_sch"),
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
                    name: "ITEM".to_string(),
                    local_name: "ITEM".to_string(),
                    full_local_name: "ITEM".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                driver_connection: chosen.clone(),
                preserve_local_name_on_refresh: true,
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

        assert!(subgraph.chosen_driver.is_some());
        assert_eq!(subgraph.driver_connection.borrow().name, "SEEDED");
        let owner = match subgraph.drivers[0].borrow().clone() {
            super::LiveProjectStrongDriverOwner::SymbolPin { owner, .. } => {
                owner.upgrade().expect("symbol pin owner")
            }
            _ => panic!("expected symbol pin strong-driver owner"),
        };
        assert!(Rc::ptr_eq(
            &owner.borrow().connection,
            &owner.borrow().driver_connection
        ));
        assert_eq!(owner.borrow().connection.borrow().name, "SEEDED");
    }

    #[test]
    fn build_live_reduced_subgraph_handles_promote_chosen_text_driver_owner() {
        let chosen = ReducedProjectConnection {
            net_code: 1,
            connection_type: ReducedProjectConnectionType::Net,
            name: "/SIG".to_string(),
            local_name: "SIG".to_string(),
            full_local_name: "/SIG".to_string(),
            sheet_instance_path: String::new(),
            members: Vec::new(),
        };

        let reduced = vec![ReducedProjectSubgraphEntry {
            subgraph_code: 1,
            code: 1,
            name: "/SIG".to_string(),
            resolved_connection: chosen.clone(),
            driver_connection: chosen.clone(),
            chosen_driver_identity: None,
            drivers: vec![ReducedProjectStrongDriver {
                kind: ReducedProjectDriverKind::Label,
                priority: 6,
                connection: chosen.clone(),
                identity: Some(super::ReducedProjectDriverIdentity::Label {
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                    at: PointKey(5, 6),
                    kind: super::reduced_label_kind_sort_key(LabelKind::Global),
                }),
            }],
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: String::new(),
            anchor: PointKey(5, 6),
            points: Vec::new(),
            nodes: Vec::new(),
            base_pins: Vec::new(),
            label_links: vec![ReducedLabelLink {
                schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                at: PointKey(5, 6),
                kind: LabelKind::Global,
                connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "ITEM".to_string(),
                    local_name: "ITEM".to_string(),
                    full_local_name: "ITEM".to_string(),
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
        let subgraph = handles[0].borrow();
        let owner = match &*subgraph.drivers[0].borrow() {
            super::LiveProjectStrongDriverOwner::Label { owner, .. } => {
                owner.upgrade().expect("label owner")
            }
            _ => panic!("expected label strong-driver owner"),
        };

        assert_eq!(owner.borrow().connection.borrow().name, "ITEM");
        assert_eq!(owner.borrow().driver_connection.borrow().name, "/SIG");
        assert!(subgraph.chosen_driver.is_some());
        assert_eq!(subgraph.driver_connection.borrow().name, "/SIG");
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
            driver_connection: ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Net,
                name: "PWR".to_string(),
                local_name: "PWR".to_string(),
                full_local_name: "PWR".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
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
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
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
                    driver_connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "VCC".to_string(),
                        local_name: "VCC".to_string(),
                        full_local_name: "VCC".to_string(),
                        sheet_instance_path: String::new(),
                        members: Vec::new(),
                    },
                    preserve_local_name_on_refresh: true,
                },
                super::ReducedProjectBasePin {
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
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
                    driver_connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "GND".to_string(),
                        local_name: "GND".to_string(),
                        full_local_name: "GND".to_string(),
                        sheet_instance_path: String::new(),
                        members: Vec::new(),
                    },
                    preserve_local_name_on_refresh: true,
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
        let first_owner = match &*subgraph.drivers[0].borrow() {
            super::LiveProjectStrongDriverOwner::SymbolPin { owner, .. } => {
                owner.upgrade().expect("first symbol pin owner")
            }
            _ => panic!("expected first symbol pin strong-driver owner"),
        };
        let second_owner = match &*subgraph.drivers[1].borrow() {
            super::LiveProjectStrongDriverOwner::SymbolPin { owner, .. } => {
                owner.upgrade().expect("second symbol pin owner")
            }
            _ => panic!("expected second symbol pin strong-driver owner"),
        };

        assert_eq!(first_owner.borrow().pin.key.number.as_deref(), Some("1"));
        assert_eq!(second_owner.borrow().pin.key.number.as_deref(), Some("2"));
        assert_eq!(first_owner.borrow().connection.borrow().name, "VCC");
        assert_eq!(second_owner.borrow().connection.borrow().name, "GND");
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
                driver_connection: ReducedProjectConnection {
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/SIG0".to_string(),
                    local_name: "SIG0".to_string(),
                    full_local_name: "/SIG0".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
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
            .upgrade()
            .expect("bus neighbor handle");
        let net_parent = net.bus_parent_links[0]
            .borrow()
            .subgraph_handle
            .upgrade()
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/CHILD".to_string(),
                    local_name: "CHILD".to_string(),
                    full_local_name: "/CHILD".to_string(),
                    sheet_instance_path: "/child".to_string(),
                    members: Vec::new(),
                },
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
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
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
                driver_connection: ReducedProjectConnection {
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
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/SIG0".to_string(),
                    local_name: "SIG0".to_string(),
                    full_local_name: "/SIG0".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/SIG0".to_string(),
                    local_name: "SIG0".to_string(),
                    full_local_name: "/SIG0".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
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
    fn live_bus_link_refresh_preserves_shared_link_handles() {
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
                driver_connection: ReducedProjectConnection {
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/OLD1".to_string(),
                    local_name: "OLD1".to_string(),
                    full_local_name: "/OLD1".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
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
            },
        ];

        let handles = build_live_reduced_subgraph_handles(&reduced);
        let old_parent_link = handles[1].borrow().bus_parent_links[0].clone();
        let old_neighbor_link = handles[0].borrow().bus_neighbor_links[0].clone();

        {
            let parent = handles[0].borrow_mut();
            let connection = parent.driver_connection.borrow_mut();
            let member = connection.members[0].clone();
            let mut member = member.borrow_mut();
            member.name = "RENAMED1".to_string();
            member.local_name = "RENAMED1".to_string();
            member.full_local_name = "/RENAMED1".to_string();
        }

        refresh_reduced_live_bus_link_members_on_handles_for_indexes(&handles, &[0, 1]);

        let new_parent_link = handles[1].borrow().bus_parent_links[0].clone();
        let new_neighbor_link = handles[0].borrow().bus_neighbor_links[0].clone();

        assert!(Rc::ptr_eq(&old_parent_link, &new_parent_link));
        assert!(Rc::ptr_eq(&old_neighbor_link, &new_neighbor_link));
        assert_eq!(new_parent_link.borrow().member.borrow().name, "RENAMED1");
        assert_eq!(new_neighbor_link.borrow().member.borrow().name, "RENAMED1");
    }

    #[test]
    fn replay_reduced_live_stale_bus_members_updates_other_bus_subgraphs() {
        let live_subgraphs = vec![
            LiveReducedSubgraph {
                source_index: 0,
                driver_connection: Rc::new(RefCell::new(
                    ReducedProjectConnection {
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
                    }
                    .into(),
                )),
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
                driver_connection: Rc::new(RefCell::new(
                    ReducedProjectConnection {
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
                    }
                    .into(),
                )),
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
    fn clone_live_bus_member_handle_preserves_nested_member_handles() {
        let target_child = Rc::new(RefCell::new(super::LiveProjectBusMember::from(
            ReducedBusMember {
                net_code: 0,
                name: "OLD1".to_string(),
                local_name: "OLD1".to_string(),
                full_local_name: "/OLD1".to_string(),
                vector_index: Some(1),
                kind: ReducedBusMemberKind::Net,
                members: Vec::new(),
            },
        )));
        let target = Rc::new(RefCell::new(super::LiveProjectBusMember::from(
            ReducedBusMember {
                net_code: 0,
                name: "BUS".to_string(),
                local_name: "BUS".to_string(),
                full_local_name: "/BUS".to_string(),
                vector_index: None,
                kind: ReducedBusMemberKind::Bus,
                members: vec![target_child.borrow().snapshot()],
            },
        )));
        target.borrow_mut().members = vec![target_child.clone()];

        let source_child = Rc::new(RefCell::new(super::LiveProjectBusMember::from(
            ReducedBusMember {
                net_code: 0,
                name: "RENAMED1".to_string(),
                local_name: "RENAMED1".to_string(),
                full_local_name: "/RENAMED1".to_string(),
                vector_index: Some(1),
                kind: ReducedBusMemberKind::Net,
                members: Vec::new(),
            },
        )));
        let source = Rc::new(RefCell::new(super::LiveProjectBusMember::from(
            ReducedBusMember {
                net_code: 0,
                name: "BUS".to_string(),
                local_name: "BUS".to_string(),
                full_local_name: "/BUS".to_string(),
                vector_index: None,
                kind: ReducedBusMemberKind::Bus,
                members: vec![source_child.borrow().snapshot()],
            },
        )));
        source.borrow_mut().members = vec![source_child];

        super::clone_live_bus_member_handle_into_live_bus_member_handle(&target, &source);

        let refreshed_child = target.borrow().members[0].clone();
        assert!(Rc::ptr_eq(&refreshed_child, &target_child));
        assert_eq!(refreshed_child.borrow().name, "RENAMED1");
        assert_eq!(refreshed_child.borrow().full_local_name, "/RENAMED1");
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
                driver_connection: ReducedProjectConnection {
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
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "/PWR".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
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
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
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

        refresh_reduced_live_bus_parent_members(&mut graph);
        refresh_reduced_live_bus_link_members(&mut graph);

        assert_eq!(graph[0].resolved_connection.members[0].name, "/PWR");
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
                driver_connection: ReducedProjectConnection {
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
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "/PWR".to_string(),
                    sheet_instance_path: "different-sheet".to_string(),
                    members: Vec::new(),
                },
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
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
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

        refresh_reduced_live_bus_parent_members(&mut graph);
        refresh_reduced_live_bus_link_members(&mut graph);

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
                driver_connection: ReducedProjectConnection {
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/OLD".to_string(),
                    local_name: "OLD".to_string(),
                    full_local_name: "/OLD".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
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
        assert_eq!(graph[1].driver_connection.full_local_name, "/SIG1");
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
                driver_connection: ReducedProjectConnection {
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "/PWR".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
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
            graph[0].driver_connection.members[0].full_local_name,
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
                driver_connection: ReducedProjectConnection {
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "/PWR".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
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
            graph[0].driver_connection.members[0].full_local_name,
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
                driver_connection: ReducedProjectConnection {
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "/PWR".to_string(),
                    sheet_instance_path: "different-sheet".to_string(),
                    members: Vec::new(),
                },
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
            graph[0].driver_connection.members[0].full_local_name,
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
                driver_connection: ReducedProjectConnection {
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
                driver_connection: ReducedProjectConnection {
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
                driver_connection: connection.clone(),
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/OLD1".to_string(),
                    local_name: "OLD1".to_string(),
                    full_local_name: "/OLD1".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
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
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/ROOT_SIG".to_string(),
                    local_name: "ROOT_SIG".to_string(),
                    full_local_name: "/ROOT_SIG".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
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
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/Child/GLOBAL_SIG".to_string(),
                    local_name: "GLOBAL_SIG".to_string(),
                    full_local_name: "/Child/GLOBAL_SIG".to_string(),
                    sheet_instance_path: "/child".to_string(),
                    members: Vec::new(),
                },
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
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
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

        refresh_reduced_live_graph_propagation(&mut graph);

        assert_eq!(graph[0].name, "/Child/GLOBAL_SIG");
        assert_eq!(graph[1].name, "/Child/GLOBAL_SIG");
        assert_eq!(
            graph[0].driver_connection.full_local_name,
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
            driver_connection: chosen_connection.clone(),
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "VCC".to_string(),
                    local_name: "VCC".to_string(),
                    full_local_name: "VCC".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "PWR_ALT".to_string(),
                    local_name: "PWR_ALT".to_string(),
                    full_local_name: "PWR_ALT".to_string(),
                    sheet_instance_path: "/other".to_string(),
                    members: Vec::new(),
                },
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
        assert_eq!(graph[1].driver_connection.name, "VCC");
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
            driver_connection: ReducedProjectConnection {
                net_code: 0,
                connection_type: ReducedProjectConnectionType::Net,
                name: "Net-(R1-Pad1)".to_string(),
                local_name: "Net-(R1-Pad1)".to_string(),
                full_local_name: "Net-(R1-Pad1)".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
            chosen_driver_identity: None,
            drivers: Vec::new(),
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: String::new(),
            anchor: PointKey(0, 0),
            points: Vec::new(),
            nodes: Vec::new(),
            base_pins: vec![crate::connectivity::ReducedProjectBasePin {
                schematic_path: std::path::PathBuf::from("root.kicad_sch"),
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: String::new(),
                    local_name: String::new(),
                    full_local_name: String::new(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                preserve_local_name_on_refresh: false,
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
        assert_eq!(graph[0].driver_connection.name, "unconnected-(R1-Pad1)");
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
            driver_connection: ReducedProjectConnection {
                net_code: 0,
                connection_type: ReducedProjectConnectionType::Net,
                name: "Net-(R1-Pad1)".to_string(),
                local_name: "Net-(R1-Pad1)".to_string(),
                full_local_name: "Net-(R1-Pad1)".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
            chosen_driver_identity: None,
            drivers: Vec::new(),
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: String::new(),
            anchor: PointKey(0, 0),
            points: Vec::new(),
            nodes: Vec::new(),
            base_pins: vec![crate::connectivity::ReducedProjectBasePin {
                schematic_path: std::path::PathBuf::from("root.kicad_sch"),
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: String::new(),
                    local_name: String::new(),
                    full_local_name: String::new(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                preserve_local_name_on_refresh: false,
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

        assert_eq!(connection.borrow().name, "unconnected-(R1-Pad1)");
    }

    #[test]
    fn live_item_refresh_updates_non_driver_base_pin_connections() {
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
            driver_connection: ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Net,
                name: "/SIG".to_string(),
                local_name: "SIG".to_string(),
                full_local_name: "/SIG".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
            chosen_driver_identity: None,
            drivers: vec![ReducedProjectStrongDriver {
                kind: ReducedProjectDriverKind::Label,
                priority: 6,
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
                    kind: super::reduced_label_kind_sort_key(LabelKind::Global),
                }),
            }],
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: String::new(),
            anchor: PointKey(0, 0),
            points: Vec::new(),
            nodes: Vec::new(),
            base_pins: vec![crate::connectivity::ReducedProjectBasePin {
                schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                key: crate::connectivity::ReducedNetBasePinKey {
                    sheet_instance_path: String::new(),
                    symbol_uuid: Some("u1".to_string()),
                    at: PointKey(0, 0),
                    name: Some("IN".to_string()),
                    number: Some("1".to_string()),
                },
                number: Some("1".to_string()),
                electrical_type: Some("input".to_string()),
                connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "Net-(U1-Pad1)".to_string(),
                    local_name: "Net-(U1-Pad1)".to_string(),
                    full_local_name: "Net-(U1-Pad1)".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "Net-(U1-Pad1)".to_string(),
                    local_name: "Net-(U1-Pad1)".to_string(),
                    full_local_name: "Net-(U1-Pad1)".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                preserve_local_name_on_refresh: false,
            }],
            label_links: vec![ReducedLabelLink {
                schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                at: PointKey(0, 0),
                kind: LabelKind::Global,
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
        {
            let subgraph = handles[0].borrow();
            assert_eq!(subgraph.driver_connection.borrow().name, "/SIG");
            assert!(matches!(
                subgraph
                    .chosen_driver
                    .as_ref()
                    .map(|driver| driver.borrow().snapshot().kind),
                Some(ReducedProjectDriverKind::Label)
            ));
            assert!(subgraph.base_pins[0].borrow().driver.is_none());
        }
        super::sync_live_reduced_item_connections_from_driver_handle(&handles[0]);

        let connection = handles[0].borrow().base_pins[0].borrow().connection.clone();
        assert_eq!(connection.borrow().name, "/SIG");
        assert_eq!(connection.borrow().local_name, "SIG");
    }

    #[test]
    fn live_item_refresh_preserves_attached_strong_driver_local_pin_names() {
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
            driver_connection: ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Net,
                name: "/SIG".to_string(),
                local_name: "SIG".to_string(),
                full_local_name: "/SIG".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
            chosen_driver_identity: None,
            drivers: vec![
                ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: 6,
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
                        kind: super::reduced_label_kind_sort_key(LabelKind::Global),
                    }),
                },
                ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::PowerPin,
                    priority: 5,
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
                        symbol_uuid: Some("pwr".to_string()),
                        at: PointKey(10, 0),
                        pin_number: Some("1".to_string()),
                    }),
                },
            ],
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: String::new(),
            anchor: PointKey(0, 0),
            points: Vec::new(),
            nodes: Vec::new(),
            base_pins: vec![crate::connectivity::ReducedProjectBasePin {
                schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                key: crate::connectivity::ReducedNetBasePinKey {
                    sheet_instance_path: String::new(),
                    symbol_uuid: Some("pwr".to_string()),
                    at: PointKey(10, 0),
                    name: Some("VCC".to_string()),
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "VCC".to_string(),
                    local_name: "VCC".to_string(),
                    full_local_name: "VCC".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                preserve_local_name_on_refresh: true,
            }],
            label_links: vec![ReducedLabelLink {
                schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                at: PointKey(0, 0),
                kind: LabelKind::Global,
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

        let connection = handles[0].borrow().base_pins[0].borrow().connection.clone();
        assert_eq!(connection.borrow().name, "/SIG");
        assert_eq!(connection.borrow().local_name, "VCC");
        assert_eq!(connection.borrow().full_local_name, "/SIG");
        assert_eq!(connection.borrow().net_code, 1);
    }

    #[test]
    fn live_projection_preserves_attached_strong_driver_local_pin_names() {
        let mut reduced = vec![ReducedProjectSubgraphEntry {
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
            driver_connection: ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Net,
                name: "/SIG".to_string(),
                local_name: "SIG".to_string(),
                full_local_name: "/SIG".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
            chosen_driver_identity: None,
            drivers: vec![
                ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: 6,
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
                        kind: super::reduced_label_kind_sort_key(LabelKind::Global),
                    }),
                },
                ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::PowerPin,
                    priority: 5,
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
                        symbol_uuid: Some("pwr".to_string()),
                        at: PointKey(10, 0),
                        pin_number: Some("1".to_string()),
                    }),
                },
            ],
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: String::new(),
            anchor: PointKey(0, 0),
            points: Vec::new(),
            nodes: Vec::new(),
            base_pins: vec![crate::connectivity::ReducedProjectBasePin {
                schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                key: crate::connectivity::ReducedNetBasePinKey {
                    sheet_instance_path: String::new(),
                    symbol_uuid: Some("pwr".to_string()),
                    at: PointKey(10, 0),
                    name: Some("VCC".to_string()),
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "VCC".to_string(),
                    local_name: "VCC".to_string(),
                    full_local_name: "VCC".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                preserve_local_name_on_refresh: true,
            }],
            label_links: vec![ReducedLabelLink {
                schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                at: PointKey(0, 0),
                kind: LabelKind::Global,
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
        apply_live_reduced_driver_connections_from_handles(&mut reduced, &handles);

        assert_eq!(reduced[0].base_pins[0].connection.name, "/SIG");
        assert_eq!(reduced[0].base_pins[0].connection.local_name, "VCC");
        assert_eq!(reduced[0].base_pins[0].connection.full_local_name, "/SIG");
        assert_eq!(reduced[0].base_pins[0].connection.net_code, 1);
        assert_eq!(reduced[0].drivers[1].connection.name, "/SIG");
        assert_eq!(reduced[0].drivers[1].connection.local_name, "VCC");
        assert_eq!(reduced[0].drivers[1].connection.full_local_name, "/SIG");
        assert_eq!(reduced[0].drivers[1].connection.net_code, 1);
    }

    #[test]
    fn live_projection_updates_reduced_base_pin_connections() {
        let mut reduced = vec![ReducedProjectSubgraphEntry {
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
            driver_connection: ReducedProjectConnection {
                net_code: 0,
                connection_type: ReducedProjectConnectionType::Net,
                name: "Net-(R1-Pad1)".to_string(),
                local_name: "Net-(R1-Pad1)".to_string(),
                full_local_name: "Net-(R1-Pad1)".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
            chosen_driver_identity: None,
            drivers: Vec::new(),
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: String::new(),
            anchor: PointKey(0, 0),
            points: Vec::new(),
            nodes: Vec::new(),
            base_pins: vec![crate::connectivity::ReducedProjectBasePin {
                schematic_path: std::path::PathBuf::from("root.kicad_sch"),
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: String::new(),
                    local_name: String::new(),
                    full_local_name: String::new(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                preserve_local_name_on_refresh: false,
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

        super::refresh_reduced_live_graph_propagation(&mut reduced);

        assert_eq!(
            reduced[0].base_pins[0].connection.name,
            "unconnected-(R1-Pad1)"
        );
    }

    #[test]
    fn live_projection_updates_driver_snapshots() {
        let mut reduced = vec![ReducedProjectSubgraphEntry {
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
            driver_connection: ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Net,
                name: "PWR".to_string(),
                local_name: "PWR".to_string(),
                full_local_name: "PWR".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
            chosen_driver_identity: None,
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
            base_pins: vec![crate::connectivity::ReducedProjectBasePin {
                schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                key: crate::connectivity::ReducedNetBasePinKey {
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "PWR".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                preserve_local_name_on_refresh: true,
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
        {
            let subgraph = handles[0].borrow();
            let owner = match subgraph.drivers[0].borrow().clone() {
                super::LiveProjectStrongDriverOwner::SymbolPin { owner, .. } => {
                    owner.upgrade().expect("symbol pin owner")
                }
                _ => panic!("expected symbol pin strong-driver owner"),
            };
            let base_pin = owner.borrow_mut();
            let mut connection = base_pin.driver_connection.borrow_mut();
            connection.name = "RENAMED".to_string();
            connection.local_name = "RENAMED".to_string();
            connection.full_local_name = "RENAMED".to_string();
        }

        apply_live_reduced_driver_connections_from_handles(&mut reduced, &handles);

        assert_eq!(reduced[0].drivers[0].connection.name, "RENAMED");
    }

    #[test]
    fn live_projection_preserves_chosen_driver_identity_on_bound_owner() {
        let chosen_identity = super::ReducedProjectDriverIdentity::SymbolPin {
            schematic_path: std::path::PathBuf::from("root.kicad_sch"),
            symbol_uuid: Some("sym".to_string()),
            at: PointKey(10, 20),
            pin_number: Some("1".to_string()),
        };
        let mut reduced = vec![ReducedProjectSubgraphEntry {
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
            driver_connection: ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Net,
                name: "PWR".to_string(),
                local_name: "PWR".to_string(),
                full_local_name: "PWR".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
            chosen_driver_identity: Some(chosen_identity.clone()),
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
                identity: Some(chosen_identity.clone()),
            }],
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: String::new(),
            anchor: PointKey(10, 20),
            points: Vec::new(),
            nodes: Vec::new(),
            base_pins: vec![crate::connectivity::ReducedProjectBasePin {
                schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                key: crate::connectivity::ReducedNetBasePinKey {
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "PWR".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                preserve_local_name_on_refresh: true,
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
        apply_live_reduced_driver_connections_from_handles(&mut reduced, &handles);

        assert_eq!(reduced[0].chosen_driver_identity, Some(chosen_identity));
    }

    #[test]
    fn build_live_reduced_subgraph_handles_keep_full_base_pin_payload() {
        let reduced = vec![ReducedProjectSubgraphEntry {
            subgraph_code: 1,
            code: 1,
            name: "SIG".to_string(),
            resolved_connection: ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Net,
                name: "SIG".to_string(),
                local_name: "SIG".to_string(),
                full_local_name: "SIG".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
            driver_connection: ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Net,
                name: "SIG".to_string(),
                local_name: "SIG".to_string(),
                full_local_name: "SIG".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
            chosen_driver_identity: None,
            drivers: Vec::new(),
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: String::new(),
            anchor: PointKey(0, 0),
            points: Vec::new(),
            nodes: Vec::new(),
            base_pins: vec![crate::connectivity::ReducedProjectBasePin {
                schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                key: crate::connectivity::ReducedNetBasePinKey {
                    sheet_instance_path: String::new(),
                    symbol_uuid: Some("u1".to_string()),
                    at: PointKey(1, 2),
                    name: Some("IN".to_string()),
                    number: Some("7".to_string()),
                },
                number: Some("7".to_string()),
                electrical_type: Some("bidirectional".to_string()),
                connection: ReducedProjectConnection {
                    net_code: 1,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "SIG".to_string(),
                    local_name: "SIG".to_string(),
                    full_local_name: "SIG".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                driver_connection: ReducedProjectConnection {
                    net_code: 2,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "DRV".to_string(),
                    local_name: "DRV".to_string(),
                    full_local_name: "DRV".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                preserve_local_name_on_refresh: false,
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
        let snapshot = super::live_base_pin_handle_snapshot(&handles[0].borrow().base_pins[0]);

        assert_eq!(
            snapshot.schematic_path,
            std::path::PathBuf::from("root.kicad_sch")
        );
        assert_eq!(snapshot.key.symbol_uuid.as_deref(), Some("u1"));
        assert_eq!(snapshot.key.number.as_deref(), Some("7"));
        assert_eq!(snapshot.number.as_deref(), Some("7"));
        assert_eq!(snapshot.electrical_type.as_deref(), Some("bidirectional"));
        assert_eq!(snapshot.connection.local_name, "SIG");
        assert_eq!(snapshot.driver_connection.local_name, "DRV");
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
            driver_connection: ReducedProjectConnection {
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
                schematic_path: std::path::PathBuf::from("root.kicad_sch"),
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
        assert_eq!(connection.borrow().name, "SIG");
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
            driver_connection: ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Net,
                name: "/SIG".to_string(),
                local_name: "SIG".to_string(),
                full_local_name: "/SIG".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
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
                schematic_path: std::path::PathBuf::from("root.kicad_sch"),
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
        assert_eq!(connection.borrow().name, "/LOCAL");
        assert_eq!(connection.borrow().local_name, "LOCAL");
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
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/BUS".to_string(),
                    local_name: "BUS".to_string(),
                    full_local_name: "/BUS".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
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
                driver_connection: ReducedProjectConnection {
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
            graph[0].driver_connection.connection_type,
            ReducedProjectConnectionType::Bus
        );
        assert_eq!(
            graph[0].driver_connection.members[0].full_local_name,
            "/child/BUS0"
        );
    }
}
impl PartialEq for LiveReducedLabelLink {
    fn eq(&self, other: &Self) -> bool {
        (
            &self.schematic_path,
            self.at,
            self.kind,
            self.connection.borrow().snapshot(),
            self.driver_connection.borrow().snapshot(),
            live_optional_driver_snapshot(&self.driver),
        ) == (
            &other.schematic_path,
            other.at,
            other.kind,
            other.connection.borrow().snapshot(),
            other.driver_connection.borrow().snapshot(),
            live_optional_driver_snapshot(&other.driver),
        )
    }
}

impl Eq for LiveReducedLabelLink {}
