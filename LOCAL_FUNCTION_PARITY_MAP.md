# Parser-Only Function Parity Audit

Target: exhaust pre-hierarchy parser parity by auditing every parser-boundary function and support
function against upstream KiCad, then iterating the unresolved items one by one.

Boundary:
- In scope: `src/token.rs`, `src/model.rs`, `src/error.rs`, `src/diagnostic.rs`, `src/parser.rs`
- Out of scope: `src/loader.rs` and all hierarchy/post-load stages

Status legend:
- `same`: function boundary, token flow, mutation timing, accepted grammar, and failure behavior are
  close enough to upstream that it is no longer an active bottleneck
- `different`: meaningful parser-parity work still remains
- `blocked`: parity depends on a model/support expansion before the owning routine can match
- `not_applicable`: no distinct upstream counterpart is required; the function exists only as local
  support for an already-audited upstream flow

## Active Queue

Resolve these in order unless a direct comparison shows a prerequisite blocker first:

1. `parse_schematic` / `parse_schematic_body`
2. narrower library exactness in `parse_symbol_draw_item` / `parse_symbol_pin` / `flatten_local_lib_symbol`
3. parser-wide token/error exactness in `src/token.rs`, `src/error.rs`, and `src/diagnostic.rs`

## Layer 0: Support Files

### `src/token.rs`

| Local function | Upstream counterpart | Status | Reason | Evidence | Next action |
| --- | --- | --- | --- | --- | --- |
| `skip_utf8_bom` | DSN lexer BOM skip | `same` | helper-only and behavior is already locked | token tests | none |
| `prescan_version` | none; local lexer prescan | `different` | local prescan is still a repo-specific preprocessing step and exactness is not fully signed off | parser notes + token tests | keep auditing header/version edge cases |
| `is_line_comment_start` | DSN lexer comment detection | `same` | comment start behavior is now covered and matches parser entry needs closely enough | token tests | none |
| `skip_whitespace_and_line_comments` | DSN lexer whitespace/comment skip | `same` | line comments, BOM, and NUL whitespace are covered and stable | token tests | none |
| `is_dsn_number` | DSN lexer number classification | `same` | current grammar matches KiCad-style number token behavior closely enough | token tests | none |
| `is_schematic_keyword` | KiCad keyword token table | `different` | keyword tagging keeps improving and still needs full parser-wide signoff | recent keyword-tag commits | continue direct malformed-token audits |
| `lex` | top-level lexer entry | `different` | overall token layer is still not globally signed off | parser notes | keep after routine audits |
| `decode_quoted_escape` | DSN quoted-string escape decoding | `same` | KiCad-style escape decoding is now covered with focused tests | token tests | none |
| `lex_with_bar` | DSN lexer body | `different` | whole-token exactness still depends on final keyword and malformed-atom parity | token tests + parser notes | continue parser-wide sweep last |

### `src/diagnostic.rs`

| Local function | Upstream counterpart | Status | Reason | Evidence | Next action |
| --- | --- | --- | --- | --- | --- |
| `Diagnostic::error` | parse error construction | `different` | local diagnostic shape is still simpler than KiCad’s parser diagnostics | parser notes | revisit only if parser error exactness needs model changes |
| `Diagnostic::with_path` | none; local support | `not_applicable` | local helper only | source inspection | none |
| `Diagnostic::with_span` | none; local support | `not_applicable` | local helper only | source inspection | none |

### `src/error.rs`

No nontrivial function bodies live here, but the file remains in scope because final parser parity
still depends on error/diagnostic exactness.

| Local item | Upstream counterpart | Status | Reason | Evidence | Next action |
| --- | --- | --- | --- | --- | --- |
| `Error` enum formatting | parse/validation error reporting | `different` | exact wording/span/source parity is still incomplete | parser notes | revisit during final error sweep |

### `src/model.rs` parser support methods

Only methods that materially affect parser parity are tracked here. Pure accessors or test helpers do
not drive the queue unless a parent parser routine exposes them.

