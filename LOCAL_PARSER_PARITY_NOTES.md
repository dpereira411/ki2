## Parser Parity Notes

Target: true 1:1 structural and code-flow parity with upstream KiCad schematic parsing, not just behavioral similarity.

### Current State

Parser-only work is effectively exhausted in the current model.

The remaining parser-side gap is now a narrow formatting surface, not broad routine work:

- final native-style diagnostic / error wording polish

Active parity work is now in the loader / post-load pipeline.

### UUID Unblock Status

The UUID unblock is complete.

Done:

1. migrated parser-only and loader-path fixtures away from stable symbolic fake UUIDs
2. rewrote the remaining expectations toward:
   - valid UUID shape
   - legacy short-hex normalization
   - uniqueness on creation sites
3. enabled native-style malformed-ID replacement semantics in `parse_kiid` / `normalize_kiid`

The parser-only blocked set no longer includes malformed UUID handling.

### Diagnostic / Error-Model Unblock Plan

The diagnostic block is now a support-model expansion task, not broad parser-routine work.

To unblock native KiCad parse-diagnostic parity:

1. inventory the exact parser surfaces that currently collapse source fidelity
2. expand the local diagnostic model so it can carry the fields the parser already knows at the
   failure site instead of flattening them immediately into a reduced message:
   - path
   - span
   - line / column or byte-offset-derived source position
   - raw expectation / unexpected-token payload
   - enough context to distinguish parser failures from validation failures in formatting
3. move parser helpers onto structured diagnostic construction first, without trying to force exact
   final wording in the same patch
4. add fixture coverage for representative exactness buckets:
   - `Expecting(...)` parse failures
   - `Unexpected(...)` parse failures
   - malformed-number / malformed-bool branches
   - validation failures that currently lose source/location fidelity
5. only then tighten final `Display` / formatting behavior to match native KiCad error text as far
   as the local CLI and tests can support

Execution order:

1. audit current `src/error.rs` / `src/diagnostic.rs` formatting and document the missing fields
   - done
2. expand the diagnostic model and thread structured data through parser helper construction
   - done: structured diagnostic kinds now distinguish generic validation, `expecting`, and
     `unexpected` parser failures, and diagnostics now carry byte-span plus 1-based line/column
     source positions
3. lock parser-helper exactness with focused tests before touching broad wording
   - done: parser-helper kind regressions are in place
4. tighten final `Display` formatting and wording polish
   - in progress: parser-built diagnostics now render with the same `parse error at ...` prefix
     shape as lexer failures instead of the older local `validation error at ...` split
   - remaining active diagnostic task is now narrower final wording polish after that shared prefix
     unification
5. re-audit blocked parser helpers in `LOCAL_FUNCTION_PARITY_MAP.md`

Closest-to-upstream areas so far:

- `parsePAGE_INFO()` / top-level `paper` / modern `page`
- `parseTITLE_BLOCK()` is improving
- high-level schematic dispatch is more KiCad-shaped than before
- several legacy/version branches are already encoded

### Major Remaining Gaps

1. Top-level `ParseSchematic()` broad owner flow is no longer one of the active parser-only bottlenecks

- direct re-audit shows the broad top-level entry/dispatch flow is now structurally close enough:
  header/version handling, `generator_version` future-version timing, old `page` remap,
  embedded-files recovery, and literal top-level fallback text are no longer the active gaps
- the remaining parser-only risk is now narrower token/diagnostic exactness rather than a large
  missing owner-routine mismatch at the top level

2. Shared text/effects parsing is no longer one of the main parser-only bottlenecks

- `parseSchText()`
- `parseEDA_TEXT()` callers
- label / text / text_box / directive / netclass / global / hierarchical branches
- effects / justify / hide / hyperlink semantics still need more exact control-flow parity

Direct re-audit shows `parseSchText()` itself is no longer an active parser-only bottleneck:

- the shared text-family object-construction loop, `shape` / `length` / `iref` / `property`
  ownership, and final fieldless-label autoplacement behavior are now structurally close to upstream
- `parseEDA_TEXT()` itself is now much tighter after direct-href entry, bare-head, and native
  hyperlink-validation fixes
- the remaining text-family risk is now mostly parser-wide token/error exactness around shared
  text/effects branches, not a broad owner-routine mismatch in `parseSchText()` itself

3. Property / field parsing is no longer one of the active parser-only bottlenecks

- direct re-audit shows both `parseSchField()` and library `parseProperty()` are now structurally
  close enough that remaining parser-only risk is narrower support exactness rather than a broad
  field/property routine mismatch
- keep revisiting them only if a later support-file or diagnostic exactness check exposes a
  concrete behavioral drift

4. Symbol parsing is still not 1:1

