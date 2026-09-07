# ADR-0088: Journalgebundene Ausführungsrückmeldung

Status: Accepted

Datum: 2026-09-07

Entscheider: Tim Bornemann (Vorabannahme der mit Plan 10 verbundenen ADRs)

## Befund

Ein echter Luna-V5-Lauf ändert das öffentliche Bugfix-Fixture korrekt und schlägt
anschließend erneut einen wirkungslosen Patch vor. Nach einer Mutation entfernt der
Desktop zu Recht alte Read-Ergebnisse. Der neu kompilierte Kontext enthält aber
keine Rückmeldung über die ausgeführte Mutation. Die erfolgreiche Patchausführung
ist bereits atomar in Run-Journal und Recovery-Attempt gespeichert. RunMemory
kompaktiert abgeschlossene Schrittversuche, nicht diese laufende Arbeit.

## Entscheidung

Eine kleine Application-Projektion rekonstruiert vor jedem normalen Agentturn und
beim vorhandenen Post-Mutations-Compile die jüngste nachweislich ausgeführte Mutation
des Runs am aktuellen Snapshot. Dafür werden höchstens 64 letzte Journalereignisse
und die bestehende auf 4096 begrenzte Mutation-Attempt-Abfrage verwendet. Die gesamte
Rekonstruktion hat fünf Sekunden Timeout, prüft Cancellation vor und nach den Reads
und besitzt keine Hintergrundaufgabe. Unvollständige oder widersprüchliche
Projektionen werden nicht als Erfolg verwendet; ein außerhalb des Fensters liegender
Nachweis erzeugt keine Behauptung.

Nur ein `ToolAction/Succeeded`-Ereignis mit genau passendem Run, ToolRunId und
Abschlusszeitpunkt sowie `Succeeded/Applied`-Attempt liefert eine Rückmeldung.
Patch und Process sind geschlossene Typen; Legacy, fehlgeschlagene, partielle oder
Unknown-Ausführungen sind keine Erfolgsbelege. Ein Process-Resultat bedeutet nur,
dass die Ausführung beobachtet wurde, niemals dass ein Test bestanden hat.

Die Rückmeldung enthält ausschließlich Core-Metadaten, keine Quellen, Pfade,
Modellnotizen oder Prozessausgaben. Sie ist an Projekt/Worktree, Run, Goal,
Ledgerrevision und aktuellen Snapshot gebunden. Sie beschreibt die letzte
Runaktion, nicht unbelegt eine Aktion des aktuellen Schritts. Der Compiler prüft
die Runbindung des aktuellen Schrittversuchs und den tatsächlich geladenen
Snapshot erneut. Änderungen der Ledgerrevision verlangen eine neue Rekonstruktion.

Context-Policy V6 zählt diese unverzichtbaren Metadaten im bestehenden Goal/Ledger-
Anker vor optionaler Retrievalverteilung und nimmt sie in den Kontextdigest auf.
Der aktuelle Verifikationsstatus kommt separat aus dem Task Ledger. Nach einem
Patch kann das Modell weitere notwendige Änderungen oder die geplante Verifikation
anfordern. Es gibt weder automatische Tests nach jedem Teilpatch noch eine
Umdeutung ungültiger No-op-Aktionen in ausführbare Prüfaufträge.

Bestehende Quellinvalidation, Einzelrepair, Approval, Hash-/Snapshot-/Policygrenzen,
Runbudgets und echte Step-/Acceptance-Verifikation bleiben unverändert. Keine neue
Persistenz, Migration, Produktabhängigkeit, Netzwerkgrenze oder Indexstruktur entsteht.
Der Harness-Testtreiber verwendet die bereits vorhandene Workspace-Tokio-Version
auch direkt als Dev-Abhängigkeit, um wie die Produktion eine besitzende Laufzeit
mit Timer bereitzustellen. Fehlt eine Laufzeit, liefert der Use Case einen typisierten
Fehler; Timertests und Produktion umgehen den Timeout nicht.
Diese Rückmeldung ersetzt weder RunMemory noch Quellenbelege und beweist keine
allgemeine Modellzuverlässigkeit.

## Nachweis

Ein zunächst roter echter Patch/Approval/Index/libSQL-Vertrag verlangt die
Rückmeldung im nachfolgenden Kontext. Wiederaufbau aus neu geöffnetem Store,
falsche/stale Bindungen, fehlende/mehrdeutige Journal-Attempt-Paare, Unknown,
Process-ohne-Testbeweis, begrenzte Historie und unveränderte 8k/2k-Reserven werden
unabhängig geprüft. Anschließend vollständige lokale Rust-Gates und Nachtests des
unveränderten öffentlichen Fixtures mit den freigegebenen Modellen.

Referenzen: [ADR-0019](0019-durable-mutation-reconciliation.md),
[ADR-0087](0087-agentaktionen-ohne-modellstatusnotiz.md),
[Plan 10](../plans/10-RESEARCH_WORK_STATE.md).
