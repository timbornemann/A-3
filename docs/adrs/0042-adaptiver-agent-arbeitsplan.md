# ADR-0042: Adaptiver Agent-Arbeitsplan

- Status: Angenommen
- Datum: 2026-09-04
- Ergänzt: ADR-0010, ADR-0013, ADR-0038, ADR-0041
- Ersetzt: ausschließlich die bisher nicht produktiv geschlossene Behandlung des Zustands
  `Replan` sowie die Materialisierung eines normalen Conversation-Plans als einen einzigen
  Task-Ledger-Schritt

## Kontext

A^3 besitzt bereits einen dauerhaften Goal Contract, ein verifiziertes Task Ledger, endliche
Controllerzustände und eine append-only Replan-Historie. Die Conversation-Vorbereitung erzeugte
jedoch nur Markdown. Normale Pläne wurden daraus als ein einziger großer Ledger-Schritt
materialisiert; nur ausgewählte Slash-Command-Pläne wurden anhand ihrer obersten Listenpunkte
geteilt. Ein produktiver Run beendete sich außerdem beim Eintritt in `Replan`, obwohl die Domain
bereits sichere Planrevisionen unterstützt.

Damit konnte ein Modell innerhalb eines Schritts zwar wiederholt suchen, lesen, ändern und
prüfen, aber keinen verständlichen Arbeitsplan schrittweise abarbeiten und nach einem neuen Befund
fortschreiben. Ein offener ReAct-Loop oder eine vom Modell direkt mutierbare Todo-Liste würde die
Ressourcen-, Evidence- und Berechtigungsgrenzen aus ADR-0010 und ADR-0013 verletzen.

## Entscheidung

### Arbeitsplan

- Jede freigabefähige Plan- oder Agent-Antwort enthält einen geordneten, begrenzten Abschnitt
  `Implementation Changes` und einen geordneten `Test Plan`. Der Core kompiliert daraus vor der
  Task-Erzeugung einen `AgentWorkPlan` mit höchstens 64 atomaren Schritten.
- Jeder Schritt besitzt ein beabsichtigtes Ergebnis, eine öffentliche Begründung, mindestens eine
  erwartete Evidence-Art, eine geschlossene Verifikationsabsicht und explizite Abhängigkeiten. Der
  in der Session sichtbare Markdown-Plan bleibt Präsentation; ausschließlich der validierte
  Arbeitsplan wird in Goal Contract und Task Ledger materialisiert.
- Der Core erzeugt Task-, Step-, Verification- und Run-Identitäten. Das Modell kann keine dieser
  Identitäten, keine Verification-Evidence und keine Berechtigung festlegen.
- Implementierungsschritte werden in Planreihenfolge abhängig. Testschritte folgen den
  Implementierungsschritten und werden bevorzugt an einen aktuell manifestbelegten Testbefehl
  gebunden. Fehlt ein geeigneter automatischer Check, bleibt die bestehende explizite
  UserConfirm-Verifikation sichtbar.

### Endlicher Ausführungs- und Replan-Regelkreis

- Der produktive Ablauf bleibt `Execute → Verify → Execute/Done`. Genau eine typisierte Aktion
  wird pro Modellturn ausgewählt, und höchstens eine Mutation besitzt den Worktree-Lease.
- `RequestReplan` beendet den aktiven Step-Versuch atomar als blockiert und wechselt direkt von
  `Execute` nach `Replan`. Ein Replan ohne abgeschlossenen aktiven Versuch ist unzulässig.
- In `Replan` erzeugt der Core eine unmittelbare neue Ledger-Revision. Der blockierte oder
  fehlgeschlagene Schritt und seine noch offenen Nachfolger werden nicht umgeschrieben, sondern
  als Historie pensioniert. Eine neue Lokalisierungsaufgabe sowie frische Ersatzschritte werden
  mit neuen Identitäten eingefügt.
- Die Lokalisierungsaufgabe macht die konkrete Evidence-Lücke zum nächsten sichtbaren Todo. In ihr
  darf der bestehende Agent erneut suchen und inspizieren, bevor er eine Änderung auswählt.
  Anschließend werden die ersetzten Implementierungs- und Testschritte normal verifiziert.
- Planrevision, Ledger-Anker und Run-Event werden per Compare-and-Swap gemeinsam persistiert.
  Danach durchläuft derselbe Run erneut `Localize → Plan → Execute`.
- Automatische Replans sind pro Run fest begrenzt. Wiederholte gleiche Fehler oder ausgeschöpfte
  Controllerbudgets enden mit einem belegten, nicht erfolgreichen Zwischenstand.

