# ADR-0093: Begrenzte Originalquellen im normalen Agentenkontext

Status: Accepted

Datum: 2026-09-07

Entscheider: Tim Bornemann (Vorabannahme der mit Plan 10 verbundenen ADRs)

## Befund

Die normale Task Lens liefert Dateirevisionen, Deklarationen und Quellbereiche,
aber keine Funktionskörper. Ohne einen vom Modell gewählten Datei-Read enthält
der erste Implementierungsturn deshalb keinen Originalcode. Metadaten sind
keine hinreichende Grundlage für eine genaue Änderung. Ein separat untersuchter
Action-Wire-Vergleich mit eingeblendetem Original ersetzt diesen Produktpfad nicht.

## Entscheidung

ContextCompilerPolicy V7 materialisiert ausgewählte Quellen über den bestehenden
AgentSourceReader-Port. Der Composition Root muss diesen Port bereitstellen;
der Context Compiler erhält keinen direkten Dateisystemzugriff. Die bestehende
Task Lens bleibt der einzige Retrieve-/Rank-Pfad. Die Kontextmaterialisierung
ist keine Modellaktion, kein zusätzlicher Rechercheversuch und keine Verifikation.

Pro normalem Compile werden höchstens zwei unterschiedliche Dateirevisionen
mit jeweils höchstens 64 vollständigen Zeilen und der bestehenden 12-KiB-
Seitengrenze gelesen. Eine kooperative Gesamtabbruchfrist von zehn Sekunden
und der äußere Cancellation-Token gelten für alle Reads zusammen. Tatsächliche
Originalmarker jüngster Tool-Ergebnisse haben Vorrang vor den aktuellen
Task-Lens-Treffern. Preview-Text wird niemals in einen Quellbeleg umgedeutet.

Der Reader prüft kanonischen Pfad, Workspacegrenze, Dateityp, Geheimniskandidaten
und vollständigen Inhaltshash erneut. Stale Quellen und ungültige Reader-
Projektionen sperren den Kontext. Nicht lieferbare oder gesperrte optionale
Quellen werden nicht aufgenommen; der Pack bleibt ausdrücklich unvollständig.
Abbruch und Fristablauf werden vor dem Provideraufruf weitergegeben. Es gibt
keinen Fallback auf alte Originalbytes.

Originale stehen vor optionaler Historie und Metadaten, aber nach den zwingenden
Goal-, Ledger-, Research- und Memory-Ankern. Sie dürfen höchstens die Hälfte
des danach verfügbaren CodeAndEvidence-Budgets belegen. Nur eine vollständige
Reader-Seite, deren Header und Inhalt zusammen hineinpassen, wird aufgenommen.
Datei, Hash, tatsächlicher Bereich, Fortsetzung und begrenzter Originalinhalt
werden gemeinsam gerendert und über den bestehenden Kontextdigest gebunden.
Historische Original-Toolpreviews werden im normalen Kontext durch reine
Metadaten ersetzt; nur frisch materialisierte Originale gelten als geliefert.

Bei Replan-Lokalisierung oder Replan-Analyse ist die automatische Materialisierung
vollständig ausgeschaltet. Dort bleiben die journalisierten Originalpakete,
vier Reads und Analyseversuche nach ADR-0092 allein maßgeblich. Tool-, Provider-,
Repair-, Genehmigungs- und Mutationsverträge sowie unabhängige Tests bleiben
unverändert. Es entstehen weder neue persistierte Source-Bytes noch ein neuer
Index, eine Datenbankmigration, ein Dienst oder ein Netzwerkzugriff.

## Konsequenzen und Nachweis

Gleicher Zustand, aktuelle Quellen und gleiches Profil ergeben denselben Pack.
Eine Änderung der Quellen verhindert die Wiederverwendung des alten Packs.
Nicht passende Seiten bleiben eine sichtbare Lücke; der Agent kann weiterhin
einen engeren regulären Datei-Read wählen. Zwei Ausschnitte ersetzen weder eine
vollständige Recherche noch eine Implementierungsverifikation.

Erforderlich sind Regressionen für echte Originalbytes und genaue Tokenkosten,
Priorität, Deduplication, fehlendes Budget, Reader-Fehler, Cancellation und
Replan ohne automatische Reads. Die reale Coding-Fixture verlangt Originalcode
bereits im produktionsgleichen Preflight. Vollständige Qualitätsgates und
sequenzielle Modellläufe messen den tatsächlichen Nutzen; eine Verbesserung
der Modellzuverlässigkeit wird nicht aus einem grünen Compiler-Test abgeleitet.

Referenz: [Plan 10](../plans/10-RESEARCH_WORK_STATE.md).
