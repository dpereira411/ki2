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

1. Top-level `ParseSchematic()` switch parity is still incomplete

- Some branches are still reduced or merged compared to upstream.
- Not every accepted KiCad section has a routine boundary that mirrors the original parser.

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

3. Property / field parsing still needs closer upstream shape

- direct audits show `parseSchField()` and library `parseProperty()` are much closer than earlier notes implied
- the remaining gap here is now mostly exactness and parent-routine interaction, not a large missing branch family
- remaining field/property work should be driven by concrete parent-routine mismatches, especially under `parseSchText()`, `parseSheet()`, `parseSchematicSymbol()`, and `parseLibSymbol()`

4. Symbol parsing is still not 1:1

- direct comparison shows `parseSchematicSymbol()` is closer than earlier notes implied
- remaining symbol work is now mostly exactness and parent-routine interaction, not a missing whole-routine shape

5. Sheet parsing is no longer a broad owner-routine bottleneck

- direct comparison now shows `parseSheet()` itself is structurally close enough to stop treating it as a primary parser-only bottleneck
- the remaining sheet-side risk is narrower parser-wide exactness around shared leaves, diagnostics, and any future parent interaction that exposes a concrete mismatch

6. Library-cache symbol parsing is no longer a broad owner-routine bottleneck

- `lib_symbols` improved a lot and the broad `parseLibSymbol()` owner loop is now structurally
  close enough to stop treating it as the main parser-only bottleneck
- draw items now run on parser-owned current unit/body-style state like upstream, and helper section-head ownership is closer too
- derived-symbol flattening is also closer now: child local-lib overlays are limited to the upstream field/keyword/fp-filter subset instead of carrying a broader repo-local inheritance model
- parser-owned finalization is now much closer too: description-cache refresh and draw-item sorting are no longer hidden behind model helpers
- remaining lib gaps are now concentrated in narrower helper/exactness surfaces:
  - `parseSymbolDrawItem()`
  - `parseSymbolPin()`
  - local-lib flattening exactness

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

9. Token / lexer parity is still a major remaining gap

- token classes are better now, but parser-wide `NeedSYMBOL` / `NeedNUMBER` / keyword-token behavior is not yet uniformly ported
- this still affects exact acceptance and exact failure points
- direct upstream audit confirmed one non-gap here: KiCad `NeedSYMBOL()` and `NeedSYMBOLorNUMBER()` both accept quoted strings via `DSNLEXER::IsSymbol()`, so local shared symbol-token readers should not be tightened to reject quoted atoms

10. Error behavior still is not 1:1

- many messages and spans are closer now
- exact failure token, exact `Expecting(...)` vs `Unexpected(...)`, and exact control-flow failure timing are still not fully matched

11. Cross-file post-load pipeline is still substantially missing

- `BuildSheetListSortedByPageNumbers`
  Status: first loader-side sheet-path list now exists and is fed from hierarchy links plus root `sheet_instances`; exact KiCad ordering/metadata is still incomplete.
- `UpdateSymbolInstanceData`
  Status: first loader-side legacy `< 20221002` pass now applies root `symbol_instances` across the loaded hierarchy; remaining gap is fuller hierarchical-reference/state modeling.
- `UpdateSheetInstanceData`
  Status: first loader-side page propagation now applies root `sheet_instances` onto the loaded sheet-path list; remaining gap is threading that state through later sheet/page-count flows.
- `SetSheetNumberAndCount`
  Status: first loader-side sheet-number/count assignment now exists on the loaded sheet-path list after page sorting; remaining gap is propagating that state into later per-screen/page-reference behavior.
- `RecomputeIntersheetRefs`
  Status: first loader-side intersheet-ref recompute now derives `Intersheet References` field values from the loaded sheet list; remaining gap is tighter KiCad settings/current-sheet behavior and later `UpdateAllScreenReferences` integration.
- `UpdateAllScreenReferences`
  Status: first loader-side symbol refresh now applies hierarchical local `instances` reference/unit/value/footprint state through the loaded sheet list; remaining gap is broader per-screen update behavior beyond the current model’s symbol/global-label subset.
- `FixLegacyPowerSymbolMismatches`
  Status: first loader-side global-power value fix now handles pre-`20230221` symbols linked to global power lib symbols with hidden `power_in` pins; remaining gap is fuller lib-pin/screen semantics beyond the current symbol/value model.
- `MigrateSimModels`
  Status: still blocked on missing simulation-model / project / embedded-model representation. Upstream migration is not a parser-token tweak; it runs through the simulator model layer and rewrites symbol fields such as `Sim.Device`, `Sim.Params`, `Sim.Pins`, model/library fields, and value-field substitutions. Do not fake this stage without first expanding the Rust model beyond plain parser fields.
- `AnnotatePowerSymbols`
- `SetSheetNumberAndCount`
- `RecomputeIntersheetRefs`
- `UpdateAllScreenReferences`

### More Exact Current Priority

1. Revisit top-level `ParseSchematic()` / `parse_schematic_body()` for concrete remaining exactness mismatches.
2. Revisit `parseSchematicSymbol()` only for concrete remaining exactness mismatches.
3. Tighten remaining exact library helper surfaces (`parseSymbolDrawItem()`, `parseSymbolPin()`, local-lib flattening) where direct comparison exposes them.
4. Tighten remaining exact `parseSchField()` / library `parseProperty()` semantics when a parent routine exposes them.
5. Do a parser-wide token/error parity pass.
6. Port the missing cross-file post-load pipeline.

### Recommended Next Order

1. Keep walking the top-level `ParseSchematic()` branches in upstream order until each one has a clear local counterpart.
2. Revisit `parseSchematicSymbol()` only if direct upstream comparison exposes a concrete remaining mismatch worth porting.
3. Revisit narrower library helper surfaces only when direct comparison exposes a concrete remaining mismatch.
4. Revisit the table/textbox cluster only if one of the parent owner routines exposes a concrete remaining mismatch.

### Bottom Line

The parser is still well short of 1:1 parity.

The biggest remaining gaps are:

- top-level `ParseSchematic()` / `parse_schematic_body()`
- narrower library helper exactness
- parser-wide token / error parity
- narrower `parseSchematicSymbol()` exactness
