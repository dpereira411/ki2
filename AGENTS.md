# AGENTS

## Purpose

This repository is not aiming for a "KiCad-inspired" parser or loader. The target is a structural
Rust port of KiCad's schematic pipeline with behavior tracked against upstream KiCad.

For every feature in scope, the target is 1:1 KiCad parity in control flow and behavior, not just
similar output. Parser, loader, connectivity, ERC, and export work should all be judged against
the upstream owning code path, object construction timing, state mutation timing, accepted input,
and failure behavior.

## Working Rules

1. Prefer literal upstream structure over cleaner local abstractions.
2. Port routine-by-routine in upstream order inside the active subsystem.
3. Every nontrivial behavior should map to a specific upstream routine or branch.
4. Do not silently accept unknown tokens or states just to keep parsing or loading moving when
   upstream would reject them.
5. Do not introduce "neutral AST first, semantic pass later" architecture where KiCad validates
   while constructing domain objects.
6. When a current local representation is too reduced for upstream semantics, expand the model
   instead of normalizing away the difference.
7. Treat current local code as transitional unless it clearly mirrors an upstream routine.
8. Compatibility is judged by code flow, accepted grammar, error cases, version gates, state
   ownership, and object construction timing, not only by whether output looks plausible.
9. Every time you materially touch a function, update or add a short comment on that function
   covering:
   - which upstream routine or branch it corresponds to
   - whether it is at parity or what still diverges from upstream
   - if it is not a 1:1 upstream routine, why the local helper exists and why it is still needed
10. Do not treat any feature as complete because it produces plausible output. A feature is only
    "done" when its local owning code flow is intentionally aligned with the upstream KiCad owning
    code path, or when the remaining gap is explicitly documented as blocked in
    `PARITY_BACKLOG.md`.
11. "Ownership" means the same subsystem that owns a fact in upstream KiCad must own it locally
    too. Do not let ERC, export, or loader re-derive connectivity, net naming, occurrence state,
    or other graph-owned facts when upstream consumes them from a different owning layer.

## Strict Mode

Strict mode is the default for parity work in this repository.

1. Do not stop while meaningful parser/loader/connectivity/ERC/export parity work remains in
   `PARITY_BACKLOG.md`.
2. Stay in execution mode, not reporting mode. Do not treat summaries, green tests, or partial
   alignment as completion.
3. Prefer whole functions or tightly related routine clusters over micro-patches.
4. Do not spend a work unit on helper renames, isolated expect-string tweaks, or tiny local
   cleanups unless they are required to complete a larger upstream routine port already in
   progress.
5. Each work unit should remove a meaningful structural/code-flow mismatch with upstream.
6. If a routine is being ported, continue until the owning control flow is substantially closer to
   upstream, not just one branch cleaner.
7. If the Rust model blocks parity, expand the model instead of preserving a repo-local shortcut.
8. Remove duplicated local side state whenever upstream does not keep it.
9. Do not treat passing tests as completion; tests only validate the larger port.
10. Commits must correspond to substantial parity work, not cosmetic cleanup.
11. When a work unit is committed and meaningful backlog still remains, continue directly into the
    next work unit instead of sending a status reply. Only surface a reply if the backlog is
    exhausted or a real blocker is hit.
12. A successful commit is not, by itself, a valid reason to reply. After `cargo fmt --all`,
    `cargo test -q`, and commit succeed, immediately start the next backlog item unless the backlog
    is exhausted or a real blocker prevents further local progress.
13. If the user has explicitly asked for continuous execution, any reply without exhausted backlog
    or a real blocker is a behavior failure. In that mode, prefer doing more work over sending a
    summary.
14. Do not treat the end of a turn, a clean git status, or a green test run as an implicit
    stopping point. Those are normal checkpoints inside execution mode, not reasons to report.
15. If backlog remains, the default action after every successful work unit is: pick the next
    largest mismatch, edit, test, commit, continue. Do not wait for another user prompt to resume.
