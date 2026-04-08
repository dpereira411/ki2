# Parity Backlog

This file is the authoritative backlog for KiCad schematic parity work in this repository.

It replaces the older split across:
- `LOCAL_FUNCTION_PARITY_MAP.md`
- `LOCAL_PARSER_PARITY_NOTES.md`
- `LOCAL_PARSER_BFS_RECORD.md`

Those files now exist only as compatibility pointers or reduced artifacts.

## Goal

The target is not a KiCad-inspired parser or loader. The target is a structural Rust port of the
KiCad schematic pipeline with behavior tracked against upstream KiCad in:

- `/Users/Daniel/Desktop/kicad/eeschema/sch_io/kicad_sexpr/sch_io_kicad_sexpr_parser.cpp`
- `/Users/Daniel/Desktop/kicad/eeschema/schematic.cpp`
- `/Users/Daniel/Desktop/kicad/eeschema/sch_sheet_path.h`
- `/Users/Daniel/Desktop/kicad/eeschema/connection_graph.h`
- `/Users/Daniel/Desktop/kicad/eeschema/connection_graph.cpp`
- `/Users/Daniel/Desktop/kicad/eeschema/erc/erc.cpp`
- `/Users/Daniel/Desktop/kicad/eeschema/eeschema_jobs_handler.cpp`

Primary product goal now:
- strict ERC parity
- strict net naming parity
- strict netlist/export parity
- simulation-model parity last

Active phase order:
1. parser parity
2. loader/hierarchy baseline
3. connection-graph parity as the primary owning subsystem
4. ERC and net naming on top of that graph ownership
5. netlist/export on top of that graph ownership
6. simulation parity last

Secondary product goal:
- expose a CLI-facing API surface that behaves like `kicad-cli` for the exercised schematic paths

Feature completion standard:
- every feature in scope must target 1:1 KiCad parity in owning code flow and behavior
- "close enough" output is not sufficient for sign-off
- reduced local slices are acceptable only as temporary unblock steps and must stay marked as
  transitional until the owning upstream code path is either matched or explicitly blocked

Ownership rule:
- "ownership" means the same subsystem that owns a fact in upstream KiCad must own it locally too
- parser-owned facts should not be reconstructed later
- loader-owned occurrence/page/current-sheet state should not be rebuilt inside ERC or export
- connection-graph-owned connectivity, net naming, and subgraph facts should not be re-derived by
  ERC, export, or shown-text helpers once a shared graph owner exists
- exporter code should format graph-owned/export-base-owned state, not rebuild connectivity locally

## Current State

### Parser

Parser-only routine work is effectively exhausted in the current model.

Current parser status:
- parser routine parity is functionally done
- malformed UUID parity is done
- structured diagnostic/source-location support is done
- remaining parser-side gap is narrow final wording polish in rendered diagnostics

Parser-only boundary:
- `src/token.rs`
- `src/model.rs`
- `src/error.rs`
- `src/diagnostic.rs`
- `src/parser.rs`

### Loader / Hierarchy

Hierarchy loading is operational and broadly close to upstream, but is not yet signed off as full
1:1 parity.

What is substantially in place:
- sheet tree expansion
- reused-screen handling
- loaded sheet paths
- selected/current sheet behavior
- page-number and page-count propagation
- intersheet reference recompute
- symbol and sheet occurrence refresh
- current variant application to loaded occurrences

What is still not signed off for strict 1:1:
- connectivity-backed shown-text state
- fuller hatch/cache parity
- any richer occurrence-owned state later exposed by ERC

### ERC

Local reduced ERC support is live.

Implemented reduced analogues:
- `ERC_TESTER::TestDuplicateSheetNames()`
- `ERC_TESTER::TestTextVars()` exercised non-drawing-sheet slice
- `ERC_TESTER::TestFieldNameWhitespace()`
- `ERC_TESTER::TestMultiunitFootprints()`
- `ERC_TESTER::TestMissingUnits()`
- `ERC_TESTER::TestMissingNetclasses()`
- `ERC_TESTER::TestLabelMultipleWires()`
- `ERC_TESTER::TestFourWayJunction()`
- `ERC_TESTER::TestNoConnectPins()`
- `ERC_TESTER::TestMultUnitPinConflicts()`
- `ERC_TESTER::TestPinToPin()` reduced default-matrix slice
- `ERC_TESTER::TestDuplicatePinNets()`
- `ERC_TESTER::TestSameLocalGlobalLabel()`
- `ERC_TESTER::TestSimilarLabels()` reduced label/power name slice
- `ERC_TESTER::TestFootprintFilters()`
- `ERC_TESTER::TestStackedPinNotation()`
- `ERC_TESTER::TestGroundPins()`
- `ERC_TESTER::TestOffGridEndpoints()`
- first local `erc` CLI command on top of the live loader/ERC engine
- reduced ERC text-report output and default `<stem>-erc.rpt` behavior
- reduced ERC JSON report output

Still pending for ERC:
- remaining drawing-sheet slice of `ERC_TESTER::TestTextVars()`
- fuller `TestPinToPin()` exactness:
  - KiCad `ERC_SETTINGS` severity/pin-map overrides
  - graph-owned pin contexts and marker-selection heuristics
  - broader driver/no-connect/subgraph exactness
- `ERC_TESTER::TestLibSymbolIssues()` / `ERCE_LIB_SYMBOL_MISMATCH`
  - blocked on a real symbol-library subsystem:
    - project symbol-library table rows
    - disabled/missing-library resolution
    - external `.kicad_sym` loading
    - loaded-library symbol lookup by `LIB_ID`
    - flattened library-symbol comparison against the schematic snapshot
- `ERC_TESTER::TestFootprintLinkIssues()`
  - blocked on PCB/CvPcb-side footprint-link state and footprint-library tables
- `ERC_TESTER::TestSimModelIssues()`
  - still deferred with the broader sim-model backlog

Current drawing-sheet blocker:
- the Rust tree now has a reduced worksheet text-item carrier for custom/embedded `tbtext` items
  plus reduced shown-text/assertion/unresolved ERC coverage for that slice
- but there is still no local equivalent of KiCad's full `DS_PROXY_VIEW_ITEM` /
  `DS_DRAW_ITEM_LIST` path used by `ERC_TESTER::TestTextVars( aDrawingSheet )`
- parser support for embedded-file type `worksheet` exists, and the typed project-settings carrier
  now preserves `schematic.page_layout_descr_file`
- the loader can now resolve the active drawing-sheet source through that path, including matching
  schematic-embedded worksheet fallback
- the remaining gap is:
  - non-`tbtext` worksheet items
  - fuller drawing-sheet shown-text/painter semantics beyond the reduced token slice

Current ERC unblock paths:
- library-symbol issues:
  1. add a typed symbol-library table/project source layer
  2. add external `.kicad_sym` loading keyed by library nickname and `LIB_ID`
  3. add reduced library-symbol flatten/compare on top of that loaded symbol source
- footprint-link issues:
  1. add a reduced footprint-library table/project source layer
  2. add reduced footprint-link resolution for symbols with assigned footprints
  3. only then port the CvPcb-facing mismatch checks
