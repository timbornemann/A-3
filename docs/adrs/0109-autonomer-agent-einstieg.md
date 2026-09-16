# ADR-0109: Autonomer Agent-Einstieg ohne Nutzer-Planfreigabe

Status: Accepted

Datum: 2026-09-16

Entscheider: Tim Bornemann

Freigabe: ausdrücklicher Auftrag zur Umsetzung des Plans „Autonomer Agent-Einstieg
ohne Planfreigabe“ einschließlich der zugehörigen Architekturentscheidung.

Supersedes: ausschließlich die Pflicht zur Nutzer-Planfreigabe beim Zielmodus
`Agent` aus [ADR-0033](0033-chatbasierter-agent-workspace.md),
[ADR-0041](0041-sichere-moduswechsel-und-dauerhafte-nachrichtenwarteschlange.md)
und [ADR-0047](0047-verbindlicher-recherchearbeitsstand.md). Die drei Capability-
Envelopes, die exakte `ImplementPlan`-Bindung, `ResearchWorkState`, `AgentWorkPlan`,
Goal Contract, Task Ledger, Policy, Approval, Verification und der Controller aus
[ADR-0010](0010-single-controller-state-machine.md) bleiben unverändert.

## Kontext

Ask, Plan und Agent sind getrennte Capability-Grenzen. Plan erzeugt einen
reviewbaren Plan und übergibt ihn erst nach „Plan umsetzen“. Agent-Nachrichten ohne
ungebrochene Agent-Kontinuität wurden jedoch intern zu Plan umgeschrieben und
endeten in `AwaitingPlanReview`. Damit war der direkte Agent-Einstieg doppelt
zum Plan-Modus, und die vorhandene Agent-Materialisierung im Conversation-Job
wurde nur bei fortgesetzter Agent-Kontinuität erreicht.

Kleine lokale Modelle brauchen weiterhin Recherche und einen Core-validierten
Arbeitsplan. Sie brauchen keine zweite menschliche Freigabe, wenn der Nutzer
ausdrücklich Agent und nicht Plan gewählt hat.

## Entscheidung

- `targetMode=Agent` bleibt Agent. Der Core schreibt Agent nicht zu Plan um.
- Nach `sufficient` Recherche und gültigem `AgentWorkPlan` materialisiert derselbe
  Conversation-Job Goal Contract, Task Ledger und Run. Die Session geht nicht in
  `AwaitingPlanReview`.
- Der sichtbare Markdown-Plan bleibt Präsentation. Ausführbar ist ausschließlich
  der Core-validierte Arbeitsplan.
- `ImplementPlan` akzeptiert weiterhin nur Plan, `AwaitingPlanReview`, die exakte
  Planrevision und den aktuellen Index. Das ist der einzige Weg, einen bereits
  geprüften Plan zu übergeben. Eine Agent-Nachricht setzt keinen wartenden Plan um;
  sie startet ein unabhängiges Work Item.
- Ein Rückwechsel zu Ask oder Plan verwirft die Ausführbarkeit des früheren
  Agent-Plans. Der nächste Agent-Auftrag erzeugt einen neuen Handoff und ein neues
  Ledger, aber keine Nutzer-Planfreigabe.
- `QUESTION:`, Budgetfortsetzung, Policy, Approval, Reindex, Verification und eine
  Erweiterung von Ziel, Akzeptanzkriterien, Nicht-Zielen oder Berechtigungen bleiben
  menschliche Haltepunkte.
- Unterbrochene autonome Agent-Recherche bleibt ein Conversation-Interrupt. Der
  Rollback `AgentStartInterrupted` → Plan + `AwaitingPlanReview` gilt nur für den
  Button-Pfad `ImplementPlan`.
- Das IPC-Feld `requiresPlanReview` ist für Agent dauerhaft `false`. Das Outcome
  `requiresPlanReview` bleibt im V4-Enum für Decoderkompatibilität und wird vom
  aktuellen Core nicht erzeugt.

## Konsequenzen

### Positiv

- Plan bleibt der kollaborative Review-Pfad.
- Agent startet direkt mit derselben begrenzten Recherche und demselben Harness.
- Kleine Modelle erhalten weiter Goal, aktuellen Schritt und Evidence statt eines
  unbestätigten Freitextplans.

### Negativ

- Schlechte autonome Pläne können ohne Review-Klick in die Ausführung gehen. Die
  Gegenmaßnahmen bleiben Policy, Approval, Verification und der endliche Replan.

### Risiken und Gegenmaßnahmen

- Verwechslung von Planübergabe und neuem Agent-Auftrag — nur „Plan umsetzen“
  materialisiert die sichtbare Planrevision; eine Agent-Nachricht erzeugt neue Arbeit.
- Verdeckte Mutation — Materialisierung ändert keine Policy- oder Approval-Grenze.
- Veraltete Queue-Decoder — `requiresPlanReview` bleibt im Wire, wird aber nicht
  mehr als Agent-Halt interpretiert.

## Verworfene Alternativen

- Agent-Nachricht setzt den sichtbaren Plan um — vermischt Plan-Review und neuen
  Auftrag und widerspricht der expliziten Nutzerentscheidung.
- Recherche im Agent-Modus überspringen — widerspricht evidenzgebundener Arbeit
  und ADR-0042.
- Historische ADRs umschreiben — verboten; nur ein neues ADR darf ersetzen.

## Compliance

- Domain- und Desktoptests prüfen, dass Agent-Ziele Agent bleiben, Plan-Ziele in
  `AwaitingPlanReview` enden und `ImplementPlan` nach einem Agent-Start Conflict ist.
- Queue-Dispatch startet eine vorgemerkte Agent-Nachricht als Agent.
- Frontendtests zeigen keinen Agent-Chip „Nach Planfreigabe“ und akzeptieren das
  Legacy-Outcome weiter.
- Recherche-Fixtures für Ask, Plan und Agent-Vorbereitung bleiben read-only.

## Referenzen

- [ADR-0033](0033-chatbasierter-agent-workspace.md)
- [ADR-0041](0041-sichere-moduswechsel-und-dauerhafte-nachrichtenwarteschlange.md)
- [ADR-0042](0042-adaptiver-agent-arbeitsplan.md)
- [ADR-0047](0047-verbindlicher-recherchearbeitsstand.md)
- [Produktanforderungen](../PRODUCT_REQUIREMENTS.md)
- [Architekturregeln](../ARCHITECTURE_RULES.md)
- [Plan 14](../plans/14-AUTONOMOUS_AGENT_START.md)
