# ADR-0074: Konkreter Belegbedarf in der Planbestandsaufnahme

Status: Accepted\
Datum: 2026-09-07\
Freigabe: Fortgesetzter Plan-10-Auftrag einschließlich erforderlicher ADRs.\
Supersedes: ausschließlich das absolute Belegbedarfsverbot in V6-SummarizeOriginals aus ADR-0054/0072.

## Befund

Die lange Helferkette lässt sich nach ADR-0073 in Ask untersuchen. Plan und Agent-
Vorbereitung wählen nach vollständiger Auslieferung von entry.py dagegen bereits
SummarizeOriginals. Vollständigkeit der benannten Datei beweist nicht, dass der
Auftrag ohne die dort erst entdeckten Helfer beantwortet werden kann.

## Entscheidung

V6-SummarizeOriginals erlaubt zusätzlich denselben typisierten evidenceNeed wie
Analyze: exakte aktive Frage, leere Ergebnis-/Fragenlisten, ein bis acht begrenzte
Literal-Kandidaten aus dem aktuellen Auftrag oder tatsächlich gelieferten Originalen.
Der Core prüft dies unabhängig, behält den unveränderten offenen Arbeitsstand und
nutzt ausschließlich die vorhandene deduplizierte, budgetierte Read-Frontier.

Eine normale progress-Entscheidung verlangt weiterhin genau eine belegte
Interpretation. Freie Statuslücken, Nutzerfragen, neue Pflichten, neue Budgets und
Recherche in Design bleiben verboten. V5-Replay bleibt unverändert strikt.
Das V6-Providerschema muss für den Bedarf null Ergebnisse darstellen können;
die unabhängige phasengebundene Zulassung verbietet leeren progress weiterhin.
Diese Navigation beweist keine semantische Notwendigkeit oder Runtime-Bindung.

## Nachweis

Der reale mehrstufige Test läuft in Ask, Plan und Agent-Vorbereitung mit 4096
Evidence-Bytes. Ergebnisloser progress sowie erfundene/falsche/überlange Bedarfe
werden unabhängig geprüft; nach einem gescheiterten Repair entstehen keine Reads
oder abgeschlossenen Pflichten. V5, Design und Testentwurf behalten ihre Grenzen.
Gates und Live-Modelle sind getrennte Nachweise.
