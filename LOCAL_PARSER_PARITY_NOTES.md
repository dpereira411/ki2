## Legacy Pointer

`LOCAL_PARSER_PARITY_NOTES.md` is no longer the authoritative backlog.

Use [PARITY_BACKLOG.md](/Users/Daniel/Desktop/modular/tools/ki2/PARITY_BACKLOG.md) instead.

This file remains only so older references do not break.
4. add ERC-facing regressions for:
   - symbol variant `dnp` / `in_bom` / `on_board` / `in_pos_files`
   - sheet variant `exclude_from_sim`
   - variant field overrides on the selected occurrence
   - done for symbol-side selected-occurrence refresh
   - done for sheet-side selected-occurrence refresh
5. only after that, reopen `UpdateSymbolInstanceData`, `UpdateSheetInstanceData`, and
   `UpdateAllScreenReferences` for branch-level parity tightening
   - done for the current architecture: `SchematicProject` now carries `current_variant` too and
     shares the same variant-aware symbol/sheet occurrence refresh path as `LoadResult`
   - remaining active gap is now narrower:
     - any broader ERC semantics that need variant-aware sheet state beyond the current model

Upstream re-audit against `/Users/Daniel/Desktop/kicad/eeschema/schematic.cpp` corrected one
earlier assumption:

- `SCHEMATIC::GetCurrentVariant()` / `SetCurrentVariant()` are schematic-owned session state on
  `m_currentVariant`
- they are not sourced from `.kicad_pro` / `.kicad_prl`
- the local `LoadResult.current_variant` setter is therefore already the right kind of analogue for
  current-variant selection in the current architecture

What remains after that correction:

- current-sheet switching also needed to refresh live sheet variant state on reused child screens
  when the selected occurrence changes
  - done: `set_current_sheet_path()` now refreshes live sheet variants as well as symbol variants
    and page state
