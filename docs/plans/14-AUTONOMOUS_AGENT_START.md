# Plan 14: Autonomer Agent-Einstieg ohne Planfreigabe

Status: Abgeschlossen · 2026-09-16

## Auftrag und Autorität

Wenn der Nutzer den Zielmodus Agent wählt, soll A^3 weiterhin recherchieren und einen
Core-validierten Arbeitsplan erzeugen, diesen aber ohne Nutzer-Planfreigabe materialisieren
und ausführen. Plan bleibt der einzige Review- und Übergabepfad. Maßgeblich sind
[ADR-0109](../adrs/0109-autonomer-agent-einstieg.md),
[ADR-0042](../adrs/0042-adaptiver-agent-arbeitsplan.md),
[Architekturregeln](../ARCHITECTURE_RULES.md) und [Qualitätsgates](../QUALITY_GATES.md).

## Akzeptanzkriterien

- [x] `targetMode=Agent` bleibt Agent; Ask, Plan und neue Sessions werden nicht zu Plan
      umgeschrieben.
- [x] Ein belegter, strukturierter Agent-Plan materialisiert Goal, Ledger und Run ohne
      `AwaitingPlanReview`.
- [x] `ImplementPlan` bleibt auf Plan + `AwaitingPlanReview` + exakte Revision beschränkt.
- [x] Eine Agent-Nachricht setzt keinen wartenden Plan um.
- [x] Composer und IPC zeigen keinen Agent-Halt „Nach Planfreigabe“.
- [x] Policy, Approval, Verification, `QUESTION:` und Budgetfortsetzung bleiben Haltepunkte.

## Vertikale Umsetzung

1. ADR-0109 annehmen und den ADR-Index ergänzen.
2. Modusauflösung, Submit-Outcome und Mode-Optionen auf Agent-Eigenständigkeit umstellen.
3. Composer-Chip und Frontendtests anpassen; Plan-umsetzen-Button unverändert lassen.
4. Produkt-, Architektur-, Memory-, Security- und IPC-Verträge in derselben Änderung führen.
5. Remap-, Queue- und ImplementPlan-Verträge prüfen und das Qualitätsgate ausführen.

## Nicht-Ziele

- Recherche, `AgentWorkPlan`, Policy oder Verification überspringen.
- Einen wartenden Plan durch eine Agent-Nachricht umsetzen.
- Offener Chat-Loop, parallele Mutation oder automatische Budgeterneuerung.
- Historische ADRs umschreiben.

## Verifikation

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
- `cargo test --workspace --all-features --locked`
- Desktop-Unit-Tests `selected_agent_mode_stays_agent_and_never_requests_plan_review`,
  `plan_mode_still_persists_a_cited_plan_for_explicit_handoff`,
  `implement_plan_rejects_an_autonomous_agent_start`
- `pnpm --filter @a3/desktop test` (416 bestanden, 14 übersprungen)
- `pnpm lint`, `pnpm typecheck`, `pnpm check:links`
