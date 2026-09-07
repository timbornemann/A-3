# ADR-0071: Core-eigene Statusangaben im V6-Recherchevertrag

Status: Accepted\
Datum: 2026-09-07\
Freigabe: Fortgesetzter Plan-10-Auftrag einschließlich notwendiger Harness-Anpassungen.

## Befund und Ziel

Die Google-Gemma-Matrix nach ADR-0068 endet mit 0/12 Abschlüssen. Nach explizitem
minimalem Thinking erreicht Storage 0:0 die Analyse, endet aber weiterhin mit
OutputLimit. Mehrere vollständige JSON-Antworten scheitern unabhängig vom eigentlichen
Arbeitsergebnis an modellgenerierten Statusangaben, beispielsweise ungültigen
S-Referenzen. Die bisherigen V5-Korrekturen beseitigen einzelne Statusfehler, lassen
aber das Modell weiterhin Ziel, Erkenntnis, Lücke und nächste Arbeit wiederholen.
Der Core besitzt Originalauftrag, Fragevertrag, Quellen und nächste Arbeit bereits.

## Entscheidung

Der aktuelle Modellvertrag wird als V6 versioniert. Die phasengebundenen
`work.questions` und `work.results` und die engen Entscheidungen `progress`,
`question` beziehungsweise Legacy-Finalisierung `plan` bleiben erhalten.
`decision.note` entfällt vollständig: V6 akzeptiert keine vom Modell erzeugten
Status-, Fortschritts-, Abschluss- oder Quellenmetadaten neben den Arbeitsergebnissen.
V3 bis V5 bleiben mit ihren bisherigen eigenständigen strikten Decodern lesbar.
V5-Zulassung wird nicht gelockert, und gespeicherte Verträge werden nicht umgeschrieben.

Die normalisierte Präsentationsnotiz erhält eine typisierte Herkunft. Eine V6-
Notiz wird nach erfolgreicher Work-Admission ausschließlich aus dem Core-Prüfstand
gebildet: gebundenes Ziel, Zahl beantworteter Teilfragen und nächste offene Pflicht.
Sie behauptet keine Repository-Fakten und erhält keine erfundenen S-/E-Belege.
Solche Verwaltungsnotizen werden nicht erneut als Rechercheerkenntnisse gesammelt.
V3–V5-Modellnotizen behalten ihre historische Behandlung. Der nächste Lesehinweis
stammt bei V6 aus der offenen Pflicht, nicht aus frei erfundenen Modell-Lücken.

Die Weiterentwicklung ergänzt [ADR-0047](0047-verbindlicher-recherchearbeitsstand.md)
und präzisiert die Statusnotizentscheidung aus
[ADR-0049](0049-core-planpflichten-und-statusnotizen.md) ausschließlich für neue V6-
Antworten. Originalbelege, Ergebnisart, Abhängigkeiten, Freshness, Einzelrepair,
Budget, echte Nutzerentscheidungen und Sicherheitsfreigaben bleiben unverändert.
Ein gültiges Verwaltungsdokument oder der Wegfall einer Notiz beweist weiterhin
keine semantische Wahrheit. Das Modell muss die eigentliche Recherche leisten.

## Grenzen und Nachweis

Keine Persistenzmigration, kein neuer Index, kein neuer Dienst, keine zusätzliche
Abhängigkeit oder vergrößerter Prompt. Das bisherige V5-Schema bleibt als historischer
Vertrag vorhanden; die Produktionskompilierung verwendet das engere V6-Schema.
Ein alter zulässiger V5-Vorschlag kann weiterhin nur unter seinen vollständigen
V5-Prüfungen und dem aktuellen Phasen-/Evidence-Vertrag verarbeitet werden.

Pflichttests: strikte V6-Felder und Version, verbotene Modellnotizen, unveränderte
V5-Validierung, fehlende/falsche Originalanker, genau ein Repair, Core-Notizen erst
nach Zulassung, deterministische offene Pflicht, Reopen/Freshness und kein Zuwachs
der Erkenntnissammlung durch Status. Providerpakete und Schemas werden über alle
Phasen und 8k/2k-Grenzen geprüft. Vergleichbare Live-Läufe müssen Größe, Calls,
Abschlüsse, Rückfragen und tatsächliche Inhalte separat nachweisen; ein vermuteter
Qualitätsgewinn wird nicht als bestandene Abnahme ausgegeben.
