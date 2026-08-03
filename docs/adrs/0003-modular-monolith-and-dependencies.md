# ADR-0003: Modularer Monolith mit Ports und Adaptern

Status: Accepted  
Datum: 2026-08-03

## Kontext

A^3 hat mehrere komplexe Verantwortlichkeiten, soll aber leichtgewichtig bleiben. Unstrukturierter Monolithcode würde Kopplung erzeugen; Services würden Betrieb und Fehlersuche unnötig erschweren.

## Entscheidung

A^3 ist ein modularer Monolith in einem Cargo-Workspace.

- a3-domain enthält reine Domäne und Invarianten.
- a3-application enthält Use Cases und Ports.
- Feature-Crates kapseln Index, Project Map und Context.
- Adapter-Crates kapseln Storage, Modellprovider und Workspace.
- a3-protocol definiert IPC-Grenztypen.
- src-tauri ist Composition Root.

Abhängigkeiten zeigen nach innen. Domain und Application kennen keine konkreten Infrastrukturadapter.

## Konsequenzen

### Positiv

- klare Verantwortlichkeit und testbare Grenzen
- ein Prozess, einfache lokale Installation
- Adapter können ohne Fachumbau ersetzt werden

### Negativ

- mehr Crates und explizites Mapping zwischen Grenztypen
- Gefahr zu früher Abstraktion

### Risiken und Gegenmaßnahmen

- Leere Architekturhüllen — jede neue Grenze muss durch eine reale Verantwortung und einen vertikalen Slice belegt sein.
- Crate-Zyklen — CI prüft den Dependency Graph.

## Verworfene Alternativen

- Ein einziges großes Crate — Grenzen wären nur Konvention.
- Microservices — kein Nutzen für lokalen Single-User-Prozess.
- Pluginarchitektur in V1 — erhöht ABI-, Sicherheits- und Lifecycle-Komplexität.

## Compliance

Dependency-Check in CI; Reviewfrage für jede neue Cross-Crate-Abhängigkeit.

