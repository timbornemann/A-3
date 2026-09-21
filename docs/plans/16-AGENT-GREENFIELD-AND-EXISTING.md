# Plan 16: Agent in leeren und bestehenden Projekten

Status: Abgeschlossen · 2026-09-21

## Auftrag und Autorität

A^3 soll im Modus Agent nach einem Nutzerauftrag selbständig recherchieren, einen
Core-validierten internen Arbeitsplan materialisieren und ihn über den sicheren Harness
ausführen. Das gilt für bestehende Codebasen und für einen veröffentlichten leeren
Git-Worktree. Maßgeblich sind [ADR-0109](../adrs/0109-autonomer-agent-einstieg.md),
[ADR-0111](../adrs/0111-leerer-index-bestandsaufnahme.md),
[ADR-0042](../adrs/0042-adaptiver-agent-arbeitsplan.md), die
[Architekturregeln](../ARCHITECTURE_RULES.md) und die
[Qualitätsgates](../QUALITY_GATES.md).

## Akzeptanzkriterien

- [x] Ein leerer veröffentlichter Index schließt ausschließlich die Repository-
      Bestandsaufnahme als Core-Grenze; das Modell erfindet keine vorhandenen Dateien,
      APIs oder Einstiegspunkte.
- [x] Plan und Agent gelangen danach ohne Source-Zitate, aber mit aktuellem typisiertem
      Inventory-Nachweis, durch Design und Planübergang; ein nicht leerer Graph behält
      unverändert seine Quellenpflicht.
- [x] Agent materialisiert den vollständigen Core-Plan ohne Nutzer-Planfreigabe als Goal,
      Task Ledger und Run; Plan bleibt im Review-Halt.
- [x] Geordnete Änderungs- und Testziele werden als kleine, unabhängige, höchstens 64
      `AgentWorkPlan`-Schritte erhalten; freier Text kann keine Planüberschriften einschleusen.
- [x] Bestehende Repository-Recherche, Evidence-Revalidierung, Policy, Approval,
      Mutation-Serialisierung, Reindex und Verification bleiben unverändert wirksam.
- [x] Recherche- und Materialisierungsregressionen verwenden einen echten leeren
      Git-Worktree, Fast Index, sichere Recherche und libSQL; bestehende Codebasis- und
      read-only Live-Recherche-Gates bleiben grün.
- [x] Der produktive Executor legt in einem echten leeren Worktree mindestens eine geplante
      Datei über Approval und Patch an, reindiziert sie und arbeitet nur mit aktueller Evidence
      weiter.
- [x] Änderungsschritte und erst nachgelagert erkennbare Testbefehle werden ohne unprüfbaren
      `UserConfirm`-Loop sicher verifiziert; fehlende Prüfbarkeit hält modellfrei und verständlich.

## Vertikale Umsetzung

1. Leeren Graph im Core als aktuelle, begrenzte Repository-Bestandsaufnahme schließen.
2. Den Source-Zitat-Halt ausschließlich durch den vollständigen scopegleichen
   Empty-Inventory-Nachweis ersetzen.
3. Geordnete Designentscheidungen sicher und verlustfrei in atomare Arbeitsplan-Schritte
   kompilieren; Designprompts auf kleine überprüfbare Outcomes ausrichten.
4. Den echten Plan-/Agent-Recherchepfad auf einem leeren Worktree regressionsprüfen und
   Nichtleer-/Scope-Negativfälle erhalten.
5. Relevante Teiltests, vollständige Rust-/Frontend-Gates und einen ausdrücklich
   freigegebenen Luna-Nachtest ausführen; Grenzen dokumentieren.
6. Nach Annahme von [ADR-0112](../adrs/0112-nachgelagerte-greenfield-verifikation.md)
   Greenfield-Mutation, nachgelagerte Verification und den echten Executor bis zum sicheren
   Abschluss beziehungsweise expliziten Approval-Halt schließen.
7. Gemäß [ADR-0113](../adrs/0113-sichere-unterverzeichnisse-fuer-patches.md) fehlende
   Unterverzeichnisse innerhalb einer strukturierten Add-/Move-Aktion sicher und ohne
   eigenständige ungebundene Dateisystemmutation erzeugen.

## Nicht-Ziele

- Recherche, Goal Contract, Task Ledger, Policy, Approval oder Verification überspringen.
- Einen leeren Task-Lens-Treffer in einem nicht leeren Repository als leeres Projekt behandeln.
- Offenen Chat-Loop, parallele Mutation, autonome Paketinstallation, Netzwerk-, Push- oder
  Veröffentlichungsfreigabe einführen.
- Ein Repository beim Anlegen mit Scaffold-Dateien füllen.

## Verifikation

- `cargo fmt --all -- --check`
- `cargo test -p a3-domain agent_work_plan --offline --locked`
- `cargo test -p a3-desktop empty_project_plan_and_agent_reach_a_grounded_atomic_work_plan_without_sources --offline --locked -- --test-threads=1`
- `cargo test -p a3-desktop --lib research --offline --locked -- --test-threads=1`
- `cargo test --workspace --all-features --offline --locked -- --test-threads=1`
- `cargo clippy --workspace --all-targets --all-features --offline --locked -- -D warnings`
- `pnpm ci:frontend`, `pnpm check:links`, `git diff --check`
- explizit freigegebener, read-only konfigurierter `gpt-5.6-luna`-Recherchetest

Abschlussnachweis am 2026-09-21: Der vollständige Rust-Workspace-Test mit allen Features und
einzelner Testausführung sowie Clippy mit verweigerten Warnungen sind grün. Der produktive
Greenfield-E2E erzeugt über den echten Patch-Adapter `server.py`, `tests/__init__.py` und
`tests/test_server.py`, reindiziert, bindet den danach belegten
`python -B -m unittest discover`-Command, hält an der exakten One-shot-Freigabe und erreicht erst
mit strukturiert ausgewerteter Test-Evidence `Done`. Eine nicht klonbare Core-Capability verhindert,
dass dieser Approval-Pfad für einen fremden Ledger-Schritt vorbereitet wird. Fehlende Commands
führen stattdessen ohne weiteren Modellturn zu einem verständlichen dauerhaften Blocker.

Linkprüfung, Frontend-Lint, Typprüfung, Tests und Produktionsbuild sind grün. `pnpm ci:frontend`
stoppt weiterhin ausschließlich an drei bereits vor diesem Plan unformatierten und von ihm
unveränderten Dateien (`App.svelte`, `command-error.ts`, `project.test.ts`); die von diesem Plan
geänderten Frontend-Dateien bestehen die Formatprüfung. Drei aufeinanderfolgende ausdrücklich
freigegebene Tests mit dem konfigurierten `gpt-5.6-luna` schließen die Greenfield-Recherche ohne
Einstellungsänderung mit einem geerdeten atomaren Arbeitsplan ab.
