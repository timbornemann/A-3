# ADR-0072: Typisierter Belegbedarf ohne modellgenerierte Statusprosa

Status: Accepted\
Datum: 2026-09-07\
Freigabe: Fortgesetzter Plan-10-Auftrag.\
Supersedes: ausschließlich die Beschränkung der V6-Lesehinweise auf den Text der offenen Pflicht in ADR-0071.

## Befund

Die Prüfung des V6-Umbaus zeigt eine wichtige Grenze: Der bisherige Core liest
gezielt weitere vorhandene Symbole anhand einer validierten Modell-Lückennotiz.
Entfernt man alle solchen Hinweise, könnte ein im Original gerade entdeckter
weiterer Aufrufer oder Helfer nicht mehr gezielt untersucht werden. Verwaltungs-
prosa zu entfernen darf die fachliche Analyse nicht auf den ursprünglichen
Wortlaut des Auftrags beschränken.

## Entscheidung

Nur V6-Analyze erhält zusätzlich `decision.kind=evidenceNeed` mit der exakten
aktiven `question_id` und ein bis acht kurzen `targets`. Dies sind nichtausführbare
Suchkandidaten, keine frei formulierten Statusangaben, Repository-Fakten, Tools
oder Änderungen am Fragevertrag. `work.questions` und `work.results` müssen leer
sein. Initialize, vollständige Bestandsaufnahme, Design, Tests und Finalisierung
akzeptieren diese Variante nicht.

Targets sind begrenzte einfache Symbol-/relative Pfadliterale ohne Whitespace,
Steuerzeichen, Traversal, absolute Pfade, Shellsyntax oder URI. Vor Übernahme muss
jeder Kandidat wörtlich im gebundenen Originalauftrag oder einem tatsächlich
gelieferten aktuellen Originalfenster vorkommen. Der Core verwendet ihn nur zur
Kandidatenauswahl im bestehenden aktuellen Index. Alle vorhandenen Scope-, Safe-
Reader-, Deduplizierungs-, Cancellation-, Freshness- und Gesamtbudgetgrenzen gelten
weiter. Ein Textvorkommen belegt keine Semantik und erteilt keine Ausführungsrechte.

Der Core bindet den validierten Bedarf als Navigation an die weiterhin offene
Pflicht und erzeugt die übrige Statusanzeige selbst. Der Bedarf schließt keine
Teilfrage, erzeugt keine Ergebnis-Evidence und wird nicht als Erkenntnis gesammelt.
Der bestehende öffentliche Audit-/Fortsetzungsweg erhält den begrenzten Hinweis;
keine Persistenzmigration. V3–V5 bleiben unverändert. Ungültiger Bedarf erhält
höchstens den bestehenden Einzelrepair, niemals einen salvagierten Read.

## Nachweis

Erforderlich sind strikte Decoder-/Phasen-/Literalgrenzen, fehlender oder falscher
Originalbezug, unveränderte Pflichtzustände, Erhalt über Fortsetzung und eine reale
mehrstufige Recherche mit erst später benötigtem Helfer. Negative Fälle dürfen
weder neue Reads noch einen Abschluss autorisieren. Statusprosa und Modell-
Selbstbewertungen bleiben gemäß [ADR-0071](0071-core-eigene-recherchestatusangaben.md)
aus dem aktuellen Wire-Vertrag entfernt.