- direct upstream re-audit of `SCHEMATIC::RecomputeIntersheetRefs()` and
  `SCH_SHEET_PATH::UpdateAllScreenReferences()` exposed the next ERC-visible model gap:
  - KiCad rebuilds the page-ref map across the whole hierarchy, but only refreshes visible
    intersheet-ref field state on the current sheet
  - the current Rust model instead bakes resolved intersheet-ref text directly onto every global
    label property across all loaded schematics
  - done: page-ref computation now stays hierarchy-wide while resolved intersheet-ref text is only
    applied on the selected sheet
  - done: current-sheet visibility now also honors companion `.kicad_pro`
    `drawing.intersheets_ref_show` when that project setting is present
  - done: current-sheet page-list text now also honors companion `.kicad_pro`
    `drawing.intersheets_ref_own_page`, so the selected page can be excluded from the displayed
    intersheet-ref list like upstream
  - done: current-sheet intersheet-ref text now also honors companion `.kicad_pro`
    `drawing.intersheets_ref_short`, `drawing.intersheets_ref_prefix`, and
    `drawing.intersheets_ref_suffix`
  - done: the no-project fallback now uses KiCad's current default schematic-settings values for
    intersheet refs too:
    - `show = false`
    - `own_page = true`
    - `short = false`
    - `prefix = "["`
    - `suffix = "]"`
  - remaining narrower drift under the same routine:
    - done for the exercised ERC slice: hierarchy-wide page-ref recompute and current-sheet
      intersheet-ref display now use a reduced sheet-path shown-text resolver for global labels
      instead of raw `label.text`
      - current coverage now locks reused child schematics with `${SHEETNAME}`-backed global
        labels so page-ref grouping no longer collapses by raw text
      - current coverage now also locks variant-sensitive `${DNP}` grouping on reused child
        schematics, and `set_current_variant(...)` now recomputes the page-ref map before current
        sheet intersheet refresh
      - current coverage now also locks schematic-level `${TITLE}` and `${VARIANT}` grouping in
        the reduced resolver
      - current coverage now also locks project-level `${PROJECTNAME}` and companion
        `.kicad_pro` `text_variables` grouping in the reduced resolver
      - current coverage now also locks project-level `${CURRENT_DATE}` grouping in the reduced
        resolver
      - current coverage now also locks project-level `${VCSHASH}` / `${VCSSHORTHASH}` grouping
        through a reduced git-backed project resolver
      - current coverage now also locks schematic-level `${FILENAME}` / `${FILEPATH}` grouping in
        the reduced resolver
      - current coverage now also locks project-backed `${VARIANT_DESC}` grouping through typed
        `.kicad_pro` schematic variant descriptions
      - current coverage now also locks global-label `${CONNECTION_TYPE}` grouping from label
        shape without needing the blocked connectivity graph
      - current coverage now also locks reduced `${ref:FIELD[:VARIANT]}` symbol/sheet lookup
        across loaded sheet paths, including parent-reference fallback like `R1 -> R1A` and
        symbol-side unknown/unresolved markers
      - current coverage now also locks a reduced current-sheet connectivity slice for
        wire-connected labels:
        - `${NET_NAME}`
        - `${SHORT_NET_NAME}`
        - connected-directive `${NET_CLASS}` with shown-text-resolved `Netclass` fields
        - rule-area-backed `${NET_CLASS}` from directive labels inside the exercised rule polygon
        - reused-sheet grouping via `${SHEETNAME}`-resolved local labels
      - remaining divergence is the broader unported text-variable resolver surface
        (`ResolveTextVar` and fuller connection-graph semantics), not this exercised intersheet-ref
        path
      - unblock path for the remaining text-variable surface:
        1. expand the reduced current-sheet connectivity snapshot from wire/rule-area geometry to
           cached connection ownership so labels can follow KiCad's real `SCH_ITEM::Connection()`
           / `GetEffectiveNetClass()` precedence
        2. keep threading that snapshot through the shared shown-text resolver as the local
           analogue for `SCH_ITEM::Connection()` / `SCH_LABEL_BASE::ResolveTextVar()`
        3. expand the reduced cross-reference resolver only if a future ERC-visible gap proves a
           still-missing KiCad branch
        4. lock the remaining connectivity slice with focused precedence regressions if current
           ERC-visible behavior ever depends on exact connection-graph ordering
    - done for the exercised intersheet-ref subset: loader/project refresh now read one typed
      `ActiveSchematicSettings` carrier instead of scattered raw `.kicad_pro` scalar lookups
    - the current Rust tree still lacks KiCad's fuller schematic-settings/config surface beyond
      that typed intersheet-ref subset, so broader user-config-driven overrides are not modeled
    - done for the exercised refresh branch: current-sheet refresh now materializes a reduced hatch
      cache on selected-screen schematic shapes instead of leaving `UpdateHatching()` as a no-op
      - the reduced line-cache path now also clips 45-degree hatch segments across the full current
        bounding box instead of the earlier truncated half-box coverage
    - ERC work is now started in-tree:
      - reduced local `ERC_TESTER::TestDuplicateSheetNames()` analogue is implemented and tested
        for same-screen case-insensitive sheet-name collisions
      - reduced local `ERC_TESTER::TestFieldNameWhitespace()` analogue is implemented and tested
        for symbol and sheet fields
      - reduced local `ERC_TESTER::TestMultiunitFootprints()` analogue is implemented and tested
        for same-reference footprint mismatches across placed units
      - reduced local `ERC_TESTER::TestMissingUnits()` analogue is implemented and tested against
        linked library unit counts
      - reduced local `ERC_TESTER::TestMissingNetclasses()` analogue is implemented and tested
        against typed companion-project `net_settings.classes`
      - reduced local `ERC_TESTER::TestTextVars()` analogue is now implemented and tested for the
        exercised loaded-text surfaces:
        - symbol fields
        - linked library text / text boxes on placed symbols
        - label fields
        - sheet fields
        - sheet pins
        - top-level schematic text
        - top-level text boxes
        - `${ERC_WARNING...}` / `${ERC_ERROR...}` assertion markers on those same item families,
          including linked library text / text boxes
      - unblock path there is narrower than before because the reduced shown-text path now covers
        the exercised sheet/project/cross-reference/current-sheet connectivity tokens
      - remaining likely blockers for fuller `TestTextVars()` parity are:
        - drawing-sheet text coverage
      - next upstream ERC blocker cluster is the first connection-point routines:
        - `TestFourWayJunction()`
        - `TestNoConnectPins()`
        - `TestPinToPin()`
      - reduced local `ERC_TESTER::TestLabelMultipleWires()` analogue is now implemented and
        tested on the reduced wire-segment geometry path
      - direct upstream audit shows those routines all depend on real per-item connection-point
        enumeration across pins, labels, wires, junctions, and no-connect markers
      - concrete unblock path:
        1. add a reduced current-sheet connection-point snapshot keyed by loaded sheet path and XY
        2. feed it from symbol pins, labels, wire endpoints/midpoints, junctions, and no-connects
        3. port `TestFourWayJunction()` and `TestNoConnectPins()` onto that shared snapshot before
           attempting the fuller `TestPinToPin()` conflict matrix
      - the reduced cache now also clips circle hatch lines to real circle geometry instead of the
        earlier bounding-box fallback
      - the reduced cache now also respects parsed rectangle corner radius instead of running
        hatch lines through rounded-corner cutouts
    - remaining shape drift is narrower:
      - the current Rust shape model still lacks KiCad's fuller polygon/knockout hatch cache
      - hatch geometry is still reduced to cached line segments plus partial analytic clipping, not
        full `SHAPE_POLY_SET` parity
    - treat the remaining hatch gap as geometry/cache expansion work, not as another branch tweak
      in `loader.rs`
    - typed-settings unblock path completed for current-sheet intersheet refs:
      1. a typed settings carrier now exists
      2. load/project refresh now source intersheet-ref display settings from that typed layer
      3. `refresh_current_sheet_intersheet_refs()` no longer takes ad-hoc scalar setting args
      4. focused companion-project regressions now lock the typed intersheet settings
    - shape-hatching unblock path partially completed:
      1. `Shape` now carries dirty hatch state plus cached hatch lines
      2. current-sheet refresh now calls a local `update_hatching()` analogue on selected-screen
         schematic shapes
      3. focused regression locks that selected-screen hatch refresh
      4. what remains is fuller polygon/knockout cache parity beyond the current line-cache model
