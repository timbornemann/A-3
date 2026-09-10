# ADR-0108: Dauerhafter, begrenzter Fast-Index-Laufinspektor

Status: Accepted

Datum: 2026-09-10

Entscheider: Tim Bornemann

## Kontext

Der globale Fast-Index-Status zeigt bisher nur Zustand, aktuelle Phase und `completed/6`. Ein langer
oder nach einem Prozessabbruch hinterlassener Lauf ist damit nicht diagnostizierbar: Weder aktuelle
Datei und Unterfortschritt noch sichere Fehlerursache, betroffene Dateien oder letzte Aktivität sind
sichtbar. Der bestehende atomare Publish schützt zwar den vorherigen Index, liefert aber kein
dauerhaftes, vor Discovery beginnendes Diagnoseobjekt.

Repositoryinhalt, absolute Pfade, Adapterfehler und Secrets dürfen durch eine Diagnoseoberfläche
nicht zusätzlich an die unprivilegierte WebView gelangen. Die Beobachtbarkeit darf außerdem weder
pro Datei eine Transaktion auslösen noch einen ansonsten gültigen Publish von einem Journalfehler
abhängig machen.

## Entscheidung

- Ein `IndexTraceId` identifiziert einen Diagnose-Trace bereits vor Discovery und unabhängig von
  der späteren `IndexRunId`. Der Domainvertrag enthält monotone Revisionen, die Zustände `Queued`,
  `Running`, `Cancelling`, `Succeeded`, `Failed`, `Cancelled` und `Interrupted`, vier geschlossene
  Auslöser sowie die sechs geordneten Phasen Discover, Hash, Parse, Link, Rank und Publish.
- Discovery, Hashing, Parsing, Linking, Ranking und Publish melden über einen Application-Observer
  getrennte Phasen-, Unterfortschritts-, Datei- und Diagnoseereignisse. Dateiresultate unterscheiden
  neu/geändert/unverändert/gelöscht, gehasht/wiederverwendet und strukturell/
  wiederverwendet/generisch/fehlgeschlagen. Vom Discovery-Contract ausgeschlossene Secret-,
  Binary-, Vendor- und Generated-Pfade gelangen nie in den Trace; erkannte Löschungen schon.
- Knowledge-Schema V39 hält Laufzusammenfassung, exakt sechs Phasen, typisierte bounded Events,
  vollständige berücksichtigte Dateizeilen und je Datei höchstens acht sichere Diagnosecodes. Es
  gibt absichtlich keinen Fremdschlüssel von Traces auf regenerierbare Indexruns oder Snapshots.
- Journalupdates werden als kurze Upsert-Batches geschrieben. Ein Journalfehler bricht den Index
  nicht ab, sondern setzt den sichtbaren Marker `detailsIncomplete`. Ein Index-Rebuild löscht diese
  Diagnosedaten nicht.
- Retention hält während eines aktiven Laufs diesen und genau den vorherigen terminalen Lauf. Nach
  Abschluss bleibt nur der neueste terminale Lauf. Beim Projektstart werden hinterlassene aktive
  Traces als `Interrupted` und verwaiste `building`-Indexruns als fehlgeschlagen abgeschlossen.
- `query_index_activity` V1 bleibt der leichte 500-ms-In-Memory-Read. Drei neue strikt versionierte
  Commands liefern die retained Detailprojektion, eine serverseitig gesuchte/gefilterte Dateiseite
  mit höchstens 100 Einträgen und revisionsgebundene Cancel-/Retry-Aktionen. Cursor sind opak und an
  Worktree, Trace, Revision, Suche, Filter und Offset gebunden.
- Die WebView erhält nur kontrollzeichenfreie, auf 512 Zeichen begrenzte repository-relative
  Anzeigen, IDs, Zähler, Zeitwerte und geschlossene Codes mit lokalisierter Erklärung und Recovery.
  Sie erhält weder absolute Pfade noch Quelltext, Rohfehler, Secrets oder Datenbankzugriff.
