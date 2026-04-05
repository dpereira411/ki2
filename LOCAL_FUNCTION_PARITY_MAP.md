## Function Parity Map

Target: finish everything needed before hierarchy loading, bottom-up, by function tree, not by ad hoc branch chasing.

Legend:
- `done`: routine exists and is close enough structurally that it should not be the current bottleneck
- `partial`: routine exists but still has meaningful structural/code-flow gaps
- `blocked`: upstream stage needs model expansion before a real port is possible
- `next`: best current work queue

### Upstream Parser Tree

Boundary:
- In scope: lexer/token layer, parser/model support required for single-file parse parity, and all of `src/parser.rs`
- Out of scope for this map: `src/loader.rs` and all hierarchy/post-load stages

#### Layer 0: Parser Support Files

| File | Status | Notes |
| --- | --- | --- |
| `src/token.rs` | partial | token/lexer parity is still not globally signed off |
| `src/model.rs` | partial | parser-owned structures have improved, but some reduced types still remain |
| `src/error.rs` | partial | parser diagnostics still need exactness work |
| `src/diagnostic.rs` | partial | parser diagnostics still need exactness work |

#### Layer 1: Entry / Dispatch

| Upstream | Local | Status | Notes |
| --- | --- | --- | --- |
| `ParseSchematic` | `parse_schematic` + `parse_schematic_body` | partial | broad dispatch exists, but not every owning flow is proven 1:1 yet |
| `parseHeader` | inline in `parse_schematic` | partial | structurally close, but still part of top-level parity |
| `parsePAGE_INFO` | `parse_page_info` | done | one of the closest branches |
| `parseTITLE_BLOCK` | `parse_title_block` | done | raw value reads, explicit comment-slot switch, and branch ownership now line up closely enough that it is no longer the current bottleneck |

#### Layer 2: Shared Leaves / Subparsers

| Upstream | Local | Status | Notes |
| --- | --- | --- | --- |
| `parseStroke` | `parse_stroke` | done | token ownership mostly aligned |
| `parseFill` | `parse_fill` | done | token ownership mostly aligned |
| `parseEDA_TEXT` | `parse_eda_text` | partial | ownership flow is much closer, but final parser-wide token/error exactness still depends on it |
| `parseSchField` | `parse_sch_field` | partial | direct audit shows the main parent-sensitive classification flow is close; remaining work is exactness, not a large missing branch family |
| `parseSchSheetPin` | `parse_sch_sheet_pin` | done | constructor defaults, shape token flow, at/uuid/effects handling, and close ownership are now close enough that it is no longer the current bottleneck |
| `parseProperty` (lib) | `parse_lib_property` | partial | direct audit shows the constructor/order and insertion policy are close; remaining work is exactness around the surrounding lib-symbol routine |

#### Layer 3: Library Cache

| Upstream | Local | Status | Notes |
| --- | --- | --- | --- |
| `parseLibSymbol` | `parse_lib_symbol` | partial | still one of the biggest remaining parser gaps |
| `parseBodyStyles` | `parse_body_styles` | done | helper boundary restored |
| `parsePinNames` | `parse_pin_names` | done | helper boundary restored |
| `parsePinNumbers` | `parse_pin_numbers` | done | helper boundary restored |
| `ParseSymbolDrawItem` | `parse_symbol_draw_item` | partial | dispatch exists, but full draw-item parity still not signed off |
| `parseSymbolArc` | `parse_symbol_arc` | partial | better, but library draw-item family still grouped as partial |
| `parseSymbolBezier` | `parse_symbol_bezier` | partial | same |
| `parseSymbolCircle` | `parse_symbol_circle` | partial | same |
| `parseSymbolPin` | `parse_symbol_pin` | partial | same |
| `parseSymbolPolyLine` | `parse_symbol_polyline` | partial | same |
| `parseSymbolRectangle` | `parse_symbol_rectangle` | partial | same |
| `parseSymbolText` | `parse_symbol_text` | partial | hidden-text-to-field flow exists, still not final |
| `parseSymbolTextBox` | `parse_symbol_text_box` | partial | closer after shared textbox work |

#### Layer 4: Schematic Owners

