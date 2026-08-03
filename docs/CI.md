# A^3 Continuous Integration

Status: Foundation-Baseline

Stand: 2026-08-04

## Workflow und Rechte

Der Workflow [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) läuft bei Pushes, Pull Requests und manueller Auslösung. Er besitzt ausschließlich `contents: read`; Checkout persistiert keine Git-Credentials. Der Workflow veröffentlicht nichts, erzeugt keinen Release und baut keine Installer.

Alle verwendeten GitHub-Actions sind auf vollständige Commit-SHAs gepinnt. Node.js, pnpm und Rust stammen aus den im Repository versionierten Toolchain-Dateien. Frontend-Abhängigkeiten werden ausschließlich mit `pnpm install --frozen-lockfile` installiert.

## Quality-Job

Der Quality-Job auf Ubuntu 22.04 führt aus:

1. Rustfmt;
2. Clippy für den gesamten Workspace, alle Targets und Features mit `-D warnings`;
3. alle Rust- und Doc-Tests;
4. Rustdoc mit `-D warnings`;
5. Prettier, ESLint, Svelte-Typecheck, Frontend- und CI-Tool-Tests sowie den statischen Frontend-Build;
6. den lokalen Markdown-Linkcheck;
7. die Erzeugung und den Upload des Dependency-/Lizenzberichts.

Der Bericht `target/reports/dependency-license-report.json` ist deterministisch für identische Lockfiles und installierte Abhängigkeiten. Er enthält normalisierte Paketnamen, Versionen, Lizenzangaben und Quellenklassen für Cargo und pnpm, aber keine lokalen absoluten Pfade. Unbekannte Lizenzen lassen die Tool-Tests fehlschlagen. Das CI-Artefakt wird 14 Tage aufbewahrt.

## Plattformmatrix

Die native Matrix führt Workspace-Tests und `tauri build --no-bundle` auf vier Runnern aus:

| Ziel | Runner |
| --- | --- |
| Linux x86_64 | `ubuntu-22.04` |
| Windows x86_64 | `windows-2022` |
| macOS ARM64 | `macos-15` |
| macOS x86_64 | `macos-15-intel` |

Ubuntu 22.04 ist bewusst die ältere von Tauri empfohlene Linux-Baseline mit WebKitGTK 4.1. GitHub dokumentiert `macos-15` aktuell als ARM64- und `macos-15-intel` als Intel-Runner. Signierte Installer und Plattform-Smokes mit echter UI bleiben spätere Release-Arbeitspakete.

Quellen: [GitHub-hosted runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners), [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/), [Tauri AppImage baseline](https://v2.tauri.app/distribute/appimage/).

## Lokale Ausführung

Nach dem Bootstrap entsprechen diese Befehle den CI-Gates:

~~~text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
pnpm ci:frontend
pnpm check:links
pnpm report:dependencies
pnpm --filter @a3/desktop tauri build --no-bundle
~~~

Der Foundation-Plan bleibt bis zum ersten vollständig grünen öffentlichen Matrixlauf `In Progress`. Ein lokal erfolgreicher Windows-Build ist kein Ersatz für den Linux- oder macOS-Nachweis.
