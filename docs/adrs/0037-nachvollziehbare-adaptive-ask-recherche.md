# ADR-0037: Nachvollziehbare adaptive Ask-Recherche

Status: Accepted

Datum: 2026-09-03

Entscheider: Tim Bornemann

Ergänzt: ADR-0030 und ADR-0033. Deren sichere Source-Vorschau, Capability-Envelopes und
Sessionautorität bleiben unverändert gültig.

## Kontext

Ask verwendete bisher eine intern angehängte, auf wenige Dateianfänge begrenzte Task Lens und
genau einen unstrukturierten Modellturn. Nutzer konnten weder erkennen, welche aktuelle Evidence
ausgewählt wurde, noch konkrete Repositoryfragen wie eine TODO-/FIXME-Suche zuverlässig stellen.
Eine bloße Anzeige von Modelltext oder internem Chain-of-Thought wäre weder überprüfbar noch mit
den Sicherheits- und Evidence-Regeln von A^3 vereinbar.

## Entscheidung

- Jeder Ask-Turn bindet sich vor dem ersten Modellaufruf an genau einen veröffentlichten Indexrun
  und Snapshot. Explizite `@pfad`-Nennungen werden zuerst gegen diesen unveränderlichen Anker
  aufgelöst; auch eine zusätzliche Task-Lens-Suche verwendet denselben Anker.
- Task Lens bietet eine kompatible Trace-Projektion. Sie erhält den bisherigen Lens-Inhalt
  unverändert und ergänzt ausschließlich Kanäle, Kandidaten- und Auswahlzahlen, Begrenzung sowie
  geschlossene Auswahlgründe. Source-Inhalte und Rankingwerte gehören nicht in den Trace.
- Symbol- und Span-Treffer werden an ihrem aktuellen Deklarations- beziehungsweise Evidence-Bereich
  gelesen. Semantische Kandidaten werden erst nach einem erfolgreichen hashgebundenen Source-Read
  zitierbare Evidence.
- Der Ask-Modellvertrag ist strikt versioniert. Ein Turn liefert entweder eine fertige Antwort mit
  turnlokalen Quellenreferenzen oder höchstens vier Read-only-Aktionen aus `searchIndex`,
  `searchSourceText`, `inspectPath` und `inspectSource`. Es folgt höchstens eine Aktionsrunde und
  über den gesamten Turn höchstens ein Reparaturversuch. Keine Aktion darf mutieren, Prozesse oder
  Shell starten, das Netzwerk verwenden oder WebView-Dateizugriff verleihen.
- `searchSourceText` akzeptiert höchstens acht case-insensitive Literale und endet nach 100 Treffern,
  2.000 aktuellen Indexdateien, 32 MiB geprüftem Text oder 30 Sekunden. Der vorhandene sichere
  Reader schließt Binary-, Generated-, Secret-, zu große und nicht aktuelle Inhalte aus. Eine
  begrenzte Suche darf niemals als vollständiger Negativnachweis dargestellt werden.
- Der Modellkontext wird mit der konservativen Zählstrategie des verifizierten Modellprofils
  begrenzt. Die aktuelle Frage und aktuelle Evidence werden auch bei Kürzung erhalten; ältere
  Assistentenantworten sind kein Beweis.
- Knowledge-Schema V30 speichert append-only Ask-Turns, inhaltsfreie Rechercheereignisse,
  hashgebundene Evidence-Referenzen und Antwortzitate. Antwort, terminales Ereignis und Zitate
  werden atomar zusammen mit der nächsten Sessionrevision gespeichert. Nicht gespeichert werden
  Quelltext, Prompts, Rohantworten, Chain-of-Thought, Providerdaten, Budgets oder Rankingwerte.
- Presentation Delete entfernt über denselben Sessioneintrag auch diese Präsentationsdaten; Archivieren
  erhält sie. Alte Sessions werden nicht rückwirkend angereichert und zeigen einen ausdrücklichen
  Hinweis auf den damals nicht aufgezeichneten Rechercheweg.
- Vier projektgebundene V1-Reads liefern höchstens 32 Turns, einen höchstens 64 Ereignisse großen
  Rechercheweg, höchstens 50 Quellen pro Cursor-Seite und eine bestehende sichere Source-Vorschau
  mit maximal 64 Zeilen beziehungsweise 16 KiB. Requests akzeptieren Session, Turnsequenz und
  opake Cursor beziehungsweise Source-Referenzen, aber keine Pfade, Evidence-, Snapshot-, Index-
  oder Provider-IDs. Cursor sind an Worktree, Session, Turn, Trace-Revision und aktuellen Index
  gebunden.
