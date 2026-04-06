# Parser BFS Record

Target: parser-only, pre-hierarchy flow inventory for `src/parser.rs`.

Purpose:
- record the current local parser call graph in breadth-first layers
- provide an audit artifact for “does every upstream routine family have a local counterpart?”
- support parity tracking alongside `LOCAL_FUNCTION_PARITY_MAP.md`

Non-goal:
- this is not the primary porting order
- primary execution order remains bottom-up by dependency, not BFS order

Legend:
- `done`: structurally close enough that it should not be the current bottleneck
- `partial`: still has meaningful exactness / control-flow work left

## Layer 0

- `parse_schematic_file` (`partial`)

## Layer 1

- `parse_schematic` (`partial`)

## Layer 2

- `parse_schematic_body` (`partial`)

## Layer 3: Top-Level Section Dispatch

- `parse_page_info` (`done`)
- `parse_title_block` (`done`)
- `parse_sch_lib_symbols` (`partial`)
- `parse_schematic_symbol` (`partial`)
- `parse_sch_sheet` (`partial`)
- `parse_junction` (`partial`)
- `parse_no_connect` (`partial`)
- `parse_bus_entry` (`partial`)
- `parse_sch_line` (`partial`)
- `parse_sch_polyline` (`partial`)
- `parse_sch_arc` (`partial`)
- `parse_sch_circle` (`partial`)
- `parse_sch_rectangle` (`partial`)
- `parse_sch_bezier` (`partial`)
- `parse_sch_rule_area` (`partial`)
- `parse_sch_text` (`partial`)
- `parse_sch_text_box` (`done`)
- `parse_sch_table` (`done`)
- `parse_sch_image` (`done`)
- `parse_bus_alias` (`done`)
- `parse_group` (`done`)
- `parse_embedded_files` (`partial`)
- `parse_sch_sheet_instances` (`partial`)
- `parse_sch_symbol_instances` (`partial`)

## Layer 4: Owner-Sensitive Children

- `parse_lib_symbol` (`partial`)
- `parse_sch_sheet_pin` (`done`)
- `parse_sch_field` (`partial`)
- `parse_sch_text_box_content` (`done`)
- `parse_sch_table_cell` (`done`)
- `parse_group_members` (`done`)

## Layer 5: Library Helper / Draw-Item Family

- `parse_body_styles` (`done`)
- `parse_pin_names` (`done`)
- `parse_pin_numbers` (`done`)
- `parse_lib_property` (`partial`)
- `parse_symbol_draw_item` (`partial`)
- `parse_symbol_arc` (`partial`)
- `parse_symbol_bezier` (`partial`)
- `parse_symbol_circle` (`partial`)
- `parse_symbol_polyline` (`partial`)
- `parse_symbol_rectangle` (`partial`)
- `parse_symbol_text` (`partial`)
- `parse_symbol_text_box` (`partial`)
- `parse_symbol_pin` (`partial`)

## Layer 6: Shared Leaves

- `parse_stroke` (`done`)
- `parse_fill` (`done`)
- `parse_eda_text` (`partial`)
- `parse_xy2` (`partial`)
- `parse_xy2_lib` (`partial`)
- `parse_i32_atom` (`partial`)
- `parse_f64_atom` (`partial`)
- `parse_internal_units_atom` (`partial`)
- `parse_bool_atom` (`partial`)
- `parse_kiid` (`partial`)
- `parse_raw_kiid` (`partial`)
- `parse_kiid_atom` (`partial`)
- `parse_maybe_absent_bool` (`partial`)

## Layer 7: Shared Token Readers

- `need_left` (`partial`)
- `need_right` (`partial`)
- `need_symbol_atom` (`partial`)
- `need_unquoted_symbol_atom` (`partial`)
- `need_quoted_atom` (`partial`)
- `need_symbol_or_number_atom` (`partial`)

## Active Endgame Queue

Use this BFS record as an audit checklist, but keep execution order dependency-driven:

1. `parse_eda_text`
2. `parse_sch_text`
3. `parse_lib_symbol`
4. `parse_sch_sheet`
5. `parse_schematic_symbol`
6. parser-wide token / diagnostic exactness

## Completion Rule

Do not mark the parser-only boundary complete until:

- every `partial` routine above is either:
  - moved to `done`, or
  - split into a smaller explicit remaining mismatch elsewhere
- `LOCAL_FUNCTION_PARITY_MAP.md` no longer contains parser-only `partial` bottlenecks
- remaining differences are outside the parser-only boundary
