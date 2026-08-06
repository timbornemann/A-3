# A^3 – Autonomous Agent Assistant

A^3 ist ein lokaler, plattformübergreifender Coding-Agent für Windows, Linux und macOS. Seine Zuverlässigkeit entsteht durch einen deterministischen Harness, evidenzgebundenes Gedächtnis, kontrollierte Werkzeuge und eine präzise Kontextzusammenstellung.

Der sichtbare Produktname besteht exakt aus den drei Zeichen **A^3**. In technischen Bezeichnern, in denen `^` ungeeignet ist, wird der Slug **a3** verwendet.

## Entwicklungsstand

Der Foundation-Meilenstein sowie Projektidentität, lokale Storage-Grundlage, begrenzte
Repository-Discovery, inhaltsbasierte Worktree-Snapshots und der versionierte Tree-sitter-
LanguageAdapter-Contract sowie die Rust-/Cargo-, TypeScript-/JavaScript-/Package- und
Python-/Packaging-Produktadapter sowie der deterministische Graph-Linker, die versionierte
Rankingprojektion, das atomische Publish und der plattformneutrale inkrementelle File-Watcher des
Fast Index sowie die snapshotgebundene exakte und fehlertolerante lexikalische Pfad-, Symbol- und
Signatursuche, die begrenzte evidenzgebundene Graphabfrage und die versionierte deterministische
Retrieval Fusion, optionale profilisolierte Embeddings sowie die LLM-freie Manifest-/Pfad-
Modulbildung mit ergänzenden Graphcommunities und deterministischer Repository Card sind
abgeschlossen. Ebenfalls implementiert sind das versionierte Module-Card-Schema, der
veröffentlichungsgebundene Deep-Map-Planer, der begrenzte Read-only Explorer, die strikte
Claim-Verifikation, atomare Card-Publikation, die budgetierte Task Lens und die direkte plus
ein Hop begrenzte Card-Invalidierung mit priorisierter Remapqueue. Eine versionierte, vollständig
offline laufende Retrieval-Evalbaseline misst den real publizierten Exact-, Lexical- und
Graphpfad des gemischten Rust-/TypeScript-/Python-Fixtures. Die mehrsprachige
Deep-Map-Abnahme führt außerdem die drei Produkt-Fixtures durch Snapshot, Publish,
Modulprojektion und budgetierte Planung. Eine zusätzliche Offline-Abnahme führt den aktuellen
M4/M5-Anwendungskern über Indexierung, Deep Map und Task Lens vollständig ohne Semantic-Port aus
und bestätigt den konstruktiv deaktivierten Embedding-Batchpfad. Der Durable Harness besitzt nun
einen revisionierten Goal Contract, ein verifiziertes Task Ledger und ein append-only Run Journal
mit atomarer libSQL-Materialisierung, sicherer Redaction und versioniertem JSONL-Export. Der
allgemeine lokale `ModelProvider` besitzt begrenzte neutrale Streaming-Events, Cancellation und
Gesamttimeout; sein Ollama-kompatibler Adapter erzwingt eine Local-only-Endpoint-Policy, strikte
NDJSON-Validierung und content-freie Fehler. Das versionierte `ModelProfile`, konservative
Tokenzählung, manuelle laufparametersichere Overrides sowie ein echter Ollama-Capability-Self-Test
sind ebenfalls abgeschlossen. Fehlgeschlagene Structured-Output-Proben können keine ausführbaren
Aktionen freischalten. Der statische Promptvertrag und die allgemeine versionierte AgentAction-
Union sind nun als kompakter budgetierter Systemvertrag, eingebettetes striktes JSON-Schema,
unabhängiger Runtime-Decoder und verbrauchbarer einzelner Repair-Versuch umgesetzt. Als Nächstes
folgt der deterministische Context Compiler. Die
verbindliche Architektur- und Entwicklungsbaseline liegt unter
[`docs/`](docs/README.md); implementierte Funktionen dürfen den dort festgelegten Entscheidungen und
Qualitätsgates nicht widersprechen.

## Projektmodell

A^3 wird als öffentliches Open-Source-Repository unter `GPL-3.0-only` entwickelt. Verantwortlicher Maintainer und finale Freigabeinstanz für Architecture Decision Records ist **Tim Bornemann**.

## Voraussetzungen