### Nutzerentscheidungen

- Eine fachliche Richtungsentscheidung ist keine Policy-Freigabe. Der Agent darf einen Blocker
  mit verständlichem Grund melden; die Session wechselt dann in ihren vorhandenen menschlichen
  Haltepunkt `AwaitingUser` und behält Goal, Ledger, Quellen und bereits verifizierte Schritte.
- Innerhalb des bestätigten Ziels darf der Agent selbst neu planen. Eine Erweiterung von Ziel,
  Akzeptanzkriterien, Nicht-Zielen, externen Abhängigkeiten oder Berechtigungen benötigt weiterhin
  eine neue Nutzerentscheidung beziehungsweise Planfreigabe.
- Öffentliche Arbeitsnotizen enthalten Ziel, Befund, Evidence-Lücke und nächsten Schritt. Sie
  sind Präsentationsdaten, kein Chain-of-Thought und keine Ausführungsautorität.

### Darstellung

- Der aktuelle Arbeitsplan wird aus dem autoritativen Task Ledger projiziert. Sichtbar sind
  Planrevision, Schritte, Zustand, beabsichtigtes Ergebnis, Begründung, erwartete Evidence und
  Verifikationsart.
- Pensionierte Schritte bleiben in Ledger und Run-Journal auditierbar. Die normale Oberfläche
  zeigt den aktuellen Plan in Ausführungsreihenfolge und kennzeichnet eine neue Ledger-Revision
  verständlich als Anpassung nach einem neuen Befund.
- Chat und Agentenlauf-Seitenleiste verwenden dieselbe Projektion und starten keine eigene Arbeit.

## Folgen

- Normale Agent-Aufträge werden als mehrere kleine, einzeln prüfbare Todos ausgeführt.
- Ein unerwarteter Befund oder wiederholter Verifikationsfehler beendet den produktiven Lauf nicht
  mehr automatisch, sondern erzeugt einen begrenzten, auditierbaren Replan.
- Kleine lokale Modelle erhalten jeweils nur Goal, aktuellen Step, offenen Befund und aktuelle
  Evidence statt eines wachsenden freien Plans.
- Die deterministische Markdown-Kompilierung ist eine vorwärtskompatible Eingangsgrenze. Ein
  späteres vollständig typisiertes Plan-Ausgabeschema kann denselben `AgentWorkPlan` erzeugen,
  ohne Goal-, Ledger- oder Run-Invarianten zu verändern.

## Nicht entschieden

- Kein Multi-Agent-System und keine parallele Mutation.
- Kein unbegrenzter Recherche-, Replan- oder Ausführungsloop.
- Keine freie Shell, Paketinstallation, Netzwerkfreigabe oder autonome Git-Veröffentlichung.
- Keine Anzeige oder Persistenz von Chain-of-Thought, Providertranskripten oder Rohprompts.
- Keine rückwirkende Änderung bereits abgeschlossener Schritte.

## Compliance

- Domain-Tests prüfen Planbegrenzung, Abschnittserkennung, Reihenfolge und leere Pläne.
- Controller-Tests prüfen `Execute → Replan`, einen geschlossenen aktiven Versuch und weiterhin
  genau eine Aktion pro Turn.
- Executor- und Storage-Tests prüfen atomare Ledger-Revisionen, neu eingefügte Schritte,
  Weiterarbeit nach Replan und Wiederaufnahme ohne doppelte Mutation.
- Frontendtests prüfen mehrere Todos, Planrevisionen, aktive Zustände und die verständliche
  Kennzeichnung eines angepassten Arbeitsplans.

## Referenzen

- [ADR-0010](0010-single-controller-state-machine.md)
- [ADR-0012](0012-safe-tools-and-approval-policy.md)
- [ADR-0013](0013-goal-contract-ledger-and-event-journal.md)
- [ADR-0019](0019-durable-mutation-reconciliation.md)
- [ADR-0020](0020-agent-runtime-ownership-and-pause.md)
- [ADR-0038](0038-agentische-mehr-runden-recherche.md)
- [ADR-0041](0041-sichere-moduswechsel-und-dauerhafte-nachrichtenwarteschlange.md)
- [Memory und Context](../MEMORY_AND_CONTEXT.md)
- [Security und Execution](../SECURITY_AND_EXECUTION.md)
- [Agent Harness](../plans/04-MEMORY_AND_AGENT_HARNESS.md)
- [Editing und Verification](../plans/05-EDITING_AND_VERIFICATION.md)
