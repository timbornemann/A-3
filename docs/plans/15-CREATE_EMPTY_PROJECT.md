# Plan 15: Leeres Projekt von vorn anlegen

Status: Abgeschlossen · 2026-09-16

## Auftrag und Autorität

Nutzer sollen in A^3 ein leeres lokales Git-Projekt anlegen und danach wie in
jedem anderen Worktree mit Ask, Plan und Agent arbeiten. Maßgeblich sind
[ADR-0110](../adrs/0110-leeres-projekt-anlegen.md),
[ADR-0029](../adrs/0029-core-owned-project-catalog-and-restoration.md),
[Architekturregeln](../ARCHITECTURE_RULES.md) und [Qualitätsgates](../QUALITY_GATES.md).

## Akzeptanzkriterien

- [x] `create_project` ist pfadlos; nur der native Ordnerdialog autorisiert den Root.
- [x] Init nur in einem existierenden, leeren Nicht-Git-Ordner; HEAD unborn `main`.
- [x] Kein Commit, Remote, README oder Scaffold; isoliertes gix ohne User-Git-Config.
- [x] Danach Inspektion, Katalog und Aktivierung wie `OpenProject`.
- [x] `open_project` bleibt unverändert; Nicht-Git bleibt `notGitRepository`.
- [x] Fail-closed für nicht leer, bereits Git, Nested-Worktree und Dialogabbruch.

## Vertikale Umsetzung

1. ADR-0110 annehmen und den ADR-Index ergänzen.
2. `CreateProject`, `EmptyWorktreeInitializer` und gix-`init_opts` anbinden.
3. IPC-Command, ErrorCodes, Capability und UI-Button **Neues Projekt**.
4. Produkt-, IPC-, Security-, Architecture- und Quality-Gate-Verträge führen.
5. Application-, Adapter-, IPC- und Frontend-Contracts prüfen; Quality Gate.

## Nicht-Ziele

- Scaffold, Initial-Commit oder Remote-Clone.
- Git-Init in einem vollen Codeordner.
- Pfad oder Name aus der WebView.
- `open_project` um stilles Init erweitern.
- Agent automatisch mit einer Idee starten.

## Verifikation

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
- `cargo test --workspace --all-features --locked`
- Desktop-Unit-Tests für `create_project` ohne Pfad
- `pnpm --filter @a3/desktop test`
- `pnpm lint`, `pnpm typecheck`, `pnpm check:links`