| Local function | Upstream counterpart | Status | Reason | Evidence | Next action |
| --- | --- | --- | --- | --- | --- |
| `LibSymbol::new` | `LIB_SYMBOL` constructor defaults | `same` | mandatory fields and root-unit ownership are close enough | model tests | none |
| `LibSymbol::has_legacy_alternate_body_style` | legacy body-style inference | `same` | now used in upstream-shaped finalization | parser/model tests | none |
| `LibSymbol::next_field_ordinal` | field ordinal ownership | `same` | supports parser-owned hidden/user field flow correctly enough | model tests | none |
| `LibDrawItem::new` | library draw-item defaults | `same` | defaults are constructor-owned and tested | model tests | none |
| `Label::new` | `SCH_LABEL_BASE` constructors | `same` | mandatory-field/default-shape ownership now matches upstream closely enough | model tests | none |
| `Label::next_field_ordinal` | label field ordinal ownership | `same` | parser-visible behavior is locked | model tests | none |
| `Label::set_position` | `SCH_LABEL_BASE::SetPosition` | `same` | attached-field movement is model-owned and tested | model tests | none |
| `Label::set_angle` | label angle setter | `same` | now separate from position flow like upstream | model tests | none |
| `Label::set_spin` | label spin setter | `same` | separate owner mutation now matches `parseSchText()` needs | model tests | none |
| `Text::new` | `SCH_TEXT` constructor defaults | `same` | defaults and geometry are covered | model tests | none |
| `Text::set_position` | `SCH_TEXT::SetPosition` | `same` | owner movement is separated from angle mutation | model tests | none |
| `Text::set_angle` | text angle setter | `same` | separate from position like upstream | model tests | none |
| `TextBox::new` | `SCH_TEXTBOX` constructor defaults | `same` | defaults are stable and tested | model tests | none |
| `TableCell::new` | `SCH_TABLECELL` constructor defaults | `same` | defaults/grid ownership are stable | model tests | none |
| `Stroke::new` | stroke defaults | `same` | parser-owned defaults are covered | model tests | none |
| `Fill::new` | fill defaults | `same` | parser-owned defaults are covered | model tests | none |
| `Table::new` | `SCH_TABLE` constructor defaults | `same` | border/separator defaults are locked | model tests | none |
| `Table::add_cell` | table cell ownership/materialization | `same` | table-grid ownership is now explicit and tested | model tests | none |
| `Table::get_cell` | local support | `not_applicable` | no upstream parser counterpart | source inspection | none |
| `Table::row_count` | local support | `not_applicable` | no upstream parser counterpart | source inspection | none |
| `Table::next_available_cell_slot` | table-cell placement support | `same` | required for parser-owned table materialization | model tests | none |
| `Image::new` | `SCH_BITMAP` constructor defaults | `same` | defaults are stable and no longer a bottleneck | parser/model tests | none |
| `Shape::new` | schematic shape constructor defaults | `same` | defaults now live in constructors and are tested | model tests | none |
| `Symbol::new` | `SCH_SYMBOL` constructor defaults | `same` | mandatory fields/default state are close enough | model tests | none |
| `Symbol::set_field_text` | mandatory-field text mutation | `same` | live field updates now preserve metadata and owner identity | model tests | none |
| `Symbol::set_position` | `SCH_SYMBOL::SetPosition` | `same` | attached-field movement now follows owner semantics | model tests | none |
| `Symbol::set_angle` | transform/orientation mutation | `same` | separate from position flow like upstream parse branch | model tests | none |
| `Symbol::update_prefix_from_reference` | reference-prefix refresh | `same` | parser-visible side effects are covered | model tests | none |
| `Symbol::next_field_ordinal` | symbol field ordinal ownership | `same` | parser uses this correctly for user fields | model tests | none |
| `Sheet::new` | `SCH_SHEET` constructor defaults | `same` | mandatory fields/default state are close enough | model tests | none |
| `Sheet::set_position` | `SCH_SHEET::SetPosition` | `same` | owner movement now updates pins like upstream | model tests | none |
| `Sheet::set_size` | `SCH_SHEET::SetSize` / `Resize` | `same` | owner resize now reconstrains pins like upstream | model tests | none |
| `Sheet::name` | local derived helper | `not_applicable` | support accessor only | source inspection | none |
| `Sheet::filename` | local derived helper | `not_applicable` | support accessor only | source inspection | none |
| `Sheet::is_vertical_orientation` | local support | `not_applicable` | support helper only | source inspection | none |
| `Sheet::next_field_ordinal` | sheet field ordinal ownership | `same` | used correctly by parser | model tests | none |
| `SymbolPin::new` | placed symbol pin defaults | `same` | parser-owned optional state starts correctly | model tests | none |
| `Property::new` | field constructor defaults | `same` | default geometry/IDs are now explicit and tested | model tests | none |
| `Property::new_named` | classified field construction | `same` | parser builds property objects early like upstream | parser/model tests | none |
| `Property::sort_ordinal` | local sort support | `not_applicable` | no direct upstream parser counterpart | source inspection | none |
| `PropertyKind::is_user_field` | classification support | `not_applicable` | local enum helper only | source inspection | none |
| `PropertyKind::is_mandatory` | classification support | `not_applicable` | local enum helper only | source inspection | none |
| `PropertyKind::canonical_key` | field-name canonicalization | `same` | parser relies on it and behavior is stable enough | parser tests | none |
| `PropertyKind::default_field_id` | `FIELD_T` mapping | `same` | mandatory/user IDs now follow upstream | parser/model tests | none |
| `SheetPin::new` | `SCH_SHEET_PIN` constructor defaults | `same` | default geometry/side comes from owner sheet as required | model tests | none |
| `SheetPin::set_side_with_sheet_geometry` | owner side application | `same` | parser-owned side forcing now matches upstream sheet geometry flow | model tests | none |
| `SheetPin::constrain_on_sheet_edge` | owner edge constraint | `same` | parser-owned edge constraint is covered | model tests | none |