- direct comparison now shows `parseSchematicSymbol()` itself is structurally close enough to stop treating it as a primary parser-only bottleneck
- the remaining symbol-side risk is narrower exactness around shared leaves, helper routines, and parser-wide token/error behavior rather than a broad owner-routine mismatch

5. Sheet parsing is no longer a broad owner-routine bottleneck

- direct comparison now shows `parseSheet()` itself is structurally close enough to stop treating it as a primary parser-only bottleneck
- the remaining sheet-side risk is narrower parser-wide exactness around shared leaves, diagnostics, and any future parent interaction that exposes a concrete mismatch

6. Library-cache symbol parsing is no longer a broad parser-only bottleneck

- `lib_symbols` improved a lot and the broad `parseLibSymbol()` owner loop is now structurally
  close enough to stop treating it as the main parser-only bottleneck
- draw items now run on parser-owned current unit/body-style state like upstream, and helper section-head ownership is closer too
- derived-symbol flattening is also closer now: child local-lib overlays are limited to the upstream field/keyword/fp-filter subset instead of carrying a broader repo-local inheritance model
- parser-owned finalization is now much closer too: description-cache refresh and draw-item sorting are no longer hidden behind model helpers
- direct re-audit now shows local-lib flattening is also close enough to stop treating it as an
  active parser-only bottleneck
- remaining parser-only work should now be driven by support exactness rather than broad library
  routine drift

7. Shape parsing still has gaps

- `arc`, `circle`, `rectangle`, `bezier`, `polyline`, `rule_area`

These are better than before, but still not at exact upstream routine parity.

`bus_alias` is no longer one of the active parser-only bottlenecks:

- direct upstream comparison shows alias-name parsing, `members` section ownership, empty-members
  acceptance, invalid-member `Expecting( "quoted string" )`, and legacy overbar conversion are now
  close enough to stop treating it as a primary gap

`image` is also no longer one of the active parser-only bottlenecks:

- direct upstream comparison shows `at` / `scale` / `uuid` / `data` ownership, non-normal scale
  fallback, invalid-data failure, and legacy image-PPI adjustment are now close enough to stop
  treating it as a primary gap

The table/textbox cluster is no longer the main parser-only bottleneck:

- `parseSchTextBox()`
- `parseSchTableCell()`
- `parseSchTextBoxContent()`
- `parseSchTable()`

Direct upstream comparison shows those routines are now structurally close enough that remaining
parser-only work should be driven elsewhere unless a parent routine exposes a concrete mismatch.

8. Group / post-parse behavior is still not fully upstream-shaped

- some deferred behavior exists
- post-parse fixup flow is still not a literal match to KiCad

9. Token / lexer parity is no longer a broad parser-only bottleneck

- direct re-audit plus the current token regressions now cover the parser-facing lexer behavior
  closely enough:
  - BOM / comment / NUL handling
  - DSN number grammar
  - quoted escape decoding
  - bar-delimited atoms
  - reserved-keyword tagging for real parser heads and DSN-string leak paths
- one deliberate non-gap remains documented here: KiCad `NeedSYMBOL()` and
  `NeedSYMBOLorNUMBER()` both accept quoted strings via `DSNLEXER::IsSymbol()`, so local shared
  symbol-token readers should not be tightened to reject quoted atoms
- the remaining parser-only exactness is now mostly UUID semantics and diagnostic formatting, not a
  broad lexer/token-flow mismatch

10. Error behavior is no longer blocked on parser routine work; it is blocked on the local error model

- many messages and spans are already much closer
- the remaining gap is not “find one more parser branch”; it is that the current local
  `Diagnostic` / `Error` representation is too reduced to express KiCad-style source/line/offset
  parse diagnostics literally
- exact `Expecting(...)` / `Unexpected(...)` parity is now blocked on expanding that model rather
  than on more routine-level parser edits

11. Cross-file post-load pipeline is still the active parity backlog

- `BuildSheetListSortedByPageNumbers`
  Status: first loader-side sheet-path list now exists and is fed from hierarchy links plus root `sheet_instances`; exact KiCad ordering/metadata is still incomplete.
- `UpdateSymbolInstanceData`
  Status: loader-side legacy `< 20221002` pass now applies root `symbol_instances` across the loaded hierarchy, preserves local instance value/footprint state, and keeps reused screens on the same first-instance-baseline / selected-occurrence model as the modern loader flow. Remaining gap is narrower hierarchical-reference/state modeling beyond the current symbol fields.
- `UpdateSheetInstanceData`
  Status: loader-side page propagation now applies root `sheet_instances` onto the loaded sheet-path list; later per-screen page-number/count state now also derives from that sorted list. Current-sheet selection also refreshes reused-screen live page state, including explicit same-schematic occurrence switches. Page comparison exactness is now locked across numeric pages, numeric-before-string pages, and natural ordering inside string pages. Remaining gap is narrower reused-screen/current-sheet semantics beyond the currently modeled page fields.