16. If a reply is unavoidable, it must explain the blocker or state that the backlog is exhausted.
    Do not send celebratory, summary-only, or "latest progress" replies while executable parity
    work still remains.
17. When a real blocker is identified, do not stop at naming it. Find the concrete path to
    unblocking it and record that path in `PARITY_BACKLOG.md` before treating the work as blocked.
18. When the product goal is strict ERC, net naming, or netlist/export parity, connection-graph
    parity is a primary workstream, not a side task. Do not keep extending reduced geometry-only
    checks once the backlog shows the remaining gaps depend on KiCad's fuller connection ownership
    model.
19. Once parser parity and the broad loader/hierarchy baseline are sufficient to support the next
    owning subsystem, move priority to that owning subsystem instead of trying to finish every
    lower layer to cosmetic 100% first.
20. When strict ERC/net naming/export parity is the active goal, ERC and export patches should
    primarily remove connection-graph or settings ownership mismatches. Do not let feature-count
    growth replace the graph-owned work that still blocks strict parity.
21. Once the reduced graph has absorbed the honest static/shared ownership work, move priority to
    the fuller live `SCH_CONNECTION` / `CONNECTION_SUBGRAPH` analogue instead of continuing to
    expand snapshot-only propagation helpers. Treat the reduced graph as transitional scaffolding,
    not the destination architecture.
22. At the end of each work unit, question whether the current plan is still the best one. Play
    devil's advocate against the active approach. If that review shows a cleaner upstream-shaped
    path or shows the current direction is becoming transitional churn, change course instead of
    continuing out of momentum.

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
   - direct tests cover the routine's explicit upstream branches

## Source Of Truth

1. Upstream KiCad code is the authority on parser, loader, connectivity, ERC, and export behavior
   and structure.
2. `AGENTS.md` should stay short and operational. Do not keep routine-by-routine parity trivia
   here.
3. Use `PARITY_BACKLOG.md` as the authoritative backlog/status/blocked-surface document.
4. `LOCAL_PARSER_BFS_RECORD.md` is only a reduced parser coverage artifact.
5. When a detailed local rule is no longer broadly useful as an operating instruction, move it out
   of `AGENTS.md` into `PARITY_BACKLOG.md` instead of growing this file further.

## Global Parity Rules

1. Tests should move toward upstream syntax and behavior, not the other way around.
2. Shared token readers should mirror KiCad semantics:
   - `NeedSYMBOL()` accepts bare symbols, keyword tokens, and quoted strings
   - `NeedSYMBOLorNUMBER()` also accepts quoted strings
3. Parent-sensitive property/field parsing matters:
   - symbol, sheet, and global-label mandatory fields are not generic user properties
   - `private` survives only for user fields unless upstream clearly keeps it
4. Legacy/version branches are first-class behavior. Port them explicitly instead of
   approximating them away.
5. Keep section-head ownership, close-token ownership, state-mutation timing, and object ownership
   aligned with the upstream owning routine whenever practical.
6. Keep routine-specific parity rules, exact `Expecting(...)` strings, narrow version gates, and
   one-off branch notes in `PARITY_BACKLOG.md`, not in `AGENTS.md`.
7. If a feature still depends on a reduced local carrier, comment that divergence at the touched
   function and keep the unblock path recorded in `PARITY_BACKLOG.md`.

## Expected Workflow

1. Identify the exact upstream routine(s) being ported.
2. Read the relevant upstream code first.
3. Patch local model/parser/loader/connectivity/export code to mirror that routine as directly as
   practical.
4. Add or update regression tests using upstream-shaped input and behavior.
5. Run `cargo test`.

## What To Avoid

- "Rustier" redesigns that obscure upstream control flow.
- Generic catch-all parsers or helpers when upstream has specialized routines.
- Silent skips for unsupported nested constructs.
- Expanding surface area without tying it back to a real upstream branch.
- Leaving detailed parity trivia in `AGENTS.md` once it belongs in `PARITY_BACKLOG.md`.
- Reply patterns like "it's X, not Y" / "the issue is A, not B" as filler framing. State the fact
  directly unless the contrast is technically necessary.
