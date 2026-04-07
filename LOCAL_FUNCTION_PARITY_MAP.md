## Legacy Pointer

`LOCAL_FUNCTION_PARITY_MAP.md` is no longer the authoritative backlog.

Use [PARITY_BACKLOG.md](/Users/Daniel/Desktop/modular/tools/ki2/PARITY_BACKLOG.md) instead.

This file remains only so older references do not break.
| `parseEDA_TEXT` | `parse_eda_text` | `same` | bare `font`/`justify`/`hide`/`href` heads, direct `href` entry, and native hyperlink acceptance/rejection cases are now tight enough that it is no longer the active bottleneck | direct source comparison, native `kicad-cli` probes, and focused tests | none |
| `parseSchField` | `parse_sch_field` | `same` | direct re-audit shows the remaining parser-only risk is no longer in the broad field routine: parent-sensitive header classification, legacy `id` ignore flow, and field-object mutation timing are structurally close enough to upstream | direct source comparison plus focused field regressions | none |
| `parseProperty` | `parse_lib_property` | `same` | direct re-audit shows the remaining parser-only risk is no longer in the broad library-property routine: constructor/order, mandatory overwrite, `ki_*` metadata flow, and duplicate user-field suffixing are structurally close enough to upstream | direct source comparison plus focused lib-property regressions | none |
| `parseSchSheetPin` | `parse_sch_sheet_pin` | `same` | constructor defaults, side/geometry flow, and close ownership are now stable | direct audit + tests | none |
| `parseSchTextBoxContent` | `parse_sch_text_box_content` | `same` | owner/body split is structurally close enough | direct audit + tests | none |
| `parseSchTableCell` | `parse_sch_table_cell` | `same` | distinct cell ownership is in place | direct audit + tests | none |

## Layer 3: Library Cache

| Upstream routine | Local routine | Status | Reason | Evidence | Next action |
| --- | --- | --- | --- | --- | --- |
| `ParseSymbolDrawItem` | `parse_symbol_draw_item` | `same` | direct re-audit shows the remaining lib exactness is no longer in the dispatcher: branch set, token ownership, and literal fallback text are structurally aligned with upstream | direct source comparison against upstream body plus existing lib draw-item regressions | none |
| `parseLibSymbol` | `parse_lib_symbol` | `same` | direct re-audit shows the broad owner routine is now structurally close enough: root-unit construction, nested-unit parsing, metadata branches, embedded-file recovery, and parser-owned finalization are no longer the active lib bottleneck; the remaining lib drift is narrower draw-item/helper exactness | direct source comparison against upstream body plus existing lib-symbol regressions | none |
| `parseSymbolArc` | `parse_symbol_arc` | `same` | remaining mismatches are no longer a primary bottleneck | existing arc tests | none |
| `parseSymbolBezier` | `parse_symbol_bezier` | `same` | branch shape and malformed-point behavior are covered | existing tests | none |
| `parseSymbolCircle` | `parse_symbol_circle` | `same` | defaults and token flow are stable enough | existing tests | none |
| `parseSymbolPolyLine` | `parse_symbol_polyline` | `same` | token set and point grammar are stable enough | existing tests | none |
| `parseSymbolRectangle` | `parse_symbol_rectangle` | `same` | corner-radius/internal-units flow is stable enough | existing tests | none |
| `parseSymbolText` | `parse_symbol_text` | `same` | hidden-text-to-field flow and effects ownership are stable enough | existing tests | none |
| `parseSymbolTextBox` | `parse_symbol_text_box` | `same` | text-box ownership and defaults are stable enough | existing tests | none |
| `parseSymbolPin` | `parse_symbol_pin` | `same` | direct re-audit shows the remaining lib exactness is no longer here: child-head ownership, bare/valued hide flow, name/number effects timing, and alternate overwrite semantics are structurally close enough to upstream | direct source comparison against upstream body plus existing lib-pin regressions | none |
| `embedded_files` body | `parse_embedded_files` | `same` | section ownership and recovery behavior are now close enough | direct audit + tests | none |

## Layer 4: Schematic Owners

