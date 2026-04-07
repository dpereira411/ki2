# AGENTS

## Purpose

This repository is not aiming for a "KiCad-inspired" parser. The target is a structural Rust port of KiCad's schematic parsing and validation flow, with behavior tracked against upstream `eeschema/sch_io/kicad_sexpr/sch_io_kicad_sexpr_parser.cpp`.

## Working Rules

1. Prefer literal upstream structure over cleaner local abstractions.
2. Port routine-by-routine in upstream order.
3. Every nontrivial parser behavior should map to a specific upstream routine or branch.
4. Do not silently accept unknown tokens just to keep parsing moving when upstream would reject them.
5. Do not introduce "neutral AST first, semantic pass later" architecture for schematic parsing. KiCad validates while constructing domain objects.
6. When a current local representation is too reduced for upstream semantics, expand the model instead of normalizing away the difference.
7. Treat current parser code as transitional unless it clearly mirrors an upstream routine.
8. Parser compatibility is judged by control flow, accepted grammar, error cases, version gates, and object construction timing, not only by whether files parse.
9. Every time you materially touch a function, update or add a short comment on that function covering:
   - which upstream routine or branch it corresponds to
   - whether it is at parity or what still diverges from upstream
   - if it is not a 1:1 upstream routine, why the local helper exists and why it is still needed

## Strict Mode

Strict mode is the default for parser-parity work in this repository.

1. Do not stop while meaningful parser/loader parity work remains in `LOCAL_PARSER_PARITY_NOTES.md`.
2. Stay in execution mode, not reporting mode. Do not treat summaries, green tests, or partial alignment as completion.
3. Prefer whole functions or tightly related routine clusters over micro-patches.
4. Do not spend a work unit on helper renames, isolated expect-string tweaks, or tiny local cleanups unless they are required to complete a larger upstream routine port already in progress.
5. Each work unit should remove a meaningful structural/code-flow mismatch with upstream.
6. If a routine is being ported, continue until the owning control flow is substantially closer to upstream, not just one branch cleaner.
7. If the Rust model blocks parity, expand the model instead of preserving a repo-local shortcut.
8. Remove duplicated local side state whenever upstream does not keep it.
9. Do not treat passing tests as completion; tests only validate the larger port.
10. Commits must correspond to substantial parser/loader parity work, not cosmetic cleanup.
11. When a work unit is committed and meaningful backlog still remains, continue directly into the next work unit instead of sending a status reply. Only surface a reply if the backlog is exhausted or a real blocker is hit.
12. A successful commit is not, by itself, a valid reason to reply. After `cargo fmt --all`, `cargo test -q`, and commit succeed, immediately start the next backlog item unless the backlog is exhausted or a real blocker prevents further local progress.
13. If the user has explicitly asked for continuous execution, any reply without exhausted backlog or a real blocker is a behavior failure. In that mode, prefer doing more work over sending a summary.
14. Do not treat the end of a turn, a clean git status, or a green test run as an implicit stopping point. Those are normal checkpoints inside execution mode, not reasons to report.
15. If backlog remains, the default action after every successful work unit is: pick the next largest mismatch, edit, test, commit, continue. Do not wait for another user prompt to resume.
16. If a reply is unavoidable, it must explain the blocker or state that the backlog is exhausted. Do not send celebratory, summary-only, or “latest progress” replies while executable parity work still remains.
17. When a real blocker is identified, do not stop at naming it. Find the concrete path to unblocking it and record that path in the backlog files (`LOCAL_PARSER_PARITY_NOTES.md`, `LOCAL_FUNCTION_PARITY_MAP.md`, or both) before treating the work as blocked.

## Parser-Only Parity Strategy

When the target is exact pre-hierarchy parser parity, use a bottom-up dependency strategy instead
of opportunistic branch chasing.

1. Treat the parser-only boundary as:
   - `src/token.rs`
   - `src/model.rs`
   - `src/error.rs`
   - `src/diagnostic.rs`
   - `src/parser.rs`
2. Treat hierarchy loading and post-load stages as out of scope until the parser-only map is
   exhausted.
3. Prefer a function-tree/parity-map workflow:
   - build or maintain a parser-only function map
   - mark each routine as `done`, `partial`, or `blocked`
   - drive work from dependency order, not from whatever mismatch is easiest to notice
4. Port bottom-up in this order:
   - token/lexer rules
   - primitive parser helpers
   - shared leaf subparsers
   - owner-sensitive mid-level routines
   - big owning parser routines
   - top-level parser entry/dispatch
5. Do not treat top-level routine coverage as proof of parity. A matching dispatch tree is only
   evidence that corresponding entrypoints exist, not that ownership, timing, token flow, or error
   behavior are 1:1.
6. The preferred completion criteria for a routine are:
   - upstream function boundary is recognizable
   - token consumption order is close to upstream
   - owner/timing of state mutation is close to upstream
   - default/fallback/error branches are close to upstream
   - direct tests cover the routine’s explicit upstream branches

### Bottom-Up Priority

1. True leaves first:
   - tokenization and token classes
   - numeric/symbol/bool helpers
   - `parseStroke`
   - `parseFill`
   - `parseEDA_TEXT`
   - `parseSchField`
   - lib `parseProperty`
2. Then owner-sensitive mid-level routines:
   - textbox/table-cell cluster
   - library draw-item cluster
   - `parseSchSheetPin`
3. Then large owner routines:
   - `parseSchText`
   - `parseSchematicSymbol`
   - `parseSheet`
   - `parseLibSymbol`
4. Then top-level parser flow:
   - `parse_schematic`
   - `parse_schematic_body`

### Working Heuristics

1. If a parent routine depends on a leaf that is still structurally wrong, fix the leaf first.
2. If a model limitation blocks a literal routine port, expand the model before forcing the parent
   routine into a repo-local shortcut.
3. Prefer tests that lock upstream syntax and branch structure, not only end-state behavior.
4. Prefer routine-cluster commits that eliminate a dependency bottleneck over cosmetic progress on
   higher-level routines.

## Source Of Truth

1. Upstream KiCad code is the authority on parser behavior and structure.
2. `AGENTS.md` should stay short and operational. Do not keep routine-by-routine parity trivia here.
3. Use these files for detailed parity work:
   - `LOCAL_FUNCTION_PARITY_MAP.md`: current parser-only backlog and status
   - `LOCAL_PARSER_PARITY_NOTES.md`: local findings, traps, and blockers
4. When a detailed local rule is no longer broadly useful as an operating instruction, move it out of `AGENTS.md` into the notes/map instead of growing this file further.

## Global Parity Rules

1. Tests should move toward upstream syntax and behavior, not the other way around.
2. Shared token readers should mirror KiCad semantics:
   - `NeedSYMBOL()` accepts bare symbols, keyword tokens, and quoted strings
   - `NeedSYMBOLorNUMBER()` also accepts quoted strings
3. Parent-sensitive property/field parsing matters:
   - symbol, sheet, and global-label mandatory fields are not generic user properties
   - `private` survives only for user fields unless upstream clearly keeps it
