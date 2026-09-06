# ADR-0063: Idempotente V5-Statusquellen

Status: Accepted\
Datum: 2026-09-06\
Freigabe: Fortgesetzter Nutzerauftrag zu Plan 10 einschließlich Modelltests und direkter Korrekturen.\
Ergänzt ADR-0059/0061; ersetzt ausschließlich die Eindeutigkeitsforderung der V5-Statusquellenliste.

## Befund und Entscheidung

Der Gemma-Nachtest `eval-1788707786335.jsonl` verbraucht in Storage 0:2 einen
Einzelrepair wegen wiederholter gültiger `finding_source_refs`, obwohl die
Ergebnisanker bereits kanonisch und eindeutig sind. Statusquellen sind wie
Ergebnisquellen eine Menge, keine Anweisung zu mehrfachen Zugriffen.

Nur in der getrennten V5-Statusnotiz entfällt `uniqueItems`. Der unabhängige
Decoder prüft weiterhin zuerst das unveränderte Eingabelimit von 32 Einträgen
und jeden einzelnen kanonischen S-Verweis. Erst danach übernimmt er gleiche
gültige Nummern einmal in Reihenfolge ihres ersten Auftretens. Ungültige Nummern
oder Typen werden niemals durch Deduplizierung verborgen. Die anschließende
Zulassung prüft weiterhin, ob jede Quelle tatsächlich im aktuellen Scope existiert.

V1–V4, historische `answer`-/`research`-Notizen und AgentAction-Notizen behalten
ihre bisherigen Verträge. Kein neuer Read, keine Quelle, kein Ergebnis und keine
Abschlussbefugnis entstehen aus einer Statusnotiz. Budgets und der Einzelrepair
bleiben unverändert. Dies erweitert die leeren V5-Navigationshinweise aus ADR-0061
nicht auf weitere Felder oder auf ungültige Quellen.

## Verifikation

Decoderregressionen prüfen wiederholte gültige Einträge, Erstauftretensreihenfolge,
32/33-Eingabegrenze, ungültige hintere Einträge, neutrale leere Ergebnisse und
unveränderte Legacy-Schemas. Ein echter Git-/Index-/Reader-/libSQL-Vertrag prüft
Ask, Plan und Agent-Vorbereitung ohne zusätzlichen Repair oder Read und mit
bytegleichen Originalen. Die Regressionen werden vor der Korrektur ausgeführt;
echte freigegebene Modellnachtests bleiben getrennt von formalen Verträgen.
