# ADR-0103: Explizite Befehlsnamen getrennt in Änderung und Testplan erhalten

Status: Accepted\
Datum: 2026-09-08\
Freigabe: Fortgesetzter Plan-10-Auftrag einschließlich automatischer ADR-Annahme.

## Befund

Der [Schema-Grounding-Nachtest](../plans/10-SCHEMA_GROUNDING_VALIDATION.md) enthält
einen formal abgeschlossenen Granite-Plan für `python main.py import-csv <filepath>`.
Der Änderungsentwurf nennt stattdessen `import`. Die Erwähnung von `import-csv` im
Testabschnitt lässt den Gesamtwortcheck bestehen. Der Fehler entsteht schon bei der
Zulassung des Entwurfs, nicht erst in der verlustfreien Darstellung aus ADR-0050.

## Entscheidung

Die bestehenden drei Core-Planpflichten, V7-Ausgaben und Quellenregeln bleiben.
Bei neu eingehenden Design-/DesignTests-Ergebnissen dieses Vertrags prüft der Core
eine zusätzliche notwendige Namensabdeckung, separat je Ergebnis. Der Name darf
nicht aus einem anderen Ergebnis, dem Originalauftrag oder dem Kontext geliehen
werden. Eine Verletzung erreicht weder Ergebnis-Commit noch Analyse-Receipt und
erhält höchstens den vorhandenen Einzelrepair mit konkretem Namen aus dem Auftrag.

Eine reine Application-Projektion erkennt ausschließlich ausgeschriebene Aufrufe
der Form `python|python3 relative.py command <argument>` beziehungsweise
`node relative.js|.mjs|.cjs command <argument>`. Runner und Dateiendung müssen passen;
Namen sind case-sensitive ASCII-Identifier mit Bindestrich/Unterstrich, höchstens
64 Bytes. Der Platzhalter muss vollständig sein. Es werden keine Befehle aus
normaler Prosa, Dateiinhalten, Suchtreffern, Modellantworten oder unvollständigen
Shellausdrücken erschlossen. Die Projektion führt nichts aus und prüft weder die
Existenz eines Scripts noch eine Berechtigung. Sie bewahrt lediglich ausdrücklich
genannte Befehlsnamen als Textanker.

Der vollständige Auftrag bleibt die Autorität. Höchstens vier unterschiedliche
Namen werden aus höchstens 32 KiB Originalauftrag abgeleitet; bei Überschreitung
entsteht keine teilweise Schutzbehauptung. Nicht erkannte Syntax bleibt beim
bisherigen vollständigen Auftrag ohne zusätzliche Namensprüfung. Für erkannte
Aufrufe werden die Namen in beiden Designphasen im regulär abgerechneten Kontext
angezeigt. Originalauftrag und verpflichtende Voraussetzungen dürfen dafür nicht
gekürzt werden. Der Repair bleibt innerhalb von 768 Bytes.

Bereits zugelassene historische Ergebnisse werden nicht rückwirkend verändert.
Ask, Bestandsanalyse, benutzerdefinierte Rechercheverträge und folgenreiche offene
Nutzerentscheidungen bleiben unverändert. Kein neues Wire-/DB-Schema, Index,
Modellaufruf, Read, Reparaturrecht oder ausführbarer Prozess wird eingeführt.

## Grenzen und Abnahme

Diese Prüfung erkennt fehlende exakte Namen, nicht jede falsche Verwendung eines
vorhandenen Namens. Ein korrekter Token neben widersprüchlicher Prosa ist weiterhin
kein semantischer Beweis. Reine Prosa wie „CLI-Befehl import-csv in main.py“ fällt
absichtlich nicht unter die enge Syntax. Allgemeine Auftragserfüllung, tatsächliche
Seiteneffekte und Implementierungsverifikation bleiben getrennte offene Abnahmen.

Regressionen müssen den falschen Entwurf auch dann ablehnen, wenn die richtige
Benennung im Auftrag oder einem anderen Ergebnis steht. Weitere Pflichtfälle:
groß-/kleinschreibungssensitive Tokengrenzen, mehrere Namen, Duplikate, Überlauf,
UTF-8, unverstandene Syntax, Ask-/Bestandsisolation, unveränderte Originalpakete,
genau ein Repair, persistierter Wiederanlauf und keine falsche Fertigstellung.
Gegenbalancierte reale Modellnachtests verwenden dieselbe öffentliche Aufgabe und
unveränderte Rubrik; verbesserte Worttreffer allein rechtfertigen keine Freigabe.

Referenzen: [ADR-0047](0047-verbindlicher-recherchearbeitsstand.md),
[ADR-0049](0049-core-planpflichten-und-statusnotizen.md),
[ADR-0050](0050-verlustfreie-entwurfsuebergabe.md),
[ADR-0075](0075-disjunkte-rechercheantworten-statt-leerfortschritt.md).
