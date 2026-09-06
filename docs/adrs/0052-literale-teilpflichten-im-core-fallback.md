# ADR-0052: Literale Teilpflichten im Core-Fallback

Status: Accepted\
Datum: 2026-09-06\
Freigabe: Nutzerauftrag zu Plan 10 einschließlich zugehöriger ADRs.\
Ersetzt ausschließlich die Ein-Fragen-Granularität des Fallbacks aus ADR-0051.

## Befund

Die Modelltests zeigten trotz belegter Originaldateien wiederholt ausgelassene
Teile zusammengesetzter Fragen. Ein einziger allgemeiner Untersuchungsauftrag
macht diese Teile im aktiven Arbeitspaket nicht einzeln sichtbar.

## Entscheidung

Nur im bereits zugelassenen Fallback aus ADR-0051 darf der Core kurze Originalaufträge
bis 512 UTF-8-Bytes an Satz- und anschließenden Listengrenzen außerhalb von Zitaten
in höchstens sechs erforderliche Repository-Teilpflichten aufteilen. Die Reihenfolge,
wörtlichen Fragmente und der vollständige Originalauftrag bleiben erhalten. Dateilisten
vor der ersten Satzgrenze bleiben zusammen. Ein längerer Rest wird als unveränderter
zusammenhängender Ausschnitt übernommen, nicht umformuliert oder abgeschnitten.
Lange Originalaufträge behalten die einzelne allgemeine Pflicht aus ADR-0051.

Das ist eine mechanische Segmentierung, keine semantische Zerlegung oder
Vollständigkeitsgarantie. Sie erfindet weder Abhängigkeiten noch Quellen, Ergebnisse,
Budgets oder Berechtigungen. Schema-/Domain-Validierung, der einzelne Repair,
bestehende Verträge und die gesonderten Core-Planpflichten bleiben unverändert.

## Verifikation

Regressionen prüfen wörtliche Abdeckung, Reihenfolge, UTF-8, Dateilisten, Zitate,
mehrzeilige Reste mit mehrfachen Leerzeichen, die Sechs-Fragen-Grenze und die
unveränderte Domain-Zulassung. Die Modellmatrix bleibt ein separater Qualitätsnachweis.
