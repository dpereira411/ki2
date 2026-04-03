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
- `group` declarations are parsed first and resolved after the rest of the file. Do not eagerly fold them into generic item parsing.
- `polyline` is not equivalent to `wire`/`bus`. Two-point polylines collapse to line-like objects; longer ones remain shapes.
- `rule_area` grammar is specialized and wraps a nested `polyline`; it is not just another generic point-list shape.
- `text`, `label`, `global_label`, `hierarchical_label`, `directive_label`, and `netclass_flag` should be treated as one shared parser family with type-specific branches, like upstream `parseSchText()`.
- `property` parsing is parent-sensitive. Symbol, sheet, and global-label mandatory fields are not just arbitrary user properties.
- `private` only survives for user fields; it should not be blindly preserved on mandatory fields.
- Legacy compatibility branches matter: `~` empty-string handling, root-path normalization, old overbar notation, pin UUID version gates, legacy `iref`, and similar cases should be ported explicitly rather than approximated.
- Tests should be updated toward upstream syntax, not the other way around.
- `paper` / `page` parsing should stay split the way KiCad uses it: `parsePAGE_INFO()` for `paper` and legacy `page <= 20200506`, and a separate modern top-level `page` sniff path using `SYMBOL or NUMBER` tokens.
- Legacy top-level `page <= 20200506` should be normalized to the `paper` branch before the main schematic-section dispatch, like upstream `token = T_paper`, rather than handled as a nested special case inside the modern `page` branch.
- Keep that legacy `page -> paper` remap inline at the dispatch token/switch boundary, not in a separate normalization helper. The upstream shape is a token rewrite immediately before the branch dispatch.
- Once normalized, `paper` and modern top-level `page` should be direct dispatch branches, not extra wrapper helpers. Keep the branch bodies aligned with the upstream switch cases rather than inserting local section-mediation layers.
- Modern top-level `page` sniff should store the two consumed `SYMBOL or NUMBER` tokens exactly as read. Do not reuse page-number normalization from `sheet_instances`/`symbol_instances` in this branch; upstream only sniffs and moves on.
- That modern `page` sniff acceptance set includes keyword tokens too, because KiCad `IsSymbol()` accepts keyword-token matches. Do not narrow this branch to only non-keyword identifiers.
- Modern top-level `page` sniff should call the shared `NeedSYMBOLorNUMBER`-style path directly for both consumed tokens, with only local missing-field mapping layered on top. Do not add page-specific parse aliases around that branch.
- The default schematic screen page settings come from `SCH_SCREEN` construction and should start as `A4`, not `A3`.
- `PAGE_INFO::SetType()` is case-insensitive. Mixed-case page kinds like `usletter` or `gerber` should canonicalize to KiCad's enum spelling instead of being rejected or preserved raw.
- That case-insensitive `SetType()` rule also applies to `user`; lower-case `user` must still enter the custom-width/custom-height branch rather than being treated as an invalid page type.
- The optional `portrait` tail in `parsePAGE_INFO()` is still a real keyword token, not a case-insensitive page-type string. `PORTRAIT` should fail where `portrait` succeeds.
- The `parsePAGE_INFO()` tail should follow KiCad's `token = NextTok(); if( token == T_portrait ) ... else if( token != T_RIGHT ) Expecting( "portrait" )` flow. Do not reintroduce a pure lookahead-only helper for that branch.
- `parsePAGE_INFO()` should also own the final right-paren consumption for the `paper` / legacy-`page` section, like upstream. Do not split that close-token responsibility back out into the outer section wrapper.
- Invalid page-type diagnostics in `parsePAGE_INFO()` should point at the consumed bad page-type token itself, not the following token. Keep the error span/message ownership on the token that failed `SetType()`.
- Inside `parsePAGE_INFO()`, the page-type read should go straight through the shared symbol requirement (`NeedSYMBOL`-style) rather than paper-specific alias helpers. Keep this branch structurally close to the upstream routine body.
- For `paper "User"`, orientation follows upstream `PAGE_INFO` statefulness: custom width/height can already make the page portrait before the optional `portrait` token is seen, and `portrait` only swaps when the current orientation is still landscape.
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