| Upstream | Local | Status | Notes |
| --- | --- | --- | --- |
| `parseSchematicSymbol` | `parse_schematic_symbol` | partial | still one of the biggest remaining owner routines |
| `parseSheet` | `parse_sch_sheet` | partial | still one of the biggest remaining owner routines |
| `parseSchText` | `parse_sch_text` | partial | shared family is unified now, but still not fully signed off |
| `parseSchTextBox` | `parse_sch_text_box` | partial | caller-owned flow now matches upstream better |
| `parseSchTableCell` | `parse_sch_table_cell` | partial | distinct cell model now exists, but the shared textbox-body cluster is still not fully signed off |
| `parseSchTextBoxContent` | `parse_sch_text_box_content` | partial | caller-owned now, but final textbox/table semantics are still reduced |
| `parseSchTable` | `parse_sch_table` | partial | table model still simplified |
| `parseImage` | `parse_sch_image` | partial | structurally close, not final |
| `parseSchPolyLine` | `parse_sch_polyline` | partial | closer, not final |
| `parseLine` | `parse_sch_line` | partial | closer, not final |
| `parseSchArc` | `parse_sch_arc` | partial | closer, not final |
| `parseSchCircle` | `parse_sch_circle` | partial | closer, not final |
| `parseSchRectangle` | `parse_sch_rectangle` | partial | closer, not final |
| `parseSchRuleArea` | `parse_sch_rule_area` | partial | closer, not final |
| `parseSchBezier` | `parse_sch_bezier` | partial | closer, not final |
| `parseJunction` | `parse_junction` | partial | close but still in schematic owner family |
| `parseNoConnect` | `parse_no_connect` | partial | same |
| `parseBusEntry` | `parse_bus_entry` | partial | same |
| `parseBusAlias` | `parse_bus_alias` | partial | much tighter, but not globally signed off |
| `parseGroupMembers` + `parseGroup` | `parse_group` | partial | still simplified versus upstream split |
| `parseSchSheetInstances` | `parse_sch_sheet_instances` | partial | parser exists, loader integration still evolving |
| `parseSchSymbolInstances` | `parse_sch_symbol_instances` | partial | parser exists, loader integration still evolving |

### Exact Scope Before Hierarchy Loading

Finish these, in this order, before moving back to `src/loader.rs`:

1. `src/token.rs`
2. parser primitive helpers
3. parser shared subparsers
4. library-cache parser routines
5. schematic item owner routines
6. top-level `parse_schematic` / `parse_schematic_body`

### Bottom-Up Port Order

#### Layer 1: Shared Bottlenecks

1. `parse_eda_text`
2. `parse_sch_field` (no longer the primary bottleneck; revisit only if a parent routine exposes a concrete mismatch)
3. `parse_lib_property` (no longer the primary bottleneck; revisit only if a parent routine exposes a concrete mismatch)

These are still parent/owner-sensitive leaves that many higher routines depend on.

#### Layer 2: Owner-Sensitive Mid-Level Routines

1. `parse_sch_text_box_content` + `parse_sch_table_cell` + `parse_sch_table`
2. library draw-item family under `parse_symbol_draw_item`
3. `parse_sch_sheet_pin` (done; revisit only if `parse_sch_sheet` comparison exposes a concrete remaining mismatch)

#### Layer 3: Big Owner Routines

1. `parse_sch_sheet`
2. `parse_schematic_symbol`
3. `parse_sch_text`
4. `parse_lib_symbol`

#### Layer 5: Top-Level Parser

1. `parse_schematic` / `parse_schematic_body`

### Immediate Next Candidates

Pick the first routine cluster whose direct dependencies above are no longer the bottleneck:

1. Revisit `parse_sch_sheet` against upstream `parseSheet()` as a full routine comparison.
2. Revisit `parse_schematic_symbol` against upstream `parseSchematicSymbol()` as a full routine comparison.
3. Revisit `parse_sch_text` against upstream `parseSchText()` for the remaining owner-flow and exactness edges.
4. Revisit `parse_sch_text_box_content` + `parse_sch_table` for remaining table/textbox semantics.
5. Revisit `parse_schematic` / `parse_schematic_body` after the owning subroutines above are tighter.

### Explicitly Deferred Until After This Map Is Exhausted

- `src/loader.rs`
- `load_schematic_tree`
- `load_hierarchy`
- `build_sheet_list_sorted_by_page_numbers`
- `update_symbol_instance_data`
- `update_sheet_instance_data`
- `set_sheet_number_and_count`
- `recompute_intersheet_refs`
- `update_all_screen_references`
- `annotate_power_symbols`
- `fix_legacy_power_symbol_mismatches`
- `MigrateSimModels`
