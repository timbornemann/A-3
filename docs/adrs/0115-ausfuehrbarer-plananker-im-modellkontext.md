# ADR-0115: Ausführbarer Plananker im konfigurierten Modellkontext

Status: Accepted  
Datum: 2026-09-22  
Entscheider: Tim Bornemann

Freigabe: Der Auftraggeber hat alle für den Agentenumbau erforderlichen ADRs ausdrücklich
angenommen und verlangt, dass der reale Recherche-zu-Agent-Pfad auch mit dem konfigurierten
`gpt-5.6-luna` in einem leeren Repository tatsächlich Dateien erzeugt und verifiziert.

Ergänzt: [ADR-0077](0077-vollstaendige-agent-anker-im-konfigurierten-kontext.md),
[ADR-0094](0094-zweistufige-agentaktionen-im-kontrollierten-vergleich.md),
[ADR-0095](0095-gefuehrte-nachentscheidung-nach-angewendeten-aenderungen.md),
[ADR-0111](0111-leerer-index-bestandsaufnahme.md) und
[ADR-0114](0114-chatgebundene-agentenausfuehrung.md). Vollständiges Ziel, aktueller Schritt,
Verifikationszustand und Rechercheevidenz bleiben Pflichtanker. Dieses ADR ändert weder
Kontextreserven noch Sicherheits- oder Freigabegrenzen. Es ersetzt ausschließlich deren frühere
Annahme, dass die zweistufige, quellengelenkte Aktionserzeugung nur ein Testvergleich und
`SingleAction` weiterhin der Produktstandard ist.

## Kontext

Die bisherigen Live-Prüfungen trennten Recherche und Coding: Ein Test erzeugte mit dem realen
Modell einen Greenfield-Plan, ein anderer startete Coding mit einem handgebauten kleinen Ledger
ohne Recherche-Handoff. Der produktive Pfad materialisiert dagegen jeden Top-Level-Planpunkt
sowohl als präzisen Ledger-Schritt als auch nochmals als eigenes Muss-Akzeptanzkriterium.

Beim reproduzierten Hello-World-Auftrag entstanden so vierzehn Schritte und vierzehn inhaltlich
identische Goal-Wiederholungen. Der erste Coding-Turn belegte mit der konservativen Profilzählung
7.139 Einheiten für den unveränderlichen Aktionsvertrag, 5.759 für Goal/Ledger und 828 für den
Recherche-Handoff. Nach Ausgabe- und Sicherheitsreserve passten die Pflichtbereiche nicht in das
konfigurierte 16.384er Fenster. A^3 brach vor jedem Modellaufruf mit
`Context(Budget(AllocationOverflow))` ab und projizierte die Ursache fälschlich als allgemeine
fehlende Laufzeitfähigkeit. Deshalb konnte weder ein Patch noch eine Datei entstehen.

## Entscheidung

- Der Goal Contract eines automatisch materialisierten Rechercheplans enthält genau ein
  zielweites Muss-Kriterium: Der unveränderte Nutzerauftrag ist gemäß dem freigegebenen Plan
  vollständig umgesetzt und jeder aktive Planschritt hat seine eigene typisierte Verifikation
  bestanden.
- Alle präzisen Planergebnisse bleiben verlustfrei in geordneten `TaskLedger`-Schritten erhalten.
  Für einen nachweislich leeren Startindex fasst der Compiler benachbarte Ergebnisse nur bis zu
  einer festen Textgrenze zu überschaubaren Änderungsslices zusammen. Die Testplanergebnisse
  werden zuerst als lokale automatisierte Tests implementiert; danach folgt genau ein
  nachgelagert gebundener operationaler Testschritt. Nur dieser letzte abhängige Schritt trägt
  das zielweite Kriterium. Seine Vorgängerkette kann ihn erst nach frischer erfolgreicher
  Step-Verifikation aller früheren aktiven Schritte erreichen; der Acceptance-Verifier verlangt
  anschließend aktuelle Evidence für das zielweite Kriterium, bevor `Done` möglich ist.
- Das zielweite Kriterium ersetzt ausschließlich die redundante 1:1-Kopie jedes Schritttexts in
  den Goal-Anker. Nutzerziel, aktueller Schritt, Verifikationsspezifikation, Ledgerstatus,
  Recherche-Handoff und aktuelle Originale bleiben nach den bisherigen Regeln vollständig oder
  der Turn scheitert geschlossen.
- Der kombinierte reale Prüfpfad führt Recherche, Planmaterialisierung, persistente
  Session-Verknüpfung und Produktions-Executor in demselben leeren Worktree aus. Getrennte
  Recherche- und Coding-Smokes gelten nicht als Nachweis dieses Übergangs.
- Fehler vor einer Modellaktion behalten ihre geschlossene Ursache. Insbesondere erhält ein
  nicht passender Pflichtkontext einen eigenen nutzergeeigneten Kapazitätshaltegrund statt der
  allgemeinen Meldung für Modell-, Speicher- oder Werkzeugausfall.
