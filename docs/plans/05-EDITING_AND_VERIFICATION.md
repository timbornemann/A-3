# Plan 05: Sicheres Editing, Prozesse und Verifikation

Ziel: Der Agent kann kontrolliert Code ändern und die Änderung gegen Akzeptanzkriterien verifizieren.

Relevante ADRs: 0010, 0012, 0013

## E1 Policy Engine

Abhängigkeiten: Gate M6

- [ ] ActionClass und RiskLevel
- [ ] Root-, Path-, Process-, Network- und Git-Policy
- [ ] PolicyDecision mit Begründung
- [ ] ApprovalRequest und scopegebundene Approval
- [ ] Ablauf und Widerruf
- [ ] Audit Events

Akzeptanz:

- jede privilegierte Action erhält genau eine PolicyDecision;
- Freigabe für einen Pfad gilt nicht automatisch für andere Pfade;
- Sicherheitsregeln können nicht durch Workspace-Regeln gelockert werden.

## E2 Secure File Tools

Abhängigkeiten: E1

- [ ] begrenztes Datei-Lesen
- [ ] Directory Listing mit Ignore-Policy
- [ ] Canonicalization nach Symlinkauflösung
- [ ] Special-File-Ablehnung
- [ ] Secretklassifikation und Redaction
- [ ] Read Evidence

Akzeptanz:

- Traversal- und Symlink-Escape-Fixtures werden abgelehnt;
- Binär- und Großdateien werden nicht unkontrolliert gelesen;
- erlaubte Subtree-Reads funktionieren plattformübergreifend.

## E3 PatchAction

Abhängigkeiten: E1, E2

- [ ] erwarteter Snapshot und Content Hash
- [ ] Add, Update, Move und Delete als getrennte Operationen
- [ ] Patchvorschau
- [ ] atomare Dateioperation soweit möglich
- [ ] Konflikt bei Useränderung
- [ ] keine automatische Überschreibung
- [ ] Post-Patch Change Set

Akzeptanz:

- konkurrierende Useränderung verhindert Anwendung;
- Patch außerhalb Root wird abgelehnt;
- Line-Ending und Encoding-Fixtures bleiben korrekt;
- Diff und Evidence zeigen den tatsächlichen neuen Inhalt.

## E4 ProcessRunner

Abhängigkeiten: E1

- [ ] argv-basierter Start
- [ ] CWD- und Executable-Policy
- [ ] Env-Allowlist
- [ ] Timeout und Cancellation
- [ ] Outputcap und Stream Events
- [ ] Prozessbaumbeendigung Windows, Linux, macOS
- [ ] Network-Klassifikation

Akzeptanz:

- Shellzeichen in Argumenten werden nicht interpretiert;
- Endlosprozess endet nach Timeout;
- Child-Prozess bleibt nach Cancel nicht bestehen;
- Outputoverflow blockiert den Prozess nicht.

## E5 Command Discovery

Abhängigkeiten: Fast Index, E4

- [ ] sichere Test-, Build-, Lint- und Formatbefehle aus Manifesten ableiten
- [ ] Commands als ProcessSpec anzeigen
- [ ] Allowlist je Projekt bestätigen
- [ ] kein automatisches Paketinstall
- [ ] Working Directory je Package

Akzeptanz:

- Rust-, Node- und Python-Fixtures erhalten korrekte Standardbefehle;
- Lockfile-fehlender Installversuch startet nicht automatisch;
- Monorepo-Kommandos laufen im richtigen Package.

## E6 Verification Engine

Abhängigkeiten: E3 bis E5

- [ ] VerificationSpec-Typen: Command, Test, DiffInvariant, Diagnostic, UserConfirm
- [ ] schmalste relevante Prüfung zuerst
- [ ] Ergebnisse als TestEvidence oder CommandEvidence
- [ ] Muss- und Soll-Kriterien
- [ ] Freshness und Snapshotbindung
- [ ] Acceptance-Verifier

Akzeptanz:

- Exitcode allein ohne erwartete Semantik genügt nicht bei spezialisierten Verifikationen;
- Done wird bei fehlgeschlagenem Muss-Kriterium blockiert;
- Useränderung nach Test macht betroffene Verification stale.

## E7 Mutating Controller

Abhängigkeiten: E1, E3, E6

- [ ] ApplyPatch- und Run-Action-Schemas
- [ ] ein Mutations-Lock pro Worktree
- [ ] Policy und Approval vor Ausführung
- [ ] sofortige Indexinvalidierung nach Patch
- [ ] Context Recompile
- [ ] Verify und Replan
- [ ] Fortschrittsdetektor

Akzeptanz:

- kein zweiter mutierender Turn parallel;
- Modell arbeitet nach Patch nicht mit altem Codekontext weiter;
- wiederholte identische Fehlaktion triggert Replan oder Stop.

## E8 Failure Recovery

Abhängigkeiten: E7

- [ ] Patch Conflict
- [ ] Test Failure
- [ ] Tool Timeout
- [ ] Provider Disconnect
- [ ] DB Busy oder Corruption
- [ ] User Cancel
- [ ] App Crash zwischen Patch und Journal

Akzeptanz:

- keine Recovery verwirft fremde Änderungen;
- Zustand zeigt klar Applied, NotApplied oder Unknown;
- Unknown erfordert Reconciliation vor weiterer Mutation.

## E9 End-to-End Coding Tasks

Abhängigkeiten: E8

- [ ] kleiner lokaler Bugfix
- [ ] Änderung über zwei Module
- [ ] Test ergänzen
- [ ] absichtlich scheiternder Plan und Replan
- [ ] zwischenzeitliche Useränderung
- [ ] Context Compaction während Aufgabe

Akzeptanz:

- jede Änderung besitzt Goal, Step, Patch, Evidence und Verification;
- keine Aufgabe endet Done mit rotem Muss-Test;
- Evalresultate sind reproduzierbar gespeichert.

## Gate M7

- [ ] Security-Negativsuite grün
- [ ] Prozessbaumtests auf drei OS
- [ ] Mutations-Lock und Snapshotkonflikte grün
- [ ] alle E2E-Tasks mit vollständigem Audit
- [ ] kein Netzwerk oder Paketinstall ohne Approval
- [ ] keine destruktive Gitaktion im automatischen Pfad

