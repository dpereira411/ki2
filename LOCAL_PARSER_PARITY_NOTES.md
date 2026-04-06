## Parser Parity Notes

Target: true 1:1 structural and code-flow parity with upstream KiCad schematic parsing, not just behavioral similarity.

### Current State

The parser is much closer structurally in some key routines, but it is still not at full 1:1 parity.

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

11. Cross-file post-load pipeline is still substantially missing

- `BuildSheetListSortedByPageNumbers`
  Status: first loader-side sheet-path list now exists and is fed from hierarchy links plus root `sheet_instances`; exact KiCad ordering/metadata is still incomplete.
- `UpdateSymbolInstanceData`
  Status: first loader-side legacy `< 20221002` pass now applies root `symbol_instances` across the loaded hierarchy; remaining gap is fuller hierarchical-reference/state modeling.
- `UpdateSheetInstanceData`
  Status: first loader-side page propagation now applies root `sheet_instances` onto the loaded sheet-path list; later per-screen page-number/count state now also derives from that sorted list. Remaining gap is fuller reused-screen/current-sheet semantics.
- `SetSheetNumberAndCount`
  Status: loader-side sheet-number/count assignment now exists both on the loaded sheet-path list and on loaded `Screen` objects (`page_number`, `page_count`, `virtual_page_number`). Remaining gap is exact reused-screen/current-sheet behavior.
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
- `AnnotatePowerSymbols`
- `SetSheetNumberAndCount`
- `RecomputeIntersheetRefs`
- `UpdateAllScreenReferences`

### More Exact Current Priority

1. Tighten remaining loader/post-load exactness around reused-screen/current-sheet page semantics.
2. Decide whether to unblock native malformed-UUID semantics by migrating parser/loader fixtures away from stable symbolic IDs.
3. Decide whether to expand the local diagnostic/error model for native parse-error parity.
3. Port the missing cross-file post-load pipeline.

### Recommended Next Order

1. Decide whether to unblock native malformed-UUID semantics by migrating symbolic fixture IDs.
2. Decide whether to expand the diagnostic/error model for native parse-error parity.

### Bottom Line

The parser is not yet at full 1:1 parity, but the remaining parser-only gaps are now blocked
surfaces rather than broad routine work:

- UUID semantics blocked on fixture/model migration
- diagnostic / error parity blocked on error-model expansion
