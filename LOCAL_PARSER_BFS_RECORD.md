# Parser BFS Record

Target: parser-only, pre-hierarchy flow inventory for:
- `src/token.rs`
- `src/model.rs`
- `src/error.rs`
- `src/diagnostic.rs`
- `src/parser.rs`

Purpose:
- record the current local parser call graph in breadth-first layers
- provide a coverage artifact for “does every upstream parser routine family have a local counterpart?”
- complement `LOCAL_FUNCTION_PARITY_MAP.md`, which remains the authoritative parity audit

Non-goals:
- this is not the primary execution order
- this is not the authoritative parity status file
- active executable parity work is no longer in the parser layer

Status legend:
- `same`: parser-visible behavior is close enough to upstream and no longer an active parser bottleneck
- `blocked`: literal parity is still blocked by a real model/test/support limitation
- `not_applicable`: local support only, or intentionally inlined into another audited routine

Current summary:
- parser-only routine work is exhausted in the current model
- all parser nodes below are now `same`, `blocked`, or `not_applicable`
- active parity work has moved to loader/post-load flow in `src/loader.rs`

## Layer 0

- `parse_schematic_file` (`same`)

## Layer 1

- `parse_schematic` (`same`)

## Layer 2

- `parse_schematic_body` (`same`)

## Layer 3: Top-Level Section Dispatch

- `parse_page_info` (`same`)
- `parse_title_block` (`same`)
- `parse_sch_lib_symbols` (`same`)
- `parse_schematic_symbol` (`same`)
- `parse_sch_sheet` (`same`)
- `parse_junction` (`same`)
- `parse_no_connect` (`same`)
- `parse_bus_entry` (`same`)
- `parse_sch_line` (`same`)
- `parse_sch_polyline` (`same`)
- `parse_sch_arc` (`same`)
- `parse_sch_circle` (`same`)
- `parse_sch_rectangle` (`same`)
- `parse_sch_bezier` (`same`)
- `parse_sch_rule_area` (`same`)
- `parse_sch_text` (`same`)
- `parse_sch_text_box` (`same`)
- `parse_sch_table` (`same`)
- `parse_sch_image` (`same`)
- `parse_bus_alias` (`same`)
- `parse_group` (`same`)
- `parse_embedded_files` (`same`)
- `parse_sch_sheet_instances` (`same`)
- `parse_sch_symbol_instances` (`same`)

## Layer 4: Owner-Sensitive Children

- `parse_lib_symbol` (`same`)
- `parse_sch_sheet_pin` (`same`)
- `parse_sch_field` (`same`)
- `parse_sch_text_box_content` (`same`)
- `parse_sch_table_cell` (`same`)
- `parse_group_members` (`same`)

## Layer 5: Library Helper / Draw-Item Family

- `parse_body_styles` (`same`)
- `parse_pin_names` (`same`)
- `parse_pin_numbers` (`same`)
- `parse_lib_property` (`same`)
- `parse_symbol_draw_item` (`same`)
- `parse_symbol_arc` (`same`)
- `parse_symbol_bezier` (`same`)
- `parse_symbol_circle` (`same`)
- `parse_symbol_polyline` (`same`)
- `parse_symbol_rectangle` (`same`)
- `parse_symbol_text` (`same`)
- `parse_symbol_text_box` (`same`)
- `parse_symbol_pin` (`same`)

## Layer 6: Shared Leaves

- `parse_stroke` (`same`)
- `parse_fill` (`same`)
- `parse_eda_text` (`same`)
- `parse_xy2` (`same`)
- `parse_xy2_lib` (`same`)
- `parse_i32_atom` (`same`)
- `parse_f64_atom` (`same`)
- `parse_internal_units_atom` (`same`)
- `parse_bool_atom` (`same`)
- `parse_kiid` (`same`)
- `parse_raw_kiid` (`same`)
- `parse_kiid_atom` (`same`)
- `parse_maybe_absent_bool` (`same`)

## Layer 7: Shared Token Readers / Primitive Helpers

- `need_left` (`same`)
- `need_right` (`same`)
- `need_symbol_atom` (`same`)
- `need_unquoted_symbol_atom` (`same`)
- `need_quoted_atom` (`not_applicable`)
- `need_symbol_or_number_atom` (`same`)
- `need_dsn_string_atom` (`same`)
- `expecting` (`same`)
- `unexpected` (`same`)
- `error_here` (`same`)

## Layer 8: Parser-Support Model Methods

- `LibSymbol::new` (`same`)
- `LibDrawItem::new` (`same`)
- `Label::new` (`same`)
- `Label::set_position` (`same`)
- `Label::set_angle` (`same`)
- `Label::set_spin` (`same`)
- `Text::new` (`same`)
- `Text::set_position` (`same`)
- `Text::set_angle` (`same`)
- `TextBox::new` (`same`)
- `TableCell::new` (`same`)
- `Stroke::new` (`same`)
- `Fill::new` (`same`)
- `Table::new` (`same`)
- `Table::add_cell` (`same`)
- `Image::new` (`same`)
- `Shape::new` (`same`)
- `Symbol::new` (`same`)
- `Symbol::set_field_text` (`same`)
- `Symbol::set_position` (`same`)
- `Symbol::set_angle` (`same`)
- `Symbol::update_prefix_from_reference` (`same`)
- `Sheet::new` (`same`)
- `Sheet::set_position` (`same`)
- `Sheet::set_size` (`same`)
- `SheetPin::new` (`same`)
- `SheetPin::set_side_with_sheet_geometry` (`same`)
- `SheetPin::constrain_on_sheet_edge` (`same`)
- `Property::new` (`same`)
- `Property::new_named` (`same`)
- `PropertyKind::canonical_key` (`same`)
- `PropertyKind::default_field_id` (`same`)

## Blocked Parser Surfaces

These are the only remaining parser-only gaps recorded by the current audit:

1. exact diagnostic / error-model parity
   - blocked on final display/source-location fidelity after the structured diagnostic expansion
   - touches:
     - `Diagnostic::error`
     - `Error` formatting / source-location fidelity
   - staged unblock order:
     1. audit `src/error.rs` / `src/diagnostic.rs` and enumerate which parser fields are flattened away
        - done
     2. expand the diagnostic model to preserve structured source/location/expectation data
        - done
     3. retarget parser helper construction onto structured diagnostics
        - done
     4. lock representative parser/validation failure families with focused tests
        - done
     5. tighten final `Display` formatting for native wording/source fidelity
        - active

## Rule Of Use

Use this file only as a breadth-first coverage record.

For actual parity decisions:
- `LOCAL_FUNCTION_PARITY_MAP.md` is authoritative
- `LOCAL_PARSER_PARITY_NOTES.md` records current strategy, blockers, and loader priorities

Do not reopen parser routine work from this BFS file unless:
1. a blocked parser surface is being explicitly unblocked, or
2. loader/upstream comparison exposes a concrete parser regression.