- Nach 60 Sekunden ohne neue Trace-Revision zeigt das geöffnete Modal eine Stillstandswarnung und
  bietet den kooperativen Abbruch an. Es gibt keine automatische Zwangsbeendigung. Retry startet
  einen vollständigen Rescan ohne Rebuild; der bisher veröffentlichte Index bleibt bis zu einem
  erfolgreichen neuen Publish sichtbar.
- Nur das geöffnete Modal pollt die Detailprojektion, ohne überlappende Reads. Dateiseiten werden
  bei einer neuen Revision erneut vom ersten, revisionsgebundenen Cursor gelesen.

## Folgen

### Positiv

- Aktive, fehlgeschlagene und durch einen Prozessabbruch unterbrochene Läufe sind lokal mit Phase,
  Datei, Fortschritt, Ereignissen und sicherer Recovery nachvollziehbar.
- Diagnosepersistenz und Indexpublish bleiben getrennte Fehlerdomänen; ein defektes Journal kann
  keinen gültigen Index verhindern und ein Retry zerstört keinen nutzbaren Snapshot.
- Retention und feste Seiten-/Textgrenzen halten Speicher-, IPC- und Renderingkosten vorhersagbar.

### Negativ

- Die Diagnose ist absichtlich nur auf dem jeweiligen Rechner sichtbar und ersetzt keinen
  PC-übergreifenden Supportexport.
- Einzelne Ereignis- und Diagnosefolgen sind begrenzt; Kürzungen werden sichtbar markiert.
- Die vollständige Dateimenge benötigt zusätzliche lokale Zeilen und bounded Batchwrites.

## Verworfene Alternativen

- Ausschließlich ausführlichere Statusleisten-Texte — bietet keine Dateien, Timeline, Recovery oder
  vorherigen Lauf und überlastet die globale Statusprojektion.
- Rohe Logs oder Adapterfehler an die WebView — würden Trust Boundary, Secret- und Pfadregeln
  verletzen.
- Trace erst mit der `IndexRunId` beginnen — verliert gerade Discovery- und frühe Hashfehler.
- Unbegrenzte Historie oder Export — erhöht Retention-, Datenschutz- und Supportumfang ohne Bedarf
  dieses Schnitts.
- Automatische Beendigung nach 60 Sekunden — kann legitime lokale Arbeit zerstören; Stillstand ist
  ein Warnsignal, kein Beweis.

## Compliance

- Domain-/Applicationtests prüfen Zustandsmonotonie, Phasenreihenfolge, Dateiresultate, Löschungen,
  sichere Codes und `detailsIncomplete`.
- Storage-Contracts prüfen V38→V39, Rollback, Worktree-Isolation, Reopen, Retention, Suche/
  Pagination, Rebuild-Erhalt und Startup-Reconciliation.
- Manager- und IPC-Tests prüfen kooperative Cancellation, vollständigen Retry ohne Indexlöschung,
  stale Revisionen, manipulierte Cursor, Bounds und sicheren Worker-Fallback.
- Frontendtests prüfen Button-/Tastaturzugang, Fokus, beide retained Läufe, sechs Phasen,
  Dateipagination, Fehler-Recovery und die 60-Sekunden-Warnung.

## Nicht-Ziele

- Cloud- oder PC-übergreifende Diagnose und Exportformat
- Quelltext oder rohe Adapterfehler im Journal
- unbeschränkte Laufhistorie oder automatische Zwangsbeendigung
- Änderungen an Deep Map

## Referenzen

- [ADR-0004](0004-libsql-local-persistence.md)
- [ADR-0012](0012-safe-tools-and-approval-policy.md)
- [ADR-0020](0020-agent-runtime-ownership-and-pause.md)
- [Architekturregeln](../ARCHITECTURE_RULES.md)
- [Daten und Persistenz](../DATA_AND_PERSISTENCE.md)
- [Job Runtime](../JOB_RUNTIME.md)
- [IPC-Protokoll](../IPC_PROTOCOL.md)
- [Qualitätsgates](../QUALITY_GATES.md)
