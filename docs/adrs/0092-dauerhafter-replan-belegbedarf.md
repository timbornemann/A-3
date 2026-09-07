# ADR-0092: Dauerhafter typisierter Replan-Belegbedarf

Status: Accepted

Datum: 2026-09-07

Entscheider: Tim Bornemann (Vorabannahme der mit Plan 10 verbundenen ADRs)

Supersedes: Ausschließlich den Research-V5-Analysevertrag aus
[ADR-0048](0048-rungebundene-replan-recherche.md), der in
[ADR-0091](0091-statusfreie-replan-lokalisierung.md) noch erhalten blieb.

## Befund

Replan-Analyze verlangt bisher bei fehlenden Belegen leere Ergebnisse und eine
Lücke in einer freien Fortschrittsnotiz. Die Zulassung übernimmt aber nur den
ResearchWorkState; die Lücke geht verloren. Dasselbe Originalpaket wird korrekt
nicht erneut analysiert, der folgende Leseturn erhält jedoch keinen dauerhaften
Hinweis darauf, welchen Beleg die letzte Analyse noch benötigte.

## Entscheidung

Neue Replan-Analysen verwenden die vorhandene disjunkte Research-V7-Grammatik,
eingeschränkt auf Interpretation oder EvidenceNeed für die feste Pflicht Q1.
Keine Frage, freie Statusnotiz, leere Ergebnisliste oder ausführbare Aktion ist
zulässig. Historische Decoder bleiben unverändert. Prompt, Providerschema und
unabhängige Zulassung verwenden dieselben Grenzen, auch im einzigen Repair.

Ein EvidenceNeed ist ein Navigationsvorschlag, niemals Fakt, Verifikation,
Toolargument oder Freigabe. Seine ein bis acht begrenzten Literale müssen im
unveränderten Auftrag oder tatsächlich gelieferten Originalpaket vorkommen.
Der Core bindet ihn an die genauen Datei-/Hash-/Bereichsreferenzen dieses Pakets
und dessen kanonischen Analyseversuch. Er löst Q1 nicht. Ein neuer Read erhält
den Bedarf, ein neues Analyseergebnis ersetzt ihn oder löst ihn durch eine
zugelassene Interpretation ab. Dasselbe Paket wird nicht nochmals analysiert.

Knowledge V38 ergänzt den bestehenden immutable Checkpoint um eine nullable,
streng versionierte und begrenzte Belegbedarfsprojektion. Alte Zeilen bleiben
ohne erfundenen Bedarf erhalten. Speicherung mit dem Journal bleibt atomar;
Paketreferenzen müssen dieselben tatsächlich journalisierten Originalmarker
besitzen wie Ergebnisquellen. Keine Source-Bytes oder Rohantwort werden neu
gespeichert. Beim Wiederanlauf werden Originale durch den Safe Reader geladen;
vor Kontextaufnahme werden Paketmitgliedschaft, Versuch und literale Herkunft
erneut geprüft. Stale Quellen sperren die Aufnahme, nicht nur die Darstellung.

Der Core zeigt den offenen Bedarf bei folgenden Leseturns auch dann, wenn das
bereits analysierte Originalpaket nicht erneut eingeblendet wird. Ein Bedarf
kann weder still verschwinden, solange Q1 offen bleibt, noch Read-/Analysezähler
zurücksetzen. Vier Reads, ein Repair, äußere Runbudgets, Berechtigungen und
Implementierungsverifikation bleiben unverändert. Kein neuer Index, Provider,
Controller, generischer Speicher oder zusätzlicher Netzwerkzugriff entsteht.

## Nachweis

Erforderlich sind strikte Schema-/Turntests einschließlich unbekannter Ziele,
vermischter Antworten, fehlender Originale, identischem Paket, neuem Hilfsbeleg
und ausgeschöpftem Lesebudget. Storage-Vertrag und Migration prüfen Altbestand,
atomaren Rollback und exakten Wiederanlauf. Reale Safe-Reader-Tests prüfen
unveränderte und bearbeitete Quellen. Vollständige Qualitätsgates und sequenzielle
Modellnachtests ersetzen einander nicht; eine erfolgreiche Navigation allein ist
noch keine erfolgreiche Implementierung.

Referenz: [Plan 10](../plans/10-RESEARCH_WORK_STATE.md).
