# ADR-0101: Großzügigere Originalzitate in SourceReview

Status: Accepted

Datum: 2026-09-08

Freigabe: ausdrückliche Vorgabe großzügigerer Budgets und vorab akzeptierter
Plan-10-ADRs im laufenden Auftrag.

Supersedes: [ADR-0100](0100-quellenlokale-recherche-im-vergleich.md), ausschließlich
SourceReview-Version und numerische Einzelzitatgrenze.

## Befund

Der feldgenaue Live-Nachtest weist einen Planabbruch durch dasselbe 260-Byte-Zitat
in Primärantwort und Einzelrepair nach. Flash liefert ein 265-Byte-Zitat. Die
bisherige 256-Byte-Grenze unterteilt bereits kleine zusammenhängende Codebereiche;
die Interpretationen selbst und das unveränderte Gesamtdokumentbudget sind nicht
die Ursache dieser beiden Grenzverletzungen. Die Kopieraufgabe bleibt außerdem
fehleranfällig; mehr Platz allein beweist keine bessere Interpretation.

## Entscheidung

Der ausschließlich explizite native Vergleich verwendet SourceReview V2 mit
höchstens 512 UTF-8-Bytes je Originalzitat. Maximal vier Zitate, 192 Bytes für die
kurze Interpretation und 4096 Bytes für das vollständige rohe Dokument bleiben
unverändert. Das ist eine Ressourcenaufteilung innerhalb bestehender Grenzen,
keine Lockerung von Quellen-, Berechtigungs- oder Ausführungszulassung.

Version und unabhängiger Decoder wechseln gemeinsam. Es gibt keine langlebigen
SourceReview-Dokumente zu migrieren; historische Vergleichsbinaries behalten V1.
Der aktuelle Decoder akzeptiert keine V1-Ausgabe. Größere Originalzitate müssen
weiterhin exakt und eindeutig in dem tatsächlich gelieferten aktuellen Fenster
liegen; kein Abschneiden, Normalisieren oder Erfinden von Quellbytes ist erlaubt.

Vor/nach jedem Aufruf gilt der vorhandene Safe Reader. Phasenführung, vollständiger
Originalauftrag, Originalvorrang, tatsächlicher Hint-Fit, Einzelrepair, gemeinsame
Retry-/Aufruf-/Zeitgrenzen, Modellprofile und Produktstandard `joint` bleiben
unverändert. Zitate werden nicht zusätzlich in die spätere Synthese kopiert.
Herkunft ist weiterhin kein semantischer Wahrheits- oder Verifikationsbeweis.

## Nachweis

Regressionsprüfungen verlangen zulässige 260-/512-Byte-Originale sowie abgelehnte
513-Byte- und Unicode-Überschreitungen, alte Version, Mehrdeutigkeit, übergroßes
Gesamtdokument und aktuelle Hash-/Rangebindung. Tatsächliche 8k/2k- und
16k/4k-Providerpakete und echte Mehrmodus-/Repair-Verträge bleiben Pflicht.
Modellnachtests trennen die beseitigte künstliche Zitatgrenze von weiterhin
erfundenen Effekten und ausgelassenen Aufrufen. Keine Produktübernahme allein
aufgrund bestandener Form- oder Wortprüfungen.

Referenz: [Plan 10](../plans/10-RESEARCH_WORK_STATE.md).