- the remaining ERC drift is no longer a variant-source blocker
- it is back to richer occurrence-aware model coverage beyond the current symbol/sheet state

Separate non-blocking infrastructure note:

- the current tree now has minimal companion `.kicad_pro` / `.kicad_prl` loaders that preserve raw
  JSON on the loaded result
- this is useful for future ERC/project-settings work, but it is not the source of
  `SCHEMATIC::GetCurrentVariant()` parity

Until that model exists, the remaining loader drift should be treated as blocked rather than as an
unfound branch mismatch in `UpdateSymbolInstanceData`, `UpdateSheetInstanceData`, or
`UpdateAllScreenReferences`.
- `FixLegacyPowerSymbolMismatches`
  Status: loader-side legacy power-value fix now follows the placed symbol's own linked lib pins more closely instead of a screen-level first-pin summary, so unit/body-style-specific global-power symbols now repair against the active lib pin like upstream. It still marks the screen modified only when the value actually changes, and explicitly leaves local-power or visible-pin symbols untouched. Direct re-audit did not find another model-visible mismatch in the current symbol/lib-pin representation; treat any remaining drift here as blocked on fuller lib-pin/screen semantics beyond the current model.
- `MigrateSimModels`
  Status: loader-side migration now covers the upstream-representable slices the current model can express:
  1. the early-return branch that rewrites mid-v7 field spellings (`Sim_Device`, `Sim_Type`, `Sim_Params`, `Sim_Pins`) and converts `Sim_Pins` index arrays into `Sim.Pins` name-value maps when source pins are available
  2. the explicit legacy `Spice_*` raw-SPICE fallback branch that removes `Spice_Primitive` / `Spice_Model` / `Spice_Node_Sequence` / `Spice_Lib_File` fields and synthesizes `Sim.Device=SPICE`, `Sim.Params`, and `Sim.Pins`
  3. the inferred legacy passive/source branch where `Spice_Primitive` matches the symbol prefix and the existing `Value` field remains the model source, while legacy `Spice_Node_Sequence` is still migrated into `Sim.Pins`
  4. the explicit legacy `V` / `I` DC-source branch where `Spice_Model` like `dc(1)` becomes `Sim.Device`, `Sim.Type=DC`, migrated `Sim.Pins`, and an updated `Value` field
  5. the explicit legacy `V` / `I` built-in source branches where `Spice_Model` like `sin(...)`, `pulse(...)`, `exp(...)`, `am(...)`, `sffm(...)`, `pwl(...)`, `whitenoise(...)`, `pinknoise(...)`, `burstnoise(...)`, `randuniform(...)`, `randgaussian(...)`, `randexp(...)`, and `randpoisson(...)` becomes `Sim.Device`, `Sim.Type`, named `Sim.Params`, and migrated/defaulted `Sim.Pins`
  6. raw `Spice_Lib_File` fallback now has explicit coverage for escaped-model parameter formatting and default `Sim.Pins` synthesis from lib pins when `Spice_Node_Sequence` is absent
  7. legacy `Spice_Node_Sequence` and `dc(...)` parsing now accept full decoded whitespace, not only plain spaces
  8. raw `Spice_*` fallback parameter formatting is now locked across rich, model-only, and lib-only inputs instead of only the fully populated branch
  9. remaining simple representable migration branches are now locked too: comma-separated legacy source models still parse, and primitive-only junk fields correctly do not migrate
  10. legacy helper exactness is now locked too: mixed-case `dc` / source-model kinds still migrate, and punctuation-heavy `Spice_Node_Sequence` payloads still decode into `Sim.Pins`
  11. default `Sim.Pins` synthesis is now locked too: source pins are filtered by the active symbol unit and ordered numerically before the migration writes the name-value map
  12. migrated and already-modern `Sim.*` field state now also hydrates a structured `Symbol.sim_model` snapshot during load, including `Sim.Library`, `Sim.Name`, `Sim.Ibis.Pin`, `Sim.Ibis.Model`, parsed `Sim.Params` name/value maps, and `Sim.Pins` mappings, so the remaining blocked simulator-model work is no longer forced to live only as flat property strings
  13. the raw `Spice_*` fallback branches are now also locked against that structured snapshot, not only flat migrated properties, across model-only, lib-only, primitive+lib, and lib-pin-backed migration cases
  14. the currently representable inferred-value branch is now also locked on the structured snapshot: it stays side-effect-light (`Sim.Device` / `Sim.Params` absent) while still hydrating migrated `Sim.Pins`
  15. the structured snapshot now also derives library-backed state from raw `Sim.Params` payloads when explicit `Sim.Type` / `Sim.Library` / `Sim.Name` fields are absent, so the current model can carry more of the raw-SPICE library/model branch without inventing extra migrated properties
  16. default and migrated `Sim.Pins` synthesis is now also locked on the structured snapshot, not only the flat property text, across active-unit defaults, numeric sorting, and decoded-whitespace migration
  17. the remaining representable built-in source family is now also locked on the structured snapshot more broadly, not only on isolated branches: `SIN`, `PWL`, `EXP`, `AM`, and `SFFM` all now prove device/type/params/pins state directly on `Symbol.sim_model`
  18. the residual representable source exactness branches are now also locked on the structured snapshot: `DC`, whitespace/mixed-case source parsing, `TRNOISE`, and `TRRANDOM` all prove device/type/params state directly on `Symbol.sim_model`
  19. the remaining representable migration exactness variants are now locked too: comma-separated source payloads prove the same structured `Symbol.sim_model` state as whitespace-separated forms, and primitive-only junk legacy fields still leave both migrated `Sim.*` properties and `sim_model` absent
  20. the last representable pin-map exactness variants are now locked on the structured snapshot too: defaulted source pin maps and punctuated legacy node-sequence decoding now prove the same `sim_model.pins` state as the flat migrated `Sim.Pins` property
  21. `modelFromValueField` behavior is now covered across the representable migration branches too: when legacy `Spice_Model` is absent but `Value` supplies the model text, raw-SPICE fallback now uses that text and rewrites `Value` to `${SIM.PARAMS}`, and value-backed `V`/`I` source migration now also covers the non-inferred `DC`/built-in-source branch instead of silently dropping back to the repo-local no-op path
  22. raw legacy library-model fallback now also follows the upstream name split more closely: when `Spice_Lib_File` is present, inline trailing parameters after the model name are stripped before the unresolved raw-SPICE fallback records `model="..."` in `Sim.Params`
  23. the loader now carries resolver-backed library metadata on `Symbol.sim_model` instead of dropping it after field sync: resolved library source (filesystem vs schematic-embedded vs symbol-embedded), library kind (SPICE vs IBIS), resolved model name, generated model-pin names, and generated parameter pairs all survive on the loaded symbol state
  24. embedded model content is no longer reconstructed from token payloads only; parser-side `embedded_files -> data` now preserves raw source whitespace between `| ... |`, which unblocks real embedded SPICE/IBIS model inspection instead of a whitespace-stripped pseudo-content path
  25. the first concrete resolver-backed model parsing layer now exists for the currently representable library-backed branches:
     - SPICE `.subckt` entries resolve selected model name, generated pin names, and declared `PARAMS:` defaults
     - SPICE `.model` entries resolve selected model name, model type, and parameter defaults
     - SPICE `.include` chains now recurse across embedded and filesystem libraries using relative-path resolution from the owning library source
     - IBIS component files resolve selected component name, available pin identifiers, the selected pin's model name when that pin/model state exists on the symbol, and `[Diff Pin]` partner metadata for the selected pin
  26. loader-side resolved metadata now also refines the coarse origin heuristic when the resolver proves an IBIS file: even without explicit `Sim.Ibis.*` fields, a resolved `.ibs` library now tags the loaded symbol as `Ibis` instead of leaving it on the generic library-reference path
  27. the library-backed loader branches now consume that resolver state:
     - already-modern and migrated library-backed symbols default `Sim.Pins` from resolved model pins when a compatible model signature exists
     - unresolved library-backed symbols still fall back to KiCad-style numeric/default pin mapping from the active symbol unit
     - raw `Spice_Lib_File` migration is now tagged as library-backed (`LibraryReference`) once a real library identity exists, instead of staying collapsed to plain raw SPICE
  28. structured sim-field exactness is now closer to KiCad's serializer too:
     - `Sim.Params` preserves flag parameters as `name=1`
     - quoted `Sim.Params` payloads preserve spaces and escaped quotes
     - quoted `Sim.Pins` model-pin names preserve spaces instead of being split on whitespace
     - legacy `Sim.Enable` now follows KiCad `ParseEnable()` disabling semantics for leading `0` / `f` / `n`, not only literal `"0"`
  29. the SPICE library resolver is now statement-shaped instead of whole-file token-shaped:
     - `.model` parsing no longer leaks later directives/comments into the selected model's params
     - `+` continuation lines are joined onto the owning `.model` / `.subckt` statement
     - `.include` resolution is statement-based and mixed-case tolerant
  30. library-backed model-name lookup now also follows KiCad's split-before-resolve flow more closely:
     - modern and migrated `Sim.Name` payloads are stripped at the first literal space before SPICE/IBIS library lookup instead of trying to resolve the whole inline-params string as the model name
  31. legacy `Spice_Lib_File` migration now also follows KiCad's library-model-first branch more closely when the current resolver can actually resolve the target model:
     - migrated legacy library models now produce `Sim.Library` + `Sim.Name` instead of falling straight to raw `Sim.Device=SPICE`
     - inline trailing parameters after the legacy model name now stay on `Sim.Params` in that resolved library-model branch
     - value-backed library models now rewrite `Value` to `${SIM.NAME}` instead of `${SIM.PARAMS}` when the library-backed migration succeeds
     - default `Sim.Pins` synthesis in that branch now comes from resolved model pins where available, with the existing fallback path preserved when the library model cannot be resolved
  32. the already-current mid-v7 `Sim_*` branch now also carries KiCad's compatibility fixups instead of only renaming fields:
     - `POT` pin maps remap legacy `+` / `-` model pins to `r1` / `r0`
     - `RANDNORMAL` rewrites to `RANDGAUSSIAN`, lowercases params, drops stale `min=0` / `max=0`, and renames `dt` to `ts`
     - `MUTUAL` no longer survives as a misleading subtype on inductors; it collapses to `Sim.Device=K` and drops `Sim.Type`
  33. resolved IBIS metadata now carries the first real `CreateModel()`-style type-override slice too:
     - explicit library-backed `Sim.Type=DCDRIVER|RECTDRIVER|PRBSDRIVER|DEVICE` now overrides the coarse resolved IBIS family instead of collapsing every resolved `.ibs` model to the same generic component kind
  34. simulation-library path resolution now also follows KiCad's `ResolveLibraryPath()` environment-expansion step more closely:
     - `$VAR/...` and `${VAR}/...` segments inside `Sim.Library` are expanded before project-relative and `SPICE_LIB_DIR` fallback lookup
  35. loader-side simulation-library failures no longer disappear silently:
     - missing `Sim.Name`, missing simulation-library content, and missing base-model lookups now survive on `screen.parse_warnings` with KiCad-shaped reporter text instead of being dropped after load
     - missing relative simulation libraries now also report the fuller KiCad fallback location set when project-relative and `SPICE_LIB_DIR` search paths differ, instead of collapsing that warning to one filesystem candidate
  36. current-field built-in sim mismatches now also report through load:
     - non-library `Sim.Device` / `Sim.Type` pairs that do not define a real built-in model now surface KiCad-style `No simulation model definition found...` warnings instead of silently surviving as opaque field state
     - that validity check now follows the broader upstream `TypeInfo()` family too, so valid behavioral/source/switch/transmission-line current fields are not warned spuriously
     - explicit current-field transistor/FET families such as `NPN/VBIC`, `NPN/HICUM2`, and `NMOS/BSIM3` are now treated as valid too instead of being misreported as missing models
     - structured `sim_model.origin` now keys off that same supported built-in table, so valid empty-subtype built-ins like `E/F/G/H/SUBCKT/XSPICE` no longer stay collapsed to generic field state and invalid pairs no longer masquerade as built-in models
  37. the loader now also carries the representable `CreateModel()` inference snapshot even when no explicit `Sim.*` fields exist:
     - two-pin passive/source symbols can now hydrate structured `sim_model` state from `Value` + symbol prefix (`R/L/C/V/I`) without writing new fields
     - that also keeps the legacy inferred migration branch coherent when only `Sim.Pins` survives but the model still comes from `Value`
     - explicit current/library-backed `Sim.Device=R/C/L/V/I` models now also infer value-backed params, stored-value binding, and current/source default pin maps before the generic resolved-model pin fallback runs, matching upstream `CreateModel()` timing more closely
  38. direct `Sim.Params` loading now follows the shared serializer-style parser path instead of a loader-local whitespace split:
     - explicit schematic `Sim.Params` preserve quoted values and flags consistently with `Symbol::sync_sim_model_from_properties()`
     - SI-style numeric parameter payloads like `1Meg` and `3,300u` now normalize on that shared path instead of only in the inferred-value branches
  39. explicit modern current-source field values now follow upstream `TypeInfo()` more closely instead of only the collapsed migrated names:
     - `WHITENOISE`, `PINKNOISE`, `BURSTNOISE`, `RANDUNIFORM`, `RANDGAUSSIAN`, `RANDEXP`, and `RANDPOISSON` now count as valid built-in `Sim.Type` values for current `V/I` fields
     - those explicit modern names now also trigger the same default internal-source pin-map synthesis as `SIN` / `PULSE` / `EXP` / `AM` / `SFFM` / `PWL`
  40. legacy source migration now also writes the same public `Sim.Type` field values KiCad exposes through `TypeInfo()` instead of collapsing everything to the lower-level SPICE function families:
     - `whitenoise(...)`, `pinknoise(...)`, and `burstnoise(...)` now migrate to `Sim.Type=WHITENOISE|PINKNOISE|BURSTNOISE`
     - `randuniform(...)`, `randgaussian(...)`, `randexp(...)`, and `randpoisson(...)` now migrate to `Sim.Type=RANDUNIFORM|RANDGAUSSIAN|RANDEXP|RANDPOISSON`
  41. explicit current/source `Sim.*` field loading now follows the upstream `ReadDataFields()` fallback boundary more closely:
     - when `Sim.Params` does not provide the primary parameter for the current representable built-in slice, loader-side structured hydration now falls back to `Value` instead of silently dropping that primary payload
     - the current explicit `V/I` built-in branch now covers this for `DC`, `SIN`, `PULSE`, `EXP`, `AM`, `SFFM`, and the supported noise/random families, preserving stored-value binding while leaving the original schematic fields untouched
  Net effect: the currently representable `MigrateSimModels` surface is now locked more broadly on structured, resolver-backed `Symbol.sim_model` state, not only on flat migrated properties. The remaining backlog here is no longer source lookup or basic model-entry parsing; it is the still-unported deeper library/project/control-model side.
  Model expansion in progress: `SimModel` now preserves ordered parameter payloads, ordered pin mappings, `Value`-field placeholder binding (`${SIM.PARAMS}` / `${SIM.NAME}`), enabled/disabled state, coarse model origin tagging (raw SPICE vs built-in/internal vs explicit library-reference vs IBIS), resolved library source/kind identity, resolved model family/class (`SpiceModel` vs `SpiceSubckt` vs `IbisComponent`), resolved model type, selected IBIS `Model_type` (from either explicit `Sim.Ibis.Model` or the chosen pin row), resolved IBIS differential-pin metadata, generated model-pin metadata, and generated parameter defaults instead of collapsing that state away immediately. The tree also now has explicit sim-library source-stack, content-loading, filesystem path-resolution, and first-pass SPICE/IBIS content resolution helpers for loaded symbols.
  Remaining blocked gap: the heavier simulator-model / project / embedded-model branch that still depends on fuller `CreateModel()`-style behavior beyond the now-broader representable slices: project-backed or serialized-library model resolution beyond the current SPICE-entry/include/statement and typed resolved-model-family layer, deeper control/internal model families beyond the current `DC/SIN/PULSE/EXP/AM/SFFM/PWL/TRNOISE/TRRANDOM` slice, fuller IBIS waveform/driver semantics beyond selected pin/model/`Model_type`/diff metadata, and any unresolved `Spice_*` inference paths that require richer resolved-model objects than the current metadata carrier. Do not fake that remaining stage without first expanding the Rust model beyond metadata snapshots into fuller resolved simulator-model ownership.
  Concrete unblock requirements:
  - move from typed metadata resolution to fuller resolved simulator-model objects that can express generated pins, parameters, and model semantics the way KiCad `CreateModel()` does
  - extend the current resolver beyond first-pass SPICE/IBIS entry discovery and recursive SPICE include resolution into fuller `.kicad_sim` / embedded project-model behavior and deeper model-family parsing
  - only after that fuller resolver/model layer exists should the remaining library-backed `MigrateSimModels` branch, control/internal model families, and unresolved `Spice_*` inference paths be ported literally

