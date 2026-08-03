# ADR-0001: Produktidentität und V1-Scope

Status: Accepted  
Datum: 2026-08-03

## Kontext

Ein konsistenter Name und ein enger erster Produktscope verhindern Drift in UI, Packaging, Dokumentation und Entwicklung.

## Entscheidung

- Der sichtbare Name lautet exakt **A^3**.
- Die ausgeschriebene Form lautet exakt **Autonomous Agent Assistant**.
- Der technische Slug lautet **a3**.
- A^3 ist in V1 ein lokaler Coding-Agent als Desktopanwendung für Windows, Linux und macOS.
- V1 ist offline-first und single-user.
- V1 verwendet einen einzelnen deterministischen Agentencontroller.
- Cloud-Sync, verteilte Agenten, autonomes Publishing und vollständige IDE-Funktionen liegen außerhalb von V1.

## Konsequenzen

### Positiv

- Klare Produktidentität und überprüfbarer Scope
- Fokus auf Harness- und Retrievalqualität
- Keine Cloudabhängigkeit

### Negativ

- Kollaboration und mobile Nutzung werden zunächst nicht adressiert.
- Ein technischer Slug unterscheidet sich vom sichtbaren Namen.

## Verworfene Alternativen

- A3 oder A³ als sichtbarer Name — widerspricht der festgelegten Drei-Zeichen-Schreibweise.
- Multi-Agent-System in V1 — erhöht Prompt-, Koordinations- und Debuggingkosten.
- Cloudpflicht — widerspricht Datenschutz und lokaler Nutzbarkeit.

## Compliance

UI-Snapshots und Packagingtests prüfen den Produktnamen. Scope-Erweiterungen benötigen ein neues ADR.