- fuller pin-to-pin exactness:
  - done:
    - typed companion-project `erc.pin_map`
    - typed companion-project `erc.rule_severities`
    - reduced `ERC_SETTINGS` severity application over the exercised local ERC rule slice
  - remaining gap:
    - fuller graph-owned pin context and marker ranking
    - broader KiCad ERC settings surface such as exclusions and any still-untyped rule owners

Current local reality after audit:
- there is no symbol-library subsystem in this tree today:
  - no symbol-library table rows
  - no external `.kicad_sym` loader
  - no adapter-backed `LoadSymbol( LIB_ID )` equivalent
- there is no footprint-library / CvPcb-side subsystem in this tree today:
  - no footprint library table
  - no footprint-link resolver equivalent
- do not treat `TestLibSymbolIssues()` or `TestFootprintLinkIssues()` as ordinary loader work until
  those missing subsystems exist

### Simulation

Simulation-model migration/resolution is no longer on the critical path.

Simulation parity is explicitly deferred to the end of the backlog unless it blocks ERC or net
naming, which it currently does not.

## Single Active Queue

Work this list from top to bottom unless direct upstream comparison reveals a real prerequisite.

1. Connection-graph parity
   - reduced connectivity is no longer the end target
   - strict ERC/net naming/export parity now depends on moving toward KiCad's fuller connection
     ownership model:
     - connection points
     - connected subgraphs
     - driver selection
     - item-to-subgraph lookup
     - resolved full/short net names
     - exporter-visible net ownership
   - once a shared graph owner exists for an exercised fact, keep moving callers onto that owner
     instead of adding more local one-off scans in ERC/export helpers
   - current phase pivot:
     - the reduced graph has absorbed most of the honest static/shared ownership work
     - the next primary phase is the fuller live `SCH_CONNECTION` / `CONNECTION_SUBGRAPH`
       analogue
     - do not keep extending snapshot-only propagation logic once the remaining gap is live
       mutation / clone / recache behavior
2. Remaining connection-backed shown-text exactness
   - reduced connection-backed shown-text is live for the exercised ERC slice
   - remaining work is fuller KiCad settings/subgraph exactness, not missing variable support
3. Hierarchy/loading 1:1 sign-off gaps
4. Netlist/export connectivity parity
   - first local `erc` command is live
   - first local `netlist --format xml` command is live
   - reduced text-report output, default report-path behavior, JSON output, severity filters,
     report-unit metadata, `--exit-code-violations` behavior, and reduced sheet-grouped report
     structure are live
   - remaining CLI/report parity is fuller schema/config fidelity:
     - ignored-check sections / exclusions
     - fuller JSON schema fields
     - KiCad job/config plumbing beyond direct CLI flags
5. Final parser diagnostic wording polish
6. Simulation-model parity last

## Connectivity Graph Requirements

### Why This Exists

The remaining ERC and net-naming work is blocked not on parser work, but on missing connectivity
state.

This backlog is intentionally not ordered as "finish every subsystem to 100% in isolation". The
strict ordering is by owning subsystem boundary:
- parser first
- then broad loader/hierarchy support
- then connection graph as the active primary owner
- then ERC/net naming/export consumers on top of that owner

That means later ERC/export work is only valid when it either:
- ports a consumer directly against the graph/settings owner KiCad uses, or
- exposes a real blocker in that owning subsystem and records the unblock path here

Upstream KiCad uses `CONNECTION_GRAPH` as the canonical electrical model. It is used for:
- electrical connectivity ownership
- subgraph grouping
- driver selection
- net naming
- netclass/rule application
- graph-driven ERC checks
- netlist/export-facing resolved net state

Relevant upstream entry points:
- `CONNECTION_GRAPH::Recalculate(...)`
- `CONNECTION_GRAPH::RunERC()`
- `CONNECTION_GRAPH::FindSubgraphByName(...)`
- `CONNECTION_GRAPH::FindFirstSubgraphByName(...)`
- `CONNECTION_GRAPH::GetSubgraphForItem(...)`
- `CONNECTION_GRAPH::GetResolvedSubgraphName(...)`

### Reduced Connectivity Layer Needed Now

We do not need the full final KiCad graph before more ERC work lands.

We do need a reduced current-sheet, path-aware connectivity snapshot that can support the remaining
ERC and net-text tasks.

That reduced layer must provide:

1. Connection points
- keyed by loaded sheet path
- keyed by XY
- built from:
  - symbol pins
  - sheet pins
  - wires
  - junctions
  - labels
  - no-connect markers
  - bus entries where needed by the exercised rules

2. Connected components / reduced subgraphs
- enough to answer:
  - what is connected at this point
  - whether an item is connected
  - whether two pins/items share a net
  - how many wire branches meet at a point

3. Driver resolution
- enough to derive effective net naming from:
  - local/global labels
  - power pins
  - sheet pins / hierarchy propagation
  - exercised bus-member cases if they surface in the remaining rules

4. Per-item connection lookup
- enough for:
  - pin -> net
  - label -> net
  - wire endpoint -> net
  - connected-item queries used by ERC and shown text

5. Effective net naming
- resolved full net name
- resolved short net name

6. Effective netclass lookup
- default netclass
- directive/rule-area influence for connected items

Current status:
- the reduced connection-point snapshot is now live as a shared carrier in `src/connectivity.rs`
- it currently includes:
  - projected placed-symbol pins from linked lib-pin draw items
  - sheet pins
  - wire endpoints
  - labels
  - junctions
  - no-connect markers
- it is already used by:
  - reduced `ERC_TESTER::TestFourWayJunction()`
  - reduced `ERC_TESTER::TestNoConnectPins()`
  - reduced `ERC_TESTER::TestPinToPin()`
  - connection-backed shown-text / net-name resolution in `src/loader.rs`
  - connection-backed `NET_CLASS` ownership in `src/loader.rs`
  - reduced XML net export pin/net ownership and component-first net grouping in `src/netlist.rs`
