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
| [ADRs](docs/adrs/README.md) | Akzeptierte Architekturentscheidungen |
| [Entwicklungspläne](docs/plans/README.md) | Reihenfolge, Arbeitspakete und Abnahmekriterien |

## Lizenz

A^3 wird unter der [GNU General Public License Version 3](LICENSE) (`GPL-3.0-only`) veröffentlicht.