4. Legacy/version branches are first-class parser behavior. Port them explicitly instead of approximating them away.
5. Keep section-head ownership, close-token ownership, and state-mutation timing aligned with the upstream owning routine whenever practical.
- In `parseJunction()`, `parseNoConnect()`, and `parseBusEntry()`, keep the local branch-head token and fallback `Expecting(...)` text aligned with each routine’s own children. Do not leave those small item parsers on copied head strings from unrelated symbol/field parsers.
- In `parseBusEntry()`, keep KiCad's literal fallback `Expecting(...)` text: `at, size, uuid or stroke`, even though the local branch may match `stroke` before `uuid` in source order.
- In top-level schematic `polyline` handling, keep the too-few-points failure text capitalized exactly like upstream: `Schematic polyline has too few points`.
- In that same top-level schematic `polyline` path, keep the two-point-collapse / too-few-points decision inline in the owning dispatch branch rather than behind a trivial `parse_polyline_item()` wrapper.
- In `parseSchPolyLine()`, keep the old-version stroke-style fix too: for file versions `<= 20211123`, a `stroke` with `default` style must be rewritten to `dash`.
- In `parseSchPolyLine()`, keep KiCad's literal child set and fallback `Expecting(...)` text: `pts, uuid, stroke, or fill`. Do not accept repo-local `start` / `mid` / `end` / `center` geometry tokens there.
- In `parseSchBezier()`, keep KiCad's literal fallback `Expecting(...)` text: `pts, stroke, fill or uuid`.
- In `parseSchRectangle()`, keep KiCad's literal fallback `Expecting(...)` text: `start, end, stroke, fill or uuid`, even though the rectangle routine also handles `radius`.
- In `parseSchField()`, keep the header checks distinct the way KiCad does: `Invalid property name`, `Empty property name`, and `Invalid property value` are separate branches before field classification begins.
- In `parseSchField()`, keep KiCad's literal fallback `Expecting(...)` text: `id, at, hide, show_name, do_not_autoplace or effects`. Do not introduce an extra comma before `or effects`.
- In the lib-symbol `parseProperty()` path, keep KiCad's separate literal fallback `Expecting(...)` text: `id, at, hide, show_name, do_not_autoplace, or effects`. The library-property parser does include that comma even though `parseSchField()` does not.
- In `parseSchField()`, `private` survives only for true user fields (`FIELD_T::USER`-equivalent). Do not preserve it for `SheetUser` fields just because they are user-defined at the schematic level.
- In `parseSchField()`, `show_name` and `do_not_autoplace` should continue to accept the bare-token form as `true` through `parseMaybeAbsentBool(true)`, not only explicit `yes` / `no` payloads.
- In `parseSchField()`, keep parent-sensitive field-ID classification and canonical-name mapping inline in the routine body, like upstream, instead of routing them through separate local helpers.
- In both `parseSchField()` and library `parseProperty()`, build the field/property object as soon as the header is classified, then mutate that object through the branch loop. Do not drift back to a gather-locals-first / assemble-at-return structure where upstream constructs the field earlier.
- In `parseSchField()` / `parse_sch_field()`, library `parseProperty()` / `parse_lib_property()`, and `parseSchSheetPin()` / `parse_sch_sheet_pin()`, the child routine should consume its own `property` / `pin` section head once it owns the real parse body. Parent routines should peek and dispatch, not strip those child heads first.
- In both `parseSchField()` and library `parseProperty()`, parsed `(id ...)` payloads are legacy and should be ignored like upstream. Keep the object’s canonical field ID derived from its parent/name classification instead of overwriting it from the parsed token.
- In `parseSchematicSymbol()` / `parse_schematic_symbol()`, keep symbol-property insertion inline in the parser body: mandatory fields overwrite by field kind/ID, and nonmandatory fields overwrite existing fields by name, like upstream `GetField( field->GetName() )` before `AddField()`.
- In `parseSchematicSymbol()` / `parse_schematic_symbol()` and `parseSheet()` / `parse_sch_sheet()`, once the owning routine is parser-owned it should consume its own `symbol` / `sheet` section head token rather than relying on the top-level dispatcher to strip that token first.
- In `parseLibSymbol()` / `parse_lib_symbol()`, start each library symbol with the root `1_1` unit already present and route both top-level root draw items and nested `symbol "..._1_1"` content into that same owning unit. Do not lazily synthesize or duplicate the root unit where upstream starts with unit count 1 and adds draw items onto it.
- In `parseLibSymbol()` / `parse_lib_symbol()`, keep full library identity and item name split the way KiCad does: the parsed `LIB_ID` stays as the symbol's library identifier and unit-name prefix, while the `LIB_SYMBOL` object name is only the lib-item-name portion. Do not collapse full `Device:R`-style IDs back into one `name` field.
- In `parseLibSymbol()` / `parse_lib_symbol()`, keep nested unit lookup/synthesis inline in the parser body once that branch is parser-owned. Do not hide the current-unit bind behind a one-caller model helper.
- In `parseLibSymbol()` / `parse_lib_symbol()`, once the parser owns the current unit binding, keep draw-item-kind and draw-item accumulation inline in that routine body. Do not hide top-level or nested unit draw-item pushes behind trivial model forwarding helpers.
- In library `parseProperty()` / `parse_lib_property()`, keep the post-parse insertion policy inline in the parser body too: mandatory fields overwrite existing mandatory fields, `ki_keywords` / `ki_description` / `ki_fp_filters` / `ki_locked` mutate symbol metadata directly, and duplicate user fields are renamed with `_1`..`_9` before the parser gives up and skips the field. Do not hide that policy behind a model helper.
- In both parser-side and loader-side synthesized mandatory properties, keep KiCad `FIELD_T` IDs too. `Reference` / `Value` / `Footprint` / `Datasheet` / `Intersheet References` / `Sheetname` / `Sheetfile` should not be left on `None` placeholders when the field kind is already known.
- Keep user-field IDs aligned with upstream too. Parsed symbol/global-label user fields should carry `FIELD_T::USER` (`0`), and sheet user fields should carry `FIELD_T::SHEET_USER` (`9`) instead of being normalized to `None`.
- In loader-side symbol-reference refresh, update existing mandatory field objects in place when they already exist. Do not replace parsed field metadata like position, visibility, or effects just to refresh `Reference` / `Value` / `Footprint` text.
- In both parser-side and loader-side symbol mandatory-field refresh paths, keep `Reference` / `Value` / `Footprint` mutation inline at the owning branch once that flow is parser/loader-owned. Do not hide those in-place updates behind a generic model helper.
- In `parseSchText()` and `parseSchField()`, nested child heads like `at`, `shape`, `iref`, `uuid`, `effects`, `id`, `hide`, `show_name`, and `do_not_autoplace` are real unquoted keyword tokens. Quoted strings must not dispatch those branches.
- `fields_autoplaced` must not stay collapsed to a plain boolean. KiCad parses and stores real `AUTOPLACE_NONE` / `AUTOPLACE_AUTO` state on text, labels, symbols, and sheets, and `parseSchText()` specifically forces labels with no fields back to `AUTOPLACE_AUTO`.
- In `parseSchText()`, construct the concrete `SCH_TEXT` / label object before walking child branches, then mutate that object through the loop. Do not drift back to a gather-locals-first / assemble-at-return structure there.
- In `parseSchText()`, keep the shared text-family control flow on one owning routine loop after the concrete text/label object is constructed. Do not split the body back into separate top-level `Text` and `Label` parser arms once the goal is structural parity with upstream’s single `SCH_TEXT*` walk.
- In the lib-symbol parsers too, bare keyword branches like `private`, `hide`, `locked`, and `demorgan` should consume through the direct unquoted-keyword path once matched, not through the generic atom reader after a precheck. Keep those branches structurally aligned with upstream keyword-token handling.
- In `parseSchematicSymbol()` and `parseSheet()`, the owning routine’s child heads and nested `instances` / `variant` / `field` heads should stay on real unquoted keyword-token paths. Quoted strings must not dispatch those branches.
- In `parseSheet()`, keep KiCad's literal default `Expecting(...)` text: `at, size, stroke, background, instances, uuid, property, or pin`, even though the local routine also handles `exclude_from_sim`, `in_bom`, `on_board`, `dnp`, `fields_autoplaced`, and `fill`.
- The same unquoted-keyword-token rule applies in `parseSchSheetPin()`, `parseSchSheetInstances()`, and `parseSchSymbolInstances()`: their `at` / `uuid` / `effects` / `path` / `page` / `reference` / `unit` / `value` / `footprint` heads are parser keywords, not quoted strings.
- In top-level `parseSchSheetInstances()` and `parseSchSymbolInstances()`, keep child-head ownership local to those loops too: peek `path` and nested `page` / `reference` / `unit` / `value` / `footprint`, then consume the exact branch token inside that branch instead of flattening the loops through one eager keyword read.
- In `parseSchSheetInstances()` and `parseSchSymbolInstances()`, keep the local branch-head token and fallback `Expecting(...)` text aligned with the actual child set owned by each routine. Do not leave stale copied tokens like `path` in nested `page` / `reference` branches.
- In top-level `parseSchSheetInstances()`, keep the inner fallback literal on the upstream `path or page` text, even though only `page` is accepted there.
- In top-level `parseSchSymbolInstances()`, keep the inner fallback literal on the upstream `path, unit, value or footprint` text, even though the actual accepted child token is `reference`.
- In top-level `parseSchSheetInstances()` / `parseSchSymbolInstances()`, keep root-path insertion inline in the owning routine bodies where upstream mutates the parsed path directly. Do not hide that flow behind a generic path-normalization helper once these routines are otherwise local.
- In top-level `parseSchSheetInstances()` / `parse_sch_sheet_instances()`, for file versions `>= 20221110`, do not keep the empty root path as a normal stored sheet instance. Preserve that parsed page number on separate root-sheet state and let only non-root sheet instances stay in the screen instance list, like upstream.
- The same rule applies in schematic `text_box`, `table`, and `image` parsing: heads like `size`, `column_count`, `table_cell`, `border`, `separators`, `scale`, and `data` are parser keywords and must not be dispatched from quoted strings.
- In `parseFill()`, keep the outer branch-head and fallback `Expecting(...)` text on KiCad's literal `type or color` path. The longer `none, outline, hatch, reverse_hatch, cross_hatch, color or background` list belongs only to the nested `type` value branch.
- In `parseStroke()` and `parseFill()`, keep child-head ownership local to the owning routine too: peek `width` / `type` / `color`, then consume the exact branch token inside that branch instead of flattening the whole child dispatch through one eager keyword read.
- The same unquoted-keyword-token rule applies across the schematic shape family too: `pts`, `start`, `mid`, `end`, `center`, `radius`, `stroke`, `fill`, `uuid`, and `polyline` in `rule_area` are parser keywords, not quoted strings.
- Keep the same token boundary in low-level library/style routines: lib-pin child heads (`at`, `name`, `number`, `hide`, `length`, `alternate`, nested `effects`), lib-property child heads, and stroke/fill child heads (`width`, `type`, `color`) are parser keywords and must not be dispatched from quoted strings.
- In `parseSymbolPin()` / `parse_symbol_pin()`, keep child-head ownership local to the owning routine too: peek the nested keyword head, then consume the exact branch token inside `at` / `name` / `number` / `hide` / `length` / `alternate` rather than flattening the whole child dispatch through one eager keyword read.
- `parse_lib_symbols()` itself should also stay on the keyword-token path: the top-level `symbol` child head inside `lib_symbols` is a parser keyword and must not dispatch from a quoted string.
- Keep the same token boundary in helper grammar routines too: `parse_pts()` should require real `xy` keyword tokens, and `parse_embedded_file_body()` should require real `name` / `data` keyword tokens with the matching `Expecting( "name or data" )` behavior.
- The same token boundary applies in the core grammar entrypoints too: root `kicad_sch`, top-level `version`, `bus_alias` `members`, and raw `xy` heads in line/bezier parsing are parser keywords and must not dispatch from quoted strings.
- Boolean parsing should stay on the upstream `yes` / `no` token path too. Do not accept repo-local `true` / `false` synonyms in `parseBool()` / `parseMaybeAbsentBool()`-equivalent branches.
- Numeric parsing should stay on the upstream `NeedNUMBER()` path too. Do not accept quoted/string atoms in places that KiCad parses through `parseInt()` / `parseDouble()`.
- RGB color channels should stay on the upstream `parseInt()` path too: red/green/blue are integer tokens, while alpha remains `parseDouble()`.
- In `parseSheet()`, the `<= 20200310` legacy sheet-field-ID recovery should happen at property-parse time, like upstream, not as a later post-loop rewrite over the final property list.
- In that same `<= 20200310` `parseSheet()` recovery branch, fix the field ID itself too. Do not rewrite only `kind`/`key` and leave recovered `Sheetname` / `Sheetfile` properties on stale parsed IDs.
- In `parseSheet()`, construct the `Sheet` object up front and mutate it through the branch loop. Do not drift back to a gather-locals-first / assemble-at-return routine shape there.
- Keep the sheet routine boundary named after upstream too: once it owns schematic sheet parsing, it should not stay on a vague local `parse_sheet()` name.
- In `parseSheet()`, mandatory sheet-name and sheet-file resolution should key off field kind/ID (`SheetName` / `SheetFile`), not canonicalized key-string lookups. That keeps the post-loop validation closer to upstream `FindField(..., FIELD_T::...)`.
- In `parseSheet()` and the loaded sheet model, do not persist repo-local `name` / `filename` side-channel copies alongside the parsed `Sheetname` / `Sheetfile` fields. Upstream sheet identity lives in the field set; derived helpers should read from those fields directly.
- In `parseSheet()`, mandatory sheet field overwrite behavior should stay inline in the property branch, like upstream field accumulation, rather than through a repo-local `upsert_sheet_property()` helper.
- In `parseSheet()` / `parse_sch_sheet()`, keep sheet field ownership on the upstream accumulation path too: collect parsed properties in a separate field list during the loop, then assign that list onto the sheet at the end. Do not overwrite mandatory sheet fields in place during parse where upstream preserves parse order and duplicates.
- In `parseSheet()`, keep the nested `instances` walk inline in the sheet routine body too. The `project -> path -> page/variant` control flow should stay local to the routine instead of being hidden behind a `parse_sheet_local_instances()` helper.
- In `parseSheet()`, keep nested sheet instances on the upstream assignment flow too: stage the parsed `project -> path -> page/variant` entries locally within the `instances` branch, then assign them onto the sheet once at the end of that branch, like upstream `setInstances(...)`.
- In `parseSchematicSymbol()`, nested local instances should stay on the symbol’s owning hierarchical-reference path, but once that branch is parser-owned do not hide it behind a trivial model forwarding helper. Keep the mutation inline in the routine body, aligned with upstream `AddHierarchicalReference(...)` ownership timing.
- In `parseSchText()` / `parse_sch_text()`, global-label property insertion policy should stay inline in the owning routine body. Do not hide the mandatory `Intersheet References` overwrite vs user-field append split behind a model helper.
- In `parseSchematicSymbol()` / `parse_schematic_symbol()`, nested local hierarchical-reference insertion should stay inline in the parser routine body once the parser owns that branch. Do not hide it behind a trivial model forwarding helper.
- In `parseSheet()` / `parse_sch_sheet()`, the final nested-instance assignment should also stay inline in the parser routine body once the parser owns that branch. Do not hide it behind a trivial model forwarding helper.
- In both `parseSheet()` and `parseSchematicSymbol()`, keep `variant` parsing inline in the owning routine body too. The `name/dnp/exclude_from_sim/in_bom/on_board/in_pos_files/field` walk should not be hidden behind a shared `parse_variant()` helper if the goal is structural parity with upstream.
- In those same inline `variant` branches, initialize variant attributes from the owning symbol/sheet before applying overrides, like upstream `InitializeAttributes()`. Do not seed them from hardcoded defaults. For sheets specifically, `in_pos_files` starts false because sheets do not carry position-files exclusion.
- In `parseSheet()`, do not add repo-local post-loop requirements that `(at ...)` or `(size ...)` must be present. Upstream leaves the sheet at its default position/size if those tokens are absent.
- In nested `parseSheet()` `variant` blocks, `in_bom` uses the old wrong polarity before `20260306` and only matches the token's positive logic from `20260306` onward. Keep that exact version gate in the parser branch.
- In `parseSheet()`, keep the missing mandatory-property diagnostics capitalized like upstream: `Missing sheet name property` and `Missing sheet file property`.
- In `parseSchSheetPin()`, keep `Invalid sheet pin name` and `Empty sheet pin name` as distinct header branches, like upstream, instead of collapsing them into a generic atom parse failure.
- In `parseSchSheetPin()`, the leading shape token should also be a real unquoted symbol/keyword token. Quoted strings like `"input"` are not upstream shape tokens and must be rejected.
- In `parseSchSheetPin()`, keep `uuid` on the shared `NeedSYMBOL()` path too. Do not accept nested non-symbol tokens there through the generic string parser.
- In `parseSchSheetPin()`, construct the owning sheet-pin object with real default geometry before parsing optional children. Do not model missing `at`/side as `None`; upstream `SCH_SHEET_PIN` already has a default position and side before the optional `at` branch runs.
- In that same `parseSchSheetPin()` flow, the default side comes from the parent sheet’s current orientation at construction time. If the pin is parsed before a later `size` token, keep the earlier default side rather than recomputing it after the whole sheet finishes parsing.
- Keep the sheet-pin routine boundary named after upstream too: `parseSchSheetPin()` should not stay on a repo-local helper name once it is the real owning branch.
- In `parseSchematicSymbol()`, mandatory symbol field overwrite behavior should stay inline in the `property` branch and key off parsed field kind/ID, not a repo-local `upsert_symbol_property()` helper or key-string-only matching.
- In `parseSchematicSymbol()`, nonmandatory symbol fields still overwrite by field name, like upstream `GetField( field->GetName() )`; do not blindly append duplicate user fields with the same name.
- Keep the placed-symbol routine boundary named after upstream too: once it owns schematic symbol parsing, it should not stay on a vague local `parse_symbol()` name.
- In `parseSchematicSymbol()`, do not add repo-local post-loop requirements that `lib_id` or `(at ...)` must be present. Upstream leaves the placed symbol at its default library ID / position / orientation when those tokens are absent.
- In `parseSchematicSymbol()`, do not run a second local canonicalization pass over field names after `parseSchField()`-equivalent parsing. Mandatory symbol field naming should already be settled by the property parser.
- In `parseSchematicSymbol()`, `default_instance` and nested symbol-instance `Value` / `Footprint` updates should mutate existing mandatory field objects in place when they already exist. Do not replace parsed field metadata just to refresh the text.
- In `parseSchematicSymbol()`, construct the `Symbol` object up front and mutate it through the branch loop. Do not drift back to a gather-locals-first / assemble-at-return routine shape there.
- In `parseSchematicSymbol()`, `lib_name` should keep the distinct `Invalid symbol library name` header failure instead of going through a generic string parser.
- In `parseSchematicSymbol()`, `lib_id` should stay inline on the shared `NeedSYMBOLorNUMBER()` path, like upstream. Do not route it through a generic string helper that accepts nested non-symbol tokens or hides the `Expecting( "symbol|number" )` branch.
- In `parseSchematicSymbol()`, `mirror` should stay on the symbol-token path too. Do not accept nested non-symbol tokens there through the generic string parser before checking `x` or `y`.
- In that same `parseSchematicSymbol()` `mirror` branch, only real unquoted symbol/keyword tokens should be accepted. Quoted strings like `"x"` are not upstream mirror-axis tokens and must be rejected.
- In `parseSchematicSymbol()`, keep the `default_instance` sub-parse inline in the symbol routine body, like upstream, rather than routing it through a local helper.
- In `parseSchematicSymbol()`, do not persist repo-local `default_reference` / `default_unit` side-channel state from `default_instance`. Upstream only consumes that branch during parse; only `Value` / `Footprint` are applied onto live symbol field state there.
- In `parseSchematicSymbol()`, do not persist repo-local `default_value` / `default_footprint` side-channel state from `default_instance` either. Upstream applies those tokens directly onto the live mandatory fields and does not keep a second stored copy.
- In that inline `default_instance` branch, keep `reference` on the shared `NeedSYMBOL()` path, like upstream, instead of accepting it through the generic string parser.
- In `parseSchematicSymbol()`, keep the nested `instances` walk inline in the symbol routine body too. The `project -> path -> reference/unit/value/footprint/variant` control flow should stay local to the routine instead of being hidden behind a `parse_symbol_local_instances()` helper.
- In that same `parseSchematicSymbol()` instance/default-instance flow, keep `value` and `footprint` on the shared `NeedSYMBOL()` path too, while preserving the legacy `~` empty-string handling. Do not accept nested non-symbol tokens there through the generic string parser.
- In that same `parseSchematicSymbol()` instance/default-instance flow, keep the `Value` / `Footprint` field mutation inline in the owning routine too. Do not route those updates through a repo-local `upsert_symbol_field_text()` helper.
- In `parseSchematicSymbol()`, keep the placed-symbol `pin` sub-parse inline in the symbol routine body too. The `number -> alternate/uuid` walk should stay local to the routine instead of being hidden behind `parse_symbol_pin()`.
- In `parseSchematicSymbol()`, keep KiCad's literal default `Expecting(...)` text: `lib_id, lib_name, at, mirror, uuid, exclude_from_sim, on_board, in_bom, dnp, default_instance, property, pin, or instances`, even though the local routine also handles `convert`, `body_style`, `unit`, `in_pos_files`, and `fields_autoplaced`.
- In `parseSchTable()`, keep KiCad's literal fallback `Expecting(...)` text even where it uses historical token names like `columns` and `col_widths`. Do not silently "improve" those strings to local token spellings if the goal is structural/error parity.
- In `parseSchTable()`, keep the no-cells failure text capitalized exactly like upstream: `Invalid table: no cells defined`.
- Keep the table routine boundary named after upstream too: once it owns the schematic table grammar, it should not stay on a vague local `parse_table()` name.
- Keep the image routine boundary named after upstream too: once it owns schematic image parsing, it should not stay on a vague local `parse_image()` name.
- In that inline placed-symbol `pin` branch, keep the pin `number` on the shared `NeedSYMBOL()` path, like upstream, instead of accepting it through the generic string parser.
- In that same inline placed-symbol `pin` branch, keep `alternate` on the shared `NeedSYMBOL()` path too. Do not accept nested non-symbol tokens there through the generic string parser.
- In that same inline placed-symbol `pin` branch, keep `uuid` on the shared `NeedSYMBOL()` path too before applying the `20210126` version gate.
- In shared `variant field` parsing, keep `name` and `value` on the `NeedSYMBOL()` path, like upstream. Invalid nested tokens should fail as `Invalid variant field name` / `Invalid variant field value`, not be accepted through a generic string parser.
- In shared `variant` parsing, keep the variant `name` on the `NeedSYMBOL()` path too. Invalid nested tokens should fail as `Invalid variant name`, not be accepted through the generic string path.
- In nested symbol/sheet instance parsing, keep `project`, `path`, and instance `reference` on the shared `NeedSYMBOL()` path, like upstream. Do not accept those headers through the generic string parser.
- Keep instance-path normalization helpers named after their upstream role too. Once a helper exists only for schematic sheet/symbol instance paths, it should not stay on a vague generic name.
- In `parseSchematicSymbol()`, `default_instance.value` / `footprint` and nested symbol-instance `value` / `footprint` should update the symbol's own `Value` / `Footprint` field text during parse, like upstream `SetValueFieldText()` / `SetFootprintFieldText()`, not live only in side-channel instance structs.
- In both top-level and nested sheet-instance parsing, keep `page` on the shared `NeedSYMBOL()` path before page-number normalization. Do not accept nested lists there through the generic string parser.
- In top-level `parseSchSymbolInstances()`, keep KiCad's literal fallback `Expecting(...)` text: `reference, unit, value or footprint`. Do not let that branch drift to repo-local text like `path, unit, value or footprint`.
- Keep the top-level instance routine boundaries named after upstream too: `parseSchSheetInstances()` and `parseSchSymbolInstances()` should not stay on repo-local helper names once they are the real owning branches.
- In `parseSchematicSymbol()` and top-level `parseSchSymbolInstances()`, keep `value` / `footprint` token reads and the legacy `~`-to-empty handling inline in the owning routine branches. Do not hide that branch-local control flow behind a shared `parse_symbol_text_atom()` helper.
- In `parseGroup()`, keep group member UUIDs on the shared `NeedSYMBOL()` path too. Do not accept nested non-symbol tokens there through the generic string parser.
- In `parseLibSymbol()`, the top-level lib symbol name should use a distinct `Invalid symbol name` header branch before library-identifier validation. Do not route that header through the generic library-ID helper.
- In `parseLibSymbol()`, keep KiCad's literal fallback `Expecting(...)` text even where it is narrower than the real accepted branch set. Upstream still says `pin_names, pin_numbers, arc, bezier, circle, pin, polyline, rectangle, or text` there.
- In `parseLibSymbol()`, `extends` should use its own `Invalid parent symbol name` `NeedSYMBOL()` branch too, instead of being routed through the generic library-ID helper.
- Keep the legacy lib-symbol body-style fixup helpers named after their upstream role too. Once they mirror KiCad `HasLegacyAlternateBodyStyle()` behavior, they should not stay on vague repo-local names.
- In nested `parseLibSymbol()` unit parsing, the unit name should use its own `Invalid symbol unit name` `NeedSYMBOL()` branch before prefix/suffix validation. Do not start that branch from the generic string parser.
- In that same nested lib-symbol unit parser, keep KiCad's literal fallback `Expecting(...)` text too: `arc, bezier, circle, pin, polyline, rectangle, or text`, even though `unit_name` and `text_box` are also valid branches there.
- In nested library `unit_name` parsing, only real symbol tokens should be consumed. Non-symbol atoms like numbers should fall through to the closing-paren path and fail there, like upstream.
- In `parseLibSymbol()`, keep the nested `symbol` unit walk inline in the owning routine body once it is the real branch owner. Do not hide the unit-name/suffix validation and draw-item loop behind a separate `parse_lib_symbol_unit()` helper.
- In `parseLibSymbol()`, `jumper_pin_groups` should only consume symbol/string pin names and should keep the upstream `Expecting( "list of pin names" )` behavior for invalid members. Do not accept arbitrary atoms there through the generic string parser.
- In library `parseProperty()` / `parse_lib_property()`, keep the same distinct header failures as upstream: `Invalid property name`, `Empty property name`, and `Invalid property value`.
- In library `parseProperty()` / `parse_lib_property()`, keep mandatory overwrite, `ki_*` special cases, duplicate user-field renaming, and final property insertion inside the property parser itself, like upstream. Do not split that flow across `parseLibSymbol()` and a returning helper.
- In library `parseProperty()` / `parse_lib_property()`, preserve `private` on the parsed field object even for mandatory lib fields. Upstream sets `private` before mandatory-field overwrite; do not strip it back to user-fields only.
- In `parseLibSymbol()`, construct the `LibSymbol` object up front and mutate it through the branch loop. Do not drift back to a gather-locals-first / assemble-at-return routine shape there.
- In library-cache parsing, keep the same helper boundaries KiCad uses where they materially define control flow: `parseBodyStyles()`, `parsePinNames()`, `parsePinNumbers()`, and `ParseSymbolDrawItem()` should exist as real helper code paths instead of staying flattened into one giant `parseLibSymbol()` body.
- In that library property branch, keep `show_name` and `do_not_autoplace` handling aligned with upstream `parseMaybeAbsentBool(true)` semantics, not just `id/at/hide/effects`.
- In that same library property branch, `effects` must still update field visibility when it contains `hide`, like upstream `parseEDA_TEXT()`. Do not leave `visible = true` just because the hide came through `effects` instead of the standalone `hide` token.
- In that same library property branch, keep symbol-field classification and canonical mandatory-field naming inline in `parse_lib_property()`, like upstream, instead of routing it through a repo-local helper.
- In `parseLibSymbol()` itself, duplicate property handling should stay inline in the owning `property` branch and key off parsed field kind/ID for mandatory fields, not a repo-local `upsert_lib_symbol_property()` helper or a second canonicalization pass after `parse_lib_property()`.
- In the library draw-item parsers too, `private` should stay inline in each owning draw-item routine rather than being hidden behind a shared `parse_lib_shape_prefix()` helper. Keep that token flow local to the draw-item parser, like upstream.
- In `parseLibSymbol()` and nested library-unit parsing, draw-item dispatch and top-level draw-item accumulation should stay inline in the owning routines too. Do not route them through repo-local `parse_lib_draw_item()` / `push_lib_draw_item()` helpers that hide the actual branch structure and close-token ownership.
- In that same library draw-item cluster, keep `ParseSymbolDrawItem()` / `parse_symbol_draw_item()` token-driven too: dispatch from the current draw-item head inside the routine, not from a passed string copied out by the caller.
- Keep library-cache property insertion and draw-item accumulation on the `LibSymbol` / `LibSymbolUnit` owning objects too. Do not leave `parseLibSymbol()` / `parse_lib_property()` doing repeated parser-side vector surgery for mandatory property overwrite, duplicate user-field renaming, or root `1_1` draw-item accumulation.
- In nested `parseLibSymbol()` unit parsing, do not stage draw items into a temporary unit and merge them back after the loop. Resolve the owning `LibSymbolUnit` first and mutate that unit directly through the nested parse flow, like upstream `AddDrawItem()` ownership.
- In that same nested `parseLibSymbol()` unit branch, bind the owning unit once at branch entry and mutate that exact unit through `unit_name` and draw-item children. Do not keep re-looking up the same unit on every nested child branch once it has been resolved.
- In those same library shape parsers, keep the initial `LibDrawItem` construction inline per routine too. Do not hide the default object state behind a shared `empty_lib_draw_item()` helper if the goal is routine-by-routine structural parity with upstream.
- Keep the library draw-item routine boundaries named after upstream too. Once a routine owns library `arc` / `bezier` / `circle` / `polyline` / `rectangle` parsing, it should not stay on a repo-local `*_draw_item` helper name.
- In library `parseSymbolPin()` / `parse_lib_pin_draw_item()`, keep `name`, `number`, and `alternate` name on strict symbol-token paths with their own distinct invalid-name branches. Do not accept nested lists there through the generic string parser.
- In that same library pin branch, keep the leading electrical type and graphic shape on strict symbol-token paths too, like upstream token dispatch, instead of accepting nested lists through the generic string parser.
- In that same library pin type/shape branch, only real unquoted symbol/keyword tokens should be accepted. Quoted strings like `"input"` or `"line"` are not upstream pin-type/pin-shape tokens and must be rejected.
- In that same library pin branch, keep the electrical-type and graphic-shape enum mapping inline in `parse_lib_pin_draw_item()` itself rather than behind separate `parse_lib_pin_electrical_type()` / `parse_lib_pin_graphic_shape()` helpers.
- In the library pin `alternate` branch, keep alternate type and alternate shape on those same strict symbol-token paths too, instead of accepting nested lists through the generic string parser.
- In shared `parseEDA_TEXT()`-style effects parsing, keyword branches like bare `hide`, inline `bold`/`italic`, list-head `font`/`justify`/`href`, and `justify` members must use real unquoted symbol/keyword tokens too. Quoted strings are not upstream keyword tokens there and must be rejected.
- Keep hyperlink validation helpers named after their upstream role too. Once a helper exists only to mirror KiCad hyperlink validation during text-effects parsing, it should not stay on a vague boolean-style name.
- Keep the shared `parseEDA_TEXT()` call signature caller-specific too. The local parser should thread the same `convert overbar syntax` / `enforce min text size` intent through its call sites, even where the current model only uses part of that information.
- Keep that shared effects entrypoint named after upstream too: once it is the real common text-effects parser, it should not stay on a repo-local helper name like `parse_effects_summary()`.
- In the shared `parseEDA_TEXT()` `font` branch, keep KiCad's literal fallback `Expecting(...)` text: `face, size, thickness, line_spacing, bold, or italic`, even though that branch also handles `color`.
- In that same shared `parseEDA_TEXT()` path, `font face` and `href` payloads should stay on the shared `NeedSYMBOL()` token path too. Do not accept numeric/non-symbol atoms there through the generic atom reader.
- In that same shared `parseEDA_TEXT()`-style effects path, return and thread the real `TextEffects` struct directly. Do not hide it behind a repo-local `EffectsSummary` wrapper.
- In that same shared `parseEDA_TEXT()`-style effects path, keep nested `font` and `justify` walks inline in the owning routine once they have shrunk to one-caller local subloops. Do not keep separate `parse_effects_font()` / `parse_effects_justify()` wrappers around those branches.
- In that same `parseEDA_TEXT()` core, keep child-head ownership local to the nested `font` and `justify` loops too: peek `face` / `size` / `thickness` / `color` / `line_spacing` / `bold` / `italic` and `left` / `right` / `top` / `bottom` / `mirror`, then consume the exact branch token inside that branch instead of flattening those nested loops through one eager keyword read.
- Keep the bare `bold` / `italic` defaulting logic inline in `parseEDA_TEXT()` too once it has shrunk to one-caller token handling. Do not leave it behind a one-off `parse_inline_optional_bool()` helper.
- In `parseEDA_TEXT()`, keep text/visibility mutation on caller-owned state the way upstream mutates `EDA_TEXT` directly. Do not drift back to a detached `TextEffects` summary-return path that hides overbar conversion and visibility ownership from the caller.
- In `parseEDA_TEXT()`, let the shared routine own the nested `effects` section head and its closing `)` too. Callers should dispatch on `effects`, then hand control to `parseEDA_TEXT()` without pre-consuming that nested list head or post-consuming its close token.
- Ownership parity means matching KiCad's real state owner, mutation timing, and routine boundary together. Do not treat "move it into a model/helper method" as a generic cleanup rule if upstream keeps the logic inline in the parser routine.
- Use owning-object mutation when upstream constructs the object early and mutates it through the parse flow. Do not keep repo-local side channels, detached staging state, or late synthetic replacement where KiCad mutates the real object in place.
- Do not let ownership cleanup hide upstream parser flow. If KiCad keeps insertion, overwrite, or special-case policy inline in a parser routine, keep that policy inline there instead of pushing it behind a generic model helper.
- In `parseSheet()` / `parse_sch_sheet()`, keep sheet fields on the upstream accumulation path: parse fields into a separate ordered list during the loop, then assign that list onto the sheet at the end. Do not collapse that into eager in-place mandatory-field overwrite just because the sheet owns the final field list.
- In library `parseProperty()` / `parse_lib_property()`, keep the post-parse insertion policy inline in the property parser. Mandatory overwrite, `ki_*` metadata handling, duplicate-user-field renaming, and final add/skip behavior should not be hidden behind a generic `LibSymbol` insertion helper.
- In schematic/library free-text parsing (`parseSchText()` and library `parseSymbolText()`), do not eagerly convert old overbar notation at payload-read time. For those text objects, the upstream conversion point is the later `parseEDA_TEXT()` effects path when conversion is enabled.
- The same rule applies to schematic/library `VALUE` fields: once `parseEDA_TEXT()` owns caller-side text mutation, do not pre-convert legacy overbar notation in the property parser right before calling it. Without an `effects` branch, the raw legacy field text should stay raw.
- In parser-wide enum/keyword branches such as lib-symbol `power` scope and stroke/fill `type`, only real unquoted symbol/keyword tokens should be accepted. Quoted strings like `"local"`, `"dash"`, or `"color"` are not upstream enum tokens and must be rejected.
- Keep the stroke parser body in `parse_stroke()` itself. Do not hide it behind a repo-local `parse_stroke_with_seed()` wrapper when the schematic parser only uses the direct no-seed path.
- In lib-symbol `pin_names` / `pin_numbers`, the legacy bare `hide` form must be a real unquoted keyword token. Quoted `"hide"` should not be treated like the pre-20241004 bare keyword branch.
- In those same `pin_names` / `pin_numbers` helpers, nested list-head keywords like `hide` and `offset` must also be real unquoted keyword tokens. Quoted strings like `("hide" yes)` or `("offset" 0.5)` are not upstream helper branches and must be rejected.
- In lib-symbol `body_styles`, the special `demorgan` marker must be a real unquoted keyword token. Quoted `"demorgan"` is just a body-style name, not the upstream keyword branch.
- In `parseLibSymbol()`, top-level child heads like `power`, `body_styles`, `pin_names`, `pin_numbers`, `property`, `extends`, `symbol`, `embedded_fonts`, `embedded_files`, and the draw-item kinds must dispatch from real unquoted keyword tokens. Quoted strings like `("power" local)` are not upstream branch heads and should fall into the existing warning-and-skip recovery with KiCad's stale `Expecting(...)` text.
- In the low-level `parseLibSymbol()` helper cluster (`power`, `parseBodyStyles()`, `parsePinNames()`, `parsePinNumbers()`), keep child-head ownership local to the owning routine too: peek `global/local`, `offset`, and `hide`, then consume the exact branch token inside that branch instead of flattening those helper loops through one eager keyword read.
- In `ParseSymbolDrawItem()` / `parse_symbol_draw_item()` and the nested lib `embedded_files` parser, keep child-head ownership local too: peek the draw-item or embedded-file child head first, then consume the exact branch token inside that branch instead of flattening the whole dispatcher or file-body loop through one eager keyword read.
- Keep the owning `lib_symbols` block routine named after its upstream role too. Once it is the real owner of the top-level library-cache symbol loop, it should not stay on a vague repo-local wrapper name.
- In top-level and lib-symbol `embedded_files` parsing, the only valid child head is `file`, and that head should dispatch from the real unquoted keyword-token path with the matching `Expecting( "file" )` fallback.
- In those same top-level and lib-symbol `embedded_files` branches, keep the `file` loop inline in the owning routine too. Do not hide the branch-local `file` dispatch and warning-recovery boundary behind a shared `parse_embedded_files_block()` helper.
- In those same top-level and lib-symbol `embedded_files` branches, keep each `file` body parse inline in the owning routine too. Do not hide the local `name/data` walk behind a shared `parse_embedded_file_body()` helper.
- Keep the top-level `embedded_files` branch inline in `parse_schematic_body()` as well once it has shrunk to the local version-gate plus `file` loop and warning recovery. Do not keep a trivial `parse_embedded_files()` wrapper around that switch branch.
- Parser-wide bare keyword probes like `private`, group `locked`, and pre-20241004 bare lib-pin `hide` must also require real unquoted keyword tokens. Quoted strings must not trigger those branches.
- In `parseGroup()`, keep the branch-head token and fallback `Expecting(...)` text local to the routine: `uuid, lib_id, members`. Do not leave stale copied head strings from unrelated style parsers in the group branch.
- In lib-symbol `jumper_pin_groups`, member names must stay on the quoted-string path like upstream `DSN_STRING` handling. Unquoted pin names are not valid there.
- In lib-symbol draw-item parsers (`arc`, `bezier`, `circle`, `polyline`, `rectangle`, `text`, `text_box`), nested list-head keywords like `start`, `mid`, `end`, `radius`, `pts`, `at`, `size`, `stroke`, `fill`, `margins`, and `effects` must be real unquoted keyword tokens. Quoted strings must not dispatch those branches.
- In library `parseSymbolRectangle()` / `parse_lib_rectangle_draw_item()`, keep KiCad's literal fallback `Expecting(...)` text: `start, end, stroke, or fill`, even though the rectangle routine also handles `radius`.
- In that same library draw-item family, point-list `xy` entries must stay on the real unquoted keyword-token path too. Quoted `"xy"` inside lib-symbol `bezier`/`polyline` point lists is not an upstream branch head and must be rejected.
- In schematic/library `pts` branches, keep the nested `xy` loop inline in the owning routine once that branch has shrunk to direct point-list handling. Do not hide it behind a shared `parse_pts()` helper.
- In library `parseSymbolText()` / `parse_lib_text_draw_item()`, keep the text payload on its own `Invalid text string` symbol-token path before entering the body parser.
- In library `parseSymbolText()` / `parse_lib_text_draw_item()`, hidden text should not stay on a transitional local `converted_to_field` marker. Keep the upstream result shape: invisible library text parses as a `field` draw item directly.
- In library `parseSymbolText()` / `parse_lib_text_draw_item()` and `parseImage()`, construct the owning item up front and mutate it through the branch loop. Do not drift back to gather-locals-first / assemble-at-return flow in those routines.
- In `parseSchTable()` and the schematic shape family (`parseSchPolyLine()`, `parseSchArc()`, `parseSchCircle()`, `parseSchRectangle()`, `parseSchBezier()`), construct the owning object up front and mutate it through the branch loop. Do not drift back to gather-locals-first / assemble-at-return flow in those routines.
- In the connectivity-item family (`parseJunction()`, `parseNoConnect()`, `parseBusEntry()`, and `parseLine()` / `parse_sch_line()`), construct the owning object up front and mutate it through the branch loop too. Do not drift back to gather-locals-first / assemble-at-return flow there.
- In the pin-parser family (`parseSymbolPin()` / `parse_symbol_pin()` and `parseSchSheetPin()` / `parse_sch_sheet_pin()`), construct the owning pin object up front and mutate it through the branch loop too. Do not drift back to gather-locals-first / assemble-at-return flow there.
- In the instance-list family (`parseSchSheetInstances()` / `parse_sch_sheet_instances()` and `parseSchSymbolInstances()` / `parse_sch_symbol_instances()`), construct each instance object as soon as the `path` header is read and mutate it through the nested child loop. Do not drift back to gather-locals-first / assemble-at-return flow there.
- In the small aggregate parsers (`parseBusAlias()` / `parse_bus_alias()` and `parseGroup()` / `parse_group()`), construct the owning aggregate object up front and mutate it through the branch loop too. Do not drift back to detached local accumulators there.
- In top-level `parse_schematic_body()` / `ParseSchematic()` flow, append parsed schematic items and top-level instance records directly onto the owning screen as each branch completes. Do not drift back to detached top-level item/instance return vectors when upstream mutates `screen` in those branches.
- In `parseSchematicSymbol()` / `parse_schematic_symbol()` and `parseSheet()` / `parse_sch_sheet()`, construct nested local instance objects as soon as the `path` token is read, then mutate those objects through `reference` / `unit` / `value` / `footprint` / `page` / `variant` parsing. Do not drift back to detached local accumulators for nested instances or variants there.
- In the nested `instances` / `variant` / variant-`field` flows under `parseSchematicSymbol()` and `parseSheet()`, keep child-head ownership local to each owning loop too: peek `project` / `path` / `reference` / `unit` / `value` / `footprint` / `page` / `variant` / `name` / `dnp` / `exclude_from_sim` / `in_bom` / `on_board` / `in_pos_files` / `field`, then consume the exact branch token inside that branch instead of flattening each loop through one eager keyword read.
- In the nested `instances` / `variant` / variant-`field` flows under `parseSchematicSymbol()` and `parseSheet()`, keep variant ownership keyed by variant name and variant-field ownership keyed by field name, like upstream `m_Variants[variant.m_Name] = variant` and `variant.m_Fields[fieldName] = fieldValue`. Do not append duplicate variants or duplicate variant fields as independent list entries.
- In library `parseSymbolTextBox()` / `parse_lib_text_box_content()`, keep the text payload on that same `Invalid text string` symbol-token path before entering the body parser.
- In library `text_box` parsing, keep the body walk and `LibDrawItem` construction in `parseSymbolTextBox()` / `parse_lib_text_box_draw_item()` itself. Do not hide that routine behind a separate `parse_lib_text_box_content()` helper.
- In library `parseSymbolTextBox()` / `parse_lib_text_box_draw_item()`, keep KiCad's literal fallback `Expecting(...)` text: `at, size, stroke, fill or effects`, even though the routine also handles legacy `start/end` and `margins`.
- For the `paper` / `page` area, the remaining exactness after the local helper chain is ported is parser-wide token-category adoption. If a future discrepancy in this area requires broader `NeedSYMBOL` / `NeedNUMBER` parity outside the dedicated page helpers, treat that as a wider lexer/parser task rather than another local `parsePAGE_INFO()` branch.
- The first missing cross-file post-load stage belongs in the loader-side hierarchy flow, not as scattered parser fixups. Build a real loaded sheet-path list before applying root-screen `symbol_instances` / `sheet_instances`.
- Keep that loaded sheet-path list structurally close to upstream `BuildSheetListSortedByPageNumbers()`: root path first as an explicit hierarchy entry, then child entries derived from sheet UUID links, with page-number assignment applied from the root screen’s `sheet_instances`.
- For legacy `< 20221002` files, apply root-screen `symbol_instances` across the loaded hierarchy using those sheet paths rather than leaving `SetLegacySymbolInstanceData()` as a dead partial fixup. The effective symbol update belongs in the post-load flow, not in per-file parsing.
- Once that loaded sheet-path list is sorted by page data, keep sheet-number/count assignment on the same loader-side hierarchy object. That is the base state later cross-file stages such as intersheet-reference recomputation should consume, not a parser-local approximation.
- Recompute intersheet references from that loader-side sheet list too. Global-label `Intersheet References` state should be derived from the full loaded hierarchy’s sheet-number/page map, not left as whatever the per-file parser happened to see.
- Use that same loader-side sheet list to drive `UpdateAllScreenReferences()`-style symbol refresh from hierarchical local `instances`, including live `Reference` / `unit` / `Value` / `Footprint` refresh where the loaded symbol instance carries them. Do not regress to an older local split where `value` / `footprint` only refreshed through the legacy top-level `symbol_instances` path.
- The legacy loader-side `UpdateSymbolInstanceData()` pass should also materialize hierarchical symbol-instance state on the symbol itself, not only refresh live fields/unit. Upstream adds hierarchical reference data as well as updating `Reference` / `Value` / `Footprint`.
- In the loader-side post-load flow, keep the pre-`20230221` legacy global-power fix too: if a placed symbol resolves to a global power lib symbol whose first lib pin is hidden `power_in`, its value field must be corrected to that pin name after load.
- Keep owning-object field mutation on the model too. Symbol, sheet, and global-label field updates should go through object-owned mutation paths instead of repeated parser/loader-side vector surgery.
- In `parseSchematicSymbol()` and loader-side symbol refresh, `Reference` / `Value` / `Footprint` updates should use the symbol’s owning field-mutation path so existing field metadata survives and canonical field identity stays local to the symbol object.
- Keep that owning symbol field-mutation path shared across parser and loader follow-up stages too: local/default instance text updates, legacy root `symbol_instances`, power-symbol fixes, and post-load annotation/reference refresh should all update `Reference` / `Value` / `Footprint` through the same symbol-owned path instead of open-coding repeated property-vector surgery.
- In `parseSheet()`, `Sheetname` / `Sheetfile` insertion should use the sheet’s owning field-mutation path too. That path should also normalize `Sheetfile` text to forward slashes like upstream `SCH_SHEET::SetFields()`.
- In `parseSheet()`, keep that ownership on the final assignment path: accumulate parsed sheet properties locally, then hand the whole field list to the sheet’s owning setter for canonical `Sheetfile` normalization. Do not open-code `Sheetfile` slash normalization in the parser branch after the owning sheet path exists.
- In `parseSchText()` and loader-side intersheet-ref recompute, global-label `Intersheet References` mutation should use the label’s owning mandatory-field path rather than open-coding mandatory-field construction/replacement at each call site.

## Expected Workflow

1. Identify the exact upstream routine(s) being ported.
2. Read the relevant upstream code first.
3. Patch local model/parser/loader to mirror that routine as directly as practical.
4. Add or update regression tests using upstream-shaped input.
5. Run `cargo test`.

## What To Avoid

- "Rustier" parser redesigns that obscure upstream control flow.
- Generic catch-all shape or property parsers when upstream has specialized routines.
- Silent skips for unsupported nested constructs.
- Expanding surface area without tying it back to a real upstream branch.