- ownership is now materially closer to KiCad than before:
  - ERC no longer owns the only connection-point/component builder
  - loader, ERC, and net export now share one reduced connection owner
  - reduced connected-label driver/name selection now also lives there instead of being rebuilt in
    loader helpers
  - reduced modern power-symbol net drivers now also live there instead of being silently unnamed
    on export/current-sheet net-name paths
  - reduced sheet-pin name drivers now participate in that shared net-name ownership too
  - reduced ordinary symbol-pin default net names now also live there so unlabeled nets are no
    longer dropped from reduced export/current-sheet naming
  - reduced point-netclass ownership now also lives there instead of being rebuilt in loader
    geometry scans
  - reduced driver-label `Netclass` fields now also feed the shared netclass owner
  - reduced net-map grouping for XML/KiCad export now also lives there instead of being rebuilt
    inside `src/netlist.rs`
  - reduced cross-sheet net grouping, duplicate-pin preference, and sorted netcode allocation now
    also live there instead of staying exporter-local
  - reduced symbol-pin item lookup now also exists for pin-owned shown-text/ERC net-name queries
    instead of routing those only through raw point lookups
  - reduced project point lookup now also reads through that same shared project net owner instead
    of rebuilding the project net map and rescanning schematics per ERC query
  - reduced project net map plus pin/point lookup now live on `SchematicProject` instead of only
    in free connectivity helpers, with variant changes invalidating that reduced graph owner
  - reduced project point identity now covers all reduced subgraph member points, not only the
    chosen anchor, so label/marker point lookups no longer fall back just because they are not the
    anchor point
  - ERC point-net lookup now reads only through the shared project graph owner instead of keeping a
    second current-sheet point-net fallback inside `src/erc.rs`
  - intersheet-ref `NET_*` shown-text grouping now also builds one shared reduced project graph for
    the whole hierarchy pass instead of resolving connectivity label-by-label through local
    current-sheet reducers
  - intersheet-ref cross-reference pin `NET_NAME` / `SHORT_NET_NAME` now also read through that
    same shared graph pass, including `${REF:NET_CLASS(pin)}` after project-graph candidate
    ownership was widened to `(sheet instance path, reference, pin)` so reused-sheet symbol-pin
    lookups no longer lose per-occurrence netclass ownership before identity assignment
  - current-sheet / cross-reference `SHORT_NET_NAME` text vars now also prefer the shared reduced
    subgraph driver-name owner instead of trimming the already-resolved full net name after the
    fact, matching KiCad's separate `Name(true)` vs full-name ownership more closely
  - reduced XML export now walks shared connection components first instead of only asking every
    pin for an independent point-net name
  - reduced driver tie-breaking now prefers non-`-Pad` names when priorities match
  - reduced driver tie-breaking now also prefers bus supersets over subsets on equal-priority bus
    labels through the shared connectivity owner instead of leaving bus-width choice exporter-local
  - shared reduced bus-member expansion now recursively expands top-level aliases before ERC /
    naming / export consumers use those members for bus matching or driver tie-breaking
  - shared reduced `NET_CLASS` ownership now also propagates bus-label netclass assignments to bus
    members instead of leaving bus-entry/member netclass resolution to loader-local scans
- reduced XML single-node `+no_connect` marking is now live in `src/netlist.rs`
- reduced XML conditional `pinfunction` emission for single unnamed pins is now live in
  `src/netlist.rs`
- the next honest step is no longer "move connected label/rule-area scans":
  - grow the shared owner from reduced connected-label/power/sheet-pin/default-pin/netclass
    selection toward real subgraph ownership and broader driver resolution

Remaining divergence:
- this is still not a full KiCad `CONNECTION_GRAPH`
- it still lacks subgraph ownership, fuller driver resolution, and the broader graph-owned item
  model
- the next real consumers are:
  - fuller connection-backed shown-text precedence
  - hierarchy/loading sign-off on connectivity-backed state
  - fuller pin-matrix/settings exactness beyond the reduced default slice

### What Full KiCad Connectivity Is Used For

The full graph is broader than ERC.

It is used for:
- ERC graph checks
- net naming
- netclass/rule application
- connection-backed text variables
- netlist generation
- SPICE/export flows
- cross-probing and net selection behavior
- bus conflict and bus-member handling

If CLI parity expands beyond ERC and basic net naming, fuller graph parity will eventually need:
- net codes
- bus-aware subgraphs
- fuller hierarchy propagation
- broader graph-owned ERC checks
- export-facing resolved net model parity

## ERC Requirements Before Serious Rule Expansion

The next real ERC work depends on a reduced connectivity layer.

Required before implementing the remaining connection-driven ERC rules:

1. Reduced connection-point snapshot
2. Reduced same-net / connected-component model
3. Connection-backed shown-text resolution for:
   - `NET_NAME`
   - `SHORT_NET_NAME`
   - `NET_CLASS`
4. Reused-sheet/current-sheet regressions for those variables
5. Connection-point-driven regressions for:
   - pin-to-pin conflicts

Current status:
- step 1 is done
- reduced four-way junction coverage is done
- reduced no-connect pin coverage is done
- reduced same-net / connected-component ownership is now live for the exercised ERC slice
- reduced `CONNECTION_GRAPH::ercCheckLabels()` coverage is now live through the shared reduced
  label-component owner:
  - `erc-label-not-connected`
  - `erc-label-single-pin`
  - label iteration now starts from shared graph-owned label-item identity instead of per-subgraph
    point-list recovery
  - same-name label grouping, pin counts, and no-connect aggregation now also derive from shared
    reduced project subgraphs plus a shared `GetAllSubgraphs()`-style lookup instead of regrouping
    local component `net_name` strings inside ERC
  - current divergence is fuller cross-sheet subgraph/bus-parent neighbor ownership, not absence of
    the graph-owned label rule
- reduced `CONNECTION_GRAPH::ercCheckSingleGlobalLabel()` coverage is now live through the loaded
  sheet-list shown-text walk
- reduced `CONNECTION_GRAPH::ercCheckHierSheets()` coverage is now live for:
  - root-sheet hierarchical labels
  - dangling parent sheet pins
  - parent/child sheet-pin name mismatches
- reduced `CONNECTION_GRAPH::ercCheckDirectiveLabels()` coverage is now live through the shared
  reduced label-component snapshot
- the small graph-owned label/hierarchy cluster is now covered in the reduced graph
- reduced `CONNECTION_GRAPH::ercCheckNoConnects()` coverage is now live for:
  - no-connect markers on connected local nets
  - no-connect markers on same-name disconnected subgraphs that KiCad merges into one net
  - dangling no-connect markers with no pins or labels
  - no-connect iteration now starts from shared graph-owned marker identity instead of per-subgraph
    point-list recovery
  - hier-pin / hierarchical-label special-case exemption when the local subgraph is only the
    no-connect plus that hierarchy boundary item
  - unnamed marker-only local subgraphs now survive into the shared reduced project graph instead
    of disappearing before the graph-owned no-connect pass runs
  - same-name no-connect neighbor aggregation now uses the shared `GetAllSubgraphs()`-style lookup
    instead of a local per-net regroup in ERC
  - current divergence is the fuller marker attachment path, not
    absence of the graph-owned no-connect rule
- reduced wire-only coverage is now live for:
  - `CONNECTION_GRAPH::ercCheckFloatingWires()`
  - `CONNECTION_GRAPH::ercCheckDanglingWireEndpoints()`
  - floating-wire and dangling-endpoint iteration now starts from shared reduced subgraph
    wire-item membership instead of per-sheet connection-point/component rescans
- reduced bus-entry participation is now also live for that cluster through the shared segment owner
- unnamed wire/bus-entry-only local components now survive into the shared reduced project graph
  instead of disappearing before the graph-owned wire ERC passes run
- current remaining gap in that cluster is fuller bus conflict/subgraph semantics, not absence of
  the wire/bus-entry floating-endpoint rules
- reduced `CONNECTION_GRAPH::ercCheckBusToNetConflicts()` coverage is now live through:
  - shared reduced project subgraphs
  - shared reduced wire/bus line membership on those subgraphs
  - reduced bus-vs-net classification from line kind and shown text
- current remaining bus graph gaps are the member-aware branches, not gross bus-vs-net conflicts
- reduced `CONNECTION_GRAPH::ercCheckBusToBusConflicts()` coverage is now live through:
  - shared reduced subgraph-owned bus-member caches for label/port members
  - reduced bus-member expansion from aliases and bracketed vectors
  - nested bus-group text expansion with brace-depth-aware member splitting
  - shared reduced direct-member objects with `Name()`-style local naming on subgraphs
  - reduced shared-member overlap acceptance for connected bus label/port pairs
  - shared reduced label/port ownership on project subgraphs
