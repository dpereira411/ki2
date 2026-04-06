## Parser Parity Notes

Target: true 1:1 structural and code-flow parity with upstream KiCad schematic parsing, not just behavioral similarity.

### Current State

Parser-only work is effectively exhausted in the current model.

The remaining parser-side gaps are now blocked surfaces, not broad routine work:

- native malformed-UUID semantics, which still require fixture/model migration away from stable
  symbolic IDs
- native diagnostic / error-format parity, which still requires expanding the local
  `Diagnostic` / `Error` model

Active parity work is now in the loader / post-load pipeline.

### UUID Unblock Plan

The UUID block is now a fixture/model migration task, not a broad parser-routine task.

To unblock native KiCad malformed-UUID semantics:

1. migrate tests away from stable symbolic fake UUIDs like `root-u`, `sheet-a`, `sym-u`, `wire-u`
2. prefer:
   - valid UUID fixtures where identity only needs to stay stable
   - short hex fixtures only in tests that explicitly lock legacy normalization
3. rewrite expectations away from exact symbolic values and toward:
   - normalized UUID shape
   - referential consistency
   - uniqueness on creation sites
4. keep creation-site and reference-site expectations separate:
   - creation sites consume uniqueness
   - references normalize without drifting away from their targets

Execution order:

1. group/member and item-reference tests with symbolic UUIDs but no hierarchy-path dependency
2. parser-only single-file fixtures with symbolic item UUIDs
3. hierarchy/loader fixtures that currently encode symbolic UUIDs into instance paths
4. only then enable full native malformed-ID replacement semantics in `parse_kiid`

### Diagnostic / Error-Model Unblock Plan

The diagnostic block is now a support-model expansion task, not broad parser-routine work.

To unblock native KiCad parse-diagnostic parity:

1. inventory the exact parser surfaces that currently collapse source fidelity:
   - `Diagnostic::error`
   - parser helpers `expecting`, `unexpected`, `error_here`
   - `validation`
   - `Error` display formatting
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
2. expand the diagnostic model and thread structured data through parser helper construction
3. lock parser-helper exactness with focused tests before touching broad wording
4. tighten final `Display` formatting and source-location rendering
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
  Status: loader-side legacy `< 20221002` pass now applies root `symbol_instances` across the loaded hierarchy, seeds live symbol state from local instances, and preserves local instance value/footprint state. Remaining gap is fuller hierarchical-reference/state modeling.
- `UpdateSheetInstanceData`
  Status: loader-side page propagation now applies root `sheet_instances` onto the loaded sheet-path list; later per-screen page-number/count state now also derives from that sorted list. Current-sheet selection also refreshes reused-screen live page state. Remaining gap is fuller reused-screen/current-sheet semantics.
- `SetSheetNumberAndCount`
  Status: loader-side sheet-number/count assignment now exists both on the loaded sheet-path list and on loaded `Screen` objects (`page_number`, `page_count`, `virtual_page_number`), plus current-sheet helpers now expose the selected occurrence page state. Remaining gap is exact reused-screen/current-sheet behavior.
- `RecomputeIntersheetRefs`
  Status: loader-side intersheet-ref recompute now derives `Intersheet References` field values from the loaded sheet list while preserving explicit visible-property state. Remaining gap is tighter KiCad settings/current-sheet behavior and later `UpdateAllScreenReferences` integration.
- `UpdateAllScreenReferences`
  Status: loader-side symbol refresh now applies hierarchical local `instances` reference/unit/value/footprint state through the loaded sheet list, and global-label default `Intersheet References` placement is refreshed after load. Remaining gap is broader per-screen update behavior beyond the current model’s symbol/global-label subset.
- `FixLegacyPowerSymbolMismatches`
  Status: first loader-side global-power value fix now handles pre-`20230221` symbols linked to global power lib symbols with hidden `power_in` pins; remaining gap is fuller lib-pin/screen semantics beyond the current symbol/value model.
- `MigrateSimModels`
  Status: loader-side migration now covers the upstream-representable slices the current model can express:
  1. the early-return branch that rewrites mid-v7 field spellings (`Sim_Device`, `Sim_Type`, `Sim_Params`, `Sim_Pins`) and converts `Sim_Pins` index arrays into `Sim.Pins` name-value maps when source pins are available
  2. the explicit legacy `Spice_*` raw-SPICE fallback branch that removes `Spice_Primitive` / `Spice_Model` / `Spice_Node_Sequence` / `Spice_Lib_File` fields and synthesizes `Sim.Device=SPICE`, `Sim.Params`, and `Sim.Pins`
  3. the inferred legacy passive/source branch where `Spice_Primitive` matches the symbol prefix and the existing `Value` field remains the model source, while legacy `Spice_Node_Sequence` is still migrated into `Sim.Pins`
  4. the explicit legacy `V` / `I` DC-source branch where `Spice_Model` like `dc(1)` becomes `Sim.Device`, `Sim.Type=DC`, migrated `Sim.Pins`, and an updated `Value` field
  5. the explicit legacy `V` / `I` built-in source branches where `Spice_Model` like `sin(...)`, `pulse(...)`, `exp(...)`, `am(...)`, and `sffm(...)` becomes `Sim.Device`, `Sim.Type`, named `Sim.Params`, and migrated/defaulted `Sim.Pins`
  Remaining blocked gap: the heavier simulator-model / project / embedded-model branch that resolves library-backed models, broader internal source/model functions beyond the current `DC/SIN/PULSE/EXP/AM/SFFM` slice, value-field substitutions beyond the simple DC slice, and full `Spice_*` inference paths. Do not fake that remaining stage without first expanding the Rust model beyond plain parser fields.

### More Exact Current Priority

1. Tighten remaining loader/post-load exactness around reused-screen/current-sheet page semantics.
2. Tighten remaining loader-side `UpdateAllScreenReferences` exactness against the current-sheet path.
3. Tighten remaining loader-side `RecomputeIntersheetRefs` / `SetSheetNumberAndCount` exactness.
4. Leave parser-only blocked surfaces alone unless we explicitly choose fixture migration or error-model expansion.

### Recommended Next Order

1. `UpdateAllScreenReferences`
2. `SetSheetNumberAndCount`
3. `RecomputeIntersheetRefs`
4. `UpdateSymbolInstanceData`
5. `FixLegacyPowerSymbolMismatches`
6. only then revisit parser blocked surfaces or the heavier blocked `MigrateSimModels` branch

### Bottom Line

The parser-only layer is effectively exhausted in the current model, but two blocked surfaces still
prevent a literal claim of perfect parser parity:

- UUID semantics blocked on fixture/model migration
- diagnostic / error parity blocked on error-model expansion and formatting audit

The active executable backlog is now loader/post-load parity.