## Layer 1: Entry / Dispatch

| Upstream routine | Local routine | Status | Reason | Evidence | Next action |
| --- | --- | --- | --- | --- | --- |
| `ParseSchematic` | `parse_schematic` | `different` | broad flow is close, but top-level exactness is not fully signed off | current map + direct audit | revisit after remaining owner routines |
| `parseHeader` | inline in `parse_schematic` | `different` | local inline header/prescan path still differs from literal upstream header routine | source comparison | audit late/version failure paths |
| top-level dispatch loop | `parse_schematic_body` | `different` | dispatch coverage exists, but exact fallback/error flow is still being tightened | source comparison | finish after owner routines |
| `parsePAGE_INFO` | `parse_page_info` | `same` | one of the closest routines | existing map/tests | none |
| `parseTITLE_BLOCK` | `parse_title_block` | `same` | comment-slot and branch ownership are close enough | existing map/tests | none |
| `parseLibSymbols` wrapper | `parse_sch_lib_symbols` | `different` | structurally close, but still depends on final `parse_lib_symbol` exactness | source comparison | resolve through `parse_lib_symbol` |

## Layer 2: Shared Leaves / Subparsers

| Upstream routine | Local routine | Status | Reason | Evidence | Next action |
| --- | --- | --- | --- | --- | --- |
| `parseBodyStyles` | `parse_body_styles` | `same` | helper boundary restored and behavior is stable enough | direct audit | none |
| `parsePinNames` | `parse_pin_names` | `same` | helper boundary restored and direct behavior is stable | direct audit | none |
| `parsePinNumbers` | `parse_pin_numbers` | `same` | helper boundary restored and direct behavior is stable | direct audit | none |
| `parseStroke` | `parse_stroke` | `same` | token ownership and internal-units flow are close enough | existing tests | none |
| `parseFill` | `parse_fill` | `same` | token ownership and fill-type flow are close enough | existing tests | none |
| `parseEDA_TEXT` | `parse_eda_text` | `same` | bare `font`/`justify`/`hide`/`href` heads, direct `href` entry, and native hyperlink acceptance/rejection cases are now tight enough that it is no longer the active bottleneck | direct source comparison, native `kicad-cli` probes, and focused tests | none |
| `parseSchField` | `parse_sch_field` | `different` | parent-sensitive flow is close, but exactness still depends on parent routines and diagnostics | direct audit | revisit from active parent mismatch |
| `parseProperty` | `parse_lib_property` | `different` | constructor/order is close, but final lib-symbol exactness still depends on it | direct audit | revisit from `parse_lib_symbol` |
| `parseSchSheetPin` | `parse_sch_sheet_pin` | `same` | constructor defaults, side/geometry flow, and close ownership are now stable | direct audit + tests | none |
| `parseSchTextBoxContent` | `parse_sch_text_box_content` | `same` | owner/body split is structurally close enough | direct audit + tests | none |
| `parseSchTableCell` | `parse_sch_table_cell` | `same` | distinct cell ownership is in place | direct audit + tests | none |

