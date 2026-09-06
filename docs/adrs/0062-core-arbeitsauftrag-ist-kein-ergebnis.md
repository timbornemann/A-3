# ADR-0062: Core-Arbeitsauftrag ist kein Ergebnis

Status: Accepted\
Datum: 2026-09-06\
Freigabe: Nutzerauftrag zu Plan 10 einschließlich zugehöriger ADRs.\
Ergänzt ADR-0049/0057; historische Aggregate und allgemeine Ask-Antworten bleiben unverändert.

## Befund und Entscheidung

Qwen gab im 8k-Nachtest `eval-1788706145063.jsonl`, CSV-Variante 0,
die vollständige Core-Pflicht für die Testplanung wortgleich als Ergebnis zurück.
Der formale Abschluss enthielt dadurch keine konkreten Tests. Die Begriffrubrik
erkannte dies nicht, weil dieselben Begriffe auch in Auftrag und Bestandsanalyse stehen.

Der unabhängige WorkGuard weist neue Ergebnisvorschläge im eindeutig erkannten
Core-Planvertrag zurück, wenn deren vollständiger Text lediglich eine der drei
Core-Pflichten wiederholt. Der Vergleich normalisiert ausschließlich Whitespace;
er interpretiert weder Sprache noch Code oder beliebige Nutzerwünsche. Die Prüfung
erfolgt vor Ergebniszulassung und erzeugt einen eigenen typisierten Diagnosegrund.
Ein gezielter Hinweis verwendet ausschließlich den vorhandenen Einzelrepair.
Wiederholt auch dieser nur die Pflicht, bleibt die Frage offen und erhält keine
Analysequittung. Es gibt keine weitere Leserunde oder Budgeterneuerung.

Der Core-Planvertrag ist eine explizite Vorbedingung. Wortgleiche Antworten auf
allgemeine Ask-Fragen können legitim sein und bleiben unberührt. Persistierte
historische Ergebnisse werden nicht umgeschrieben. Ein nicht identischer Text
ist dadurch ausdrücklich noch nicht inhaltlich richtig, vollständig oder verifiziert.

## Verifikation

Ein direkter Guard-Test prüft alle drei Phasen, alle Core-Pflichten,
Whitespace-Varianten, atomare Ablehnung, gültige Ergebnisse und unveränderte
literale Ask-Antworten. Reale Plan-/Agent-Vorbereitungstests mit Git, Fast Index,
Safe Reader und libSQL prüfen erfolgreichen sowie ausgeschöpften Einzelrepair,
null adaptive Zugriffe, Reopen mit offener Testpflicht und bytegleiche Originale.
Der konkrete Qwen-CSV-Fall wird anschließend mit unverändertem 8k-Profil live geprüft.
