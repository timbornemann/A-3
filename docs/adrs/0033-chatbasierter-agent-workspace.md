# ADR-0033: Chatbasierter Agent Workspace

Status: Accepted  
Datum: 2026-08-29  
Entscheider: Tim Bornemann

## Kontext

Der bisherige Agent-Bereich verteilt Goal, Plan, Aktivität, Inspektion, Freigaben und Recovery auf
mehrere interne Teilflächen. Die vorhandenen Core-Fähigkeiten sind dadurch zwar erreichbar, aber
nicht als zusammenhängender Arbeitsablauf verständlich. A^3 benötigt einen vertrauten,
chatbasierten Einstieg, ohne seine deterministische Zustandsmaschine, den Task Ledger oder die
zentralen Sicherheitsgrenzen durch einen offenen Chat-Loop zu ersetzen.

## Entscheidung

- `Agent` ist genau eine projektgebundene Seite mit einer ein- und ausklappbaren Sessionleiste,
  einer zentralen Unterhaltung und einem ein- und ausklappbaren Inspector für Fortschritt,
  Änderungen und Review.
- Die drei Startmodi bilden geschlossene Capability-Envelopes: `Ask` darf nur lesen und berichten,
  `Plan` darf nur recherchieren, Rückfragen stellen und unveränderliche Planrevisionen erzeugen,
  `Agent` darf ausschließlich über den vorhandenen Harness, typisierte Werkzeuge, zentrale Policy,
  Approval und Verification mutieren.
- Eine Session ist eine projekt- und worktree-lokale, append-only versionierte
  Präsentationsprojektion. Sie gruppiert Nachrichten und verweist optional auf ein Core-eigenes
  Work Item und dessen `TaskId`. Goal Contract, Task Ledger, Agent Run, Journal, Evidence und
  Approval bleiben alleinige fachliche Autorität.
- Ein Plan wird erst mit seiner exakten sichtbaren Planrevision umgesetzt. `Ask → Plan → Agent`
  ist innerhalb eines Work Items vorwärts möglich; eine Rückstufung erzeugt ein neues Work Item.
- Laufende Modell- und Agentenarbeit gehört dem begrenzten Job Scheduler. Sie unterstützt
  Fortschritt und kooperative Cancellation und wird bei Projektwechsel oder Shutdown beendet.
  Pro Worktree darf weiterhin nur eine mutierende Agentenaktion aktiv sein.
- Layoutpräferenzen enthalten ausschließlich globale, inhaltsfreie Größen und Collapse-Zustände.
  V1 verwendet 264 Pixel für die Sessionleiste und 400 Pixel für den Inspector; die Grenzen liegen
  bei 220–360 beziehungsweise 320–640 Pixel. Schmale Fenster verwenden Drawer statt zusätzliche
  Unterseiten.
- IPC bleibt schmal, versioniert und pfadlos. Die WebView erhält weder Datei-, Shell-, SQL-,
  Provider-, Credential- noch Policy-Autorität. Sessionlöschung entfernt nur die
  Präsentationsinhalte; fachliche Auditdaten bleiben erhalten.
- Gesprächshistorie wird nicht pauschal in Modellkontext kopiert. Der Context Compiler bindet
  aktuelle Goal-, Ledger-, Snapshot- und Evidence-Anker deterministisch; Präsentationstext ist
  niemals Beweis.

## Konsequenzen

### Positiv

- Nutzer starten, verfolgen, prüfen und vertiefen Arbeit in einem vertrauten durchgehenden Ablauf.
- Ask, Planung und Mutation sind sichtbar unterscheidbar und technisch getrennt erzwungen.
- Follow-ups behalten den projektlokalen Zusammenhang, ohne Audit- oder Evidenzautorität in die
  Session zu verschieben.
- Bestehende Activity-, Inspection-, Approval- und Recovery-Projektionen werden wiederverwendet.

### Negativ

- Sessionprojektion und fachlicher Run müssen über stabile IDs und optimistic revisions verbunden
  und bei Fehlern konservativ als getrennte Zustände angezeigt werden.
- Eine Unterhaltung kann mehrere Work Items enthalten; die UI darf deshalb nie aus der letzten
  Nachricht auf den fachlichen Abschluss schließen.

### Risiken und Gegenmaßnahmen

- Verwechslung von Chat und Autorität — jede Session referenziert ausschließlich Core-eigene
  `TaskId`-Anker; Done kommt nur aus Verification und Acceptance.
- Veraltete Planausführung — `ImplementPlan` akzeptiert nur die exakte aktuelle Planrevision und
  Sessionrevision.
- Verdeckte privilegierte Aktionen — alle Mutationen bleiben im Harness und benötigen dieselben
  Policy- und Approval-Prüfungen wie außerhalb der Chatansicht.
- Unbegrenzter Verlauf oder DOM — IPC-Seiten, Entry-Grenzen, serverseitige Suche und begrenzte
  gerenderte Projektionen bleiben verpflichtend.

## Verworfene Alternativen

- Bestehende interne Unterseiten lediglich neu anordnen — beseitigt den fragmentierten Workflow
  nicht.
- Ein allgemeiner LLM-Chat mit direkten Tauri-Datei- oder Shellwerkzeugen — widerspricht ADR-0010,
  ADR-0012 und dem unprivilegierten WebView.
- Sessions als neue fachliche Run-Autorität — würde Goal, Ledger, Journal und Evidence duplizieren
  und widersprüchliche Abschlusszustände ermöglichen.
- Vollständigen Verlauf in jeden Prompt kopieren — ist nondeterministisch, teuer und kann veraltete
  Aussagen gegenüber aktueller Evidence bevorzugen.

## Compliance

- Domain- und Storage-Contracts prüfen Revisionen, Sequenzen, Worktree-Isolation, Tombstones und
  Modustransitionen.
- IPC- und TypeScript-Contracts lehnen unbekannte Felder, Pfade, freie Befehle und veraltete
  Revisionen ab.
- Application- und Desktoptests prüfen Capability-Envelopes, exakte Planfreigabe,
  Scheduler-Eigentum, Projektwechsel und die Bindung an Goal/Ledger/Run.
- Component-, Accessibility- und responsive Browsertests prüfen den vollständigen Ein-Seiten-
  Workflow sowie Größen-, Fokus-, DOM- und Reflow-Grenzen.

## Referenzen

- [ADR-0010](0010-single-controller-state-machine.md)
- [ADR-0012](0012-safe-tools-and-approval-policy.md)
- [ADR-0013](0013-goal-contract-ledger-and-event-journal.md)
- [ADR-0020](0020-agent-runtime-ownership-and-pause.md)
- [ADR-0021](0021-bounded-agent-inspection.md)
- [ADR-0022](0022-task-bound-approval-center.md)
- [ADR-0025](0025-bounded-desktop-rendering-and-lifecycle.md)
- [Plan 06](../plans/06-DESKTOP_PRODUCT.md)
