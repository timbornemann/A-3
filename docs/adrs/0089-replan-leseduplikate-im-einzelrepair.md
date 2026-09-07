# ADR-0089: Replan-Leseduplikate im Einzelrepair

Status: Accepted

Datum: 2026-09-07

Entscheider: Tim Bornemann (Vorabannahme der mit Plan 10 verbundenen ADRs)

## Befund

Der unveränderte echte Granite-Agentlauf endet nach zwei fehlgeschlagenen Tests
mit `ReplanReadRejected(RepeatedRead)`, Failed Sequenz 26. Der vorgeschlagene Read
ist strukturell korrekt, hat aber bereits einen dauerhaften Access-Versuch. Die
Prüfung erfolgt bislang erst nach Abschluss der primären Aktionsdecodierung und
ihres möglichen Repairs. Ein Duplikat erhält deshalb keine korrigierbare Rückmeldung,
obwohl noch ein anderer erlaubter Read und der primäre Einzelrepair verfügbar sein
können. Der frühere Fehlername `InvalidReadResult` verdeckte diesen Unterschied.

## Entscheidung

Während offener Replan-Lokalisierung prüft die Aktionszulassung nach unverändert
strikter Schema-/Ankerprüfung zusätzlich den bestehenden unveränderlichen
ReplanResearchCheckpoint. Genau `RepeatedRead` wird als geschlossener
`replan_read_repeated`-Fehler in denselben einzigen Repair aufgenommen, der bereits
Struktur-, Anker- und Snapshotkonflikte behandelt. Der Hinweis fordert einen anderen
relevanten read-only Leseauftrag, spiegelt aber weder Rohantwort noch private Pfade,
Queries oder Quelleninhalt zurück. Der Core ersetzt den vorgeschlagenen Read nicht.

Primärantwort und korrigierte Antwort werden gegen denselben Run-/Schritt-/Snapshot-
gebundenen Prüfstand zugelassen. Ein wiederholtes Duplikat bleibt terminal. Ein
nach einem Strukturfehler korrigiertes Duplikat eröffnet keinen zweiten Repair.
Ein gültiger anderer Read läuft durch dieselbe Policy, Safe-Reader-, Timeout-,
Cancellation- und Journalgrenze und zählt als tatsächlicher Read des bestehenden
Budgets. Eine Korrektur ist weder ein Untersuchungsergebnis noch Testverifikation.

Die bestehende Vier-Read-Grenze, gespeicherten kanonischen Accesskeys und ihre
Wiederanlaufsemantik bleiben unverändert. Ein ausgeschöpftes Readbudget erhält
keinen neuen Repair durch diese Regel. Die nachgelagerte Replan-Read-Prüfung bleibt
als unabhängige Grenze erhalten. Eine bereits fertige Replan-Untersuchung unterliegt
nicht erneut dem Lokalisierungs-Duplikatfilter. Nicht-Replan-Turns sind unverändert.

Keine Protokollmigration entsteht: Lokalisation bleibt AgentAction V4, Analyse
Research V5, normale Agentaktionen V5 und Ask/Plan Research V7. Kein neuer Index,
keine Abhängigkeit, keine Kontext-/Outputvergrößerung und kein zusätzliches
Repair-/Tool-/Freigabebudget werden eingeführt. Dies erweitert das bereits in
[ADR-0085](0085-patch-snapshotkonflikte-im-einzelrepair.md) verwendete Zulassungsmuster,
ohne dessen Patchgrenzen zu verändern.

## Nachweis

Tests müssen belegen: Duplikat→anderer Read führt genau einen Toolaufruf aus;
Duplikat→Duplikat und Strukturfehler→Duplikat bleiben nach zwei Modellantworten ohne
Toolwirkung terminal. Ausgeschöpftes Budget bleibt gesperrt, Wiederanlauf erneuert
weder Accesskeys noch Zähler. Schema-only-Legacy-, Snapshot-, Mutations-, echte
Storage- und Kontextverträge bleiben bestehen. Die öffentliche Agent-Fixture wird
mit Granite und Luna sowie dem vorhandenen Qwen-8k-Profil nachgetestet; beobachtete
Restfehler werden getrennt dokumentiert, nicht als Abschluss gewertet.

Referenzen: [ADR-0048](0048-rungebundene-replan-recherche.md),
[Plan 10](../plans/10-RESEARCH_WORK_STATE.md).
