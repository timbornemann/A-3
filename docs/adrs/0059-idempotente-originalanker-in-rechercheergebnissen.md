# ADR-0059: Idempotente Originalanker in Rechercheergebnissen

Status: Accepted\
Datum: 2026-09-06\
Freigabe: Nutzerauftrag zu Plan 10 einschließlich zugehöriger ADRs und direkter Korrektur der Modelltestbefunde.

## Befund

Im realen Ornith-Nachtest werden gültige Originalanker mehrfach innerhalb eines
Ergebnisses genannt, etwa wenn mehrere erklärte Methoden im selben Fenster liegen.
Das V5-Schema erlaubt diese Wiederholung, der unabhängige Decoder verwirft aber
die gesamte Antwort. Ein inhaltsfreier Mengenfehler verbraucht so den Einzelrepair
und kann eine vollständig belegbare Recherche blockieren.

## Entscheidung

Ein Ergebnis referenziert eine Menge tatsächlich gelieferter Originalfenster.
Der Decoder validiert weiterhin jedes Eingabeelement vollständig. Anschließend
übernimmt er einen identischen gültigen E-Anker nur einmal, in der Reihenfolge
seines ersten Auftretens. Das bestehende Limit von 32 Eingabeelementen gilt vor
dieser Kanonisierung. Nichtkanonische Schreibweisen, unbekannte Felder, ungültige
IDs und falsche Typen bleiben ungültig; nichts wird als Anker erraten oder repariert.

Die Zulassung prüft weiterhin die aktuelle Auslieferung, eindeutige Fensterbindung,
Revision, Range und Freshness. Die Normalisierung erzeugt weder neue Quellen noch
eine fachliche Abschlussaussage. Historische Quote-/Statusnotiz-Verträge bleiben
unverändert. Dies präzisiert die V5-Decodergrenze aus ADR-0047, nicht Sicherheits-
oder Ausführungsrechte, äußere Budgets oder die Höchstzahl von Repairs.

## Verifikation

Eine unabhängige Decoderregression muss die zuvor abgewiesene Wiederholung zulassen,
ohne E0/E9/E01, zusätzliche Felder oder Eingaben oberhalb der Grenze zu akzeptieren.
Ein echter Plan-/Agent-Controllervertrag mit Git, Index, Reader und libSQL wiederholt
Originalanker und muss ohne Repair oder neue Leserunde abschließen. Bestehende
Negativfälle für ungelieferte, mehrdeutige und veraltete Originale bleiben verbindlich.
Der reale lokale Fehlfall und beide vollständigen Modellmatrizen werden nachgetestet;
der frühere lokale Teilbericht zählt nicht als vollständige Abnahme.