### More Exact Current Priority

Primary goal has changed from full simulation parity to ERC-critical loader parity.

Working strategy has changed with that goal:
- parser parity was driven bottom-up by dependency because helper/token/ownership leaves controlled
  parent correctness
- loader/ERC parity should now be driven in upstream pipeline order, function by function, because
  the remaining risk is in loaded-screen state transitions rather than missing parser leaves

1. Keep simulation-model work at the end of the backlog. It is not a prerequisite for hierarchy
   loading, current-sheet semantics, intersheet references, or core ERC-visible symbol/sheet state.
2. Re-open loader/post-load branches that materially affect ERC parity first:
   - `UpdateAllScreenReferences`
   - `UpdateSymbolInstanceData`
   - `UpdateSheetInstanceData`
   - `SetSheetNumberAndCount`
   - `RecomputeIntersheetRefs`
   - `FixLegacyPowerSymbolMismatches`
3. Only reopen parser blocked surfaces if ERC-driven loader work exposes a concrete parser/state
   mismatch.
4. Leave the remaining `MigrateSimModels` branch parked until the ERC-critical queue is exhausted or
   we explicitly decide to build the fuller resolved simulator-model layer.

### Recommended Next Order

1. direct upstream re-audit of `UpdateAllScreenReferences` for remaining ERC-visible reused-screen /
   current-occurrence drift
   - done for the currently representable symbol/sheet occurrence and current-sheet intersheet-ref
     paths
   - remaining drift under this routine is typed schematic-settings coverage beyond companion
     project JSON plus shape-hatching refresh state