- reduced `CONNECTION_GRAPH::ercCheckBusToBusEntryConflicts()` coverage is now live through:
  - shared reduced project subgraphs
  - shared reduced bus-entry line membership
  - shared reduced bus-member objects with flattened `FullLocalName()`-style comparison only at the
    ERC comparison site
  - shared reduced non-bus connection-name caches on subgraphs
  - reduced bus-member expansion plus reduced shared non-bus driver names
  - shared singular full driver-name ownership on subgraphs, so the bus-entry warning fallback no
    longer guesses from the resolved whole-net name when upstream uses the driver connection name
  - prefixed bus-group members like `USB{DP DM}` -> `USB.DP` / `USB.DM`
  - nested bus-group text expansion like `USB{PAIR{DP DM} AUX}`
  - KiCad-style suppression when a higher-priority global label or power pin overrides the bus
    member driver
  - KiCad-style single-warning flow from the shared subgraph driver name instead of one warning
    per non-bus shown-text on the same subgraph
  - reduced report anchoring now follows the shared bus-entry item position instead of the old
    repo-local net-label point
- the named graph-owned bus conflict trio is now covered in the reduced graph
- reduced `ercCheckMultipleDrivers()` coverage is now live for the exercised strong-driver slice
- reduced `ercCheckMultipleDrivers()` now reads strong-driver conflicts from the shared reduced
  project subgraph owner instead of rebuilding them from per-sheet connection components
- reduced shared subgraph ownership now preserves node-less driver subgraphs so pure connected-label
  strong-driver conflicts survive into the graph-owned ERC path
- reduced pin-to-pin coverage is now live on top of the upstream default pin matrix
- reduced `TestPinToPin()` now iterates the shared reduced project net map like upstream `m_nets`
  instead of per-sheet connection components, while still using shared physical `base_pins` so
  same-reference physical pin multiplicity survives node flattening
- reduced `TestPinToPin()` now also builds deterministic reduced pin contexts from shared
  `base_pins` (sheet path, reference, pin number, pin type, point) instead of treating each net as
  an unordered bag of bare pin types
- reduced `TestPinToPin()` now also follows the upstream weighted conflict-selection shape more
  closely:
  - gathers all mismatching pin pairs on one net
  - ranks pins by KiCad-style pin-type weights
  - chooses the nearest conflicting partner on the same sheet when possible before emitting
    diagnostics
- reduced missing-driver selection now also prefers non-power-symbol driven pins over power-symbol
  pins when choosing one report target, matching KiCad's `nonPowerPinsNeedingDrivers` branch more
  closely with the local signal that is currently modeled
- reduced pin-context ordering now also uses a local `StrNumCmp` analogue for references and pin
  numbers before conflict selection, instead of plain lexical ordering
- reduced cross-reference shown-text now covers the exercised symbol pin-function slice:
  - `${REF:NET_NAME(pin)}`
  - `${REF:SHORT_NET_NAME(pin)}`
  - `${REF:PIN_NAME(pin)}`
- reduced cross-reference shown-text now also covers:
  - `${REF:NET_CLASS(pin)}`
- the remaining gap is fuller KiCad settings/subgraph exactness, not absence of the rule
- graph-only symbol-pin net lookup now covers the exercised multi-pin power-symbol branch:
  - shared reduced pin identity now preserves base-pin ownership even when a symbol is excluded
    from netlist node emission, which keeps power-symbol ERC pin lookup on the graph-owned path
  - focused regression is live for the old `erc_reports_ground_pins_on_non_ground_nets` miss
  - `ERC_TESTER::TestGroundPins()` now reads pin net names from the shared project graph only
  - remaining gap is broader item ownership beyond the now-covered power-symbol branch, not this
    specific fallback
- the drawing-sheet text-vars slice is now functionally covered for the exercised ERC path
- remaining drawing-sheet work is broader worksheet draw-item/painter parity, not missing
  `ERC_TESTER::TestTextVars()` text behavior
