# ADR-0034: Deep-Map-Laufjournal und Current-Index-Lifecycle

Status: Accepted

Datum: 2026-08-29

Entscheider: Tim Bornemann

Supersedes: ausschließlich die flüchtige Retention der letzten 32 Deep-Map-Aktivitäten aus
ADR-0031; alle übrigen Entscheidungen aus ADR-0031 bleiben unverändert gültig.

## Kontext

Eine verifizierte Module-Card-Publikation ist pro Fast-Index absichtlich unveränderlich. Der
bisherige Deep-Map-Start prüft diesen Zustand jedoch nicht vor Planung und Modellarbeit. Ein
erneuter Lauf gegen denselben Index verursacht deshalb unnötige Providerarbeit und endet erst am
atomaren Commit mit dem unspezifischen Diagnosecode `publication`. Zusätzlich verliert die
WebView ihren auf 32 Einträge begrenzten Aktivitätsfeed bei Neustart und kann einen fehlgeschlagenen
Schritt nicht dauerhaft und sicher erklären.

## Entscheidung

- Der Core liest vor jeder neuen Planung über einen read-only
  `DeepMapPublicationStateStore` eine konsistente Projektion der jüngsten Indexpublikation, Module
  Cards, Card-FTS-Einträge und des Projektionsmarkers. Eine vollständig veröffentlichte Projektion
  ist `Current`; eine neue Publikation ohne Cards ist `Ready`; widersprüchliche Projektionen sind
  ein Storage-Vertragsfehler.
- `Current` beendet einen Start vor Planner und Provider als `AlreadyCurrent`. Erkennt erst der
  atomare Publisher eine gleichzeitige vollständige Publikation desselben Indexes, wird diese als
  `AlreadyPublished` klassifiziert und nach erneuter Current-Prüfung ebenfalls erfolgreich
  beendet. Ein stale Index oder ein unvollständiger beziehungsweise widersprüchlicher Batch bleibt
  ein Fehler.
- Deep-Map-Läufe, nummerierte Planschritte und sichere Ereignisse werden lokal in
  `deep_map_runs`, `deep_map_steps` und dem append-only `deep_map_events`-Journal gespeichert.
  Run- und Step-Projektionen werden mit dem zugehörigen Ereignis transaktional fortgeschrieben.
  Beim Öffnen eines Projekts werden nicht terminale Läufe als `interrupted` reconciled.
- Das Journal enthält ausschließlich geschlossene Zustände, sichere Zeit- und Mengenwerte,
  opake Core-IDs, Provider-/Modell-IDs, Profile-Versionen und Diagnosecodes. Prompts,
  Modellantworten, Chain-of-Thought, Provider-Payloads, Source-Inhalt, Credentials und rohe
  Fehlertexte werden weder gespeichert noch ausgegeben. Reservierte Budgets sind keine
  Behauptung über tatsächlichen Tokenverbrauch.
- Journalfehler steuern weder Modellarbeit noch Veröffentlichung. Der Lauf setzt seine eigentliche
  Arbeit fort und markiert seine Detailprojektion als unvollständig.
- Die WebView erhält eine kompakte, diskriminierte V3-Lifecycle-Projektion ohne Ereignisfeed.
  Getrennte V1-Reads liefern höchstens 20 Läufe beziehungsweise 50 Einträge je Seite und genau ein
  ausgewähltes Detail. Selektionen und Cursor werden vom Core ausgegeben und bei jedem Read erneut
  an Projekt und Publikation gebunden.
- Ein neuer Fast-Index macht die vorherige Deep-Map-Publikation nicht aktuell und führt den
  Lifecycle wieder nach `Ready`. Bereits vorhandene Cards ohne Journal bleiben `Current`; die UI
  kennzeichnet, dass für die historische Publikation keine Einzelschritte vorliegen.

## Konsequenzen

### Positiv

- Wiederholte Starts desselben Indexes verursachen keine Modellaufrufe oder Modellkosten.
- Publication-Rennen sind idempotent, ohne stale oder inkonsistente Publikationen zu akzeptieren.
- Fehler und einzelne Schritte bleiben nach Neustarts nachvollziehbar, ohne sensible Inhalte in
  Persistenz oder WebView zu übernehmen.
- Die schmale Deep-Map-Leiste benötigt keinen ungebundenen Live-Feed; der gemeinsame Inspector
  lädt nur die gerade benötigten begrenzten Seiten.

### Negativ

- Das Knowledge-Schema und die Zahl der versionierten Desktop-Reads wachsen.
- Journal und materialisierte Projektionen müssen transaktional und bei Teilverfügbarkeit
  konservativ behandelt werden.
- Tatsächlicher Provider-Tokenverbrauch bleibt unsichtbar, bis alle Provider ihn zuverlässig über
  die providerneutrale Schnittstelle liefern.

### Risiken und Gegenmaßnahmen

- Eine kompromittierte WebView erfindet Run-IDs oder Cursor — der Core akzeptiert nur
  Core-ausgegebene Selektionen und bindet sie erneut an das aktive Projekt.
- Ein Journal wächst unbegrenzt in der Darstellung — Persistenz bleibt vollständig, Reads sind
  aber fest auf 20 Läufe und 50 Einträge begrenzt und cursorpaginiert.
- Ein Journalfehler stoppt die Fachoperation — Journalwrites sind best effort; die sichtbare
  `detailsIncomplete`-Markierung verhindert eine falsche Vollständigkeitsbehauptung.
- Eine vorhandene Teilpublikation wird als aktuell fehlinterpretiert — Current verlangt
  übereinstimmende Card-, FTS- und Projektionszähler in derselben Read-Transaktion.

## Verworfene Alternativen

- Zweite Publikationen desselben Indexes überschreiben — widerspricht der unveränderlichen,
  evidence-gebundenen Card-Publikation.
- Erst am Commit auf Duplikate reagieren — verhindert weder Providerkosten noch unnötige Arbeit.
- Prompts oder rohe Providerfehler für Diagnose speichern — verletzt die lokale Trust-Grenze und
  ist für eine sichere Bedienerdiagnose nicht erforderlich.
- Den bisherigen 32er-Ring lediglich vergrößern — bleibt nach Neustart verloren und erlaubt keine
  projektgebundene Pagination.

## Compliance

- Storage-Tests prüfen erste Publikation, `Current` nach Reopen, konsistente Zähler,
  `AlreadyPublished`, monotone Ereignisse und Reconciliation.
- Application-Tests beweisen, dass Planner, Provider und Publisher bei `Current` nicht aufgerufen
  werden, sowie die getrennten Publication-Fehlerklassen.
- Protokolltests verwerfen unbekannte Felder, fremde oder erfundene Selektionen, stale Cursor und
  übergroße Seiten.
- UI- und Browsertests prüfen die drei festen Modi, zustandsabhängige Aktionen, anklickbare Fehler,
  den gemeinsamen Inspector, Tastaturzugang und begrenzte DOM-Größe.

## Referenzen

- [ADR-0013](0013-goal-contract-ledger-and-event-journal.md)
- [ADR-0025](0025-bounded-desktop-rendering-and-lifecycle.md)
- [ADR-0031](0031-progressive-code-atlas-index-evidence.md)
- [ADR-0032](0032-openai-model-provider.md)
- [Architekturregeln](../ARCHITECTURE_RULES.md)
- [Daten und Persistenz](../DATA_AND_PERSISTENCE.md)
- [IPC-Protokoll](../IPC_PROTOCOL.md)

