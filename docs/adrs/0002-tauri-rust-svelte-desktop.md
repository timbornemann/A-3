# ADR-0002: Tauri 2, Rust und Svelte für Desktop

Status: Accepted  
Datum: 2026-08-03

## Kontext

A^3 benötigt performanten Zugriff auf Dateien, Prozesse, Datenbank und Watcher sowie eine moderne plattformübergreifende UI. Eine gebündelte Browser-Runtime würde Speicher- und Paketgröße erhöhen.

## Entscheidung

- Desktop Shell: Tauri 2
- privilegierter Core: Rust
- UI: Svelte mit TypeScript und statischem Vite-Build
- UI und Core kommunizieren über schmale typisierte IPC-Commands und Events.
- Die WebView bleibt unprivilegiert; Geschäftslogik und sensible Zustände liegen im Core.
- Es wird keine Node-Runtime als dauerhafter Backendprozess ausgeliefert.

## Konsequenzen

### Positiv

- Native Systemintegration und speichersicherer Core
- Nutzung der OS-WebView statt gebündelter Browser-Runtime
- ein UI-Codebestand für drei Desktopplattformen

### Negativ

- WebView-Unterschiede zwischen WebView2, WKWebView und WebKitGTK müssen getestet werden.
- Rust- und TypeScript-Toolchain sind erforderlich.
- Plattformsignierung bleibt plattformspezifisch.

### Risiken und Gegenmaßnahmen

- IPC wird zur Sicherheitsgrenze — versionierte Schemas, Payloadlimits und Capabilities.
- UI kann während CPU-Arbeit stocken — Index und Agentenjobs laufen außerhalb des UI-Threads.

## Verworfene Alternativen

- Electron — höherer Runtime- und Speicher-Overhead.
- rein native separate UIs — dreifacher Produktaufwand.
- vollständig Rust-basierte UI — derzeit geringere Produktivität für komplexe Desktopinteraktionen.

## Compliance

Frontend darf keine direkten Filesystem-, Shell- oder SQL-Capabilities erhalten. Architekturtests prüfen verbotene Crate-Abhängigkeiten.

## Referenzen

- [Tauri Architecture](https://v2.tauri.app/concept/architecture/)
- [Tauri Process Model](https://v2.tauri.app/concept/process-model/)

