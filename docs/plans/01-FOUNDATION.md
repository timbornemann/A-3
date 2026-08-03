# Plan 01: Foundation und Walking Skeleton

Ziel: Ein minimaler A^3-Desktop-Slice beweist Build, Architekturgrenzen, IPC, Jobs und Cross-Platform-CI.

Relevante ADRs: 0001, 0002, 0003, 0014

## F1 Repository bootstrap

Abhängigkeiten: keine

- [ ] Workspaceverzeichnisse gemäß ARCHITECTURE.md anlegen
- [ ] Root Cargo.toml mit zentralen Package- und Lintregeln
- [ ] pnpm über Corepack als einzigen Frontend-Paketmanager konfigurieren und Version pinnen
- [ ] rust-toolchain.toml und Node-Version pinnen
- [ ] EditorConfig, Gitignore und Formattingkonfiguration
- [ ] README, AGENTS.md, docs und ADRs integrieren
- [ ] minimale Lizenzdatei nach Nutzerentscheidung

Akzeptanz:

- frischer Checkout besitzt einen dokumentierten Bootstrapweg;
- Lockfiles sind committed;
- keine doppelte Paketmanagerkonfiguration;
- cargo metadata zeigt keine Dependency-Zyklen.

Prüfung:

- cargo metadata
- cargo fmt --check
- Frontend-Install mit frozen Lockfile

## F2 Crate boundaries

Abhängigkeiten: F1

- [ ] a3-domain mit Version- und Health-Value-Objects
- [ ] a3-application mit HealthQuery-Port
- [ ] a3-protocol mit versioniertem Health DTO
- [ ] leere Adapter nur anlegen, wenn der erste Slice sie nutzt
- [ ] src-tauri als Composition Root

Akzeptanz:

- Domain importiert kein Tauri oder Adapter-Crate;
- HealthQuery ist über einen Use Case erreichbar;
- Protokoll-DTO und Domain-Typ werden explizit gemappt.

Prüfung:

- Unit-Test des Health Use Case
- Dependency-Graph-Check

## F3 Tauri-Svelte walking skeleton

Abhängigkeiten: F2

- [ ] Tauri 2 und Svelte/TypeScript mit statischem Build
- [ ] UI zeigt exakt A^3 und Autonomous Agent Assistant
- [ ] UI ruft HealthQuery auf
- [ ] Rust antwortet mit Appversion, Protokollversion und Plattform
- [ ] sicherer Fehlercodepfad
- [ ] keine generische Tauri-FS-, Shell- oder SQL-Capability

Akzeptanz:

- Desktop startet und zeigt Healthzustand;
- IPC-Payload wird auf beiden Seiten typgeprüft;
- ungültige Payload wird abgelehnt und erzeugt keinen Panic.

Prüfung:

- Rust Integrationstest
- Frontend Component Test
- manueller Smoke auf Entwicklerplattform

## F4 Job primitives

Abhängigkeiten: F2

- [ ] JobId, JobStatus, Progress und CancellationToken
- [ ] begrenzter Job Scheduler
- [ ] typisierte ProgressEvents
- [ ] Shutdown wartet oder bricht Jobs kontrolliert ab
- [ ] FakeClock und deterministische Tests

Akzeptanz:

- ein Testjob meldet geordnet Fortschritt;
- Cancellation beendet ihn innerhalb des Budgets;
- kein detached Task bleibt nach Shutdown.

Prüfung:

- Concurrency- und Cancellation-Tests

## F5 CI baseline

Abhängigkeiten: F1 bis F4

- [ ] Rust fmt, Clippy und Tests
- [ ] Frontend format, lint, typecheck und tests
- [ ] Builds auf Windows, Linux und macOS
- [ ] Dependency- und Lizenzbericht als CI-Artefakt
- [ ] Markdown-Linkprüfung für lokale Links

Akzeptanz:

- ein absichtlicher Warning-, Test- oder Linkfehler macht CI rot;
- Plattformmatrix baut den Walking Skeleton.

## Gate M1

- [ ] alle F-Pakete abgeschlossen
- [ ] QUALITY_GATES.md erfüllt
- [ ] Architekturreview bestätigt unprivilegierte UI
- [ ] README enthält lokale Startanleitung
- [ ] keine Platzhalter-Panics oder unerreichbaren TODO-Pfade
