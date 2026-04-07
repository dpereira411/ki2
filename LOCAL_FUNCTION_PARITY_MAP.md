# Parser-Only Function Parity Audit

Target: exhaust pre-hierarchy parser parity by auditing every parser-boundary function and support
function against upstream KiCad, then iterating the unresolved items one by one.

Current state:
- every parser-only routine is now either `same`, `not_applicable`, or explicitly `blocked`
- parser-only routine work is exhausted in the current model
- active parity work has moved to `src/loader.rs` / post-load flow
- simulation-model parity is no longer the primary queue; ERC-critical loader parity takes
  precedence and sim-model work is deferred to the end of the backlog

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

1. tighten `Error` / `Diagnostic` display wording for native parse-error parity

This queue is intentionally parked while loader/post-load parity is active. Do not reopen routine
work in `src/parser.rs` unless one of the blocked surfaces is explicitly being unblocked.

## Loader / ERC Priority

When working beyond the parser boundary, use this loader-side priority order:

1. `UpdateAllScreenReferences`
2. `UpdateSymbolInstanceData`
3. `UpdateSheetInstanceData`
4. `SetSheetNumberAndCount`
5. `RecomputeIntersheetRefs`
6. `FixLegacyPowerSymbolMismatches`
7. `MigrateSimModels` last

Reason:
- the first six items materially affect hierarchy/current-sheet/reference state and ERC-visible
  symbol/sheet behavior
- `MigrateSimModels` is simulation-facing parity, not a prerequisite for hierarchy loading or core
  ERC behavior

Current ERC blocker:
- direct re-audit did not find another honest branch-level mismatch in
  `UpdateAllScreenReferences`, `UpdateSymbolInstanceData`, or `UpdateSheetInstanceData`
- the remaining executable gap is model-shaped: the loader has no notion of an active variant per
  loaded occurrence, so parsed symbol/sheet `instance.variants` data cannot be applied onto live
  selected-occurrence state
- partial unblock landed:
  - the loaded project now carries `current_variant`
  - live symbol occurrence refresh is variant-aware and restores baseline state when the selected
    occurrence or selected variant changes
  - live sheet objects now apply current-variant state through the selected local sheet occurrence
    when one matches the current sheet path, with first-instance fallback in the current model
  - `SchematicProject` now carries the same `current_variant` session state and reuses the same
    variant-aware occurrence refresh helpers as `LoadResult`
- remaining blocker is narrower:
  - direct upstream re-audit showed `SCHEMATIC::GetCurrentVariant()` is schematic-owned session
    state, not project-file state
  - local `LoadResult.current_variant` is therefore the right current-architecture analogue
  - current-sheet-scoped intersheet-ref refresh is now structurally closer to upstream:
    hierarchy-wide page-ref computation stays separate from current-sheet label refresh, and
    non-current screens keep their parsed intersheet-ref field text
  - remaining blocker under that same routine is narrower:
    - hierarchy-wide intersheet page-ref recompute and current-sheet intersheet display now use a
      reduced sheet-path shown-text resolver for global labels instead of raw `label.text`
      - that exercised slice now also includes current-variant sheet-field / `DNP` resolution and
        variant-triggered page-ref-map recompute
      - the reduced resolver also now covers schematic `VARIANT` / `VARIANTNAME` plus stable
        title-block tokens used by upstream before project vars
      - remaining divergence is the broader unported text-variable resolver surface, not this
        exercised intersheet-ref branch
    - companion `.kicad_pro` `drawing.intersheets_ref_show` and
      `drawing.intersheets_ref_own_page` are now honored when present, and the current tree also
      honors project-backed `short` / `prefix` / `suffix` formatting on current-sheet intersheet
      refs; the no-project path now uses KiCad's current default schematic-settings values, and
      load/project refresh now share one typed intersheet-settings carrier instead of scattered raw
      JSON scalar lookups. What remains is the broader KiCad schematic-settings surface beyond that
      typed intersheet subset
      - unblock path recorded in `LOCAL_PARSER_PARITY_NOTES.md`
    - current Rust shapes now refresh a reduced hatch cache on selected screens, but still do not
      carry KiCad's fuller polygon/knockout hatching state behind `shape->UpdateHatching()`
      - unblock path recorded in `LOCAL_PARSER_PARITY_NOTES.md`
  - broader ERC semantics that depend on richer occurrence-aware symbol/sheet state remain blocked
    on that fuller model
- do not keep reopening those three routines for blind branch chasing until that occurrence/variant
  model is expanded

If diagnostic/error unblocking is chosen, execute it in this order:
1. audit `src/error.rs` / `src/diagnostic.rs` and enumerate the parser fields lost by the current
   reduced representation
   - done
2. expand the diagnostic model so parser helpers can carry structured source/location/expectation
   data
   - done
3. retarget `expecting`, `unexpected`, `error_here`, and `validation` to build structured
   diagnostics first
   - done
4. add focused exactness tests for representative parser and validation failure families
   - done
5. tighten final `Display` / formatting behavior to match native KiCad wording as far as the local
   CLI model can support
   - active, but now narrowed to local CLI wording polish after the line/column display cleanup

## Layer 0: Support Files

### `src/token.rs`

