# ADR-0054: Vollständig gelieferte Planbestandsaufnahme

Status: Accepted\
Datum: 2026-09-06\
Freigabe: Nutzerauftrag zu Plan 10 einschließlich zugehöriger ADRs.\
Ergänzt ADR-0049/0050; allgemeine Analyze-Verträge bleiben unverändert.

## Befund

Ein Luna-Wiederholungslauf ließ die feste erste Planpflicht trotz vollständig
gelieferter benannter Originaldateien leer. Die Statusnotiz verneinte weiteren
API-Bedarf, verlangte aber nicht angeforderte Test-/Storage-Details. Der Core
unterdrückte die erneute Analyse desselben Pakets korrekt; weitere Reads konnten
diesen falsch offenen Bestandsauftrag nicht abschließen.

## Entscheidung

Nur für Q1 des exakt erkannten dreiteiligen Core-Planvertrags wird die engere
V5-Phase `SummarizeOriginals` gewählt, wenn jede ausdrücklich erforderliche Revision
als vollständig gelesen markiert ist und ihre lückenlose Originalabdeckung ab
Dateianfang vollständig im aktuellen Modellpaket liegt. Cachebesitz, Suchtreffer,
frühere Auslieferung oder gekürzte Fenster genügen nicht. Ohne diese Voraussetzung
bleibt die nullable Phase Analyze zuständig.

Diese Phase verlangt genau eine quellengebundene Interpretation der tatsächlich
gelieferten Einstiegspunkte, APIs und sichtbaren Integrationsgrenzen. Nicht gezeigte
externe Implementierungen werden als Grenze benannt, nicht erfunden und nicht zur
Voraussetzung für eine Beschreibung der vorhandenen Dateien gemacht. Zukunftsdesign
bleibt in Q2/Q3. Fragen, neue Pflichten und leere Ergebnisse sind hier schemawidrig
und dürfen nur den bestehenden einmaligen Repair nutzen. Auch ein formal gültiges
Ergebnis muss die unveränderte Original-/Hash-/Scope-Zulassung bestehen.

Dies ist keine automatische Faktenzulassung und keine semantische
Vollständigkeitsgarantie. Ungültige zweite Antworten bleiben Fehler. Budgets und
Berechtigungen werden nicht erweitert; Replan und allgemeine Ask-Fragen verwenden
weiterhin ihre bisherigen Phasen.

## Verifikation

Schema und unabhängiger Decoder prüfen genau ein Ergebnis, Interpretation,
richtige Teilfrage, ausschließlich tatsächliche E-Anker und keine Frage.
Core-Regressionen prüfen vollständige gegenüber partieller/früherer/falscher
Revision-Auslieferung, den unveränderten allgemeinen Analyze-Pfad und die feste
Planvertragsgrenze. Provider-Schemaprojektionen und echte Modellläufe werden geprüft.
