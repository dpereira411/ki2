use std::cell::RefCell;
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
    pub(crate) non_endpoint_wire_segment_count: usize,
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
    pub(crate) visible: bool,
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
    pub(crate) reference: Option<String>,
    pub(crate) number: Option<String>,
    pub(crate) electrical_type: Option<String>,
    pub(crate) visible: bool,
    pub(crate) is_power_symbol: bool,
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
    pub(crate) base_pins: Vec<ReducedProjectBasePin>,
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
    pub(crate) dangling: bool,
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
    pub(crate) chosen_driver_index: Option<usize>,
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

impl ReducedProjectSubgraphEntry {
    // Upstream parity: reduced graph-boundary analogue for mirroring outward subgraph name and
    // resolved connection state from the owning reduced driver connection. This still keeps both
    // reduced boundary carriers because caller-facing reduced queries read them directly, but it
    // centralizes that boundary sync on the reduced subgraph owner instead of leaving final graph
    // assembly and cache rebuild to assign parallel boundary fields independently.
    fn sync_boundary_state_from_driver_owner(&mut self) {
        let owner_name = self.driver_connection.name.clone();
        self.name = owner_name.clone();
        self.resolved_connection = self.driver_connection.clone();
        self.resolved_connection.name = owner_name;
    }
}

// Upstream parity: reduced predicate for the `CONNECTION_GRAPH::processSubGraphs()` branch that
// skips merge and bus-neighbor candidates without `CONNECTION_SUBGRAPH::m_strong_driver`. This is
// still local-only-transitional because the Rust graph lacks the real mutable `m_strong_driver`
// bit; it mirrors the exercised decision from reduced drivers and KiCad's unique sheet-pin
// promotion branch until a fuller live `CONNECTION_SUBGRAPH` owner stores that state directly.
fn reduced_project_subgraph_has_process_strong_driver(
    subgraphs: &[ReducedProjectSubgraphEntry],
    subgraphs_by_sheet_and_name: &BTreeMap<(String, String), Vec<usize>>,
    subgraph_index: usize,
) -> bool {
    let Some(subgraph) = subgraphs.get(subgraph_index) else {
        return false;
    };

    if subgraph
        .drivers
        .iter()
        .any(|driver| driver.priority >= reduced_hierarchical_label_driver_priority())
    {
        return true;
    }

    let chosen_driver =
        reduced_project_chosen_driver_index(subgraph).and_then(|index| subgraph.drivers.get(index));

    if !matches!(
        chosen_driver.map(|driver| driver.kind),
        Some(ReducedProjectDriverKind::SheetPin)
    ) {
        return false;
    }

    let driver_name = if !subgraph.driver_connection.full_local_name.is_empty() {
        &subgraph.driver_connection.full_local_name
    } else {
        &subgraph.driver_connection.name
    };

    !subgraphs_by_sheet_and_name
        .get(&(subgraph.sheet_instance_path.clone(), driver_name.clone()))
        .is_some_and(|same_sheet| same_sheet.iter().any(|index| *index != subgraph_index))
}

// Upstream parity: reduced cache rebuild for the name maps `processSubGraphs()` consults while it
// mutates weak duplicate names. This is transitional because the final owner should update the live
// `CONNECTION_GRAPH` maps through `recacheSubgraphName()`.
fn reduced_project_rebuild_process_name_indexes(
    subgraphs: &[ReducedProjectSubgraphEntry],
) -> (
    BTreeMap<String, Vec<usize>>,
    BTreeMap<(String, String), Vec<usize>>,
) {
    let mut subgraphs_by_name = BTreeMap::<String, Vec<usize>>::new();
    let mut subgraphs_by_sheet_and_name = BTreeMap::<(String, String), Vec<usize>>::new();

    for (index, subgraph) in subgraphs.iter().enumerate() {
        let owner_name = subgraph.driver_connection.name.clone();
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
    }

    (subgraphs_by_name, subgraphs_by_sheet_and_name)
}

// Upstream parity: reduced analogue for `CONNECTION_SUBGRAPH::m_driver` identity in
// `processSubGraphs()` secondary-driver loops. The reduced graph normally stores this explicitly,
// but unresolved transitional carriers still follow the graph-build invariant that the first strong
// driver is chosen until a fuller live `CONNECTION_SUBGRAPH` owns the pointer directly.
fn reduced_project_chosen_driver_index(subgraph: &ReducedProjectSubgraphEntry) -> Option<usize> {
    subgraph
        .chosen_driver_index
        .or_else(|| (!subgraph.drivers.is_empty()).then_some(0))
}

// Upstream parity: reduced helper for `processSubGraphs()` weak-name suffix generation. This is a
// reduced stand-in for `create_new_name()` until the local graph owns mutable `SCH_CONNECTION`
// objects with real `SetSuffix()` / `ConfigureFromLabel()` behavior.
fn reduced_project_weak_conflict_new_name(
    connection: &ReducedProjectConnection,
    suffix: usize,
) -> String {
    if connection.connection_type == ReducedProjectConnectionType::BusGroup {
        let prefix = connection
            .local_name
            .split_once('{')
            .map(|(prefix, _)| prefix)
            .filter(|prefix| !prefix.is_empty())
            .unwrap_or("BUS");
        let old_name = connection
            .name
            .split_once('{')
            .map(|(_, inner)| inner)
            .unwrap_or(connection.name.as_str());

        return format!("{prefix}_{suffix}{{{old_name}");
    }

    format!("{}_{suffix}", connection.name)
}

// Upstream parity: reduced helper for applying the weak-name suffix mutation that upstream applies
// to `SCH_CONNECTION`. This remains partial because reduced connections store projected name fields
// instead of one authoritative live connection object.
fn reduced_project_apply_weak_conflict_name(
    connection: &mut ReducedProjectConnection,
    new_name: String,
) {
    if connection.connection_type == ReducedProjectConnectionType::BusGroup {
        connection.name = new_name.clone();
        connection.local_name = reduced_short_net_name(&new_name);
        connection.full_local_name = new_name;
        return;
    }

    let suffix = new_name
        .strip_prefix(&connection.name)
        .unwrap_or_default()
        .to_string();
    connection.name = new_name.clone();
    connection.local_name.push_str(&suffix);
    connection.full_local_name = if connection.full_local_name.is_empty() {
        new_name
    } else {
        format!("{}{}", connection.full_local_name, suffix)
    };
}

// Upstream parity: reduced local analogue for the weak-driver duplicate rename branch in
// `CONNECTION_GRAPH::processSubGraphs()`. This mutates the reduced `driver_connection` before
// hierarchy/bus-neighbor construction so weak duplicate default names cannot merge like strong
// labels. Remaining divergence is that suffix application is modeled on reduced connection names
// rather than `SCH_CONNECTION::SetSuffix()` / `ConfigureFromLabel()` on a live object.
fn reduced_project_rename_weak_conflict_subgraphs(
    subgraphs: &mut [ReducedProjectSubgraphEntry],
    subgraphs_by_name: &mut BTreeMap<String, Vec<usize>>,
    subgraphs_by_sheet_and_name: &mut BTreeMap<(String, String), Vec<usize>>,
) {
    for index in 0..subgraphs.len() {
        if subgraphs[index]
            .drivers
            .iter()
            .any(|driver| driver.priority >= reduced_hierarchical_label_driver_priority())
        {
            continue;
        }

        let name = subgraphs[index].driver_connection.name.clone();
        if name.is_empty() {
            continue;
        }

        let mut conflict_key = name.clone();
        let mut conflict_count = subgraphs_by_name
            .get(&conflict_key)
            .map(|indexes| indexes.len())
            .unwrap_or_default();

        if conflict_count <= 1
            && subgraphs[index].driver_connection.connection_type
                == ReducedProjectConnectionType::Bus
        {
            let prefix_only = format!("{}[]", name.split('[').next().unwrap_or(""));

            if let Some(indexes) = subgraphs_by_name.get(&prefix_only) {
                conflict_key = prefix_only;
                conflict_count = indexes.len();
            }
        }

        if conflict_count <= 1 {
            continue;
        }

        let mut suffix = 1;
        let new_name = loop {
            let candidate =
                reduced_project_weak_conflict_new_name(&subgraphs[index].driver_connection, suffix);
            suffix += 1;

            if !subgraphs_by_name.contains_key(&candidate) {
                break candidate;
            }
        };

        if let Some(indexes) = subgraphs_by_name.get_mut(&conflict_key) {
            indexes.retain(|candidate| *candidate != index);
        }
        if conflict_key != name
            && let Some(indexes) = subgraphs_by_name.get_mut(&name)
        {
            indexes.retain(|candidate| *candidate != index);
        }

        reduced_project_apply_weak_conflict_name(
            &mut subgraphs[index].driver_connection,
            new_name.clone(),
        );
        subgraphs[index].sync_boundary_state_from_driver_owner();
        subgraphs_by_name.entry(new_name).or_default().push(index);
    }

    let (rebuilt_by_name, rebuilt_by_sheet_and_name) =
        reduced_project_rebuild_process_name_indexes(subgraphs);
    *subgraphs_by_name = rebuilt_by_name;
    *subgraphs_by_sheet_and_name = rebuilt_by_sheet_and_name;
}

fn push_unique<T: PartialEq>(target: &mut Vec<T>, value: T) {
    if !target.contains(&value) {
        target.push(value);
    }
}

fn append_unique<T: PartialEq>(target: &mut Vec<T>, values: Vec<T>) {
    for value in values {
        push_unique(target, value);
    }
}

// Upstream parity: CONNECTION_GRAPH::processSubGraphs invalidated-subgraph ResolveDrivers tail
// parity_status: partial
// local_kind: local-only-transitional
// divergence: reranks reduced strong-driver snapshots instead of invoking live
// `CONNECTION_SUBGRAPH::ResolveDrivers()` on pointer-owned items after `Absorb()`
// local_only_reason: the reduced graph still stores absorbed item/driver data in snapshot vectors
// replaced_by: fuller live `CONNECTION_SUBGRAPH` owner with real `m_drivers` / `m_driver`
// remove_when: absorbed subgraph mutation and driver resolution run on live subgraph objects
fn reduced_project_resolve_absorbed_driver(subgraph: &mut ReducedProjectSubgraphEntry) {
    let Some((index, driver)) =
        subgraph
            .drivers
            .iter()
            .enumerate()
            .max_by(|(_lhs_index, lhs), (_rhs_index, rhs)| {
                lhs.priority.cmp(&rhs.priority).then_with(|| {
                    reduced_project_strong_driver_full_name(rhs)
                        .cmp(reduced_project_strong_driver_full_name(lhs))
                })
            })
    else {
        return;
    };

    subgraph.chosen_driver_index = Some(index);
    subgraph.driver_connection = driver.connection.clone();
    subgraph.sync_boundary_state_from_driver_owner();
}

// Upstream parity: CONNECTION_GRAPH::processSubGraphs candidate driver name comparison
// parity_status: partial
// local_kind: local-only-transitional
// divergence: compares reduced projected `name` / `full_local_name` strings instead of
// `SCH_CONNECTION::Name( true )`
// local_only_reason: reduced connections are not yet live `SCH_CONNECTION` objects
// replaced_by: fuller live `SCH_CONNECTION` analogue with `Name(true)` semantics
// remove_when: processSubGraphs candidate matching uses live connection objects
fn reduced_project_driver_match_name(connection: &ReducedProjectConnection, name: &str) -> bool {
    connection.name == name
        || (!connection.full_local_name.is_empty() && connection.full_local_name == name)
}

// Upstream parity: CONNECTION_GRAPH::processSubGraphs candidate primary/secondary-driver match branch
// parity_status: partial
// local_kind: local-only-transitional
// divergence: evaluates reduced label/power driver records and skips sheet pins, but still lacks
// full `SCH_ITEM*` default-connection ownership and absorbed-pointer traversal
// local_only_reason: same-type reduced absorption needs a graph-owned match predicate before the
// fuller live subgraph owner exists
// replaced_by: fuller `CONNECTION_SUBGRAPH` candidate loop using `getDefaultConnection()`
// remove_when: processSubGraphs matching runs on live `CONNECTION_SUBGRAPH` item sets
fn reduced_project_absorb_candidate_matches_name(
    candidate: &ReducedProjectSubgraphEntry,
    name: &str,
) -> bool {
    if reduced_project_driver_match_name(&candidate.driver_connection, name) {
        return true;
    }

    if candidate
        .drivers
        .iter()
        .filter(|driver| driver.priority >= reduced_hierarchical_label_driver_priority())
        .count()
        < 2
    {
        return false;
    }

    let chosen_driver_index = reduced_project_chosen_driver_index(candidate);
    candidate.drivers.iter().enumerate().any(|(index, driver)| {
        Some(index) != chosen_driver_index
            && matches!(
                driver.kind,
                ReducedProjectDriverKind::Label | ReducedProjectDriverKind::PowerPin
            )
            && reduced_project_driver_match_name(&driver.connection, name)
    })
}

// Upstream parity: CONNECTION_GRAPH::processSubGraphs `add_connections_to_check()` slice
// parity_status: partial
// local_kind: local-only-transitional
// divergence: derives secondary label connections from reduced strong-driver records instead of
// iterating full `m_items` through `getDefaultConnection()`; power-pin secondary expansion remains
// blocked on fuller live pin ownership for multi-pin power symbols
// local_only_reason: reduced absorption still runs before a fuller live `CONNECTION_SUBGRAPH`
// item set exists
// replaced_by: fuller processSubGraphs loop over live subgraph items and default connections
// remove_when: reduced same-name absorption is replaced by live `CONNECTION_SUBGRAPH::Absorb()`
fn reduced_project_absorb_push_secondary_test_names(
    subgraph: &ReducedProjectSubgraphEntry,
    test_names: &mut Vec<String>,
) {
    let parent_type = subgraph.driver_connection.connection_type;

    for (index, driver) in subgraph.drivers.iter().enumerate() {
        if Some(index) == reduced_project_chosen_driver_index(subgraph)
            || driver.connection.connection_type != parent_type
            || driver.kind != ReducedProjectDriverKind::Label
            || reduced_project_driver_match_name(
                &driver.connection,
                &subgraph.driver_connection.name,
            )
        {
            continue;
        }

        push_unique(test_names, driver.connection.name.clone());
        if !driver.connection.full_local_name.is_empty() {
            push_unique(test_names, driver.connection.full_local_name.clone());
        }

        if matches!(
            driver.connection.connection_type,
            ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
        ) {
            for member in reduced_bus_member_leaf_objects(&driver.connection.members) {
                push_unique(test_names, member.full_local_name);
                push_unique(test_names, member.name);
            }
        }
    }
}

