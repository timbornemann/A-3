# Produktanforderungen für A^3 V1

Status: verbindliche Baseline  
Stand: 2026-08-03

## Produktversprechen

A^3 ermöglicht einem Entwickler, einen lokalen Coding-Agenten mit begrenztem Modellkontext sicher und nachvollziehbar auf einer großen lokalen Codebasis arbeiten zu lassen. Die Software bleibt ohne Cloud funktionsfähig und erklärt, auf welcher echten Codeevidenz Planung, Änderung und Abschluss beruhen.

## Funktionale Anforderungen

### Projekte

- **FR-001** A^3 MUSS einen lokalen Git-Worktree über einen nativen Ordnerdialog öffnen können.
- **FR-002** A^3 MUSS Repositories ohne Remote, zusätzliche Git-Worktrees und einen Unborn-Branch unterstützen.
- **FR-003** A^3 MUSS zuletzt verwendete Projekte verwalten, ohne Runtime-Dateien in das Repository zu schreiben.
- **FR-004** Das Entfernen eines Projekts aus A^3 DARF den Quellcode nicht löschen.

### Index

- **FR-010** A^3 MUSS relevante Dateien unter Beachtung von Git- und A^3-Ignore-Regeln entdecken.
- **FR-011** A^3 MUSS Dateirevisionen über Content Hashes und Snapshots identifizieren.
- **FR-012** A^3 MUSS Rust, TypeScript/JavaScript und Python strukturell indexieren.
- **FR-013** A^3 MUSS Symbole, Imports, Exports, syntaktische Calls, Tests, Manifeste und Einstiegspunkte erfassen.
- **FR-014** A^3 MUSS Änderungen inkrementell verarbeiten und gelöschte Daten invalidieren.
- **FR-015** A^3 MUSS einen letzten konsistenten Index behalten, wenn ein neuer Indexlauf scheitert.

### Retrieval und Karte

- **FR-020** A^3 MUSS exakte Pfad- und Symbolsuche, FTS sowie begrenzte Graphtraversierung anbieten.
- **FR-021** Jeder Treffer MUSS Herkunft und Beziehung zur Query erklären.
- **FR-022** A^3 MUSS ohne Embeddings vollständig für deterministische Suche nutzbar bleiben.
- **FR-023** A^3 MUSS eine Repository- und Modulkarte ohne LLM erzeugen können.
- **FR-024** A^3 MUSS eine budgetierte Deep Map mit evidenzgebundenen Module Cards erzeugen können.
- **FR-025** Facts, Observations und Hypotheses MÜSSEN unterscheidbar sein.
- **FR-026** Für eine Aufgabe MUSS A^3 eine kleine Task Lens statt eines vollständigen Repositorydumps erzeugen.

### Aufgaben und Memory

- **FR-030** Jede Agentenaufgabe MUSS einen revisionierten Goal Contract besitzen.
- **FR-031** A^3 MUSS Akzeptanzkriterien, Grenzen, Non-Goals und Nutzerentscheidungen dauerhaft halten.
- **FR-032** A^3 MUSS Planung als typisiertes Task Ledger mit überprüfbaren Schritten verwalten.
- **FR-033** Ein Schritt DARF nur nach erfolgreicher Verification als Completed gelten.
- **FR-034** Veraltete Evidence MUSS abhängige Claims und abgeschlossene Schritte invalidieren.
- **FR-035** Ein Run MUSS nach Appneustart sicher fortsetzbar, neu planbar oder abbrechbar sein.

### Modell und Kontext

- **FR-040** A^3 MUSS einen explizit konfigurierten lokalen Ollama-kompatiblen Modellserver verwenden können.
- **FR-041** A^3 MUSS Modellfähigkeiten in einem ModelProfile validieren.
- **FR-042** Jeder Turn MUSS ein neu kompiliertes, tokenbegrenztes Context Pack verwenden.
- **FR-043** Goal Contract, aktueller Schritt und Verifikationsstatus MÜSSEN in jedem Turn verankert sein.
- **FR-044** Ungültige strukturierte Modellausgabe DARF nie ausgeführt werden.
- **FR-045** Der Indexbrowser MUSS ohne konfiguriertes Modell nutzbar sein.

### Werkzeuge und Agent

- **FR-050** Der Agent MUSS über einen endlichen Controller mit dokumentierten Zuständen laufen.
- **FR-051** Pro Turn DARF höchstens eine Tool Action ausgeführt werden.
- **FR-052** A^3 MUSS Suche, gezielte Inspektion, Patchen, lokale Prozessausführung, Ledgerupdate und Finish anbieten.
- **FR-053** Pro Worktree DARF höchstens eine Mutation gleichzeitig laufen.
- **FR-054** Patches MÜSSEN den erwarteten Snapshot und Content Hash prüfen.
- **FR-055** Prozesse MÜSSEN argv-basiert, begrenzt und abbrechbar sein.
- **FR-056** Nach einer Mutation MUSS der Indexzustand vor weiterer Modellarbeit abgeglichen werden.
- **FR-057** Done MUSS an aktuelle Muss-Akzeptanzkriterien und Evidence gebunden sein.

