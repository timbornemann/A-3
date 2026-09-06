# libsql 0.9.29: single connection close

This directory contains the existing locked crates.io source archive, not a new
dependency or feature selection. Upstream: <https://github.com/tursodatabase/libsql>.
The original Cargo manifest and README declare MIT licensing and retain the
upstream authorship/license references. Upstream files are retained as packaged.

Archive SHA-256:
`2329faffc510cc3c6b4f00169a39177cc7099d3ed7647fc92f7cf26e53a8d976`.

The ONLY functional delta from that archive is deletion of the six-line redundant
`impl Drop for LibsqlConnection` block in `src/local/impls.rs`. The contained local
Connection already owns `disconnect()` through its own Drop. Calling it twice on
the same sole-owner native pointer is unsafe; no new unsafe code is introduced.

Four existing `columns` return signatures additionally spell their elided lifetime
as `Column<'_>` (local/statement.rs, local/impls.rs, statement.rs). This has no runtime
effect and removes Rust 1.93's `mismatched_lifetime_syntaxes` warnings now exposed for
the local path dependency. No warning is suppressed.

Do not format or refactor this third-party tree. It is excluded from workspace
membership. Cargo features remain `default-features=false, features=["core"]`.
There are no new transitive versions. No registry cache source was changed.

See [ADR-0055](../docs/adrs/0055-libsql-einmalige-verbindungsfreigabe.md) and the
retry-free [native lifetime regression](../crates/a3-storage-libsql/tests/connection_lifecycle.rs).
Replace this patch only with a reviewed upstream fix and retain that regression.
