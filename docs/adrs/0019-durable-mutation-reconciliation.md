# ADR-0019: Dauerhafte Mutationsdisposition und Reconciliation

Status: Accepted  
Datum: 2026-08-11  
Entscheider: Tim Bornemann

## Kontext

Der mutierende Controller persistiert einen Toolversuch vor dem Adapteraufruf und schließt ein
erfolgreiches Ergebnis gemeinsam mit dem Tool-Journal ab. Dateisystem und Prozessgruppe können
jedoch nicht an derselben Datenbanktransaktion teilnehmen. Ein Appabbruch, ein nicht mehr
erreichbarer Store oder ein nicht vollständig beobachtbarer Prozess kann deshalb nach Beginn der
Mutation eintreten, bevor A^3 deren Ergebnis dauerhaft klassifiziert hat.

Der bisherige Versuchslifecycle `in_flight`, `succeeded`, `failed`, `cancelled`, `denied` und
`interrupted` beschreibt die Ausführung, aber nicht, ob die Wirkung im Worktree angewendet wurde.
Ein bloßes Retry könnte einen Patch doppelt anwenden, eine fremde Änderung überschreiben oder mit
einem veralteten Index weiterarbeiten. E8 verlangt daher die sichtbare Unterscheidung
`Applied`, `NotApplied` und `Unknown` sowie eine Reconciliation vor jeder weiteren Mutation.

## Entscheidung

- Jeder mutierende Toolversuch besitzt zusätzlich zu seinem Lifecycle eine content-freie,
  dauerhafte Mutationsprojektion mit Action-Fingerprint, Aktionsart und genau einer Disposition:
  `Applied`, `NotApplied` oder `Unknown`.
- Vor dem ersten Aufruf einer Patch- oder Prozessgrenze werden Toolversuch und
  Mutationsprojektion atomar gespeichert. Der Anfangszustand ist `Unknown` mit ausstehender
  Reconciliation. Dadurch bleibt ein Crash vor jeder späteren Klassifikation fail-closed.
- Ein Patchresultat mit einem vollständigen oder partiellen `PatchChangeSet` ist `Applied`.
  Ein typisierter Patchfehler, dessen Adaptervertrag keine sichtbare Änderung erlaubt, ist
  `NotApplied`.
- Ein Prozess, der einen terminalen Exitstatus geliefert hat, ist `Applied`, unabhängig davon, ob
  seine objektive Verifikation besteht. Ablehnung, Cancellation vor dem Spawn und Spawnfehler sind
  `NotApplied`. Timeout, Cancellation nach dem Spawn sowie ein Beobachtungs-, Reap- oder
  Ergebnisverlust nach möglichem Spawn sind `Unknown`.
- Lifecycle und Disposition werden gemeinsam beendet. Der erfolgreiche Toolresultat-/Journalpfad
  setzt die Mutation in derselben Transaktion auf `Applied`. Kann diese Transaktion nach einer
  sichtbaren Wirkung nicht bestätigt werden, bleibt der vorher gespeicherte Zustand `Unknown`.
- Ein nicht reconciliertes `Unknown` eines Worktrees verhindert atomar den Beginn jedes weiteren
  mutierenden Toolversuchs. Read-, Index- und Recovery-Arbeit bleiben möglich.
- Reconciliation verändert, verwirft oder wiederholt keine Worktree-Änderung. Sie erstellt unter
  dem einzigen Worktree-Mutations-Lease einen vollständigen autoritativen Indexsnapshot und bindet
  genau diesen Snapshot gemeinsam mit einem Recovery-Event dauerhaft an den unbekannten Versuch.
  Die historische Disposition bleibt `Unknown`, wird aber als reconciliert markiert.
- Ein reconciliertes `Unknown` erlaubt erst nach `Replan` neue Mutationen; `Resume` darf eine
  unbekannte Wirkung nicht als erfolgreich interpretieren. `Cancel` bleibt jederzeit möglich.
- Provider-, Context-, Policy- oder Persistenzfehler vor dem dauerhaften Mutationsbeginn sind
  ausdrücklich `NotApplied`; kein Adapter darf in diesem Fall aufgerufen werden.