| Local function | Upstream counterpart | Status | Reason | Evidence | Next action |
| --- | --- | --- | --- | --- | --- |
| `skip_utf8_bom` | DSN lexer BOM skip | `same` | helper-only and behavior is already locked | token tests | none |
| `prescan_version` | none; local lexer prescan | `not_applicable` | repo-local support for early bar-mode setup only; direct re-audit plus header/version regressions cover the remaining parser-facing behavior closely enough | token tests + top-level header regressions | none |
| `is_line_comment_start` | DSN lexer comment detection | `same` | comment start behavior is now covered and matches parser entry needs closely enough | token tests | none |
| `skip_whitespace_and_line_comments` | DSN lexer whitespace/comment skip | `same` | line comments, BOM, and NUL whitespace are covered and stable | token tests | none |
| `is_dsn_number` | DSN lexer number classification | `same` | current grammar matches KiCad-style number token behavior closely enough | token tests | none |
| `is_schematic_keyword` | KiCad keyword token table | `same` | direct re-audit now covers the remaining parser-facing keyword surfaces closely enough: real unquoted parser heads are reserved, DSN-string leak paths are locked, and the remaining parser-only risk is no longer in broad keyword tagging | keyword-tag commits plus direct branch-head audit and regressions | none |
| `lex` | top-level lexer entry | `same` | direct re-audit plus focused tests now cover the parser-facing lexer behavior closely enough: BOM/comment/NUL skipping, number classification, quoted escapes, and bar-mode setup are no longer active bottlenecks | token tests + parser entry regressions | none |
| `decode_quoted_escape` | DSN quoted-string escape decoding | `same` | KiCad-style escape decoding is now covered with focused tests | token tests | none |
| `lex_with_bar` | DSN lexer body | `same` | direct re-audit plus token regressions now cover the parser-facing DSN lexer behavior closely enough: quoted escapes, malformed raw newlines, bar delimiters, and keyword tagging no longer leave an active parser-only bottleneck here | token tests + parser regressions | none |

### `src/diagnostic.rs`

| Local function | Upstream counterpart | Status | Reason | Evidence | Next action |
| --- | --- | --- | --- | --- | --- |
| `Diagnostic::error` | parse error construction | `blocked` | structured diagnostic kinds plus raw spans and 1-based line/column source positions now exist, and rendered validation errors now prefer KiCad-style line/column output over repo-local byte-span text; the remaining gap is narrower local CLI wording fidelity | parser notes + diagnostic regressions | tighten final formatting wording only if a concrete wording mismatch is found |
| `Diagnostic::with_path` | none; local support | `not_applicable` | local helper only | source inspection | none |
| `Diagnostic::with_span` | none; local support | `not_applicable` | local helper only | source inspection | none |

### `src/error.rs`

No nontrivial function bodies live here, but the file remains in scope because final parser parity
still depends on error/diagnostic exactness.

| Local item | Upstream counterpart | Status | Reason | Evidence | Next action |
| --- | --- | --- | --- | --- | --- |
| `Error` enum formatting | parse/validation error reporting | `blocked` | structured parser-helper data and source positions now exist, parser/validation failures no longer split on a repo-local `validation error at ...` prefix, and rendered validation locations now use line/column instead of local byte-span noise; the remaining gap is narrower local CLI wording fidelity in rendered errors | parser notes + diagnostic regressions | tighten `Display` formatting wording only if a concrete mismatch is found |

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
| `ParseSchematic` | `parse_schematic` | `same` | direct re-audit shows the broad top-level owner flow is now structurally close enough: root/header entry, version gating, generator-version future-version timing, root-uuid seeding, and post-parse fixup ordering are no longer active parser-only bottlenecks | direct source comparison plus focused header/top-level regressions | none |
| `parseHeader` | inline in `parse_schematic` | `not_applicable` | the header routine is intentionally inlined locally; direct re-audit shows version/default handling and late/future-version failure timing are close enough that the separate upstream helper no longer needs to stay on the active queue | direct source comparison plus focused header/version regressions | none |
| top-level dispatch loop | `parse_schematic_body` | `same` | direct re-audit shows the remaining parser-only risk is no longer in the broad dispatch loop: accepted section set, old `page` remap, embedded-files recovery, and literal fallback text are close enough to upstream | direct source comparison plus top-level section regressions | none |
| `parsePAGE_INFO` | `parse_page_info` | `same` | one of the closest routines | existing map/tests | none |
| `parseTITLE_BLOCK` | `parse_title_block` | `same` | comment-slot and branch ownership are close enough | existing map/tests | none |
| `parseLibSymbols` wrapper | `parse_sch_lib_symbols` | `same` | direct re-audit shows the wrapper loop is now structurally aligned: it owns the `lib_symbols` head, requires only `symbol` children, and dispatches directly into `parse_lib_symbol()` like upstream | direct source comparison | none |

## Layer 2: Shared Leaves / Subparsers

| Upstream routine | Local routine | Status | Reason | Evidence | Next action |
| --- | --- | --- | --- | --- | --- |
| `parseBodyStyles` | `parse_body_styles` | `same` | helper boundary restored and behavior is stable enough | direct audit | none |
| `parsePinNames` | `parse_pin_names` | `same` | helper boundary restored and direct behavior is stable | direct audit | none |
| `parsePinNumbers` | `parse_pin_numbers` | `same` | helper boundary restored and direct behavior is stable | direct audit | none |
| `parseStroke` | `parse_stroke` | `same` | token ownership and internal-units flow are close enough | existing tests | none |
| `parseFill` | `parse_fill` | `same` | token ownership and fill-type flow are close enough | existing tests | none |
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