- the next honest connection-graph ERC gaps are no longer label ownership itself; they are the
  remaining graph-owned passes without local analogues:
  - fuller shared connection/subgraph ownership for strict 1:1 net naming and export:
    - fuller item-owned connection naming beyond the now-shared `Name()`-style
      full-vs-short/path-qualified reduced net naming split
    - netcode-style ownership beyond the now-live shared graph code preservation on the reduced
      whole-net map
    - fuller resolved member-object ownership beyond the now-live reduced bus-member tree
  - shared connection points now keep bus segments distinct from wire segments, so wire-only ERC
    branches no longer count buses through the old collapsed `Wire` member kind
  - hierarchy-side sheet-pin shown-text now uses a reduced `SCH_SHEET_PIN::GetShownText()` owner:
    - connection-backed tokens resolve from the parent sheet-pin connection point
    - shared graph driver-name selection and current-sheet `Name(false)` lookup now also consume
      that shown-text owner instead of raw sheet-pin names, including unlabeled nets driven only
      by sheet-pin shown text
    - remaining sheet/project text vars recurse through the child sheet path like the upstream
      parent-sheet branch
    - `ercCheckHierSheets()` plus the exercised bus conflict checks now compare parent sheet pins
      through that owner instead of raw names
  - remaining gap is broader sheet-pin item ownership beyond the now-live shown-text path, not the
    old raw-name comparison branch
  - shared reduced subgraphs now also own:
    - reduced bus-parent links
    - reduced hierarchy parent/child links
    - reduced label text-item membership with shown text, point, and bus-vs-net class
    - reduced hierarchy pin/port text-item membership with shown text, point, and bus-vs-net class
  - `ercCheckLabels()` now walks shared parent links for no-connect and local-hierarchy state
  - `ercCheckBusToNetConflicts()` and `ercCheckBusToBusConflicts()` now classify text items from
    shared subgraph ownership instead of rescanning labels and sheet pins out of the schematic
  - shared reduced strong-driver ownership now also preserves reduced driver kind:
    - `ercCheckMultipleDrivers()` now mirrors KiCad's exercised "labels and power pins only"
      secondary-driver filter instead of warning on sheet-pin-only name differences
  - `ercCheckNoConnects()`, `ercCheckFloatingWires()`, `ercCheckDanglingWireEndpoints()`, and the
    reduced parent-sheet dangling-pin query now also consume those shared label/hierarchy links
    instead of the older point-only label/sheet-pin side snapshots
  - the remaining point-only label/sheet-pin snapshots are now bookkeeping for item lookup and
    graph indexing, not the active ERC owner for those exercised rules
  - current concrete blocker for the next strict graph step is now narrower:
    - the shared reduced graph does carry reduced connection objects with:
      - connection type
      - local/full-local/resolved names
      - current connection sheet ownership
      - member trees
      - vector-member indexes plus reduced `matchBusMember()`-style matching
      - link-owned label/sheet-pin/hier-port connections
      - member-keyed reduced bus parent/neighbor links
      - reduced stale-member refresh from final child connections
      - reduced repeated settle/fixpoint passes instead of one static propagation step
      - reduced bus-entry connected-bus ownership on shared subgraph indexes
      - first live `propagateToNeighbors()` slices on shared subgraph wrappers for:
        - hierarchy-chain best-driver selection
        - direct bus-neighbor driver/member cloning before reduced cleanup
        - direct child-net -> parent-bus member refresh before reduced cleanup
        - direct multiple-parent member rename / same-name subgraph refresh before reduced cleanup
        - direct bus parent/neighbor link member rematch before reduced cleanup
        - repeated live bus fixpoint over those slices, including stale same-bus link replay after
          promoted-member renames
        - exercised post-propagation item-connection updates before the reduced fallback:
          - weak single-pin `Net-(` -> `unconnected-(` renaming
          - sheet-pin bus/member promotion from bus-typed child neighbors
        - live graph-name cache owner for same-name subgraph recache on renamed live subgraphs
        - one shared live bus fixpoint object set across those bus sub-passes instead of rebuilding
          fresh live wrappers between each sub-pass
        - one combined live graph-propagation owner in graph build across:
          - hierarchy-chain propagation
          - bus propagation
          - exercised post-propagation item updates
        - true recursive live graph traversal now owns the exercised propagation path instead of
          the earlier whole-graph repeat sweeps
        - the recursive live walk is now seeded from hierarchy discovery, while newly dirtied
          bus-connected subgraphs are reached by recursive revisits instead of pre-expanded
          hierarchy+bus components
        - global secondary-driver promotion now runs on the shared live subgraph owner and
          recurses promoted candidates immediately instead of waiting for a later outer pass
        - one shared stale-member bag now rides with each recursive live propagation root,
          including cross-bus member replay beyond the earlier same-bus-only refresh
        - live stale-member replay is now scoped to the active recursive propagation root instead
          of the whole graph
        - hierarchy-chain propagation now reruns inside that shared live loop instead of only once
          before bus propagation, so bus-driven changes can feed back into hierarchy selection on
          the same live owner
        - the exercised multiple-bus-parent rename / same-name recache step now runs after live
          propagation, closer to KiCad's post-`propagateToNeighbors()` ordering, before item
          connection updates
        - live bus parent/neighbor links now rebuild from stable bus-parent topology plus the
          current parent member tree instead of depending on stale mutable link snapshots
        - live subgraphs now also own reduced live label/sheet-pin/hier-port connection carriers
          during propagation, and the final reduced projection writes those per-link owners back
          instead of blasting every item-side connection from the chosen driver snapshot only at
          the end
        - live reduced connection carriers are now shared mutable local owners rather than copied
          wrapper values, and the recursive live graph mutates those owners through borrow/update
          paths instead of swapping plain reduced structs
        - the active live graph now uses a dedicated local live connection payload instead of
          reusing `ReducedProjectConnection` directly as its shared mutable owner
        - live bus-entry items now also keep an attached live bus connection owner during graph
          build, and only collapse back to `connected_bus_subgraph_index` when projecting to the
          reduced query surface
        - bus-neighbor propagation now mutates existing live connection owners in place instead of
          replacing them with brand-new owners, so attached live bus-entry references keep identity
          across exercised driver/member updates
        - active live topology now prefers shared live handles for:
          - bus member links
          - plain bus parents
          - hierarchy parent/child links
          - active propagation components
          - same-name recache caches
        - the reduced projection now follows those live owners more directly for:
          - bus-entry attached bus indexes
          - bus parent indexes
          - hierarchy parent/child indexes
          - label/sheet-pin/hier-port live connection owners
    - the remaining gap is that these are still static reduced snapshots, not live
      `SCH_CONNECTION` / `CONNECTION_SUBGRAPH` objects:
      - the recursive walk now has local shared live connection owners, but it still does not have
        a full live `SCH_CONNECTION` / `CONNECTION_SUBGRAPH` object graph with pointer identity
        shared across all items, subgraphs, and attached bus items
      - the active stale-member bag now uses the live local member payload and the exercised
        active rematch helpers now match live-to-live, but replay still does not carry one fuller
        shared live connection/member object graph across all visited bus subgraphs after
        hierarchy propagation
      - no full live cached driver/item connection topology that can be recached in place across
        labels, pins, sheet pins, bus entries, and connected items by pointer identity via a
        `recacheSubgraphName()`-style owner; the local live connection owners now exist, but item
        and subgraph relationships still synchronize through local wrappers instead of a fuller
        shared object graph
      - the active recursive graph build now runs on shared live subgraph handles, but those
        handles still wrap reduced local subgraph carriers instead of a fuller local
        `CONNECTION_SUBGRAPH` analogue with stable pointer-style topology and recache/update
        behavior across the whole graph
      - connected-bus-item ownership now reaches the shared live subgraph graph for bus entries,
        but still not all the way to fuller live item / connection pointer topology across every
        attached item kind
      - the non-test live subgraph payload no longer stores copied hierarchy/plain-parent reduced
        indexes for active propagation; those topology indexes are now seeded from the reduced
        graph only during live-handle attachment and rebuilt only at projection time
      - the non-test live bus parent/neighbor link payload no longer stores copied reduced target
        indexes for active propagation; those target indexes are now seeded only during
        handle attachment and rebuilt only at projection time
      - live connection member trees, the active stale-member bag, stored live bus
        parent/neighbor links, the exercised active rematch helpers, active same-name recache
        updates, and active bus-driven promotion now use dedicated live local member/connection
        payloads, and active live bus-entry items no longer carry copied reduced bus indexes
        beside the live bus owner, but projection and remaining boundary adapters still round-trip
        through reduced snapshots instead of keeping one fuller live member/pointer graph through
        propagation and projection
      - after removing copied active bus-entry, hierarchy/plain-parent, and bus-link indexes from
        the non-test live payload, the main remaining reduced carriers are the live item wrappers
        themselves and `source_index`-style projection identity, not more active-topology side
        caches
      - active live wire-item ownership is now shared on the live graph instead of copied
        per-subgraph wrapper state, so the remaining live item-wrapper gap is increasingly
        concentrated in labels, sheet pins, hierarchy ports, and the remaining projection identity
        edges
      - active label links, sheet pins, and hierarchy ports are now also shared live item owners,
        so the main remaining item-wrapper gap is the fuller shared item/pointer topology and the
        `source_index`-style projection boundary rather than more copied active item wrappers
      - active recursive propagation, connected-component collection, and secondary-driver
        promotion now use shared live-handle identity instead of reduced subgraph indexes as their
        traversal identity; `source_index` remains mainly as projection identity and test
        scaffolding
      - active bus-link rematch now also uses handle-keyed temporary refresh state instead of
        reduced-index-keyed vectors on the live path
      - active bus parent/neighbor links now also sit on shared live link owners instead of copied
        value links inside each live subgraph, so active propagation mutates shared link state
        alongside the shared live item owners
      - after the active traversal and bus-link refresh handle ports, the remaining `source_index`
        uses are mostly projection, tests, and deterministic ordering rather than core live graph
        ownership/control flow
      - active bus-neighbor propagation and bus-link parent/child matching no longer bounce
        through reduced subgraph indexes when the relevant live handles already exist
      - the non-test live subgraph payload no longer stores `source_index`; reduced subgraph
        position is now derived from the live handle graph only at projection sites, while test
        scaffolding keeps explicit source indexes where it still needs them
      - live bus items now also use shared local item owners, so the active live item layer is no
        longer split between shared wire/text handles and copied bus-item values
      - duplicated live summary side state is smaller now too: active hierarchy and driver checks
        no longer carry copied `has_hier_*`, `local_driver`, or `strong_driver_count` fields when
        the live subgraph already owns the underlying handles and driver list
      - `base_pin_count` is now gone from the active live payload; live post-propagation checks
        read shared live base-pin payload directly
      - the remaining live summary field is now `driver_identity`, and removing it cleanly depends
        on fuller live driver item ownership rather than another local summary-field cleanup
    - concrete next unblock path:
      1. replace the reduced wrapper connections inside the recursive walk with a live local
         `SCH_CONNECTION` analogue that items and subgraphs can share by identity
      2. move live name recache and the remaining projection/boundary bus-member ownership onto
         that same connection/member owner instead of cloning reduced snapshots through recursive
         revisits
      3. widen the new live bus-entry and item-side owners into fuller live item/connection
         pointer ownership instead of collapsing them back to reduced wrappers and subgraph indexes
         at projection time
      4. remove the remaining reduced search/rematch adapters around the new live member payload
         so propagation and link refresh stop rebuilding `ReducedBusMember` keys on the active path
      5. replace the current reduced live subgraph handle payload with a fuller local
         `CONNECTION_SUBGRAPH` analogue so topology, dirty state, same-name recache, and attached
         live item owners stay on one shared object graph instead of reduced wrapper structs
      6. only after that, revisit remaining item/connection pointer ownership and connected-bus-item
         promotion
    - remaining bus-entry and parent-neighbor exactness now depends on that live-ish connection
      object behavior, not another local schematic scan or another point-list cleanup
  - architectural direction from this point:
    - keep the reduced graph as the shared caller-facing owner for now
    - begin replacing its snapshot-only propagation core with fuller live connection/subgraph
      objects
    - expected reusable reduced-graph pieces:
      - item/subgraph indexing
      - driver identity data
      - bus member parsing/tree structure
      - parent/neighbor relationships
      - caller-facing graph queries and many existing tests
    - expected transitional pieces:
      - snapshot settle/fixpoint passes
      - snapshot clone helpers
      - reduced recache logic where upstream mutates live objects recursively
  - next live-graph queue:
    1. add a local `SCH_CONNECTION` analogue with:
       - type
       - local/full-local/resolved names
       - net code
       - member tree
       - sheet ownership
       - clone/recache support
    2. add a local live `CONNECTION_SUBGRAPH` analogue with:
       - dirty state
       - driver connection
       - parent/child / bus-neighbor links
       - connected-bus-item ownership
    3. port one upstream live propagation path at a time:
       - `propagateToNeighbors()`
       - stale bus-member replay
       - in-place driver connection replacement
       - `recacheSubgraphName()`
    4. keep ERC/export on the existing shared graph query surface while replacing the internals

