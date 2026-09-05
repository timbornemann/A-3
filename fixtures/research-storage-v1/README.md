# Storage research regression fixture

Original, offline source excerpts for testing context compilation, not an executable app.
The absent storage classes are intentional: the research question concerns selection and
configuration, not backend implementation. Tests add overlapping and unrelated candidates to
ensure these small relevant files remain complete in a fixed context window. No database,
environment changes, provider calls, or external TaskFlow project are required by the test.

## Reproduction and measured result (2026-09-05)

The desktop regression includes these three files with five unrelated candidates under a
4,096-byte context budget. Before the fix, only 1/3 files arrived complete and the whole packet
used 4,373 bytes. After the fix, 3/3 arrive complete in 4,041 bytes. This measures deterministic
context coverage, not model accuracy or response latency. Separate tests cover overlapping
sources, later pages, revision isolation, Unicode, exact Lens seeds and bounded citation repair.

The real libSQL completion/delete regression supplies a public note whose historical references
converge onto the same source. It failed before canonicalization and passes afterwards, without
changing the database uniqueness constraint. Missing members of the original evidence chain
still reject reuse. No database migration or edits to the user's TaskFlow project are needed.

Run from the workspace root:

```powershell
cargo test -p a3-desktop --lib research_regression_tests -- --nocapture
cargo test -p a3-application --lib revalidated_note_sources_are_unique_without_weakening_bounds
cargo test -p a3-storage-libsql answer_event_citations_and_session_revision_commit_atomically_and_delete_together --lib -- --nocapture
cargo fmt --all -- --check
cargo test --workspace --all-features --target-dir target/research-verification --jobs 4
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo doc --workspace --all-features --no-deps
pnpm format:check
pnpm lint
pnpm typecheck
pnpm --filter @a3/desktop exec vitest run --maxWorkers=2
pnpm test:tools
pnpm build
pnpm check:links
git diff --check
```

The separate Cargo target directory avoids overwriting a running Windows desktop executable;
the ordinary target-directory attempt was blocked by the open app. Live provider smoke tests
remain opt-in and were not executed. The frontend environment reports Node 25.6.1 instead of
the pinned 24.14.0, and the production build retains existing BigInt target warnings.

The conversation regression additionally checks stable polling ownership across fresh
projections. The old effect cancelled its timer three times in this fixture; the fixed effect
does not cancel it until unmount. The diagram mount test uses a local renderer stub, keeping
Mermaid layout out of polling timing; renderer and sanitizer contracts remain separate.
