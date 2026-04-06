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

2. Shared text/effects parsing is still not a literal port

- `parseSchText()`
- `parseEDA_TEXT()` callers
- label / text / text_box / directive / netclass / global / hierarchical branches
- effects / justify / hide / hyperlink semantics still need more exact control-flow parity

Direct re-audit shows `parseSchText()` itself is no longer the broad owner-routine mismatch it used to
be:

- the shared text-family object-construction loop, `shape` / `length` / `iref` / `property`
  ownership, and final fieldless-label autoplacement behavior are now structurally close to upstream
- the remaining text-family gap is narrower and is now concentrated more in `parseEDA_TEXT()`
  exactness and text/effects interaction than in a missing `parseSchText()` routine shape

3. Property / field parsing still needs closer upstream shape

- direct audits show `parseSchField()` and library `parseProperty()` are much closer than earlier notes implied
- the remaining gap here is now mostly exactness and parent-routine interaction, not a large missing branch family
- remaining field/property work should be driven by concrete parent-routine mismatches, especially under `parseSchText()`, `parseSheet()`, `parseSchematicSymbol()`, and `parseLibSymbol()`

4. Symbol parsing is still not 1:1

- direct comparison shows `parseSchematicSymbol()` is closer than earlier notes implied
- remaining symbol work is now mostly exactness and parent-routine interaction, not a missing whole-routine shape

5. Sheet parsing is still not 1:1

- direct comparison shows `parseSheet()` is closer than earlier notes implied
- remaining sheet work is now mostly exactness and surrounding parser interaction, not a missing whole-routine shape

6. Library-cache symbol parsing is still partial

- `lib_symbols` improved a lot, but it is still not a true routine-by-routine port of upstream library symbol parsing
- draw items now run on parser-owned current unit/body-style state like upstream, and helper section-head ownership is closer too
- derived-symbol flattening is also closer now: child local-lib overlays are limited to the upstream field/keyword/fp-filter subset instead of carrying a broader repo-local inheritance model
- parser-owned finalization is now much closer too: description-cache refresh and draw-item sorting are no longer hidden behind model helpers
- remaining gaps are now more concentrated in narrower `parseLibSymbol()` branch / error exactness than in broad ownership or finalization flow

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

1. Tighten remaining exact `parseEDA_TEXT()` and shared text/effects semantics.
2. Revisit `parseSchText()` where that shared text/effects exactness still leaks into the owner routine.
3. Finish the remaining narrower `parseLibSymbol()` exact branch / error parity.
4. Revisit `parseSheet()` only for concrete remaining exactness mismatches.
5. Revisit `parseSchematicSymbol()` only for concrete remaining exactness mismatches.
6. Tighten remaining exact `parseSchField()` / library `parseProperty()` semantics when a parent routine exposes them.
7. Do a parser-wide token/error parity pass.
8. Port the missing cross-file post-load pipeline.

### Recommended Next Order

1. Port `parseSchText()` and shared text/effects callers more literally.
2. Finish `parseLibSymbol()` / library draw-item routine parity.
3. Revisit `parseSheet()` only if direct upstream comparison exposes a concrete remaining mismatch worth porting.
4. Revisit `parseSchematicSymbol()` only if direct upstream comparison exposes a concrete remaining mismatch worth porting.
5. Keep walking the top-level `ParseSchematic()` branches in upstream order until each one has a clear local counterpart.
6. Revisit the table/textbox cluster only if one of the parent owner routines exposes a concrete remaining mismatch.

### Bottom Line

The parser is still well short of 1:1 parity.

The biggest remaining gaps are:

- `parseSchText`
- `parseLibSymbol`
- parser-wide token / error parity
- library symbol parsing