## Net Naming / CLI Requirements

If the goal is to match `kicad-cli` behavior for ERC and net naming, the minimum required subsystem
set is:

1. Hierarchy/current-sheet model
- loaded sheet paths
- reused-screen handling
- current sheet
- current variant
- occurrence-aware symbol and sheet refresh

2. Reduced connectivity model
- current-sheet, path-aware, connection-point keyed

3. Shown-text resolver
- field resolution
- cross-reference resolution
- connection-backed net variables

4. Typed project/settings surface
- intersheet settings
- text variables
- default netclass
- named netclass set
- exercised rule-area influence

5. ERC diagnostic model
- structured diagnostics with source positions

6. Later, if CLI scope expands:
- fuller connection graph
- bus-aware naming and conflicts
- net codes
- exporter-facing resolved net model

## Netlist Export Parity Requirements

If the target includes schematic export parity with KiCad CLI, the backlog must cover the upstream
netlist/exporter stack too, not just ERC.

Relevant upstream files:
- `/Users/Daniel/Desktop/kicad/eeschema/eeschema_jobs_handler.cpp`
- `/Users/Daniel/Desktop/kicad/eeschema/netlist_exporters/netlist_generator.cpp`
- `/Users/Daniel/Desktop/kicad/eeschema/netlist_exporters/netlist_exporter_base.h`
- `/Users/Daniel/Desktop/kicad/eeschema/netlist_exporters/netlist_exporter_base.cpp`
- `/Users/Daniel/Desktop/kicad/eeschema/netlist_exporters/netlist_exporter_xml.cpp`
- `/Users/Daniel/Desktop/kicad/eeschema/netlist_exporters/netlist_exporter_kicad.cpp`
- `/Users/Daniel/Desktop/kicad/eeschema/netlist_exporters/netlist_exporter_spice.cpp`
- format-specific exporters such as:
  - OrCAD
  - CADSTAR
  - Allegro
  - PADS

### Upstream Export Preconditions

Before KiCad writes a netlist, it does more than just serialize current symbols.

Observed upstream preconditions:

1. Annotation must be valid
- `ReadyToNetlist()` / CLI job checks annotation first

2. Duplicate sheet names are checked
- `ERC_TESTER::TestDuplicateSheetNames(false)` is used as an export precondition

3. Power symbols are annotated/fixed before export
- `Hierarchy().AnnotatePowerSymbols()`

4. Connectivity must be rebuilt/up to date
- when incremental connectivity is enabled, netlist generation forces a full rebuild
- exporter code assumes a valid connection model underneath

This means export parity depends on:
- hierarchy/load parity
- annotation-visible symbol state
- duplicate-sheet-name ERC parity
- connectivity graph/net naming parity

### Common Exporter Base Requirements

The common exporter layer requires:

1. Symbol iteration and filtering parity
- `findNextSymbol(...)`
- skip virtual/power-only/internal symbols where KiCad does
- process only the correct occurrence/unit set

2. Pin-list construction parity
- `CreatePinList(...)`
- `findAllUnitsOfSymbol(...)`
- `eraseDuplicatePins(...)`
- multi-unit symbol handling
- duplicate power/common pins deduplication
- connected vs unconnected pin retention rules per exporter

3. Library-part collection parity
- `m_libParts`
- stable part/library identity for export

4. Field/value exposure parity
- mandatory/user/generated field visibility as seen by exporters
- current variant and occurrence-aware field text where exporters use shown text

### XML / KiCad Netlist Requirements

The XML / KiCad-style exporters need:

1. symbol list parity
- references
- values
- footprints
- fields
- sheet paths
- current variant-sensitive symbol state

2. library / libpart parity
- linked library identity
- pins
- aliases / units as exposed in export

3. net list parity
- resolved nets
- per-net node membership
- stable effective net names

4. reduced component class parity
- upstream XML exporter includes:
  - reduced KiCad-format root `groups` export is live
  - reduced KiCad-format root `variants` export is live
  - reduced `<component_classes>` export is live
  - fuller rule-area child-item and sheet-level component-class aggregation still diverges

### SPICE Export Requirements

SPICE export is a separate parity surface and is broader than core ERC.

It needs:

1. current-sheet-as-root behavior
- `OPTION_CUR_SHEET_AS_ROOT`
- reduced hierarchy-root selection parity

