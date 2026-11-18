# Produktanforderungen für A^3 V1

Status: verbindlicher Zielvertrag nach ADR-0119; bestehende Implementierung bleibt Altbaseline

Stand: 2026-11-06

Die Anforderungen an Modi, Aufgabenführung und Kontext folgen dem angenommenen
[ADR-0119](adrs/0119-kleinschrittiger-agentenkern-fuer-lokale-modelle.md) und dem
[Zielvertrag](SMALL_LOCAL_MODEL_AGENT_CORE.md). Sie sind keine Behauptung bereits
implementierter Funktionen. Nach [Plan 22](plans/22-SMALL-LOCAL-MODEL-AGENT-CORE.md)
entstehen die neuen Verträge in P2 bis P10 ausschließlich in der Kandidaten-Testkomposition.
Bis P11 bleibt der bisherige SourceGuided-Produktionspfad einschließlich seiner
Ask-/Plan-/Agent-Verträge aktiv. P11 ist die einzige Produktionsumschaltung mit
frisch initialisierten A^3-Daten. Nach [ADR-0119](adrs/0119-kleinschrittiger-agentenkern-fuer-lokale-modelle.md) entfallen Migration und
Lesbarkeitspflicht alter Entwicklungsdaten; Neustart und Recovery neuer Runs bleiben Pflicht.

## Produktversprechen

A^3 ermöglicht einem Entwickler, einen lokalen Coding-Agenten mit begrenztem Modellkontext sicher und nachvollziehbar auf einer großen lokalen Codebasis arbeiten zu lassen. Die Software bleibt ohne Cloud funktionsfähig und erklärt, auf welcher echten Codeevidenz Planung, Änderung und Abschluss beruhen.

## Funktionale Anforderungen

### Projekte

- **FR-001** A^3 MUSS einen lokalen Git-Worktree über einen nativen Ordnerdialog öffnen können.
- **FR-002** A^3 MUSS Repositories ohne Remote, zusätzliche Git-Worktrees und einen Unborn-Branch unterstützen.
- **FR-003** A^3 MUSS zuletzt verwendete Projekte verwalten, ohne Runtime-Dateien in das Repository zu schreiben.
- **FR-004** Das Entfernen eines Projekts aus A^3 DARF den Quellcode nicht löschen.
- **FR-005** A^3 MUSS ein leeres lokales Git-Repository über einen nativen Ordnerdialog in einem existierenden leeren Verzeichnis anlegen können, ohne Pfad aus der WebView, ohne Initial-Commit und ohne Remote.

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
- **FR-036** Der Agent MUSS die nächsten kleinen Umsetzungsschritte strukturiert planen und als
  rollierenden, begrenzten Horizont materialisieren. Markdownabschnitte sind kein regulärer
  Planeingang. Offene globale Kriterien MÜSSEN nach dem letzten aktuellen Schritt im selben Run
  per dauerhaftem Ledger-/Journal-CAS zu weiterer Planung führen; Budgets und unveränderliche
  Versuche bleiben erhalten. Ein fachlicher Replan MUSS die Zerlegung tatsächlich ändern können.
- **FR-037** Der vollständige Originalauftrag MUSS pro Zielrevision unveränderlich im bestehenden
  Goal-Store erhalten bleiben: höchstens 64 KiB UTF-8, höchstens 33 geordnete Teile zu je 2048 Bytes,
  ohne getrennte Mehrbytezeichen. Übergröße MUSS vor Aufgabenstart erklärt abgewiesen werden;
  stille Kürzung und der Präsentationsverlauf als alleinige Aufgabenautorität sind unzulässig.
- **FR-038** Akzeptanzkriterien MÜSSEN an ursprüngliche Auftragsteile, konkrete Ergebnisse und
  passende Nachweisarten gebunden sein. Vor Done MUSS die vollständige aktuelle Abdeckung aus
  Originalauftrag, Kriterien, Ledger und Evidence geprüft werden. Ein beliebiger grüner Command,
  Diff oder Modelltext genügt nicht; fachfremde rote Checks MÜSSEN transparent bleiben.

