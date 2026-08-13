# ADR-0020: Agent-Laufzeitbesitz und kooperative Pause

Status: Accepted

Datum: 2026-08-13

Entscheider: Tim Bornemann

## Kontext

ADR-0010 definiert die endliche fachliche Zustandsmaschine eines Agent Runs ohne Zustand
`Paused`. Der gemeinsame Job-Scheduler kennt ebenfalls nur terminale kooperative Cancellation.
Plan 06 verlangt dennoch eine bewusste Pause mit anschließendem Resume, ohne Modell- oder
Toolarbeit im Hintergrund weiterlaufen zu lassen.

Eine Scheduler-Cancellation direkt als fachliches `CancelRequested` zu behandeln würde den Run
terminal auf `Cancelled` setzen und damit eine Fortsetzung ausschließen. Umgekehrt darf eine bloße
UI-Markierung keinen Pause-Checkpoint vortäuschen. H11 markiert bei der Recovery-Inspektion
verlassene In-flight-Toolversuche als `Interrupted`; diese Inspektion darf deshalb niemals gegen
einen noch vom aktuellen Prozess besessenen Worker laufen.

## Entscheidung

- Der Desktop-Composition-Root besitzt genau einen begrenzten `AgentRunManager`. Dieser ist die
  einzige Produktlaufzeit, die Agentenarbeit in den gemeinsamen Scheduler einreichen, pausieren
  oder während Projektwechsel und Shutdown beenden darf.
- Der Manager verwendet einen eigenen, nicht fachlichen Produktlifecycle: `Idle`, `Queued`,
  `Running`, `Pausing`, `Paused`, `Cancelling`, `Succeeded`, `Failed` und `Cancelled`. Diese Werte
  erweitern weder `AgentControllerState` noch den gemeinsamen Scheduler-Automaten und sind keine
  neue persistente Wahrheitsquelle.
- Polling, Recovery-Queries und das Mounten der WebView starten keine Modell- oder Toolarbeit.
  Jeder Versuch entsteht nur aus einer expliziten Start-, Resume- oder Replan-Aktion und bleibt an
  die Core-abgeleitete Task sowie die zu Beginn geprüften Ledgeranker gebunden.
- Pause fordert kooperative Scheduler-Cancellation an. Der Executor darf diese betriebliche
  Cancellation nicht als fachliches `CancelRequested` committen. `Paused` wird erst sichtbar,
  nachdem der Scheduler terminal `Cancelled` ist, der Executor die Cancellation bestätigt hat
  und eine nachgelagerte H11/E8-Inspektion den weiterhin nichtterminalen Run, aktuelle Snapshots,
  stale Evidence und Mutationsdisposition neu geprüft hat.
- Während `Queued`, `Running`, `Pausing` oder `Cancelling` darf keine H11-Inspektion desselben
  besessenen Tasks In-flight-Versuche unterbrechen. Die UI erhält stattdessen eine kleine
  content-freie Managerprojektion und kann nur den exakt besessenen Task pausieren oder abbrechen.
- Resume und Replan aus `Paused` verwenden den bestehenden atomaren H11/E8-Commit. Nur ein
  erfolgreicher Commit darf einen neuen Scheduler-eigenen Versuch starten. Der neue Versuch leitet
  den Run erneut aus der Task ab und wiederholt keine bereits bestätigte Arbeit.
- Ein ausdrückliches Cancel stoppt zuerst den besessenen Worker. Erst nach dessen terminaler
  Beendigung committed der Manager die H11-Cancel-Wahl und zeigt `Cancelled` erst nach dem
  dauerhaften terminalen Controllerzustand. Pause und Cancel konkurrieren nicht mit einem
  gleichzeitig direkt ausgeführten Recovery-Commit.
- Projektwechsel und Shutdown fordern Cancellation an und warten über Manager und Scheduler auf
  alle besessenen Worker. Sie behaupten keinen Nutzer-Cancel. Nach einem Prozessabbruch bleibt der
  bestehende H11-Neustartpfad die autoritative Wiederherstellung.
- Der Executor-Port gehört zu `a3-application`; konkrete Provider-, Workspace- und Storageadapter
  bleiben außerhalb. Ohne verifiziertes ausführbares Modellprofil existiert keine Agent-
  Executor-Capability, und A^3 bleibt als Indexbrowser nutzbar.