2. SPICE net-name conversion
- `ConvertToSpiceMarkup(...)`

3. directive collection
- `.include`
- directives
- save options / simulation command options

4. simulation-model parity
- this is why SPICE export is still downstream from the currently deferred sim-model backlog

5. per-pin net-name generation
- resolved net names must already exist before SPICE export can be 1:1

### Non-SPICE Netlist Formats

Formats like OrCAD, CADSTAR, Allegro, and PADS depend mostly on:
- hierarchy/load parity
- symbol/unit/pin-list parity
- resolved net names
- stable pin ordering and remaining full-graph duplicate-pin handling beyond the now-live reduced XML
  exporter path
- format-specific text normalization/sorting

These are downstream from the common exporter base plus net naming.

### Export Parity Queue

Do not treat exporter parity as complete until all of these have been audited explicitly:

1. common exporter base
- symbol filtering
- multi-unit collection
- reduced ordered-symbol primary selection is now live on the XML/KiCad path:
  - same-reference symbols now choose a primary before component construction
  - later multi-unit duplicates now skip through one shared exporter-base-style walk
- reduced `addSymbolFields()` multi-unit field scavenging is now live on the XML/KiCad path:
  - value / footprint / datasheet / description
  - non-mandatory user fields
- duplicate-pin erasure is now live on the reduced XML exporter path; full common-exporter ownership
  still remains

2. XML / KiCad netlist exporter
- symbols
- libraries/libparts
- nets
- fuller graph-owned netcode/name ownership

3. net naming parity
- exporter-visible net names are only as good as the connection model

4. format-specific exporters
- only after the common exporter and XML/KiCad surfaces are locked

5. SPICE exporter
- explicitly last among exporters unless simulation parity becomes primary

### Current Exporter Backlog Status

What is already covered indirectly:
- duplicate sheet names check is implemented on the ERC side
- much of the hierarchy/current-sheet/variant groundwork is already present
- reduced XML component export is now live:
  - reduced `design` header
  - root source/date/tool
  - project text vars
  - reduced per-sheet title-block export
  - occurrence-aware component filtering
  - reduced multi-unit component collapse by reference
  - reference/value/footprint/datasheet/description export
  - reduced `libsource`
    - exercised schematic `lib_name` / non-`UseLibIdLookup()` branch is now live
    - exercised always-present `description=` attribute is now live, including empty descriptions
  - reduced component metadata properties:
    - symbol user fields now also emit on the `<property>` stream before sheet properties, matching
      the exercised `makeSymbols()` property walk more closely
    - `exclude_from_bom`
    - `exclude_from_board`
    - `exclude_from_pos_files`
    - `dnp`
      - exercised exclude-flag ordering now precedes `ki_keywords` / `ki_fp_filters`, matching
        KiCad's `makeSymbols()` property walk more closely
    - `ki_keywords`
    - `ki_fp_filters`
      - exercised blank filter entries are now skipped like KiCad's joined/trimmed export path
    - `duplicate_pin_numbers_are_jumpers`
    - `jumper_pin_groups`
      - exercised pin-name order now follows KiCad's sorted-set group ownership
    - component metadata properties now emit through one ordered property stream instead of
      repo-local write buckets
  - reduced component-local variant diffs on `<comp><variants>`
  - reduced KiCad-format root `<groups>` export
  - reduced KiCad-format root `<variants>` export
  - reduced `<component_classes>` export from symbol fields and enclosing rule-area directives
  - reduced `GNL_OPT_KICAD` board-mode filtering for symbols and net nodes
  - reduced parent-sheet `<property>` export on components
  - reduced multi-unit field/value merge by unit order
  - reduced component `<fields>` now mirrors the exercised `addSymbolFields()` slice:
    - user fields
    - lowest-unit empty user fields now stay authoritative instead of being skipped by repo-local
      nonblank filtering
    - canonical `Footprint`
    - canonical `Datasheet`
    - canonical `Description`
    - exercised component `<fields>` now emit before `<libsource>`, matching `addSymbolFields()`
      ownership inside `makeSymbols()` more closely
  - reduced `sheetpath names` / `tstamps` from loaded hierarchy paths
  - reduced `tstamps`
  - reduced per-lib-unit pin export on `<units>`
    - exercised linked library-unit order is now preserved instead of repo-local name sorting
  - exercised XML component child ordering now follows KiCad's `makeSymbols()` shape more closely
    through:
    - `<property>` / `<variants>` / jumper metadata before `<sheetpath>`
    - `<component_classes>` before `<tstamps>` / `<units>`
  - reduced `libparts`
  - reduced libpart pin lists from schematic-linked lib-symbol snapshots
    - exercised libpart field order now follows linked library-field order instead of repo-local
      key sorting
    - exercised full libpart field-list export is now live
    - exercised blank `<footprints><fp>` entries are now skipped
    - exercised pin `type` emission is now live
    - exercised libpart pin ordering now follows `StrNumCmp`
  - reduced root `<libraries>` section is now live after `<libparts>`
- reduced `nets`
- reduced node lists from the current point-net resolver
- reduced graph-side net grouping now flows through one shared `GetNetMap()` analogue before XML /
  KiCad export formatting
- XML/KiCad net export now also consumes that shared reduced `GetNetMap()` view directly instead of
  regrouping subgraphs inside the exporter
- XML net writing now also starts from the shared reduced whole-net owner instead of carrying a
  second exporter-local net regroup alongside it
- reduced project-wide net grouping now owns cross-sheet merge plus sorted netcode allocation
- reduced project-wide net grouping now assigns reduced whole-net codes by first seen full net
  name at the shared graph boundary instead of lexically sorting names before code allocation
- reduced project graph now also preserves per-sheet reduced subgraphs plus
  `FindSubgraphByName()` / `FindFirstSubgraphByName()`-style lookup boundaries instead of
  flattening directly to whole-net identities only
- reduced project `FindSubgraphByName()` lookup now keys by `(sheet instance path, resolved full
  net name)` like KiCad instead of the old repo-local short-driver key
- reduced project `FindSubgraphByName()` lookup now also preserves KiCad's same-sheet duplicate
  resolved-name list shape instead of collapsing `(sheet, name)` to one overwritten subgraph index
- reduced project `FindFirstSubgraphByName()` lookup now also preserves the exercised vector-bus
  `prefix[]` alias entries KiCad stores beside full resolved bus names
- reduced project graph now also exposes a shared `GetAllSubgraphs()`-style same-name lookup for
  ERC/export callers instead of forcing each caller to rebuild per-net neighbor maps
- reduced project graph now also keeps reduced item-to-subgraph identity for connection points and
  symbol pins instead of flattening those lookups straight to whole-net identity
- reduced project graph now also keeps reduced item-to-subgraph identity for labels and no-connect
  markers, so graph-owned ERC passes can start from shared item lookup instead of per-subgraph
  point membership recovery
- point/pin/label/no-connect net identity now also derives back through that shared subgraph owner instead of
  keeping duplicate item-to-whole-net side maps beside the shared subgraph indexes
- whole-net map views now also derive from the shared reduced subgraph owner instead of storing a
  second flattened project-net vector beside the same graph
- reduced project subgraphs now keep their own stable subgraph codes instead of reusing only the
  whole-net code space
