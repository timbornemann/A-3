# ADR-0102: Originalgebundene Operationshinweise

Status: Accepted

Datum: 2026-09-08

Freigabe: fortgesetzter Plan-10-Auftrag einschließlich zugehöriger ADRs.

## Befund

Die quellenlokale V2-Recherche schließt ab, verwechselt aber weiterhin Methodennamen
mit implementierten Wirkungen und lässt konkrete Writer aus. Der echte Python-Parser
des bestehenden Fast Index liefert für das öffentliche Audit-Fixture bereits
`open` und `output.write` sowie getrennte `Return`-Schritte der Storage-Methoden.
Er behält dabei ausdrücklich Dynamic-/Unsupported-Grenzen. Freie Interpretation
allein nutzt diese vorhandene strukturelle Information nicht zuverlässig.

## Entscheidung

Der explizite native `source-local`-Vergleich erhält vor seinen unveränderten
SourceReview-V2-Aufrufen ein Core-kompiliertes, optionales Operationsinventar aus
dem bestehenden `ExploreFunctionFlows`-Reader nach ADR-0045. Produktstandard
bleibt `joint`; es gibt weder neue Modellphasen noch einen weiteren Index.

Nur vollständig im tatsächlich gelieferten Originalfenster liegende Funktionen
derselben Revision sind Kandidaten. Der Core inspiziert höchstens acht Funktionen
je Fenster, in Quellreihenfolge, ohne Call-Pfadexpansion. Höchstens 4096 Symbole
werden je Auswahl untersucht; ein begrenztes Präfix ist keine vollständige
Dateiinventur. Ein gemeinsames Zwei-Sekunden-Limit begrenzt diese Metadatenphase je
Fenster zusätzlich zur unveränderten Recherchefrist und Cancellation.

Die Projektion nennt ausschließlich beobachtete Schrittkategorien, Aufrufnamen,
Originalzeilen und bekannte Analysegrenzen. Sie behauptet keine Laufzeitfolge,
keine Wirkung unbekannter Callees und niemals Nebenwirkungsfreiheit aus fehlenden
Calls. Lokale Aufrufstellen bleiben getrennt. Quellnamen sind untrusted data.

Höchstens 1024 UTF-8-Bytes, begrenzt durch den tatsächlich freien Rest des
Einzelquellenpakets, dürfen zusätzlich verwendet werden. Der Core übernimmt nur
ganze Funktionszeilen, markiert Auslassungen und verdrängt oder kürzt keinen
Originalauftrag, Originaltext oder Repair. Fehlt der Platz, entfallen ausschließlich
diese optionalen Hinweise. Fehlende historische Flow-Artefakte sind keine Aussage
über fehlende Operationen; Fehler, Publikationswechsel oder Cancellation werden
nicht als erfolgreiche Metadatenlieferung ausgegeben.

Es werden keine neuen Originale gelesen. Die begrenzten Index-Metadatenreads sind
Teil der Kontextvorbereitung, keine vom Modell angeforderten neuen Leseaktionen.
Vor und nach der Metadatenvorbereitung sowie dem Modellaufruf gilt der bestehende
Safe Reader; der Flow-Reader prüft zusätzlich seine aktuelle Publikation. Die
Metadaten werden nicht als neue Facts oder persistente Rechercheergebnisse gespeichert.
V2-Decoder, Zitate, Quellenzulassung, gemeinsame Modell-/Repair-/Retrygrenzen und
abschließende originale Inhaltsprüfung bleiben unverändert.

## Nachweis und Nichtziele

Tests prüfen den echten Parser-/Flowpfad, exakte Revisions-/Rangebindung, Teilfenster,
ganzen UTF-8-Fit, deterministische Reihenfolge, fehlende Flow-Artefakte, Cancellation,
Live-Edit und unveränderte 8k-/16k-Pakete. Ein nativer Modellvergleich muss Wirkung
und zusätzliche Kosten messen, bevor eine Produktübernahme erwogen wird.

Keine allgemeine semantische Verifikation, keine automatische Beantwortung durch
den Core, kein neuer Tool-/Persistenzvertrag und keine Lockerung von Berechtigungen.
Abweichende Planliterale und doppelte Endantworten bleiben getrennte offene Defekte.

Referenzen: [ADR-0045](0045-fast-index-function-flow-analysis.md),
[ADR-0100](0100-quellenlokale-recherche-im-vergleich.md),
[ADR-0101](0101-grosszuegigere-originalzitate-in-source-review.md),
[Plan 10](../plans/10-RESEARCH_WORK_STATE.md).
