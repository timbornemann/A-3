# A^3 – Autonomous Agent Assistant

A^3 ist ein lokaler, plattformübergreifender Coding-Agent für Windows, Linux und macOS. Seine Zuverlässigkeit entsteht durch einen deterministischen Harness, evidenzgebundenes Gedächtnis, kontrollierte Werkzeuge und eine präzise Kontextzusammenstellung.

Der sichtbare Produktname besteht exakt aus den drei Zeichen **A^3**. In technischen Bezeichnern, in denen `^` ungeeignet ist, wird der Slug **a3** verwendet.

## Entwicklungsstand

Das Repository befindet sich im Foundation-Meilenstein. Die verbindliche Architektur- und Entwicklungsbaseline liegt unter [`docs/`](docs/README.md); implementierte Funktionen dürfen den dort festgelegten Entscheidungen und Qualitätsgates nicht widersprechen.

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

Die WebView ist unprivilegiert. Im aktuellen Walking Skeleton darf sie ausschließlich den typisierten `query_health`-Command aufrufen; Datei-, Shell- und SQL-Zugriffe sind nicht freigegeben.

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
