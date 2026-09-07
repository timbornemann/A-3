# ADR-0068: Variable Array-Maxima im Gemini-Wire-Schema

Status: Accepted\
Datum: 2026-09-07\
Freigabe: Fortgesetzter Plan-10-Auftrag einschließlich Modelltests und zugehöriger ADRs.

## Befund

Google `gemma-4-26b-a4b-it` besteht die Capability-Probe, weist aber das reale
Research-Initialize-Schema zurück. Einheiten allein hatten das nicht erkannt.
Kontrollierte Live-Diagnosen mit identischem Modell, Profil und öffentlichem Prompt
schließen Systemrolle, einfache Unions, Nullarrays und numerische Konstanten als
alleinige Ursache aus. Typisieren der Versionskonstante oder Inlining von `$ref`
behebt die Zurückweisung nicht. Entfernen der Array-Maxima größer eins liefert
dagegen Stop und 673 sichtbare Bytes. Das Testdokument enthält weiterhin ungültige
Referenzen und wird deshalb nicht als korrekte Recherche gewertet.

## Entscheidung

Ergänzend zur vorhandenen Gemini-Schemaprojektion aus
[ADR-0027](0027-google-gemini-model-provider.md) entfernt ausschließlich der Adapter
variable Array-Maxima größer eins aus dem übersetzten Wire-Schema. Leere Arrays
(`maxItems=0`), Einzelresultate (`maxItems=1`) und exakte Arity
(`minItems=maxItems`, einschließlich kompaktierter Tuples) bleiben erhalten.
Die Projektion erfolgt nach der bestehenden Tuple-Kompaktierung. Sie traversiert
nur Schema-Knoten, nicht konstante/Enum-Daten oder Beschreibungen.

Das originale versionierte Schema und der unabhängige Runtime-Decoder bleiben
unverändert. Größen-, Quellen-, Phasen-, Abhängigkeits- und Berechtigungsprüfungen
werden nicht entfernt; übergroße Modellantworten werden weiter abgewiesen.
Die Provider-Grammatik ist nur Erzeugungshilfe, keine Zulassungsinstanz. Kein
größerer Prompt, Kontext, Output, Repair, Retry oder Read und kein automatischer
Fallback auf unstrukturierten Text. Andere Provider ändern sich nicht.

## Folgen und Nachweis

Die Anfrage vermeidet den nachgewiesenen komplexen Array-Wire-Vertrag, kann aber
mehr vom Core abzuweisende Überlängen erzeugen. Der vollständige Live-Nachtest
muss daher tatsächliche Abschlüsse, Reparaturen, Nutzerhalte und Inhalte prüfen.
Ein Stop-Signal oder eine Modell-Selbsteinschätzung ist kein Erfolgskriterium.

Rot→Grün-Regressionsprüfungen kontrollieren unveränderte Core-Schemas, verbotene
33 Fragen/32 Abhängigkeiten, erhaltene 0/1-/Tuple-Grenzen und nicht veränderte
Literalwerte. Provider-HTTP-Verträge und die reale Recherchematrix ergänzen diese
Tests; sämtliche Policy-/Secret-/Freshness-Grenzen bleiben bestehen.