## Layer 3: Library Cache

| Upstream routine | Local routine | Status | Reason | Evidence | Next action |
| --- | --- | --- | --- | --- | --- |
| `ParseSymbolDrawItem` | `parse_symbol_draw_item` | `different` | current-unit/body-style ownership is close, but branch/error exactness remains | notes + source comparison | resolve through draw-item family audit |
| `parseLibSymbol` | `parse_lib_symbol` | `same` | direct re-audit shows the broad owner routine is now structurally close enough: root-unit construction, nested-unit parsing, metadata branches, embedded-file recovery, and parser-owned finalization are no longer the active lib bottleneck; the remaining lib drift is narrower draw-item/helper exactness | direct source comparison against upstream body plus existing lib-symbol regressions | none |
| `parseSymbolArc` | `parse_symbol_arc` | `same` | remaining mismatches are no longer a primary bottleneck | existing arc tests | none |
| `parseSymbolBezier` | `parse_symbol_bezier` | `same` | branch shape and malformed-point behavior are covered | existing tests | none |
| `parseSymbolCircle` | `parse_symbol_circle` | `same` | defaults and token flow are stable enough | existing tests | none |
| `parseSymbolPolyLine` | `parse_symbol_polyline` | `same` | token set and point grammar are stable enough | existing tests | none |
| `parseSymbolRectangle` | `parse_symbol_rectangle` | `same` | corner-radius/internal-units flow is stable enough | existing tests | none |
| `parseSymbolText` | `parse_symbol_text` | `same` | hidden-text-to-field flow and effects ownership are stable enough | existing tests | none |
| `parseSymbolTextBox` | `parse_symbol_text_box` | `same` | text-box ownership and defaults are stable enough | existing tests | none |
| `parseSymbolPin` | `parse_symbol_pin` | `different` | still part of the narrower lib-symbol exactness surface | source comparison | revisit only if direct diff shows a concrete mismatch |
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
| `is_valid_lib_id_shape` | `LIB_ID::Parse` helper split | `different` | validation shape is still local rather than the real KiCad parser implementation | direct audit | revisit during lib endgame if behavior diverges |
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
| `parse_kiid` | `parseKIID` wrapper | `different` | full malformed-ID semantics are still not fully ported | notes | revisit if test/model migration is feasible |
| `parse_raw_kiid` | raw KIID path | `same` | raw-vs-normalized split is explicit and covered | recent UUID audits | none |
| `parse_kiid_atom` | `parseKIID` low-level read | `different` | tied to full UUID exactness still not signed off | notes | revisit with UUID semantics |
| `normalize_kiid` | KIID normalization/uniqueness | `different` | malformed non-UUID handling still differs from native KiCad | notes | blocked on broader fixture/test migration |
| `parse_maybe_absent_bool` | `parseMaybeAbsentBool` | `same` | current behavior is close and well covered | tests | none |
| `require_known_version` | local support | `not_applicable` | repo-local support for parser entry ordering | source inspection | none |
| `need_left` | `NeedLEFT` | `same` | stable low-level exactness | tests | none |
| `need_right` | `NeedRIGHT` | `same` | stable low-level exactness | tests | none |
| `need_symbol_atom` | `NeedSYMBOL` | `same` | quoted/symbol acceptance now matches KiCad expectations closely enough | direct audit + tests | none |
| `need_unquoted_symbol_atom` | keyword-token path | `different` | parser-wide reserved-keyword exactness is still in progress | recent keyword work | finish token sweep |
| `need_quoted_atom` | quoted-string helper | `not_applicable` | local support only | source inspection | none |
| `need_symbol_or_number_atom` | `NeedSYMBOLorNUMBER` | `same` | quoted/symbol/number acceptance is stable enough | direct audit + tests | none |
| `need_dsn_string_atom` | DSN string branch helper | `different` | depends on final keyword-tag exactness | recent keyword work | finish token sweep |
| `at_right` | local token helper | `not_applicable` | local parser support only | source inspection | none |
| `at_unquoted_symbol_with` | local token helper | `not_applicable` | local parser support only | source inspection | none |
| `current_nesting_depth` | local token helper | `not_applicable` | local parser support only | source inspection | none |
| `skip_to_block_right` | local recovery helper | `same` | used for embedded-file warning recovery only; behavior is stable enough | tests | none |
| `current` | local token helper | `not_applicable` | local parser support only | source inspection | none |
| `current_span` | local token helper | `not_applicable` | local parser support only | source inspection | none |
| `expecting` | parse diagnostic helper | `different` | final message/span parity still belongs to the endgame sweep | parser notes | revisit last |
| `unexpected` | parse diagnostic helper | `different` | final message/span parity still belongs to the endgame sweep | parser notes | revisit last |
| `error_here` | parse diagnostic helper | `different` | final message/span parity still belongs to the endgame sweep | parser notes | revisit last |
| `find_standard_page_info` | paper lookup support | `same` | stable and no longer active | direct behavior | none |
| `parse_page_info` | `parsePAGE_INFO` | `same` | tracked above at owner layer; helper remains stable | direct audit | none |
| `convert_old_overbar_notation` | legacy overbar conversion | `same` | behavior is covered and stable enough | tests | none |
| `unescape_string_markers` | KiCad string-marker unescape support | `same` | used narrowly and covered | tests | none |
| `fixup_legacy_lib_symbol_alternate_body_styles` | parser post-fixup | `same` | remaining body-style ownership moved into parser/finalization already | direct audit | none |
| `update_local_lib_symbol_links` | parser local-lib link refresh | `same` | parser-only local-lib link fixup is stable enough | tests | none |
| `flatten_local_lib_symbol` | local-lib `Flatten()` analogue | `different` | much closer now, but still part of the remaining lib-symbol exactness surface | direct audit | revisit with `parse_lib_symbol` |
| `has_legacy_alternate_body_style` | local helper | `same` | behavior is stable enough | tests | none |
| `fixup_embedded_data` | parser embedded-data finalization | `same` | metadata hydration/recovery is stable enough | tests | none |
| `resolve_groups` | deferred group resolution | `same` | parser-only group finalization is no longer active | tests | none |
| `get_item_index_by_uuid` | local group support | `not_applicable` | local support only | source inspection | none |
| `item_uuid` | local group support | `not_applicable` | local support only | source inspection | none |
| `groups_sanity_check` | group cycle repair | `same` | covered and no longer active | tests | none |
| `validation` | parse validation helper | `different` | final diagnostic exactness still pending | parser notes | revisit last |

## Implementation Rule

For any row marked `different` or `blocked`:

1. compare the upstream body directly to the local body
2. identify one concrete mismatch only
3. patch that mismatch
4. run `cargo fmt --all`
5. run `cargo test -q`
6. update the row if the function is no longer active

Do not return to loader/post-load work until every parser-only row above is either `same`,
`not_applicable`, or explicitly `blocked` for a real model reason.