## Konsequenzen

### Positiv

- Pause bleibt mit ADR-0010 kompatibel und ist erst nach tatsächlich beendeter Hintergrundarbeit
  sichtbar.
- Recovery kann keinen im aktuellen Prozess lebenden Toolversuch versehentlich unterbrechen.
- Cancel bleibt dauerhaft und atomar, während betriebliche Pause keinen terminalen Fachzustand
  vortäuscht.
- Dieselbe begrenzte Scheduler-, Backpressure- und Shutdown-Infrastruktur wie Fast Index und Deep
  Map bleibt verantwortlich.

### Negativ

- Desktop-Komposition und IPC müssen Produktlifecycle und fachlichen Controllerzustand getrennt
  projizieren.
- Pause und Cancel benötigen einen zweistufigen Ablauf aus Workerbeendigung und anschließender
  Recovery-Transaktion.
- Ein Appabbruch verliert den flüchtigen Produktlifecycle; der durable H11-Pfad muss ihn bewusst
  rekonstruieren.

### Risiken und Gegenmaßnahmen

- Der Executor ignoriert Cancellation — Scheduler und Shutdown bleiben wartend; Contract-Tests
  verlangen eine begrenzte Reaktionszeit für Modell-, Tool- und Adaptergrenzen.
- Ledger oder Snapshot ändern sich während des Stopps — H11/E8 prüft die Anker nach
  Workerbeendigung erneut und sperrt Resume fail-closed.
- Eine UI-Aktion trifft einen anderen Task — Manager und Application revalidieren Task und
  Ledgeranker; Run-, Step-, Snapshot-, Job- und Event-IDs bleiben Core-eigen.
- Managerzustand und durable Aktivität weichen ab — durable Run-, Ledger- und Journalprojektionen
  bleiben autoritativ; widersprüchliche Managerergebnisse werden `Failed` statt `Paused`.

## Verworfene Alternativen

- `Paused` zu ADR-0010 hinzufügen — vermischt betriebliche Workersteuerung mit dem fachlichen
  Controller und würde ein supersedierendes ADR erfordern.
- Pause als UI-only Flag — lässt Modell- oder Toolarbeit möglicherweise weiterlaufen und besitzt
  keinen überprüfbaren Checkpoint.
- Scheduler-Cancellation als fachliches Cancel committen — macht Resume unmöglich und verwechselt
  Pause mit einer Nutzerentscheidung.
- H11 während eines lebenden Workers ausführen — kann einen legitimen In-flight-Versuch als
  verlassen markieren und erlaubt konkurrierende Runmutation.

## Compliance

- Deterministische Manager-Tests prüfen expliziten Start, Queued-/Running-Grenzen, kooperative
  Pause, H11-Validierung, Resume als neuen Job, dauerhaftes Cancel, Projektwechsel und Shutdown.
- Contract-Tests beweisen, dass weder Statusabfrage noch WebView-Mount Modellarbeit startet und
  dass `Paused` nur nach terminaler Scheduler-Cancellation plus gültiger Recovery-Inspektion
  erscheint.
- IPC-Tests lehnen WebView-gelieferte Run-, Step-, Snapshot-, Job-, Event- oder Provideridentitäten
  ab und halten Produktlifecycle und `AgentControllerState` getrennt.

## Referenzen

- [ADR-0002](0002-tauri-rust-svelte-desktop.md)
- [ADR-0003](0003-modular-monolith-and-dependencies.md)
- [ADR-0010](0010-single-controller-state-machine.md)
- [ADR-0012](0012-safe-tools-and-approval-policy.md)
- [ADR-0013](0013-goal-contract-ledger-and-event-journal.md)
- [ADR-0018](0018-model-provider-port-ownership.md)
- [ADR-0019](0019-durable-mutation-reconciliation.md)
- [Job-Laufzeit](../JOB_RUNTIME.md)
- [Agent Harness H11](../plans/04-MEMORY_AND_AGENT_HARNESS.md#h11-resume-und-recovery)
- [Desktop Product U5](../plans/06-DESKTOP_PRODUCT.md#u5-agent-workspace)