- reduced project subgraphs now keep local driver names from the shared driver-selection owner
  instead of deriving them by stripping the full resolved net name
- shared reduced `driver_names` now also keep connected sheet-pin drivers instead of limiting the
  subgraph driver set to labels and power pins only
- shared reduced project subgraphs now also keep full-local driver names alongside display driver
  names, so bus-entry ERC can test reduced full-local bus members against the same shared driver
  owner instead of reconstructing path-qualified names inside ERC
- reduced project subgraphs now also keep strong-driver name sets for graph-owned ERC conflict
  consumers instead of forcing those callers back through per-sheet component scans
- reduced driver selection now also compares bus supersets/subsets through shared direct
  bus-member objects instead of reparsing flattened member strings at the ranking site
- reduced project graph now also keeps unnamed no-connect-only subgraphs instead of requiring a
  resolved net-map entry before a local subgraph can exist
- reduced project graph now also keeps unnamed wire/bus-entry-only local subgraphs instead of
  requiring a resolved net-map entry before graph-owned wire ERC can see them
- XML/KiCad net export now aggregates `nets` from the shared reduced subgraph owner instead of
  consuming only the already-flattened whole-net carrier
- XML net writing now also rebuilds write-time net records from shared reduced subgraphs in the
  same shape as KiCad `makeListOfNets()` instead of serializing the pre-flattened whole-net carrier
- XML net writing now also keeps KiCad's writer-owned net ordering/code assignment boundary instead
  of serializing shared graph codes directly
- shared whole-net map canonicalization now prefers user-named `(ref,pin)` ownership before final
  netcode assignment, so discarded duplicate-pin auto nets do not leave stale code gaps in export
- XML/KiCad net writing now also mirrors the exercised `makeListOfNets()` write-time `#...`
  power/virtual-symbol node filter, including skipped power-only nets without renumbering later
  emitted net codes
- XML net writing now also applies write-time node sort/dedup after subgraph grouping instead of
  relying only on pre-flattened net-map dedup

What is not yet explicitly tracked as complete:
- fuller KiCad/default `kicad` netlist CLI surface
  - exercised CLI default format/output path now follows KiCad's `KICADSEXPR` branch (`.net`)
  - exercised KiCad CLI format aliases now accept both `kicadsexpr` and `kicadxml`
  - exercised `--variant <name>` now applies the selected current variant before export
  - exercised duplicate-sheet-name warning now fires on the netlist command path before export
  - exercised annotation warning now fires on the netlist command path before export
    through the same occurrence/variant-aware symbol text ownership used by export
- exporter-base symbol/pin collection parity
- remaining XML/KiCad netlist structure parity is now narrower:
  - exercised blank component `<value>` now follows KiCad `addSymbolFields()` by emitting `~`
  - fuller graph-owned netcode/name ownership
- exporter-visible net naming parity
- format-specific text normalization parity beyond the now-live XML `StrNumCmp` component/net
  ordering
- SPICE exporter parity
- remaining XML net-node drift is now narrower:
  - fuller graph-owned netcode/name ownership

### Exporter-Specific Blockers

1. Net exports are still blocked on reduced/full connectivity work
- resolved net names
- connected nodes
- netclass-backed naming where relevant

2. SPICE export remains blocked on deferred sim-model parity

3. Some exporter-visible symbol/unit behavior may still expose occurrence/model reductions once
   exporter audits begin

4. Library/libpart export parity is blocked on the same missing symbol-library subsystem that
   blocks `ERC_TESTER::TestLibSymbolIssues()`
   - exercised root `<libraries>` section now exists, but URI-backed `<library>` child population
     remains blocked on that missing symbol-library subsystem

Current export unblock path:
1. add the CLI `netlist` command surface in upstream format order
   - reduced `xml` is now live
   - `kicad` remains the next format only if the common exporter base is still honest enough
   - other formats only after a common exporter base exists
2. build a reduced common exporter base on top of:
   - current occurrence-aware symbol state
   - current reduced connectivity/net-name snapshot
   - current duplicate-pin / multi-unit ERC groundwork
3. treat library/libpart export as blocked until the symbol-library subsystem exists
4. only after that add fuller net export and then format-specific writers

## Hierarchy / Loader Sign-Off Checklist

Hierarchy/loading should not be called 1:1 signed off until these are closed:

1. Reduced connectivity snapshot for current-sheet labels and pins
2. Connection-backed `NET_NAME` / `SHORT_NET_NAME` / `NET_CLASS`
3. Fuller hatch cache parity where current-screen refresh depends on it
4. Any remaining richer occurrence-owned state exposed by ERC

## Simulation Work

Simulation work is intentionally parked at the end of the backlog.

Why:
- it is not required for hierarchy loading
- it is not required for the next ERC cluster
- it is not required for core net naming

Return to simulation only after ERC-critical and net-naming-critical connectivity work is in place.

Simulation remaining blocker when resumed:
- fuller `CreateModel()`-style resolved simulator-model object layer
- project-backed / serialized-library `.kicad_sim` resolution
- deeper control/internal model families
- fuller IBIS waveform/driver semantics

## Blockers And Unblock Paths

### Blocker: Remaining ERC rules need connection ownership, not wire geometry only

Current local ERC can still fake some checks from geometry, but the next rules cannot honestly be
ported that way.

Unblock path:
1. add reduced current-sheet connection-point snapshot
   - done
2. use that snapshot for `TestFourWayJunction()`
   - done
3. use that snapshot for `TestNoConnectPins()`
   - done
4. group the same points into reduced connected components
   - done for the exercised ERC slice
5. resolve effective same-net ownership on those components
   - done for reduced ERC pin conflicts
6. expose that reduced ownership to shown-text and ERC
7. tighten fuller `TestPinToPin()` settings/subgraph exactness only if real KiCad divergence is
   found

### Blocker: Hierarchy loading is not yet fully 1:1 signed off

Operationally close is not the same as exact parity.

Unblock path:
1. complete reduced connectivity-backed shown-text
2. complete the remaining ERC-visible occurrence/current-sheet state
3. expand hatch/cache only if the exercised behavior depends on it
4. re-audit hierarchy/loading after the connectivity-backed ERC slice lands

### Blocker: Final parser wording polish

Parser correctness is no longer blocked. Only narrow local CLI wording fidelity remains.

Unblock path:
1. only revisit if a concrete mismatch is found against native KiCad wording
2. avoid reopening parser routine work for this unless evidence requires it

## Tracking Rules

When a real blocker is found:
1. record the blocker here
2. record the concrete unblock path here
3. do not treat the work as blocked until the path is written down

When a function is materially touched:
1. update the function comment in code with upstream mapping and divergence
2. if the touch changes backlog state, update this file in the same work unit

## Legacy File Roles

These files are no longer authoritative:

- `LOCAL_FUNCTION_PARITY_MAP.md`
  - compatibility pointer to this file
- `LOCAL_PARSER_PARITY_NOTES.md`
  - compatibility pointer to this file
- `LOCAL_PARSER_BFS_RECORD.md`
  - reduced parser-only coverage artifact only

## Current Bottom Line

The next real work is:
- reduced connectivity graph
- then the remaining connectivity-driven ERC rules

The next real non-goal is:
- simulation-model parity