| Upstream routine | Local routine | Status | Reason | Evidence | Next action |
| --- | --- | --- | --- | --- | --- |
| `parseBusAlias` | `parse_bus_alias` | `same` | direct upstream audit cleared it from the active bottleneck set | existing notes/tests | none |
| `parseJunction` | `parse_junction` | `same` | constructor/default/token flow is stable enough | existing tests | none |
| `parseNoConnect` | `parse_no_connect` | `same` | constructor/default/token flow is stable enough | existing tests | none |
| `parseBusEntry` | `parse_bus_entry` | `same` | legacy/default stroke and size flow are stable enough | existing tests | none |
| `parseLine` | `parse_sch_line` | `same` | line/token ownership is close enough | existing tests | none |
| `parseSchText` | `parse_sch_text` | `same` | direct upstream re-audit shows the shared text-family constructor loop, `shape` / `length` / `iref` / `property` ownership, `at` owner mutation, and fieldless-label autoplacement are now close enough that it is no longer an active parser-only bottleneck | direct source comparison plus the existing focused text-family regressions | none |
| `parseSchTextBox` | `parse_sch_text_box` | `same` | no longer a primary bottleneck | direct audit | none |
| `parseSchTable` | `parse_sch_table` | `same` | table ownership and no-cell behavior are stable enough | direct audit + tests | none |
| `parseImage` | `parse_sch_image` | `same` | no longer a primary bottleneck | direct audit | none |
| `parseSchPolyLine` | `parse_sch_polyline` | `same` | shape-specific remaining drift is no longer active enough to block endgame | existing tests | none |
| `parseSchArc` | `parse_sch_arc` | `same` | shape finalization flow is stable enough | existing tests | none |
| `parseSchCircle` | `parse_sch_circle` | `same` | shape finalization flow is stable enough | existing tests | none |
| `parseSchRectangle` | `parse_sch_rectangle` | `same` | shape finalization flow is stable enough | existing tests | none |
| `parseSchBezier` | `parse_sch_bezier` | `same` | shape finalization flow is stable enough | existing tests | none |
| `parseSchRuleArea` | `parse_sch_rule_area` | `same` | rule-area/polyline ownership is stable enough | existing tests | none |
| `parseSchematicSymbol` | `parse_schematic_symbol` | `same` | direct re-audit shows the broad owner routine is now structurally close enough: upfront construction, inline `default_instance` and `instances` walks, parent-sensitive property insertion, first-instance live-state seeding, and local-instance value/footprint ownership are no longer active parser-only bottlenecks | direct source comparison plus the focused symbol-instance/property regressions | none |
| `parseSheet` | `parse_sch_sheet` | `same` | direct re-audit shows the broad owner routine is now structurally close enough: upfront construction, inline `instances` walk, legacy field-ID recovery timing, owner-driven pin geometry, duplicate mandatory-field accumulation, and deferred `Sheetfile` normalization all align closely enough that it is no longer an active parser-only bottleneck | direct source comparison plus the focused sheet regressions | none |
| `parseSchSheetInstances` | `parse_sch_sheet_instances` | `same` | parser-only behavior is stable enough; loader integration is deferred | current notes | none in parser-only phase |
| `parseSchSymbolInstances` | `parse_sch_symbol_instances` | `same` | parser-only behavior is stable enough; loader integration is deferred | current notes | none in parser-only phase |
| `parseGroup` | `parse_group` | `same` | deferred-resolution and cycle repair are stable enough | direct audit + tests | none |
| `parseGroupMembers` | `parse_group_members` | `same` | member parsing/normalization is stable enough | existing tests | none |

## Layer 5: Parser Primitives / Utilities

