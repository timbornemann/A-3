# ADR-0050: Verlustfreie Entwurfsübergabe und phasengebundene Belege

Status: Accepted\
Datum: 2026-09-06\
Freigabe: Nutzerauftrag zu Plan 10 einschließlich zugehöriger ADRs und direkter Korrektur der Modelltestbefunde.

## Befund

Die Modellmatrix zeigte, dass ein formal vollständiger Plan beim erneuten Formulieren
bereits getroffene Fehler- und Testentscheidungen verändern konnte. Abhängige Entwürfe
erhielten zudem nur 384-Byte-Vorschauen. Eine vollständige Übergabe machte anschließend
eine pauschale Quellenreserve und nicht mehr ausgelieferte E-Marker sichtbar.

## Entscheidung

Die drei Core-Pflichten aus ADR-0049 bleiben bestehen. Abhängige Entwurfsschritte
erhalten festgehaltene Designentscheidungen vollständig, nicht als gekürzte Vorschau.
Die deterministische Kontextverteilung priorisiert diese Pflichtdaten und den unveränderten
Nutzerauftrag vor optionalen weiteren Originalfenstern. Das konfigurierte Kontextlimit
wird nicht erhöht. Unpassende Pflichtdaten führen weiterhin zum sichtbaren sicheren Halt.

Die Core-Phase `Design(Q)` ist eine streng validierte Projektion von V5: ausschließlich
`designDecision` für die aktive Frage und eine leere Liste neuer Evidenzanker. Sie
baut auf bereits zugelassenen Voraussetzungen auf, ist selbst aber weder neue
Repository-Tatsache noch Implementierungsverifikation. `Analyze(Q)` bleibt die
Phase für aktuelle Originalbelege. Transport-Schema und unabhängiger Decoder setzen
dieselbe Grenze durch; ein Einzelrepair erhält keinen weiter gefassten Vertrag.

Für den erkannten unveränderten Core-Planvertrag entsteht der Plan ohne erneuten
Finalize-Modellaufruf direkt aus Änderungs- und Testentscheidungen. Formatierung
erfindet keine Fehlerpolitik, Schnittstelle oder Verifikation. Der bestehende
Ledger-Compiler prüft die resultierende Planform. Historische Verträge bleiben erhalten.

Bei ausdrücklich benannten bestehenden Dateien darf die Initialisierung keinen
rein aus Entwurfsfragen bestehenden Vertrag einfrieren. Die fehlende erforderliche
Repository-Pflicht wird vor dem Commit im bestehenden Einzelrepair beanstandet.
Das ist keine automatische semantische Umdeutung der Nutzerfrage.

## Grenzen

Eine Quelle und ein deterministisches Format beweisen nicht die Wahrheit oder
Widerspruchsfreiheit natürlicher Modellaussagen. Deshalb werden notwendige
Vollständigkeitskriterien und inhaltliche Sichtprüfung getrennt ausgewiesen.
Es entstehen weder neue äußere Budgets noch Berechtigungen oder automatische
Nutzerzustimmungen. Diese Präzisierung ersetzt nur die zusätzliche freie Planformatierung
im Core-Vertrag, nicht die Zustands- und Sicherheitsgrenzen aus ADR-0047 bis ADR-0049.
