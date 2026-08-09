# A^3 Masterplan

Status: Planned  
Ziel: vollständiger Weg von leerem Repository zu A^3 V1

## Produktdefinition V1

Ein Nutzer kann:

1. A^3 auf Windows, Linux oder macOS starten.
2. einen lokalen Git-Worktree öffnen;
3. einen schnellen deterministischen Index erstellen und inkrementell aktuell halten;
4. Projektstruktur, Symbole, Abhängigkeiten und Tests durchsuchen;
5. eine budgetierte Deep Map erzeugen;
6. einem lokalen Modell eine Coding-Aufgabe geben;
7. Goal Contract und Plan prüfen;
8. den Agenten sicher suchen, lesen, patchen und testen lassen;
9. Diff, Evidenz, Fortschritt, Freigaben und Verifikation nachvollziehen;
10. einen Run abbrechen und später ohne Zielverlust fortsetzen.

## Meilensteine

| ID | Meilenstein | Ergebnis | Abhängigkeit |
| --- | --- | --- | --- |
| M0 | Architekturbaseline | Dokumente, ADRs, Regeln und CI-Konzept | keine |
| M1 | Walking Skeleton | Tauri-App ruft typisierten Rust-Use-Case auf | M0 |
| M2 | Lokales Projekt | Worktree öffnen, Identität und DB persistieren | M1 |
| M3 | Fast Index | Rust, TS/JS und Python inkrementell indexieren | M2 |
| M4 | Retrieval | Exact, FTS, Graph und Testsuche | M3 |
| M5 | Project Map | Deep Map mit Evidenz und Task Lens | M4 |
| M6 | Durable Harness | Goal, Ledger, Context Compiler, read-only Run | M5 |
| M7 | Editing Agent | sichere Patches, Prozesse und Verifikation | M6 |
| M8 | Desktop Product | vollständiger, zugänglicher Kernworkflow | M7 |
| M9 | Evaluation | Qualitäts- und Performancebaseline | M8 |
| M10 | Cross-Platform Beta | Installer und Smoke-Tests für drei OS | M9 |
| M11 | V1 | signierter, dokumentierter Offline-Release | M10 |

## Master-Checkliste

### M0 Architekturbaseline

- [x] Dokumentenpaket in das Repository übernehmen
- [x] ADR-Prozess und Eigentümer festlegen
- [x] AGENTS.md am Repository-Root aktivieren
- [x] Lizenz und öffentliches Repositorymodell entscheiden

### M1 Walking Skeleton

- [x] Cargo-Workspace und Desktop-App scaffolden
- [x] Svelte-UI statisch in Tauri laden
- [x] ersten versionierten IPC-Query implementieren
- [x] Plattform-CI für Compile und Unit Tests aktivieren

### M2 Lokales Projekt

- [ ] sichere Ordnerauswahl und Rootvalidierung
- [ ] RepositoryIdentity und WorktreeIdentity
- [ ] catalog.db und knowledge.db
- [ ] Migration Runner und Storage Contract
- [ ] Projekt öffnen, schließen und zuletzt verwendet

### M3 Fast Index

- [x] Discovery und Ignore Engine
- [x] Content Hashing und Snapshots
- [x] Tree-sitter LanguageAdapter-Contract
- [x] Rust-Adapter
- [x] TypeScript-/JavaScript-Adapter
- [x] Python-Adapter
- [x] Symbolgraph und atomisches Publish
- [x] File Watcher und inkrementeller Delta-Lauf

### M4 Retrieval

- [ ] exakte Pfad- und Symbolsuche
- [ ] FTS
- [ ] Graphtraversierung
- [ ] Test- und Einstiegspunktsuche
- [ ] Ranking und Ergebnisbegründung
- [x] Retrieval-Golden-Evals

### M5 Project Map

- [ ] Module Clustering und Seeds
- [ ] Module-Card-Schema
- [ ] budgetierter Read-only Explorer
- [ ] Claim-Verifier
- [ ] Deep-Map-Coverage und Stopbedingungen
- [x] Task Lens
- [x] stale Card Invalidation

### M6 Durable Harness

- [x] Goal Contract
- [x] Task Ledger
- [x] Run Journal
- [x] Context Compiler
- [x] evidenzgebundene Run-Memory-Compaction
- [x] Ollama-kompatibler Provider
- [x] ModelProfile und Capability Self-Test
- [x] read-only Controllerlauf
- [x] Resume nach Neustart

### M7 Editing Agent

- [x] Policy Engine und Approval-Modell
- [x] begrenzte sichere Dateiwerkzeuge und evidenzgebundenes Directory Listing
- [x] hashgeschützte PatchAction
- [x] argv ProcessRunner
- [x] evidenzgebundene Command Discovery und projektbezogene Allowlist
- [x] Test- und Buildverifikation
- [ ] Replan und Failure Recovery
- [ ] ein Mutations-Lock pro Worktree
- [x] Done-Gate gegen Acceptance Criteria

### M8 Desktop Product

- [ ] Projects
- [ ] Index Progress
- [ ] Map
- [ ] Agent Workspace
- [ ] Diff und Evidence Inspector
- [ ] Approval UI
- [ ] Settings und Model Health
- [ ] Keyboard und Accessibility
- [ ] Cancellation und Recovery UX

### M9 Evaluation

- [ ] Fixture-Repositories
- [ ] Retrieval-Evalset
- [ ] Agentenaufgaben
- [ ] Performanceharness
- [ ] RTX-5070-Ti-Modellprofilmessungen
- [ ] Regressiondashboard lokal oder als CI-Artefakt

### M10 Cross-Platform Beta

- [ ] Windows-Installer und Smoke-Test
- [ ] Linux AppImage oder Paket und Smoke-Test
- [ ] macOS Universalstrategie, Signierung und Notarisierung
- [ ] Update- und Rollbackstrategie
- [ ] Datenbankmigrationsprobe aus realer Vorversion

### M11 V1

- [ ] alle Muss-Gates grün
- [ ] Threat Model überprüft
- [ ] Datenschutz- und Offline-Dokumentation
- [ ] User Guide und Troubleshooting
- [ ] reproduzierbare Buildartefakte
- [ ] Release Notes und bekannte Grenzen

## Globale Exit-Kriterien

V1 ist nicht abgeschlossen, solange eines gilt:

- ein Muss-Akzeptanzkriterium ist nur durch Text statt Evidence bestätigt;
- stale Facts können den Context Compiler erreichen;
- eine Mutation kann den erlaubten Root verlassen;
- UI besitzt generische Shell-, FS- oder SQL-Rechte;
- ein langer Run verliert Goal Contract oder erledigte Verifikation;
- Indexdelta benötigt regelmäßig Full Reindex;
- Windows, Linux oder macOS hat keinen bestandenen Smoke-Test;
- ein Performancebudget ist weder erfüllt noch mit begründeter Abweichung dokumentiert.
