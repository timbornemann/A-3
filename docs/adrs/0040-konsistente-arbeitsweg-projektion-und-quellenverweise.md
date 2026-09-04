# ADR-0040: Konsistente Arbeitsweg-Projektion und Quellenverweise

Status: Accepted

Datum: 2026-09-04

Entscheider: Tim Bornemann

Ergänzt: ADR-0037 und ADR-0038. Die dort festgelegten Recherche-, Evidence-, Preview- und
Persistenzgrenzen bleiben unverändert gültig.

## Kontext

Die Detailansicht lud Ereignisse und Quellen über getrennte Reads. Während eines laufenden Turns
konnte ein neuer Eventstand dadurch einen gerade ausgegebenen Source-Cursor ungültig machen. Die
Oberfläche behielt dann zwar die Ereignisse, ersetzte aber die Quellen durch eine leere Liste und
behauptete fälschlich, es gebe keine zitierbare Quelle. Beim Übergang von Live-Recherche zur
Antwort wurde außerdem eine zweite Komponenteninstanz erzeugt. Progressive Schritte und
Disclosure-Zustand begannen dadurch sichtbar von vorn.

Reine Marker wie `【S1】` waren zwar turnlokal, in der Antwort aber nicht unmittelbar einer Datei
und einem Zeilenbereich zuzuordnen. Eine Zuordnung durch die WebView oder durch geratenen
historischen Kontext wäre mit der Core-Autorität nicht vereinbar.

## Entscheidung

- Der neueste Nutzerturn besitzt im Conversation-Rendering genau eine langlebige
  Arbeitsweg-Komponenteninstanz. Die Antwort wird vor ihr ergänzt; der Live→Antwort-Übergang
  ersetzt oder remountet sie nicht. Der kompakte Inspector spiegelt dieselbe geladene Projektion
  und denselben progressiven Anzeigestand, statt einen zweiten Polling- und Timerzyklus zu starten.
- Die sichtbare Timeline wird deterministisch in `Vorbereitung`, nummerierte
  `Recherche-Runde N` und `Abschluss` gruppiert. Aufeinanderfolgende technische Start- und
  Ergebnisevents derselben Vorbereitungs- oder Lokalisierungsphase bilden einen logischen Punkt.
  Unterschiedliche Reads bleiben einzeln sichtbar.
- Der sichtbare Kasten „Aktueller Stand“ entfällt. Genau eine visuell verborgene höfliche
  Live-Region meldet nur die aktuelle Aktion. Nach einem erfolgreichen Abschluss klappt die zuvor
  live geöffnete Ansicht einmalig 700 ms nach sichtbarer Antwort und sichtbarem Terminalpunkt ein.
  Fehler, Abbruch und Fortsetzungsbedarf bleiben geöffnet; manuelle Bedienung hat anschließend
  Vorrang.
- `query_agent_work_trace_projection` V1 liest Detail, Zitate und erste Source-Page in einer
  gemeinsamen lokalen Read-Transaktion. Der Desktop revalidiert zusätzlich Eventrevision,
  Quellenzahl und Indexanker vor Ausgabe. Bei einer parallelen Änderung lautet das geschlossene
  Ergebnis `updating`.
- Weitere Pages laufen über `query_agent_work_trace_sources_v2`. Projektionsreferenz und Cursor
  sind opak an Worktree, Session, Turn, Eventrevision, Quellenzahl und aktuellen Index gebunden.
  Eine Abweichung liefert `projectionChanged`; ältere V1-Reads bleiben kompatibel.
- Jede Source erhält das öffentliche Label `S1` bis `S200`, deterministisch aus ihrer bereits
  persistierten turnlokalen Ordinalzahl. Die Ordinalzahl selbst und interne Source-IDs werden nicht
  ausgegeben.
- Neue Antworten werden nur veröffentlicht, wenn die Menge der exakten Marker `【S<n>】` außerhalb
  von Codeblöcken und Inline-Code exakt den strukturierten `source_refs` entspricht. Eine
  Abweichung nutzt den bestehenden einzigen Repair-Versuch und kann danach keine korrekt belegte
  Antwort vortäuschen.
- Die WebView ergänzt gültige Marker ausschließlich anhand der Core-Projektion um Dateiname und
  Zeilenbereich. Ein Klick öffnet dieselbe Research-Ansicht, wählt die Quelle und verwendet die
  bestehende sichere Preview. Unbekannte historische Marker bleiben Text.
- Die Quellenansicht trennt „Für die Antwort verwendet“ und „Zusätzlich gefunden“. Ein leerer
  Zustand ist nur nach erfolgreichem Projektionsread mit `sourceCount = 0` zulässig. Bei bekannten
  Zählern und fehlenden Details bleibt die letzte vollständige Projektion sichtbar und es erscheint
  eine konkrete Retry-Meldung.

## Konsequenzen

- Der Übergang zwischen Recherche-Runden und Antwort bleibt ruhig; bereits sichtbare Schritte,
  Auswahl und Vorschau flackern nicht.
- Antwort und Quellenblock verwenden dieselbe Core-eigene S-Zuordnung. Nutzer können jede
  tatsächlich verwendete Quelle direkt aus dem Antworttext prüfen.
- Eine zusätzliche opake Projektionserkennung und ein Revalidierungsread sind nötig. Das
  Knowledge-Schema ändert sich nicht; V30 bis V32 bleiben ohne Backfill lesbar.
- Rohes Chain-of-Thought, Prompts, Modelltranskripte, Quelltextpersistenz und neue Dateirechte
  entstehen nicht.

## Compliance

- Storage- und IPC-Tests prüfen Snapshotread, Projektionsbindung, Paging und den Ausschluss
  interner Identitäten.
- Decoder-Tests prüfen passende, fehlende, unbekannte und in Code inert bleibende Marker.
- Frontendtests prüfen gruppierte Runden, zusammengeführte Vorbereitung, einmaligen Collapse,
  Reduced Motion, den 12/3-Quellenfehler, getrennte Listen und klickbare Dateizeilenangaben.

## Referenzen

- [ADR-0030](0030-bounded-evidence-source-preview.md)
- [ADR-0037](0037-nachvollziehbare-adaptive-ask-recherche.md)
- [ADR-0038](0038-agentische-mehr-runden-recherche.md)
- [ADR-0039](0039-evidenzgebundene-slash-commands.md)
- [Daten und Persistenz](../DATA_AND_PERSISTENCE.md)
- [IPC-Protokoll](../IPC_PROTOCOL.md)
- [Plan 06](../plans/06-DESKTOP_PRODUCT.md)