- Nach einem Indexwechsel bleiben die inhaltsfreien historischen Metadaten sichtbar. Source-
  Vorschauen werden gesperrt und die Antwort als älterer Projektstand gekennzeichnet.

## Konsequenzen

### Positiv

- Ein einfacher Ask-Turn bleibt bei einem Modellaufruf; nur fehlende Evidence verursacht genau
  eine zusätzliche kontrollierte Suchrunde.
- Nutzer können live und nach Abschluss nachvollziehen, wonach gesucht, warum ein Ziel ausgewählt
  und welche Quelle tatsächlich für die Antwort verwendet wurde.
- Konkrete TODO-/FIXME-Fragen betrachten aktuellen sicher lesbaren Repositoryinhalt statt nur
  weniger Dateianfänge.
- Quellen bleiben prüfbar, ohne Quelltext dauerhaft zu duplizieren oder der WebView einen freien
  Dateizugriff zu geben.

### Negativ

- Ein Ask-Turn benötigt zusätzliche kleine Persistenzwrites und kann bei fehlender Evidence einen
  zweiten Modellaufruf sowie begrenzte Repositoryreads ausführen.
- Historische Metadaten sind nach einem Indexwechsel weiter lesbar, ihr Quelltext aber bewusst
  nicht mehr aufrufbar.

### Risiken und Gegenmaßnahmen

- Das Modell erfindet eine Quelle — nur zuvor vom Core ausgegebene turnlokale Referenzen werden
  akzeptiert; unbekannte oder doppelte Referenzen verwerfen die strukturierte Ausgabe.
- Ein Indexwechsel mischt Projektstände — alle Reads und Aktionen verwenden den zu Turnbeginn
  gebundenen Index; Source-Vorschau und Cursor werden bei einem heutigen Ankerwechsel gesperrt.
- Repositorysuche blockiert den Desktop — Datei-, Byte-, Treffer- und Zeitgrenzen sowie kooperative
  Cancellation begrenzen jeden Read; der Scheduler bleibt Eigentümer des Jobs.
- Die Fortschrittsansicht wird zum Chain-of-Thought-Leak — sie zeigt ausschließlich geschlossene
  Core-Phasen, sichere Aktionen, Auswahlgründe und Evidence-Metadaten.

## Verworfene Alternativen

- Jeden Ask-Turn immer zweimal ausführen — erhöht Latenz und Kosten ohne Evidenzbedarf.
- Unbegrenzte Repositorysuche oder freie Modelltools — verletzt Resource- und Capability-Grenzen.
- Prompts, Rohantworten oder Quelltext für spätere Transparenz speichern — schafft neue sensible
  Wahrheitsquellen und widerspricht der lokalen Evidence-Architektur.
- Nur generische Texte wie „Sammelt Informationen“ anzeigen — macht Auswahl und Antwortquellen
  weiterhin nicht überprüfbar.

## Compliance

- Migrationstests prüfen Neuinstallation, V29→V30, leere historische Traces und atomaren Rollback.
- Storagetests prüfen atomaren Antwort-/Zitatabschluss sowie gemeinsames Presentation Delete.
- Decoder-, Laufzeit- und Source-Search-Tests prüfen strikte Actions, genau eine Reparaturgrenze,
  aktuelle Treffer außerhalb eines Dateipräfixes und ehrliche Begrenzung.
- IPC-, TypeScript- und Capabilitytests lehnen unbekannte Felder, Pfade und interne Anker ab.
- Component-Tests prüfen Live-Status, persistenten Rechercheweg, verwendete und zusätzlich
  bereitgestellte Quellen, begrenzte Suche, stale Vorschau und alte Sessions ohne Trace.

## Referenzen

- [ADR-0007](0007-evidence-graph-and-hybrid-retrieval.md)
- [ADR-0009](0009-context-compiler.md)
- [ADR-0012](0012-safe-tools-and-approval-policy.md)
- [ADR-0020](0020-agent-runtime-ownership-and-pause.md)
- [ADR-0025](0025-bounded-desktop-rendering-and-lifecycle.md)
- [ADR-0030](0030-bounded-evidence-source-preview.md)
- [ADR-0033](0033-chatbasierter-agent-workspace.md)
- [Daten und Persistenz](../DATA_AND_PERSISTENCE.md)
- [IPC-Protokoll](../IPC_PROTOCOL.md)
- [Plan 06](../plans/06-DESKTOP_PRODUCT.md)