2. direct upstream re-audit of `UpdateSymbolInstanceData` and `UpdateSheetInstanceData` for any
   remaining symbol/sheet occurrence mismatches that affect ERC-visible state
   - done for the currently representable empty-payload, selected-occurrence, and current-variant
     paths
   - remaining drift is model-shaped rather than another obvious branch mismatch
3. revisit `SetSheetNumberAndCount` / `RecomputeIntersheetRefs` only when a concrete current-sheet,
   page-state, or intersheet-reference discrepancy appears
   - current hierarchy/current-sheet split looks structurally covered in the present model
4. revisit `FixLegacyPowerSymbolMismatches` only when a concrete lib-pin/screen mismatch appears
5. keep the remaining `MigrateSimModels` branch at the end of the backlog as non-ERC simulation
   parity work

### Bottom Line

The parser-only layer is effectively exhausted in the current model, but two blocked surfaces still
prevent a literal claim of perfect parser parity:

- UUID semantics blocked on fixture/model migration
- diagnostic / error parity blocked on error-model expansion and formatting audit

The active executable backlog is now ERC-critical loader/post-load parity. The remaining
`MigrateSimModels` / resolved simulator-model branch is intentionally deferred to the end of the
backlog because it is simulation-facing, not hierarchy/ERC-critical.