### Modell und Kontext

- **FR-040** A^3 MUSS einen explizit konfigurierten lokalen Ollama-kompatiblen Modellserver verwenden können.
- **FR-041** A^3 MUSS Modellfähigkeiten in einem ModelProfile validieren.
- **FR-042** Jeder Modelljob MUSS begrenzten aktuellen Kontext verwenden; Ask- und Agentturns
  erhalten genau ihren neu kompilierten kleinen Phasenauftrag. Tatsächliche Schema-, System-,
  Thinking- und Protokollkosten MÜSSEN mitzählen; mindestens
  22 Prozent beziehungsweise das höhere konfigurierte Outputlimit bleiben reserviert.
- **FR-043** Jeder aktive Agentturn MUSS eine revisionsgebundene Teilsicht mit Teilziel, Kriterien,
  ursprünglichem Auftragsbezug, allen geltenden Constraints und Nicht-Zielen, relevanten offenen
  Fehlern und Verifikationsstatus enthalten. Eine begrenzte Projektion zeigt die globale Abdeckung;
  ausgelassene Auftragsteile bleiben dauerhaft offen. Nicht sicher ausgeschlossene Constraints
  bleiben vollständig aktiv. Pflichtinhalt, der nicht passt, erfordert Zerlegung oder Kapazitätshalt.
- **FR-044** Ungültige strukturierte Modellausgabe DARF nie ausgeführt werden.
- **FR-045** Der Indexbrowser MUSS ohne konfiguriertes Modell nutzbar sein.
- **FR-046** Das Modell MUSS fachlich planen und Code erzeugen können, ohne bekannte IDs, Hashes
  oder deterministische Zustandskanten auszuwählen. Quellenfortschritt MUSS tatsächliche neue
  aktuelle Bereiche erfassen; Readanzahl genügt nicht. NeedEvidence/TooLarge MUSS auch während
  Planung oder Edit ohne Mutation und ohne Strukturrepair darstellbar bleiben.

### Werkzeuge und Agent

- **FR-050** Der Agent MUSS über einen endlichen Controller mit dokumentierten Zuständen laufen.
- **FR-051** Pro Turn DARF höchstens eine Tool Action ausgeführt werden.
- **FR-052** A^3 MUSS Suche, gezielte Inspektion, Patchen, lokale Prozessausführung, Ledgerupdate und Finish anbieten.
- **FR-053** Pro Worktree DARF höchstens eine Mutation gleichzeitig laufen.
- **FR-054** Bereichsedits MÜSSEN an tatsächlich gelieferte aktuelle Bytes gebunden sein. Der Core
  MUSS den bestehenden vollständigen Patch kompilieren, alle Außenbytes samt BOM/EOL erhalten
  und erwarteten Snapshot sowie vollständigen Content Hash vor der Wirkung prüfen. Unscharfer
  Ersatz, Aliasfälschung und stilles Rebinding nach Fremdänderungen sind unzulässig.
- **FR-055** Prozesse MÜSSEN argv-basiert, begrenzt und abbrechbar sein.
- **FR-056** Nach einer Mutation MUSS der Indexzustand vor weiterer Modellarbeit abgeglichen werden.
- **FR-057** Done MUSS an aktuelle Muss-Akzeptanzkriterien und Evidence gebunden sein.
- **FR-058** Fehlt innerhalb eines Runs eine fachliche Richtungsentscheidung, MUSS A^3 die konkrete
  Frage als menschlichen Haltepunkt anzeigen; sie DARF weder als technischer Erfolg noch als
  zusätzliche Ausführungsberechtigung behandelt werden.

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
- **FR-075** Ask und Agent MÜSSEN ihren endlichen Arbeitsweg mit öffentlichen Befunden, Quellen
  und verständlicher aktueller Tätigkeit nachvollziehbar anzeigen; internes Chain-of-Thought bleibt
  ausgeschlossen. Planung ist ein sichtbarer interner Agentenschritt, kein eigener neuer Nutzermodus.
