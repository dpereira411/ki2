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
- ERC parity first
- net naming parity second
- simulation-model parity last

Secondary product goal:
- expose a CLI-facing API surface that behaves like `kicad-cli` for the exercised schematic paths

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

Still pending for ERC:
- `ERC_TESTER::TestFourWayJunction()`
- `ERC_TESTER::TestNoConnectPins()`
- `ERC_TESTER::TestPinToPin()`
- remaining drawing-sheet slice of `ERC_TESTER::TestTextVars()`

### Simulation

Simulation-model migration/resolution is no longer on the critical path.

Simulation parity is explicitly deferred to the end of the backlog unless it blocks ERC or net
naming, which it currently does not.

## Single Active Queue

Work this list from top to bottom unless direct upstream comparison reveals a real prerequisite.

1. Reduced current-sheet connectivity snapshot
2. `NET_NAME` / `SHORT_NET_NAME` / `NET_CLASS` connection-backed parity
3. `ERC_TESTER::TestFourWayJunction()`
4. `ERC_TESTER::TestNoConnectPins()`
5. `ERC_TESTER::TestPinToPin()`
6. Remaining drawing-sheet `TestTextVars()` coverage
7. Hierarchy/loading 1:1 sign-off gaps
8. Final parser diagnostic wording polish
9. Simulation-model parity last

## Connectivity Graph Requirements

### Why This Exists

The remaining ERC and net-naming work is blocked not on parser work, but on missing connectivity
state.

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
   - four-way junctions
   - no-connect pins
   - pin-to-pin conflicts

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

4. groups / variants / component class parity
- upstream XML exporter includes:
  - groups
  - variants
  - component class aggregation

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
- stable pin ordering and duplicate-pin handling
- format-specific text normalization/sorting

These are downstream from the common exporter base plus net naming.

### Export Parity Queue

Do not treat exporter parity as complete until all of these have been audited explicitly:

1. common exporter base
- symbol filtering
- multi-unit collection
- duplicate-pin erasure

2. XML / KiCad netlist exporter
- symbols
- libraries/libparts
- nets
- variants/groups/component classes

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

What is not yet explicitly tracked as complete:
- exporter-base symbol/pin collection parity
- XML/KiCad netlist structure parity
- exporter-visible net naming parity
- format-specific sorting/text normalization parity
- SPICE exporter parity

### Exporter-Specific Blockers

1. Net exports are still blocked on reduced/full connectivity work
- resolved net names
- connected nodes
- netclass-backed naming where relevant

2. SPICE export remains blocked on deferred sim-model parity

3. Some exporter-visible symbol/unit behavior may still expose occurrence/model reductions once
   exporter audits begin

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
2. group it into reduced connected components
3. resolve effective driver / net naming on those components
4. expose connection lookups to shown-text and ERC
5. port `TestFourWayJunction()`
6. port `TestNoConnectPins()`
7. port `TestPinToPin()`

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