- `SetSheetNumberAndCount`
  Status: loader-side sheet-number/count assignment now exists both on the loaded sheet-path list and on loaded `Screen` objects (`page_number`, `page_count`, `virtual_page_number`), plus current-sheet helpers now expose the selected occurrence page state across reused-screen entry, exit, and same-schematic occurrence switches. Direct re-audit did not find another model-visible mismatch in the current representation; treat this branch as effectively exhausted unless a new concrete page-state discrepancy appears.
- `RecomputeIntersheetRefs`
  Status: loader-side intersheet-ref recompute now derives `Intersheet References` field values from the loaded sheet list, counts reused-screen occurrences across distinct sheet paths, and preserves explicit visible-property state. Direct re-audit did not find another model-visible mismatch in the current representation; treat this branch as effectively exhausted unless a concrete current-sheet/settings discrepancy appears.
- `UpdateAllScreenReferences`
  Status: loader-side symbol refresh now applies hierarchical local `instances` reference/unit/value/footprint state through the loaded sheet list for unique screens, while reused screens stay on a coherent first-instance baseline until the current-sheet selection explicitly switches them. Leaving a reused screen now restores that baseline instead of leaving the last selected occurrence stuck on the shared screen. Global-label default `Intersheet References` placement is still refreshed after load. Direct re-audit did not find another model-visible mismatch in the current symbol/global-label subset; treat the remaining drift here as blocked on richer per-screen model coverage.
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
  Net effect: the currently representable `MigrateSimModels` surface is now locked more broadly on structured, resolver-backed `Symbol.sim_model` state, not only on flat migrated properties. The remaining backlog here is no longer source lookup or basic model-entry parsing; it is the still-unported deeper library/project/control-model side.
  Model expansion in progress: `SimModel` now preserves ordered parameter payloads, ordered pin mappings, `Value`-field placeholder binding (`${SIM.PARAMS}` / `${SIM.NAME}`), enabled/disabled state, coarse model origin tagging (raw SPICE vs built-in/internal vs explicit library-reference vs IBIS), resolved library source/kind identity, resolved model family/class (`SpiceModel` vs `SpiceSubckt` vs `IbisComponent`), resolved model type, selected IBIS `Model_type` (from either explicit `Sim.Ibis.Model` or the chosen pin row), resolved IBIS differential-pin metadata, generated model-pin metadata, and generated parameter defaults instead of collapsing that state away immediately. The tree also now has explicit sim-library source-stack, content-loading, filesystem path-resolution, and first-pass SPICE/IBIS content resolution helpers for loaded symbols.
  Remaining blocked gap: the heavier simulator-model / project / embedded-model branch that still depends on fuller `CreateModel()`-style behavior beyond the now-broader representable slices: project-backed or serialized-library model resolution beyond the current SPICE-entry/include/statement and typed resolved-model-family layer, deeper control/internal model families beyond the current `DC/SIN/PULSE/EXP/AM/SFFM/PWL/TRNOISE/TRRANDOM` slice, fuller IBIS waveform/driver semantics beyond selected pin/model/`Model_type`/diff metadata, and any unresolved `Spice_*` inference paths that require richer resolved-model objects than the current metadata carrier. Do not fake that remaining stage without first expanding the Rust model beyond metadata snapshots into fuller resolved simulator-model ownership.
  Concrete unblock requirements:
  - move from typed metadata resolution to fuller resolved simulator-model objects that can express generated pins, parameters, and model semantics the way KiCad `CreateModel()` does
  - extend the current resolver beyond first-pass SPICE/IBIS entry discovery and recursive SPICE include resolution into fuller `.kicad_sim` / embedded project-model behavior and deeper model-family parsing
  - only after that fuller resolver/model layer exists should the remaining library-backed `MigrateSimModels` branch, control/internal model families, and unresolved `Spice_*` inference paths be ported literally

### More Exact Current Priority

1. Re-open the remaining blocked `MigrateSimModels` branch on the new structured `Symbol.sim_model` state instead of flat field-only rewrites.
2. Revisit loader page/intersheet branches only if a new concrete model-visible mismatch appears.
3. Leave parser-only blocked surfaces alone unless we explicitly choose fixture migration or error-model expansion.

### Recommended Next Order

1. move the active queue to the remaining blocked `MigrateSimModels` branch on structured `Symbol.sim_model`
2. revisit loader page/intersheet branches only when a concrete new mismatch appears
3. only then revisit parser blocked surfaces

### Bottom Line

The parser-only layer is effectively exhausted in the current model, but two blocked surfaces still
prevent a literal claim of perfect parser parity:

- UUID semantics blocked on fixture/model migration
- diagnostic / error parity blocked on error-model expansion and formatting audit

The active executable backlog is now loader/post-load parity.
