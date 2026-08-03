# ADR-0014: Cross-Platform-Release mit messbaren Qualitätsgates

Status: Accepted  
Datum: 2026-08-03

## Kontext

Tauri nutzt auf Windows, macOS und Linux unterschiedliche System-WebViews. Dateisystem, Prozessmanagement, Pfade und Signierung unterscheiden sich ebenfalls. „Kompiliert auf Linux“ beweist keine Desktopportabilität.

## Entscheidung

- CI baut und testet Windows x86_64, Linux x86_64, macOS ARM64 und, solange praktikabel, macOS x86_64.
- Plattformcode liegt hinter Adaptern und besitzt Smoke-Tests.
- Installer werden nativ je Zielplattform erzeugt.
- Signierung und Notarisierung sind Releasevoraussetzung, sobald öffentliche Distribution beginnt.
- QUALITY_GATES.md definiert verbindliche Korrektheits- und Performanceziele.
- Performanceaussagen benötigen reproduzierbare Benchmarks.
- App-Ressourcen werden getrennt vom Modellserver gemessen.

## Konsequenzen

### Positiv

- Plattformfehler werden früh sichtbar.
- Geschwindigkeit wird als Produktanforderung behandelt.
- Releases sind reproduzierbar.

### Negativ

- CI- und Signieraufwand
- einige End-to-End-Tests benötigen Plattformrunner.

### Risiken und Gegenmaßnahmen

- WebView-Renderingunterschiede — kleine UI-Abstraktionen und visuelle Smoke-Tests.
- fehlende Signieridentität — interne Artefakte erlaubt, aber kein als final markierter öffentlicher Release.

## Verworfene Alternativen

- Linux-first bis zum Ende — hohes spätes Portierungsrisiko.
- nur manuelle Tests — nicht reproduzierbar.
- ein plattformfremd erzeugter Universalinstaller — technisch und signierseitig unzuverlässig.

## Compliance

Protected Release Workflow erfordert grüne Plattformmatrix und dokumentierte Benchmarkresultate.