## Konsequenzen

### Positiv

- Ein Crash zwischen Patch und Journal kann weder still als Erfolg noch als sicherer Fehlschlag
  missverstanden werden.
- Retry und Recovery überschreiben keine fremden Änderungen und wenden unbekannte Patches nicht
  erneut an.
- UI und spätere IPC-Projektionen erhalten eine kleine, eindeutige und content-freie
  Sicherheitsklassifikation.
- Der aktuelle Repositoryzustand wird vor weiterer Modell- oder Mutationsarbeit erneut
  autoritativ veröffentlicht.

### Negativ

- Knowledge-Persistenz und Recovery-Port erhalten einen weiteren versionierten Vertrag.
- Prozessabbrüche sind konservativ häufiger `Unknown`, obwohl einzelne Befehle möglicherweise
  keine Datei verändert haben.
- Reconciliation erfordert einen vollständigen Repositoryscan und anschließend einen Replan.

### Risiken und Gegenmaßnahmen

- Eine fremde Änderung tritt während Reconciliation ein — der Patchpfad prüft weiterhin Snapshot
  und Content Hashes unmittelbar vor Mutation; Watcher- und Vollscanregeln invalidieren neue
  Änderungen, ohne sie zurückzusetzen.
- Ein zweiter Runtime-Prozess startet parallel — der `IMMEDIATE`-Beginn prüft im selben
  Transaktionsschritt auf nicht reconciliertes `Unknown`; die Datenbankgrenze ergänzt den
  runtime-lokalen Lease.
- Storefehler nach sichtbarer Wirkung — der bereits persistierte Anfangszustand bleibt `Unknown`;
  Korruption und Unverfügbarkeit stoppen jede weitere Mutation.
- Reconciliation wird als Erfolg der ursprünglichen Aktion missverstanden — die Disposition
  bleibt historisch `Unknown`, `Resume` bleibt gesperrt und nur `Replan` öffnet den nächsten
  Mutationspfad.

## Verworfene Alternativen

- Jeden `interrupted`-Versuch als `NotApplied` behandeln — verliert sichtbare Patch- oder
  Prozesswirkungen und ermöglicht gefährliche Doppelanwendung.
- Jeden begonnenen Versuch als `Applied` behandeln — behauptet eine Wirkung ohne Evidenz und kann
  nicht gestartete Aktionen als erledigt markieren.
- Worktree vor Retry automatisch zurücksetzen — würde fremde Änderungen verwerfen und verletzt
  die Patch-Policy.
- Patchinhalte oder Prozessoutput für Recovery persistieren — vergrößert Secret- und
  Source-Exposition, obwohl ein content-freier Vollscan genügt.
- `Unknown` nur im Arbeitsspeicher halten — verliert die Sicherheitsgrenze genau beim relevanten
  Crash oder Neustart.

## Compliance

- Domain-Tests prüfen die erlaubten Lifecycle-/Dispositionskombinationen.
- Der gemeinsame Storage-Contract prüft atomaren Beginn, Abschluss, Reopen, Worktree-Sperre,
  Reconciliation und CAS-Rollback.
- Adapter- und Agent-Harness-Tests decken Patchkonflikt, partiellen Patch, Testfehler, Timeout,
  Providerabbruch, Storefehler, User-Cancel und Crash zwischen Patch und Journal ab.
- Migrationstests prüfen leeres V22-Schema, jedes unterstützte Vorgängerschema und vollständigen
  V21→V22-Rollback.

## Referenzen

- [ADR-0003](0003-modular-monolith-and-dependencies.md)
- [ADR-0008](0008-epistemic-memory-and-invalidation.md)
- [ADR-0010](0010-single-controller-state-machine.md)
- [ADR-0012](0012-safe-tools-and-approval-policy.md)
- [ADR-0013](0013-goal-contract-ledger-and-event-journal.md)
- [Editing und Verification E8](../plans/05-EDITING_AND_VERIFICATION.md#e8-failure-recovery)
- [Security und Execution](../SECURITY_AND_EXECUTION.md)
