# ADR-0104: Befehlsprüfung ohne zusätzlichen primären Kontexthinweis

Status: Accepted\
Datum: 2026-09-08\
Freigabe: Fortgesetzter Plan-10-Auftrag einschließlich automatischer ADR-Annahme.\
Supersedes: ausschließlich die zusätzliche Namensanzeige in beiden regulären
Designkontexten aus [ADR-0103](0103-explizite-befehlsnamen-im-plan.md).

## Befund

Der [kontrollierte Vergleich](../plans/10-REQUEST_COMMAND_VALIDATION.md) zeigt:
Mit zusätzlicher Namensanzeige bleibt Granites Entwurf namenfehlerhaft und wächst
von 1107 auf 4046 Textbytes; beide Reparaturen überschreiten die vorhandene Grenze.
Ornith hält zwar den Namen, fordert aber jetzt drei CLI-Positionsargumente anstelle
des einzigen ursprünglich verlangten Dateipfads. Diese Einzelbeobachtung beweist
keine universelle Kausalität, liefert aber keinen Nutzenbeleg für den Zusatz.

## Entscheidung

Die reine Application-Projektion, notwendige Namensprüfung je Design-Ergebnis und
der begrenzte einmalige Namensrepair aus ADR-0103 bleiben. Die zusätzliche
Namenswiederholung im primären Arbeitskontext entfällt. Der vollständige ursprüngliche
Auftrag bleibt dort unverändert die Autorität. Ein gültiger Entwurf benötigt weder
weiteren Prompttext noch einen zusätzlichen Modellaufruf. Nur eine tatsächlich
fehlende Namensabdeckung führt zum gezielten Repair mit der bestehenden Textgrenze.

Es werden keine Modelle oder Aufgaben aus der Prüfung ausgenommen. Historische
Ergebnisse, Quellenversorgung, geordnete Voraussetzungen und Budgets bleiben
erhalten. Zusätzliche Fragen, Reads und Repairs sind weiterhin unzulässig.
Die Namensprüfung ist keine Prüfung der gesamten CLI-Signatur oder beliebiger
Widersprüche. Weniger Kontexttext allein gilt nicht als Qualitätsnachweis.

## Abnahme

Die vorhandenen Syntax-, Phasen-, Repair-, Freshness-, Wiederanlauf- und
Mehrmodusverträge bleiben Pflicht. Finale Livefälle werden dem eingefrorenen Stand
ohne Doppelung zugeordnet und getrennt vom verworfenen Kontexthinweis ausgewertet.
Fehlende fachliche Ergebnisse werden nicht zu erfolgreichen Plänen umetikettiert.
