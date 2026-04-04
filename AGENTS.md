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

## Specific Learnings

- `bus_alias` must follow the KiCad form: `(<bus_alias> <name> (members ...))`, including old overbar conversion before `20210621`.
- In `bus_alias`, `members` entries must stay on the quoted-string path like upstream `Expecting( "quoted string" )` handling. Unquoted members are not valid there.
- In `parseBusAlias()`, keep the alias name itself on the shared `NeedSYMBOL()` path, like upstream. The members loop can still report `quoted string`, but the leading alias token should not go through the generic string parser.
- In `parseBusAlias()`, do not add a repo-local non-empty-members validation. Upstream accepts an empty `(members)` list and still adds the alias.
- In `parseBusAlias()`, invalid member tokens should fail as `Expecting( "quoted string" )`, not through the generic missing-atom path.
- `group` declarations are parsed first and resolved after the rest of the file. Do not eagerly fold them into generic item parsing.
- In `parseGroup()`, the optional pre-list group name should only accept a quoted string, with bare `locked` as the only non-string token allowed before the first nested list. Do not accept an unquoted group name there.
- In `parseGroup()`, keep the `lib_id` branch separate from the symbol/library helper. Upstream uses the same parse rules but a group-specific invalid-character diagnostic: `Group library link ... contains invalid character ...`.
- `polyline` is not equivalent to `wire`/`bus`. Two-point polylines collapse to line-like objects; longer ones remain shapes.
- `rule_area` grammar is specialized and wraps a nested `polyline`; it is not just another generic point-list shape.
- In `parseRuleArea()`, keep the local branch-head token and fallback `Expecting(...)` text aligned with the actual routine body too: `polyline, exclude_from_sim, in_bom, on_board, or dnp`. Do not leave a stale reordered child list there after branch churn.
- `text`, `label`, `global_label`, `hierarchical_label`, `directive_label`, and `netclass_flag` should be treated as one shared parser family with type-specific branches, like upstream `parseSchText()`.
- `property` parsing is parent-sensitive. Symbol, sheet, and global-label mandatory fields are not just arbitrary user properties.
- `private` only survives for user fields; it should not be blindly preserved on mandatory fields.
- Legacy compatibility branches matter: `~` empty-string handling, root-path normalization, old overbar notation, pin UUID version gates, legacy `iref`, and similar cases should be ported explicitly rather than approximated.
- Keep the root schematic `uuid` on the shared `NeedSYMBOL()` path too. Do not leave the top-level UUID branch on the generic string parser once the rest of the UUID family has been tightened.
- In top-level `ParseSchematic()` header handling, keep `generator` on the shared `NeedSYMBOL()` path and keep the really old `< 20200827` extra `host` version token unconditional, like upstream. Do not guard that old host-version read behind a local `at_atom()` shortcut.
- In top-level `parse_schematic_body()` dispatch, keep the section-head token and its `Expecting(...)` text aligned with the real top-level section set. Do not leave that dispatcher on a stale copied head string from an unrelated parser family.
- Tests should be updated toward upstream syntax, not the other way around.
- `paper` / `page` parsing should stay split the way KiCad uses it: `parsePAGE_INFO()` for `paper` and legacy `page <= 20200506`, and a separate modern top-level `page` sniff path using `SYMBOL or NUMBER` tokens.
- Legacy top-level `page <= 20200506` should be normalized to the `paper` branch before the main schematic-section dispatch, like upstream `token = T_paper`, rather than handled as a nested special case inside the modern `page` branch.
- Keep that legacy `page -> paper` remap inline at the dispatch token/switch boundary, not in a separate normalization helper. The upstream shape is a token rewrite immediately before the branch dispatch.
- Once normalized, `paper` and modern top-level `page` should be direct dispatch branches, not extra wrapper helpers. Keep the branch bodies aligned with the upstream switch cases rather than inserting local section-mediation layers.
- That direct-dispatch rule applies literally: do not keep `parse_paper()` / `parse_page_sniff()` wrappers around those switch branches. Keep the token reads and section-close handling in `parse_schematic_body()` where upstream owns them.
- Modern top-level `page` sniff should store the two consumed `SYMBOL or NUMBER` tokens exactly as read. Do not reuse page-number normalization from `sheet_instances`/`symbol_instances` in this branch; upstream only sniffs and moves on.
- That modern `page` sniff acceptance set includes keyword tokens too, because KiCad `IsSymbol()` accepts keyword-token matches. Do not narrow this branch to only non-keyword identifiers.
- Modern top-level `page` sniff should call the shared `NeedSYMBOLorNUMBER`-style path directly for both consumed tokens, with only local missing-field mapping layered on top. Do not add page-specific parse aliases around that branch.
- A missing raw closing `)` inside top-level `paper` / modern `page` can surface as a shared-token-stream parser error after the wrong `)` is consumed, not necessarily as a local branch-specific `expecting )` diagnostic. Treat that as a parser control-flow consequence of malformed s-expressions, not an unchecked local branch.
- The default schematic screen page settings come from `SCH_SCREEN` construction and should start as `A4`, not `A3`.
- Keep that default `A4` page setup inline at parser/screen construction time too. Do not hide the initial screen page state behind a dedicated `default_page_info()` helper if the goal is structural parity with upstream `SCH_SCREEN` construction.
- `PAGE_INFO::SetType()` is case-insensitive. Mixed-case page kinds like `usletter` or `gerber` should canonicalize to KiCad's enum spelling instead of being rejected or preserved raw.
- That case-insensitive `SetType()` rule also applies to `user`; lower-case `user` must still enter the custom-width/custom-height branch rather than being treated as an invalid page type.
- The optional `portrait` tail in `parsePAGE_INFO()` is still a real keyword token, not a case-insensitive page-type string. `PORTRAIT` should fail where `portrait` succeeds.
- The `parsePAGE_INFO()` tail should follow KiCad's `token = NextTok(); if( token == T_portrait ) ... else if( token != T_RIGHT ) Expecting( "portrait" )` flow. Do not reintroduce a pure lookahead-only helper for that branch.
- `parsePAGE_INFO()` should also own the final right-paren consumption for the `paper` / legacy-`page` section, like upstream. Do not split that close-token responsibility back out into the outer section wrapper.
- Invalid page-type diagnostics in `parsePAGE_INFO()` should point at the consumed bad page-type token itself, not the following token. Keep the error span/message ownership on the token that failed `SetType()`.
- Inside `parsePAGE_INFO()`, the page-type read should go straight through the shared symbol requirement (`NeedSYMBOL`-style) rather than paper-specific alias helpers. Keep this branch structurally close to the upstream routine body.
- Keep the page-type read and `SetType`-style validation inline inside `parsePAGE_INFO()` rather than splitting them into a dedicated `parse_page_info_type()` helper. The upstream routine performs that work in one contiguous block.
- Keep the standard-page-size extraction inline in `parsePAGE_INFO()` too. The upstream routine branches directly from page type into either custom-width/custom-height parsing or the already-initialized standard size; avoid a dedicated `parse_standard_page_dimensions()` helper.
- Keep the custom `User` width/height parsing and clamping inline in `parsePAGE_INFO()` as well. The upstream routine performs the parse-and-clamp sequence directly in the same body rather than via a `parse_user_page_dimensions()` helper.
- Keep the custom `User` width/height numeric conversion on the normal `parse_f64_atom()` path too. Do not keep a redundant `parse_f64_number_atom()` wrapper around those direct width/height reads.
- Keep shared numeric atom reads on the real `NeedNUMBER` path inside the actual `i32`/`f64` readers. Do not hide that token requirement behind a separate `parse_number_atom()` forwarding helper.
- Keep the `portrait` / `)` tail handling inline in `parsePAGE_INFO()` too. The upstream routine consumes the next token and resolves `portrait` vs `T_RIGHT` in the same body rather than via a `parse_page_info_tail_and_right()` helper.
- Keep the final `Paper` construction inline in both `SCH_SCREEN` initialization and `parsePAGE_INFO()` too. Do not hide width/height/orientation assembly behind a separate `build_page_info()` helper if the goal is structural parity with upstream.
- At the current model fidelity, the local `STANDARD_PAGE_INFOS` table matches upstream `PAGE_INFO::standardPageSizes` ordering and entries for the schematic parser’s needs. Remaining exactness in this area is no longer in the page-size table itself, but in broader lexer/parser token behavior outside the local `parsePAGE_INFO()` body.
- For `paper "User"`, orientation follows upstream `PAGE_INFO` statefulness: custom width/height can already make the page portrait before the optional `portrait` token is seen, and `portrait` only swaps when the current orientation is still landscape.
- In `parseTITLE_BLOCK()`, comment handling should stay an explicit `1..9` switch like upstream `parseTITLE_BLOCK()`, not a generic range check. Keep the invalid-comment-number failure on the default branch of that explicit mapping.
- In `parseTITLE_BLOCK()`, `title`, `date`, `rev`, `company`, and comment values are raw `NextTok()/FromUTF8()` reads upstream, not `NeedSYMBOL()` checks. Do not over-tighten those branches to symbol-only tokens.
- In `parseTITLE_BLOCK()`, keep those raw value reads inline in the routine body too. Do not route `title`/`date`/`rev`/`company`/comment payloads through a generic `parse_string_atom()` helper once the goal is structural parity with upstream `NextTok()/FromUTF8()` flow.
- In `parseTITLE_BLOCK()`, keep the branch-head token and fallback `Expecting(...)` text local to that routine too: `title, date, rev, company, or comment`. Do not leave stale copied head strings from unrelated parsers in the title-block branch.
- Keep simple top-level header/body branches like `version`, `generator`, `host`, `generator_version`, `uuid`, and `embedded_fonts` inline in the owning header/schematic-dispatch flow when they are just direct token reads. Do not hide them behind one-branch local wrappers.
- Keep the leading optional `version` handling inline in `parse_schematic()` itself once it has shrunk to the direct default-or-read branch. Do not keep a trivial `parse_header()` wrapper around that entry flow.
- The top-level `text`, `label`, `global_label`, `hierarchical_label`, `directive_label`, and `netclass_flag` branches should converge into one shared parser entrypoint shaped like upstream `parseSchText()`, rather than staying split across separate top-level parse routines.
- In that shared `parseSchText()` path, keep KiCad's `Unexpected(...)` branches for invalid `shape`, `length`, and `property` usage instead of collapsing them into the generic final `Expecting(...)` case.
- In that same shared `parseSchText()` path, keep KiCad's literal default `Expecting(...)` text: `at, shape, iref, uuid or effects`, even though the routine also handles `exclude_from_sim`, `length`, `fields_autoplaced`, and `property`.
- In that same shared `parseSchText()` path, when `shape` is valid for the current label kind, keep the shape token itself on the shared `NeedSYMBOL()` path. Do not accept nested non-symbol tokens there through the generic string parser.
- In that same shared `parseSchText()` shape branch, only real unquoted symbol/keyword tokens should be accepted. Quoted strings like `"input"` are not upstream shape tokens and must be rejected.
- In that same shared `parseSchText()` shape branch, keep the label-shape enum mapping inline in the owning routine rather than behind a separate `parse_label_shape()` helper.
- In that same shared `parseSchText()` path, do not add a repo-local requirement that non-local labels must have an explicit `shape`. Upstream leaves the label shape at its default if no `shape` token appears.
- In that same shared `parseSchText()` path, do not add a repo-local post-loop requirement that `(at ...)` must be present for labels. Upstream leaves the label at its default position and orientation if no `at` token appears.
- In the shared `parseSchText()` path, legacy `iref` only parses payload for global labels. For other text/label kinds, it should fall straight into the shared close handling so malformed payloads fail at the same point KiCad does.
- In that same non-global `iref` branch, do not eagerly consume the closing `)` locally. Upstream leaves close handling to the shared outer loop, so malformed non-global `iref` payloads fail later in shared parser flow rather than through a local `NeedRIGHT()` branch.
- In that same shared `parseSchText()` path, global-label `Intersheet References` accumulation should stay inline in the owning routine for both `iref` and explicit `property` branches, not behind a repo-local `upsert_global_label_property()` helper.
- In that shared `parseSchText()` path, keep the leading text payload on a strict symbol-token path with its own `Invalid text string` branch before any type-specific body parsing runs.
- In that same shared `parseSchText()` path, keep `uuid` on the shared `NeedSYMBOL()` path too, not on the generic string parser.
- In `parseSchTextBoxContent()`, keep the text payload on the same strict `Invalid text string` symbol-token path as upstream before any textbox body parsing runs.
- For top-level schematic `text_box`, do not keep a trivial `parse_text_box()` wrapper around the real body parser. Dispatch straight to `parseSchTextBoxContent()`-equivalent logic from the owning switch branch.
- Do not collapse top-level schematic `text_box` and table-cell parsing behind one boolean-gated body routine either. Keep those entrypoints separate so the table-cell-only `span` grammar and `Expecting(...)` path stay local to the table parser.
- In `parseSchTextBoxContent()`, keep `uuid` on the shared `NeedSYMBOL()` path too, not on the generic string parser.
- In `parseSchTextBoxContent()`, keep KiCad's literal default `Expecting(...)` text: `at, size, stroke, fill, effects or uuid`, even though the routine also handles `exclude_from_sim`, legacy `start/end`, and `margins`.
- In table-cell textbox parsing, keep KiCad's literal default `Expecting(...)` text: `at, size, stroke, fill, effects, span or uuid`, even though the routine also handles `exclude_from_sim`, legacy `start/end`, and `margins`.
- In `parseSchTextBoxContent()`, do not add a repo-local post-loop requirement that `(at ...)` or legacy `(start ...)` must be present. Upstream leaves the text box at its default position if neither token appears, while still requiring `size` unless `end` was provided.
- In `parseSchTable()`, keep `column_widths` and `row_heights` list walks inline in the owning routine once they have shrunk to direct numeric loops. Do not hide those table-specific branches behind a shared `parse_numeric_atom_list()` helper.
- In `parseSchTable()`, keep the branch-head token and fallback `Expecting(...)` text aligned with the actual table child set too: `column_count, column_widths, row_heights, cells, border, separators, or uuid`. Do not leave stale copied names like `columns`, `col_widths`, or `header` in that routine.
- In `parseImage()`, keep `uuid` on the shared `NeedSYMBOL()` path too, not on the generic string parser.
- In `parseImage()`, base64 `data` chunks should also stay on the symbol-token path and fail as `Expecting( "base64 image data" )` for invalid nested tokens, not through the generic string parser.
- In `parseImage()`, do not add a repo-local requirement that `(at ...)` must be present. Upstream leaves the bitmap position at its default if no `at` token appears.
- In `parseImage()`, keep the image-decode failure text exact too: `Failed to read image data.` with upstream capitalization and punctuation.
- Keep the same `NeedSYMBOL()` UUID rule consistent across remaining schematic items too: shapes, symbols, sheets, sheet pins, groups, shared text, text boxes, tables, and images should not accept nested lists through the generic string parser for `uuid`.
- In `parseJunction()`, `parseNoConnect()`, and `parseBusEntry()`, do not add repo-local post-loop requirements that `(at ...)` or `(size ...)` must be present. Upstream leaves those objects at their default geometry if the tokens are absent.
- In schematic shape parsing, do not add repo-local post-loop geometry requirements that upstream does not have. `parseSchArc()` and `parseSchCircle()` rely on default geometry state when control-point tokens are absent instead of throwing local “missing point” errors.
- `parseSchRectangle()` follows that same rule: do not add a repo-local exact-two-points validation there. Upstream leaves the rectangle at its default start/end geometry if those tokens are absent.
- In `parseSchRuleArea()`, do not add a repo-local minimum-point-count validation on the nested polyline. Upstream just parses the polyline and closes it rather than rejecting short point lists there.
- In `parseSchBezier()`, keep the explicit four-slot control-point dispatch inline like upstream instead of routing through the generic point-list parser. Missing control points keep default geometry; extra ones fail as `unexpected control point`.
- In schematic shape parsing, keep `fixupSchFillMode()` semantics too: `(fill (type outline))` must be rewritten to a color fill using the stroke color, not preserved as a distinct outline fill mode.
- In library `parseSymbolArc()` / `parse_lib_arc_draw_item()`, do not add a repo-local fatal error when neither midpoint nor angle data was parsed. Upstream falls back to its safe default geometry there rather than throwing a parse error.
- In `parseLine()`, keep KiCad's literal stale fallback `Expecting(...)` text too: `at, uuid or stroke`, even though the real accepted geometry branch is `pts`.
- In `parseLine()`, do not add a repo-local post-loop requirement that a `pts` block must appear. Upstream leaves wires and buses at their default start/end geometry if no `pts` token was parsed.
- In that same `parseLine()` `pts` branch, keep the explicit two-point control flow inline like upstream instead of routing through the generic point-list parser. Missing or extra points should fail through the same `NeedLEFT` / `NeedRIGHT` sequence as KiCad.
- In `parseLine()`, keep the local branch-head token and fallback `Expecting(...)` text aligned with the real child set too: `pts, uuid or stroke`. Do not leave stale copied names like `at` in the line branch.
- In `parseJunction()`, `parseNoConnect()`, and `parseBusEntry()`, keep the local branch-head token and fallback `Expecting(...)` text aligned with each routine’s own children. Do not leave those small item parsers on copied head strings from unrelated symbol/field parsers.
- In `parseBusEntry()`, keep KiCad's literal fallback `Expecting(...)` text: `at, size, uuid or stroke`, even though the local branch may match `stroke` before `uuid` in source order.
- In top-level schematic `polyline` handling, keep the too-few-points failure text capitalized exactly like upstream: `Schematic polyline has too few points`.
- In that same top-level schematic `polyline` path, keep the two-point-collapse / too-few-points decision inline in the owning dispatch branch rather than behind a trivial `parse_polyline_item()` wrapper.
- In `parseSchPolyLine()`, keep the old-version stroke-style fix too: for file versions `<= 20211123`, a `stroke` with `default` style must be rewritten to `dash`.
- In that same schematic polyline branch, keep the branch-head token and fallback `Expecting(...)` text aligned with the full routine body too. This parser branch also owns the legacy `start/mid/end/center` geometry tokens, so do not leave those out of the local head/fallback text.
- In `parseRectangleShape()`, keep the branch-head token and fallback `Expecting(...)` text aligned with the full routine body too. The rectangle parser owns `radius` as well as `start/end/stroke/fill/uuid`, so do not leave `radius` out of the local head/fallback text.
- In `parseSchField()`, keep the header checks distinct the way KiCad does: `Invalid property name`, `Empty property name`, and `Invalid property value` are separate branches before field classification begins.
- In `parseSchField()`, `private` survives only for true user fields (`FIELD_T::USER`-equivalent). Do not preserve it for `SheetUser` fields just because they are user-defined at the schematic level.
- In `parseSchField()`, `show_name` and `do_not_autoplace` should continue to accept the bare-token form as `true` through `parseMaybeAbsentBool(true)`, not only explicit `yes` / `no` payloads.
- In `parseSchField()`, keep parent-sensitive field-ID classification and canonical-name mapping inline in the routine body, like upstream, instead of routing them through separate local helpers.
- In `parseSchText()` and `parseSchField()`, nested child heads like `at`, `shape`, `iref`, `uuid`, `effects`, `id`, `hide`, `show_name`, and `do_not_autoplace` are real unquoted keyword tokens. Quoted strings must not dispatch those branches.
- In the lib-symbol parsers too, bare keyword branches like `private`, `hide`, `locked`, and `demorgan` should consume through the direct unquoted-keyword path once matched, not through the generic atom reader after a precheck. Keep those branches structurally aligned with upstream keyword-token handling.
- In `parseSchematicSymbol()` and `parseSheet()`, the owning routine’s child heads and nested `instances` / `variant` / `field` heads should stay on real unquoted keyword-token paths. Quoted strings must not dispatch those branches.
- In `parseSheet()`, keep KiCad's literal default `Expecting(...)` text: `at, size, stroke, background, instances, uuid, property, or pin`, even though the local routine also handles `exclude_from_sim`, `in_bom`, `on_board`, `dnp`, `fields_autoplaced`, and `fill`.
- The same unquoted-keyword-token rule applies in `parseSchSheetPin()`, `parseSchSheetInstances()`, and `parseSchSymbolInstances()`: their `at` / `uuid` / `effects` / `path` / `page` / `reference` / `unit` / `value` / `footprint` heads are parser keywords, not quoted strings.
- In `parseSchSheetInstances()` and `parseSchSymbolInstances()`, keep the local branch-head token and fallback `Expecting(...)` text aligned with the actual child set owned by each routine. Do not leave stale copied tokens like `path` in nested `page` / `reference` branches.
- The same rule applies in schematic `text_box`, `table`, and `image` parsing: heads like `size`, `column_count`, `table_cell`, `border`, `separators`, `scale`, and `data` are parser keywords and must not be dispatched from quoted strings.
- The same unquoted-keyword-token rule applies across the schematic shape family too: `pts`, `start`, `mid`, `end`, `center`, `radius`, `stroke`, `fill`, `uuid`, and `polyline` in `rule_area` are parser keywords, not quoted strings.
- Keep the same token boundary in low-level library/style routines: lib-pin child heads (`at`, `name`, `number`, `hide`, `length`, `alternate`, nested `effects`), lib-property child heads, and stroke/fill child heads (`width`, `type`, `color`) are parser keywords and must not be dispatched from quoted strings.
- `parse_lib_symbols()` itself should also stay on the keyword-token path: the top-level `symbol` child head inside `lib_symbols` is a parser keyword and must not dispatch from a quoted string.
- Keep the same token boundary in helper grammar routines too: `parse_pts()` should require real `xy` keyword tokens, and `parse_embedded_file_body()` should require real `name` / `data` keyword tokens with the matching `Expecting( "name or data" )` behavior.
- The same token boundary applies in the core grammar entrypoints too: root `kicad_sch`, top-level `version`, `bus_alias` `members`, and raw `xy` heads in line/bezier parsing are parser keywords and must not dispatch from quoted strings.
- Boolean parsing should stay on the upstream `yes` / `no` token path too. Do not accept repo-local `true` / `false` synonyms in `parseBool()` / `parseMaybeAbsentBool()`-equivalent branches.
- Numeric parsing should stay on the upstream `NeedNUMBER()` path too. Do not accept quoted/string atoms in places that KiCad parses through `parseInt()` / `parseDouble()`.
- RGB color channels should stay on the upstream `parseInt()` path too: red/green/blue are integer tokens, while alpha remains `parseDouble()`.
- In `parseSheet()`, the `<= 20200310` legacy sheet-field-ID recovery should happen at property-parse time, like upstream, not as a later post-loop rewrite over the final property list.
- In `parseSheet()`, mandatory sheet-name and sheet-file resolution should key off field kind/ID (`SheetName` / `SheetFile`), not canonicalized key-string lookups. That keeps the post-loop validation closer to upstream `FindField(..., FIELD_T::...)`.
- In `parseSheet()`, mandatory sheet field overwrite behavior should stay inline in the property branch, like upstream field accumulation, rather than through a repo-local `upsert_sheet_property()` helper.
- In `parseSheet()`, keep the nested `instances` walk inline in the sheet routine body too. The `project -> path -> page/variant` control flow should stay local to the routine instead of being hidden behind a `parse_sheet_local_instances()` helper.
- In both `parseSheet()` and `parseSchematicSymbol()`, keep `variant` parsing inline in the owning routine body too. The `name/dnp/exclude_from_sim/in_bom/on_board/in_pos_files/field` walk should not be hidden behind a shared `parse_variant()` helper if the goal is structural parity with upstream.
- In those same inline `variant` branches, initialize variant attributes from the owning symbol/sheet before applying overrides, like upstream `InitializeAttributes()`. Do not seed them from hardcoded defaults. For sheets specifically, `in_pos_files` starts false because sheets do not carry position-files exclusion.
- In `parseSheet()`, do not add repo-local post-loop requirements that `(at ...)` or `(size ...)` must be present. Upstream leaves the sheet at its default position/size if those tokens are absent.
- In nested `parseSheet()` `variant` blocks, `in_bom` uses the old wrong polarity before `20260306` and only matches the token's positive logic from `20260306` onward. Keep that exact version gate in the parser branch.
- In `parseSheet()`, keep the missing mandatory-property diagnostics capitalized like upstream: `Missing sheet name property` and `Missing sheet file property`.
- In `parseSchSheetPin()`, keep `Invalid sheet pin name` and `Empty sheet pin name` as distinct header branches, like upstream, instead of collapsing them into a generic atom parse failure.
- In `parseSchSheetPin()`, the leading shape token should also be a real unquoted symbol/keyword token. Quoted strings like `"input"` are not upstream shape tokens and must be rejected.
- In `parseSchSheetPin()`, keep `uuid` on the shared `NeedSYMBOL()` path too. Do not accept nested non-symbol tokens there through the generic string parser.
- In `parseSchematicSymbol()`, mandatory symbol field overwrite behavior should stay inline in the `property` branch and key off parsed field kind/ID, not a repo-local `upsert_symbol_property()` helper or key-string-only matching.
- In `parseSchematicSymbol()`, do not add repo-local post-loop requirements that `lib_id` or `(at ...)` must be present. Upstream leaves the placed symbol at its default library ID / position / orientation when those tokens are absent.
- In `parseSchematicSymbol()`, do not run a second local canonicalization pass over field names after `parseSchField()`-equivalent parsing. Mandatory symbol field naming should already be settled by the property parser.
- In `parseSchematicSymbol()`, `lib_name` should keep the distinct `Invalid symbol library name` header failure instead of going through a generic string parser.
- In `parseSchematicSymbol()`, `lib_id` should stay inline on the shared `NeedSYMBOLorNUMBER()` path, like upstream. Do not route it through a generic string helper that accepts nested non-symbol tokens or hides the `Expecting( "symbol|number" )` branch.
- In `parseSchematicSymbol()`, `mirror` should stay on the symbol-token path too. Do not accept nested non-symbol tokens there through the generic string parser before checking `x` or `y`.
- In that same `parseSchematicSymbol()` `mirror` branch, only real unquoted symbol/keyword tokens should be accepted. Quoted strings like `"x"` are not upstream mirror-axis tokens and must be rejected.
- In `parseSchematicSymbol()`, keep the `default_instance` sub-parse inline in the symbol routine body, like upstream, rather than routing it through a local helper.
- In that inline `default_instance` branch, keep `reference` on the shared `NeedSYMBOL()` path, like upstream, instead of accepting it through the generic string parser.
- In `parseSchematicSymbol()`, keep the nested `instances` walk inline in the symbol routine body too. The `project -> path -> reference/unit/value/footprint/variant` control flow should stay local to the routine instead of being hidden behind a `parse_symbol_local_instances()` helper.
- In that same `parseSchematicSymbol()` instance/default-instance flow, keep `value` and `footprint` on the shared `NeedSYMBOL()` path too, while preserving the legacy `~` empty-string handling. Do not accept nested non-symbol tokens there through the generic string parser.
- In that same `parseSchematicSymbol()` instance/default-instance flow, keep the `Value` / `Footprint` field mutation inline in the owning routine too. Do not route those updates through a repo-local `upsert_symbol_field_text()` helper.
- In `parseSchematicSymbol()`, keep the placed-symbol `pin` sub-parse inline in the symbol routine body too. The `number -> alternate/uuid` walk should stay local to the routine instead of being hidden behind `parse_symbol_pin()`.
- In `parseSchematicSymbol()`, keep KiCad's literal default `Expecting(...)` text: `lib_id, lib_name, at, mirror, uuid, exclude_from_sim, on_board, in_bom, dnp, default_instance, property, pin, or instances`, even though the local routine also handles `convert`, `body_style`, `unit`, `in_pos_files`, and `fields_autoplaced`.
- In `parseSchTable()`, keep KiCad's literal fallback `Expecting(...)` text even where it uses historical token names like `columns` and `col_widths`. Do not silently "improve" those strings to local token spellings if the goal is structural/error parity.
- In `parseSchTable()`, keep the no-cells failure text capitalized exactly like upstream: `Invalid table: no cells defined`.
- In that inline placed-symbol `pin` branch, keep the pin `number` on the shared `NeedSYMBOL()` path, like upstream, instead of accepting it through the generic string parser.
- In that same inline placed-symbol `pin` branch, keep `alternate` on the shared `NeedSYMBOL()` path too. Do not accept nested non-symbol tokens there through the generic string parser.
- In that same inline placed-symbol `pin` branch, keep `uuid` on the shared `NeedSYMBOL()` path too before applying the `20210126` version gate.
- In shared `variant field` parsing, keep `name` and `value` on the `NeedSYMBOL()` path, like upstream. Invalid nested tokens should fail as `Invalid variant field name` / `Invalid variant field value`, not be accepted through a generic string parser.
- In shared `variant` parsing, keep the variant `name` on the `NeedSYMBOL()` path too. Invalid nested tokens should fail as `Invalid variant name`, not be accepted through the generic string path.
- In nested symbol/sheet instance parsing, keep `project`, `path`, and instance `reference` on the shared `NeedSYMBOL()` path, like upstream. Do not accept those headers through the generic string parser.
- In `parseSchematicSymbol()`, `default_instance.value` / `footprint` and nested symbol-instance `value` / `footprint` should update the symbol's own `Value` / `Footprint` field text during parse, like upstream `SetValueFieldText()` / `SetFootprintFieldText()`, not live only in side-channel instance structs.
- In both top-level and nested sheet-instance parsing, keep `page` on the shared `NeedSYMBOL()` path before page-number normalization. Do not accept nested lists there through the generic string parser.
- In top-level `parseSchSymbolInstances()`, keep KiCad's literal fallback `Expecting(...)` text: `reference, unit, value or footprint`. Do not let that branch drift to repo-local text like `path, unit, value or footprint`.
- In `parseSchematicSymbol()` and top-level `parseSchSymbolInstances()`, keep `value` / `footprint` token reads and the legacy `~`-to-empty handling inline in the owning routine branches. Do not hide that branch-local control flow behind a shared `parse_symbol_text_atom()` helper.
- In `parseGroup()`, keep group member UUIDs on the shared `NeedSYMBOL()` path too. Do not accept nested non-symbol tokens there through the generic string parser.
- In `parseLibSymbol()`, the top-level lib symbol name should use a distinct `Invalid symbol name` header branch before library-identifier validation. Do not route that header through the generic library-ID helper.
- In `parseLibSymbol()`, keep KiCad's literal fallback `Expecting(...)` text even where it is narrower than the real accepted branch set. Upstream still says `pin_names, pin_numbers, arc, bezier, circle, pin, polyline, rectangle, or text` there.
- In `parseLibSymbol()`, `extends` should use its own `Invalid parent symbol name` `NeedSYMBOL()` branch too, instead of being routed through the generic library-ID helper.
- In nested `parseLibSymbol()` unit parsing, the unit name should use its own `Invalid symbol unit name` `NeedSYMBOL()` branch before prefix/suffix validation. Do not start that branch from the generic string parser.
- In that same nested lib-symbol unit parser, keep KiCad's literal fallback `Expecting(...)` text too: `arc, bezier, circle, pin, polyline, rectangle, or text`, even though `unit_name` and `text_box` are also valid branches there.
- In nested library `unit_name` parsing, only real symbol tokens should be consumed. Non-symbol atoms like numbers should fall through to the closing-paren path and fail there, like upstream.
- In `parseLibSymbol()`, `jumper_pin_groups` should only consume symbol/string pin names and should keep the upstream `Expecting( "list of pin names" )` behavior for invalid members. Do not accept arbitrary atoms there through the generic string parser.
- In library `parseProperty()` / `parse_lib_property()`, keep the same distinct header failures as upstream: `Invalid property name`, `Empty property name`, and `Invalid property value`.
- In that library property branch, keep `show_name` and `do_not_autoplace` handling aligned with upstream `parseMaybeAbsentBool(true)` semantics, not just `id/at/hide/effects`.
- In that same library property branch, `effects` must still update field visibility when it contains `hide`, like upstream `parseEDA_TEXT()`. Do not leave `visible = true` just because the hide came through `effects` instead of the standalone `hide` token.
- In that same library property branch, keep symbol-field classification and canonical mandatory-field naming inline in `parse_lib_property()`, like upstream, instead of routing it through a repo-local helper.
- In `parseLibSymbol()` itself, duplicate property handling should stay inline in the owning `property` branch and key off parsed field kind/ID for mandatory fields, not a repo-local `upsert_lib_symbol_property()` helper or a second canonicalization pass after `parse_lib_property()`.
- In the library draw-item parsers too, `private` should stay inline in each owning draw-item routine rather than being hidden behind a shared `parse_lib_shape_prefix()` helper. Keep that token flow local to the draw-item parser, like upstream.
- In `parseLibSymbol()` and nested library-unit parsing, draw-item dispatch and top-level draw-item accumulation should stay inline in the owning routines too. Do not route them through repo-local `parse_lib_draw_item()` / `push_lib_draw_item()` helpers that hide the actual branch structure and close-token ownership.
- In those same library shape parsers, keep the initial `LibDrawItem` construction inline per routine too. Do not hide the default object state behind a shared `empty_lib_draw_item()` helper if the goal is routine-by-routine structural parity with upstream.
- In library `parseSymbolPin()` / `parse_lib_pin_draw_item()`, keep `name`, `number`, and `alternate` name on strict symbol-token paths with their own distinct invalid-name branches. Do not accept nested lists there through the generic string parser.
- In that same library pin branch, keep the leading electrical type and graphic shape on strict symbol-token paths too, like upstream token dispatch, instead of accepting nested lists through the generic string parser.
- In that same library pin type/shape branch, only real unquoted symbol/keyword tokens should be accepted. Quoted strings like `"input"` or `"line"` are not upstream pin-type/pin-shape tokens and must be rejected.
- In that same library pin branch, keep the electrical-type and graphic-shape enum mapping inline in `parse_lib_pin_draw_item()` itself rather than behind separate `parse_lib_pin_electrical_type()` / `parse_lib_pin_graphic_shape()` helpers.
- In the library pin `alternate` branch, keep alternate type and alternate shape on those same strict symbol-token paths too, instead of accepting nested lists through the generic string parser.
- In shared `parseEDA_TEXT()`-style effects parsing, keyword branches like bare `hide`, inline `bold`/`italic`, list-head `font`/`justify`/`href`, and `justify` members must use real unquoted symbol/keyword tokens too. Quoted strings are not upstream keyword tokens there and must be rejected.
- In `parse_effects_font()`, keep the branch-head token and fallback `Expecting(...)` text aligned with the full routine body too. This helper owns `color` as well as `face/size/thickness/line_spacing/bold/italic`, so do not leave `color` out of the local head/fallback text.
- In that same shared `parseEDA_TEXT()`-style effects path, return and thread the real `TextEffects` struct directly. Do not hide it behind a repo-local `EffectsSummary` wrapper.
- In that same shared `parseEDA_TEXT()`-style effects path, keep nested `font` and `justify` walks inline in the owning routine once they have shrunk to one-caller local subloops. Do not keep separate `parse_effects_font()` / `parse_effects_justify()` wrappers around those branches.
- In parser-wide enum/keyword branches such as lib-symbol `power` scope and stroke/fill `type`, only real unquoted symbol/keyword tokens should be accepted. Quoted strings like `"local"`, `"dash"`, or `"color"` are not upstream enum tokens and must be rejected.
- Keep the stroke parser body in `parse_stroke()` itself. Do not hide it behind a repo-local `parse_stroke_with_seed()` wrapper when the schematic parser only uses the direct no-seed path.
- In lib-symbol `pin_names` / `pin_numbers`, the legacy bare `hide` form must be a real unquoted keyword token. Quoted `"hide"` should not be treated like the pre-20241004 bare keyword branch.
- In those same `pin_names` / `pin_numbers` helpers, nested list-head keywords like `hide` and `offset` must also be real unquoted keyword tokens. Quoted strings like `("hide" yes)` or `("offset" 0.5)` are not upstream helper branches and must be rejected.
- In lib-symbol `body_styles`, the special `demorgan` marker must be a real unquoted keyword token. Quoted `"demorgan"` is just a body-style name, not the upstream keyword branch.
- In `parseLibSymbol()`, top-level child heads like `power`, `body_styles`, `pin_names`, `pin_numbers`, `property`, `extends`, `symbol`, `embedded_fonts`, `embedded_files`, and the draw-item kinds must dispatch from real unquoted keyword tokens. Quoted strings like `("power" local)` are not upstream branch heads and should fall into the existing warning-and-skip recovery with KiCad's stale `Expecting(...)` text.
- In top-level and lib-symbol `embedded_files` parsing, the only valid child head is `file`, and that head should dispatch from the real unquoted keyword-token path with the matching `Expecting( "file" )` fallback.
- In those same top-level and lib-symbol `embedded_files` branches, keep the `file` loop inline in the owning routine too. Do not hide the branch-local `file` dispatch and warning-recovery boundary behind a shared `parse_embedded_files_block()` helper.
- In those same top-level and lib-symbol `embedded_files` branches, keep each `file` body parse inline in the owning routine too. Do not hide the local `name/data` walk behind a shared `parse_embedded_file_body()` helper.
- Keep the top-level `embedded_files` branch inline in `parse_schematic_body()` as well once it has shrunk to the local version-gate plus `file` loop and warning recovery. Do not keep a trivial `parse_embedded_files()` wrapper around that switch branch.
- Parser-wide bare keyword probes like `private`, group `locked`, and pre-20241004 bare lib-pin `hide` must also require real unquoted keyword tokens. Quoted strings must not trigger those branches.
- In `parseGroup()`, keep the branch-head token and fallback `Expecting(...)` text local to the routine: `uuid, lib_id, members`. Do not leave stale copied head strings from unrelated style parsers in the group branch.
- In lib-symbol `jumper_pin_groups`, member names must stay on the quoted-string path like upstream `DSN_STRING` handling. Unquoted pin names are not valid there.
- In lib-symbol draw-item parsers (`arc`, `bezier`, `circle`, `polyline`, `rectangle`, `text`, `text_box`), nested list-head keywords like `start`, `mid`, `end`, `radius`, `pts`, `at`, `size`, `stroke`, `fill`, `margins`, and `effects` must be real unquoted keyword tokens. Quoted strings must not dispatch those branches.
- In library `parseSymbolRectangle()` / `parse_lib_rectangle_draw_item()`, keep the branch-head token and fallback `Expecting(...)` text aligned with the full routine body too. The rectangle parser owns `radius` as well as `start/end/stroke/fill`, so do not leave `radius` out of the local head/fallback text.
- In that same library draw-item family, point-list `xy` entries must stay on the real unquoted keyword-token path too. Quoted `"xy"` inside lib-symbol `bezier`/`polyline` point lists is not an upstream branch head and must be rejected.
- In schematic/library `pts` branches, keep the nested `xy` loop inline in the owning routine once that branch has shrunk to direct point-list handling. Do not hide it behind a shared `parse_pts()` helper.
- In library `parseSymbolText()` / `parse_lib_text_draw_item()`, keep the text payload on its own `Invalid text string` symbol-token path before entering the body parser.
- In library `parseSymbolTextBox()` / `parse_lib_text_box_content()`, keep the text payload on that same `Invalid text string` symbol-token path before entering the body parser.
- In library `text_box` parsing, keep the body walk and `LibDrawItem` construction in `parseSymbolTextBox()` / `parse_lib_text_box_draw_item()` itself. Do not hide that routine behind a separate `parse_lib_text_box_content()` helper.
- In library `parseSymbolTextBox()` / `parse_lib_text_box_draw_item()`, keep the branch-head token and fallback `Expecting(...)` text aligned with the full routine body too: `start, end, at, size, stroke, fill, effects, or margins`. Do not leave `start/end/margins` out of the local fallback text or stale `margins/effects` ordering once the parser body handles `effects` first.
- For the `paper` / `page` area, the remaining exactness after the local helper chain is ported is parser-wide token-category adoption. If a future discrepancy in this area requires broader `NeedSYMBOL` / `NeedNUMBER` parity outside the dedicated page helpers, treat that as a wider lexer/parser task rather than another local `parsePAGE_INFO()` branch.

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
