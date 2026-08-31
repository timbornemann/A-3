# ADR-0036: Nutzerorientiertes Deep-Map-Laufdashboard

Status: Accepted

Datum: 2026-08-31

Entscheider: Tim Bornemann

Ergänzt: ADR-0031 und ADR-0034. Deren Index-, Evidence-, Publikations- und Journalentscheidungen
bleiben unverändert gültig.

## Kontext

Das dauerhafte Deep-Map-Journal aus ADR-0034 ist absichtlich inhaltsfrei und technisch. Eine direkte
Darstellung seiner Felder erklärt jedoch weder den Erkundungsplan noch die entstehenden Module Cards
oder die Auswirkungen im Code Atlas. Gleichzeitig dürfen unverifizierte Modellantworten, Quelltext,
Providerdaten oder historische Cards nicht zu einer bequemeren Anzeige in das Journal kopiert werden.

## Entscheidung

- Die normale Detailansicht ist eine Read-Projektion, die das inhaltsfreie Laufjournal mit genau dem
  zum Lauf gehörenden aktuellen veröffentlichten Index, verifizierten Module Cards und der aktuellen
  Atlas-Projektion kombiniert. Das Journal bleibt die Quelle für Ablauf und Fortschritt, nicht für
  Card-Inhalte.
- Knowledge-Schema V29 ergänzt jeden neuen Planschritt um genau eine geschlossene Zielreferenz
  (`ModuleId`, `SymbolId` oder Datei-`ModuleCardEvidenceId`) und die kanonisch sortierten vorgesehenen
  `ModuleCardField`-Werte. Beide Ergänzungen werden zusammen mit dem Plan atomar gespeichert. Sie
  enthalten weder Quelltext noch Modellinhalt.
- Alte Läufe werden nicht rückwirkend angereichert. Fehlende V29-Details werden als eingeschränkte
  historische Information kenntlich gemacht. Aktuelle Cards werden niemals an einen älteren
  Laufanker angehängt.
- Vier streng versionierte, projektgebundene V1-Reads liefern das Dashboard, höchstens 20
  Modulzusammenfassungen, höchstens 50 aufgelöste Planschritte und höchstens 50 aktuelle
  Atlas-Auswirkungen pro Seite. Die Oberfläche erhält nur Core-ausgegebene Selektionen und Cursor;
  nicht mehr auflösbare Referenzen geben keine interne ID preis.
- Phasen- und Modulzustände werden im Application Core aus geschlossenen Journalzuständen
  abgeleitet. Die WebView übersetzt nur diese Zustände in verständliche Texte.
- Vollständige Card-Inhalte kommen weiterhin ausschließlich über `query_module_card_detail` und
  nur für eine verifizierte, veröffentlichte Card am exakten aktuellen Laufanker. Quellen öffnen
  weiterhin ausschließlich die begrenzte Source-Preview aus ADR-0030.
- Das Dashboard zeigt keine Provider- oder Modellnamen, Prompts, Modellantworten, Tokenbudgets,
  Snapshots, Vertrauenswertungen oder internen IDs. Ein Fehler darf optional einen kurzen,
  geschlossenen Diagnosecode zeigen.

## Konsequenzen

### Positiv

- Nutzer sehen Plan, aktuelle Untersuchung, bestätigte Card und Atlas-Wirkung in einer kohärenten
  Ansicht, ohne technische Journalzeilen interpretieren zu müssen.
- Die Oberfläche kann live aktualisieren, ohne Journal und verifizierte Wissensquellen zu
  vermischen oder unveröffentlichte Entwürfe offenzulegen.
- Historische Läufe bleiben lesbar, behaupten aber keine heutige Gültigkeit.

### Negativ

- Eine Dashboard-Abfrage benötigt mehrere begrenzte, an denselben Projekt- und Laufanker gebundene
  Reads.
- Die Oberfläche muss Paging sowie den Übergang von geplant zu veröffentlicht explizit behandeln.
- Alte Läufe bieten weniger Planinformation als neue V29-Läufe.

### Risiken und Gegenmaßnahmen

- Eine stale Referenz wird als aktuelles Wissen gezeigt — Namen, Cards und Atlas-Auswirkungen werden
  nur gegen den exakten aktuellen Indexanker aufgelöst; sonst erscheint ein historischer Hinweis.
- Card-Entwürfe gelangen in die WebView — das Dashboard liest keine Providerantworten und Card-Inhalt
  wird erst nach Verifikation und atomarer Publikation über den bestehenden Detail-Read freigegeben.
- Ein großer Lauf überlastet DOM oder IPC — Module, Schritte und Auswirkungen bleiben fest auf
  20/50/50 Elemente pro Seite begrenzt und werden bei Bedarf nachgeladen.
- Polling verändert die Bedienposition — Aktualisierungen überlappen nicht und behalten ausgewähltes
  sowie aufgeklapptes Modul bei.

## Verworfene Alternativen

- Card-Inhalte im Journal speichern — dupliziert Evidence-gebundenes Wissen und schafft stale
  Wahrheiten.
- Rohe Journaltabellen nur verständlicher benennen — erklärt weder Plan noch Ergebnis und bleibt
  auf technische Implementierungsdetails fokussiert.
- Aktuelle Cards für jeden historischen Lauf anzeigen — vermischt Projektstände und erzeugt
  irreführende Kausalität.

## Compliance

- Migrationstests decken Neuinstallation, V28-Upgrade, leere historische Details und atomaren
  Rollback ab.
- Application- und IPC-Tests prüfen geschlossene Zustände, projektgebundene Cursor,
  Paging-Grenzen und das Fehlen verbotener technischer Inhalte.
- Atlas-Tests akzeptieren ausschließlich aktuelle, verifizierte Evidence am exakten Laufanker.
- Komponenten- und Integrationstests prüfen Live-Plan, veröffentlichte Cards, Fehlerhilfe,
  Historie, Atlas-Fokus, Tastaturbedienung und die kleinen Desktop-Viewports.

## Referenzen

- [ADR-0025](0025-bounded-desktop-rendering-and-lifecycle.md)
- [ADR-0030](0030-bounded-evidence-source-preview.md)
- [ADR-0031](0031-progressive-code-atlas-index-evidence.md)
- [ADR-0034](0034-deep-map-run-journal-and-current-index-lifecycle.md)
- [Daten und Persistenz](../DATA_AND_PERSISTENCE.md)
- [Index und Projektkarte](../INDEXING_AND_PROJECT_MAP.md)
- [IPC-Protokoll](../IPC_PROTOCOL.md)