// Upstream parity: reduced local analogue for the same-sheet/same-type primary-driver slice of
// `CONNECTION_SUBGRAPH::Absorb()` as called from `CONNECTION_GRAPH::processSubGraphs()`. This is
// still partial because the Rust graph lacks `m_absorbed_by`, live item pointers, bus-entry
// connected-bus ownership, and the full secondary-driver traversal before `ResolveDrivers()`, but
// it now moves items from exact matching net/bus candidate subgraphs onto one reduced owner before
// hierarchy, bus-link, ERC, or export lookups observe them. Bus-entry carriers are deliberately
// skipped until the fuller live subgraph can preserve their connected-bus item topology while
// absorbing; same-sheet bus-member net names are also skipped for the same reason.
fn reduced_project_absorb_primary_same_name_subgraphs(
    subgraphs: &mut Vec<ReducedProjectSubgraphEntry>,
) {
    let mut absorbed = vec![false; subgraphs.len()];
    let subgraphs_by_sheet_and_name = reduced_project_rebuild_process_name_indexes(subgraphs).1;

    for parent_index in 0..subgraphs.len() {
        let parent_type = subgraphs[parent_index].driver_connection.connection_type;
        if absorbed[parent_index]
            || !matches!(
                parent_type,
                ReducedProjectConnectionType::Net
                    | ReducedProjectConnectionType::Bus
                    | ReducedProjectConnectionType::BusGroup
            )
            || subgraphs[parent_index]
                .bus_items
                .iter()
                .any(|item| item.is_bus_entry)
            || !reduced_project_subgraph_has_process_strong_driver(
                subgraphs,
                &subgraphs_by_sheet_and_name,
                parent_index,
            )
        {
            continue;
        }

        let parent_sheet = subgraphs[parent_index].sheet_instance_path.clone();
        let parent_name = subgraphs[parent_index].driver_connection.name.clone();
        let mut parent_test_names = vec![parent_name.clone()];
        if matches!(
            parent_type,
            ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
        ) {
            for member in reduced_bus_member_leaf_objects(&subgraphs[parent_index].bus_members) {
                push_unique(&mut parent_test_names, member.full_local_name);
                push_unique(&mut parent_test_names, member.name);
            }
        }
        reduced_project_absorb_push_secondary_test_names(
            &subgraphs[parent_index],
            &mut parent_test_names,
        );
        if parent_type == ReducedProjectConnectionType::Net
            && subgraphs.iter().any(|candidate| {
                candidate.sheet_instance_path == parent_sheet
                    && candidate.bus_members.iter().any(|member| {
                        reduced_bus_member_leaf_objects(std::slice::from_ref(member))
                            .iter()
                            .any(|leaf| {
                                leaf.full_local_name == parent_name || leaf.name == parent_name
                            })
                    })
            })
        {
            continue;
        }

        let mut candidate_index = parent_index + 1;
        while candidate_index < subgraphs.len() {
            if absorbed[candidate_index]
                || subgraphs[candidate_index].sheet_instance_path != parent_sheet
                || subgraphs[candidate_index].driver_connection.connection_type != parent_type
                || !parent_test_names.iter().any(|test_name| {
                    reduced_project_absorb_candidate_matches_name(
                        &subgraphs[candidate_index],
                        test_name,
                    )
                })
                || subgraphs[candidate_index]
                    .bus_items
                    .iter()
                    .any(|item| item.is_bus_entry)
                || !reduced_project_subgraph_has_process_strong_driver(
                    subgraphs,
                    &subgraphs_by_sheet_and_name,
                    candidate_index,
                )
            {
                candidate_index += 1;
                continue;
            }

            let candidate = subgraphs[candidate_index].clone();
            reduced_project_absorb_push_secondary_test_names(&candidate, &mut parent_test_names);
            let parent = &mut subgraphs[parent_index];
            append_unique(&mut parent.points, candidate.points);
            append_unique(&mut parent.nodes, candidate.nodes);
            append_unique(&mut parent.base_pins, candidate.base_pins);
            append_unique(&mut parent.label_links, candidate.label_links);
            append_unique(&mut parent.no_connect_points, candidate.no_connect_points);
            append_unique(&mut parent.hier_sheet_pins, candidate.hier_sheet_pins);
            append_unique(&mut parent.hier_ports, candidate.hier_ports);
            append_unique(&mut parent.bus_members, candidate.bus_members);
            append_unique(&mut parent.bus_items, candidate.bus_items);
            append_unique(&mut parent.wire_items, candidate.wire_items);
            append_unique(&mut parent.drivers, candidate.drivers);
            parent.has_no_connect |= candidate.has_no_connect;
            if parent.class.is_empty() {
                parent.class = candidate.class;
            }
            reduced_project_resolve_absorbed_driver(parent);
            absorbed[candidate_index] = true;
            candidate_index += 1;
        }
    }

    let mut next_code = 1;
    subgraphs.retain_mut(|subgraph| {
        let keep = !absorbed
            .get(subgraph.subgraph_code.saturating_sub(1))
            .copied()
            .unwrap_or(false);
        if keep {
            subgraph.subgraph_code = next_code;
            next_code += 1;
        }
        keep
    });
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
        child_sheet_uuid: Option<String>,
    },
    SymbolPin {
        schematic_path: std::path::PathBuf,
        sheet_instance_path: String,
        symbol_uuid: Option<String>,
        at: PointKey,
        pin_number: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ReducedProjectDriverKind {
    Label,
    SheetPin,
    Pin,
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

fn unattached_live_strong_driver_owner(
    identity: Option<ReducedProjectDriverIdentity>,
    connection: &LiveProjectConnectionHandle,
    kind: ReducedProjectDriverKind,
    priority: i32,
) -> LiveProjectStrongDriverOwner {
    #[cfg(test)]
    {
        return LiveProjectStrongDriverOwner::Floating {
            identity,
            connection: connection.clone(),
            kind,
            priority,
        };
    }

    #[cfg(not(test))]
    {
        if kind == ReducedProjectDriverKind::PowerPin {
            return LiveProjectStrongDriverOwner::Floating {
                identity,
                connection: connection.clone(),
                kind,
                priority,
            };
        }

        let _ = identity;
        let _ = connection;
        let _ = priority;
        panic!("production live strong drivers must bind to concrete item owners");
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
                    child_sheet_uuid: owner.child_sheet_uuid.clone(),
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
                            sheet_instance_path: owner.pin.key.sheet_instance_path.clone(),
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

impl LiveProjectStrongDriverOwner {
    // Upstream parity: local live-driver analogue for `CONNECTION_SUBGRAPH::GetNameForDriver()`
    // comparisons. It intentionally reads shown/local driver text instead of the resolved
    // connection name so secondary-driver promotion follows KiCad's driver-name matching branch.
    fn driver_name(&self) -> String {
        let connection = self.connection_handle();
        let connection = connection.borrow();
        if connection.local_name.is_empty() {
            connection.name.clone()
        } else {
            connection.local_name.clone()
        }
    }

    // Upstream parity: local live-driver analogue for reduced projection at the graph boundary.
    // Consumers still read reduced strong-driver records, but the shared live driver owner now
    // projects itself onto that boundary instead of leaving driver snapshot assembly in free
    // helper functions outside the owner graph.
    fn project_onto_reduced(&self, target: &mut ReducedProjectStrongDriver) {
        target.kind = self.kind();
        target.priority = self.priority();
        match self {
            LiveProjectStrongDriverOwner::Floating { connection, .. } => {
                connection
                    .borrow()
                    .project_onto_reduced(&mut target.connection);
            }
            LiveProjectStrongDriverOwner::Label { owner, .. } => {
                owner
                    .upgrade()
                    .expect("live label driver requires an attached owner")
                    .borrow()
                    .project_driver_connection_onto_reduced(&mut target.connection);
            }
            LiveProjectStrongDriverOwner::SheetPin { owner, .. } => {
                owner
                    .upgrade()
                    .expect("live sheet-pin driver requires an attached owner")
                    .borrow()
                    .project_driver_connection_onto_reduced(&mut target.connection);
            }
            LiveProjectStrongDriverOwner::HierPort { owner, .. } => {
                owner
                    .upgrade()
                    .expect("live hierarchical-port driver requires an attached owner")
                    .borrow()
                    .project_driver_connection_onto_reduced(&mut target.connection);
            }
            LiveProjectStrongDriverOwner::SymbolPin { owner, .. } => {
                owner
                    .upgrade()
                    .expect("live symbol-pin driver requires an attached owner")
                    .borrow()
                    .driver_connection
                    .borrow()
                    .project_onto_reduced(&mut target.connection);
            }
        }
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
// identity after `ResolveDrivers()`. This now derives that identity through the chosen reduced
// driver owner stored on the subgraph instead of duplicating the identity as parallel subgraph
// side state, which is closer to KiCad's chosen-driver ownership even though the Rust tree still
// lacks the fuller live driver object graph behind that owner.
pub(crate) fn reduced_project_subgraph_driver_identity(
    subgraph: &ReducedProjectSubgraphEntry,
) -> Option<&ReducedProjectDriverIdentity> {
    subgraph
        .chosen_driver_index
        .and_then(|index| subgraph.drivers.get(index))
        .and_then(|driver| driver.identity.as_ref())
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
struct ReducedProjectSymbolIdentityKey {
    sheet_instance_path: String,
    symbol_uuid: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReducedProjectSymbolPin {
    pub(crate) schematic_path: std::path::PathBuf,
    pub(crate) at: PointKey,
    pub(crate) name: Option<String>,
    pub(crate) number: Option<String>,
    pub(crate) electrical_type: Option<String>,
    pub(crate) visible: bool,
    pub(crate) reference: Option<String>,
    pub(crate) is_power_symbol: bool,
    pub(crate) subgraph_index: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReducedProjectSymbolPinInventory {
    pub(crate) unit: Option<i32>,
    pub(crate) unit_count: usize,
    pub(crate) duplicate_pin_numbers_are_jumpers: bool,
    pub(crate) pins: Vec<ReducedProjectSymbolPin>,
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ReducedProjectSheetPinIdentityKey {
    sheet_instance_path: String,
    at: PointKey,
    child_sheet_uuid: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReducedSubgraphWireItem {
    pub(crate) start: PointKey,
    pub(crate) end: PointKey,
    pub(crate) is_bus_entry: bool,
    pub(crate) start_is_wire_side: bool,
    pub(crate) connected_bus_subgraph_index: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReducedProjectNetGraph {
    subgraphs: Vec<ReducedProjectSubgraphEntry>,
    subgraphs_by_name: BTreeMap<String, Vec<usize>>,
    subgraphs_by_sheet_and_name: BTreeMap<(String, String), Vec<usize>>,
    symbol_pins_by_symbol:
        BTreeMap<ReducedProjectSymbolIdentityKey, ReducedProjectSymbolPinInventory>,
    pin_subgraph_identities: BTreeMap<ReducedNetBasePinKey, usize>,
    pin_subgraph_identities_by_location: BTreeMap<ReducedProjectPinIdentityKey, usize>,
    point_subgraph_identities: BTreeMap<ReducedProjectPointIdentityKey, usize>,
    label_subgraph_identities: BTreeMap<ReducedProjectLabelIdentityKey, usize>,
    no_connect_subgraph_identities: BTreeMap<ReducedProjectNoConnectIdentityKey, usize>,
    sheet_pin_subgraph_identities: BTreeMap<ReducedProjectSheetPinIdentityKey, usize>,
}

pub(crate) struct ReducedProjectGraphInputs<'a> {
    pub(crate) schematics: &'a [Schematic],
    pub(crate) sheet_paths: &'a [LoadedSheetPath],
    pub(crate) project: Option<&'a LoadedProjectSettings>,
    pub(crate) current_variant: Option<&'a str>,
}

fn point_key_matches(key: PointKey, at: [f64; 2]) -> bool {
    points_equal([f64::from_bits(key.0), f64::from_bits(key.1)], at)
}

fn point_key_set_contains(points: &BTreeSet<PointKey>, at: [f64; 2]) -> bool {
    points.iter().copied().any(|key| point_key_matches(key, at))
}

fn bus_entry_preferred_wire_endpoint(
    schematic: &Schematic,
    point_snapshot: &BTreeMap<PointKey, ConnectionPointSnapshot>,
    entry: &crate::model::BusEntry,
) -> [f64; 2] {
    let end = [entry.at[0] + entry.size[0], entry.at[1] + entry.size[1]];
    let endpoint_members = |at| {
        point_snapshot
            .values()
            .find(|point| points_equal(point.at, at))
            .map(|point| point.members.as_slice())
            .unwrap_or(&[])
    };
    let has_wire = |at| {
        schematic.screen.items.iter().any(|item| match item {
            SchItem::Wire(line) => line
                .points
                .windows(2)
                .any(|pair| point_on_wire_segment(at, pair[0], pair[1])),
            _ => false,
        })
    };
    let has_bus = |at| {
        schematic.screen.items.iter().any(|item| match item {
            SchItem::Bus(line) => line
                .points
                .windows(2)
                .any(|pair| point_on_wire_segment(at, pair[0], pair[1])),
            _ => false,
        })
    };
    let has_non_bus_owner = |at| {
        endpoint_members(at).iter().any(|member| {
            matches!(
                member.kind,
                ConnectionMemberKind::Wire
                    | ConnectionMemberKind::SymbolPin
                    | ConnectionMemberKind::SheetPin
                    | ConnectionMemberKind::Label
                    | ConnectionMemberKind::NoConnectMarker
            )
        })
    };

    match (has_non_bus_owner(entry.at), has_non_bus_owner(end)) {
        (true, false) => entry.at,
        (false, true) => end,
        _ => match (has_bus(entry.at), has_bus(end)) {
            (true, false) => end,
            (false, true) => entry.at,
            _ => match (has_wire(entry.at), has_wire(end)) {
                (true, false) => entry.at,
                (false, true) => end,
                _ => entry.at,
            },
        },
    }
}

fn reduced_graph_connection_component_at(
    schematic: &Schematic,
    at: [f64; 2],
) -> Option<ConnectionComponent> {
    collect_reduced_graph_connection_components(schematic)
        .into_iter()
        .find(|component| {
            component
                .members
                .iter()
                .any(|member| points_equal(member.at, at))
        })
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
// Remaining divergence is fuller live `SCH_CONNECTION*` member identity outside the exercised
// reduced snapshot matcher.
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
// detached whole-net entries. Bus members are walked in KiCad's queue order rather than recursive
// depth-first order so first-seen net-code assignment matches `assignNetCodesToBus()`. Remaining
// divergence is the still-missing in-place live clone/update timing on real connection objects
// during propagation.
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
    fn member_at_path_mut<'a>(
        members: &'a mut [ReducedBusMember],
        path: &[usize],
    ) -> Option<&'a mut ReducedBusMember> {
        let (index, rest) = path.split_first()?;
        let member = members.get_mut(*index)?;

        if rest.is_empty() {
            Some(member)
        } else {
            member_at_path_mut(&mut member.members, rest)
        }
    }

    let mut queue = (0..members.len())
        .map(|index| vec![index])
        .collect::<Vec<_>>();
    let mut cursor = 0;

    while cursor < queue.len() {
        let path = queue[cursor].clone();
        cursor += 1;
        let Some(member) = member_at_path_mut(members, &path) else {
            continue;
        };

        if member.kind == ReducedBusMemberKind::Bus {
            member.net_code = 0;
            for index in 0..member.members.len() {
                let mut child_path = path.clone();
                child_path.push(index);
                queue.push(child_path);
            }
        } else if !member.full_local_name.is_empty() {
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

fn live_bus_member_handle_id(handle: &LiveProjectBusMemberHandle) -> usize {
    Rc::as_ptr(handle) as usize
}

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

    // upstream: CONNECTION_GRAPH::matchBusMember vector-bus branch with SCH_CONNECTION search
    // parity_status: partial
    // local_kind: local-only-transitional
    // divergence: derives vector index from reduced connection text because the connection
    // carrier does not yet store SCH_CONNECTION::VectorIndex()
    // local_only_reason: current live connection owner lacks explicit vector-index state
    // replaced_by: full SCH_CONNECTION analogue with stored VectorIndex()
    // remove_when: live connection matching reads VectorIndex() directly
    fn matches_connection_vector_member(&self, search: &LiveProjectConnection) -> bool {
        if let Some(search_index) = reduced_connection_vector_index_guess(search) {
            return self.vector_index == Some(search_index);
        }

        self.matches_connection_member(search)
    }

    // upstream: CONNECTION_GRAPH::matchBusMember group-bus non-vector member branch
    // parity_status: partial
    // local_kind: local-only-transitional
    // divergence: compares reduced live bus-member payloads instead of full SCH_CONNECTIONs
    // local_only_reason: current live graph stores reduced bus-member owners
    // replaced_by: full SCH_CONNECTION member tree owned by the live graph
    // remove_when: live bus member matching runs directly on SCH_CONNECTION analogues
    fn matches_group_member(&self, search: &LiveProjectBusMember) -> bool {
        self.kind != ReducedBusMemberKind::Bus && self.local_name == search.local_name
    }

    // upstream: CONNECTION_GRAPH::matchBusMember group-bus nested-vector branch
    // parity_status: partial
    // local_kind: local-only-transitional
    // divergence: direct nested-vector scan on reduced live bus-member payloads
    // local_only_reason: current live graph stores reduced bus-member owners
    // replaced_by: full SCH_CONNECTION member tree owned by the live graph
    // remove_when: live bus member matching runs directly on SCH_CONNECTION analogues
    fn find_group_vector_member_live(
        &self,
        search: &LiveProjectBusMember,
    ) -> Option<LiveProjectBusMemberHandle> {
        for member in &self.members {
            let member_ref = member.borrow();
            if member_ref.matches_group_member(search) {
                return Some(member.clone());
            }
        }

        None
    }

    // upstream: CONNECTION_GRAPH::matchBusMember group-bus nested-vector branch
    // parity_status: partial
    // local_kind: local-only-transitional
    // divergence: searches with a reduced live connection payload instead of SCH_CONNECTION*
    // local_only_reason: secondary-driver retry still projects drivers to reduced connection owners
    // replaced_by: full SCH_CONNECTION member tree owned by the live graph
    // remove_when: live bus member matching runs directly on SCH_CONNECTION analogues
    fn find_group_vector_connection_member_live(
        &self,
        search: &LiveProjectConnection,
    ) -> Option<LiveProjectBusMemberHandle> {
        for member in &self.members {
            let member_ref = member.borrow();
            if member_ref.matches_connection_member(search) {
                return Some(member.clone());
            }
        }

        None
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

// upstream: SCH_CONNECTION::VectorIndex() as consumed by CONNECTION_GRAPH::matchBusMember
// parity_status: partial
// local_kind: local-only-transitional
// divergence: extracts trailing digits from reduced connection names instead of reading stored
// vector-index state
// local_only_reason: current live connection owner lacks explicit vector-index state
// replaced_by: full SCH_CONNECTION analogue with stored VectorIndex()
// remove_when: live connection matching reads VectorIndex() directly
fn reduced_connection_vector_index_guess(connection: &LiveProjectConnection) -> Option<usize> {
    fn trailing_digits(value: &str) -> Option<usize> {
        let digits = value
            .chars()
            .rev()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>();
        if digits.is_empty() {
            return None;
        }
        digits.chars().rev().collect::<String>().parse().ok()
    }

    trailing_digits(&connection.local_name).or_else(|| trailing_digits(&connection.name))
}

// upstream: CONNECTION_GRAPH::propagateToNeighbors same-parent bus preservation branch
// parity_status: partial
// local_kind: local-only-transitional
// divergence: constructs a reduced live connection from the current neighbor name instead of
// `SCH_CONNECTION temp( nullptr, sheet ); temp.ConfigureFromLabel( neighbor_name )`
// local_only_reason: current propagation still carries reduced connection fields rather than a
// full mutable SCH_CONNECTION object
// replaced_by: live SCH_CONNECTION analogue with ConfigureFromLabel semantics
// remove_when: bus-neighbor propagation owns real SCH_CONNECTION objects
fn reduced_live_net_connection_from_label_name(
    name: &str,
    sheet_instance_path: &str,
) -> LiveProjectConnection {
    LiveProjectConnection {
        net_code: 0,
        connection_type: ReducedProjectConnectionType::Net,
        name: name.to_string(),
        local_name: reduced_short_net_name(name),
        full_local_name: name.to_string(),
        sheet_instance_path: sheet_instance_path.to_string(),
        members: Vec::new(),
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

    // Upstream parity: local live-owner bridge toward the common
    // `SCH_CONNECTION::Clone()` + dirty-recache call sites in `CONNECTION_GRAPH`. This still
    // compares reduced live connection payloads instead of full KiCad `SCH_CONNECTION` objects,
    // but the connection owner now decides whether cloning would mutate its state so
    // `CONNECTION_SUBGRAPH` propagation stops carrying a parallel clone-equality policy.
    fn clone_from_live_connection_if_changed(&mut self, source: &LiveProjectConnection) -> bool {
        if live_connection_clone_eq(self, source) {
            return false;
        }

        self.clone_from_live_connection(source);
        true
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
            let matches = if self.connection_type == ReducedProjectConnectionType::BusGroup {
                member_ref.matches_group_member(search)
            } else {
                member_ref.matches_live_member(search)
            };
            if matches {
                return Some(member.clone());
            }

            if member_ref.kind == ReducedBusMemberKind::Bus {
                let found = if self.connection_type == ReducedProjectConnectionType::BusGroup {
                    member_ref.find_group_vector_member_live(search)
                } else {
                    member_ref.find_descendant_live(search)
                };
                if let Some(found) = found {
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
            let matches = if self.connection_type == ReducedProjectConnectionType::Bus {
                member_ref.matches_connection_vector_member(search)
            } else {
                member_ref.matches_connection_member(search)
            };
            if matches {
                return Some(member.clone());
            }

            if member_ref.kind == ReducedBusMemberKind::Bus {
                let found = if self.connection_type == ReducedProjectConnectionType::BusGroup {
                    member_ref.find_group_vector_connection_member_live(search)
                } else {
                    member_ref.find_descendant_for_connection(search)
                };
                if let Some(found) = found {
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
            let matches = if self.connection_type == ReducedProjectConnectionType::BusGroup {
                member_ref.matches_group_member(search)
            } else {
                member_ref.matches_live_member(search)
            };
            if matches {
                return Some(member.clone());
            }

            if member_ref.kind == ReducedBusMemberKind::Bus {
                drop(member_ref);
                let found = if self.connection_type == ReducedProjectConnectionType::BusGroup {
                    member.borrow().find_group_vector_member_live(search)
                } else {
                    member.borrow_mut().find_descendant_mut_live(search)
                };
                if let Some(found) = found {
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

fn clone_live_connection_owner_into_live_connection_owner_if_changed(
    target: &mut LiveProjectConnection,
    source: &LiveProjectConnection,
) -> bool {
    target.clone_from_live_connection_if_changed(source)
}

fn clone_live_connection_handle_from_handle_if_changed(
    target: &LiveProjectConnectionHandle,
    source: &LiveProjectConnectionHandle,
) -> bool {
    if Rc::ptr_eq(target, source) {
        return false;
    }

    clone_live_connection_owner_into_live_connection_owner_if_changed(
        &mut target.borrow_mut(),
        &source.borrow(),
    )
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
    preserved_local_name: Option<&str>,
) {
    clone_live_connection_owner_into_live_connection_owner(target, source);

    if let Some(local_name) = preserved_local_name {
        target.local_name = local_name.to_string();
    }
}

#[derive(Clone, Debug)]
struct LiveReducedLabelLink {
    schematic_path: std::path::PathBuf,
    at: PointKey,
    kind: LabelKind,
    dangling: bool,
    connection: LiveProjectConnectionHandle,
    driver_connection: LiveProjectConnectionHandle,
    driver: Option<LiveProjectStrongDriverHandle>,
    shown_text_local_name: String,
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
    shown_text_local_name: String,
}
type LiveReducedHierSheetPinLinkHandle = Rc<RefCell<LiveReducedHierSheetPinLink>>;

#[derive(Clone, Debug)]
struct LiveReducedHierPortLink {
    schematic_path: std::path::PathBuf,
    at: PointKey,
    connection: LiveProjectConnectionHandle,
    driver_connection: LiveProjectConnectionHandle,
    driver: Option<LiveProjectStrongDriverHandle>,
    shown_text_local_name: String,
}
type LiveReducedHierPortLinkHandle = Rc<RefCell<LiveReducedHierPortLink>>;

#[derive(Clone, Debug)]
struct LiveReducedBasePinPayload {
    schematic_path: std::path::PathBuf,
    key: ReducedNetBasePinKey,
    reference: Option<String>,
    number: Option<String>,
    electrical_type: Option<String>,
    visible: bool,
    is_power_symbol: bool,
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
    preserved_local_name: Option<String>,
}

type LiveReducedBasePinHandle = Rc<RefCell<LiveReducedBasePin>>;

impl LiveReducedLabelLink {
    // Upstream parity: local item-owner analogue for the exercised
    // `CONNECTION_SUBGRAPH::UpdateItemConnections()` label branch. This still mutates a reduced
    // live item carrier instead of a real `SCH_LABEL`, but the label owner now decides whether to
    // adopt the chosen live connection, keeps shown-text local-name ownership on the label owner,
    // and keeps the `item != m_driver` skip on the owner path instead of leaving that policy in a
    // separate helper loop.
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

        clone_live_connection_owner_into_live_connection_owner(
            &mut self.connection.borrow_mut(),
            &driver_connection.borrow(),
        );
        if !self.shown_text_local_name.is_empty() {
            self.connection.borrow_mut().local_name = self.shown_text_local_name.clone();
        }
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

    fn project_item_connection_onto_reduced(&self, target: &mut ReducedProjectConnection) {
        self.connection.borrow().project_onto_reduced(target);
        if !self.shown_text_local_name.is_empty() {
            target.local_name = self.shown_text_local_name.clone();
        }
    }

    // Upstream parity: local text-driver projection analogue for
    // `CONNECTION_SUBGRAPH::GetNameForDriver()` on label drivers. The live connection may have
    // been propagated to the chosen net name, but the driver-conflict name remains owned by the
    // label shown text until the fuller `SCH_LABEL_BASE` driver item is ported.
    fn project_driver_connection_onto_reduced(&self, target: &mut ReducedProjectConnection) {
        self.driver_connection.borrow().project_onto_reduced(target);
        if !self.shown_text_local_name.is_empty() {
            target.local_name = self.shown_text_local_name.clone();
        }
    }
}

impl LiveReducedHierSheetPinLink {
    // Upstream parity: local item-owner analogue for the exercised sheet-pin branch of
    // `CONNECTION_SUBGRAPH::UpdateItemConnections()`. This still runs on a reduced live link owner
    // instead of a real `SCH_SHEET_PIN`, but the owner now applies the chosen-driver skip,
    // keeps shown-text local-name ownership on the sheet-pin owner, and applies the
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

        clone_live_connection_owner_into_live_connection_owner(
            &mut self.connection.borrow_mut(),
            &driver_connection.borrow(),
        );
        if !self.shown_text_local_name.is_empty() {
            self.connection.borrow_mut().local_name = self.shown_text_local_name.clone();
        }
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

    fn project_item_connection_onto_reduced(&self, target: &mut ReducedProjectConnection) {
        self.connection.borrow().project_onto_reduced(target);
        if !self.shown_text_local_name.is_empty() {
            target.local_name = self.shown_text_local_name.clone();
        }
    }

    // Upstream parity: local sheet-pin driver projection analogue for
    // `CONNECTION_SUBGRAPH::GetNameForDriver()`. The reduced live owner still carries the shown
    // text instead of a full `SCH_SHEET_PIN`, so projection reapplies that text after connection
    // propagation has rewritten the attached driver connection.
    fn project_driver_connection_onto_reduced(&self, target: &mut ReducedProjectConnection) {
        self.driver_connection.borrow().project_onto_reduced(target);
        if !self.shown_text_local_name.is_empty() {
            target.local_name = self.shown_text_local_name.clone();
        }
    }
}

impl LiveReducedHierPortLink {
    // Upstream parity: local item-owner analogue for the exercised hierarchical-port branch of
    // `CONNECTION_SUBGRAPH::UpdateItemConnections()`. The live owner still wraps reduced payload,
    // but it now owns shown-text local-name preservation and the exercised update decision instead
    // of helper-side branch duplication.
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

        clone_live_connection_owner_into_live_connection_owner(
            &mut self.connection.borrow_mut(),
            &driver_connection.borrow(),
        );
        if !self.shown_text_local_name.is_empty() {
            self.connection.borrow_mut().local_name = self.shown_text_local_name.clone();
        }
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

    fn project_item_connection_onto_reduced(&self, target: &mut ReducedProjectConnection) {
        self.connection.borrow().project_onto_reduced(target);
        if !self.shown_text_local_name.is_empty() {
            target.local_name = self.shown_text_local_name.clone();
        }
    }

    // Upstream parity: local hierarchical-label driver projection analogue for
    // `CONNECTION_SUBGRAPH::GetNameForDriver()`. This preserves the hierarchical-port shown text
    // for driver diagnostics even when the live driver connection has been cloned from a stronger
    // hierarchy-chain driver.
    fn project_driver_connection_onto_reduced(&self, target: &mut ReducedProjectConnection) {
        self.driver_connection.borrow().project_onto_reduced(target);
        if !self.shown_text_local_name.is_empty() {
            target.local_name = self.shown_text_local_name.clone();
        }
    }
}

impl LiveReducedBasePin {
    // Upstream parity: local pin-owner analogue for the exercised pin branch of
    // `CONNECTION_SUBGRAPH::UpdateItemConnections()`. This still updates a reduced live base-pin
    // owner instead of a real `SCH_PIN`, but the owner now decides whether to preserve explicit
    // pin-local text, skip the chosen driver, and adopt the chosen live connection. Attached
    // strong-driver pins now widen their dedicated pin-driver connection owner onto that same
    // chosen net identity while preserving explicit pin-owned local driver text through owner
    // state instead of a clone-time string heuristic, so active symbol-pin driver reads stop
    // staying on pre-propagation setup snapshots after graph updates.
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

        if !Rc::ptr_eq(&self.connection, driver_connection) {
            clone_live_connection_owner_into_live_base_pin_connection_owner(
                &mut self.connection.borrow_mut(),
                &driver_connection.borrow(),
                self.preserved_local_name.as_deref(),
            );
        }

        if refresh_attached_strong_driver_pins && self.driver.is_some() {
            if !Rc::ptr_eq(&self.driver_connection, driver_connection) {
                clone_live_connection_owner_into_live_base_pin_connection_owner(
                    &mut self.driver_connection.borrow_mut(),
                    &driver_connection.borrow(),
                    self.preserved_local_name.as_deref(),
                );
            }
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
            None,
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
    // the owner itself instead of re-deriving it from connection strings during later refresh.
    fn attach_strong_driver(
        &mut self,
        owner: &LiveReducedBasePinHandle,
        driver: &LiveProjectStrongDriverHandle,
        kind: ReducedProjectDriverKind,
        priority: i32,
    ) -> LiveProjectStrongDriverOwner {
        self.driver = Some(driver.clone());
        self.preserved_local_name = Some(self.driver_connection.borrow().local_name.clone());
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
            &self.shown_text_local_name,
            self.connection.borrow().snapshot(),
            self.driver_connection.borrow().snapshot(),
            live_optional_driver_snapshot(&self.driver),
        ) == (
            &other.schematic_path,
            other.at,
            &other.child_sheet_uuid,
            &other.shown_text_local_name,
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
            &self.shown_text_local_name,
            self.connection.borrow().snapshot(),
            self.driver_connection.borrow().snapshot(),
            live_optional_driver_snapshot(&self.driver),
        )
            .cmp(&(
                &other.schematic_path,
                other.at,
                &other.child_sheet_uuid,
                &other.shown_text_local_name,
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
            &self.shown_text_local_name,
            self.connection.borrow().snapshot(),
            self.driver_connection.borrow().snapshot(),
            live_optional_driver_snapshot(&self.driver),
        ) == (
            &other.schematic_path,
            other.at,
            &other.shown_text_local_name,
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
            &self.shown_text_local_name,
            self.connection.borrow().snapshot(),
            self.driver_connection.borrow().snapshot(),
            live_optional_driver_snapshot(&self.driver),
        )
            .cmp(&(
                &other.schematic_path,
                other.at,
                &other.shown_text_local_name,
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
    start_is_wire_side: bool,
    connection: LiveProjectConnectionHandle,
    connected_bus_connection_handle: Option<LiveProjectConnectionHandle>,
}

impl LiveReducedSubgraphWireItem {
    // Upstream parity: local wire-item analogue for the exercised connected-bus attachment KiCad
    // keeps on bus entries during graph build. This still identifies the attached bus from reduced
    // wire geometry instead of real `SCH_LINE*` pointers, but the shared wire-item owner now owns
    // the geometric match plus the attached-bus connection handle instead of leaving that
    // decision in a free graph builder loop or routing it through a second bus-item indirection
    // on the active path. Reduced projection derives the attached bus subgraph index back out
    // from that shared bus connection owner instead of keeping a second live subgraph weak handle
    // on the active wire-item path. Remaining divergence is the still-missing fuller live item
    // pointer graph beyond these direct owner handles.
    fn attach_connected_bus_subgraph(
        &mut self,
        bus_side: PointKey,
        sheet_instance_path: &str,
        bus_subgraphs: &[(
            String,
            LiveProjectConnectionHandle,
            Weak<RefCell<LiveReducedSubgraph>>,
            Vec<(PointKey, PointKey)>,
        )],
    ) {
        if !self.is_bus_entry {
            return;
        }

        let attached_bus = bus_subgraphs.iter().find_map(
            |(bus_sheet_path, bus_connection, _bus_subgraph, bus_segments)| {
                (*bus_sheet_path == sheet_instance_path
                    && bus_segments.iter().any(|(start, end)| {
                        point_on_wire_segment(
                            [f64::from_bits(bus_side.0), f64::from_bits(bus_side.1)],
                            [f64::from_bits(start.0), f64::from_bits(start.1)],
                            [f64::from_bits(end.0), f64::from_bits(end.1)],
                        )
                    }))
                .then(|| bus_connection.clone())
            },
        );

        match attached_bus {
            Some(connection) => {
                self.connected_bus_connection_handle = Some(connection);
            }
            None => {
                self.connected_bus_connection_handle = None;
            }
        }
    }

    // Upstream parity: local wire/bus-item analogue for the exercised item loop inside
    // `CONNECTION_SUBGRAPH::UpdateItemConnections()`. This still refreshes reduced wire geometry
    // owners instead of real `SCH_LINE*` item connections, but each live item now keeps its own
    // connection owner and clones the chosen subgraph driver into that owner with the same
    // bus/net mismatch guard KiCad applies before item mutation. Remaining divergence is the
    // still-missing fuller live item pointer graph, not shared-driver aliasing on this item path.
    fn refresh_from_driver_connection(
        &mut self,
        driver_connection: &LiveProjectConnectionHandle,
        driver_connection_type: ReducedProjectConnectionType,
    ) {
        if reduced_connection_kind_mismatch(
            driver_connection_type,
            self.connection.borrow().connection_type,
        ) {
            return;
        }

        clone_live_connection_owner_into_live_connection_owner(
            &mut self.connection.borrow_mut(),
            &driver_connection.borrow(),
        );
    }
}

impl PartialEq for LiveReducedSubgraphWireItem {
    fn eq(&self, other: &Self) -> bool {
        self.start == other.start
            && self.end == other.end
            && self.is_bus_entry == other.is_bus_entry
            && self.start_is_wire_side == other.start_is_wire_side
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
    // allocation. Strong-driver owner attachment on the active path now also reads identity/kind/
    // priority from the live driver owners themselves instead of zipping back through reduced
    // strong-driver records during build, and chosen-driver binding now follows the reduced chosen
    // driver slot directly onto the live driver handle list instead of re-deriving chosen
    // identity from the reduced strong-driver vector. Remaining divergence is the still-missing
    // fuller live driver-item graph, not reduced strong-driver metadata on the active attachment
    // path.
    fn attach_from_reduced(
        &mut self,
        reduced_subgraph: &ReducedProjectSubgraphEntry,
        live_subgraphs: &[LiveReducedSubgraphHandle],
    ) {
        self.attach_topology_from_reduced(reduced_subgraph, live_subgraphs);

        let chosen_connection = self.driver_connection.clone();
        let live_drivers = self.drivers.clone();
        let chosen_driver = reduced_subgraph
            .chosen_driver_index
            .and_then(|index| live_drivers.get(index).cloned());

        for driver in &live_drivers {
            let driver_ref = driver.borrow();
            let identity = driver_ref.identity();
            let driver_kind = driver_ref.kind();
            let priority = driver_ref.priority();
            let floating_connection = match &*driver.borrow() {
                LiveProjectStrongDriverOwner::Floating { connection, .. } => connection.clone(),
                _ => driver_ref.connection_handle(),
            };
            drop(driver_ref);
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

            self.attach_strong_driver(driver, chosen_driver.as_ref(), &chosen_connection);
        }

        self.refresh_base_pin_connections_from_driver(false);
        self.attach_item_connections_from_driver();
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
                        subgraph.driver_connection.clone(),
                        Rc::downgrade(handle),
                        subgraph
                            .bus_items
                            .iter()
                            .map(|item| {
                                let item = item.borrow();
                                (item.start, item.end)
                            })
                            .collect::<Vec<_>>(),
                    )
                })
            })
            .collect::<Vec<_>>();

        for handle in live_subgraphs {
            let sheet_instance_path = {
                let subgraph = handle.borrow();
                subgraph.sheet_instance_path.clone()
            };
            let subgraph = handle.borrow_mut();

            for item_handle in &subgraph.wire_items {
                let bus_side = {
                    let item = item_handle.borrow();
                    if item.start_is_wire_side {
                        item.end
                    } else {
                        item.start
                    }
                };
                item_handle.borrow_mut().attach_connected_bus_subgraph(
                    bus_side,
                    &sheet_instance_path,
                    &bus_subgraphs,
                );
            }
        }
    }

    // Upstream parity: local live-subgraph analogue for binding one exercised strong driver onto
    // the shared subgraph owner during driver resolution. This still seeds from reduced projected
    // identities instead of a fuller live `ResolveDrivers()` object graph, but the subgraph owner
    // now owns chosen-driver adoption and chosen-driver-connection attachment instead of leaving
    // that branch open-coded in the surrounding builder. Symbol-pin and text-item branches now
    // compare through attached live owner-side driver connections against the already-seeded live
    // subgraph driver handle. Active build now follows the reduced chosen-driver slot onto the
    // live driver handle list directly instead of re-deriving chosen identity from reduced strong
    // drivers during owner attachment. Chosen symbol-pin owners now also alias their item
    // connection onto that chosen driver handle, while chosen text-item owners still keep split
    // item-vs-driver connections and preserve shown-text ownership explicitly on the item owner.
    // Remaining divergence is the still-missing fuller live driver-item object graph, not reduced
    // chosen-driver matching on the active path.
    fn attach_strong_driver(
        &mut self,
        driver: &LiveProjectStrongDriverHandle,
        chosen_driver: Option<&LiveProjectStrongDriverHandle>,
        chosen_connection: &LiveProjectConnectionHandle,
    ) {
        let is_chosen_driver = chosen_driver
            .map(|chosen| Rc::ptr_eq(driver, chosen))
            .unwrap_or_else(|| {
                let driver_connection = driver.borrow().connection_handle();
                *driver_connection.borrow() == *chosen_connection.borrow()
            });

        if is_chosen_driver {
            self.chosen_driver = Some(driver.clone());
            self.driver_connection = driver.borrow().connection_handle();
            match &*driver.borrow() {
                LiveProjectStrongDriverOwner::SymbolPin { owner, .. } => {
                    if let Some(base_pin) = owner.upgrade() {
                        base_pin
                            .borrow_mut()
                            .adopt_driver_connection_as_item_connection();
                    }
                }
                LiveProjectStrongDriverOwner::Label { .. }
                | LiveProjectStrongDriverOwner::SheetPin { .. }
                | LiveProjectStrongDriverOwner::HierPort { .. }
                | LiveProjectStrongDriverOwner::Floating { .. } => {}
            }
        }
    }

    // Upstream parity: local live-subgraph analogue for binding one reduced strong-driver
    // identity back onto the exercised live item owner it belongs to before chosen-driver
    // selection. Production graph build now requires those exercised drivers to bind to concrete
    // live item owners; unattached floating owners remain only for reduced/manual test scaffolding
    // until the fuller live driver-item graph exists. The shared subgraph owner now owns the
    // item-match and attachment flow instead of leaving that selection as one large free-function
    // match around the graph.
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
                        .unwrap_or_else(|| {
                            unattached_live_strong_driver_owner(
                                fallback_identity,
                                floating_connection,
                                driver_kind,
                                priority,
                            )
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
                        .unwrap_or_else(|| {
                            unattached_live_strong_driver_owner(
                                fallback_identity,
                                floating_connection,
                                driver_kind,
                                priority,
                            )
                        })
                }
            }
            Some(ReducedProjectDriverIdentity::SheetPin {
                at,
                child_sheet_uuid,
                ..
            }) => self
                .hier_sheet_pins
                .iter()
                .find(|pin| {
                    let pin = pin.borrow();
                    pin.at == at && pin.child_sheet_uuid == child_sheet_uuid
                })
                .map(|pin| {
                    pin.borrow_mut().attach_strong_driver(
                        pin,
                        floating_connection,
                        driver,
                        driver_kind,
                        priority,
                    )
                })
                .unwrap_or_else(|| {
                    unattached_live_strong_driver_owner(
                        fallback_identity,
                        floating_connection,
                        driver_kind,
                        priority,
                    )
                }),
            Some(ReducedProjectDriverIdentity::SymbolPin {
                sheet_instance_path,
                symbol_uuid,
                at,
                pin_number,
                ..
            }) => self
                .base_pins
                .iter()
                .find(|base_pin| {
                    let key = &base_pin.borrow().pin.key;
                    key.sheet_instance_path == sheet_instance_path
                        && key.symbol_uuid.as_ref() == symbol_uuid.as_ref()
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
                .unwrap_or_else(|| {
                    unattached_live_strong_driver_owner(
                        fallback_identity,
                        floating_connection,
                        driver_kind,
                        priority,
                    )
                }),
            None => unattached_live_strong_driver_owner(
                None,
                floating_connection,
                driver_kind,
                priority,
            ),
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

    // Upstream parity: local live-subgraph analogue for binding exercised wire/bus item
    // connections onto the chosen live subgraph connection during graph build. The item payload is
    // still reduced, but the shared subgraph owner now owns that exercised item-connection
    // attachment instead of leaving it in a separate builder loop. Remaining divergence is the
    // still-missing fuller live item pointer graph beyond these direct connection handles.
    fn attach_item_connections_from_driver(&mut self) {
        for item in &self.bus_items {
            item.borrow_mut().refresh_from_driver_connection(
                &self.driver_connection,
                self.driver_connection.borrow().connection_type,
            );
        }
        for item in &self.wire_items {
            item.borrow_mut().refresh_from_driver_connection(
                &self.driver_connection,
                self.driver_connection.borrow().connection_type,
            );
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
        for item in &self.bus_items {
            item.borrow_mut()
                .refresh_from_driver_connection(&driver_connection, driver_connection_type);
        }
        for item in &self.wire_items {
            item.borrow_mut()
                .refresh_from_driver_connection(&driver_connection, driver_connection_type);
        }

        self.refresh_base_pin_connections_from_driver(true);
    }

    // Upstream parity: local live-subgraph analogue for the exercised reduced projection boundary
    // after graph mutation. Consumers still read reduced graph state, but the shared live subgraph
    // owner now pushes its chosen-driver owner slot, strong drivers, and item/pin connection
    // owners onto that boundary while the reduced subgraph owner re-derives outward name/resolved
    // connection state from the projected driver owner instead of assigning those boundary fields
    // independently in this loop.
    fn project_driver_and_item_state_onto_reduced(
        &self,
        reduced: &mut ReducedProjectSubgraphEntry,
    ) {
        let live_driver = self.driver_connection.borrow();
        live_driver.project_onto_reduced(&mut reduced.driver_connection);
        reduced.sync_boundary_state_from_driver_owner();
        reduced.drivers = live_strong_driver_handles_to_snapshots(&self.drivers);
        reduced.chosen_driver_index = self.chosen_driver.as_ref().and_then(|chosen| {
            self.drivers
                .iter()
                .position(|driver| Rc::ptr_eq(driver, chosen))
        });

        for (target, source) in reduced.label_links.iter_mut().zip(self.label_links.iter()) {
            let source = source.borrow();
            source.project_item_connection_onto_reduced(&mut target.connection);
        }

        for (target, source) in reduced
            .hier_sheet_pins
            .iter_mut()
            .zip(self.hier_sheet_pins.iter())
        {
            let source = source.borrow();
            source.project_item_connection_onto_reduced(&mut target.connection);
        }

        for (target, source) in reduced.hier_ports.iter_mut().zip(self.hier_ports.iter()) {
            let source = source.borrow();
            source.project_item_connection_onto_reduced(&mut target.connection);
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
            target.connected_bus_subgraph_index = source
                .borrow()
                .connected_bus_connection_handle
                .as_ref()
                .and_then(|connection| {
                    live_subgraphs.iter().position(|candidate| {
                        Rc::ptr_eq(&candidate.borrow().driver_connection, connection)
                    })
                });
        }
    }

    // Upstream parity: local live-subgraph analogue for the exercised post-propagation
    // `UpdateItemConnections()` follow-up branches KiCad runs after names settle. This still
    // operates on reduced live owners instead of fuller item pointers, but the shared subgraph
    // owner now owns the self-driven symbol-pin no-connect rename refresh and the self-driven
    // sheet-pin child-bus promotion branch instead of leaving those branches open-coded in the
    // outer handle loop.
    fn refresh_post_propagation_item_connections(handle: &LiveReducedSubgraphHandle) {
        handle.borrow_mut().dirty = false;
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
                        let parent = handle.borrow();
                        let child = child_handle.borrow();
                        let child_connection = child.driver_connection.borrow();

                        let matching_hier_port = parent.hier_sheet_pins.iter().any(|pin| {
                            let pin = pin.borrow();
                            child.hier_ports.iter().any(|port| {
                                port.borrow().connection.borrow().local_name
                                    == pin.connection.borrow().local_name
                            })
                        });
                        (matching_hier_port
                            && matches!(
                                child_connection.connection_type,
                                ReducedProjectConnectionType::Bus
                                    | ReducedProjectConnectionType::BusGroup
                            ))
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
            }
        }
    }

    // Upstream parity: local live-subgraph analogue for the hierarchy-chain slice inside
    // `propagateToNeighbors()`. This still mutates reduced live carriers instead of full local
    // `CONNECTION_SUBGRAPH` objects, but the shared subgraph owner now owns the traversal,
    // chosen-driver rewrite, and immediate bus-neighbor propagation for one hierarchy-connected
    // component. The rewrite now stays on the chosen live driver handle instead of snapshotting a
    // reduced-shaped chosen connection through the active propagation path.
    fn propagate_hierarchy_chain(
        start: &LiveReducedSubgraphHandle,
        live_subgraphs: &[LiveReducedSubgraphHandle],
        force: bool,
        stale_members: &mut Vec<LiveProjectBusMemberHandle>,
        defer_dirty: bool,
    ) {
        let start_has_hier_ports = !start.borrow().hier_ports.is_empty();
        let start_has_hier_pins = !start.borrow().hier_sheet_pins.is_empty();
        if !force && start_has_hier_ports && start_has_hier_pins {
            start.borrow_mut().dirty = defer_dirty;
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
                if parent_handle.borrow().hier_sheet_pins.is_empty() {
                } else if parent_handle
                    .borrow()
                    .driver_connection
                    .borrow()
                    .connection_type
                    != handle.borrow().driver_connection.borrow().connection_type
                {
                } else if live_subgraphs_have_matching_hierarchy_driver_names(
                    &parent_handle,
                    &handle,
                ) && visited_set.insert(live_subgraph_handle_id(&parent_handle))
                {
                    stack.push(parent_handle);
                }
            }

            for child_handle in live_subgraph_child_handles_from_handle(&handle) {
                if child_handle.borrow().hier_ports.is_empty() {
                    continue;
                }
                if live_subgraph_strong_driver_count(&child_handle.borrow()) == 0 {
                    continue;
                }
                if !live_subgraphs_have_matching_hierarchy_driver_names(&handle, &child_handle) {
                    continue;
                }
                if visited_set.insert(live_subgraph_handle_id(&child_handle)) {
                    stack.push(child_handle);
                }
            }
        }

        let mut best_handle = start.clone();
        let mut highest = live_reduced_subgraph_driver_priority(&start.borrow());
        let mut best_is_strong = highest >= reduced_hierarchical_label_driver_priority();
        let mut best_name = start.borrow().driver_connection.borrow().name.clone();

        if highest < reduced_global_power_pin_driver_priority() {
            for handle in visited.iter().filter(|handle| !Rc::ptr_eq(handle, start)) {
                let priority = live_reduced_subgraph_driver_priority(&handle.borrow());
                let candidate_strong = priority >= reduced_hierarchical_label_driver_priority();
                let candidate_name = handle.borrow().driver_connection.borrow().name.clone();
                let candidate_depth =
                    reduced_sheet_path_depth(&handle.borrow().sheet_instance_path);
                let best_depth =
                    reduced_sheet_path_depth(&best_handle.borrow().sheet_instance_path);
                let shorter_path = candidate_depth < best_depth;
                let as_good_path = candidate_depth <= best_depth;

                if (priority >= reduced_global_power_pin_driver_priority())
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
        let chosen_is_bus = matches!(
            chosen_connection.borrow().connection_type,
            ReducedProjectConnectionType::Bus | ReducedProjectConnectionType::BusGroup
        );

        for handle in &visited {
            let target_connection = handle.borrow().driver_connection.clone();
            clone_live_connection_handle_from_handle_if_changed(
                &target_connection,
                &chosen_connection,
            );
            handle.borrow_mut().dirty = false;
        }

        if chosen_is_bus {
            Self::refresh_bus_neighbor_drivers(live_subgraphs, &visited, stale_members);
        }
    }

    // Upstream parity: local live-subgraph analogue for collecting the hierarchy-visited slice
    // around one dirty subgraph during `propagateToNeighbors()`. Bus-parent and bus-neighbor
    // mutations are handled by their own branches and should not expand this visited chain. Parent
    // and child hierarchy scans intentionally remain independent, matching upstream's two visit
    // loops in `propagateToNeighbors()`.
    fn collect_propagation_component_handles(
        start: &LiveReducedSubgraphHandle,
        _live_subgraphs: &[LiveReducedSubgraphHandle],
    ) -> Vec<LiveReducedSubgraphHandle> {
        let mut queue = VecDeque::from([start.clone()]);
        let mut visited = BTreeSet::from([live_subgraph_handle_id(start)]);
        let mut component = Vec::new();

        while let Some(handle) = queue.pop_front() {
            component.push(handle.clone());
            if let Some(parent_handle) = live_subgraph_parent_handle_from_handle(&handle) {
                if parent_handle.borrow().hier_sheet_pins.is_empty() {
                } else if parent_handle
                    .borrow()
                    .driver_connection
                    .borrow()
                    .connection_type
                    != handle.borrow().driver_connection.borrow().connection_type
                {
                } else if live_subgraphs_have_matching_hierarchy_driver_names(
                    &parent_handle,
                    &handle,
                ) && visited.insert(live_subgraph_handle_id(&parent_handle))
                {
                    queue.push_back(parent_handle);
                }
            }

            for child_handle in live_subgraph_child_handles_from_handle(&handle) {
                if child_handle.borrow().hier_ports.is_empty() {
                    continue;
                }
                if live_subgraph_strong_driver_count(&child_handle.borrow()) == 0 {
                    continue;
                }
                if !live_subgraphs_have_matching_hierarchy_driver_names(&handle, &child_handle) {
                    continue;
                }
                if visited.insert(live_subgraph_handle_id(&child_handle)) {
                    queue.push_back(child_handle);
                }
            }
        }

        component.sort_by_key(live_subgraph_handle_id);
        component
    }

    // Upstream parity: local live-subgraph analogue for the `global_subgraphs` snapshot KiCad
    // builds once from `!m_local_driver` candidates before the secondary-driver promotion pass.
    fn collect_global_subgraph_handles(
        live_subgraphs: &[LiveReducedSubgraphHandle],
    ) -> Vec<LiveReducedSubgraphHandle> {
        live_subgraphs
            .iter()
            .filter(|handle| !live_subgraph_has_local_driver(&handle.borrow()))
            .cloned()
            .collect()
    }

    // Upstream parity: local live-subgraph analogue for the global-secondary-driver promotion
    // branch KiCad runs before neighbor propagation. The active path now keeps the promotion walk
    // on the shared subgraph owner instead of an outer free helper around the handle graph.
    fn refresh_global_secondary_driver_promotions(
        start: &LiveReducedSubgraphHandle,
        global_subgraphs: &[LiveReducedSubgraphHandle],
    ) -> Vec<LiveReducedSubgraphHandle> {
        if live_subgraph_has_local_driver(&start.borrow())
            || live_subgraph_strong_driver_count(&start.borrow()) < 2
        {
            return Vec::new();
        }

        let chosen_connection = start.borrow().driver_connection.clone();
        let chosen_driver = start.borrow().chosen_driver.clone();
        let start_sheet = start.borrow().sheet_instance_path.clone();
        let secondary_drivers = start.borrow().drivers.clone();
        let mut promoted = Vec::new();

        for secondary_driver in secondary_drivers {
            if chosen_driver
                .as_ref()
                .is_some_and(|chosen_driver| Rc::ptr_eq(chosen_driver, &secondary_driver))
            {
                continue;
            }

            let secondary_name = secondary_driver.borrow().driver_name();

            if secondary_name == chosen_connection.borrow().name {
                continue;
            }

            let secondary_is_global =
                secondary_driver.borrow().priority() >= reduced_global_power_pin_driver_priority();

            for handle in global_subgraphs.iter() {
                if Rc::ptr_eq(handle, start) {
                    continue;
                }

                if !secondary_is_global && handle.borrow().sheet_instance_path != start_sheet {
                    continue;
                }

                if !handle.borrow().drivers.iter().any(|candidate_driver| {
                    candidate_driver.borrow().driver_name() == secondary_name
                }) {
                    continue;
                }

                let target_connection = handle.borrow().driver_connection.clone();
                clone_live_connection_handle_from_handle_if_changed(
                    &target_connection,
                    &chosen_connection,
                );
                handle.borrow_mut().dirty = false;

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
    fn propagate_neighbors_from_selected_start(
        start: &LiveReducedSubgraphHandle,
        live_subgraphs: &[LiveReducedSubgraphHandle],
        global_subgraphs: &[LiveReducedSubgraphHandle],
        force: bool,
        visiting: &mut BTreeSet<usize>,
        stale_members: &mut Vec<LiveProjectBusMemberHandle>,
        selected_clean_start: bool,
    ) {
        let start_id = live_subgraph_handle_id(start);
        if (!start.borrow().dirty && !selected_clean_start) || !visiting.insert(start_id) {
            return;
        }

        if !force {
            let promoted =
                Self::refresh_global_secondary_driver_promotions(start, global_subgraphs);

            for promoted_handle in promoted {
                Self::propagate_neighbors_from_selected_start(
                    &promoted_handle,
                    live_subgraphs,
                    global_subgraphs,
                    false,
                    visiting,
                    stale_members,
                    true,
                );
            }
        }

        let active = Self::collect_propagation_component_handles(start, live_subgraphs);
        let mut dirty_active = active
            .iter()
            .filter(|handle| handle.borrow().dirty)
            .cloned()
            .collect::<Vec<_>>();

        if selected_clean_start && !dirty_active.iter().any(|handle| Rc::ptr_eq(handle, start)) {
            dirty_active.push(start.clone());
        }

        for handle in &dirty_active {
            handle.borrow_mut().dirty = false;
        }

        let bus_neighbor_recurse_targets =
            Self::refresh_bus_neighbor_drivers(live_subgraphs, &dirty_active, stale_members);

        for (target, member) in bus_neighbor_recurse_targets {
            if Rc::ptr_eq(&target, start) {
                continue;
            }

            Self::propagate_neighbors_from_selected_start(
                &target,
                live_subgraphs,
                global_subgraphs,
                force,
                visiting,
                stale_members,
                false,
            );

            let target_connection = target.borrow().driver_connection.clone();
            if target_connection.borrow().full_local_name != member.borrow().full_local_name {
                clone_live_connection_owner_into_live_bus_member(
                    &mut member.borrow_mut(),
                    &target_connection.borrow(),
                );

                if !stale_members
                    .iter()
                    .any(|candidate| live_bus_member_handles_eq(candidate, &member))
                {
                    stale_members.push(member);
                }
            }
        }

        for handle in &dirty_active {
            let has_hierarchy_links = live_subgraph_has_hierarchy_handles_from_handle(handle);

            if !has_hierarchy_links {
                continue;
            }

            let defer_dirty = !(selected_clean_start && Rc::ptr_eq(handle, start));
            Self::propagate_hierarchy_chain(
                handle,
                live_subgraphs,
                force,
                stale_members,
                defer_dirty,
            );
        }
        Self::refresh_bus_parent_members(live_subgraphs, &dirty_active);
        Self::replay_stale_bus_members(live_subgraphs, &active, stale_members);

        let recurse_targets = live_subgraphs
            .iter()
            .filter(|handle| !Rc::ptr_eq(handle, start) && handle.borrow().dirty)
            .cloned()
            .collect::<Vec<_>>();

        visiting.remove(&start_id);
        for handle in recurse_targets {
            Self::propagate_neighbors_from_selected_start(
                &handle,
                live_subgraphs,
                global_subgraphs,
                force,
                visiting,
                stale_members,
                false,
            );
        }

        if start.borrow().dirty
            && !(!force && live_subgraph_has_both_hierarchy_ports_and_pins_from_handle(start))
        {
            Self::propagate_neighbors_from_selected_start(
                start,
                live_subgraphs,
                global_subgraphs,
                force,
                visiting,
                stale_members,
                true,
            );
        }
    }

    // Upstream parity: thin entrypoint for ordinary dirty-root traversal into the selected-start
    // propagation walk above.
    fn propagate_neighbors(
        start: &LiveReducedSubgraphHandle,
        live_subgraphs: &[LiveReducedSubgraphHandle],
        global_subgraphs: &[LiveReducedSubgraphHandle],
        force: bool,
        visiting: &mut BTreeSet<usize>,
        stale_members: &mut Vec<LiveProjectBusMemberHandle>,
    ) {
        Self::propagate_neighbors_from_selected_start(
            start,
            live_subgraphs,
            global_subgraphs,
            force,
            visiting,
            stale_members,
            false,
        );
    }

    // Upstream parity: local live-subgraph analogue for the repeated dirty-root walk KiCad drives
    // from live subgraphs during graph build. The active handle path now keeps that root loop on
    // the shared subgraph owner instead of a free outer coordinator around the graph.
    fn run_dirty_roots(live_subgraphs: &[LiveReducedSubgraphHandle], force: bool) {
        let global_subgraphs = Self::collect_global_subgraph_handles(live_subgraphs);

        for start in live_subgraphs {
            if !start.borrow().dirty {
                continue;
            }

            let mut stale_members = Vec::new();
            let mut visiting = BTreeSet::new();
            Self::propagate_neighbors(
                start,
                live_subgraphs,
                &global_subgraphs,
                force,
                &mut visiting,
                &mut stale_members,
            );
        }
    }

    // Upstream parity: local live-subgraph analogue for the bus-neighbor mutation branch inside
    // `propagateToNeighbors()`. The active recursive walk now keeps this driver/member promotion
    // step on the shared subgraph owner instead of a free helper around the handle graph, and the
    // reduced link walk now mirrors KiCad's secondary-driver retry when the original member
    // snapshot no longer matches the parent bus.
    fn refresh_bus_neighbor_drivers(
        live_subgraphs: &[LiveReducedSubgraphHandle],
        component: &[LiveReducedSubgraphHandle],
        stale_members: &mut Vec<LiveProjectBusMemberHandle>,
    ) -> Vec<(LiveReducedSubgraphHandle, LiveProjectBusMemberHandle)> {
        let mut recurse_targets = Vec::new();

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
                    .full_local_name
                    .cmp(&right.borrow().member.borrow().full_local_name)
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
                    parent_connection
                        .find_member_live(&current_link_member.borrow())
                        .or_else(|| {
                            Self::find_bus_neighbor_member_from_secondary_drivers(
                                live_subgraphs,
                                &sorted_links,
                                &current_link_member,
                                &parent_connection,
                            )
                        })
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

                if promoted_connection.connection_type != ReducedProjectConnectionType::Net {
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
                        let search = reduced_live_net_connection_from_label_name(
                            &neighbor_name,
                            &neighbor_sheet_instance_path,
                        );
                        parent_connection
                            .find_member_for_connection(&search)
                            .is_some()
                    };
                    if parent_has_search {
                        continue;
                    }
                }

                if live_reduced_subgraph_driver_priority(&neighbor_handle.borrow())
                    >= reduced_global_power_pin_driver_priority()
                {
                    let old_member = parent_member.clone();
                    let refreshed_member = {
                        clone_live_connection_owner_into_live_bus_member(
                            &mut parent_member.borrow_mut(),
                            &promoted_connection,
                        );
                        parent_member.clone()
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
                neighbor_handle.borrow_mut().dirty = true;
                recurse_targets.push((neighbor_handle, parent_member));
            }
        }

        recurse_targets.sort_by(|(left_handle, left_member), (right_handle, right_member)| {
            live_subgraph_handle_id(left_handle)
                .cmp(&live_subgraph_handle_id(right_handle))
                .then(
                    live_bus_member_handle_id(left_member)
                        .cmp(&live_bus_member_handle_id(right_member)),
                )
        });
        recurse_targets.dedup_by(|(left_handle, left_member), (right_handle, right_member)| {
            Rc::ptr_eq(left_handle, right_handle) && Rc::ptr_eq(left_member, right_member)
        });
        recurse_targets
    }

    // Upstream parity: local live-subgraph analogue for the "try harder" secondary-driver loop
    // inside `CONNECTION_GRAPH::propagateToNeighbors()`. KiCad re-runs `matchBusMember()` using
    // each secondary driver's default connection when the cached bus-member link is stale; the
    // reduced owner still matches by live connection/member carriers instead of full
    // `SCH_ITEM*`/`SCH_CONNECTION*` objects.
    fn find_bus_neighbor_member_from_secondary_drivers(
        live_subgraphs: &[LiveReducedSubgraphHandle],
        sorted_links: &[LiveReducedSubgraphLinkHandle],
        current_link_member: &LiveProjectBusMemberHandle,
        parent_connection: &LiveProjectConnection,
    ) -> Option<LiveProjectBusMemberHandle> {
        for candidate_link in sorted_links {
            let same_member = {
                let candidate_link = candidate_link.borrow();
                live_bus_member_handles_eq(&candidate_link.member, current_link_member)
            };
            if !same_member {
                continue;
            }

            let Some(candidate_handle) =
                live_subgraph_handle_for_link(live_subgraphs, candidate_link)
            else {
                continue;
            };
            let candidate_drivers = {
                let candidate = candidate_handle.borrow();
                if live_subgraph_strong_driver_count(&candidate) < 2 {
                    continue;
                }
                candidate.drivers.clone()
            };

            for driver in candidate_drivers {
                let driver = driver.borrow();
                if !matches!(
                    driver.kind(),
                    ReducedProjectDriverKind::Label | ReducedProjectDriverKind::PowerPin
                ) {
                    continue;
                }
                let driver_connection = driver.connection_handle();
                if let Some(member) =
                    parent_connection.find_member_for_connection(&driver_connection.borrow())
                {
                    return Some(member);
                }
            }
        }

        None
    }

    // Upstream parity: local live-subgraph analogue for refreshing parent-bus members from dirty
    // child net connections during the active recursive walk. Upstream mutates the matched bus
    // member in place inside `propagateToNeighbors()` and continues the current visited-chain
    // propagation without requeueing the parent bus as a fresh dirty root.
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

                {
                    let parent = parent_handle.borrow();
                    let mut parent_connection = parent.driver_connection.borrow_mut();
                    let link_member = link.borrow().member.clone();
                    let Some(member) =
                        parent_connection.find_member_mut_live(&link_member.borrow())
                    else {
                        continue;
                    };
                    clone_live_connection_owner_into_live_bus_member(
                        &mut member.borrow_mut(),
                        &child_connection.borrow(),
                    );
                }
            }
        }
    }

    // Upstream parity: local live-subgraph analogue for replaying stale bus members across the
    // active recursive live graph after neighbor and parent refresh mutate bus members in place.
    fn replay_stale_bus_members(
        live_subgraphs: &[LiveReducedSubgraphHandle],
        component: &[LiveReducedSubgraphHandle],
        stale_members: &mut Vec<LiveProjectBusMemberHandle>,
    ) {
        let cached_stale_members = stale_members.clone();

        for stale_member in &cached_stale_members {
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

                let matched = {
                    let subgraph = handle.borrow();
                    let mut connection = subgraph.driver_connection.borrow_mut();
                    let Some(member) = connection.find_member_mut_live(&stale_member.borrow())
                    else {
                        continue;
                    };
                    if !Rc::ptr_eq(&member, stale_member) {
                        clone_live_bus_member_into_live_bus_member(
                            &mut member.borrow_mut(),
                            &stale_member.borrow(),
                        );
                    }
                    true
                };

                if matched {
                    Self::refresh_bus_neighbor_drivers(
                        live_subgraphs,
                        std::slice::from_ref(handle),
                        stale_members,
                    );
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

                let existing_link = existing_parent_links.iter().find(|link| {
                    live_subgraph_handle_for_link(live_subgraphs, link)
                        .as_ref()
                        .is_some_and(|candidate| Rc::ptr_eq(candidate, &parent_handle))
                });

                let Some(refreshed_member) = refreshed_member else {
                    if let Some(existing_link) = existing_link {
                        refreshed_parent_links
                            .entry(child_id)
                            .or_default()
                            .push(existing_link.clone());
                    }
                    continue;
                };

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
            live.bus_parent_links = next_parent_links;
            live.bus_neighbor_links = next_neighbor_links;
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

                    if member.borrow().name == connection.borrow().name {
                        continue;
                    }

                    let old_name = if member.borrow().full_local_name.is_empty() {
                        member.borrow().name.clone()
                    } else {
                        member.borrow().full_local_name.clone()
                    };
                    clone_live_connection_owner_into_live_bus_member(
                        &mut member.borrow_mut(),
                        &connection.borrow(),
                    );
                    old_name
                };

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
                        let target_connection = candidate_handle.borrow().driver_connection.clone();
                        let changed = clone_live_connection_handle_from_handle_if_changed(
                            &target_connection,
                            &connection,
                        );
                        if changed {
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
        reference: base_pin.pin.reference.clone(),
        number: base_pin.pin.number.clone(),
        electrical_type: base_pin.pin.electrical_type.clone(),
        visible: base_pin.pin.visible,
        is_power_symbol: base_pin.pin.is_power_symbol,
        connection: base_pin.connection.borrow().snapshot(),
        driver_connection: base_pin.driver_connection.borrow().snapshot(),
        preserve_local_name_on_refresh: base_pin.preserved_local_name.is_some(),
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
    subgraph
        .drivers
        .iter()
        .filter(|driver| driver.borrow().priority() >= reduced_hierarchical_label_driver_priority())
        .count()
}

fn live_subgraph_has_local_driver(subgraph: &LiveReducedSubgraph) -> bool {
    live_reduced_subgraph_driver_priority(subgraph) < reduced_global_power_pin_driver_priority()
}

fn live_subgraph_base_pin_count(subgraph: &LiveReducedSubgraph) -> usize {
    subgraph.base_pins.len()
}

fn live_reduced_subgraph_driver_priority(subgraph: &LiveReducedSubgraph) -> i32 {
    subgraph
        .chosen_driver
        .as_ref()
        .map(|driver| driver.borrow().priority())
        .or_else(|| {
            subgraph
                .drivers
                .first()
                .map(|driver| driver.borrow().priority())
        })
        .or_else(|| {
            (!matches!(
                subgraph.driver_connection.borrow().connection_type,
                ReducedProjectConnectionType::None
            ))
            .then_some(reduced_pin_driver_priority())
        })
        .unwrap_or(0)
}

fn live_subgraph_is_self_driven_symbol_pin(subgraph: &LiveReducedSubgraph) -> bool {
    let has_weak_symbol_pin_driver = subgraph.drivers.len() == 1
        && matches!(
            &*subgraph.drivers[0].borrow(),
            LiveProjectStrongDriverOwner::SymbolPin { kind, priority, .. }
                if *kind == ReducedProjectDriverKind::Pin
                    && *priority == reduced_pin_driver_priority()
        );

    (live_subgraph_strong_driver_count(subgraph) == 0 || has_weak_symbol_pin_driver)
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
// construction, and exercised text-item driver owners now also seed from their reduced
// item-owned connections instead of starting from empty `NONE` sentinels before attachment.
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
                                reference: pin.reference.clone(),
                                number: pin.number.clone(),
                                electrical_type: pin.electrical_type.clone(),
                                visible: pin.visible,
                                is_power_symbol: pin.is_power_symbol,
                            },
                            connection: Rc::new(RefCell::new(pin.connection.clone().into())),
                            driver_connection: Rc::new(RefCell::new(
                                pin.driver_connection.clone().into(),
                            )),
                            driver: None,
                            preserved_local_name: pin
                                .preserve_local_name_on_refresh
                                .then(|| pin.driver_connection.local_name.clone()),
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
                            dangling: link.dangling,
                            connection: Rc::new(RefCell::new(link.connection.clone().into())),
                            driver_connection: Rc::new(RefCell::new(
                                link.connection.clone().into(),
                            )),
                            driver: None,
                            shown_text_local_name: link.connection.local_name.clone(),
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
                            driver_connection: Rc::new(RefCell::new(pin.connection.clone().into())),
                            driver: None,
                            shown_text_local_name: pin.connection.local_name.clone(),
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
                            driver_connection: Rc::new(RefCell::new(
                                port.connection.clone().into(),
                            )),
                            driver: None,
                            shown_text_local_name: port.connection.local_name.clone(),
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
                            start_is_wire_side: item.start_is_wire_side,
                            connection: Rc::new(RefCell::new(
                                subgraph.driver_connection.clone().into(),
                            )),
                            connected_bus_connection_handle: None,
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
                            start_is_wire_side: item.start_is_wire_side,
                            connection: Rc::new(RefCell::new(
                                subgraph.driver_connection.clone().into(),
                            )),
                            connected_bus_connection_handle: None,
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
            .attach_from_reduced(&reduced_subgraphs[index], &handles);
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

// Upstream parity: live-subgraph analogue for the `GetNameForDriver()` equality checks in
// `CONNECTION_GRAPH::propagateToNeighbors()` hierarchy visits. The current topology still starts
// from reduced precomputed parent/child handles, so the active walk revalidates the current
// sheet-pin and hierarchical-label shown names before following that edge.
fn live_subgraphs_have_matching_hierarchy_driver_names(
    parent: &LiveReducedSubgraphHandle,
    child: &LiveReducedSubgraphHandle,
) -> bool {
    let parent = parent.borrow();
    let child = child.borrow();

    parent.hier_sheet_pins.iter().any(|pin| {
        let pin = pin.borrow();
        let pin_connection = pin.connection.borrow();

        child.hier_ports.iter().any(|port| {
            let port = port.borrow();
            let port_connection = port.connection.borrow();

            pin_connection.connection_type == port_connection.connection_type
                && pin_connection.local_name == port_connection.local_name
        })
    })
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

fn live_subgraph_has_both_hierarchy_ports_and_pins_from_handle(
    handle: &LiveReducedSubgraphHandle,
) -> bool {
    let subgraph = handle.borrow();
    !subgraph.hier_ports.is_empty() && !subgraph.hier_sheet_pins.is_empty()
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
// keeps recache/update tied to the shared active graph object graph. Like KiCad's
// `recacheSubgraphName()`, this updates only the exact old-name/new-name buckets; prefix-only
// vector-bus aliases are initial lookup helpers, not maintained by this recache branch.
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

    let sheet_path = live_subgraphs[subgraph_index]
        .borrow()
        .sheet_instance_path
        .clone();
    let sheet_key = (sheet_path, old_name.to_string());
    if let Some(indexes) = subgraphs_by_sheet_and_name.get_mut(&sheet_key) {
        indexes.retain(|index| *index != subgraph_index);
    }

    let subgraph = live_subgraphs[subgraph_index].borrow();
    let name = subgraph.cache_name();
    subgraphs_by_name
        .entry(name.clone())
        .or_default()
        .push(subgraph_index);
    subgraphs_by_sheet_and_name
        .entry((subgraph.sheet_instance_path.clone(), name))
        .or_default()
        .push(subgraph_index);
}

// Upstream parity: local bridge for handle-cache recache on the shared live subgraph owner. This
// mirrors `CONNECTION_GRAPH::recacheSubgraphName()` exact-bucket behavior; it intentionally does
// not update prefix-only vector-bus aliases after a rename.
fn recache_live_reduced_subgraph_name_handle_cache_from_handles(
    subgraphs_by_name: &mut BTreeMap<String, Vec<LiveReducedSubgraphHandle>>,
    subgraphs_by_sheet_and_name: &mut BTreeMap<(String, String), Vec<LiveReducedSubgraphHandle>>,
    subgraph_handle: &LiveReducedSubgraphHandle,
    old_name: &str,
) {
    if let Some(handles) = subgraphs_by_name.get_mut(old_name) {
        handles.retain(|handle| !Rc::ptr_eq(handle, subgraph_handle));
    }

    let subgraph = subgraph_handle.borrow();
    let sheet_key = (subgraph.sheet_instance_path.clone(), old_name.to_string());
    if let Some(handles) = subgraphs_by_sheet_and_name.get_mut(&sheet_key) {
        handles.retain(|handle| !Rc::ptr_eq(handle, subgraph_handle));
    }

    let name = subgraph.cache_name();
    subgraphs_by_name
        .entry(name.clone())
        .or_default()
        .push(subgraph_handle.clone());
    subgraphs_by_sheet_and_name
        .entry((subgraph.sheet_instance_path.clone(), name))
        .or_default()
        .push(subgraph_handle.clone());
}

// Upstream parity: local bridge for the hierarchy-chain portion of live graph propagation on the
// shared live subgraph owner. This still mutates reduced live carriers instead of full local
// `CONNECTION_SUBGRAPH` objects, but the active recursive graph build now walks shared subgraph
// handles and uses handle identity plus narrow live handle reads instead of cloning whole live
// subgraph wrappers for traversal.
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
// runs on shared live subgraph handles. Hierarchy propagation consumes one explicit dirty-handle
// subset per recursive visit, while bus-link rematching stays in the final post-propagation pass.
// Remaining divergence is the still-missing fuller local `CONNECTION_SUBGRAPH` / item-pointer
// topology behind those handles.
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
    let component = all_indexes
        .iter()
        .filter_map(|index| live_subgraphs.get(*index).cloned())
        .collect::<Vec<_>>();
    LiveReducedSubgraph::refresh_bus_link_members(&live_subgraphs, &component);
    refresh_reduced_live_post_propagation_item_connections_on_handles(&live_subgraphs);
    apply_live_reduced_driver_connections_from_handles(reduced_subgraphs, &live_subgraphs);
    live_subgraphs
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
// `driver_connection` owner through the reduced subgraph owner itself during this cache rebuild
// instead of assigning both in parallel, and production cache/code assignment now also reads the
// reduced subgraph name from that same owner instead of treating `subgraph.name` as a second
// source of truth. Remaining divergence is the still-missing live cache mutation on real subgraph
// objects.
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
            let next_code = net_codes.len() + 1;
            let code = *net_codes.entry(owner_name.clone()).or_insert(next_code);
            subgraph.code = code;
            assign_reduced_connection_net_codes(&mut subgraph.driver_connection, &mut net_codes);
            subgraph.sync_boundary_state_from_driver_owner();

            for link in &mut subgraph.label_links {
                assign_reduced_connection_net_codes(&mut link.connection, &mut net_codes);
            }

            for pin in &mut subgraph.hier_sheet_pins {
                assign_reduced_connection_net_codes(&mut pin.connection, &mut net_codes);
            }

            for port in &mut subgraph.hier_ports {
                assign_reduced_connection_net_codes(&mut port.connection, &mut net_codes);
            }

            for base_pin in &mut subgraph.base_pins {
                assign_reduced_connection_net_codes(&mut base_pin.connection, &mut net_codes);
                assign_reduced_connection_net_codes(
                    &mut base_pin.driver_connection,
                    &mut net_codes,
                );
            }

            for driver in &mut subgraph.drivers {
                assign_reduced_connection_net_codes(&mut driver.connection, &mut net_codes);
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
                visible: pin.visible,
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
        })
        .collect()
}

// Upstream parity: reduced local connection-point owner used before subgraph grouping. This still
// stores reduced members instead of live `SCH_ITEM*`, but symbol-pin dedup now keys by both symbol
// UUID and pin number so stacked pins stay distinct the way separate `SCH_PIN` items do upstream.
// It also now merges near-equal projected/item points onto one connection-point owner instead of
// splitting them by raw float bits, matching the exercised KiCad geometry behavior more closely on
// the shared graph path.
fn push_connection_member(
    snapshot: &mut BTreeMap<PointKey, ConnectionPointSnapshot>,
    member: ConnectionMember,
) {
    let key = snapshot
        .keys()
        .copied()
        .find(|key| points_equal(member.at, [f64::from_bits(key.0), f64::from_bits(key.1)]))
        .unwrap_or_else(|| point_key(member.at));
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
                    },
                );
            }
            _ => {}
        }
    }

    snapshot
}

// Upstream parity: reduced local analogue for the connectable-item counting branch in
// `ERC_TESTER::TestFourWayJunction()`. This is not a 1:1 marker/source-item pass because the Rust
// graph still uses reduced connection-point snapshots instead of live item sets, but it keeps the
// four-way item-kind filter on the connectivity owner instead of making ERC inspect connection
// snapshot internals. Remaining divergence is fuller live item ownership and exact marker
// attachment.
pub(crate) fn collect_reduced_four_way_junction_points(schematic: &Schematic) -> Vec<[f64; 2]> {
    collect_connection_points(schematic)
        .into_values()
        .filter_map(|point| {
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

            (junction_items >= 4).then_some(point.at)
        })
        .collect()
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
    collect_connection_components_with_options(schematic, true)
}

fn collect_reduced_graph_connection_components(schematic: &Schematic) -> Vec<ConnectionComponent> {
    collect_connection_components_with_options(schematic, false)
}

fn collect_connection_components_with_options(
    schematic: &Schematic,
    include_bus_entry_segments: bool,
) -> Vec<ConnectionComponent> {
    let point_snapshot = collect_connection_points(schematic);
    let points = point_snapshot.values().cloned().collect::<Vec<_>>();
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
        SchItem::BusEntry(entry) => {
            if !include_bus_entry_segments {
                return None;
            }

            let endpoint_members = |at| {
                points
                    .iter()
                    .find(|point| points_equal(point.at, at))
                    .map(|point| point.members.as_slice())
                    .unwrap_or(&[])
            };
            let end = [entry.at[0] + entry.size[0], entry.at[1] + entry.size[1]];
            let wire_side = bus_entry_preferred_wire_endpoint(schematic, &point_snapshot, entry);
            let bus_side = if points_equal(wire_side, entry.at) {
                end
            } else {
                entry.at
            };
            let bus_side_has_bus = endpoint_members(bus_side)
                .iter()
                .any(|member| member.kind == ConnectionMemberKind::Bus);
            let wire_side_has_member_connection_owner =
                endpoint_members(wire_side).iter().any(|member| {
                    matches!(
                        member.kind,
                        ConnectionMemberKind::Wire
                            | ConnectionMemberKind::SymbolPin
                            | ConnectionMemberKind::SheetPin
                            | ConnectionMemberKind::Label
                            | ConnectionMemberKind::NoConnectMarker
                    )
                });
            (include_bus_entry_segments
                || !(bus_side_has_bus && wire_side_has_member_connection_owner))
                .then_some([entry.at, end])
        }
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

    for component in collect_reduced_graph_connection_components(schematic) {
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

            let shown_reference = symbol_reference(symbol);
            let reference = symbol.in_netlist.then(|| shown_reference.clone()).flatten();

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
                    reference: shown_reference.clone(),
                    number: pin.number.clone(),
                    electrical_type: pin.electrical_type.clone(),
                    visible: pin.visible,
                    is_power_symbol: symbol
                        .lib_symbol
                        .as_ref()
                        .is_some_and(|lib_symbol| lib_symbol.power),
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

// Upstream parity: reduced local analogue for the project-wide `CONNECTION_GRAPH` owner behind
// `Recalculate()`, `GetNetMap()`, and `GetSubgraphForItem()`. This is not a 1:1 graph owner
// because the Rust tree still lacks real `CONNECTION_SUBGRAPH` objects, live item pointers,
// full `updateItemConnectivity()` / `ResolveDrivers()` mutation flow, and the broader dirty/recache
// lifecycle KiCad runs through `Recalculate()`. It does now own one shared reduced project net map
// plus item lookup indexes instead of making ERC and export rebuild those facts independently.
// Remaining divergence is the missing full subgraph object model and graph-owned resolved-name
// caches beyond this reduced project graph; candidate ownership is now widened to
// `(sheet instance path, reference, pin)` so reused-sheet symbol-pin identity is not collapsed
// before pin net/class ownership is assigned, item-to-net facts now derive through the shared
// subgraph owner instead of duplicate item-to-whole-net side maps, outward reduced
// `resolved_connection` state is now also derived from the required reduced `driver_connection`
// owner through the reduced subgraph owner instead of being rebuilt from parallel raw fields
// during final graph assembly, whole-net views are derived from the shared subgraph owner instead
// of a second stored flattened carrier, reduced label/sheet-pin/no-connect membership now rides on
// the shared subgraph owner for graph-side ERC rules instead of per-sheet component rescans,
// reduced driver identity now rides on that same owner so `RunERC()`-style reused-screen
// de-duplication can happen above the shared graph boundary, and final reduced subgraph names now
// also derive from the required reduced `driver_connection` owner instead of treating `name` as an
// independent production owner. Pending reduced subgraph assembly now also keys its pending
// net/base-pin/node side maps through that pending driver owner instead of carrying a second
// pending `name` field beside it. The outward reduced node carrier is still narrower than a real
// `CONNECTION_SUBGRAPH` item owner.
pub(crate) fn collect_reduced_project_net_graph_from_inputs(
    inputs: ReducedProjectGraphInputs<'_>,
    for_board: bool,
) -> ReducedProjectNetGraph {
    struct PendingProjectSubgraph {
        driver_connection: ReducedProjectConnection,
        chosen_driver_index: Option<usize>,
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

                let connected_component = reduced_graph_connection_component_at(
                    schematic,
                    [f64::from_bits(points[0].0), f64::from_bits(points[0].1)],
                )
                .expect("project reduced subgraph must keep its source component");

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
                    strong_drivers.first(),
                    &bus_members,
                    &label_links,
                    &hier_sheet_pins,
                    &hier_ports,
                );
                let chosen_driver_index = (!strong_drivers.is_empty()).then_some(0);
                let pending_name = driver_connection.name.clone();

                pending_subgraphs.push(PendingProjectSubgraph {
                    driver_connection,
                    chosen_driver_index,
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

                nets.entry(pending_name.clone()).or_insert_with(|| {
                    (
                        class.clone(),
                        has_no_connect,
                        BTreeMap::new(),
                        all_base_pins_by_net
                            .get(&pending_name)
                            .cloned()
                            .unwrap_or_default(),
                    )
                });

                all_base_pins_by_net
                    .entry(pending_name.clone())
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
                        pending_name.clone(),
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

        for connected_component in collect_reduced_graph_connection_components(schematic) {
            let component_points = connected_component
                .members
                .iter()
                .map(|member| point_key(member.at))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let already_has_pending_subgraph = pending_subgraphs.iter().any(|pending| {
                pending.sheet_instance_path == sheet_path.instance_path
                    && pending.points == component_points
            });

            if already_has_pending_subgraph {
                continue;
            }

            let net_name = resolve_reduced_net_name_on_component(
                schematic,
                &connected_component,
                Some(&sheet_path_prefix),
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
            )
            .unwrap_or_default();

            if !net_name.is_empty() {
                continue;
            }

            let keeps_local_subgraph = connected_component.members.iter().any(|member| {
                matches!(
                    member.kind,
                    ConnectionMemberKind::Wire | ConnectionMemberKind::NoConnectMarker
                )
            }) || (connected_component
                .members
                .iter()
                .any(|member| member.kind == ConnectionMemberKind::BusEntry)
                && !connected_component
                    .members
                    .iter()
                    .any(|member| member.kind == ConnectionMemberKind::Bus));

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
                &[],
                &label_links,
                &hier_sheet_pins,
                &hier_ports,
            );

            pending_subgraphs.push(PendingProjectSubgraph {
                driver_connection,
                chosen_driver_index: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: true,
                sheet_instance_path: sheet_path.instance_path.clone(),
                anchor: point_key(connected_component.anchor),
                points: component_points,
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
    let mut sheet_pin_subgraph_identities = BTreeMap::new();
    let mut net_identities_by_name = BTreeMap::<String, ReducedProjectNetIdentity>::new();

    for (subgraph_index, pending) in pending_subgraphs.into_iter().enumerate() {
        let pending_name = pending.driver_connection.name.clone();
        if !pending_name.is_empty() && !net_identities_by_name.contains_key(&pending_name) {
            let (class, has_no_connect, _nodes, _base_pins) =
                nets.get(&pending_name).cloned().unwrap_or_default();
            let code = net_identities_by_name.len() + 1;
            net_identities_by_name.insert(
                pending_name.clone(),
                ReducedProjectNetIdentity {
                    code,
                    name: pending_name.clone(),
                    class,
                    has_no_connect,
                },
            );
        }
        let net_identity = net_identities_by_name.get(&pending_name);
        let resolved_name = net_identity
            .map(|net| net.name.clone())
            .unwrap_or_else(|| pending_name.clone());
        let mut subgraph = ReducedProjectSubgraphEntry {
            subgraph_code: subgraph_index + 1,
            code: net_identity.map(|net| net.code).unwrap_or_default(),
            name: String::new(),
            resolved_connection: ReducedProjectConnection {
                net_code: 0,
                connection_type: ReducedProjectConnectionType::None,
                name: String::new(),
                local_name: String::new(),
                full_local_name: String::new(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
            driver_connection: pending.driver_connection.clone(),
            chosen_driver_index: pending.chosen_driver_index,
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
        subgraph.driver_connection.name = resolved_name;
        subgraph.sync_boundary_state_from_driver_owner();
        let owner_name = subgraph.driver_connection.name.clone();

        let index = reduced_subgraphs.len();
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
        for base_pin in &subgraph.base_pins {
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
        for point in &subgraph.points {
            point_subgraph_identities.insert(
                ReducedProjectPointIdentityKey {
                    sheet_instance_path: subgraph.sheet_instance_path.clone(),
                    at: *point,
                },
                index,
            );
        }
        for label in &subgraph.label_links {
            label_subgraph_identities.insert(
                ReducedProjectLabelIdentityKey {
                    sheet_instance_path: subgraph.sheet_instance_path.clone(),
                    at: label.at,
                    kind: reduced_label_kind_sort_key(label.kind),
                },
                index,
            );
        }
        for point in &subgraph.no_connect_points {
            no_connect_subgraph_identities.insert(
                ReducedProjectNoConnectIdentityKey {
                    sheet_instance_path: subgraph.sheet_instance_path.clone(),
                    at: *point,
                },
                index,
            );
        }
        for pin in &subgraph.hier_sheet_pins {
            sheet_pin_subgraph_identities.insert(
                ReducedProjectSheetPinIdentityKey {
                    sheet_instance_path: subgraph.sheet_instance_path.clone(),
                    at: pin.at,
                    child_sheet_uuid: pin.child_sheet_uuid.clone(),
                },
                index,
            );
        }
        reduced_subgraphs.push(subgraph);
    }

    reduced_project_rename_weak_conflict_subgraphs(
        &mut reduced_subgraphs,
        &mut subgraphs_by_name,
        &mut subgraphs_by_sheet_and_name,
    );
    reduced_project_absorb_primary_same_name_subgraphs(&mut reduced_subgraphs);
    assign_reduced_connected_bus_subgraph_indexes(&mut reduced_subgraphs);
    subgraphs_by_sheet_and_name =
        reduced_project_rebuild_process_name_indexes(&reduced_subgraphs).1;

    pin_subgraph_identities.clear();
    pin_subgraph_identities_by_location.clear();
    point_subgraph_identities.clear();
    label_subgraph_identities.clear();
    no_connect_subgraph_identities.clear();
    sheet_pin_subgraph_identities.clear();

    for (index, subgraph) in reduced_subgraphs.iter().enumerate() {
        for base_pin in &subgraph.base_pins {
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
        for point in &subgraph.points {
            point_subgraph_identities.insert(
                ReducedProjectPointIdentityKey {
                    sheet_instance_path: subgraph.sheet_instance_path.clone(),
                    at: *point,
                },
                index,
            );
        }
        for label in &subgraph.label_links {
            label_subgraph_identities.insert(
                ReducedProjectLabelIdentityKey {
                    sheet_instance_path: subgraph.sheet_instance_path.clone(),
                    at: label.at,
                    kind: reduced_label_kind_sort_key(label.kind),
                },
                index,
            );
        }
        for point in &subgraph.no_connect_points {
            no_connect_subgraph_identities.insert(
                ReducedProjectNoConnectIdentityKey {
                    sheet_instance_path: subgraph.sheet_instance_path.clone(),
                    at: *point,
                },
                index,
            );
        }
        for pin in &subgraph.hier_sheet_pins {
            sheet_pin_subgraph_identities.insert(
                ReducedProjectSheetPinIdentityKey {
                    sheet_instance_path: subgraph.sheet_instance_path.clone(),
                    at: pin.at,
                    child_sheet_uuid: pin.child_sheet_uuid.clone(),
                },
                index,
            );
        }
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

        if !reduced_project_subgraph_has_process_strong_driver(
            &reduced_subgraphs,
            &subgraphs_by_sheet_and_name,
            parent_index,
        ) {
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
            if child.driver_connection.connection_type != ReducedProjectConnectionType::Net {
                continue;
            }

            if !reduced_project_subgraph_has_process_strong_driver(
                &reduced_subgraphs,
                &subgraphs_by_sheet_and_name,
                *child_index,
            ) {
                continue;
            }

            let mut child_names = child
                .label_links
                .iter()
                .map(|link| &link.connection)
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
            if child
                .drivers
                .iter()
                .filter(|driver| driver.priority >= reduced_hierarchical_label_driver_priority())
                .count()
                > 1
            {
                for driver in &child.drivers {
                    if !matches!(
                        driver.kind,
                        ReducedProjectDriverKind::Label | ReducedProjectDriverKind::PowerPin
                    ) {
                        continue;
                    }
                    if driver.connection.connection_type != driver_connection.connection_type {
                        continue;
                    }
                    if driver.connection.full_local_name == driver_connection.full_local_name {
                        continue;
                    }
                    if !driver.connection.full_local_name.is_empty() {
                        child_names.push(driver.connection.full_local_name.clone());
                    } else if !driver.connection.name.is_empty() {
                        child_names.push(driver.connection.name.clone());
                    }
                }
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
    let symbol_pins_by_symbol =
        build_reduced_project_symbol_pin_inventory(&inputs, &pin_subgraph_identities_by_location);

    ReducedProjectNetGraph {
        subgraphs: reduced_subgraphs,
        subgraphs_by_name,
        subgraphs_by_sheet_and_name,
        symbol_pins_by_symbol,
        pin_subgraph_identities,
        pin_subgraph_identities_by_location,
        point_subgraph_identities,
        label_subgraph_identities,
        no_connect_subgraph_identities,
        sheet_pin_subgraph_identities,
    }
}

// Upstream parity: reduced local analogue for the project-facing `CONNECTION_GRAPH` cache boundary
// that later callers reach through `GetNetMap()` and related queries. This wrapper exists because
// `SchematicProject` is currently the main cached graph owner, but the underlying reduced graph
// construction now accepts raw loaded inputs so loader-side hierarchy passes can reuse the same
// owner path instead of rebuilding connectivity via per-label current-sheet scans. Remaining
// divergence is the still-missing full subgraph object model and broader `Recalculate()` lifecycle
// behind both callers.
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

// Upstream parity: reduced local analogue for the project-wide `CONNECTION_GRAPH::GetNetMap()`
// consumer path used by KiCad's net exporters. This is not a 1:1 graph owner because the Rust
// tree still lacks real `CONNECTION_SUBGRAPH` objects, graph-owned item identity, and the exact
// exporter-facing container shape, but it now derives whole-net entries from the shared reduced
// subgraph owner instead of storing a second flattened net vector beside it. Remaining divergence
// is the missing full subgraph object model and graph-owned resolved-name caches beyond this
// reduced project net map. It now also preserves the shared graph's reduced net codes for
// non-export callers instead of renumbering them a second time at the flattened whole-net layer,
// and whole-net grouping now reads net names from the required reduced `driver_connection` owner
// instead of a parallel reduced subgraph `name` field. Whole-net base pins now also stay on shared
// reduced base-pin owners instead of collapsing to keys, so ERC/export callers can keep graph-owned
// per-pin context at the whole-net boundary, including node-less one-pin nets that still need ERC
// driver checks. Write-time exporters still do their own emitted-code assignment like KiCad
// `makeListOfNets()`.
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
            Vec<ReducedProjectBasePin>,
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
            Option<ReducedProjectBasePin>,
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
                    Vec::<ReducedProjectBasePin>::new(),
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
                .cloned()
                .or_else(|| subgraph.base_pins.first().cloned());
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
            if !entry
                .3
                .iter()
                .any(|candidate| candidate.key == base_pin.key)
            {
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
        if let Some(base_pin) = base_pin_key {
            if !entry
                .3
                .iter()
                .any(|candidate| candidate.key == base_pin.key)
            {
                entry.3.push(base_pin);
            }
        }
    }

    grouped
        .into_iter()
        .filter_map(
            |((code, name), (class, has_no_connect, nodes, base_pins))| {
                let nodes = nodes.into_values().collect::<Vec<_>>();
                ((!nodes.is_empty()) || !base_pins.is_empty()).then_some((
                    code,
                    name,
                    class,
                    has_no_connect,
                    nodes,
                    base_pins,
                ))
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

#[cfg_attr(not(test), allow(dead_code))]
// upstream: CONNECTION_GRAPH::GetNetMap or none
// parity_status: partial
// local_kind: local-only-transitional
// divergence: still returns reduced cloned subgraph snapshots by shared graph borrow instead of
// live `CONNECTION_SUBGRAPH*` owners
// local_only_reason: keeps production ERC/export callers on the shared reduced graph owner
// instead of cloning subgraph storage into local helper vectors
// replaced_by: fuller live `CONNECTION_SUBGRAPH` owner graph
// remove_when: production callers can iterate live `CONNECTION_SUBGRAPH` owners directly
pub(crate) fn reduced_project_subgraphs(
    graph: &ReducedProjectNetGraph,
) -> &[ReducedProjectSubgraphEntry] {
    &graph.subgraphs
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
        .or_else(|| {
            graph
                .point_subgraph_identities
                .iter()
                .find_map(|(key, index)| {
                    (key.sheet_instance_path == sheet_path.instance_path
                        && point_key_matches(key.at, at))
                    .then(|| graph.subgraphs.get(*index))
                    .flatten()
                })
        })
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
        .or_else(|| {
            graph
                .label_subgraph_identities
                .iter()
                .find_map(|(key, index)| {
                    (key.sheet_instance_path == sheet_path.instance_path
                        && key.kind == reduced_label_kind_sort_key(label.kind)
                        && point_key_matches(key.at, label.at))
                    .then(|| graph.subgraphs.get(*index))
                    .flatten()
                })
        })
}

// Upstream parity: reduced local analogue for the label half of
// `CONNECTION_GRAPH::GetNetFromItem()` on the project graph path. This still returns reduced net
// identity instead of a live `CONNECTION_SUBGRAPH`, but it now reports the label's net name from
// the required label identity owner instead of generic point identity at the same coordinates.
// Remaining divergence is fuller live item identity and the still-missing live
// `CONNECTION_SUBGRAPH` object.
pub(crate) fn resolve_reduced_project_net_for_label(
    graph: &ReducedProjectNetGraph,
    sheet_path: &LoadedSheetPath,
    label: &Label,
) -> Option<ReducedProjectNetIdentity> {
    resolve_reduced_project_subgraph_for_label(graph, sheet_path, label).map(|subgraph| {
        ReducedProjectNetIdentity {
            code: subgraph.code,
            name: subgraph.driver_connection.name.clone(),
            class: subgraph.class.clone(),
            has_no_connect: subgraph.has_no_connect,
        }
    })
}

// Upstream parity: reduced local analogue for the label `Name(true)` path via
// `CONNECTION_GRAPH::GetSubgraphForItem()`. This is not a 1:1 KiCad connection object because the
// Rust tree still lacks live `SCH_CONNECTION` instances, but it now reads the label's local driver
// name from the required label identity owner instead of generic point lookup.
// Remaining divergence is fuller live connection-object caching and item ownership.
pub(crate) fn resolve_reduced_project_driver_name_for_label(
    graph: &ReducedProjectNetGraph,
    sheet_path: &LoadedSheetPath,
    label: &Label,
) -> Option<String> {
    resolve_reduced_project_subgraph_for_label(graph, sheet_path, label)
        .map(|subgraph| subgraph.driver_connection.local_name.clone())
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
        .or_else(|| {
            graph
                .no_connect_subgraph_identities
                .iter()
                .find_map(|(key, index)| {
                    (key.sheet_instance_path == sheet_path.instance_path
                        && point_key_matches(key.at, at))
                    .then(|| graph.subgraphs.get(*index))
                    .flatten()
                })
        })
}

// Upstream parity: reduced local analogue for the no-connect pin connectivity branch in
// `ERC_TESTER::TestNoConnectPins()`. This is not a 1:1 connectable-item query because the Rust
// graph still projects symbol pins and wire/label owners into reduced subgraph snapshots, but it
// keeps the "no_connect pin has another owner at the same point" test on the shared graph owner
// instead of rebuilding it from ERC-local connection-point snapshots. Remaining divergence is
// fuller live `SCH_PIN` / item ownership and marker attachment.
pub(crate) fn reduced_project_no_connect_pin_has_connected_owner(
    graph: &ReducedProjectNetGraph,
    pin: &ReducedProjectSymbolPin,
) -> bool {
    if pin.electrical_type.as_deref() != Some("no_connect") {
        return false;
    }

    let Some(subgraph) = pin
        .subgraph_index
        .and_then(|index| graph.subgraphs.get(index))
    else {
        return false;
    };

    subgraph.base_pins.iter().any(|base_pin| {
        base_pin.key.at == pin.at && base_pin.electrical_type.as_deref() != Some("no_connect")
    }) || subgraph
        .wire_items
        .iter()
        .any(|item| item.start == pin.at || item.end == pin.at)
        || subgraph
            .bus_items
            .iter()
            .any(|item| item.start == pin.at || item.end == pin.at)
        || subgraph.label_links.iter().any(|label| label.at == pin.at)
        || subgraph
            .hier_sheet_pins
            .iter()
            .any(|sheet_pin| sheet_pin.at == pin.at)
        || subgraph.hier_ports.iter().any(|port| port.at == pin.at)
}

#[cfg_attr(not(test), allow(dead_code))]
// Upstream parity: reduced local analogue for the sheet-pin item half of
// `CONNECTION_GRAPH::GetSubgraphForItem()` on the project graph path. This is not a 1:1 KiCad
// item map because the Rust tree still keys sheet pins by `(sheet instance path, point,
// child-sheet uuid)` instead of live `SCH_SHEET_PIN*`, but it preserves shared sheet-pin-to-
// subgraph identity instead of routing sheet-pin ownership through the generic point lookup.
// Remaining divergence is fuller sheet-pin item identity and the still-missing live
// `CONNECTION_SUBGRAPH` object.
pub(crate) fn resolve_reduced_project_subgraph_for_sheet_pin<'a>(
    graph: &'a ReducedProjectNetGraph,
    sheet_path: &LoadedSheetPath,
    at: [f64; 2],
    child_sheet_uuid: Option<&str>,
) -> Option<&'a ReducedProjectSubgraphEntry> {
    graph
        .sheet_pin_subgraph_identities
        .get(&reduced_project_sheet_pin_identity_key(
            sheet_path,
            point_key(at),
            child_sheet_uuid,
        ))
        .and_then(|index| graph.subgraphs.get(*index))
        .or_else(|| {
            graph
                .sheet_pin_subgraph_identities
                .iter()
                .find_map(|(key, index)| {
                    (key.sheet_instance_path == sheet_path.instance_path
                        && key.child_sheet_uuid.as_deref() == child_sheet_uuid
                        && point_key_matches(key.at, at))
                    .then(|| graph.subgraphs.get(*index))
                    .flatten()
                })
        })
}

// Upstream parity: reduced local analogue for the sheet-pin half of
// `CONNECTION_GRAPH::GetNetFromItem()` on the project graph path. This still returns reduced net
// identity instead of a live `CONNECTION_SUBGRAPH`, but it now reports the sheet pin's net name
// from the required sheet-pin identity owner instead of falling back to generic point identity at
// the same coordinates. Remaining divergence is fuller live item identity and the still-missing
// live `CONNECTION_SUBGRAPH` object.
pub(crate) fn resolve_reduced_project_net_for_sheet_pin(
    graph: &ReducedProjectNetGraph,
    sheet_path: &LoadedSheetPath,
    at: [f64; 2],
    child_sheet_uuid: Option<&str>,
) -> Option<ReducedProjectNetIdentity> {
    resolve_reduced_project_subgraph_for_sheet_pin(graph, sheet_path, at, child_sheet_uuid).map(
        |subgraph| ReducedProjectNetIdentity {
            code: subgraph.code,
            name: subgraph.driver_connection.name.clone(),
            class: subgraph.class.clone(),
            has_no_connect: subgraph.has_no_connect,
        },
    )
}

// Upstream parity: reduced local analogue for the sheet-pin `Name(true)` path via
// `CONNECTION_GRAPH::GetSubgraphForItem()`. This is not a 1:1 KiCad connection object because the
// Rust tree still lacks live `SCH_CONNECTION` instances, but it now reads the sheet pin's local
// driver name from the required sheet-pin identity owner instead of generic point lookup.
// Remaining divergence is fuller live connection-object caching and item ownership.
pub(crate) fn resolve_reduced_project_driver_name_for_sheet_pin(
    graph: &ReducedProjectNetGraph,
    sheet_path: &LoadedSheetPath,
    at: [f64; 2],
    child_sheet_uuid: Option<&str>,
) -> Option<String> {
    resolve_reduced_project_subgraph_for_sheet_pin(graph, sheet_path, at, child_sheet_uuid)
        .map(|subgraph| subgraph.driver_connection.local_name.clone())
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

fn reduced_project_symbol_identity_key(
    sheet_path: &LoadedSheetPath,
    symbol: &Symbol,
) -> ReducedProjectSymbolIdentityKey {
    ReducedProjectSymbolIdentityKey {
        sheet_instance_path: sheet_path.instance_path.clone(),
        symbol_uuid: symbol.uuid.clone(),
    }
}

// upstream: CONNECTION_GRAPH::buildConnectionGraph / ERC_TESTER symbol-pin enumeration helpers or
// none
// parity_status: partial
// local_kind: local-only-transitional
// divergence: still builds reduced graph-owned symbol-pin inventories instead of exposing live
// `SCH_SYMBOL` / `SCH_PIN` owners with direct `SCH_CONNECTION` state
// local_only_reason: centralizes per-symbol pin inventory, placed-unit membership, and connected
// subgraph ownership on one graph-owned pass so ERC/net-name callers stop re-walking symbols
// ad hoc
// replaced_by: fuller live `SCH_SYMBOL` / `SCH_PIN` owner graph
// remove_when: production callers can iterate live symbol/pin owners directly
fn build_reduced_project_symbol_pin_inventory(
    inputs: &ReducedProjectGraphInputs<'_>,
    pin_subgraph_identities_by_location: &BTreeMap<ReducedProjectPinIdentityKey, usize>,
) -> BTreeMap<ReducedProjectSymbolIdentityKey, ReducedProjectSymbolPinInventory> {
    let mut symbol_pins_by_symbol =
        BTreeMap::<ReducedProjectSymbolIdentityKey, ReducedProjectSymbolPinInventory>::new();

    for sheet_path in inputs.sheet_paths {
        let Some(schematic) = inputs
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

            let reference = resolved_symbol_text_property_value(
                inputs.schematics,
                sheet_path,
                inputs.project,
                inputs.current_variant,
                symbol,
                "Reference",
            );
            let is_power_symbol = symbol
                .lib_symbol
                .as_ref()
                .is_some_and(|lib_symbol| lib_symbol.power);
            let unit_count = symbol
                .lib_symbol
                .as_ref()
                .map(|lib_symbol| {
                    lib_symbol
                        .units
                        .iter()
                        .map(|unit| unit.unit_number)
                        .collect::<BTreeSet<_>>()
                        .len()
                })
                .unwrap_or(0);
            let duplicate_pin_numbers_are_jumpers = symbol
                .lib_symbol
                .as_ref()
                .is_some_and(|lib_symbol| lib_symbol.duplicate_pin_numbers_are_jumpers);

            let projected_pins = projected_symbol_pin_info(symbol)
                .into_iter()
                .map(|pin| ReducedProjectSymbolPin {
                    schematic_path: schematic.path.clone(),
                    at: point_key(pin.at),
                    name: pin.name.clone(),
                    number: pin.number.clone(),
                    electrical_type: pin.electrical_type.clone(),
                    visible: pin.visible,
                    reference: reference.clone(),
                    is_power_symbol,
                    subgraph_index: pin_subgraph_identities_by_location
                        .get(&reduced_project_pin_identity_key(
                            sheet_path,
                            symbol,
                            pin.at,
                            pin.number.as_deref(),
                        ))
                        .copied(),
                })
                .collect::<Vec<_>>();

            symbol_pins_by_symbol
                .entry(reduced_project_symbol_identity_key(sheet_path, symbol))
                .and_modify(|inventory| {
                    inventory.unit = inventory.unit.or(symbol.unit);
                    inventory.unit_count = inventory.unit_count.max(unit_count);
                    inventory.duplicate_pin_numbers_are_jumpers |=
                        duplicate_pin_numbers_are_jumpers;
                    inventory.pins.extend(projected_pins.clone());
                })
                .or_insert(ReducedProjectSymbolPinInventory {
                    unit: symbol.unit,
                    unit_count,
                    duplicate_pin_numbers_are_jumpers,
                    pins: projected_pins,
                });
        }
    }

    symbol_pins_by_symbol
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

fn reduced_project_sheet_pin_identity_key(
    sheet_path: &LoadedSheetPath,
    at: PointKey,
    child_sheet_uuid: Option<&str>,
) -> ReducedProjectSheetPinIdentityKey {
    ReducedProjectSheetPinIdentityKey {
        sheet_instance_path: sheet_path.instance_path.clone(),
        at,
        child_sheet_uuid: child_sheet_uuid.map(str::to_string),
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
                        dangling: label_is_dangling_on_component(
                            schematic,
                            connected_component,
                            label.at,
                        ),
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
    let point_snapshot = collect_connection_points(schematic);
    let mut bus_items = schematic
        .screen
        .items
        .iter()
        .filter_map(|item| match item {
            SchItem::Bus(line) => {
                let start = line.points.first().copied()?;
                let end = line.points.last().copied()?;
                (point_key_set_contains(&component_points, start)
                    || point_key_set_contains(&component_points, end))
                .then_some(ReducedSubgraphWireItem {
                    start: point_key(start),
                    end: point_key(end),
                    is_bus_entry: false,
                    start_is_wire_side: false,
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
                (point_key_set_contains(&component_points, start)
                    || point_key_set_contains(&component_points, end))
                .then_some(ReducedSubgraphWireItem {
                    start: point_key(start),
                    end: point_key(end),
                    is_bus_entry: false,
                    start_is_wire_side: false,
                    connected_bus_subgraph_index: None,
                })
            }
            SchItem::BusEntry(entry) => {
                let wire_side =
                    bus_entry_preferred_wire_endpoint(schematic, &point_snapshot, entry);
                let end = [entry.at[0] + entry.size[0], entry.at[1] + entry.size[1]];
                let bus_side = if points_equal(wire_side, entry.at) {
                    end
                } else {
                    entry.at
                };
                point_key_set_contains(&component_points, wire_side).then_some(
                    ReducedSubgraphWireItem {
                        start: point_key(wire_side),
                        end: point_key(bus_side),
                        is_bus_entry: true,
                        start_is_wire_side: true,
                        connected_bus_subgraph_index: None,
                    },
                )
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
        .or_else(|| {
            pin_name.and_then(|pin_name| {
                graph
                    .pin_subgraph_identities
                    .iter()
                    .find_map(|(key, index)| {
                        (key.sheet_instance_path == sheet_path.instance_path
                            && key.symbol_uuid == symbol.uuid
                            && key.name.as_deref() == Some(pin_name)
                            && key.number.as_deref() == pin_number
                            && point_key_matches(key.at, at))
                        .then(|| graph.subgraphs.get(*index))
                        .flatten()
                    })
            })
        })
        .or_else(|| {
            graph
                .pin_subgraph_identities_by_location
                .iter()
                .find_map(|(key, index)| {
                    (key.sheet_instance_path == sheet_path.instance_path
                        && key.symbol_uuid == symbol.uuid
                        && key.number.as_deref() == pin_number
                        && point_key_matches(key.at, at))
                    .then(|| graph.subgraphs.get(*index))
                    .flatten()
                })
        })
}

#[cfg_attr(not(test), allow(dead_code))]
// upstream: CONNECTION_GRAPH::GetSubgraphForItem or none
// parity_status: partial
// local_kind: local-only-transitional
// divergence: still exposes reduced graph-owned symbol pin inventory instead of live `SCH_PIN*`
// plus `SCH_SYMBOL*` owners
// local_only_reason: keeps ERC/net-name callers on one graph-owned per-symbol pin inventory with
// exercised per-symbol metadata instead of re-walking symbol/lib state ad hoc
// replaced_by: fuller live `SCH_PIN` / `SCH_SYMBOL` owner graph
// remove_when: production callers can iterate live symbol/pin owners directly
pub(crate) fn reduced_project_symbol_pin_inventory<'a>(
    graph: &'a ReducedProjectNetGraph,
    sheet_path: &LoadedSheetPath,
    symbol: &Symbol,
) -> Option<&'a ReducedProjectSymbolPinInventory> {
    graph
        .symbol_pins_by_symbol
        .get(&reduced_project_symbol_identity_key(sheet_path, symbol))
}

#[cfg_attr(not(test), allow(dead_code))]
// upstream: CONNECTION_GRAPH::GetNetMap or none
// parity_status: partial
// local_kind: local-only-transitional
// divergence: still iterates reduced graph-owned symbol pin inventories instead of live
// `SCH_SYMBOL*` / `SCH_PIN*` owners
// local_only_reason: keeps ERC callers on shared graph-owned per-symbol pin inventories by sheet
// instead of re-walking schematic items to rediscover which symbol owners the graph already knows
// about
// replaced_by: fuller live `SCH_SYMBOL` / `SCH_PIN` owner graph
// remove_when: production callers can iterate live symbol/pin owners directly by sheet
pub(crate) fn collect_reduced_project_symbol_pin_inventories_in_sheet<'a>(
    graph: &'a ReducedProjectNetGraph,
    sheet_path: &LoadedSheetPath,
) -> Vec<&'a ReducedProjectSymbolPinInventory> {
    graph
        .symbol_pins_by_symbol
        .iter()
        .filter_map(|(key, inventory)| {
            (key.sheet_instance_path == sheet_path.instance_path).then_some(inventory)
        })
        .collect()
}

#[cfg_attr(not(test), allow(dead_code))]
// Upstream parity: reduced local analogue for iterating a symbol's `SCH_PIN` owners through the
// shared graph. This still projects reduced pin payload instead of exposing live `SCH_PIN*`
// objects, but it keeps ERC/net-name callers on one graph-owned per-symbol pin inventory,
// including unconnected pins, instead of re-projecting symbol pins ad hoc at each call site.
// Remaining divergence is the fuller live pin object layer behind this reduced inventory.
pub(crate) fn collect_reduced_project_symbol_pins<'a>(
    graph: &'a ReducedProjectNetGraph,
    sheet_path: &LoadedSheetPath,
    symbol: &Symbol,
) -> Vec<&'a ReducedProjectSymbolPin> {
    reduced_project_symbol_pin_inventory(graph, sheet_path, symbol)
        .into_iter()
        .flat_map(|inventory| inventory.pins.iter())
        .collect()
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

fn reduced_connected_bus_subgraph_for_wire_item_in<'a>(
    reduced_subgraphs: &'a [ReducedProjectSubgraphEntry],
    owner_subgraph: &ReducedProjectSubgraphEntry,
    wire_item: &ReducedSubgraphWireItem,
) -> Option<&'a ReducedProjectSubgraphEntry> {
    if !wire_item.is_bus_entry {
        return None;
    }

    if let Some(index) = wire_item.connected_bus_subgraph_index {
        if let Some(candidate) = reduced_subgraphs.get(index) {
            if candidate.sheet_instance_path == owner_subgraph.sheet_instance_path
                && !candidate.bus_items.is_empty()
            {
                return Some(candidate);
            }
        }
    }

    let bus_side = if wire_item.start_is_wire_side {
        wire_item.end
    } else {
        wire_item.start
    };

    reduced_subgraphs.iter().find(|candidate| {
        candidate.sheet_instance_path == owner_subgraph.sheet_instance_path
            && !candidate.bus_items.is_empty()
            && candidate.bus_items.iter().any(|bus_item| {
                point_on_wire_segment(
                    [f64::from_bits(bus_side.0), f64::from_bits(bus_side.1)],
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
}

// Upstream parity: reduced local analogue for the attached-bus lookup KiCad reaches through the
// connection graph on the bus-entry ERC/query path. This now prefers the graph-owned projected
// attached-bus subgraph index before falling back to the reduced geometric match, so ERC/export
// callers do not need to know how that owner is recovered. Remaining divergence is the still-
// missing fuller live bus-item / `SCH_CONNECTION` object graph behind this reduced owner query.
pub(crate) fn reduced_project_connected_bus_subgraph_for_wire_item<'a>(
    graph: &'a ReducedProjectNetGraph,
    subgraph: &ReducedProjectSubgraphEntry,
    wire_item: &ReducedSubgraphWireItem,
) -> Option<&'a ReducedProjectSubgraphEntry> {
    reduced_connected_bus_subgraph_for_wire_item_in(&graph.subgraphs, subgraph, wire_item)
}

// Upstream parity: reduced local analogue for the bus-entry connected-bus endpoint ownership KiCad
// gets from live `SCH_CONNECTION` / `CONNECTION_SUBGRAPH` item links. This is not a 1:1 item-owner
// query because the Rust graph still projects bus-entry attachments into reduced wire items, but it
// keeps the attached-bus endpoint coverage check inside the shared graph owner instead of making
// ERC scan bus segments independently. Remaining divergence is fuller live bus-entry item
// ownership.
pub(crate) fn reduced_project_wire_item_endpoint_has_connected_bus_owner(
    graph: &ReducedProjectNetGraph,
    subgraph: &ReducedProjectSubgraphEntry,
    wire_item: &ReducedSubgraphWireItem,
    endpoint: PointKey,
) -> bool {
    if !wire_item.is_bus_entry {
        return false;
    }

    let bus_side = if wire_item.start_is_wire_side {
        wire_item.end
    } else {
        wire_item.start
    };

    if endpoint != bus_side {
        return false;
    }

    let endpoint_at = [f64::from_bits(endpoint.0), f64::from_bits(endpoint.1)];

    reduced_project_connected_bus_subgraph_for_wire_item(graph, subgraph, wire_item).is_some_and(
        |bus_subgraph| {
            bus_subgraph.bus_items.iter().any(|item| {
                point_on_wire_segment(
                    endpoint_at,
                    [f64::from_bits(item.start.0), f64::from_bits(item.start.1)],
                    [f64::from_bits(item.end.0), f64::from_bits(item.end.1)],
                )
            })
        },
    )
}

fn assign_reduced_connected_bus_subgraph_indexes(
    reduced_subgraphs: &mut [ReducedProjectSubgraphEntry],
) {
    let bus_candidates = reduced_subgraphs
        .iter()
        .enumerate()
        .filter_map(|(index, subgraph)| {
            (!subgraph.bus_items.is_empty()).then_some((
                index,
                subgraph.sheet_instance_path.clone(),
                subgraph
                    .bus_items
                    .iter()
                    .map(|item| (item.start, item.end))
                    .collect::<Vec<_>>(),
            ))
        })
        .collect::<Vec<_>>();

    for subgraph in reduced_subgraphs.iter_mut() {
        let sheet_instance_path = subgraph.sheet_instance_path.clone();
        for wire_item in &mut subgraph.wire_items {
            if !wire_item.is_bus_entry {
                continue;
            }

            let bus_side = if wire_item.start_is_wire_side {
                wire_item.end
            } else {
                wire_item.start
            };
            wire_item.connected_bus_subgraph_index = bus_candidates.iter().find_map(
                |(candidate_index, candidate_sheet_path, bus_segments)| {
                    (*candidate_sheet_path == sheet_instance_path
                        && bus_segments.iter().any(|(start, end)| {
                            point_on_wire_segment(
                                [f64::from_bits(bus_side.0), f64::from_bits(bus_side.1)],
                                [f64::from_bits(start.0), f64::from_bits(start.1)],
                                [f64::from_bits(end.0), f64::from_bits(end.1)],
                            )
                        }))
                    .then_some(*candidate_index)
                },
            );
        }
    }
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

// Upstream parity: ERC_TESTER::TestLabelMultipleWires exercised wire-touch count branch.
// upstream: ERC_TESTER::TestLabelMultipleWires exercised wire-touch count branch
// parity_status: partial
// local_kind: local-only-transitional
// divergence: counts reduced wire segments by geometry instead of asking live SCH_TEXT/SCH_LINE
// overlap state
// local_only_reason: keeps label multiple-wire ERC input on the shared graph snapshot while live
// label item ownership is still missing
// replaced_by: fuller live CONNECTION_SUBGRAPH/SCH_TEXT/SCH_LINE item owner
// remove_when: label multiple-wire ERC consumes live graph item overlap state directly
fn label_non_endpoint_wire_segment_count(wire_segments: &[[[f64; 2]; 2]], at: [f64; 2]) -> usize {
    wire_segments
        .iter()
        .filter(|segment| {
            point_on_wire_segment(at, segment[0], segment[1])
                && !points_equal(at, segment[0])
                && !points_equal(at, segment[1])
        })
        .count()
}

// Upstream parity: reduced local analogue for the label-item `IsDangling()` facts consumed by
// `CONNECTION_GRAPH::ercCheckLabels()`, `ercCheckDirectiveLabels()`, and the exercised label
// multiple-wires branch. This is not a 1:1 KiCad subgraph snapshot because the Rust tree still
// lacks live `SCH_TEXT*` objects and graph-owned label item state. It exists for the remaining
// per-label dangling and wire-touch probes while the shared project subgraph owner carries the
// broader label/pin/no-connect grouping facts.
pub(crate) fn collect_reduced_label_component_snapshots(
    schematic: &Schematic,
) -> Vec<ReducedLabelComponentSnapshot> {
    let wire_segments = collect_wire_segments(schematic);

    collect_connection_components(schematic)
        .into_iter()
        .filter_map(|connected_component| {
            let labels =
                schematic
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
                                non_endpoint_wire_segment_count:
                                    label_non_endpoint_wire_segment_count(&wire_segments, label.at),
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

// Upstream parity: reduced local analogue for the directive-label dangling branch in
// `CONNECTION_GRAPH::ercCheckDirectiveLabels()`. This is not a 1:1 graph item query because the
// Rust tree still derives dangling state from reduced label-component snapshots, but it keeps the
// directive-specific label filter on the connectivity owner instead of having ERC inspect reduced
// snapshot internals. Remaining divergence is fuller live `SCH_TEXT` item ownership and marker
// attachment.
pub(crate) fn collect_reduced_dangling_directive_label_points(
    schematic: &Schematic,
) -> Vec<[f64; 2]> {
    collect_reduced_label_component_snapshots(schematic)
        .into_iter()
        .flat_map(|component| component.labels.into_iter())
        .filter_map(|label| {
            (label.kind == LabelKind::Directive && label.dangling).then_some(label.at)
        })
        .collect()
}

// Upstream parity: reduced local analogue for the wire-only traversal KiCad uses for several
// connection-point queries before it reaches fuller live `SCH_CONNECTION` ownership. This is not
// a 1:1 graph walk because the Rust tree still expands over reduced geometric wire segments
// instead of live connection objects, but it centralizes the exercised wire-only connected-set
// traversal so ERC and reduced connectivity callers stop open-coding their own segment scans.
// Remaining divergence is fuller live `SCH_CONNECTION` / `CONNECTION_SUBGRAPH` ownership.
pub(crate) fn connected_wire_segment_indices(
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

// Upstream parity: reduced local analogue for the connected-wire label-name query KiCad reaches
// through the connection graph on bus-entry ERC paths. This is not a 1:1 `SCH_CONNECTION` owner
// because the Rust tree still walks reduced wire segments and computes shown label text on demand,
// but it keeps the wire-only label-name selection on the shared connectivity owner instead of
// re-deriving it independently inside ERC. Remaining divergence is fuller live item ownership and
// cached connection-object names.
pub(crate) fn reduced_connected_wire_label_full_names_at(
    schematics: &[Schematic],
    sheet_paths: &[LoadedSheetPath],
    sheet_path: &LoadedSheetPath,
    project: Option<&LoadedProjectSettings>,
    current_variant: Option<&str>,
    at: [f64; 2],
) -> Vec<String> {
    let Some(schematic) = schematics
        .iter()
        .find(|schematic| schematic.path == sheet_path.schematic_path)
    else {
        return Vec::new();
    };

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
    let sheet_path_prefix = reduced_net_name_sheet_path_prefix(sheet_paths, sheet_path);

    let mut names = schematic
        .screen
        .items
        .iter()
        .filter_map(|item| match item {
            SchItem::Label(label) if label.kind != LabelKind::Directive => connected_segments
                .iter()
                .copied()
                .any(|segment_index| {
                    let segment = wire_segments[segment_index];
                    point_on_wire_segment(label.at, segment[0], segment[1])
                })
                .then(|| {
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
                        LabelKind::Directive => unreachable!(),
                    };

                    reduced_driver_full_name(
                        &shown,
                        source,
                        if label.kind == LabelKind::Global {
                            ""
                        } else {
                            &sheet_path_prefix
                        },
                    )
                }),
            _ => None,
        })
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
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

// upstream: CONNECTION_SUBGRAPH::GetDriverPriority label branches
// parity_status: same
// local_kind: upstream-native
// divergence: none for reduced label-kind priority values
// local_only_reason: none
// replaced_by: fuller live `SCH_LABEL_BASE` driver item owner
// remove_when: label priority reads directly from live KiCad-shaped item owners
fn reduced_label_driver_priority(label: &Label) -> i32 {
    match label.kind {
        LabelKind::Global => reduced_global_label_driver_priority(),
        LabelKind::Local => reduced_local_label_driver_priority(),
        LabelKind::Hierarchical => reduced_hierarchical_label_driver_priority(),
        LabelKind::Directive => reduced_none_driver_priority(),
    }
}

// upstream: CONNECTION_SUBGRAPH::GetDriverPriority default/none branch
// parity_status: same
// local_kind: upstream-native
// divergence: none for the reduced priority value
// local_only_reason: none
// replaced_by: fuller live `SCH_ITEM` driver item owner
// remove_when: driver priority reads directly from live KiCad-shaped item owners
fn reduced_none_driver_priority() -> i32 {
    0
}

// upstream: CONNECTION_SUBGRAPH::GetDriverPriority SCH_HIER_LABEL_T branch
// parity_status: same
// local_kind: upstream-native
// divergence: none for the reduced priority value
// local_only_reason: none
// replaced_by: fuller live `SCH_HIERLABEL` driver item owner
// remove_when: hierarchical-label priority reads directly from live KiCad-shaped item owners
fn reduced_hierarchical_label_driver_priority() -> i32 {
    3
}

// upstream: CONNECTION_SUBGRAPH::GetDriverPriority SCH_LABEL_T branch
// parity_status: same
// local_kind: upstream-native
// divergence: none for the reduced priority value
// local_only_reason: none
// replaced_by: fuller live `SCH_LABEL` driver item owner
// remove_when: local-label priority reads directly from live KiCad-shaped item owners
fn reduced_local_label_driver_priority() -> i32 {
    4
}

// upstream: CONNECTION_SUBGRAPH::GetDriverPriority SCH_GLOBAL_LABEL_T branch
// parity_status: same
// local_kind: upstream-native
// divergence: none for the reduced priority value
// local_only_reason: none
// replaced_by: fuller live `SCH_GLOBALLABEL` driver item owner
// remove_when: global-label priority reads directly from live KiCad-shaped item owners
fn reduced_global_label_driver_priority() -> i32 {
    7
}

// upstream: CONNECTION_SUBGRAPH::ResolveDrivers candidate_cmp sheet-pin shape branch
// parity_status: partial
// local_kind: upstream-native
// divergence: reduced `SheetPinShape` carrier instead of live `SCH_SHEET_PIN`
// local_only_reason: none
// replaced_by: fuller live `SCH_SHEET_PIN` driver item owner
// remove_when: sheet-pin candidate comparison runs on live KiCad-shaped item owners
fn reduced_sheet_pin_driver_rank(shape: SheetPinShape) -> i32 {
    match shape {
        SheetPinShape::Output => 1,
        SheetPinShape::Input
        | SheetPinShape::Bidirectional
        | SheetPinShape::TriState
        | SheetPinShape::Unspecified => 0,
    }
}

// upstream: CONNECTION_SUBGRAPH::GetDriverPriority SCH_SHEET_PIN_T branch
// parity_status: same
// local_kind: upstream-native
// divergence: none for the reduced priority value
// local_only_reason: none
// replaced_by: fuller live `SCH_SHEET_PIN` driver item owner
// remove_when: sheet-pin priority reads directly from live KiCad-shaped item owners
fn reduced_sheet_pin_driver_priority() -> i32 {
    2
}

// upstream: CONNECTION_SUBGRAPH::GetDriverPriority SCH_PIN_T non-power branch
// parity_status: same
// local_kind: upstream-native
// divergence: none for the reduced priority value
// local_only_reason: none
// replaced_by: fuller live `SCH_PIN` driver item owner
// remove_when: pin priority reads directly from live KiCad-shaped item owners
fn reduced_pin_driver_priority() -> i32 {
    1
}

// upstream: CONNECTION_SUBGRAPH::GetDriverPriority global power-pin branch
// parity_status: same
// local_kind: upstream-native
// divergence: none for the reduced priority value
// local_only_reason: none
// replaced_by: fuller live `SCH_PIN` driver item owner
// remove_when: global-power priority reads directly from live KiCad-shaped item owners
fn reduced_global_power_pin_driver_priority() -> i32 {
    6
}

// upstream: CONNECTION_SUBGRAPH::GetDriverPriority local power-pin branch
// parity_status: same
// local_kind: upstream-native
// divergence: none for the reduced priority value
// local_only_reason: none
// replaced_by: fuller live `SCH_PIN` driver item owner
// remove_when: local-power priority reads directly from live KiCad-shaped item owners
fn reduced_local_power_pin_driver_priority() -> i32 {
    5
}

fn reduced_power_pin_driver_priority(
    symbol: &Symbol,
    electrical_type: Option<&str>,
) -> Option<i32> {
    let lib_symbol = symbol.lib_symbol.as_ref()?;

    if electrical_type != Some("power_in") || !lib_symbol.power {
        return None;
    }

    Some(if lib_symbol.local_power {
        reduced_local_power_pin_driver_priority()
    } else {
        reduced_global_power_pin_driver_priority()
    })
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

// upstream: CONNECTION_SUBGRAPH::GetDriverPriority SCH_PIN_T non-power exclusion branch
// parity_status: partial
// local_kind: upstream-native
// divergence: checks the reduced local-lib symbol reference field instead of live SCH_SYMBOL
// local_only_reason: current connectivity still projects pins from reduced symbol/lib-symbol data
// replaced_by: fuller live SCH_PIN/SCH_SYMBOL driver item owner
// remove_when: driver priority reads directly from live KiCad-shaped item owners
fn reduced_symbol_lib_reference_starts_with_hash(symbol: &Symbol) -> bool {
    symbol
        .lib_symbol
        .as_ref()
        .and_then(|lib_symbol| {
            lib_symbol
                .properties
                .iter()
                .find(|property| property.kind == crate::model::PropertyKind::SymbolReference)
        })
        .is_some_and(|property| property.value.starts_with('#'))
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

    if reduced_power_pin_driver_priority(symbol, pin.electrical_type.as_deref()).is_some() {
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
    chosen_driver: Option<&ReducedProjectStrongDriver>,
    bus_members: &[ReducedBusMember],
    label_links: &[ReducedLabelLink],
    hier_sheet_pins: &[ReducedHierSheetPinLink],
    hier_ports: &[ReducedHierPortLink],
) -> ReducedProjectConnection {
    if let Some(driver) = chosen_driver {
        let local_name = driver.connection.local_name.clone();
        let full_local_name = driver.connection.full_local_name.clone();
        return build_reduced_project_connection(
            schematic,
            sheet_instance_path.to_string(),
            resolved_name.to_string(),
            local_name.clone(),
            full_local_name,
            if reduced_text_is_bus(schematic, &local_name) {
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

// Upstream parity: reduced local analogue for the driver collection inside
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
// symbols keep per-pin driver text through the shared graph owner. The reduced collection now
// also mirrors KiCad's strong-driver cleanup by dropping weak sheet-pin/default-pin drivers once a
// hierarchical-label-or-stronger driver exists. Ordinary symbol pins now participate as weak
// `SCH_PIN_T` drivers before that cleanup instead of existing only as a priority fallback on the
// reduced subgraph, and ordinary pins whose library-symbol reference starts with `#` now rank as
// `NONE` like upstream. Remaining divergence is the still-missing live connection object plus
// fuller power/bus-parent driver ownership.
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
    let mut drivers = Vec::<(ReducedProjectStrongDriver, i32)>::new();

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

                drivers.push((
                    ReducedProjectStrongDriver {
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
                    },
                    0,
                ));
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

                    drivers.push((
                        ReducedProjectStrongDriver {
                            kind: ReducedProjectDriverKind::SheetPin,
                            priority: reduced_sheet_pin_driver_priority(),
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
                                child_sheet_uuid: sheet.uuid.clone(),
                            }),
                        },
                        reduced_sheet_pin_driver_rank(pin.shape),
                    ));
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

                            drivers.push((
                                ReducedProjectStrongDriver {
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
                                        sheet_instance_path: sheet_instance_path.to_string(),
                                        symbol_uuid: symbol.uuid.clone(),
                                        at: point_key(pin.at),
                                        pin_number: pin.number.clone(),
                                    }),
                                },
                                0,
                            ));
                        }
                    } else if symbol.in_netlist
                        && symbol.on_board
                        && !reduced_symbol_lib_reference_starts_with_hash(symbol)
                    {
                        if let Some(text) =
                            reduced_symbol_pin_default_net_name(symbol, pin, &unit_pins, false)
                        {
                            drivers.push((
                                ReducedProjectStrongDriver {
                                    kind: ReducedProjectDriverKind::Pin,
                                    priority: reduced_pin_driver_priority(),
                                    connection: build_reduced_project_driver_connection(
                                        schematic,
                                        sheet_instance_path,
                                        text.clone(),
                                        text,
                                        "",
                                    ),
                                    identity: Some(ReducedProjectDriverIdentity::SymbolPin {
                                        schematic_path: schematic_path.to_path_buf(),
                                        sheet_instance_path: sheet_instance_path.to_string(),
                                        symbol_uuid: symbol.uuid.clone(),
                                        at: point_key(pin.at),
                                        pin_number: pin.number.clone(),
                                    }),
                                },
                                0,
                            ));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    drivers.retain(|(driver, _sheet_pin_rank)| {
        !reduced_project_strong_driver_name(driver).is_empty()
            && !reduced_project_strong_driver_name(driver).contains("${")
            && !reduced_project_strong_driver_name(driver).starts_with('<')
    });

    if drivers.iter().any(|(driver, _sheet_pin_rank)| {
        driver.priority >= reduced_hierarchical_label_driver_priority()
    }) {
        drivers.retain(|(driver, _sheet_pin_rank)| {
            driver.priority >= reduced_hierarchical_label_driver_priority()
        });
    }

    drivers.sort_by(|(lhs, lhs_sheet_pin_rank), (rhs, rhs_sheet_pin_rank)| {
        let lhs_name = reduced_project_strong_driver_name(lhs);
        let rhs_name = reduced_project_strong_driver_name(rhs);
        let lhs_low_quality_name = lhs_name.contains("-Pad");
        let rhs_low_quality_name = rhs_name.contains("-Pad");

        rhs.priority
            .cmp(&lhs.priority)
            .then_with(|| reduced_bus_subset_cmp(schematic, lhs_name, rhs_name))
            .then_with(|| rhs_sheet_pin_rank.cmp(lhs_sheet_pin_rank))
            .then_with(|| lhs_low_quality_name.cmp(&rhs_low_quality_name))
            .then_with(|| lhs_name.cmp(rhs_name))
    });
    drivers
        .into_iter()
        .map(|(driver, _sheet_pin_rank)| driver)
        .collect()
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
// - ordinary pins outside the netlist/board, or whose library-symbol reference starts with `#`,
//   are skipped like upstream
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
                        priority: reduced_sheet_pin_driver_priority(),
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
                                if !symbol.in_netlist
                                    || !symbol.on_board
                                    || reduced_symbol_lib_reference_starts_with_hash(symbol)
                                {
                                    return None;
                                }

                                reduced_symbol_pin_default_net_name(symbol, pin, &unit_pins, false)
                                    .map(|text| ReducedDriverNameCandidate {
                                        priority: reduced_pin_driver_priority(),
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
        LiveProjectBusMember, LiveProjectConnection, LiveReducedSubgraph,
        LiveReducedSubgraphHandle, PointKey, ReducedBusMember, ReducedBusMemberKind,
        ReducedHierPortLink, ReducedHierSheetPinLink, ReducedLabelLink,
        ReducedProjectBusNeighborLink, ReducedProjectConnection, ReducedProjectConnectionType,
        ReducedProjectDriverKind, ReducedProjectStrongDriver, ReducedProjectSubgraphEntry,
        ReducedSubgraphWireItem, apply_live_reduced_driver_connections_from_handles,
        build_live_reduced_name_caches_from_handles, build_live_reduced_subgraph_handles,
        clone_reduced_connection_into_live_connection_owner,
        find_first_reduced_project_subgraph_by_name, find_reduced_project_subgraph_by_name,
        recache_live_reduced_subgraph_name_from_handles,
        recache_live_reduced_subgraph_name_handle_cache_from_handles, reduced_bus_member_objects,
        refresh_reduced_live_graph_propagation, resolve_reduced_net_name_at,
        resolve_reduced_project_driver_name_for_label, resolve_reduced_project_net_at,
        resolve_reduced_project_net_for_label, resolve_reduced_project_subgraph_at,
        resolve_reduced_project_subgraph_for_label,
        resolve_reduced_project_subgraph_for_no_connect,
        resolve_reduced_project_subgraph_for_sheet_pin,
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

    fn test_net_connection(
        name: &str,
        local_name: &str,
        full_local_name: &str,
        sheet_instance_path: &str,
    ) -> ReducedProjectConnection {
        ReducedProjectConnection {
            net_code: 0,
            connection_type: ReducedProjectConnectionType::Net,
            name: name.to_string(),
            local_name: local_name.to_string(),
            full_local_name: full_local_name.to_string(),
            sheet_instance_path: sheet_instance_path.to_string(),
            members: Vec::new(),
        }
    }

    fn test_bus_member(name: &str, local_name: &str, full_local_name: &str) -> ReducedBusMember {
        ReducedBusMember {
            net_code: 0,
            name: name.to_string(),
            local_name: local_name.to_string(),
            full_local_name: full_local_name.to_string(),
            vector_index: None,
            kind: ReducedBusMemberKind::Net,
            members: Vec::new(),
        }
    }

    fn test_bus_connection(
        name: &str,
        local_name: &str,
        full_local_name: &str,
        sheet_instance_path: &str,
        members: Vec<ReducedBusMember>,
    ) -> ReducedProjectConnection {
        ReducedProjectConnection {
            net_code: 0,
            connection_type: ReducedProjectConnectionType::Bus,
            name: name.to_string(),
            local_name: local_name.to_string(),
            full_local_name: full_local_name.to_string(),
            sheet_instance_path: sheet_instance_path.to_string(),
            members,
        }
    }

    fn test_net_subgraph(
        subgraph_code: usize,
        driver_connection: ReducedProjectConnection,
        drivers: Vec<ReducedProjectStrongDriver>,
        sheet_instance_path: &str,
    ) -> ReducedProjectSubgraphEntry {
        ReducedProjectSubgraphEntry {
            subgraph_code,
            code: subgraph_code,
            name: driver_connection.name.clone(),
            resolved_connection: driver_connection.clone(),
            driver_connection,
            chosen_driver_index: None,
            drivers,
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: sheet_instance_path.to_string(),
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
        }
    }

    fn test_power_driver(connection: ReducedProjectConnection) -> ReducedProjectStrongDriver {
        ReducedProjectStrongDriver {
            kind: ReducedProjectDriverKind::PowerPin,
            priority: 6,
            connection,
            identity: None,
        }
    }

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
        let target = Rc::new(RefCell::new(super::LiveProjectConnection::from(
            ReducedProjectConnection {
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
        )));
        let source = Rc::new(RefCell::new(super::LiveProjectConnection::from(
            ReducedProjectConnection {
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
            },
        )));

        super::clone_live_connection_owner_into_live_connection_owner(
            &mut target.borrow_mut(),
            &source.borrow(),
        );

        let target_connection = target.borrow().snapshot();
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
                chosen_driver_index: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0.0f64.to_bits(), 0.0f64.to_bits()),
                points: vec![PointKey(0.0f64.to_bits(), 0.0f64.to_bits())],
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
                    start_is_wire_side: false,
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
                chosen_driver_index: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(5, 5),
                points: vec![PointKey(5, 5)],
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
                    start_is_wire_side: true,
                    connected_bus_subgraph_index: None,
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
            .connected_bus_connection_handle = Some(handles[0].borrow().driver_connection.clone());

        apply_live_reduced_driver_connections_from_handles(&mut reduced, &handles);

        assert_eq!(
            reduced[1].wire_items[0].connected_bus_subgraph_index,
            Some(0)
        );

        let connected_bus = super::reduced_connected_bus_subgraph_for_wire_item_in(
            &reduced,
            &reduced[1],
            &reduced[1].wire_items[0],
        )
        .expect("attached bus subgraph");
        assert_eq!(connected_bus.subgraph_code, reduced[0].subgraph_code);
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
                chosen_driver_index: None,
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
                    start_is_wire_side: false,
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
                chosen_driver_index: None,
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
    fn reduced_connected_bus_lookup_skips_bus_touching_wire_side_of_entry() {
        let reduced = vec![
            ReducedProjectSubgraphEntry {
                subgraph_code: 1,
                code: 1,
                name: "/WRONG".to_string(),
                resolved_connection: test_bus_connection(
                    "/WRONG",
                    "WRONG",
                    "/WRONG",
                    "",
                    Vec::new(),
                ),
                driver_connection: test_bus_connection("/WRONG", "WRONG", "/WRONG", "", Vec::new()),
                chosen_driver_index: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0.0f64.to_bits(), 0.0f64.to_bits()),
                points: vec![PointKey(0.0f64.to_bits(), 0.0f64.to_bits())],
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: vec![ReducedSubgraphWireItem {
                    start: PointKey((-5.0f64).to_bits(), 0.0f64.to_bits()),
                    end: PointKey(0.0f64.to_bits(), 0.0f64.to_bits()),
                    is_bus_entry: false,
                    start_is_wire_side: false,
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
                name: "/RIGHT".to_string(),
                resolved_connection: test_bus_connection(
                    "/RIGHT",
                    "RIGHT",
                    "/RIGHT",
                    "",
                    Vec::new(),
                ),
                driver_connection: test_bus_connection("/RIGHT", "RIGHT", "/RIGHT", "", Vec::new()),
                chosen_driver_index: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(5.0f64.to_bits(), 5.0f64.to_bits()),
                points: vec![PointKey(5.0f64.to_bits(), 5.0f64.to_bits())],
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: vec![ReducedSubgraphWireItem {
                    start: PointKey(5.0f64.to_bits(), 5.0f64.to_bits()),
                    end: PointKey(10.0f64.to_bits(), 5.0f64.to_bits()),
                    is_bus_entry: false,
                    start_is_wire_side: false,
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
                subgraph_code: 3,
                code: 3,
                name: "/ENTRY".to_string(),
                resolved_connection: test_net_connection("/ENTRY", "ENTRY", "/ENTRY", ""),
                driver_connection: test_net_connection("/ENTRY", "ENTRY", "/ENTRY", ""),
                chosen_driver_index: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0.0f64.to_bits(), 0.0f64.to_bits()),
                points: vec![PointKey(0.0f64.to_bits(), 0.0f64.to_bits())],
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: vec![ReducedSubgraphWireItem {
                    start: PointKey(0.0f64.to_bits(), 0.0f64.to_bits()),
                    end: PointKey(5.0f64.to_bits(), 5.0f64.to_bits()),
                    is_bus_entry: true,
                    start_is_wire_side: true,
                    connected_bus_subgraph_index: None,
                }],
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
        ];

        let connected_bus = super::reduced_connected_bus_subgraph_for_wire_item_in(
            &reduced,
            &reduced[2],
            &reduced[2].wire_items[0],
        )
        .expect("attached bus subgraph");
        assert_eq!(connected_bus.subgraph_code, reduced[1].subgraph_code);
    }

    #[test]
    fn reduced_connected_bus_lookup_prefers_projected_owner_index_over_geometry() {
        let reduced = vec![
            ReducedProjectSubgraphEntry {
                subgraph_code: 1,
                code: 1,
                name: "/INDEXED".to_string(),
                resolved_connection: test_bus_connection(
                    "/INDEXED",
                    "INDEXED",
                    "/INDEXED",
                    "",
                    Vec::new(),
                ),
                driver_connection: test_bus_connection(
                    "/INDEXED",
                    "INDEXED",
                    "/INDEXED",
                    "",
                    Vec::new(),
                ),
                chosen_driver_index: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey((-10.0f64).to_bits(), 5.0f64.to_bits()),
                points: vec![PointKey((-10.0f64).to_bits(), 5.0f64.to_bits())],
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: vec![ReducedSubgraphWireItem {
                    start: PointKey((-10.0f64).to_bits(), 5.0f64.to_bits()),
                    end: PointKey((-5.0f64).to_bits(), 5.0f64.to_bits()),
                    is_bus_entry: false,
                    start_is_wire_side: false,
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
                name: "/GEOMETRIC".to_string(),
                resolved_connection: test_bus_connection(
                    "/GEOMETRIC",
                    "GEOMETRIC",
                    "/GEOMETRIC",
                    "",
                    Vec::new(),
                ),
                driver_connection: test_bus_connection(
                    "/GEOMETRIC",
                    "GEOMETRIC",
                    "/GEOMETRIC",
                    "",
                    Vec::new(),
                ),
                chosen_driver_index: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(5.0f64.to_bits(), 5.0f64.to_bits()),
                points: vec![PointKey(5.0f64.to_bits(), 5.0f64.to_bits())],
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: vec![ReducedSubgraphWireItem {
                    start: PointKey(5.0f64.to_bits(), 5.0f64.to_bits()),
                    end: PointKey(10.0f64.to_bits(), 5.0f64.to_bits()),
                    is_bus_entry: false,
                    start_is_wire_side: false,
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
                subgraph_code: 3,
                code: 3,
                name: "/ENTRY".to_string(),
                resolved_connection: test_net_connection("/ENTRY", "ENTRY", "/ENTRY", ""),
                driver_connection: test_net_connection("/ENTRY", "ENTRY", "/ENTRY", ""),
                chosen_driver_index: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0.0f64.to_bits(), 0.0f64.to_bits()),
                points: vec![PointKey(0.0f64.to_bits(), 0.0f64.to_bits())],
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: vec![ReducedSubgraphWireItem {
                    start: PointKey(0.0f64.to_bits(), 0.0f64.to_bits()),
                    end: PointKey(5.0f64.to_bits(), 5.0f64.to_bits()),
                    is_bus_entry: true,
                    start_is_wire_side: true,
                    connected_bus_subgraph_index: Some(0),
                }],
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
        ];

        let connected_bus = super::reduced_connected_bus_subgraph_for_wire_item_in(
            &reduced,
            &reduced[2],
            &reduced[2].wire_items[0],
        )
        .expect("attached bus subgraph");
        assert_eq!(connected_bus.subgraph_code, reduced[0].subgraph_code);
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
                chosen_driver_index: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: vec![ReducedHierSheetPinLink {
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                    at: PointKey(0, 0),
                    child_sheet_uuid: Some("child-sheet".to_string()),
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
                hier_ports: Vec::new(),
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
                chosen_driver_index: None,
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
            chosen_driver_index: None,
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
                dangling: false,
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
    fn live_bus_entry_connected_bus_owner_refreshes_bus_item_clone_after_driver_update() {
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
                chosen_driver_index: None,
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
                    start_is_wire_side: false,
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
                chosen_driver_index: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(5, 0),
                points: vec![PointKey(5, 0)],
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
                    start_is_wire_side: true,
                    connected_bus_subgraph_index: None,
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
        let component = vec![live[0].clone()];
        LiveReducedSubgraph::refresh_bus_neighbor_drivers(&live, &component, &mut Vec::new());

        let live_bus_entry = live[1].borrow();
        let attached_bus_connection = live_bus_entry.wire_items[0]
            .borrow()
            .connected_bus_connection_handle
            .clone()
            .expect("connected bus connection owner");
        assert!(Rc::ptr_eq(
            &attached_bus_connection,
            &live[0].borrow().driver_connection
        ));
        let connected_bus_subgraph = live
            .iter()
            .find(|candidate| {
                Rc::ptr_eq(
                    &candidate.borrow().driver_connection,
                    &attached_bus_connection,
                )
            })
            .expect("connected bus subgraph owner");
        let connected_bus_connection = connected_bus_subgraph.borrow().bus_items[0]
            .borrow()
            .connection
            .clone();
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
            attached_bus_connection.borrow().members[0]
                .borrow()
                .full_local_name,
            "/PWR"
        );
        assert_eq!(
            connected_bus_connection.borrow().members[0]
                .borrow()
                .full_local_name,
            "/OLD1"
        );

        super::sync_live_reduced_item_connections_from_driver_handle(&live[0]);

        assert_eq!(
            connected_bus_connection.borrow().members[0]
                .borrow()
                .full_local_name,
            "/PWR"
        );
    }

    #[test]
    fn live_bus_entry_attachment_skips_bus_touching_wire_side_of_entry() {
        let reduced = vec![
            ReducedProjectSubgraphEntry {
                subgraph_code: 1,
                code: 1,
                name: "/WRONG".to_string(),
                resolved_connection: test_bus_connection(
                    "/WRONG",
                    "WRONG",
                    "/WRONG",
                    "",
                    Vec::new(),
                ),
                driver_connection: test_bus_connection("/WRONG", "WRONG", "/WRONG", "", Vec::new()),
                chosen_driver_index: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: vec![PointKey(0, 0)],
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: vec![ReducedSubgraphWireItem {
                    start: PointKey((-5.0f64).to_bits(), 0.0f64.to_bits()),
                    end: PointKey(0.0f64.to_bits(), 0.0f64.to_bits()),
                    is_bus_entry: false,
                    start_is_wire_side: false,
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
                name: "/RIGHT".to_string(),
                resolved_connection: test_bus_connection(
                    "/RIGHT",
                    "RIGHT",
                    "/RIGHT",
                    "",
                    Vec::new(),
                ),
                driver_connection: test_bus_connection("/RIGHT", "RIGHT", "/RIGHT", "", Vec::new()),
                chosen_driver_index: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(5.0f64.to_bits(), 5.0f64.to_bits()),
                points: vec![PointKey(5.0f64.to_bits(), 5.0f64.to_bits())],
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: vec![ReducedSubgraphWireItem {
                    start: PointKey(5.0f64.to_bits(), 5.0f64.to_bits()),
                    end: PointKey(10.0f64.to_bits(), 5.0f64.to_bits()),
                    is_bus_entry: false,
                    start_is_wire_side: false,
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
                subgraph_code: 3,
                code: 3,
                name: "/ENTRY".to_string(),
                resolved_connection: test_net_connection("/ENTRY", "ENTRY", "/ENTRY", ""),
                driver_connection: test_net_connection("/ENTRY", "ENTRY", "/ENTRY", ""),
                chosen_driver_index: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0.0f64.to_bits(), 0.0f64.to_bits()),
                points: vec![PointKey(0.0f64.to_bits(), 0.0f64.to_bits())],
                nodes: Vec::new(),
                base_pins: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: Vec::new(),
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: vec![ReducedSubgraphWireItem {
                    start: PointKey(0.0f64.to_bits(), 0.0f64.to_bits()),
                    end: PointKey(5.0f64.to_bits(), 5.0f64.to_bits()),
                    is_bus_entry: true,
                    start_is_wire_side: true,
                    connected_bus_subgraph_index: None,
                }],
                bus_neighbor_links: Vec::new(),
                bus_parent_links: Vec::new(),
                bus_parent_indexes: Vec::new(),
                hier_parent_index: None,
                hier_child_indexes: Vec::new(),
            },
        ];

        let live = build_live_reduced_subgraph_handles(&reduced);
        LiveReducedSubgraph::attach_connected_bus_items(&live);

        let attached_bus_connection = live[2].borrow().wire_items[0]
            .borrow()
            .connected_bus_connection_handle
            .clone()
            .expect("connected bus connection owner");

        assert!(Rc::ptr_eq(
            &attached_bus_connection,
            &live[1].borrow().driver_connection
        ));
        assert!(!Rc::ptr_eq(
            &attached_bus_connection,
            &live[0].borrow().driver_connection
        ));
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
    fn reduced_project_label_identity_can_differ_from_generic_point_identity() {
        let reduced = super::ReducedProjectNetGraph {
            subgraphs: vec![
                ReducedProjectSubgraphEntry {
                    subgraph_code: 1,
                    code: 1,
                    name: "label-net".to_string(),
                    resolved_connection: test_net_connection(
                        "label-net",
                        "LABEL_LOCAL",
                        "/LABEL_LOCAL",
                        "",
                    ),
                    driver_connection: test_net_connection(
                        "label-net",
                        "LABEL_LOCAL",
                        "/LABEL_LOCAL",
                        "",
                    ),
                    chosen_driver_index: None,
                    drivers: Vec::new(),
                    class: "LabelClass".to_string(),
                    has_no_connect: false,
                    sheet_instance_path: String::new(),
                    anchor: PointKey(0, 0),
                    points: vec![PointKey(0, 0)],
                    nodes: Vec::new(),
                    base_pins: Vec::new(),
                    label_links: vec![ReducedLabelLink {
                        schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                        at: PointKey(0, 0),
                        kind: LabelKind::Global,
                        dangling: false,
                        connection: test_net_connection(
                            "label-net",
                            "LABEL_LOCAL",
                            "/LABEL_LOCAL",
                            "",
                        ),
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
                ReducedProjectSubgraphEntry {
                    subgraph_code: 2,
                    code: 2,
                    name: "point-net".to_string(),
                    resolved_connection: test_net_connection(
                        "point-net",
                        "POINT_LOCAL",
                        "/POINT_LOCAL",
                        "",
                    ),
                    driver_connection: test_net_connection(
                        "point-net",
                        "POINT_LOCAL",
                        "/POINT_LOCAL",
                        "",
                    ),
                    chosen_driver_index: None,
                    drivers: Vec::new(),
                    class: "PointClass".to_string(),
                    has_no_connect: false,
                    sheet_instance_path: String::new(),
                    anchor: PointKey(0, 0),
                    points: vec![PointKey(0, 0)],
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
            ],
            subgraphs_by_name: BTreeMap::new(),
            subgraphs_by_sheet_and_name: BTreeMap::new(),
            symbol_pins_by_symbol: BTreeMap::new(),
            pin_subgraph_identities: BTreeMap::new(),
            pin_subgraph_identities_by_location: BTreeMap::new(),
            point_subgraph_identities: BTreeMap::from([(
                super::ReducedProjectPointIdentityKey {
                    sheet_instance_path: String::new(),
                    at: PointKey(0, 0),
                },
                1,
            )]),
            label_subgraph_identities: BTreeMap::from([(
                super::ReducedProjectLabelIdentityKey {
                    sheet_instance_path: String::new(),
                    at: PointKey(0, 0),
                    kind: super::reduced_label_kind_sort_key(LabelKind::Global),
                },
                0,
            )]),
            no_connect_subgraph_identities: BTreeMap::new(),
            sheet_pin_subgraph_identities: BTreeMap::new(),
        };
        let sheet_path = crate::loader::LoadedSheetPath {
            schematic_path: std::path::PathBuf::from("root.kicad_sch"),
            instance_path: String::new(),
            symbol_path: String::new(),
            sheet_uuid: None,
            sheet_name: Some("Root".to_string()),
            page: Some("1".to_string()),
            sheet_number: 1,
            sheet_count: 1,
        };
        let label = crate::model::Label {
            kind: LabelKind::Global,
            text: "${SHORT_NET_NAME}/${NET_CLASS}/${NET_NAME}".to_string(),
            at: [0.0, 0.0],
            ..crate::model::Label::new(LabelKind::Global, String::new())
        };

        let by_label =
            resolve_reduced_project_net_for_label(&reduced, &sheet_path, &label).expect("label");
        let driver_name =
            resolve_reduced_project_driver_name_for_label(&reduced, &sheet_path, &label)
                .expect("label driver");
        let by_point =
            resolve_reduced_project_net_at(&reduced, &sheet_path, [0.0, 0.0]).expect("point");

        assert_eq!(by_label.name, "label-net");
        assert_eq!(by_label.class, "LabelClass");
        assert_eq!(driver_name, "LABEL_LOCAL");
        assert_eq!(by_point.name, "point-net");
    }

    #[test]
    fn reduced_project_sheet_pin_identity_uses_child_sheet_uuid() {
        let reduced = super::ReducedProjectNetGraph {
            subgraphs: vec![
                ReducedProjectSubgraphEntry {
                    subgraph_code: 1,
                    code: 1,
                    name: "/A".to_string(),
                    resolved_connection: test_net_connection("/A", "A", "/A", ""),
                    driver_connection: test_net_connection("/A", "A", "/A", ""),
                    chosen_driver_index: None,
                    drivers: Vec::new(),
                    class: String::new(),
                    has_no_connect: false,
                    sheet_instance_path: String::new(),
                    anchor: PointKey(0, 0),
                    points: vec![PointKey(0, 0)],
                    nodes: Vec::new(),
                    base_pins: Vec::new(),
                    label_links: Vec::new(),
                    no_connect_points: Vec::new(),
                    hier_sheet_pins: vec![ReducedHierSheetPinLink {
                        schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                        at: PointKey(0, 0),
                        child_sheet_uuid: Some("sheet-a".to_string()),
                        connection: test_net_connection("/A", "A", "/A", ""),
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
                    name: "/B".to_string(),
                    resolved_connection: test_net_connection("/B", "B", "/B", ""),
                    driver_connection: test_net_connection("/B", "B", "/B", ""),
                    chosen_driver_index: None,
                    drivers: Vec::new(),
                    class: String::new(),
                    has_no_connect: false,
                    sheet_instance_path: String::new(),
                    anchor: PointKey(0, 0),
                    points: vec![PointKey(0, 0)],
                    nodes: Vec::new(),
                    base_pins: Vec::new(),
                    label_links: Vec::new(),
                    no_connect_points: Vec::new(),
                    hier_sheet_pins: vec![ReducedHierSheetPinLink {
                        schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                        at: PointKey(0, 0),
                        child_sheet_uuid: Some("sheet-b".to_string()),
                        connection: test_net_connection("/B", "B", "/B", ""),
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
            ],
            subgraphs_by_name: BTreeMap::new(),
            subgraphs_by_sheet_and_name: BTreeMap::new(),
            symbol_pins_by_symbol: BTreeMap::new(),
            pin_subgraph_identities: BTreeMap::new(),
            pin_subgraph_identities_by_location: BTreeMap::new(),
            point_subgraph_identities: BTreeMap::from([(
                super::ReducedProjectPointIdentityKey {
                    sheet_instance_path: String::new(),
                    at: PointKey(0, 0),
                },
                1,
            )]),
            label_subgraph_identities: BTreeMap::new(),
            no_connect_subgraph_identities: BTreeMap::new(),
            sheet_pin_subgraph_identities: BTreeMap::from([
                (
                    super::ReducedProjectSheetPinIdentityKey {
                        sheet_instance_path: String::new(),
                        at: PointKey(0, 0),
                        child_sheet_uuid: Some("sheet-a".to_string()),
                    },
                    0,
                ),
                (
                    super::ReducedProjectSheetPinIdentityKey {
                        sheet_instance_path: String::new(),
                        at: PointKey(0, 0),
                        child_sheet_uuid: Some("sheet-b".to_string()),
                    },
                    1,
                ),
            ]),
        };
        let sheet_path = crate::loader::LoadedSheetPath {
            schematic_path: std::path::PathBuf::from("root.kicad_sch"),
            instance_path: String::new(),
            symbol_path: String::new(),
            sheet_uuid: None,
            sheet_name: Some("Root".to_string()),
            page: Some("1".to_string()),
            sheet_number: 1,
            sheet_count: 1,
        };

        let by_sheet_pin = resolve_reduced_project_subgraph_for_sheet_pin(
            &reduced,
            &sheet_path,
            [0.0, 0.0],
            Some("sheet-a"),
        )
        .expect("sheet pin subgraph");
        let by_point =
            resolve_reduced_project_subgraph_at(&reduced, &sheet_path, [0.0, 0.0]).expect("point");

        assert_eq!(by_sheet_pin.subgraph_code, 1);
        assert_eq!(by_point.subgraph_code, 2);
    }

    #[test]
    fn shown_sheet_pin_text_prefers_sheet_pin_identity_over_generic_point_identity() {
        let reduced = super::ReducedProjectNetGraph {
            subgraphs: vec![
                ReducedProjectSubgraphEntry {
                    subgraph_code: 1,
                    code: 1,
                    name: "sheet-net".to_string(),
                    resolved_connection: test_net_connection(
                        "sheet-net",
                        "SHEET_LOCAL",
                        "/Child/SHEET_LOCAL",
                        "",
                    ),
                    driver_connection: test_net_connection(
                        "sheet-net",
                        "SHEET_LOCAL",
                        "/Child/SHEET_LOCAL",
                        "",
                    ),
                    chosen_driver_index: None,
                    drivers: Vec::new(),
                    class: "SheetClass".to_string(),
                    has_no_connect: false,
                    sheet_instance_path: String::new(),
                    anchor: PointKey(0, 0),
                    points: vec![PointKey(0, 0)],
                    nodes: Vec::new(),
                    base_pins: Vec::new(),
                    label_links: Vec::new(),
                    no_connect_points: Vec::new(),
                    hier_sheet_pins: vec![ReducedHierSheetPinLink {
                        schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                        at: PointKey(0, 0),
                        child_sheet_uuid: Some("child-sheet".to_string()),
                        connection: test_net_connection(
                            "sheet-net",
                            "SHEET_LOCAL",
                            "/Child/SHEET_LOCAL",
                            "",
                        ),
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
                    name: "point-net".to_string(),
                    resolved_connection: test_net_connection(
                        "point-net",
                        "POINT_LOCAL",
                        "/POINT_LOCAL",
                        "",
                    ),
                    driver_connection: test_net_connection(
                        "point-net",
                        "POINT_LOCAL",
                        "/POINT_LOCAL",
                        "",
                    ),
                    chosen_driver_index: None,
                    drivers: Vec::new(),
                    class: "PointClass".to_string(),
                    has_no_connect: false,
                    sheet_instance_path: String::new(),
                    anchor: PointKey(0, 0),
                    points: vec![PointKey(0, 0)],
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
            ],
            subgraphs_by_name: BTreeMap::new(),
            subgraphs_by_sheet_and_name: BTreeMap::new(),
            symbol_pins_by_symbol: BTreeMap::new(),
            pin_subgraph_identities: BTreeMap::new(),
            pin_subgraph_identities_by_location: BTreeMap::new(),
            point_subgraph_identities: BTreeMap::from([(
                super::ReducedProjectPointIdentityKey {
                    sheet_instance_path: String::new(),
                    at: PointKey(0, 0),
                },
                1,
            )]),
            label_subgraph_identities: BTreeMap::new(),
            no_connect_subgraph_identities: BTreeMap::new(),
            sheet_pin_subgraph_identities: BTreeMap::from([(
                super::ReducedProjectSheetPinIdentityKey {
                    sheet_instance_path: String::new(),
                    at: PointKey(0, 0),
                    child_sheet_uuid: Some("child-sheet".to_string()),
                },
                0,
            )]),
        };
        let parent_sheet_path = crate::loader::LoadedSheetPath {
            schematic_path: std::path::PathBuf::from("root.kicad_sch"),
            instance_path: String::new(),
            symbol_path: String::new(),
            sheet_uuid: None,
            sheet_name: Some("Root".to_string()),
            page: Some("1".to_string()),
            sheet_number: 1,
            sheet_count: 2,
        };
        let child_sheet_path = crate::loader::LoadedSheetPath {
            schematic_path: std::path::PathBuf::from("child.kicad_sch"),
            instance_path: "/child-sheet".to_string(),
            symbol_path: String::new(),
            sheet_uuid: Some("child-sheet".to_string()),
            sheet_name: Some("Child".to_string()),
            page: Some("2".to_string()),
            sheet_number: 2,
            sheet_count: 2,
        };
        let sheet = crate::model::Sheet {
            uuid: Some("child-sheet".to_string()),
            ..crate::model::Sheet::new()
        };
        let pin = crate::model::SheetPin {
            name: "${SHORT_NET_NAME}/${NET_CLASS}/${NET_NAME}".to_string(),
            at: [0.0, 0.0],
            ..crate::model::SheetPin::new("SIG".to_string(), &sheet)
        };

        let shown = crate::loader::shown_sheet_pin_text(
            &[],
            &[parent_sheet_path.clone(), child_sheet_path.clone()],
            &parent_sheet_path,
            &child_sheet_path,
            None,
            None,
            Some(&reduced),
            &pin,
        );

        assert_eq!(shown, "SHEET_LOCAL/SheetClass/sheet-net");
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
    fn reduced_project_subgraph_lookup_absorbs_same_sheet_duplicate_name() {
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
        assert_eq!(first_by_point.subgraph_code, second_by_point.subgraph_code);
        assert!(
            first_by_point
                .points
                .contains(&super::point_key([10.0, 0.0]))
        );
        assert!(
            first_by_point
                .points
                .contains(&super::point_key([10.0, 20.0]))
        );

        let by_name =
            find_reduced_project_subgraph_by_name(&graph, &first_by_point.name, root_sheet)
                .expect("same-sheet lookup");
        let by_first = find_first_reduced_project_subgraph_by_name(&graph, &first_by_point.name)
            .expect("global same-name lookup");
        assert_eq!(by_name.subgraph_code, first_by_point.subgraph_code);
        assert_eq!(by_first.subgraph_code, first_by_point.subgraph_code);
        assert_eq!(
            super::collect_reduced_project_subgraphs_by_name(&graph, &first_by_point.name).len(),
            1
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reduced_absorb_reresolves_driver_after_merge() {
        let mut subgraphs = vec![
            test_net_subgraph(
                1,
                test_net_connection("/SIG", "SIG", "/SIG", ""),
                vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: super::reduced_hierarchical_label_driver_priority(),
                    connection: test_net_connection("/SIG", "SIG", "/SIG", ""),
                    identity: None,
                }],
                "",
            ),
            test_net_subgraph(
                2,
                test_net_connection("/SIG", "SIG", "/SIG", ""),
                vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: super::reduced_local_label_driver_priority(),
                    connection: test_net_connection("/SIG", "SIG", "/SIG", ""),
                    identity: None,
                }],
                "",
            ),
        ];

        super::reduced_project_absorb_primary_same_name_subgraphs(&mut subgraphs);

        assert_eq!(subgraphs.len(), 1);
        assert_eq!(subgraphs[0].drivers.len(), 2);
        let chosen = subgraphs[0]
            .chosen_driver_index
            .and_then(|index| subgraphs[0].drivers.get(index))
            .expect("chosen driver");
        assert_eq!(
            chosen.priority,
            super::reduced_local_label_driver_priority()
        );
    }

    #[test]
    fn reduced_absorb_merges_same_sheet_duplicate_bus_drivers() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_duplicate_bus_absorb_{}.kicad_sch",
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
  (uuid "73050000-0000-0000-0000-000000000202")
  (paper "A4")
  (bus (pts (xy 0 0) (xy 10 0)))
  (label "DATA[0..1]" (at 10 0 0) (effects (font (size 1 1))))
  (bus (pts (xy 0 20) (xy 10 20)))
  (label "DATA[0..1]" (at 10 20 0) (effects (font (size 1 1)))))"#,
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
            .expect("first bus subgraph");
        let second_by_point = resolve_reduced_project_subgraph_at(&graph, root_sheet, [10.0, 20.0])
            .expect("second bus subgraph");

        assert_eq!(first_by_point.subgraph_code, second_by_point.subgraph_code);
        assert_eq!(
            first_by_point.driver_connection.connection_type,
            ReducedProjectConnectionType::Bus
        );
        assert_eq!(
            super::collect_reduced_project_subgraphs_by_name(&graph, &first_by_point.name).len(),
            1
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reduced_absorb_uses_candidate_secondary_driver_names() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_secondary_absorb_{}.kicad_sch",
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
  (uuid "73050000-0000-0000-0000-000000000203")
  (paper "A4")
  (wire (pts (xy 0 0) (xy 10 0)))
  (global_label "PWR" (shape input) (at 10 0 0) (effects (font (size 1 1))))
  (wire (pts (xy 0 20) (xy 20 20)))
  (global_label "AAA" (shape input) (at 0 20 0) (effects (font (size 1 1))))
  (global_label "PWR" (shape input) (at 20 20 0) (effects (font (size 1 1)))))"#,
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
            .expect("first net subgraph");
        let second_by_point = resolve_reduced_project_subgraph_at(&graph, root_sheet, [20.0, 20.0])
            .expect("second net subgraph");

        assert_eq!(first_by_point.subgraph_code, second_by_point.subgraph_code);
        assert_eq!(first_by_point.driver_connection.name, "AAA");
        assert_eq!(
            super::collect_reduced_project_subgraphs_by_name(&graph, "AAA").len(),
            1
        );
        assert!(super::collect_reduced_project_subgraphs_by_name(&graph, "PWR").is_empty());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reduced_absorb_uses_parent_secondary_driver_names() {
        let mut subgraphs = vec![
            test_net_subgraph(
                1,
                test_net_connection("/AAA", "AAA", "/AAA", ""),
                vec![
                    ReducedProjectStrongDriver {
                        kind: ReducedProjectDriverKind::Label,
                        priority: super::reduced_local_label_driver_priority(),
                        connection: test_net_connection("/AAA", "AAA", "/AAA", ""),
                        identity: None,
                    },
                    ReducedProjectStrongDriver {
                        kind: ReducedProjectDriverKind::Label,
                        priority: super::reduced_local_label_driver_priority(),
                        connection: test_net_connection("/PWR", "PWR", "/PWR", ""),
                        identity: None,
                    },
                ],
                "",
            ),
            test_net_subgraph(
                2,
                test_net_connection("/PWR", "PWR", "/PWR", ""),
                vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: super::reduced_local_label_driver_priority(),
                    connection: test_net_connection("/PWR", "PWR", "/PWR", ""),
                    identity: None,
                }],
                "",
            ),
        ];
        subgraphs[0].chosen_driver_index = Some(0);
        subgraphs[1].chosen_driver_index = Some(0);

        super::reduced_project_absorb_primary_same_name_subgraphs(&mut subgraphs);

        assert_eq!(subgraphs.len(), 1);
        assert!(
            subgraphs[0]
                .drivers
                .iter()
                .any(|driver| driver.connection.name == "/PWR")
        );
    }

    #[test]
    fn reduced_absorb_secondary_matching_skips_implicit_chosen_driver() {
        let candidate = test_net_subgraph(
            1,
            test_net_connection("/RESOLVED", "RESOLVED", "/RESOLVED", ""),
            vec![
                ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: super::reduced_local_label_driver_priority(),
                    connection: test_net_connection("/ALIAS", "ALIAS", "/ALIAS", ""),
                    identity: None,
                },
                ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: super::reduced_local_label_driver_priority(),
                    connection: test_net_connection("/OTHER", "OTHER", "/OTHER", ""),
                    identity: None,
                },
            ],
            "",
        );

        assert!(!super::reduced_project_absorb_candidate_matches_name(
            &candidate, "/ALIAS"
        ));
        assert!(super::reduced_project_absorb_candidate_matches_name(
            &candidate, "/OTHER"
        ));
    }

    #[test]
    fn reduced_absorb_follows_absorbed_candidate_secondary_driver_names() {
        let mut subgraphs = vec![
            test_net_subgraph(
                1,
                test_net_connection("/AAA", "AAA", "/AAA", ""),
                vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: super::reduced_local_label_driver_priority(),
                    connection: test_net_connection("/AAA", "AAA", "/AAA", ""),
                    identity: None,
                }],
                "",
            ),
            test_net_subgraph(
                2,
                test_net_connection("/AAA", "AAA", "/AAA", ""),
                vec![
                    ReducedProjectStrongDriver {
                        kind: ReducedProjectDriverKind::Label,
                        priority: super::reduced_local_label_driver_priority(),
                        connection: test_net_connection("/AAA", "AAA", "/AAA", ""),
                        identity: None,
                    },
                    ReducedProjectStrongDriver {
                        kind: ReducedProjectDriverKind::Label,
                        priority: super::reduced_local_label_driver_priority(),
                        connection: test_net_connection("/CCC", "CCC", "/CCC", ""),
                        identity: None,
                    },
                ],
                "",
            ),
            test_net_subgraph(
                3,
                test_net_connection("/CCC", "CCC", "/CCC", ""),
                vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: super::reduced_local_label_driver_priority(),
                    connection: test_net_connection("/CCC", "CCC", "/CCC", ""),
                    identity: None,
                }],
                "",
            ),
        ];
        subgraphs[0].chosen_driver_index = Some(0);
        subgraphs[1].chosen_driver_index = Some(0);
        subgraphs[2].chosen_driver_index = Some(0);

        super::reduced_project_absorb_primary_same_name_subgraphs(&mut subgraphs);

        assert_eq!(subgraphs.len(), 1);
        assert!(
            subgraphs[0]
                .drivers
                .iter()
                .any(|driver| driver.connection.name == "/CCC")
        );
    }

    #[test]
    fn reduced_absorb_uses_parent_bus_member_secondary_driver_names() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_bus_secondary_absorb_{}.kicad_sch",
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
  (uuid "73050000-0000-0000-0000-000000000204")
  (paper "A4")
  (bus (pts (xy 0 0) (xy 10 0)))
  (label "DATA[0..0]" (at 10 0 0) (effects (font (size 1 1))))
  (bus (pts (xy 0 20) (xy 20 20)))
  (label "ALT[0..0]" (at 0 20 0) (effects (font (size 1 1))))
  (label "DATA0" (at 20 20 0) (effects (font (size 1 1)))))"#,
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

        let parent_bus = resolve_reduced_project_subgraph_at(&graph, root_sheet, [10.0, 0.0])
            .expect("parent bus subgraph");
        let candidate_bus = resolve_reduced_project_subgraph_at(&graph, root_sheet, [20.0, 20.0])
            .expect("candidate bus subgraph");

        assert_eq!(parent_bus.subgraph_code, candidate_bus.subgraph_code);
        assert_eq!(
            parent_bus.driver_connection.connection_type,
            ReducedProjectConnectionType::Bus
        );
        assert!(
            parent_bus
                .drivers
                .iter()
                .any(|driver| driver.connection.local_name == "DATA0")
        );

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
    fn reduced_project_item_connections_get_graph_owned_net_codes() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_item_connection_net_codes_{}.kicad_sch",
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
    (symbol "device:OnePin"
      (property "Reference" "U" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "OnePin" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "OnePin_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "A" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "device:OnePin")
    (uuid "73050000-0000-0000-0000-000000000631")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "U1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "OnePin" (at 0 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy 10 0)))
  (global_label "SIG" (shape input) (at 10 0 0) (effects (font (size 1 1)))))"#,
        )
        .expect("write schematic");

        let loaded = load_schematic_tree(&path).expect("load tree");
        let project = SchematicProject::from_load_result(loaded);
        let graph = project.reduced_project_net_graph(false);
        let subgraph = graph
            .subgraphs
            .iter()
            .find(|subgraph| subgraph.name == "SIG")
            .expect("SIG subgraph");

        assert_eq!(subgraph.driver_connection.net_code, 1);
        assert_eq!(subgraph.base_pins[0].connection.net_code, 1);
        assert_eq!(subgraph.base_pins[0].driver_connection.name, "Net-(U1-A)");
        assert_eq!(subgraph.base_pins[0].driver_connection.net_code, 2);
        assert_eq!(subgraph.drivers[0].connection.net_code, 1);

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
    fn reduced_bus_member_net_codes_follow_assign_net_codes_queue_order() {
        let mut connection = ReducedProjectConnection {
            net_code: 0,
            connection_type: ReducedProjectConnectionType::BusGroup,
            name: "GROUP".to_string(),
            local_name: "GROUP".to_string(),
            full_local_name: "GROUP".to_string(),
            sheet_instance_path: String::new(),
            members: vec![
                ReducedBusMember {
                    net_code: 0,
                    name: "A[0]".to_string(),
                    local_name: "A".to_string(),
                    full_local_name: "A[0]".to_string(),
                    vector_index: None,
                    kind: ReducedBusMemberKind::Bus,
                    members: vec![ReducedBusMember {
                        net_code: 0,
                        name: "A0".to_string(),
                        local_name: "A0".to_string(),
                        full_local_name: "A0".to_string(),
                        vector_index: Some(0),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }],
                },
                ReducedBusMember {
                    net_code: 0,
                    name: "B".to_string(),
                    local_name: "B".to_string(),
                    full_local_name: "B".to_string(),
                    vector_index: None,
                    kind: ReducedBusMemberKind::Net,
                    members: Vec::new(),
                },
            ],
        };
        let mut net_codes = BTreeMap::new();

        super::assign_reduced_connection_net_codes(&mut connection, &mut net_codes);

        assert_eq!(connection.net_code, 1);
        assert_eq!(connection.members[0].net_code, 0);
        assert_eq!(connection.members[1].net_code, 2);
        assert_eq!(connection.members[0].members[0].net_code, 3);
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
    fn reduced_project_no_connect_pin_owner_reads_graph_point_owners() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_no_connect_pin_owner_{}.kicad_sch",
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
    (symbol "Device:NC"
      (symbol "NC_1_1"
        (pin no_connect line
          (at 0 0 0)
          (length 2.54)
          (name "NC")
          (number "1")))))
  (symbol
    (lib_id "Device:NC")
    (at 0 0 0)
    (uuid "73050000-0000-0000-0000-0000000008aa"))
  (wire (pts (xy 0 0) (xy 10 0))))"#,
        )
        .expect("write schematic");

        let loaded = load_schematic_tree(&path).expect("load tree");
        let project = SchematicProject::from_load_result(loaded);
        let root_sheet = project
            .sheet_paths
            .iter()
            .find(|sheet_path| sheet_path.instance_path.is_empty())
            .expect("root sheet");
        let graph = project.reduced_project_net_graph(false);
        let pin =
            super::collect_reduced_project_symbol_pin_inventories_in_sheet(&graph, root_sheet)
                .into_iter()
                .flat_map(|inventory| inventory.pins.iter())
                .find(|pin| pin.electrical_type.as_deref() == Some("no_connect"))
                .expect("no-connect pin");

        assert!(super::reduced_project_no_connect_pin_has_connected_owner(
            &graph, pin,
        ));

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
                chosen_driver_index: None,
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
                        reference: None,
                        number: Some("1".to_string()),
                        electrical_type: None,
                        visible: true,
                        is_power_symbol: true,
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
                        reference: None,
                        number: Some("2".to_string()),
                        electrical_type: None,
                        visible: true,
                        is_power_symbol: true,
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
            symbol_pins_by_symbol: BTreeMap::new(),
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
            sheet_pin_subgraph_identities: BTreeMap::new(),
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
                chosen_driver_index: None,
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
            symbol_pins_by_symbol: BTreeMap::new(),
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
            sheet_pin_subgraph_identities: BTreeMap::new(),
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
    fn reduced_project_symbol_pin_inventory_keeps_unconnected_pins() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_symbol_pin_inventory_{}.kicad_sch",
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
    (symbol "device:U"
      (property "Reference" "U" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "U" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "U_1_1"
        (pin input line (at 0 0 180) (length 2.54)
          (name "IN" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1)))))
        (pin output line (at 10 0 0) (length 2.54)
          (name "OUT" (effects (font (size 1 1))))
          (number "2" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "device:U")
    (uuid "73050000-0000-0000-0000-000000000991")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "U1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "U" (at 0 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy -10 0)))
  (global_label "SIG" (shape input) (at -10 0 0) (effects (font (size 1 1)))))"#,
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
            .expect("symbol");

        let pins = super::collect_reduced_project_symbol_pins(&graph, &sheet_path, symbol);
        assert_eq!(pins.len(), 2);

        let connected_pin = pins
            .iter()
            .find(|pin| pin.number.as_deref() == Some("1"))
            .expect("connected pin");
        let unconnected_pin = pins
            .iter()
            .find(|pin| pin.number.as_deref() == Some("2"))
            .expect("unconnected pin");

        assert_eq!(connected_pin.reference.as_deref(), Some("U1"));
        assert_eq!(unconnected_pin.reference.as_deref(), Some("U1"));
        assert_ne!(connected_pin.subgraph_index, unconnected_pin.subgraph_index);

        let _ = fs::remove_file(path);
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
            visible: true,
        };
        let unit_pins = vec![
            pin.clone(),
            super::ProjectedSymbolPin {
                at: [10.0, 0.0],
                name: Some("A".to_string()),
                number: Some("[4-5]".to_string()),
                electrical_type: Some("input".to_string()),
                visible: true,
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
            visible: true,
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
    fn reduced_seeded_symbol_pin_connection_uses_local_power_pin_name() {
        let mut symbol = crate::model::Symbol::new();
        let mut lib_symbol = crate::model::LibSymbol::new("power:LOCAL_PWR".to_string());
        lib_symbol.power = true;
        lib_symbol.local_power = true;
        symbol.lib_symbol = Some(lib_symbol);
        symbol.set_field_text(
            crate::model::PropertyKind::SymbolReference,
            "#PWR1".to_string(),
        );

        let pin = super::ProjectedSymbolPin {
            at: [0.0, 0.0],
            name: Some("LOCAL_PWR".to_string()),
            number: Some("1".to_string()),
            electrical_type: Some("power_in".to_string()),
            visible: true,
        };
        let unit_pins = vec![pin.clone()];

        let connection =
            super::reduced_seeded_symbol_pin_connection(&symbol, &pin, &unit_pins, "/sheet/");

        assert_eq!(
            connection.connection_type,
            super::ReducedProjectConnectionType::Net
        );
        assert_eq!(connection.name, "LOCAL_PWR");
        assert_eq!(connection.local_name, "LOCAL_PWR");
        assert_eq!(connection.full_local_name, "LOCAL_PWR");
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
    fn reduced_resolve_drivers_skips_ordinary_pin_when_lib_reference_is_private() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_private_lib_ref_pin_driver_{}.kicad_sch",
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
    (symbol "device:PrivateDriver"
      (property "Reference" "#U" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "PrivateDriver" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "PrivateDriver_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "A" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "device:PrivateDriver")
    (uuid "73050000-0000-0000-0000-000000000621")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "U1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "PrivateDriver" (at 0 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy 10 0))))"##,
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
        let component =
            super::connection_component_for_symbol_pin(&schematic, symbol, [0.0, 0.0], Some("1"))
                .expect("component");

        let candidate = super::resolve_reduced_driver_name_candidate_on_component(
            &schematic,
            &component,
            |label| label.text.clone(),
            |_sheet, pin| pin.name.clone(),
        );
        let drivers = super::collect_reduced_strong_drivers(
            &schematic,
            &path,
            "",
            &component,
            "",
            |label| label.text.clone(),
            |_sheet, pin| pin.name.clone(),
        );

        assert!(candidate.is_none());
        assert!(drivers.is_empty());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reduced_driver_name_candidate_skips_ordinary_pin_outside_netlist() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_non_netlist_pin_driver_{}.kicad_sch",
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
    (symbol "device:NoNetlist"
      (property "Reference" "U" (id 0) (at 0 0 0) (effects (font (size 1 1))))
      (property "Value" "NoNetlist" (id 1) (at 0 0 0) (effects (font (size 1 1))))
      (symbol "NoNetlist_1_1"
        (pin passive line (at 0 0 180) (length 2.54)
          (name "A" (effects (font (size 1 1))))
          (number "1" (effects (font (size 1 1))))))))
  (symbol
    (lib_id "device:NoNetlist")
    (uuid "73050000-0000-0000-0000-000000000622")
    (at 0 0 0)
    (unit 1)
    (property "Reference" "#U1" (at 0 0 0) (effects (font (size 1 1))))
    (property "Value" "NoNetlist" (at 0 0 0) (effects (font (size 1 1)))))
  (wire (pts (xy 0 0) (xy 10 0))))"##,
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
        let component =
            super::connection_component_for_symbol_pin(&schematic, symbol, [0.0, 0.0], Some("1"))
                .expect("component");

        let candidate = super::resolve_reduced_driver_name_candidate_on_component(
            &schematic,
            &component,
            |label| label.text.clone(),
            |_sheet, pin| pin.name.clone(),
        );

        assert!(candidate.is_none());

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
    fn collect_reduced_strong_drivers_prefers_output_sheet_pin_driver() {
        let root_path = env::temp_dir().join(format!(
            "ki2_connectivity_sheet_pin_strong_driver_rank_{}.kicad_sch",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let child_path = env::temp_dir().join(format!(
            "ki2_connectivity_sheet_pin_strong_driver_rank_child_{}.kicad_sch",
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
    (uuid "73050000-0000-0000-0000-000000000511")
    (property "Sheetname" "Child" (id 0) (at 0 0 0) (effects (font (size 1 1))))
    (property "Sheetfile" "{}" (id 1) (at 0 0 0) (effects (font (size 1 1))))
    (pin "Z" output (at 0 5 180) (uuid "73050000-0000-0000-0000-000000000512"))
    (pin "A" input (at 20 5 0) (uuid "73050000-0000-0000-0000-000000000513"))))"#,
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
        let drivers = super::collect_reduced_strong_drivers(
            schematic,
            &sheet_path.schematic_path,
            &sheet_path.instance_path,
            &component,
            "",
            |label| label.text.clone(),
            |_sheet, pin| pin.name.clone(),
        );

        assert_eq!(drivers.len(), 2);
        assert_eq!(drivers[0].kind, ReducedProjectDriverKind::SheetPin);
        assert_eq!(drivers[0].connection.local_name, "Z");
        assert_eq!(
            drivers[0].identity,
            Some(super::ReducedProjectDriverIdentity::SheetPin {
                schematic_path: sheet_path.schematic_path.clone(),
                at: super::point_key([0.0, 5.0]),
                child_sheet_uuid: Some("73050000-0000-0000-0000-000000000511".to_string()),
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
    fn reduced_four_way_junction_points_count_only_connectable_items() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_four_way_junction_points_{}.kicad_sch",
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
  (wire (pts (xy -10 0) (xy 0 0)))
  (wire (pts (xy 0 0) (xy 10 0)))
  (wire (pts (xy 0 -10) (xy 0 0)))
  (wire (pts (xy 0 0) (xy 0 10)))
  (bus (pts (xy 0 0) (xy 10 10)))
  (junction (at 0 0)))"#,
        )
        .expect("write schematic");

        let schematic = parse_schematic_file(&path).expect("parse schematic");
        let points = super::collect_reduced_four_way_junction_points(&schematic);

        assert_eq!(points, vec![[0.0, 0.0]]);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn reduced_label_component_snapshot_counts_non_endpoint_wire_segments() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_label_wire_touch_count_{}.kicad_sch",
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
  (label "L" (at 0 0 0) (effects (font (size 1 1))))
  (wire (pts (xy -10 0) (xy 10 0)))
  (wire (pts (xy 0 -10) (xy 0 10))))"#,
        )
        .expect("write schematic");

        let schematic = parse_schematic_file(&path).expect("parse schematic");
        let snapshots = super::collect_reduced_label_component_snapshots(&schematic);
        let label = snapshots
            .iter()
            .flat_map(|snapshot| snapshot.labels.iter())
            .find(|label| label.kind == LabelKind::Local)
            .expect("local label snapshot");

        assert_eq!(label.non_endpoint_wire_segment_count, 2);
        assert!(!label.dangling);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn reduced_project_label_links_carry_dangling_state() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_label_link_dangling_{}.kicad_sch",
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
  (label "L" (at 0 0 0) (effects (font (size 1 1)))))"#,
        )
        .expect("write schematic");

        let loaded = load_schematic_tree(&path).expect("load tree");
        let project = SchematicProject::from_load_result(loaded);
        let graph = project.reduced_project_net_graph(false);
        let link = graph
            .subgraphs
            .iter()
            .flat_map(|subgraph| subgraph.label_links.iter())
            .find(|label| label.kind == LabelKind::Local)
            .expect("local label link");

        assert!(link.dangling);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reduced_dangling_directive_label_points_filter_directives() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_dangling_directive_labels_{}.kicad_sch",
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
  (directive_label "D" (at 0 0 0) (effects (font (size 1 1))))
  (label "L" (at 10 0 0) (effects (font (size 1 1)))))"#,
        )
        .expect("write schematic");

        let schematic = parse_schematic_file(&path).expect("parse schematic");
        let points = super::collect_reduced_dangling_directive_label_points(&schematic);

        assert_eq!(points, vec![[0.0, 0.0]]);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn reduced_connection_components_keep_reversed_bus_entries_out_of_bus_component() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_reversed_bus_entry_components_{}.kicad_sch",
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
  (wire (pts (xy -5 0) (xy 0 0)))
  (label "ADDR9" (at -5 0 0) (effects (font (size 1 1))))
  (bus (pts (xy 5 5) (xy 15 5)))
  (global_label "DATA[0..7]" (shape input) (at 15 5 0) (effects (font (size 1 1))))
  (bus_entry (at 0 0) (size 5 5)))"#,
        )
        .expect("write schematic");

        let schematic = crate::parser::parse_schematic_file(&path).expect("parse schematic");
        let components = super::collect_connection_components_with_options(&schematic, false);

        assert_eq!(components.len(), 2, "{components:#?}");
        assert!(components.iter().any(|component| {
            component.members.iter().any(|member| {
                member.kind == super::ConnectionMemberKind::Bus
                    && crate::loader::points_equal(member.at, [5.0, 5.0])
            }) && component.members.iter().any(|member| {
                member.kind == super::ConnectionMemberKind::Label
                    && crate::loader::points_equal(member.at, [15.0, 5.0])
            })
        }));
        assert!(components.iter().any(|component| {
            component.members.iter().any(|member| {
                member.kind == super::ConnectionMemberKind::Wire
                    && crate::loader::points_equal(member.at, [0.0, 0.0])
            }) && component.members.iter().any(|member| {
                member.kind == super::ConnectionMemberKind::Label
                    && crate::loader::points_equal(member.at, [-5.0, 0.0])
            })
        }));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn reduced_connection_components_keep_dangling_bus_entries_out_of_bus_component() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_dangling_bus_entry_components_{}.kicad_sch",
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
  (global_label "DATA[0..7]" (shape input) (at 10 0 0) (effects (font (size 1 1))))
  (bus_entry (at 5 0) (size 5 5)))"#,
        )
        .expect("write schematic");

        let schematic = crate::parser::parse_schematic_file(&path).expect("parse schematic");
        let components = super::collect_connection_components_with_options(&schematic, false);

        assert_eq!(components.len(), 2, "{components:#?}");
        assert!(components.iter().any(|component| {
            component
                .members
                .iter()
                .any(|member| member.kind == super::ConnectionMemberKind::Bus)
                && component.members.iter().any(|member| {
                    member.kind == super::ConnectionMemberKind::Label
                        && crate::loader::points_equal(member.at, [10.0, 0.0])
                })
        }));
        assert!(components.iter().any(|component| {
            component.members.len() == 1
                && component.members[0].kind == super::ConnectionMemberKind::BusEntry
                && crate::loader::points_equal(component.members[0].at, [10.0, 5.0])
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
    fn reduced_project_graph_assigns_bus_entry_connected_bus_owner_indexes() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_bus_entry_owner_indexes_{}.kicad_sch",
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
  (uuid "73050000-0000-0000-0000-0000000007aa")
  (paper "A4")
  (wire (pts (xy -5 5) (xy 0 0)))
  (bus (pts (xy 0 0) (xy 10 0)))
  (bus_entry (at 0 0) (size -5 5))
  (label "ADDR9" (at -5 5 0) (effects (font (size 1 1))))
  (global_label "DATA[0..7]" (shape input) (at 10 0 0) (effects (font (size 1 1)))))"#,
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

        let entry = resolve_reduced_project_subgraph_at(&graph, root_sheet, [-5.0, 5.0])
            .expect("entry subgraph");

        let bus_entry = entry
            .wire_items
            .iter()
            .find(|item| item.is_bus_entry)
            .expect("bus entry wire item");
        let connected_bus_index = bus_entry
            .connected_bus_subgraph_index
            .expect("connected bus owner index");
        let connected_bus =
            super::reduced_project_connected_bus_subgraph_for_wire_item(&graph, entry, bus_entry)
                .expect("connected bus subgraph");
        assert_eq!(
            super::reduced_project_subgraph_index(&graph, connected_bus)
                .expect("connected bus graph index"),
            connected_bus_index
        );
        assert!(
            super::reduced_project_wire_item_endpoint_has_connected_bus_owner(
                &graph,
                entry,
                bus_entry,
                bus_entry.end,
            )
        );
        assert!(
            !super::reduced_project_wire_item_endpoint_has_connected_bus_owner(
                &graph,
                entry,
                bus_entry,
                bus_entry.start,
            )
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reduced_project_bus_links_use_secondary_driver_names() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_bus_secondary_driver_links_{}.kicad_sch",
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
  (uuid "73050000-0000-0000-0000-000000000702")
  (paper "A4")
  (bus (pts (xy 0 0) (xy 10 0)))
  (global_label "{PWR}" (shape input) (at 10 0 0) (effects (font (size 1 1))))
  (wire (pts (xy 0 20) (xy 10 20)))
  (global_label "AAA" (shape input) (at 0 20 0) (effects (font (size 1 1))))
  (global_label "PWR" (shape input) (at 10 20 0) (effects (font (size 1 1)))))"#,
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

        assert_eq!(net.driver_connection.full_local_name, "AAA");
        assert!(bus.bus_neighbor_links.iter().any(|link| {
            link.member.local_name == "PWR"
                && link.member.full_local_name == "AAA"
                && link.subgraph_index == net.subgraph_code - 1
        }));
        assert!(net.bus_parent_links.iter().any(|link| {
            link.member.local_name == "PWR"
                && link.member.full_local_name == "AAA"
                && link.subgraph_index == bus.subgraph_code - 1
        }));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reduced_project_bus_links_skip_secondary_sheet_pin_names() {
        let root_path = env::temp_dir().join(format!(
            "ki2_connectivity_bus_secondary_sheet_pin_links_{}.kicad_sch",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let child_path = env::temp_dir().join(format!(
            "ki2_connectivity_bus_secondary_sheet_pin_links_child_{}.kicad_sch",
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
  (uuid "73050000-0000-0000-0000-000000000703")
  (paper "A4")
  (bus (pts (xy 0 0) (xy 10 0)))
  (global_label "{{B}}" (shape input) (at 10 0 0) (effects (font (size 1 1))))
  (wire (pts (xy 0 20) (xy 20 20)))
  (sheet (at 0 15) (size 20 10)
    (uuid "73050000-0000-0000-0000-000000000704")
    (property "Sheetname" "Child" (id 0) (at 0 15 0) (effects (font (size 1 1))))
    (property "Sheetfile" "{}" (id 1) (at 0 17 0) (effects (font (size 1 1))))
    (pin "A" input (at 0 20 180) (uuid "73050000-0000-0000-0000-000000000705"))
    (pin "B" input (at 20 20 0) (uuid "73050000-0000-0000-0000-000000000706"))))"#,
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
        let project = SchematicProject::from_load_result(loaded);
        let root_sheet = project
            .sheet_paths
            .iter()
            .find(|sheet_path| sheet_path.instance_path.is_empty())
            .expect("root sheet path");
        let graph = project.reduced_project_net_graph(false);

        let bus = resolve_reduced_project_subgraph_at(&graph, root_sheet, [10.0, 0.0])
            .expect("bus subgraph");
        let net = resolve_reduced_project_subgraph_at(&graph, root_sheet, [20.0, 20.0])
            .expect("sheet-pin net subgraph");

        assert_eq!(net.driver_connection.local_name, "A");
        assert!(!bus.bus_neighbor_links.iter().any(|link| {
            link.member.local_name == "B" && link.subgraph_index == net.subgraph_code - 1
        }));

        let _ = fs::remove_file(root_path);
        let _ = fs::remove_file(child_path);
    }

    #[test]
    fn reduced_project_bus_links_skip_candidate_bus_driver_connections() {
        let path = env::temp_dir().join(format!(
            "ki2_connectivity_bus_candidate_bus_driver_links_{}.kicad_sch",
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
  (uuid "73050000-0000-0000-0000-000000000707")
  (paper "A4")
  (bus (pts (xy 0 0) (xy 10 0)))
  (label "DATA[0..0]" (at 10 0 0) (effects (font (size 1 1))))
  (bus (pts (xy 0 20) (xy 10 20)))
  (global_label "{DATA0}" (shape input) (at 0 20 0) (effects (font (size 1 1))))
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
        let candidate_bus = resolve_reduced_project_subgraph_at(&graph, root_sheet, [10.0, 20.0])
            .expect("candidate bus subgraph");

        assert_eq!(
            candidate_bus.driver_connection.connection_type,
            ReducedProjectConnectionType::BusGroup
        );
        assert!(!bus.bus_neighbor_links.iter().any(|link| {
            link.member.local_name == "DATA0"
                && link.subgraph_index == candidate_bus.subgraph_code - 1
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
            chosen_driver_index: None,
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
            chosen_driver_index: None,
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
        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        let component = live_subgraphs.iter().cloned().collect::<Vec<_>>();
        LiveReducedSubgraph::refresh_bus_link_members(&live_subgraphs, &component);
        apply_live_reduced_driver_connections_from_handles(&mut graph, &live_subgraphs);
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
                chosen_driver_index: None,
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
                chosen_driver_index: None,
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
                chosen_driver_index: None,
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
    fn recache_live_reduced_subgraph_name_keeps_prefix_aliases_stale() {
        let live_subgraphs = vec![LiveReducedSubgraph {
            source_index: 0,
            driver_connection: Rc::new(RefCell::new(
                ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/A[0]".to_string(),
                    local_name: "A0".to_string(),
                    full_local_name: "/A[0]".to_string(),
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
        let (mut by_name, mut by_sheet_and_name) =
            build_live_reduced_name_caches_from_handles(&handles);

        {
            let subgraph = handles[0].borrow();
            let mut connection = subgraph.driver_connection.borrow_mut();
            connection.name = "/B[0]".to_string();
            connection.local_name = "B0".to_string();
            connection.full_local_name = "/B[0]".to_string();
        }

        recache_live_reduced_subgraph_name_from_handles(
            &handles,
            &mut by_name,
            &mut by_sheet_and_name,
            0,
            "/A[0]",
        );

        assert_eq!(by_name.get("/A[0]"), Some(&Vec::new()));
        assert_eq!(by_name.get("/A[]"), Some(&vec![0]));
        assert_eq!(by_name.get("/B[0]"), Some(&vec![0]));
        assert!(!by_name.contains_key("/B[]"));
    }

    #[test]
    fn recache_live_reduced_subgraph_name_handle_cache_keeps_prefix_aliases_stale() {
        let live_subgraph = Rc::new(RefCell::new(LiveReducedSubgraph {
            source_index: 0,
            driver_connection: Rc::new(RefCell::new(
                ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/A[0]".to_string(),
                    local_name: "A0".to_string(),
                    full_local_name: "/A[0]".to_string(),
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
        }));
        let handles = vec![live_subgraph.clone()];
        let (mut by_name, mut by_sheet_and_name) =
            super::build_live_reduced_name_handle_caches_from_handles(&handles);

        {
            let subgraph = live_subgraph.borrow();
            let mut connection = subgraph.driver_connection.borrow_mut();
            connection.name = "/B[0]".to_string();
            connection.local_name = "B0".to_string();
            connection.full_local_name = "/B[0]".to_string();
        }

        recache_live_reduced_subgraph_name_handle_cache_from_handles(
            &mut by_name,
            &mut by_sheet_and_name,
            &live_subgraph,
            "/A[0]",
        );

        assert!(
            by_name
                .get("/A[]")
                .into_iter()
                .flatten()
                .any(|handle| Rc::ptr_eq(handle, &live_subgraph))
        );
        assert!(
            by_name
                .get("/B[0]")
                .into_iter()
                .flatten()
                .any(|handle| Rc::ptr_eq(handle, &live_subgraph))
        );
        assert!(!by_name.contains_key("/B[]"));
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
            chosen_driver_index: None,
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
                start_is_wire_side: false,
                connected_bus_subgraph_index: None,
            }],
            wire_items: vec![ReducedSubgraphWireItem {
                start: PointKey(0, 0),
                end: PointKey(5, 5),
                is_bus_entry: true,
                start_is_wire_side: true,
                connected_bus_subgraph_index: None,
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
        assert!(!Rc::ptr_eq(
            &wire_item_connection,
            &shared.borrow().driver_connection
        ));
        assert_eq!(wire_item_connection.borrow().name, "/BUS");
        assert_eq!(
            wire_item_connection.borrow().connection_type,
            ReducedProjectConnectionType::Bus
        );
        let attached_bus_connection = shared.borrow().wire_items[0]
            .borrow()
            .connected_bus_connection_handle
            .clone()
            .expect("attached live bus connection");
        assert!(Rc::ptr_eq(
            &attached_bus_connection,
            &shared.borrow().driver_connection
        ));
        let attached_bus_item_connection = shared.borrow().bus_items[0].borrow().connection.clone();
        assert!(!Rc::ptr_eq(
            &attached_bus_item_connection,
            &shared.borrow().driver_connection
        ));
        assert_eq!(attached_bus_item_connection.borrow().name, "/BUS");
        assert_eq!(
            attached_bus_item_connection.borrow().connection_type,
            ReducedProjectConnectionType::Bus
        );
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
            chosen_driver_index: None,
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
                    child_sheet_uuid: Some("child".to_string()),
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
                        child_sheet_uuid,
                        ..
                    }) if schematic_path == std::path::PathBuf::from("root.kicad_sch")
                        && child_sheet_uuid.as_deref() == Some("child")
                ));
            }
            _ => panic!("expected sheet pin strong-driver owner"),
        }
    }

    #[test]
    fn build_live_reduced_subgraph_handles_match_sheet_pin_driver_by_child_sheet_uuid() {
        let reduced = vec![ReducedProjectSubgraphEntry {
            subgraph_code: 1,
            code: 1,
            name: "/SIG_A".to_string(),
            resolved_connection: ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Net,
                name: "/SIG_A".to_string(),
                local_name: "SIG_A".to_string(),
                full_local_name: "/SIG_A".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
            driver_connection: ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Net,
                name: "/SIG_A".to_string(),
                local_name: "SIG_A".to_string(),
                full_local_name: "/SIG_A".to_string(),
                sheet_instance_path: String::new(),
                members: Vec::new(),
            },
            chosen_driver_index: None,
            drivers: vec![ReducedProjectStrongDriver {
                kind: ReducedProjectDriverKind::SheetPin,
                priority: 1,
                connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/SIG_A".to_string(),
                    local_name: "SIG_A".to_string(),
                    full_local_name: "/SIG_A".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
                identity: Some(super::ReducedProjectDriverIdentity::SheetPin {
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                    at: PointKey(10, 20),
                    child_sheet_uuid: Some("sheet-a".to_string()),
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
            hier_sheet_pins: vec![
                ReducedHierSheetPinLink {
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                    at: PointKey(10, 20),
                    child_sheet_uuid: Some("sheet-b".to_string()),
                    connection: ReducedProjectConnection {
                        net_code: 2,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "/SIG_B".to_string(),
                        local_name: "SIG_B".to_string(),
                        full_local_name: "/SIG_B".to_string(),
                        sheet_instance_path: String::new(),
                        members: Vec::new(),
                    },
                },
                ReducedHierSheetPinLink {
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                    at: PointKey(10, 20),
                    child_sheet_uuid: Some("sheet-a".to_string()),
                    connection: ReducedProjectConnection {
                        net_code: 1,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "/SIG_A".to_string(),
                        local_name: "SIG_A".to_string(),
                        full_local_name: "/SIG_A".to_string(),
                        sheet_instance_path: String::new(),
                        members: Vec::new(),
                    },
                },
            ],
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
            super::LiveProjectStrongDriverOwner::SheetPin { owner, .. } => {
                owner.upgrade().expect("sheet pin owner")
            }
            _ => panic!("expected sheet pin strong-driver owner"),
        };

        assert!(Rc::ptr_eq(&owner, &subgraph.hier_sheet_pins[1]));
        assert!(!Rc::ptr_eq(&owner, &subgraph.hier_sheet_pins[0]));
        assert_eq!(owner.borrow().child_sheet_uuid.as_deref(), Some("sheet-a"));
    }

    #[test]
    fn build_live_reduced_subgraph_handles_seed_text_driver_owners() {
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
            chosen_driver_index: None,
            drivers: Vec::new(),
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: String::new(),
            anchor: PointKey(10, 20),
            points: Vec::new(),
            nodes: Vec::new(),
            base_pins: Vec::new(),
            label_links: vec![ReducedLabelLink {
                schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                at: PointKey(5, 6),
                kind: LabelKind::Global,
                dangling: false,
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
            hier_sheet_pins: vec![ReducedHierSheetPinLink {
                schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                at: PointKey(10, 20),
                child_sheet_uuid: Some("child".to_string()),
                connection: ReducedProjectConnection {
                    net_code: 2,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/CHILD".to_string(),
                    local_name: "CHILD".to_string(),
                    full_local_name: "/CHILD".to_string(),
                    sheet_instance_path: String::new(),
                    members: Vec::new(),
                },
            }],
            hier_ports: vec![ReducedHierPortLink {
                schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                at: PointKey(30, 40),
                connection: ReducedProjectConnection {
                    net_code: 3,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "/PORT".to_string(),
                    local_name: "PORT".to_string(),
                    full_local_name: "/PORT".to_string(),
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
        }];

        let handles = build_live_reduced_subgraph_handles(&reduced);
        let subgraph = handles[0].borrow();

        let label = subgraph.label_links[0].borrow();
        assert!(!Rc::ptr_eq(&label.connection, &label.driver_connection));
        assert!(super::live_connection_clone_eq(
            &label.connection.borrow(),
            &label.driver_connection.borrow()
        ));

        let sheet_pin = subgraph.hier_sheet_pins[0].borrow();
        assert!(!Rc::ptr_eq(
            &sheet_pin.connection,
            &sheet_pin.driver_connection
        ));
        assert!(super::live_connection_clone_eq(
            &sheet_pin.connection.borrow(),
            &sheet_pin.driver_connection.borrow()
        ));

        let hier_port = subgraph.hier_ports[0].borrow();
        assert!(!Rc::ptr_eq(
            &hier_port.connection,
            &hier_port.driver_connection
        ));
        assert!(super::live_connection_clone_eq(
            &hier_port.connection.borrow(),
            &hier_port.driver_connection.borrow()
        ));
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
            chosen_driver_index: None,
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
                dangling: false,
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
                        ..
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
            chosen_driver_index: Some(0),
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
                    sheet_instance_path: String::new(),
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
                reference: None,
                number: Some("1".to_string()),
                electrical_type: Some("power_in".to_string()),
                visible: true,
                is_power_symbol: true,
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
                        sheet_instance_path,
                        symbol_uuid: Some(ref uuid),
                        at: PointKey(10, 20),
                        pin_number: Some(ref pin_number),
                        ..
                    }) if sheet_instance_path.is_empty() && uuid == "sym" && pin_number == "1"
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
            chosen_driver_index: None,
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
                    sheet_instance_path: String::new(),
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
                reference: None,
                number: Some("1".to_string()),
                electrical_type: Some("power_in".to_string()),
                visible: true,
                is_power_symbol: true,
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
            chosen_driver_index: None,
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
                    sheet_instance_path: String::new(),
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
                reference: None,
                number: Some("1".to_string()),
                electrical_type: Some("power_in".to_string()),
                visible: true,
                is_power_symbol: true,
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
    fn build_live_reduced_subgraph_handles_match_symbol_pin_driver_by_sheet_instance_path() {
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
                sheet_instance_path: "/a".to_string(),
                members: Vec::new(),
            },
            driver_connection: ReducedProjectConnection {
                net_code: 1,
                connection_type: ReducedProjectConnectionType::Net,
                name: "PWR".to_string(),
                local_name: "PWR".to_string(),
                full_local_name: "PWR".to_string(),
                sheet_instance_path: "/a".to_string(),
                members: Vec::new(),
            },
            chosen_driver_index: None,
            drivers: vec![ReducedProjectStrongDriver {
                kind: ReducedProjectDriverKind::PowerPin,
                priority: 6,
                connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "PWR".to_string(),
                    local_name: "PWR".to_string(),
                    full_local_name: "PWR".to_string(),
                    sheet_instance_path: "/a".to_string(),
                    members: Vec::new(),
                },
                identity: Some(super::ReducedProjectDriverIdentity::SymbolPin {
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                    sheet_instance_path: "/a".to_string(),
                    symbol_uuid: Some("sym".to_string()),
                    at: PointKey(10, 20),
                    pin_number: Some("1".to_string()),
                }),
            }],
            class: String::new(),
            has_no_connect: false,
            sheet_instance_path: "/a".to_string(),
            anchor: PointKey(10, 20),
            points: Vec::new(),
            nodes: Vec::new(),
            base_pins: vec![
                super::ReducedProjectBasePin {
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                    key: super::ReducedNetBasePinKey {
                        sheet_instance_path: "/b".to_string(),
                        symbol_uuid: Some("sym".to_string()),
                        at: PointKey(10, 20),
                        name: Some("1".to_string()),
                        number: Some("1".to_string()),
                    },
                    reference: None,
                    number: Some("1".to_string()),
                    electrical_type: Some("power_in".to_string()),
                    visible: true,
                    is_power_symbol: true,
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "OTHER".to_string(),
                        local_name: "OTHER".to_string(),
                        full_local_name: "OTHER".to_string(),
                        sheet_instance_path: "/b".to_string(),
                        members: Vec::new(),
                    },
                    driver_connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "OTHER".to_string(),
                        local_name: "OTHER".to_string(),
                        full_local_name: "OTHER".to_string(),
                        sheet_instance_path: "/b".to_string(),
                        members: Vec::new(),
                    },
                    preserve_local_name_on_refresh: true,
                },
                super::ReducedProjectBasePin {
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                    key: super::ReducedNetBasePinKey {
                        sheet_instance_path: "/a".to_string(),
                        symbol_uuid: Some("sym".to_string()),
                        at: PointKey(10, 20),
                        name: Some("1".to_string()),
                        number: Some("1".to_string()),
                    },
                    reference: None,
                    number: Some("1".to_string()),
                    electrical_type: Some("power_in".to_string()),
                    visible: true,
                    is_power_symbol: true,
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "PWR".to_string(),
                        local_name: "PWR".to_string(),
                        full_local_name: "PWR".to_string(),
                        sheet_instance_path: "/a".to_string(),
                        members: Vec::new(),
                    },
                    driver_connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "PWR".to_string(),
                        local_name: "PWR".to_string(),
                        full_local_name: "PWR".to_string(),
                        sheet_instance_path: "/a".to_string(),
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

        let owner = match &*subgraph.drivers[0].borrow() {
            super::LiveProjectStrongDriverOwner::SymbolPin { owner, .. } => {
                owner.upgrade().expect("symbol pin owner")
            }
            _ => panic!("expected symbol pin strong-driver owner"),
        };

        assert!(Rc::ptr_eq(&owner, &subgraph.base_pins[1]));
        assert!(!Rc::ptr_eq(&owner, &subgraph.base_pins[0]));
        assert_eq!(owner.borrow().pin.key.sheet_instance_path, "/a");
    }

    #[test]
    fn live_reduced_subgraph_driver_priority_uses_chosen_driver() {
        let mut graph = vec![test_net_subgraph(
            1,
            test_net_connection("/LOCAL", "LOCAL", "/LOCAL", ""),
            vec![
                ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::PowerPin,
                    priority: super::reduced_global_power_pin_driver_priority(),
                    connection: test_net_connection("GLOBAL", "GLOBAL", "GLOBAL", ""),
                    identity: None,
                },
                ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: super::reduced_local_label_driver_priority(),
                    connection: test_net_connection("/LOCAL", "LOCAL", "/LOCAL", ""),
                    identity: None,
                },
            ],
            "",
        )];
        graph[0].chosen_driver_index = Some(1);

        let handles = build_live_reduced_subgraph_handles(&graph);
        let subgraph = handles[0].borrow();

        assert_eq!(
            super::live_reduced_subgraph_driver_priority(&subgraph),
            super::reduced_local_label_driver_priority()
        );
        assert!(super::live_subgraph_has_local_driver(&subgraph));
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
            chosen_driver_index: None,
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
                dangling: false,
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

        assert!(!Rc::ptr_eq(
            &owner.borrow().connection,
            &owner.borrow().driver_connection
        ));
        assert_eq!(owner.borrow().connection.borrow().name, "ITEM");
        assert_eq!(owner.borrow().driver_connection.borrow().name, "/SIG");
        assert_eq!(owner.borrow().shown_text_local_name, "ITEM");
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
            chosen_driver_index: None,
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
                        sheet_instance_path: String::new(),
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
                        sheet_instance_path: String::new(),
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
                    reference: None,
                    number: Some("1".to_string()),
                    electrical_type: Some("power_in".to_string()),
                    visible: true,
                    is_power_symbol: true,
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
                    reference: None,
                    number: Some("2".to_string()),
                    electrical_type: Some("power_in".to_string()),
                    visible: true,
                    is_power_symbol: true,
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
                chosen_driver_index: None,
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
                    start_is_wire_side: false,
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
                chosen_driver_index: None,
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
                chosen_driver_index: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(0, 0),
                points: Vec::new(),
                nodes: Vec::new(),
                label_links: Vec::new(),
                no_connect_points: Vec::new(),
                hier_sheet_pins: vec![ReducedHierSheetPinLink {
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                    at: PointKey(1, 0),
                    child_sheet_uuid: Some("child-sheet".to_string()),
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "/CHILD".to_string(),
                        local_name: "CHILD".to_string(),
                        full_local_name: "/CHILD".to_string(),
                        sheet_instance_path: String::new(),
                        members: Vec::new(),
                    },
                }],
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
                chosen_driver_index: None,
                drivers: vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: super::reduced_local_label_driver_priority(),
                    connection: ReducedProjectConnection {
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
                    identity: None,
                }],
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: "/child".to_string(),
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
                        name: "/CHILD".to_string(),
                        local_name: "CHILD".to_string(),
                        full_local_name: "/CHILD".to_string(),
                        sheet_instance_path: "/child".to_string(),
                        members: Vec::new(),
                    },
                }],
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

        let mut component =
            LiveReducedSubgraph::collect_propagation_component_handles(&handles[0], &handles)
                .into_iter()
                .map(|handle| handle.borrow().source_index)
                .collect::<Vec<_>>();
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
    fn collect_live_reduced_propagation_component_excludes_bus_parent_handles() {
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
                chosen_driver_index: None,
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
                    start_is_wire_side: false,
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
                chosen_driver_index: None,
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
        let mut component =
            LiveReducedSubgraph::collect_propagation_component_handles(&handles[1], &handles)
                .into_iter()
                .map(|handle| handle.borrow().source_index)
                .collect::<Vec<_>>();
        component.sort_unstable();

        assert_eq!(component, vec![1]);
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
                chosen_driver_index: None,
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
                chosen_driver_index: None,
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

        let component = vec![handles[0].clone(), handles[1].clone()];
        LiveReducedSubgraph::refresh_bus_link_members(&handles, &component);

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

        let component = vec![handles[0].clone(), handles[1].clone()];
        let mut stale_members = vec![Rc::new(RefCell::new(super::LiveProjectBusMember::from(
            ReducedBusMember {
                net_code: 0,
                name: "PWR".to_string(),
                local_name: "PWR".to_string(),
                full_local_name: "/PWR".to_string(),
                vector_index: Some(1),
                kind: ReducedBusMemberKind::Net,
                members: Vec::new(),
            },
        )))];
        LiveReducedSubgraph::replay_stale_bus_members(&handles, &component, &mut stale_members);

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
                chosen_driver_index: None,
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
                chosen_driver_index: None,
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

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        let component = live_subgraphs.iter().cloned().collect::<Vec<_>>();
        LiveReducedSubgraph::refresh_bus_parent_members(&live_subgraphs, &component);
        LiveReducedSubgraph::refresh_bus_link_members(&live_subgraphs, &component);
        apply_live_reduced_driver_connections_from_handles(&mut graph, &live_subgraphs);

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
                chosen_driver_index: None,
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
                chosen_driver_index: None,
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

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        let component = live_subgraphs.iter().cloned().collect::<Vec<_>>();
        LiveReducedSubgraph::refresh_bus_parent_members(&live_subgraphs, &component);
        LiveReducedSubgraph::refresh_bus_link_members(&live_subgraphs, &component);
        apply_live_reduced_driver_connections_from_handles(&mut graph, &live_subgraphs);

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
                chosen_driver_index: None,
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
                chosen_driver_index: None,
                drivers: Vec::new(),
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: String::new(),
                anchor: PointKey(1, 1),
                points: Vec::new(),
                nodes: Vec::new(),
                base_pins: vec![crate::connectivity::ReducedProjectBasePin {
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                    key: crate::connectivity::ReducedNetBasePinKey {
                        sheet_instance_path: String::new(),
                        symbol_uuid: Some("u1".to_string()),
                        at: PointKey(1, 1),
                        name: Some("IN".to_string()),
                        number: Some("1".to_string()),
                    },
                    reference: None,
                    number: Some("1".to_string()),
                    electrical_type: Some("input".to_string()),
                    visible: true,
                    is_power_symbol: false,
                    connection: ReducedProjectConnection {
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

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        let component = live_subgraphs.iter().cloned().collect::<Vec<_>>();
        let mut stale_members = Vec::new();
        let recurse_targets = LiveReducedSubgraph::refresh_bus_neighbor_drivers(
            &live_subgraphs,
            &component,
            &mut stale_members,
        );
        LiveReducedSubgraph::replay_stale_bus_members(
            &live_subgraphs,
            &component,
            &mut stale_members,
        );
        for handle in &live_subgraphs {
            LiveReducedSubgraph::refresh_post_propagation_item_connections(handle);
        }
        apply_live_reduced_driver_connections_from_handles(&mut graph, &live_subgraphs);

        assert_eq!(graph[1].name, "/SIG1");
        assert_eq!(graph[1].resolved_connection.full_local_name, "/SIG1");
        assert_eq!(graph[1].driver_connection.full_local_name, "/SIG1");
        assert_eq!(graph[1].base_pins[0].connection.full_local_name, "/SIG1");
        assert_eq!(recurse_targets.len(), 1);
        assert!(Rc::ptr_eq(&recurse_targets[0].0, &live_subgraphs[1]));
    }

    #[test]
    fn reduced_live_bus_neighbors_retry_secondary_driver_for_stale_member() {
        let mut bus_member = test_bus_member("SIG1", "SIG1", "/SIG1");
        bus_member.vector_index = Some(1);
        let stale_link_member = test_bus_member("ALT", "ALT", "/ALT");
        let sig_connection = test_net_connection("/SIG1", "SIG1", "/SIG1", "");
        let alt_connection = test_net_connection("/ALT", "ALT", "/ALT", "");
        let mut graph = vec![
            test_net_subgraph(
                1,
                test_bus_connection("/BUS", "BUS", "/BUS", "", vec![bus_member]),
                Vec::new(),
                "",
            ),
            test_net_subgraph(
                2,
                test_net_connection("/OLD", "OLD", "/OLD", ""),
                vec![
                    ReducedProjectStrongDriver {
                        kind: ReducedProjectDriverKind::Label,
                        priority: super::reduced_local_label_driver_priority(),
                        connection: alt_connection,
                        identity: None,
                    },
                    ReducedProjectStrongDriver {
                        kind: ReducedProjectDriverKind::Label,
                        priority: super::reduced_local_label_driver_priority(),
                        connection: sig_connection,
                        identity: None,
                    },
                ],
                "",
            ),
        ];
        graph[0].bus_neighbor_links = vec![ReducedProjectBusNeighborLink {
            member: stale_link_member.clone(),
            subgraph_index: 1,
        }];
        graph[1].bus_parent_links = vec![ReducedProjectBusNeighborLink {
            member: stale_link_member,
            subgraph_index: 0,
        }];
        graph[1].bus_parent_indexes = vec![0];

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        let component = live_subgraphs.iter().cloned().collect::<Vec<_>>();
        let mut stale_members = Vec::new();
        let recurse_targets = LiveReducedSubgraph::refresh_bus_neighbor_drivers(
            &live_subgraphs,
            &component,
            &mut stale_members,
        );
        for handle in &live_subgraphs {
            LiveReducedSubgraph::refresh_post_propagation_item_connections(handle);
        }
        apply_live_reduced_driver_connections_from_handles(&mut graph, &live_subgraphs);

        assert_eq!(graph[1].driver_connection.full_local_name, "/SIG1");
        assert_eq!(graph[1].name, "/SIG1");
        assert_eq!(recurse_targets.len(), 1);
        assert!(Rc::ptr_eq(&recurse_targets[0].0, &live_subgraphs[1]));
    }

    #[test]
    fn reduced_live_bus_neighbors_secondary_retry_skips_ordinary_pin_driver() {
        let member_a = test_bus_member("SIGA", "SIGA", "/SIGA");
        let member_b = test_bus_member("SIGB", "SIGB", "/SIGB");
        let stale_link_member = test_bus_member("ALT", "ALT", "/ALT");
        let pin_connection = test_net_connection("/SIGA", "SIGA", "/SIGA", "");
        let label_connection = test_net_connection("/SIGB", "SIGB", "/SIGB", "");
        let other_label_connection = test_net_connection("/OTHER", "OTHER", "/OTHER", "");
        let mut graph = vec![
            test_net_subgraph(
                1,
                test_bus_connection("/BUS", "BUS", "/BUS", "", vec![member_a, member_b]),
                Vec::new(),
                "",
            ),
            test_net_subgraph(
                2,
                test_net_connection("/OLD", "OLD", "/OLD", ""),
                vec![
                    ReducedProjectStrongDriver {
                        kind: ReducedProjectDriverKind::Pin,
                        priority: super::reduced_pin_driver_priority(),
                        connection: pin_connection,
                        identity: None,
                    },
                    ReducedProjectStrongDriver {
                        kind: ReducedProjectDriverKind::Label,
                        priority: super::reduced_local_label_driver_priority(),
                        connection: label_connection,
                        identity: None,
                    },
                    ReducedProjectStrongDriver {
                        kind: ReducedProjectDriverKind::Label,
                        priority: super::reduced_hierarchical_label_driver_priority(),
                        connection: other_label_connection,
                        identity: None,
                    },
                ],
                "",
            ),
        ];
        graph[0].bus_neighbor_links = vec![ReducedProjectBusNeighborLink {
            member: stale_link_member.clone(),
            subgraph_index: 1,
        }];
        graph[1].bus_parent_links = vec![ReducedProjectBusNeighborLink {
            member: stale_link_member,
            subgraph_index: 0,
        }];
        graph[1].bus_parent_indexes = vec![0];

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        let component = live_subgraphs.iter().cloned().collect::<Vec<_>>();
        let mut stale_members = Vec::new();
        let recurse_targets = LiveReducedSubgraph::refresh_bus_neighbor_drivers(
            &live_subgraphs,
            &component,
            &mut stale_members,
        );
        apply_live_reduced_driver_connections_from_handles(&mut graph, &live_subgraphs);

        assert_eq!(graph[1].driver_connection.full_local_name, "/SIGB");
        assert_eq!(recurse_targets.len(), 1);
        assert!(Rc::ptr_eq(&recurse_targets[0].0, &live_subgraphs[1]));
    }

    #[test]
    fn reduced_live_bus_neighbors_promote_secondary_retry_member() {
        let mut bus_member = test_bus_member("SIG1", "SIG1", "/SIG1");
        bus_member.vector_index = Some(1);
        let stale_link_member = test_bus_member("ALT", "ALT", "/ALT");
        let sig_connection = test_net_connection("/SIG1", "SIG1", "/SIG1", "");
        let alt_connection = test_net_connection("/ALT", "ALT", "/ALT", "");
        let power_connection = test_net_connection("/PWR", "PWR", "/PWR", "");
        let mut graph = vec![
            test_net_subgraph(
                1,
                test_bus_connection("/BUS", "BUS", "/BUS", "", vec![bus_member]),
                Vec::new(),
                "",
            ),
            test_net_subgraph(
                2,
                power_connection.clone(),
                vec![
                    ReducedProjectStrongDriver {
                        kind: ReducedProjectDriverKind::Label,
                        priority: super::reduced_global_label_driver_priority(),
                        connection: alt_connection,
                        identity: None,
                    },
                    ReducedProjectStrongDriver {
                        kind: ReducedProjectDriverKind::Label,
                        priority: super::reduced_global_label_driver_priority(),
                        connection: sig_connection,
                        identity: None,
                    },
                ],
                "",
            ),
        ];
        graph[0].bus_neighbor_links = vec![ReducedProjectBusNeighborLink {
            member: stale_link_member.clone(),
            subgraph_index: 1,
        }];
        graph[1].bus_parent_links = vec![ReducedProjectBusNeighborLink {
            member: stale_link_member,
            subgraph_index: 0,
        }];
        graph[1].bus_parent_indexes = vec![0];

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        let component = live_subgraphs.iter().cloned().collect::<Vec<_>>();
        let mut stale_members = Vec::new();
        let recurse_targets = LiveReducedSubgraph::refresh_bus_neighbor_drivers(
            &live_subgraphs,
            &component,
            &mut stale_members,
        );
        apply_live_reduced_driver_connections_from_handles(&mut graph, &live_subgraphs);

        assert!(recurse_targets.is_empty());
        assert_eq!(
            graph[0].driver_connection.members[0].full_local_name,
            "/PWR"
        );
        assert_eq!(stale_members.len(), 1);
        assert_eq!(stale_members[0].borrow().full_local_name, "/PWR");
    }

    #[test]
    fn live_group_bus_member_match_uses_local_name_inside_nested_vectors() {
        let group = LiveProjectConnection {
            net_code: 0,
            connection_type: ReducedProjectConnectionType::BusGroup,
            name: "GROUP{A[0], B[0]}".to_string(),
            local_name: "GROUP".to_string(),
            full_local_name: "/GROUP".to_string(),
            sheet_instance_path: String::new(),
            members: vec![
                Rc::new(RefCell::new(LiveProjectBusMember {
                    net_code: 0,
                    name: "A[0]".to_string(),
                    local_name: "A".to_string(),
                    full_local_name: "/A".to_string(),
                    vector_index: None,
                    kind: ReducedBusMemberKind::Bus,
                    members: vec![Rc::new(RefCell::new(LiveProjectBusMember {
                        net_code: 0,
                        name: "A0".to_string(),
                        local_name: "A0".to_string(),
                        full_local_name: "/A0".to_string(),
                        vector_index: Some(0),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }))],
                })),
                Rc::new(RefCell::new(LiveProjectBusMember {
                    net_code: 0,
                    name: "B[0]".to_string(),
                    local_name: "B".to_string(),
                    full_local_name: "/B".to_string(),
                    vector_index: None,
                    kind: ReducedBusMemberKind::Bus,
                    members: vec![Rc::new(RefCell::new(LiveProjectBusMember {
                        net_code: 0,
                        name: "B0".to_string(),
                        local_name: "B0".to_string(),
                        full_local_name: "/B0".to_string(),
                        vector_index: Some(0),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }))],
                })),
            ],
        };
        let search = LiveProjectBusMember {
            net_code: 0,
            name: "B0".to_string(),
            local_name: "B0".to_string(),
            full_local_name: "/B0".to_string(),
            vector_index: Some(0),
            kind: ReducedBusMemberKind::Net,
            members: Vec::new(),
        };

        let matched = group
            .find_member_live(&search)
            .expect("group member should match by local name");

        assert_eq!(matched.borrow().local_name, "B0");
    }

    #[test]
    fn live_group_bus_connection_match_does_not_recurse_past_nested_vector_members() {
        let group = LiveProjectConnection {
            net_code: 0,
            connection_type: ReducedProjectConnectionType::BusGroup,
            name: "GROUP{A[0], B[0]}".to_string(),
            local_name: "GROUP".to_string(),
            full_local_name: "/GROUP".to_string(),
            sheet_instance_path: String::new(),
            members: vec![
                Rc::new(RefCell::new(LiveProjectBusMember {
                    net_code: 0,
                    name: "A[0]".to_string(),
                    local_name: "A".to_string(),
                    full_local_name: "/A".to_string(),
                    vector_index: None,
                    kind: ReducedBusMemberKind::Bus,
                    members: vec![Rc::new(RefCell::new(LiveProjectBusMember {
                        net_code: 0,
                        name: "INNER[0]".to_string(),
                        local_name: "INNER".to_string(),
                        full_local_name: "/INNER".to_string(),
                        vector_index: None,
                        kind: ReducedBusMemberKind::Bus,
                        members: vec![Rc::new(RefCell::new(LiveProjectBusMember {
                            net_code: 0,
                            name: "B0".to_string(),
                            local_name: "B0".to_string(),
                            full_local_name: "/wrong/B0".to_string(),
                            vector_index: Some(0),
                            kind: ReducedBusMemberKind::Net,
                            members: Vec::new(),
                        }))],
                    }))],
                })),
                Rc::new(RefCell::new(LiveProjectBusMember {
                    net_code: 0,
                    name: "B[0]".to_string(),
                    local_name: "B".to_string(),
                    full_local_name: "/B".to_string(),
                    vector_index: None,
                    kind: ReducedBusMemberKind::Bus,
                    members: vec![Rc::new(RefCell::new(LiveProjectBusMember {
                        net_code: 0,
                        name: "B0".to_string(),
                        local_name: "B0".to_string(),
                        full_local_name: "/B0".to_string(),
                        vector_index: Some(0),
                        kind: ReducedBusMemberKind::Net,
                        members: Vec::new(),
                    }))],
                })),
            ],
        };
        let search = LiveProjectConnection {
            net_code: 0,
            connection_type: ReducedProjectConnectionType::Net,
            name: "/B0".to_string(),
            local_name: "B0".to_string(),
            full_local_name: "/B0".to_string(),
            sheet_instance_path: String::new(),
            members: Vec::new(),
        };

        let matched = group
            .find_member_for_connection(&search)
            .expect("group member should match direct vector member by local name");

        assert_eq!(matched.borrow().full_local_name, "/B0");
    }

    #[test]
    fn live_vector_bus_connection_match_uses_vector_index_not_local_name() {
        let bus = LiveProjectConnection {
            net_code: 0,
            connection_type: ReducedProjectConnectionType::Bus,
            name: "A[0..1]".to_string(),
            local_name: "A".to_string(),
            full_local_name: "/A".to_string(),
            sheet_instance_path: String::new(),
            members: vec![
                Rc::new(RefCell::new(LiveProjectBusMember {
                    net_code: 0,
                    name: "A0".to_string(),
                    local_name: "A0".to_string(),
                    full_local_name: "/A0".to_string(),
                    vector_index: Some(0),
                    kind: ReducedBusMemberKind::Net,
                    members: Vec::new(),
                })),
                Rc::new(RefCell::new(LiveProjectBusMember {
                    net_code: 0,
                    name: "A1".to_string(),
                    local_name: "A1".to_string(),
                    full_local_name: "/A1".to_string(),
                    vector_index: Some(1),
                    kind: ReducedBusMemberKind::Net,
                    members: Vec::new(),
                })),
            ],
        };
        let search = LiveProjectConnection {
            net_code: 0,
            connection_type: ReducedProjectConnectionType::Net,
            name: "/B0".to_string(),
            local_name: "B0".to_string(),
            full_local_name: "/B0".to_string(),
            sheet_instance_path: String::new(),
            members: Vec::new(),
        };

        let matched = bus
            .find_member_for_connection(&search)
            .expect("vector bus member should match by vector index");

        assert_eq!(matched.borrow().full_local_name, "/A0");
    }

    #[test]
    fn reduced_live_bus_neighbor_recursion_refreshes_upgraded_member() {
        let member = test_bus_member("SIG1", "SIG1", "/SIG1");
        let sig_connection = test_net_connection("/SIG1", "SIG1", "/SIG1", "");
        let power_connection = test_net_connection("/PWR", "PWR", "/PWR", "/child");
        let mut graph = vec![
            test_net_subgraph(
                1,
                test_bus_connection("/BUS", "BUS", "/BUS", "", vec![member.clone()]),
                Vec::new(),
                "",
            ),
            test_net_subgraph(
                2,
                test_net_connection("/OLD", "OLD", "/OLD", ""),
                Vec::new(),
                "",
            ),
            test_net_subgraph(
                3,
                power_connection.clone(),
                vec![test_power_driver(power_connection.clone())],
                "/child",
            ),
        ];
        graph[0].bus_neighbor_links = vec![ReducedProjectBusNeighborLink {
            member: member.clone(),
            subgraph_index: 1,
        }];
        graph[1].bus_parent_links = vec![ReducedProjectBusNeighborLink {
            member,
            subgraph_index: 0,
        }];
        graph[1].bus_parent_indexes = vec![0];
        graph[1].hier_child_indexes = vec![2];
        graph[1].hier_sheet_pins = vec![ReducedHierSheetPinLink {
            schematic_path: std::path::PathBuf::from("root.kicad_sch"),
            at: PointKey(0, 0),
            child_sheet_uuid: Some("child".to_string()),
            connection: sig_connection.clone(),
        }];
        graph[2].hier_parent_index = Some(1);
        graph[2].hier_ports = vec![ReducedHierPortLink {
            schematic_path: std::path::PathBuf::from("child.kicad_sch"),
            at: PointKey(0, 0),
            connection: sig_connection,
        }];

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        let global_subgraphs =
            LiveReducedSubgraph::collect_global_subgraph_handles(&live_subgraphs);
        let mut visiting = std::collections::BTreeSet::new();
        let mut stale_members = Vec::new();
        LiveReducedSubgraph::propagate_neighbors_from_selected_start(
            &live_subgraphs[0],
            &live_subgraphs,
            &global_subgraphs,
            false,
            &mut visiting,
            &mut stale_members,
            false,
        );
        apply_live_reduced_driver_connections_from_handles(&mut graph, &live_subgraphs);

        assert_eq!(graph[1].driver_connection.full_local_name, "/PWR");
        assert_eq!(
            graph[0].driver_connection.members[0].full_local_name,
            "/PWR"
        );
        assert_eq!(stale_members.len(), 1);
        assert_eq!(stale_members[0].borrow().full_local_name, "/PWR");
    }

    #[test]
    fn reduced_live_bus_neighbors_skip_non_net_neighbor_driver() {
        let member = test_bus_member("SIG1", "SIG1", "/SIG1");
        let mut graph = vec![
            test_net_subgraph(
                1,
                test_bus_connection("/BUS", "BUS", "/BUS", "", vec![member.clone()]),
                Vec::new(),
                "",
            ),
            test_net_subgraph(
                2,
                test_bus_connection("/OTHER", "OTHER", "/OTHER", "", vec![member.clone()]),
                Vec::new(),
                "",
            ),
        ];
        graph[0].bus_neighbor_links = vec![ReducedProjectBusNeighborLink {
            member: member.clone(),
            subgraph_index: 1,
        }];
        graph[1].bus_parent_links = vec![ReducedProjectBusNeighborLink {
            member,
            subgraph_index: 0,
        }];
        graph[1].bus_parent_indexes = vec![0];

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        let component = live_subgraphs.iter().cloned().collect::<Vec<_>>();
        let mut stale_members = Vec::new();
        LiveReducedSubgraph::refresh_bus_neighbor_drivers(
            &live_subgraphs,
            &component,
            &mut stale_members,
        );
        apply_live_reduced_driver_connections_from_handles(&mut graph, &live_subgraphs);

        assert_eq!(
            graph[1].driver_connection.connection_type,
            ReducedProjectConnectionType::Bus
        );
        assert_eq!(graph[1].driver_connection.full_local_name, "/OTHER");
    }

    #[test]
    fn reduced_live_bus_neighbors_sort_by_resolved_member_name() {
        let first_by_display = test_bus_member("A", "A", "/Z");
        let first_by_resolved = test_bus_member("Z", "Z", "/A");
        let mut graph = vec![
            test_net_subgraph(
                1,
                test_bus_connection(
                    "/BUS",
                    "BUS",
                    "/BUS",
                    "",
                    vec![first_by_display.clone(), first_by_resolved.clone()],
                ),
                Vec::new(),
                "",
            ),
            test_net_subgraph(
                2,
                test_net_connection("/OLD", "OLD", "/OLD", ""),
                Vec::new(),
                "",
            ),
        ];
        graph[0].bus_neighbor_links = vec![
            ReducedProjectBusNeighborLink {
                member: first_by_display.clone(),
                subgraph_index: 1,
            },
            ReducedProjectBusNeighborLink {
                member: first_by_resolved.clone(),
                subgraph_index: 1,
            },
        ];
        graph[1].bus_parent_links = vec![
            ReducedProjectBusNeighborLink {
                member: first_by_display,
                subgraph_index: 0,
            },
            ReducedProjectBusNeighborLink {
                member: first_by_resolved,
                subgraph_index: 0,
            },
        ];
        graph[1].bus_parent_indexes = vec![0];

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        let component = live_subgraphs.iter().cloned().collect::<Vec<_>>();
        let mut stale_members = Vec::new();
        LiveReducedSubgraph::refresh_bus_neighbor_drivers(
            &live_subgraphs,
            &component,
            &mut stale_members,
        );
        apply_live_reduced_driver_connections_from_handles(&mut graph, &live_subgraphs);

        assert_eq!(graph[1].driver_connection.full_local_name, "/Z");
    }

    #[test]
    fn reduced_live_bus_neighbors_preserve_same_parent_member_by_neighbor_name() {
        let member_a = test_bus_member("SIGA", "SIGA", "/SIGA");
        let member_b = test_bus_member("SIGB", "SIGB", "/SIGB");
        let mut graph = vec![
            test_net_subgraph(
                1,
                test_bus_connection(
                    "/BUS",
                    "BUS",
                    "/BUS",
                    "",
                    vec![member_a.clone(), member_b.clone()],
                ),
                Vec::new(),
                "",
            ),
            test_net_subgraph(
                2,
                test_net_connection("/SIGA", "RENAMED", "/SIGA", ""),
                Vec::new(),
                "/child",
            ),
        ];
        graph[0].bus_neighbor_links = vec![ReducedProjectBusNeighborLink {
            member: member_b.clone(),
            subgraph_index: 1,
        }];
        graph[1].bus_parent_links = vec![ReducedProjectBusNeighborLink {
            member: member_b,
            subgraph_index: 0,
        }];
        graph[1].bus_parent_indexes = vec![0];

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        let component = live_subgraphs.iter().cloned().collect::<Vec<_>>();
        let mut stale_members = Vec::new();
        let recurse_targets = LiveReducedSubgraph::refresh_bus_neighbor_drivers(
            &live_subgraphs,
            &component,
            &mut stale_members,
        );
        apply_live_reduced_driver_connections_from_handles(&mut graph, &live_subgraphs);

        assert!(recurse_targets.is_empty());
        assert_eq!(graph[1].driver_connection.full_local_name, "/SIGA");
        assert_eq!(graph[1].driver_connection.local_name, "RENAMED");
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
                chosen_driver_index: None,
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
                chosen_driver_index: None,
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

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        let component = live_subgraphs.iter().cloned().collect::<Vec<_>>();
        let mut stale_members = Vec::new();
        LiveReducedSubgraph::refresh_bus_neighbor_drivers(
            &live_subgraphs,
            &component,
            &mut stale_members,
        );
        LiveReducedSubgraph::replay_stale_bus_members(
            &live_subgraphs,
            &component,
            &mut stale_members,
        );
        apply_live_reduced_driver_connections_from_handles(&mut graph, &live_subgraphs);

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
    fn reduced_live_bus_neighbors_promote_global_neighbor_without_dirtying_parent_bus() {
        let graph = vec![
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
                chosen_driver_index: None,
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
                chosen_driver_index: None,
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

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        let component = live_subgraphs.iter().cloned().collect::<Vec<_>>();
        live_subgraphs[0].borrow_mut().dirty = false;
        let mut stale_members = Vec::new();
        LiveReducedSubgraph::refresh_bus_neighbor_drivers(
            &live_subgraphs,
            &component,
            &mut stale_members,
        );

        assert!(!live_subgraphs[0].borrow().dirty);
        assert_eq!(stale_members.len(), 1);
        assert_eq!(stale_members[0].borrow().full_local_name, "/PWR");
    }

    #[test]
    fn replay_reduced_live_stale_bus_members_refreshes_neighbor_nets_immediately() {
        let graph = vec![
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
                chosen_driver_index: None,
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
                chosen_driver_index: None,
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

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        let component = live_subgraphs.iter().cloned().collect::<Vec<_>>();
        let mut stale_members = vec![Rc::new(RefCell::new(super::LiveProjectBusMember::from(
            ReducedBusMember {
                net_code: 0,
                name: "PWR".to_string(),
                local_name: "PWR".to_string(),
                full_local_name: "/PWR".to_string(),
                vector_index: Some(1),
                kind: ReducedBusMemberKind::Net,
                members: Vec::new(),
            },
        )))];

        LiveReducedSubgraph::replay_stale_bus_members(
            &live_subgraphs,
            &component,
            &mut stale_members,
        );

        assert_eq!(
            live_subgraphs[1].borrow().driver_connection.borrow().name,
            "/PWR"
        );
        assert_eq!(
            live_subgraphs[1]
                .borrow()
                .driver_connection
                .borrow()
                .full_local_name,
            "/PWR"
        );
    }

    #[test]
    fn replay_reduced_live_stale_bus_members_refreshes_same_member_neighbors() {
        let member = test_bus_member("PWR", "PWR", "/PWR");
        let mut graph = vec![
            test_net_subgraph(
                1,
                test_bus_connection("/BUS", "BUS", "/BUS", "", vec![member.clone()]),
                Vec::new(),
                "",
            ),
            test_net_subgraph(
                2,
                test_net_connection("/OLD", "OLD", "/OLD", ""),
                Vec::new(),
                "",
            ),
        ];
        graph[0].bus_neighbor_links = vec![ReducedProjectBusNeighborLink {
            member: member.clone(),
            subgraph_index: 1,
        }];
        graph[1].bus_parent_links = vec![ReducedProjectBusNeighborLink {
            member,
            subgraph_index: 0,
        }];
        graph[1].bus_parent_indexes = vec![0];

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        let component = live_subgraphs.iter().cloned().collect::<Vec<_>>();
        let stale_member = live_subgraphs[0]
            .borrow()
            .driver_connection
            .borrow()
            .members[0]
            .clone();
        let mut stale_members = vec![stale_member];

        LiveReducedSubgraph::replay_stale_bus_members(
            &live_subgraphs,
            &component,
            &mut stale_members,
        );
        apply_live_reduced_driver_connections_from_handles(&mut graph, &live_subgraphs);

        assert_eq!(graph[1].driver_connection.full_local_name, "/PWR");
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
                chosen_driver_index: None,
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
                chosen_driver_index: None,
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

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        live_subgraphs[0].borrow_mut().dirty = false;
        let component = live_subgraphs.iter().cloned().collect::<Vec<_>>();
        LiveReducedSubgraph::refresh_bus_parent_members(&live_subgraphs, &component);
        apply_live_reduced_driver_connections_from_handles(&mut graph, &live_subgraphs);

        assert_eq!(
            graph[0].resolved_connection.members[0].full_local_name,
            "/PWR"
        );
        assert_eq!(
            graph[0].driver_connection.members[0].full_local_name,
            "/PWR"
        );
        assert!(!live_subgraphs[0].borrow().dirty);
    }

    #[test]
    fn reduced_live_selected_clean_bus_root_still_propagates_neighbors() {
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
                chosen_driver_index: None,
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
                chosen_driver_index: None,
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

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        live_subgraphs[0].borrow_mut().dirty = false;
        live_subgraphs[1].borrow_mut().dirty = false;
        let global_subgraphs =
            LiveReducedSubgraph::collect_global_subgraph_handles(&live_subgraphs);

        let mut visiting = std::collections::BTreeSet::new();
        let mut stale_members = Vec::new();
        LiveReducedSubgraph::propagate_neighbors_from_selected_start(
            &live_subgraphs[0],
            &live_subgraphs,
            &global_subgraphs,
            false,
            &mut visiting,
            &mut stale_members,
            true,
        );
        apply_live_reduced_driver_connections_from_handles(&mut graph, &live_subgraphs);

        assert_eq!(graph[1].name, "/SIG1");
        assert_eq!(graph[1].resolved_connection.full_local_name, "/SIG1");
        assert_eq!(graph[1].driver_connection.full_local_name, "/SIG1");
        assert!(!live_subgraphs[0].borrow().dirty);
        assert!(!live_subgraphs[1].borrow().dirty);
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
                chosen_driver_index: None,
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
                chosen_driver_index: None,
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

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        let component = live_subgraphs.iter().cloned().collect::<Vec<_>>();
        LiveReducedSubgraph::refresh_bus_parent_members(&live_subgraphs, &component);
        apply_live_reduced_driver_connections_from_handles(&mut graph, &live_subgraphs);

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
                chosen_driver_index: None,
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
                chosen_driver_index: None,
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
                chosen_driver_index: None,
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
                chosen_driver_index: None,
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
                        at: PointKey(0, 0),
                        name: Some("IN".to_string()),
                        number: Some("1".to_string()),
                    },
                    reference: None,
                    number: Some("1".to_string()),
                    electrical_type: Some("input".to_string()),
                    visible: true,
                    is_power_symbol: false,
                    connection: ReducedProjectConnection {
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
                    preserve_local_name_on_refresh: false,
                }],
                label_links: vec![ReducedLabelLink {
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                    at: PointKey(0, 0),
                    kind: LabelKind::Local,
                    dangling: false,
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

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        LiveReducedSubgraph::refresh_multiple_bus_parent_names(&live_subgraphs);
        for handle in &live_subgraphs {
            LiveReducedSubgraph::refresh_post_propagation_item_connections(handle);
        }
        apply_live_reduced_driver_connections_from_handles(&mut graph, &live_subgraphs);

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
        assert_eq!(
            graph[3].base_pins[0].connection.full_local_name,
            "/RENAMED1"
        );
    }

    #[test]
    fn reduced_live_bus_link_rematch_preserves_unmatched_parent_for_multi_parent_rename() {
        let stale_member = test_bus_member("STALE", "STALE", "/STALE");
        let old_member = test_bus_member("OLD", "OLD", "/OLD");
        let mut graph = vec![
            test_net_subgraph(
                1,
                test_bus_connection(
                    "/BUS_A",
                    "BUS_A",
                    "/BUS_A",
                    "",
                    vec![test_bus_member("UNRELATED", "UNRELATED", "/UNRELATED")],
                ),
                Vec::new(),
                "",
            ),
            test_net_subgraph(
                2,
                test_bus_connection("/BUS_B", "BUS_B", "/BUS_B", "", vec![old_member.clone()]),
                Vec::new(),
                "",
            ),
            test_net_subgraph(
                3,
                test_net_connection("/PWR", "PWR", "/PWR", ""),
                Vec::new(),
                "",
            ),
        ];
        graph[0].bus_neighbor_links = vec![ReducedProjectBusNeighborLink {
            member: stale_member.clone(),
            subgraph_index: 2,
        }];
        graph[1].bus_neighbor_links = vec![ReducedProjectBusNeighborLink {
            member: old_member.clone(),
            subgraph_index: 2,
        }];
        graph[2].bus_parent_links = vec![
            ReducedProjectBusNeighborLink {
                member: stale_member,
                subgraph_index: 0,
            },
            ReducedProjectBusNeighborLink {
                member: old_member,
                subgraph_index: 1,
            },
        ];
        graph[2].bus_parent_indexes = vec![0, 1];

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        let component = live_subgraphs.iter().cloned().collect::<Vec<_>>();
        LiveReducedSubgraph::refresh_bus_link_members(&live_subgraphs, &component);
        LiveReducedSubgraph::refresh_multiple_bus_parent_names(&live_subgraphs);
        apply_live_reduced_driver_connections_from_handles(&mut graph, &live_subgraphs);

        assert_eq!(graph[2].bus_parent_links.len(), 2);
        assert_eq!(
            graph[0].driver_connection.members[0].full_local_name,
            "/UNRELATED"
        );
        assert_eq!(
            graph[1].driver_connection.members[0].full_local_name,
            "/PWR"
        );
    }

    #[test]
    fn reduced_live_multiple_bus_parent_names_skip_same_resolved_name_different_sheet_path() {
        let mut graph = vec![
            test_net_subgraph(
                1,
                test_bus_connection(
                    "/BUS_A",
                    "BUS_A",
                    "/BUS_A",
                    "",
                    vec![test_bus_member("SIG", "SIG", "/SIG")],
                ),
                Vec::new(),
                "",
            ),
            test_net_subgraph(
                2,
                test_bus_connection(
                    "/BUS_B",
                    "BUS_B",
                    "/BUS_B",
                    "",
                    vec![test_bus_member("SIG", "SIG", "/SIG")],
                ),
                Vec::new(),
                "",
            ),
            test_net_subgraph(
                3,
                test_net_connection("SIG", "SIG", "/child/SIG", "/child"),
                Vec::new(),
                "/child",
            ),
        ];
        graph[2].bus_parent_links = vec![
            ReducedProjectBusNeighborLink {
                member: test_bus_member("SIG", "SIG", "/SIG"),
                subgraph_index: 0,
            },
            ReducedProjectBusNeighborLink {
                member: test_bus_member("SIG", "SIG", "/SIG"),
                subgraph_index: 1,
            },
        ];
        graph[2].bus_parent_indexes = vec![0, 1];

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        LiveReducedSubgraph::refresh_multiple_bus_parent_names(&live_subgraphs);
        apply_live_reduced_driver_connections_from_handles(&mut graph, &live_subgraphs);

        assert_eq!(graph[0].driver_connection.members[0].name, "SIG");
        assert_eq!(
            graph[0].driver_connection.members[0].full_local_name,
            "/SIG"
        );
        assert_eq!(graph[1].driver_connection.members[0].name, "SIG");
        assert_eq!(
            graph[1].driver_connection.members[0].full_local_name,
            "/SIG"
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
                chosen_driver_index: None,
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
                chosen_driver_index: None,
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
    fn reduced_hierarchy_descent_skips_child_without_strong_driver() {
        let mut graph = vec![
            test_net_subgraph(
                1,
                test_net_connection("/ROOT_SIG", "ROOT_SIG", "/ROOT_SIG", ""),
                vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: super::reduced_local_label_driver_priority(),
                    connection: test_net_connection("/ROOT_SIG", "ROOT_SIG", "/ROOT_SIG", ""),
                    identity: None,
                }],
                "",
            ),
            test_net_subgraph(
                2,
                test_net_connection("/CHILD_SIG", "CHILD_SIG", "/child/CHILD_SIG", "/child"),
                Vec::new(),
                "/child",
            ),
        ];
        graph[0].hier_child_indexes = vec![1];
        graph[0].hier_sheet_pins = vec![ReducedHierSheetPinLink {
            schematic_path: std::path::PathBuf::from("root.kicad_sch"),
            at: PointKey(0, 0),
            child_sheet_uuid: Some("child-sheet".to_string()),
            connection: test_net_connection("/ROOT_SIG", "ROOT_SIG", "/ROOT_SIG", ""),
        }];
        graph[1].hier_parent_index = Some(0);
        graph[1].hier_ports = vec![ReducedHierPortLink {
            schematic_path: std::path::PathBuf::from("child.kicad_sch"),
            at: PointKey(0, 0),
            connection: test_net_connection(
                "/child/ROOT_SIG",
                "ROOT_SIG",
                "/child/ROOT_SIG",
                "/child",
            ),
        }];

        let handles = build_live_reduced_subgraph_handles(&graph);
        let mut component =
            LiveReducedSubgraph::collect_propagation_component_handles(&handles[0], &handles)
                .into_iter()
                .map(|handle| handle.borrow().source_index)
                .collect::<Vec<_>>();
        component.sort_unstable();

        assert_eq!(component, vec![0]);
    }

    #[test]
    fn reduced_hierarchy_descent_requires_child_hier_ports() {
        let mut graph = vec![
            test_net_subgraph(
                1,
                test_net_connection("/ROOT_SIG", "ROOT_SIG", "/ROOT_SIG", ""),
                vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: super::reduced_local_label_driver_priority(),
                    connection: test_net_connection("/ROOT_SIG", "ROOT_SIG", "/ROOT_SIG", ""),
                    identity: None,
                }],
                "",
            ),
            test_net_subgraph(
                2,
                test_net_connection("/child/ROOT_SIG", "ROOT_SIG", "/child/ROOT_SIG", "/child"),
                vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: super::reduced_hierarchical_label_driver_priority(),
                    connection: test_net_connection(
                        "/child/ROOT_SIG",
                        "ROOT_SIG",
                        "/child/ROOT_SIG",
                        "/child",
                    ),
                    identity: None,
                }],
                "/child",
            ),
        ];
        graph[0].hier_child_indexes = vec![1];
        graph[0].hier_sheet_pins = vec![ReducedHierSheetPinLink {
            schematic_path: std::path::PathBuf::from("root.kicad_sch"),
            at: PointKey(0, 0),
            child_sheet_uuid: Some("child-sheet".to_string()),
            connection: test_net_connection("/ROOT_SIG", "ROOT_SIG", "/ROOT_SIG", ""),
        }];
        graph[1].hier_parent_index = Some(0);

        let handles = build_live_reduced_subgraph_handles(&graph);
        let mut component =
            LiveReducedSubgraph::collect_propagation_component_handles(&handles[0], &handles)
                .into_iter()
                .map(|handle| handle.borrow().source_index)
                .collect::<Vec<_>>();
        component.sort_unstable();

        assert_eq!(component, vec![0]);
    }

    #[test]
    fn reduced_hierarchy_visit_rechecks_current_driver_names() {
        let mut graph = vec![
            test_net_subgraph(
                1,
                test_net_connection("/ROOT_SIG", "ROOT_SIG", "/ROOT_SIG", ""),
                vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: super::reduced_local_label_driver_priority(),
                    connection: test_net_connection("/ROOT_SIG", "ROOT_SIG", "/ROOT_SIG", ""),
                    identity: None,
                }],
                "",
            ),
            test_net_subgraph(
                2,
                test_net_connection("/child/ROOT_SIG", "ROOT_SIG", "/child/ROOT_SIG", "/child"),
                vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: super::reduced_hierarchical_label_driver_priority(),
                    connection: test_net_connection(
                        "/child/ROOT_SIG",
                        "ROOT_SIG",
                        "/child/ROOT_SIG",
                        "/child",
                    ),
                    identity: None,
                }],
                "/child",
            ),
        ];
        graph[0].hier_child_indexes = vec![1];
        graph[0].hier_sheet_pins = vec![ReducedHierSheetPinLink {
            schematic_path: std::path::PathBuf::from("root.kicad_sch"),
            at: PointKey(0, 0),
            child_sheet_uuid: Some("child-sheet".to_string()),
            connection: test_net_connection("/ROOT_SIG", "ROOT_SIG", "/ROOT_SIG", ""),
        }];
        graph[1].hier_parent_index = Some(0);
        graph[1].hier_ports = vec![ReducedHierPortLink {
            schematic_path: std::path::PathBuf::from("child.kicad_sch"),
            at: PointKey(0, 0),
            connection: test_net_connection(
                "/child/ROOT_SIG",
                "ROOT_SIG",
                "/child/ROOT_SIG",
                "/child",
            ),
        }];

        let handles = build_live_reduced_subgraph_handles(&graph);
        handles[1].borrow().hier_ports[0]
            .borrow()
            .connection
            .borrow_mut()
            .local_name = "OTHER".to_string();

        let parent_component =
            LiveReducedSubgraph::collect_propagation_component_handles(&handles[0], &handles)
                .into_iter()
                .map(|handle| handle.borrow().source_index)
                .collect::<Vec<_>>();
        let child_component =
            LiveReducedSubgraph::collect_propagation_component_handles(&handles[1], &handles)
                .into_iter()
                .map(|handle| handle.borrow().source_index)
                .collect::<Vec<_>>();

        assert_eq!(parent_component, vec![0]);
        assert_eq!(child_component, vec![1]);
    }

    #[test]
    fn reduced_hierarchy_parent_visit_requires_matching_connection_type() {
        let mut graph = vec![
            test_net_subgraph(
                1,
                test_bus_connection(
                    "/BUS",
                    "BUS",
                    "/BUS",
                    "",
                    vec![test_bus_member("SIG0", "SIG0", "/SIG0")],
                ),
                Vec::new(),
                "",
            ),
            test_net_subgraph(
                2,
                test_net_connection("/child/SIG0", "SIG0", "/child/SIG0", "/child"),
                vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: super::reduced_hierarchical_label_driver_priority(),
                    connection: test_net_connection("/child/SIG0", "SIG0", "/child/SIG0", "/child"),
                    identity: None,
                }],
                "/child",
            ),
        ];
        graph[0].hier_child_indexes = vec![1];
        graph[0].hier_sheet_pins = vec![ReducedHierSheetPinLink {
            schematic_path: std::path::PathBuf::from("root.kicad_sch"),
            at: PointKey(0, 0),
            child_sheet_uuid: Some("child-sheet".to_string()),
            connection: test_net_connection("/SIG0", "SIG0", "/SIG0", ""),
        }];
        graph[1].hier_parent_index = Some(0);
        graph[1].hier_ports = vec![ReducedHierPortLink {
            schematic_path: std::path::PathBuf::from("child.kicad_sch"),
            at: PointKey(0, 0),
            connection: test_net_connection("/child/SIG0", "SIG0", "/child/SIG0", "/child"),
        }];

        let handles = build_live_reduced_subgraph_handles(&graph);
        let mut component =
            LiveReducedSubgraph::collect_propagation_component_handles(&handles[1], &handles)
                .into_iter()
                .map(|handle| handle.borrow().source_index)
                .collect::<Vec<_>>();
        component.sort_unstable();

        assert_eq!(component, vec![1]);
    }

    #[test]
    fn reduced_hierarchy_parent_visit_requires_parent_hier_pins() {
        let mut graph = vec![
            test_net_subgraph(
                1,
                test_net_connection("/ROOT_SIG", "ROOT_SIG", "/ROOT_SIG", ""),
                vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: super::reduced_local_label_driver_priority(),
                    connection: test_net_connection("/ROOT_SIG", "ROOT_SIG", "/ROOT_SIG", ""),
                    identity: None,
                }],
                "",
            ),
            test_net_subgraph(
                2,
                test_net_connection("/child/ROOT_SIG", "ROOT_SIG", "/child/ROOT_SIG", "/child"),
                vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: super::reduced_hierarchical_label_driver_priority(),
                    connection: test_net_connection(
                        "/child/ROOT_SIG",
                        "ROOT_SIG",
                        "/child/ROOT_SIG",
                        "/child",
                    ),
                    identity: None,
                }],
                "/child",
            ),
        ];
        graph[0].hier_child_indexes = vec![1];
        graph[1].hier_parent_index = Some(0);
        graph[1].hier_ports = vec![ReducedHierPortLink {
            schematic_path: std::path::PathBuf::from("child.kicad_sch"),
            at: PointKey(0, 0),
            connection: test_net_connection(
                "/child/ROOT_SIG",
                "ROOT_SIG",
                "/child/ROOT_SIG",
                "/child",
            ),
        }];

        let handles = build_live_reduced_subgraph_handles(&graph);
        let mut component =
            LiveReducedSubgraph::collect_propagation_component_handles(&handles[1], &handles)
                .into_iter()
                .map(|handle| handle.borrow().source_index)
                .collect::<Vec<_>>();
        component.sort_unstable();

        assert_eq!(component, vec![1]);
    }

    #[test]
    fn reduced_hierarchy_invalid_parent_does_not_skip_child_descent() {
        let mut graph = vec![
            test_net_subgraph(
                1,
                test_net_connection("/PARENT_SIG", "PARENT_SIG", "/PARENT_SIG", ""),
                vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: super::reduced_local_label_driver_priority(),
                    connection: test_net_connection("/PARENT_SIG", "PARENT_SIG", "/PARENT_SIG", ""),
                    identity: None,
                }],
                "",
            ),
            test_net_subgraph(
                2,
                test_net_connection("/mid/SIG", "SIG", "/mid/SIG", "/mid"),
                vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: super::reduced_hierarchical_label_driver_priority(),
                    connection: test_net_connection("/mid/SIG", "SIG", "/mid/SIG", "/mid"),
                    identity: None,
                }],
                "/mid",
            ),
            test_net_subgraph(
                3,
                test_net_connection("/mid/child/SIG", "SIG", "/mid/child/SIG", "/mid/child"),
                vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: super::reduced_hierarchical_label_driver_priority(),
                    connection: test_net_connection(
                        "/mid/child/SIG",
                        "SIG",
                        "/mid/child/SIG",
                        "/mid/child",
                    ),
                    identity: None,
                }],
                "/mid/child",
            ),
        ];
        graph[1].hier_parent_index = Some(0);
        graph[1].hier_ports = vec![ReducedHierPortLink {
            schematic_path: std::path::PathBuf::from("mid.kicad_sch"),
            at: PointKey(0, 0),
            connection: test_net_connection(
                "/mid/PARENT_SIG",
                "PARENT_SIG",
                "/mid/PARENT_SIG",
                "/mid",
            ),
        }];
        graph[1].hier_child_indexes = vec![2];
        graph[1].hier_sheet_pins = vec![ReducedHierSheetPinLink {
            schematic_path: std::path::PathBuf::from("mid.kicad_sch"),
            at: PointKey(10, 0),
            child_sheet_uuid: Some("child-sheet".to_string()),
            connection: test_net_connection("/mid/SIG", "SIG", "/mid/SIG", "/mid"),
        }];
        graph[2].hier_parent_index = Some(1);
        graph[2].hier_ports = vec![ReducedHierPortLink {
            schematic_path: std::path::PathBuf::from("child.kicad_sch"),
            at: PointKey(0, 0),
            connection: test_net_connection(
                "/mid/child/SIG",
                "SIG",
                "/mid/child/SIG",
                "/mid/child",
            ),
        }];

        let handles = build_live_reduced_subgraph_handles(&graph);
        let mut component =
            LiveReducedSubgraph::collect_propagation_component_handles(&handles[1], &handles)
                .into_iter()
                .map(|handle| handle.borrow().source_index)
                .collect::<Vec<_>>();
        component.sort_unstable();

        assert_eq!(component, vec![1, 2]);
    }

    #[test]
    fn dynamic_power_hierarchy_preserves_text_driver_names_after_propagation() {
        let root_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
            "../ki/tests/fixtures/erc_upstream_qa/projects/ERC_dynamic_power_symbol_test.kicad_sch",
        );
        let loaded = load_schematic_tree(&root_path).expect("load tree");
        let project = SchematicProject::from_load_result(loaded);
        let graph = project.reduced_project_net_graph(false);
        let child_gnd_subgraphs = graph
            .subgraphs
            .iter()
            .filter(|subgraph| !subgraph.sheet_instance_path.is_empty() && subgraph.name == "GND")
            .collect::<Vec<_>>();

        assert_eq!(child_gnd_subgraphs.len(), 3);
        for subgraph in child_gnd_subgraphs {
            assert!(
                subgraph.drivers.iter().any(|driver| {
                    driver.kind == ReducedProjectDriverKind::Label
                        && driver.connection.name == "GND"
                        && driver.connection.local_name == "REF_NODE"
                }),
                "hierarchical-label driver should keep shown text after GND propagation: {subgraph:?}"
            );
            assert!(
                !subgraph.drivers.iter().any(|driver| {
                    driver.kind == ReducedProjectDriverKind::Label
                        && driver.connection.local_name == "GND"
                }),
                "text driver local name should not be overwritten by propagated net name: {subgraph:?}"
            );
        }
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
            child_sheet_uuid: Some("child-sheet".to_string()),
        };
        let subgraph = ReducedProjectSubgraphEntry {
            subgraph_code: 1,
            code: 1,
            name: "/SIG".to_string(),
            resolved_connection: chosen_connection.clone(),
            driver_connection: chosen_connection.clone(),
            chosen_driver_index: Some(1),
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
                chosen_driver_index: None,
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
                chosen_driver_index: None,
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
                base_pins: vec![crate::connectivity::ReducedProjectBasePin {
                    schematic_path: std::path::PathBuf::from("root.kicad_sch"),
                    key: crate::connectivity::ReducedNetBasePinKey {
                        sheet_instance_path: "/other".to_string(),
                        symbol_uuid: Some("pwr".to_string()),
                        at: PointKey(0, 0),
                        name: Some("1".to_string()),
                        number: Some("1".to_string()),
                    },
                    reference: None,
                    number: Some("1".to_string()),
                    electrical_type: Some("power_in".to_string()),
                    visible: true,
                    is_power_symbol: true,
                    connection: ReducedProjectConnection {
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
            },
            ReducedProjectSubgraphEntry {
                subgraph_code: 3,
                code: 3,
                name: "PWR_ALT".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "PWR_ALT".to_string(),
                    local_name: "PWR_ALT".to_string(),
                    full_local_name: "/local/PWR_ALT".to_string(),
                    sheet_instance_path: "/local".to_string(),
                    members: Vec::new(),
                },
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "PWR_ALT".to_string(),
                    local_name: "PWR_ALT".to_string(),
                    full_local_name: "/local/PWR_ALT".to_string(),
                    sheet_instance_path: "/local".to_string(),
                    members: Vec::new(),
                },
                chosen_driver_index: None,
                drivers: vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: 4,
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "PWR_ALT".to_string(),
                        local_name: "PWR_ALT".to_string(),
                        full_local_name: "/local/PWR_ALT".to_string(),
                        sheet_instance_path: "/local".to_string(),
                        members: Vec::new(),
                    },
                    identity: None,
                }],
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: "/local".to_string(),
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
                subgraph_code: 4,
                code: 4,
                name: "PWR_ALT".to_string(),
                resolved_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "PWR_ALT".to_string(),
                    local_name: "PWR_ALT".to_string(),
                    full_local_name: "/same-sheet/PWR_ALT".to_string(),
                    sheet_instance_path: "/same-sheet".to_string(),
                    members: Vec::new(),
                },
                driver_connection: ReducedProjectConnection {
                    net_code: 0,
                    connection_type: ReducedProjectConnectionType::Net,
                    name: "PWR_ALT".to_string(),
                    local_name: "PWR_ALT".to_string(),
                    full_local_name: "/same-sheet/PWR_ALT".to_string(),
                    sheet_instance_path: "/same-sheet".to_string(),
                    members: Vec::new(),
                },
                chosen_driver_index: None,
                drivers: vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: 4,
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Net,
                        name: "PWR_ALT".to_string(),
                        local_name: "PWR_ALT".to_string(),
                        full_local_name: "/same-sheet/PWR_ALT".to_string(),
                        sheet_instance_path: "/same-sheet".to_string(),
                        members: Vec::new(),
                    },
                    identity: None,
                }],
                class: String::new(),
                has_no_connect: false,
                sheet_instance_path: "/same-sheet".to_string(),
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
        assert_eq!(graph[1].base_pins[0].connection.name, "VCC");
        assert_eq!(graph[1].base_pins[0].connection.local_name, "PWR_ALT");
        assert_eq!(graph[2].name, "PWR_ALT");
        assert_eq!(graph[2].driver_connection.name, "PWR_ALT");
        assert_eq!(graph[3].name, "PWR_ALT");
        assert_eq!(
            graph[3].driver_connection.full_local_name,
            "/same-sheet/PWR_ALT"
        );
    }

    #[test]
    fn reduced_live_secondary_promotion_schedules_already_cloned_candidate() {
        let chosen_connection = test_net_connection("VCC", "VCC", "VCC", "");
        let secondary_connection = test_net_connection("PWR_ALT", "PWR_ALT", "PWR_ALT", "");
        let graph = vec![
            test_net_subgraph(
                1,
                chosen_connection.clone(),
                vec![
                    test_power_driver(chosen_connection.clone()),
                    test_power_driver(secondary_connection.clone()),
                ],
                "",
            ),
            test_net_subgraph(
                2,
                chosen_connection,
                vec![test_power_driver(secondary_connection)],
                "/other",
            ),
        ];

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        let global_subgraphs =
            LiveReducedSubgraph::collect_global_subgraph_handles(&live_subgraphs);
        let promoted = LiveReducedSubgraph::refresh_global_secondary_driver_promotions(
            &live_subgraphs[0],
            &global_subgraphs,
        );

        assert_eq!(promoted.len(), 1);
        assert!(Rc::ptr_eq(&promoted[0], &live_subgraphs[1]));
        assert!(!live_subgraphs[1].borrow().dirty);
    }

    #[test]
    fn reduced_live_secondary_promotion_skips_chosen_driver_by_identity() {
        let mut graph = vec![
            test_net_subgraph(
                1,
                test_net_connection("VCC", "VCC", "VCC", ""),
                vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: super::reduced_global_label_driver_priority(),
                    connection: test_net_connection("ALIAS", "ALIAS", "ALIAS", ""),
                    identity: None,
                }],
                "",
            ),
            test_net_subgraph(
                2,
                test_net_connection("ALIAS", "ALIAS", "ALIAS", "/other"),
                vec![ReducedProjectStrongDriver {
                    kind: ReducedProjectDriverKind::Label,
                    priority: super::reduced_global_label_driver_priority(),
                    connection: test_net_connection("ALIAS", "ALIAS", "ALIAS", "/other"),
                    identity: None,
                }],
                "/other",
            ),
        ];
        graph[0].chosen_driver_index = Some(0);

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        let global_subgraphs =
            LiveReducedSubgraph::collect_global_subgraph_handles(&live_subgraphs);
        let promoted = LiveReducedSubgraph::refresh_global_secondary_driver_promotions(
            &live_subgraphs[0],
            &global_subgraphs,
        );

        assert!(promoted.is_empty());
        assert_eq!(
            live_subgraphs[1].borrow().driver_connection.borrow().name,
            "ALIAS"
        );
    }

    #[test]
    fn reduced_live_secondary_promotion_matches_driver_shown_names() {
        let chosen_connection = test_net_connection("VCC", "VCC", "VCC", "");
        let secondary_connection =
            test_net_connection("/same/PWR_ALT", "PWR_ALT", "/same/PWR_ALT", "/same");
        let candidate_connection = test_net_connection("PWR_ALT", "PWR_ALT", "PWR_ALT", "/same");
        let graph = vec![
            test_net_subgraph(
                1,
                chosen_connection.clone(),
                vec![
                    test_power_driver(chosen_connection.clone()),
                    ReducedProjectStrongDriver {
                        kind: ReducedProjectDriverKind::Label,
                        priority: super::reduced_local_label_driver_priority(),
                        connection: secondary_connection,
                        identity: None,
                    },
                ],
                "/same",
            ),
            test_net_subgraph(
                2,
                candidate_connection.clone(),
                vec![test_power_driver(candidate_connection)],
                "/same",
            ),
        ];

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        let global_subgraphs =
            LiveReducedSubgraph::collect_global_subgraph_handles(&live_subgraphs);
        let promoted = LiveReducedSubgraph::refresh_global_secondary_driver_promotions(
            &live_subgraphs[0],
            &global_subgraphs,
        );

        assert_eq!(promoted.len(), 1);
        assert!(Rc::ptr_eq(&promoted[0], &live_subgraphs[1]));
    }

    #[test]
    fn reduced_live_hierarchy_deferral_keeps_dirty_for_forced_pass() {
        let connection = test_net_connection("/SIG", "SIG", "/SIG", "");
        let mut graph = vec![test_net_subgraph(1, connection.clone(), Vec::new(), "")];
        graph[0].hier_sheet_pins = vec![ReducedHierSheetPinLink {
            schematic_path: std::path::PathBuf::from("root.kicad_sch"),
            at: PointKey(0, 0),
            child_sheet_uuid: Some("child".to_string()),
            connection: connection.clone(),
        }];
        graph[0].hier_ports = vec![ReducedHierPortLink {
            schematic_path: std::path::PathBuf::from("root.kicad_sch"),
            at: PointKey(1, 0),
            connection,
        }];

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        let mut stale_members = Vec::new();
        LiveReducedSubgraph::propagate_hierarchy_chain(
            &live_subgraphs[0],
            &live_subgraphs,
            false,
            &mut stale_members,
            true,
        );

        assert!(live_subgraphs[0].borrow().dirty);

        LiveReducedSubgraph::propagate_hierarchy_chain(
            &live_subgraphs[0],
            &live_subgraphs,
            true,
            &mut stale_members,
            true,
        );

        assert!(!live_subgraphs[0].borrow().dirty);
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
            chosen_driver_index: None,
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
                reference: None,
                number: Some("1".to_string()),
                electrical_type: Some("passive".to_string()),
                visible: true,
                is_power_symbol: false,
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

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        for handle in &live_subgraphs {
            LiveReducedSubgraph::refresh_post_propagation_item_connections(handle);
            assert!(!handle.borrow().dirty);
        }
        apply_live_reduced_driver_connections_from_handles(&mut graph, &live_subgraphs);

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
            chosen_driver_index: None,
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
                reference: None,
                number: Some("1".to_string()),
                electrical_type: Some("passive".to_string()),
                visible: true,
                is_power_symbol: false,
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
    fn live_post_propagation_sheet_pin_bus_promotion_does_not_alias_wire_item_connection() {
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
                chosen_driver_index: None,
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
                        name: "/BUS".to_string(),
                        local_name: "BUS".to_string(),
                        full_local_name: "/BUS".to_string(),
                        sheet_instance_path: String::new(),
                        members: Vec::new(),
                    },
                }],
                hier_ports: Vec::new(),
                bus_members: Vec::new(),
                bus_items: Vec::new(),
                wire_items: vec![ReducedSubgraphWireItem {
                    start: PointKey(0, 0),
                    end: PointKey(10, 0),
                    is_bus_entry: false,
                    start_is_wire_side: false,
                    connected_bus_subgraph_index: None,
                }],
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
                chosen_driver_index: None,
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
                hier_ports: vec![ReducedHierPortLink {
                    schematic_path: std::path::PathBuf::from("child.kicad_sch"),
                    at: PointKey(0, 0),
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Bus,
                        name: "/BUS".to_string(),
                        local_name: "BUS".to_string(),
                        full_local_name: "/BUS".to_string(),
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

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        let wire_item_connection = live_subgraphs[0].borrow().wire_items[0]
            .borrow()
            .connection
            .clone();
        assert!(!Rc::ptr_eq(
            &wire_item_connection,
            &live_subgraphs[0].borrow().driver_connection
        ));

        for handle in &live_subgraphs {
            LiveReducedSubgraph::refresh_post_propagation_item_connections(handle);
        }
        apply_live_reduced_driver_connections_from_handles(&mut graph, &live_subgraphs);

        assert_eq!(
            live_subgraphs[0]
                .borrow()
                .driver_connection
                .borrow()
                .connection_type,
            ReducedProjectConnectionType::Bus
        );
        assert_eq!(
            wire_item_connection.borrow().connection_type,
            ReducedProjectConnectionType::Net
        );
        assert!(wire_item_connection.borrow().members.is_empty());
        assert_eq!(
            graph[0].driver_connection.connection_type,
            ReducedProjectConnectionType::Bus
        );
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
            chosen_driver_index: None,
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
                reference: None,
                number: Some("1".to_string()),
                electrical_type: Some("input".to_string()),
                visible: true,
                is_power_symbol: false,
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
                dangling: false,
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
            chosen_driver_index: None,
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
                        sheet_instance_path: String::new(),
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
                reference: None,
                number: Some("1".to_string()),
                electrical_type: Some("power_in".to_string()),
                visible: true,
                is_power_symbol: true,
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
                dangling: false,
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
            chosen_driver_index: None,
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
                        sheet_instance_path: String::new(),
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
                reference: None,
                number: Some("1".to_string()),
                electrical_type: Some("power_in".to_string()),
                visible: true,
                is_power_symbol: true,
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
                dangling: false,
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
            chosen_driver_index: None,
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
                reference: None,
                number: Some("1".to_string()),
                electrical_type: Some("passive".to_string()),
                visible: true,
                is_power_symbol: false,
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
            chosen_driver_index: None,
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
                    sheet_instance_path: String::new(),
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
                reference: None,
                number: Some("1".to_string()),
                electrical_type: Some("power_in".to_string()),
                visible: true,
                is_power_symbol: true,
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
            sheet_instance_path: String::new(),
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
            chosen_driver_index: Some(0),
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
                reference: None,
                number: Some("1".to_string()),
                electrical_type: Some("power_in".to_string()),
                visible: true,
                is_power_symbol: true,
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

        assert_eq!(
            super::reduced_project_subgraph_driver_identity(&reduced[0]),
            Some(&chosen_identity)
        );
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
            chosen_driver_index: None,
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
                reference: None,
                number: Some("7".to_string()),
                electrical_type: Some("bidirectional".to_string()),
                visible: true,
                is_power_symbol: false,
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
            chosen_driver_index: None,
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
                dangling: false,
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
            chosen_driver_index: None,
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
                dangling: false,
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
                chosen_driver_index: None,
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
                        name: "/BUS".to_string(),
                        local_name: "BUS".to_string(),
                        full_local_name: "/BUS".to_string(),
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
                chosen_driver_index: None,
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
                hier_ports: vec![ReducedHierPortLink {
                    schematic_path: std::path::PathBuf::from("child.kicad_sch"),
                    at: PointKey(0, 0),
                    connection: ReducedProjectConnection {
                        net_code: 0,
                        connection_type: ReducedProjectConnectionType::Bus,
                        name: "/BUS".to_string(),
                        local_name: "BUS".to_string(),
                        full_local_name: "/BUS".to_string(),
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

        let live_subgraphs = build_live_reduced_subgraph_handles(&graph);
        for handle in &live_subgraphs {
            LiveReducedSubgraph::refresh_post_propagation_item_connections(handle);
            assert!(!handle.borrow().dirty);
        }
        apply_live_reduced_driver_connections_from_handles(&mut graph, &live_subgraphs);

        assert_eq!(
            graph[0].driver_connection.connection_type,
            ReducedProjectConnectionType::Bus
        );
        assert_eq!(
            graph[0].driver_connection.members[0].full_local_name,
            "/child/BUS0"
        );
    }

    #[test]
    fn reduced_process_subgraphs_strong_driver_gate_promotes_unique_sheet_pin_only() {
        let sheet_pin_driver = ReducedProjectStrongDriver {
            kind: ReducedProjectDriverKind::SheetPin,
            priority: super::reduced_sheet_pin_driver_priority(),
            connection: test_net_connection("/SIG", "SIG", "/SIG", ""),
            identity: None,
        };
        let pin_driver = ReducedProjectStrongDriver {
            kind: ReducedProjectDriverKind::Pin,
            priority: super::reduced_pin_driver_priority(),
            connection: test_net_connection("/SIG", "SIG", "/SIG", ""),
            identity: None,
        };
        let label_driver = ReducedProjectStrongDriver {
            kind: ReducedProjectDriverKind::Label,
            priority: super::reduced_local_label_driver_priority(),
            connection: test_net_connection("/SIG", "SIG", "/SIG", ""),
            identity: None,
        };

        let mut subgraphs = vec![
            test_net_subgraph(
                1,
                test_bus_connection(
                    "/BUS",
                    "BUS",
                    "/BUS",
                    "",
                    vec![test_bus_member("SIG", "SIG", "/SIG")],
                ),
                Vec::new(),
                "",
            ),
            test_net_subgraph(
                2,
                test_net_connection("/SIG", "SIG", "/SIG", ""),
                vec![pin_driver],
                "",
            ),
            test_net_subgraph(
                3,
                test_net_connection("/SIG", "SIG", "/SIG", ""),
                vec![sheet_pin_driver],
                "",
            ),
            test_net_subgraph(
                4,
                test_net_connection("/SIG", "SIG", "/SIG", ""),
                vec![label_driver],
                "",
            ),
        ];
        subgraphs[2].chosen_driver_index = Some(0);
        subgraphs[3].chosen_driver_index = Some(0);

        let unique_sheet_pin_map =
            BTreeMap::from([(("".to_string(), "/SIG".to_string()), vec![2])]);
        assert!(!super::reduced_project_subgraph_has_process_strong_driver(
            &subgraphs,
            &unique_sheet_pin_map,
            0
        ));
        assert!(!super::reduced_project_subgraph_has_process_strong_driver(
            &subgraphs,
            &unique_sheet_pin_map,
            1
        ));
        assert!(super::reduced_project_subgraph_has_process_strong_driver(
            &subgraphs,
            &unique_sheet_pin_map,
            2
        ));
        assert!(super::reduced_project_subgraph_has_process_strong_driver(
            &subgraphs,
            &unique_sheet_pin_map,
            3
        ));

        let conflicting_sheet_pin_map =
            BTreeMap::from([(("".to_string(), "/SIG".to_string()), vec![2, 3])]);
        assert!(!super::reduced_project_subgraph_has_process_strong_driver(
            &subgraphs,
            &conflicting_sheet_pin_map,
            2
        ));
    }

    #[test]
    fn reduced_process_subgraphs_renames_weak_duplicate_driver_names() {
        let pin_driver = ReducedProjectStrongDriver {
            kind: ReducedProjectDriverKind::Pin,
            priority: super::reduced_pin_driver_priority(),
            connection: test_net_connection("/SIG", "SIG", "/SIG", ""),
            identity: None,
        };
        let mut subgraphs = vec![
            test_net_subgraph(
                1,
                test_net_connection("/SIG", "SIG", "/SIG", ""),
                vec![pin_driver.clone()],
                "",
            ),
            test_net_subgraph(
                2,
                test_net_connection("/SIG", "SIG", "/SIG", ""),
                vec![pin_driver],
                "",
            ),
        ];
        let (mut by_name, mut by_sheet_and_name) =
            super::reduced_project_rebuild_process_name_indexes(&subgraphs);

        super::reduced_project_rename_weak_conflict_subgraphs(
            &mut subgraphs,
            &mut by_name,
            &mut by_sheet_and_name,
        );

        assert_eq!(subgraphs[0].driver_connection.name, "/SIG_1");
        assert_eq!(subgraphs[0].driver_connection.local_name, "SIG_1");
        assert_eq!(subgraphs[0].driver_connection.full_local_name, "/SIG_1");
        assert_eq!(subgraphs[0].name, "/SIG_1");
        assert_eq!(subgraphs[1].driver_connection.name, "/SIG");
        assert_eq!(by_name.get("/SIG_1"), Some(&vec![0]));
        assert_eq!(by_name.get("/SIG"), Some(&vec![1]));
        assert_eq!(
            by_sheet_and_name.get(&("".to_string(), "/SIG_1".to_string())),
            Some(&vec![0])
        );
    }
}
impl PartialEq for LiveReducedLabelLink {
    fn eq(&self, other: &Self) -> bool {
        (
            &self.schematic_path,
            self.at,
            self.kind,
            self.dangling,
            &self.shown_text_local_name,
            self.connection.borrow().snapshot(),
            self.driver_connection.borrow().snapshot(),
            live_optional_driver_snapshot(&self.driver),
        ) == (
            &other.schematic_path,
            other.at,
            other.kind,
            other.dangling,
            &other.shown_text_local_name,
            other.connection.borrow().snapshot(),
            other.driver_connection.borrow().snapshot(),
            live_optional_driver_snapshot(&other.driver),
        )
    }
}

impl Eq for LiveReducedLabelLink {}
