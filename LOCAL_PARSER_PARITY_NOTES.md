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

2. Shared text parsing is still not a literal port

- `parseSchText()`
- `parseEDA_TEXT()` callers
- label / text / text_box / directive / netclass / global / hierarchical branches
- effects / justify / hide / hyperlink semantics still need more exact control-flow parity

3. Property / field parsing still needs closer upstream shape

- direct audits show `parseSchField()` and library `parseProperty()` are much closer than earlier notes implied
- the remaining gap here is now mostly exactness and parent-routine interaction, not a large missing branch family
- remaining field/property work should be driven by concrete parent-routine mismatches, especially under `parseSchText()`, `parseSheet()`, `parseSchematicSymbol()`, and `parseLibSymbol()`

4. Symbol parsing is still not 1:1

- placed schematic symbol parsing is still reduced versus upstream `parseSchematicSymbol()`
- some nested branches and exact token acceptance / error behavior remain simplified

5. Sheet parsing is still not 1:1

- `parseSheet()` internals still need closer parity
- required properties, pins, instances, and nested branches are not yet a literal structural port

6. Library-cache symbol parsing is still partial

- `lib_symbols` improved a lot, but it is still not a true routine-by-routine port of upstream library symbol parsing
- draw items, fields, inherited / extended symbol behavior, and exact branch / error flow still have gaps

7. Shape / image / table parsing still has gaps

- `arc`, `circle`, `rectangle`, `bezier`, `polyline`, `rule_area`
- `image`
- `table`

These are better than before, but still not at exact upstream routine parity.

8. Group / post-parse behavior is still not fully upstream-shaped

- some deferred behavior exists
- post-parse fixup flow is still not a literal match to KiCad

9. Token / lexer parity is still a major remaining gap

- token classes are better now, but parser-wide `NeedSYMBOL` / `NeedNUMBER` / keyword-token behavior is not yet uniformly ported
- this still affects exact acceptance and exact failure points

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

1. Finish `parseSheet()` as a full owning-object routine.
2. Finish `parseSchematicSymbol()` in the same style.
3. Tighten remaining exact `parseSchText()` semantics and failure timing.
4. Tighten remaining exact `parseSchField()` / library `parseProperty()` semantics.
5. Finish `parseLibSymbol()` / library draw-item routine parity.
6. Do a parser-wide token/error parity pass.
7. Port the missing cross-file post-load pipeline.

### Recommended Next Order

1. Port `parseSchText()` and shared text/effects callers more literally.
2. Tighten `parseSheet()` to upstream structure.
3. Tighten `parseSchematicSymbol()` to upstream structure.
4. Finish `parseLibSymbol()` / library draw-item routine parity.
5. Revisit the table/textbox cluster as one shared routine family.
6. Keep walking the top-level `ParseSchematic()` branches in upstream order until each one has a clear local counterpart.

### Bottom Line

The parser is still well short of 1:1 parity.

The biggest remaining gaps are:

- `parseSchText`
- `parseSchField`
- `parseSheet`
- `parseSchematicSymbol`
- library symbol parsing
- parser-wide token / error parity