- **FR-076** Chat, Ask und Agent MÜSSEN für die nächste unabhängige Nachricht wählbar sein, ohne
  laufende Arbeit zu unterbrechen oder Capability-Grenzen zu verschieben. Chat ist global und
  funktioniert ohne Projekt; Ask und Agent benötigen ein im Core gebundenes geöffnetes Projekt.
- **FR-077** Während laufender Arbeit angenommene Folgenachrichten MÜSSEN begrenzt, dauerhaft und
  FIFO-geordnet sichtbar bleiben; Fehler, Abbruch und Neustart DÜRFEN sie nicht still starten oder
  verlieren.
- **FR-078** Ein neuer Agent-Auftrag MUSS seine aktuelle Ziel- und Quellenbindung im Core erhalten
  und innerhalb dieses Ziels ohne obligatorischen vollständigen Recherche-Handoff arbeiten können.
  Alte Entwicklungs-Plan-Sessions und ihre Queueeinträge müssen nicht im neuen Produkt lesbar
  bleiben und DÜRFEN weder als Agentaufträge noch als Ausführungsberechtigungen übernommen werden.
- **FR-079** Die operative Agentenlauf-Seitenleiste DARF erst nach Task-Materialisierung erscheinen;
  Recherche und Quellen erscheinen ausschließlich beim zugehörigen Chat-Turn.
- **FR-080** Agent MUSS auch in einem leeren Repository kleine passende Umsetzungsschritte
  beginnen können. Fehlende Originale in einem leeren Index DÜRFEN keine Fortsetzungsschleife
  erzwingen. Ask MUSS fehlende Projektbelege als Erkenntnisgrenze ausweisen.
- **FR-081** Allgemeiner Chat MUSS ohne Projektwerkzeuge und ohne Coding-JSON-Probe funktionieren.
  Auch ein geöffnetes Projekt wird nicht automatisch gelesen. Aktuelle Projektfragen werden an Ask
  verwiesen; eingefügter Code bleibt Gesprächsdaten. Sichtbare Chats werden getrennt von Katalog
  und Projektwissen mit bestehendem libSQL unter dem geprüften AppData-Root gehalten.

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
| Modi und kleiner Agentenkern | 22 | 0119; ADR-0117/0118 bleiben erhalten |

## V1-Abnahme

Eine Anforderung gilt nur als erfüllt, wenn ein automatisierter Test, ein reproduzierbarer Benchmark oder eine dokumentierte manuelle Plattformprüfung als Evidence verlinkt ist.

- Die gemeinsame Arbeitsfläche besitzt einen zentralen Composer und kontextuellen
  Fortschritts-/Änderungs-/Review-Inspector. Globale Chats und projektgebundene Ask-/Agent-Verläufe
  bleiben mit ihren jeweiligen Capability-Grenzen getrennt.
- `Chat` verarbeitet allgemeine Gespräche ohne Projektwerkzeuge; `Ask` beantwortet quellengebundene
  Projektfragen ohne Mutation, freie Prozesse oder Netzwerk; `Agent` plant und bearbeitet
  Projektaufträge über denselben endlichen sicheren Controller. Full machine erweitert weder Chat
  noch Ask. Neue implementierte Funktionen benötigen ihre eigenen Abnahmebelege.
- Nutzer sehen laufenden Status, aktuelle Tätigkeit, notwendige Freigaben, verifizierte Änderungen
  und den Abschlussreview und können danach im selben Kontext nachfragen oder eine neue Session
  beginnen.
- Die Modellabnahme MUSS echte lokale Modelle ab Nutzerauftrag verwenden und unabhängige
  Aufgabenkorrektheit, Quellenlieferung, Schemafähigkeit und Controller-Done getrennt ausweisen.
  Der feste positive 4k-Schnitt aus Plan 22 MUSS richtige Aufgabenlösungen erreichen;
  ein Kapazitätshalt zählt dort als Misserfolg. Infrastrukturkontrollen und größere Profile
  ersetzen diesen Beleg nicht.