| Local routine | Upstream counterpart | Status | Reason | Evidence | Next action |
| --- | --- | --- | --- | --- | --- |
| `fixup_sch_fill_mode` | schematic fill fixup helper | `same` | branch timing is now on owning fill path | existing tests | none |
| `find_invalid_lib_id_char` | `LIB_ID::Parse` helper split | `same` | used to preserve upstream error text shape | current behavior | none |
| `is_valid_lib_id_shape` | `LIB_ID::Parse` helper split | `same` | direct re-audit plus the empty-nickname regression now cover the real remaining shape split against KiCad `LIB_ID::Parse()`: item-name emptiness still fails, empty nicknames are accepted, and invalid characters are still handled by the sibling illegal-character path | direct source comparison plus lib-ID regressions | none |
| `clamp_text_size` | text size enforcement | `same` | current branch behavior is tested and stable | tests | none |
| `validate_hyperlink` | `EDA_TEXT::ValidateHyperlink` | `same` | scheme handling, `#page` handling, digit-scheme rejection, malformed-url rejection, and native whitespace acceptance are now close enough | source comparison, native `kicad-cli` probes, and focused tests | none |
| `get_label_spin_style` | label spin mapping | `same` | now separated cleanly from position flow | tests | none |
| `normalize_text_angle` | text angle normalization | `same` | current behavior is stable enough | tests | none |
| `get_legacy_text_margin` | legacy margin fallback | `same` | covered by textbox/table tests | tests | none |
| `read_png_ppi` | image helper | `same` | covered by tests | parser tests | none |
| `read_jpeg_ppi` | image helper | `same` | covered by tests | parser tests | none |
| `read_image_ppi` | image helper | `same` | covered by tests | parser tests | none |
| `validate_image_data` | image decode validation | `same` | covered and no longer active | integration tests | none |
| `parse_xy2` | `parseXY` | `same` | internal-units and token path are stable enough | tests | none |
| `parse_xy2_lib` | library coordinate variant | `same` | inverted-Y library behavior is covered | tests | none |
| `parse_i32_atom` | `parseInt` | `same` | parser-wide integer path is stable enough | tests | none |
| `parse_f64_atom` | `parseDouble` | `same` | parser-wide float path is stable enough | tests | none |
| `parse_internal_units_atom` | `parseInternalUnits` | `same` | clamp behavior is now explicit and tested | tests | none |
| `parse_bool_atom` | `parseBool` | `same` | yes/no exactness is stable enough | tests | none |
| `parse_kiid` | `parseKIID` wrapper | `same` | malformed symbolic IDs now normalize through native-style generated UUID replacement, legacy short-hex normalization, and uniqueness-on-creation behavior | UUID migration + malformed UUID regressions | none |
| `parse_raw_kiid` | raw KIID path | `same` | raw-vs-normalized split is explicit and covered | recent UUID audits | none |
| `parse_kiid_atom` | `parseKIID` low-level read | `same` | low-level UUID reads now follow the same malformed-ID replacement semantics as the wrapper | UUID migration + malformed UUID regressions | none |
| `normalize_kiid` | KIID normalization/uniqueness | `same` | malformed non-UUID handling now uses generated UUID replacement while legacy short-hex normalization and duplicate incrementing remain covered | UUID migration + malformed UUID regressions | none |
| `parse_maybe_absent_bool` | `parseMaybeAbsentBool` | `same` | current behavior is close and well covered | tests | none |
| `require_known_version` | local support | `not_applicable` | repo-local support for parser entry ordering | source inspection | none |
| `need_left` | `NeedLEFT` | `same` | stable low-level exactness | tests | none |
| `need_right` | `NeedRIGHT` | `same` | stable low-level exactness | tests | none |
| `need_symbol_atom` | `NeedSYMBOL` | `same` | quoted/symbol acceptance now matches KiCad expectations closely enough | direct audit + tests | none |
| `need_unquoted_symbol_atom` | keyword-token path | `same` | direct re-audit shows all real unquoted parser heads are now reserved and the helper’s keyword-only acceptance is structurally aligned with the remaining parser surfaces | direct audit against parser branch heads plus keyword-tag regressions | none |
| `need_quoted_atom` | quoted-string helper | `not_applicable` | local support only | source inspection | none |
| `need_symbol_or_number_atom` | `NeedSYMBOLorNUMBER` | `same` | quoted/symbol/number acceptance is stable enough | direct audit + tests | none |
| `need_dsn_string_atom` | DSN string branch helper | `same` | the only live parser use is `jumper_pin_groups`, and direct re-audit plus reserved-keyword regressions now cover the remaining leak paths there | direct source comparison plus jumper-pin-group keyword regressions | none |
| `at_right` | local token helper | `not_applicable` | local parser support only | source inspection | none |
| `at_unquoted_symbol_with` | local token helper | `not_applicable` | local parser support only | source inspection | none |
| `current_nesting_depth` | local token helper | `not_applicable` | local parser support only | source inspection | none |
| `skip_to_block_right` | local recovery helper | `same` | used for embedded-file warning recovery only; behavior is stable enough | tests | none |
| `current` | local token helper | `not_applicable` | local parser support only | source inspection | none |
| `current_span` | local token helper | `not_applicable` | local parser support only | source inspection | none |
| `expecting` | parse diagnostic helper | `blocked` | exact KiCad parse-error parity now depends on richer diagnostic/source-location formatting and preserving structured expectation payloads instead of flattening them immediately | parser notes + source inspection | retarget onto structured diagnostics after error-model expansion |
| `unexpected` | parse diagnostic helper | `blocked` | exact KiCad parse-error parity now depends on richer diagnostic/source-location formatting and preserving structured unexpected-token payloads instead of flattening them immediately | parser notes + source inspection | retarget onto structured diagnostics after error-model expansion |
| `error_here` | parse diagnostic helper | `blocked` | exact KiCad parse-error parity now depends on richer diagnostic/source-location formatting and explicit failure-site context the current reduced model discards | parser notes + source inspection | retarget onto structured diagnostics after error-model expansion |
| `find_standard_page_info` | paper lookup support | `same` | stable and no longer active | direct behavior | none |
| `parse_page_info` | `parsePAGE_INFO` | `same` | tracked above at owner layer; helper remains stable | direct audit | none |
| `convert_old_overbar_notation` | legacy overbar conversion | `same` | behavior is covered and stable enough | tests | none |
| `unescape_string_markers` | KiCad string-marker unescape support | `same` | used narrowly and covered | tests | none |
| `fixup_legacy_lib_symbol_alternate_body_styles` | parser post-fixup | `same` | remaining body-style ownership moved into parser/finalization already | direct audit | none |
| `update_local_lib_symbol_links` | parser local-lib link refresh | `same` | parser-only local-lib link fixup is stable enough | tests | none |
| `flatten_local_lib_symbol` | local-lib `Flatten()` analogue | `same` | direct re-audit plus existing derived-symbol regressions now cover the remaining upstream flatten branches closely enough: parent-chain order, mandatory/user-field overrides, keyword/filter inheritance, embedded-file handling, and missing-parent behavior are no longer active parser-only bottlenecks | direct source comparison against `LIB_SYMBOL::Flatten()` plus derived-local-lib regressions | none |
| `has_legacy_alternate_body_style` | local helper | `same` | behavior is stable enough | tests | none |
| `fixup_embedded_data` | parser embedded-data finalization | `same` | metadata hydration/recovery is stable enough | tests | none |
| `resolve_groups` | deferred group resolution | `same` | parser-only group finalization is no longer active | tests | none |
| `get_item_index_by_uuid` | local group support | `not_applicable` | local support only | source inspection | none |
| `item_uuid` | local group support | `not_applicable` | local support only | source inspection | none |
| `groups_sanity_check` | group cycle repair | `same` | covered and no longer active | tests | none |
| `validation` | parse validation helper | `blocked` | final validation-diagnostic exactness now depends on the same diagnostic model expansion as the other parser error helpers, plus preserving validation-specific context through final formatting | parser notes + source inspection | retarget onto structured diagnostics after error-model expansion |

## Implementation Rule

For any row marked `different` or `blocked`:

1. compare the upstream body directly to the local body
2. identify one concrete mismatch only
3. patch that mismatch
4. run `cargo fmt --all`
5. run `cargo test -q`
6. update the row if the function is no longer active

Parser-only rows above are now exhausted: every item is `same`, `not_applicable`, or explicitly
`blocked` for a real model reason.

Do not create new parser-only backlog items unless:
1. a blocked parser surface is being explicitly unblocked, or
2. direct upstream comparison during loader work exposes a concrete parser regression.