- Der Desktop-Composition-Root verwendet `SourceGuided` als Produktstandard. Nach tatsächlich
  geliefertem aktuellen Original oder ausgeschöpftem Vier-Read-Budget wählt das Modell zunächst
  nur `change`, die bereits geplante `verify`-Aktion oder einen noch fehlenden Evidence-Read.
  Anschließend wählt es einen geschlossenen Aktionsarm und füllt nur dessen variable Argumente.
  Der Core ergänzt Run-, Worktree-, Snapshot-, Step- und Verification-Anker, gleicht Add/Update
  sowie erwartete Hashes mit dem aktuellen Index ab und lässt die zusammengesetzte V5-Aktion
  erneut durch den unabhängigen Decoder. Alle Teilaufrufe teilen eine Deadline, ein Budget und
  genau einen Repair.
- Eine erfolgreiche aktuelle Original-Dateiseite beendet beim Replan die gesonderte
  Lokalisierungsanalyse und geht in denselben quellengelenkten Arbeitsentscheid über.
  Suchmetadaten allein genügen nicht. Ein redundanter Read einer bereits vollständig gelieferten
  Seite verbraucht keinen Repair, sondern wird deterministisch wieder auf Änderung oder geplante
  Verifikation begrenzt. Insgesamt bleiben höchstens vier echte Reads pro Schritt erlaubt.
- `ContextCompilerPolicyVersion::V9` hält nur Handoff-Identität, Warnhinweis sowie IDs und Status
  der Pflichtfragen im untrunkierbaren Bereich. Vollständige Outcomes und Resultate bleiben
  evidencegebunden, aber optional gerankt; Goal, aktueller Ledger-Schritt, Verifikation und
  Research-Freshness bleiben unverändert verpflichtend.
- Ein Deferred-Testschritt darf erst nach einer Änderung abgeschlossen werden, wenn der frische
  Index tatsächlich einen bevorzugten lokalen Command enthält. Fehlt er oder schlägt ein
  gebundener Test fehl, erzeugt der Core einen getrennten Diff-verifizierten Reparaturschritt und
  danach den abhängigen Retry. Die technische einmalige Deferred→Command-Bindung ist kein
  fachlicher Wiederholungs-Replan.
- Manifestfreie Python-Projekte erhalten einen direkten Standardbibliotheks-`unittest`-Command.
  Liegen Testdateien in mehreren Wurzeln, bindet ein einzelner shell-freier argv-Aufruf alle
  höchstens 16 aktuellen Testrevisionen statt das Projekt als unverifizierbar zu verwerfen.
  Test/Build laufen höchstens 120 Sekunden, Lint/Format höchstens 60 Sekunden. Ein sauberer
  Timeout beendet den ganzen Prozessbaum, veröffentlicht einen vollständigen frischen Index und
  führt in den normalen evidenzgebundenen Reparaturpfad; Cancellation oder unbestätigte
  Prozessbeendigung bleiben reconciliation-pflichtig.
- Es werden weder Kontextfenster künstlich vergrößert noch Ausgabe-/Sicherheitsreserven gesenkt.
  Repositoryinhalt bleibt untrusted; Patch und Prozess benötigen weiterhin die exakten
  bestehenden Freigaben.

## Konsequenzen

Der aktuelle Schritt bleibt detailliert, während noch nicht aktuelle Schritte nicht mehrfach im
selben Turn stehen. Kleine Modelle erhalten dadurch einen ausführbaren Kontext, ohne Ziel- oder
Evidence-Autorität zu verlieren. Ein breiter Plan kann weiterhin mehrere serielle Slices und
Freigaben erzeugen, bleibt aber durch die feste Slice-Grenze auch für kleine Modellfenster
ausführbar. Leere Projekte benötigen keine erfundenen Bestandsdateien: Die negative
Indexbestandsaufnahme trägt den Entwurf, anschließend entstehen reale Dateien und erst danach
Command- und Testevidence. Bestehende Projekte erhalten dagegen aktuelle Originalseiten und
Hashanker vor jeder Änderung.

## Compliance

- Ein Regressionstest bindet einen real erzeugten Greenfield-Rechercheplan an denselben
  Produktions-Executor und erreicht vor jeder Mutation eine konkrete Patch-Freigabe.
- Der vollständige opt-in Luna-Lauf genehmigt nur geschlossene Projektpfade und lokale
  Standardbibliothekschecks, endet erst bei `Done` und prüft Dateien sowie HTTP-Verhalten
  unabhängig vom Modell.
- Unit-Tests prüfen die gemeinsame Criterion-Abbildung und die geschlossene Klassifikation eines
  Kontextbudgetfehlers.
- Regressionen prüfen leere Inventare, V9-Handoffbudget, aktuelle Originalpriorität,
  redundante Reads, Add/Update-/Hash-Rebinding, Reparatur-vor-Retry, mehrere Python-Testwurzeln,
  Command-Timeouts und Windows-Kindprozessbeendigung.
- Die vollständigen Rust- und Frontend-Gates bleiben unverändert.

## Referenzen

- [ADR-0010](0010-single-controller-state-machine.md)
- [ADR-0012](0012-safe-tools-and-approval-policy.md)
- [ADR-0047](0047-verbindlicher-recherchearbeitsstand.md)
- [ADR-0077](0077-vollstaendige-agent-anker-im-konfigurierten-kontext.md)
- [ADR-0111](0111-leerer-index-bestandsaufnahme.md)
- [ADR-0112](0112-nachgelagerte-greenfield-verifikation.md)
- [ADR-0114](0114-chatgebundene-agentenausfuehrung.md)
- [Plan 18](../plans/18-END-TO-END-AGENT-EXECUTION.md)
