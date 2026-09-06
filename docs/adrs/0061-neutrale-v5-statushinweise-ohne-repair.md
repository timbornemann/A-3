# ADR-0061: Neutrale V5-Statushinweise ohne Repair

Status: Accepted\
Datum: 2026-09-06\
Freigabe: Fortgesetzter Nutzerauftrag zu Plan 10 einschließlich Modelltests und direkter Korrektur zugehöriger Befunde.

## Befund

Der erneute Luna-Test `eval-1788706150645.jsonl` verbraucht wiederholt einen Repair
allein wegen eines leeren `note.gap` während der Initialisierung. Ein Modell muss
dort einen zusätzlichen Navigationshinweis erfinden, obwohl der Core gerade erst
die verbindlichen Fragen festlegt. Diese reine Statusangabe ist kein Ergebnisbeleg.
Der bisherige Schema-/Decodervertrag fordert dennoch nichtleeren Text und verwirft
deshalb das gesamte ansonsten gültige Dokument.

## Entscheidung

Die V5-Arbeitsentscheidungen `progress`, `question` und `plan` erhalten eine getrennte
Statusnotiz-Schemadefinition. Ausschließlich `gap` und `next_step` dürfen darin leere
Strings enthalten. Felder, Typen, Größen, Kontrollzeichen, Goal, Finding und Quellenlisten
bleiben streng validiert. Historische `answer`-/`research`-Notizen und V1–V4 bleiben
unverändert. Die Änderung präzisiert die nichtautoritative Notizregel aus ADR-0049.

Der unabhängige Decoder übersetzt einen fehlenden zusätzlichen Hinweis in einen
festen neutralen Darstellungstext: Es wurde keine zusätzliche Beleglücke gemeldet;
der Prüfstand bleibt maßgeblich. Ohne nächsten Modellhinweis bestimmt der Core die
nächste Arbeit aus diesem Prüfstand. Die bestehenden öffentlichen Persistenz- und
IPC-Notizen bleiben dadurch nichtleer und unverändert lesbar. Es entsteht weder ein
neuer Ergebnisbeleg noch eine positive oder negative Erkenntnis, ein Read oder eine
Ausführungsbefugnis. Eine leere Analyse bleibt unbeantwortet; ein leerer Designfortschritt
bleibt ungültig. Echte Fragen brauchen weiterhin eine folgenreiche Nutzerentscheidung.

Nur vollständig schema-/decoderkonforme Arbeitsentscheidungen erreichen anschließend
die unveränderte Work-/Originalzulassung. Null, falsche Typen, unbekannte Felder,
überlange Texte, falsche Anker und ungültige Ergebnisse werden nicht gerettet.
Der Einzelrepair und die äußeren Budgets werden nicht erweitert.

## Verifikation

Ein unabhängig roter Decoderfall und ein roter realer Ask-/Plan-/Agent-Controllerfall
reproduzieren den unnötigen Halt. Nach Korrektur müssen diese mit drei regulären
Aufrufen, ohne Repair, zusätzliche Reads oder Originaländerung abschließen.
Leere Ergebnisse dürfen allein durch den neutralen Status keinen Abschluss erhalten.
Legacy- und Negativverträge sowie echte Luna-/Local-Nachtests bleiben verbindlich.
