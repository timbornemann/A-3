# ADR-0049: Core-Planpflichten statt spekulativer Rechercheblocker

Status: Accepted\
Datum: 2026-09-06\
Freigabe: Nutzerauftrag zur vollständigen Umsetzung und direkten Korrektur der Modelltestbefunde einschließlich zugehöriger ADRs.

## Befund

Die ausdrücklich freigegebene Prüfung mit dem konfigurierten `gpt-5.6-luna` ergab
zunächst fünf Fehlschläge in zwölf synthetischen Aufgaben. Alle drei CSV-Pläne
blieben unvollständig. Ein Wiederholungslauf belegte eine modellgenerierte Pflicht
zur Suche nach vorhandenen Testkonventionen, bevor neue Tests entworfen werden durften.
Der Kontext zeigte außerdem nicht den eingefrorenen Fragetyp: Selbst Entwurfsfragen
erhielten die Anweisung, vorhandenen Originalcode nachzuweisen. Quellenlose Statusnotizen
lösten unabhängig vom eigentlichen Arbeitsergebnis Repairs aus.

## Entscheidung

Für neue Plan- und Agent-Vorbereitungsläufe mit V5 erzeugt der Core drei erforderliche
Prüfaufträge: relevante bestehende Einstiegspunkte und Integrationsgrenzen verstehen,
die verlangte Änderung einschließlich Schnittstellen und Fehlerverhalten entwerfen,
konkrete Abnahme- und Regressionstests bestimmen. Reihenfolge und Abhängigkeiten
sind fest; der vollständige ursprüngliche Auftrag bleibt unverändert im Prüfstand.
Die Initialisierung benötigt keinen Modellaufruf und erneuert kein Budget.

Ask behält seine auftragsbezogene V5-Zerlegung. Bereits gespeicherte Verträge werden
nicht rückwirkend umgeschrieben. Entwurfsergebnisse sind zukünftiges Verhalten und
dürfen keinen vorhandenen neuen Code voraussetzen. Ein geplantes Dateiziel ist nicht
automatisch eine Tatsachenbehauptung über diese Datei. Die ursprüngliche Quellenabdeckung
über den gesamten Vertrag bleibt verpflichtend.

Der aktive Kontext stellt den eingefrorenen Fragetyp und seine Zulassungsbedingungen
explizit dar. V5-Statusnotizen ohne Quellen werden zu Hypothesen herabgestuft.
Sie dürfen weder Facts noch Ergebnisbelege erzeugen. Nur `work.results` durchläuft
die fachliche Zulassung. Legacy-V3/V4 behalten ihre bisherigen strikten Notizverträge.

## Grenzen

Der Core garantiert keine semantische Wahrheit beliebiger Modellinterpretationen.
Die feste Planstruktur verhindert zusätzliche modellgenerierte Pflichtblocker, ersetzt
aber nicht die originale Anforderung, Quellenprüfung oder konkrete Verifikation.
Modellmatrix, adversariale Tests und gemeinsamer Storage-Vertrag bleiben Abnahmebedingungen.
Diese Präzisierung von [ADR-0047](0047-verbindlicher-recherchearbeitsstand.md) entfernt
keine Sicherheitsfreigabe und macht einen Rechercheabschluss nicht zur Umsetzung.