### Freigaben und Sicherheit

- **FR-060** Read- und Derive-Aktionen innerhalb des erlaubten Roots KÖNNEN automatisch laufen.
- **FR-061** Netzwerk, Paketinstallation, Shellmodus, Destruktion, Publishing und Outside-Root-Zugriff MÜSSEN explizit freigegeben werden.
- **FR-062** Die UI MUSS Risk, Scope und genaue Aktion vor einer Freigabe zeigen.
- **FR-063** Freigaben MÜSSEN scopegebunden, widerrufbar und auditierbar sein.
- **FR-064** Quellcodeinhalte DÜRFEN Sicherheitsregeln nicht überschreiben.

### Desktop UX

- **FR-070** A^3 MUSS Projects, Map, Agent und Settings als Hauptbereiche anbieten.
- **FR-071** Ziel, aktueller Schritt, Snapshot, Modell- und Runstatus MÜSSEN während eines Runs sichtbar sein.
- **FR-072** Nutzer MÜSSEN Index- und Deep-Map-Jobs pausieren oder abbrechen können.
- **FR-073** Diff, Toolresultate, Evidence und Verification MÜSSEN nachvollziehbar sein.
- **FR-074** Der Kernworkflow MUSS vollständig per Tastatur nutzbar sein.
- **FR-075** Ask, Plan und Agent-Vorbereitung MÜSSEN ihren endlichen Evidence-Arbeitsweg mit
  öffentlichen Befunden und Quellen nachvollziehbar anzeigen; internes Chain-of-Thought bleibt
  ausgeschlossen.

## Nichtfunktionale Anforderungen

- **NFR-001 Offline:** Alle Kernfunktionen außer dem explizit gewählten Modellendpunkt funktionieren ohne Internet.
- **NFR-002 Plattformen:** Windows x86_64, Linux x86_64 und macOS ARM64 werden für V1 gebaut und getestet.
- **NFR-003 Datenschutz:** Kein Quellcode, Prompt, Embedding, Log oder Telemetrie verlässt ohne ausdrückliche Aktivierung das Gerät.
- **NFR-004 Sicherheit:** Ein kompromittiertes Frontend oder eine manipulierte Modellausgabe erhält keine generischen OS-Rechte.
- **NFR-005 Konsistenz:** Leser sehen nur vollständig veröffentlichte Indexsnapshots.
- **NFR-006 Reproduzierbarkeit:** Context und Retrieval speichern Snapshot, Policy-Version und Digest.
- **NFR-007 Leistung:** Die Budgets aus QUALITY_GATES.md gelten als V1-Ziele.
- **NFR-008 Wartbarkeit:** Abhängigkeiten folgen ADR-0003 und alle Boundaryadapter besitzen Contract-Tests.
- **NFR-009 Wiederherstellung:** Ein Crash darf dauerhafte Task- und Entscheidungsdaten nicht still verlieren.
- **NFR-010 Zugänglichkeit:** Kerninteraktionen erfüllen WCAG-nahe Desktopanforderungen für Tastatur, Focus und Kontrast.

## Traceability

| Bereich | Hauptplan | Kern-ADRs |
| --- | --- | --- |
| Desktop und Plattform | 01, 06, 07 | 0001, 0002, 0014 |
| Projekt und Storage | 02 | 0004, 0005 |
| Index | 02 | 0006, 0015 |
| Retrieval und Karte | 03 | 0006, 0007, 0008 |
| Memory und Context | 04 | 0008, 0009, 0013 |
| Controller und Modell | 04 | 0010, 0011 |
| Editing und Sicherheit | 05 | 0012, 0013 |
| Evaluation und Release | 07 | 0014 |

## V1-Abnahme

Eine Anforderung gilt nur als erfüllt, wenn ein automatisierter Test, ein reproduzierbarer Benchmark oder eine dokumentierte manuelle Plattformprüfung als Evidence verlinkt ist.
- Der Agent-Bereich ist eine einzige professionelle, minimalistische Chat-Arbeitsfläche mit
  projektlokalem Verlauf, zentralem Composer und kontextuellem Fortschritts-/Änderungs-/Review-
  Inspector.
- Neue Sessions starten standardmäßig im Modus `Agent`; `Ask` sammelt und berichtet ausschließlich
  Informationen, `Plan` erarbeitet mit Rückfragen einen reviewbaren Plan, und `Agent` arbeitet
  nach dessen Freigabe über den sicheren Harness.
- Nutzer sehen laufenden Status, aktuelle Tätigkeit, notwendige Freigaben, verifizierte Änderungen
  und den Abschlussreview und können danach im selben Kontext nachfragen oder eine neue Session
  beginnen.
