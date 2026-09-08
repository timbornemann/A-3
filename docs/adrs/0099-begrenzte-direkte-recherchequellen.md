# ADR-0099: Begrenzte direkte Recherchequellen

Status: Accepted

Datum: 2026-09-08

Freigabe: Fortgesetzter Plan-10-Auftrag einschließlich zugehöriger ADRs.

## Befund

Der reale öffentliche Audit-Fall liefert nur Manager und Plugin-Originale.
Die Task Lens liest keine Storage-Implementierung. Der bestehende Fast Index
verknüpft jedoch den belegten Konstruktoraufruf mit `create_storage` in der
Storage-Datei. `self.storage.save_tasks` bleibt korrekt dynamisch und ungelöst.
Mehr Kontextplatz oder ein erfolgreicher Quellenanker beweist dessen Wirkung nicht.

## Entscheidung

Vor der ersten Bestandsanalyse darf der Core einmal pro Rechercheabschnitt die
bereits vollständig gelieferten benannten Originale um direkte Callee-Quellen
ergänzen. Auswahl: höchstens 4096 kanonische Graphkanten, ausschließlich aufgelöste
Calls mit vollständig geliefertem Aufrufstellenbeleg derselben Revision, maximal
vier verschiedene zusätzliche Dateien. Keine rekursive Verfolgung, keine Auflösung
dynamischer Namen, keine Importsammlung und keine neue Indexprojektion.

Nur Analyze/SummarizeOriginals, freier Paketplatz von mindestens 1024 Bytes und
ein verbleibender SearchAllowed-Turn erlauben diesen optionalen Schritt. Vor
den Reads werden die aktuellen gelieferten Revisionen sicher revalidiert.
Vorhandene Controller-, Read-, Audit-, Timeout-, Cancellation- und
Safe-Reader-Grenzen gelten unverändert. Jede neue Datei wird über das bestehende
InspectPath ab Zeile 1 geladen. Vollständig gelesene Ziele werden nicht wiederholt.
Fehlschlag oder fehlender Platz ist weder Abwesenheitsbeweis noch neue Suchpflicht.

Der Vorrang passender vollständiger Pflichtdateien aus ADR-0056 bleibt erhalten.
Danach dürfen bereits gelesene zusätzliche Ausschnitte in bestehender Cacheordnung
vollständig in den Rest passen: einschließlich exaktem Header und höchstens acht
Fenstern insgesamt, je Revision höchstens ein zusätzlicher Ausschnitt. Eine
Zusatzquelle kürzt keine Pflichtquelle. Expliziter Fokus und progressiver
Überlaufpfad bleiben unverändert. Nur echte aktuelle Originalfenster erhalten
E-Anker; die unabhängige Range-/Hash-/Scope-Prüfung gilt auch für Ergänzungen.

Es entstehen weder Fakten noch verifizierte Laufzeitketten. Allgemeine
Inhaltswahrheit, rekursive Evidence-Frontiers und die Umstellung der mutierenden
Agentstrategie sind ausdrücklich nicht Teil dieses Schnitts.

## Nachweis

Rot→Grün über den realen Index, Safe Reader, libSQL und besessenen Researcher in
Ask, Plan und Agent-Vorbereitung. Caller, Writer und Storage müssen gleichzeitig
im tatsächlich übergebenen Paket stehen; kein Modellgedächtnis ersetzt diesen
Nachweis. Gesonderte Tests schützen deterministische Auswahl, fehlende/teilweise
oder stale Aufrufbelege, ungelöste Ziele, nicht rekursive Auswahl, Read-/Zeitgrenzen,
Byte-/Fensterlimits und unveränderte Pflichtquellen. Modellnachtests bleiben
von der Offline-Zulassung getrennt.

Referenz: [Plan 10](../plans/10-RESEARCH_WORK_STATE.md).
