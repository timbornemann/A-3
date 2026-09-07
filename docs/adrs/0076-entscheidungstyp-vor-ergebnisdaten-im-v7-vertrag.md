# ADR-0076: Entscheidungstyp vor Ergebnisdaten im V7-Vertrag

Status: Accepted\
Datum: 2026-09-07\
Freigabe: Fortgesetzter Plan-10-Auftrag mit automatischer ADR-Annahme.\
Präzisiert: die noch nicht abgenommene V7-Ergebnisdarstellung aus ADR-0075.

## Befund

Die erste flache V7-Union besteht die unabhängigen Decodertests, scheitert aber
in der Live-Matrix über sämtliche Modelle. Selbst der öffentliche triviale
Luna-Auftrag mit `def helper(): return 7` weicht auf eine Rückfrage aus; Google
liefert stattdessen Belegbedarf und nachfolgend Whitespace. Entfernt der direkte
Wirevergleich nur die Alternativen, geben beide dasselbe korrekte Ergebnis aus.
Bleiben alle Alternativen erhalten, wird das Ergebnis jedoch unter einem
gemeinsamen vorangestellten `kind` verschachtelt, antworten beide ebenfalls korrekt.

Das grenzt den Fehler auf die Form der Union ein. Eine plausible Erklärung ist
die Grammatik-/Schlüsselreihenfolge: Ergebnisdaten beginnen mit `evidence`, während
die anderen Varianten mit `kind` beginnen. Die internen Samplingmechanismen der
Provider sind damit nicht bewiesen. Ein formal zulässiges JSON-Schema allein ist
kein ausreichender Kompatibilitätsnachweis.

## Entscheidung

V7 bleibt disjunkt, erhält aber für Interpretation und Entwurf die Darstellung
`response={kind:interpretation|designDecision,result:{question_id,text,evidence}}`.
Alle Varianten beginnen in der kanonischen Schemaeigenschafts- und Required-
Reihenfolge mit dem Entscheidungstyp. Der Ergebnisinhalt liegt unter `result`;
sein epistemischer Typ kommt ausschließlich aus dem validierten äußeren `kind`.
Die anderen V7-Varianten bleiben unverändert. Flache V7-Ergebnisfelder werden
nicht als Fallback salvagiert: Dieser experimentelle Stand war nie abgenommen.
Historische V3–V6-Dokumente behalten ihre bisherigen unabhängigen Decoder.

Kein Ergebnis wird erzwungen, wenn Belege fehlen. Der bisherige konkrete Bedarf
und echte Nutzerentscheidungen bleiben in denselben Phasen erlaubt. Original-
Admission, Freshness, Budget, Einzelrepair, Sicherheitsrechte und der separate
mutierende V5-Replan ändern sich nicht. Der gemeinsame Entscheidungstyp erleichtert
Ausgabe, entscheidet aber weder über Wahrheit noch Abschluss.

## Verifikation

Strikte positive/negative verschachtelte Dokumente, keine gemischten Felder und
unveränderte aktive Frage, Originalanker und UTF-8-Grenzen; explizite Überprüfung
der ersten Schemaeigenschaft aller Unionarme; echte Providerprojektionen und
Mehrmodus-/Navigations-/Reopen-/8k-Pakete. Die direkte öffentliche Wirediagnose
behält flache, Ergebnis-only- und verschachtelte Varianten für reproduzierbare
Vergleiche. Eine bestandene Miniatur ersetzt nicht die erneute Recherchematrix
oder semantische und Live-Agent-Abnahme.