- Rust `1.93.1` mit Cargo, rustfmt und Clippy
- Node.js `24.14.0`
- Corepack mit dem über `package.json` gepinnten pnpm `11.9.0`

Die Dateien `rust-toolchain.toml`, `.node-version` und `package.json` sind die maßgeblichen Versionsquellen. Eine Node-Distribution ohne gebündeltes Corepack benötigt vor dem Bootstrap eine separate Corepack-Installation.

## Lokaler Bootstrap

```text
corepack enable
pnpm install --frozen-lockfile
cargo metadata --format-version 1 --no-deps
cargo fmt --check
```

Der Bootstrap installiert keine globale Projekt-Runtime und startet keinen Hintergrunddienst. Provider- und Netzwerkkonnektivität gehören nicht zum Foundation-Schnitt.

## Desktop lokal starten

Nach dem Bootstrap startet der Entwicklungsmodus Frontend und Tauri gemeinsam:

```text
pnpm tauri dev
```

Ein lokaler Produktionsbuild ohne Installer wird so erzeugt:

```text
pnpm tauri build --no-bundle
```

Die WebView ist unprivilegiert. Sie darf ausschließlich die typisierten Commands `query_health`,
`open_project` und `list_recent_projects` aufrufen. `open_project` öffnet den nativen Ordnerdialog im
Rust-Kern und bietet bei einem eindeutig evidenzbasiert erkannten Worktree-Umzug eine zweite native
Auswahl zum Reconciliieren, separaten Öffnen oder Abbrechen an. `list_recent_projects` liefert höchstens
zehn validierte Anzeigeprojektionen aus dem lokalen Katalog. Die WebView sendet weder Pfad noch
Reconciliation-Entscheidung und erhält keine Datei-, Dialog-, Shell- oder SQL-Plugin-Berechtigung.
Nach einem erfolgreichen Open startet der Rust-Composition-Root einen besitzenden, begrenzten
Repository-Watcher und aktualisiert den lokalen Index im Hintergrund. Dieser Pfad erweitert die
WebView-Capabilities nicht.

## Lokale Qualitätsgates

Die CI-relevanten lokalen Prüfungen sind als stabile Root-Skripte verfügbar:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
pnpm ci:frontend
pnpm check:links
pnpm report:dependencies
```

Der Dependency-/Lizenzbericht wird unter `target/reports/dependency-license-report.json` erzeugt und enthält keine lokalen absoluten Pfade. Aufbau, Runner-Matrix und Grenzen der Foundation-CI beschreibt die [CI-Dokumentation](docs/CI.md).

## Repositorystruktur

```text
apps/desktop/       Svelte-WebView und Tauri-Composition-Root
crates/             Rust-Domain, Use Cases, Features und Adapter
tests/storage-contract/
                    Adapterneutrale Storage-Verträge (nur Entwicklung und CI)
tests/model-provider-contract/
                    Neutraler Provider-Stub (nur Entwicklung und CI)
fixtures/           kleine lizenzkompatible Test-Repositories
docs/               verbindliche Architektur, ADRs und Pläne
```

Crates und Frontend-Pakete werden erst angelegt, wenn ein verifizierbarer vertikaler Slice sie benötigt. Leere Architekturhüllen sind ausdrücklich nicht vorgesehen.

## Dokumentenkarte

| Dokument | Zweck |
| --- | --- |
| [AGENTS.md](AGENTS.md) | Verbindlicher Arbeits- und Engineeringvertrag |
| [Produktanforderungen](docs/PRODUCT_REQUIREMENTS.md) | V1-Funktionsumfang und messbare Produktziele |
| [Architektur](docs/ARCHITECTURE.md) | Systemaufbau, Komponenten und Abhängigkeiten |
| [Architekturregeln](docs/ARCHITECTURE_RULES.md) | Verbindliche Code- und Modulregeln |
| [Qualitätsgates](docs/QUALITY_GATES.md) | Tests, Leistungsbudgets und Definition of Done |
| [Continuous Integration](docs/CI.md) | Lokale Gates, Plattformmatrix und Lizenzbericht |
| [ADRs](docs/adrs/README.md) | Akzeptierte Architekturentscheidungen |
| [Entwicklungspläne](docs/plans/README.md) | Reihenfolge, Arbeitspakete und Abnahmekriterien |

## Lizenz

A^3 wird unter der [GNU General Public License Version 3](LICENSE) (`GPL-3.0-only`) veröffentlicht.
