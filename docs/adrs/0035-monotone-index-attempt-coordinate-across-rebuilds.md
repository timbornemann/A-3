# ADR-0035: Monotone Index-Laufkoordinate über Rebuilds

Status: Accepted  
Datum: 2026-08-29  
Entscheider: Tim Bornemann

## Kontext

Ein expliziter Fast-Index-Rebuild entfernt vertragsgemäß die regenerierbaren Projektionen und
die zugehörigen `index_runs`. Bleibt der beobachtete Dateizustand unverändert, verwendet der
anschließende Full Rescan denselben Snapshot und dieselbe Ranking-Policy. Weil die gelöschte
Run-Historie bisher zugleich als einziger Laufzähler dient, beginnt der neue Lauf wieder mit
Sequenz eins. Die deterministische `IndexRunId` wird dadurch bytegleich zur gelöschten Publikation.

Dauerhafte Module Cards behalten absichtlich ihren Evidence-Anker über einen Index-Rebuild. Der
wiederverwendete Anker lässt diese historischen Cards deshalb erneut als `Current` erscheinen.
Deep Map darf gegen einen Current-Anker nach ADR-0034 keine zweite Publikation starten. Damit
verletzt der Rebuild das ebenfalls in ADR-0034 festgelegte Produktverhalten, nach dem ein neuer
Fast Index ohne passende Cards wieder `Ready` wird.

Die Korrektur darf weder Snapshots ohne neue Beobachtung erfinden noch Module Cards überschreiben
oder die deterministische Run-Identität durch Zeit beziehungsweise Zufall ersetzen.

## Entscheidung

- A^3 speichert pro Worktree eine positive monotone Index-Laufkoordinate als kleine dauerhafte
  Identitätsmetadaten getrennt von den regenerierbaren `index_runs` und Indexprojektionen.
- Die Vorwärtsmigration initialisiert die Koordinate mit der höchsten vorhandenen Run-Sequenz.
  Ein leerer neuer Worktree beginnt weiterhin bei eins.
- Ein expliziter Rebuild entfernt weiterhin die regenerierbaren Indexdaten und ihre Run-Zeilen,
  erhält aber die zuletzt vergebene Laufkoordinate. Der anschließende Full Rescan erhält deshalb
  auch bei unverändertem Snapshot und unveränderter Ranking-Policy eine neue `IndexRunId`.
- Der Application-Core liest die exakt nächste Koordinate über einen schmalen Storage-Port,
  leitet daraus deterministisch die Run-ID ab und übergibt beides als typisierten
  `IndexRunStart`. Der Storageadapter akzeptiert den Start nur, wenn die erwartete Koordinate
  exakt auf den dauerhaften High-Water-Mark folgt; Fortschreiben des Markers und Einfügen des
  Building-Runs erfolgen atomar in derselben Transaktion.
- Ein konkurrierend veralteter Start bleibt ein typisierter Sequenzkonflikt und wird niemals mit
  einer anderen als der in die Run-ID eingegangenen Koordinate gespeichert. Das bestehende
  worktree-lokale Mutationslease verhindert diesen Konflikt im regulären Ablauf.
- Nach einem Rebuild dürfen die aktuell retained Run-Zeilen bei einer Sequenz größer eins beginnen.
  Integritätsprüfungen verlangen eine lückenlose retained Sequenz, den exakten High-Water-Mark und
  eine eindeutige positive Reihenfolge, nicht mehr `COUNT(*) = MAX(run_sequence)`.
- Die Laufkoordinate bleibt Core-intern. Die WebView kann sie weder lesen noch wählen. Ein
  Überlauf beendet den Start vor Indexarbeit als typisierter Fehler.

ADR-0035 ergänzt ADR-0034 und supersediert keine dortige Entscheidung. Insbesondere bleibt genau
eine unveränderliche Module-Card-Publikation pro konkreter `IndexRunId` erlaubt.

## Konsequenzen

### Positiv

- Ein angeforderter Fast-Index-Rebuild erzeugt auch ohne Dateiänderung einen wirklich neuen
  Publikationsanker und setzt Deep Map anschließend auf `Ready`.
- Historische Module Cards und Claims bleiben unverändert evidence-gebunden, statt still
  überschrieben oder einem erfundenen Snapshot zugeordnet zu werden.
- Run-IDs bleiben für identische Projekt-, Snapshot-, Policy- und Laufkoordinaten
  deterministisch.

### Negativ

- Das Knowledge-Schema und der `KnowledgeIndexStore`-Vertrag erhalten eine weitere kleine
  Identitätsprojektion.
- Gültige retained Run-Sequenzen müssen nach einem Rebuild nicht mehr bei eins beginnen; Leser und
  Tests dürfen die bisherige kompakte Sequenzannahme nicht fortschreiben.

### Risiken und Gegenmaßnahmen

- Marker und Run werden getrennt sichtbar — beide Mutationen erfolgen in derselben Immediate-
  Transaktion; Fehler und Cancellation rollen sie gemeinsam zurück.
- Ein alter Core startet mit einer stale Koordinate — der Storage-CAS lehnt sie typisiert ab und
  verändert weder Marker noch Run-Zeilen.
- Die Metadaten werden fälschlich als regenerierbar gelöscht — Rebuild-Contracts prüfen ihren
  Erhalt vor und nach Reopen.

## Verworfene Alternativen

- Zweite Deep-Map-Publikation gegen dieselbe Run-ID zulassen — verletzt die unveränderliche
  Evidence-Bindung aus ADR-0034.
- Bei jedem Rebuild einen Snapshot ohne neue Dateibeobachtung erzeugen — erfindet eine
  Worktree-Generation und widerspricht dem Snapshot-Domänenmodell.
- Zeit, Zufall oder eine flüchtige Prozessgeneration in die Run-ID aufnehmen — ist weder
  reproduzierbar noch nach Neustart zuverlässig.
- Alle historischen `index_runs` beim Rebuild behalten — erweitert die Retention wesentlich und
  vermischt regenerierbare Laufzeilen mit dem einzigen benötigten High-Water-Mark.

## Compliance

- Eine V28-Migration prüft Backfill, Vorwärtsupgrade von V27 und atomaren Rollback.
- Storage-Contracts prüfen Sequenz eins beim ersten Lauf, Rebuild-Erhalt, Sequenz zwei beim
  unveränderten Rebuild, Reopen, stale Start-CAS und Überlauf.
- Der vollständige Fast-Index-Contract beweist, dass Snapshot und Ranking-Policy gleich bleiben,
  die zweite `IndexRunId` aber verschieden ist.
- Eine Deep-Map-Regression publiziert Cards für den ersten Lauf, baut den unveränderten Fast Index
  neu auf und erwartet danach `Ready`, bevor Planner oder Provider gestartet werden.

## Referenzen

- [ADR-0006](0006-deterministic-index-before-llm.md)
- [ADR-0034](0034-deep-map-run-journal-and-current-index-lifecycle.md)
- [Domänenmodell](../DOMAIN_MODEL.md)
- [Indexierung und Project Map](../INDEXING_AND_PROJECT_MAP.md)
- [Daten und Persistenz](../DATA_AND_PERSISTENCE.md)
