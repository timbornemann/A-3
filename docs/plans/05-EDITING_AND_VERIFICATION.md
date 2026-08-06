# Plan 05: Sicheres Editing, Prozesse und Verifikation

Ziel: Der Agent kann kontrolliert Code ändern und die Änderung gegen Akzeptanzkriterien verifizieren.

Alle angenommenen ADRs einschließlich 0018 bleiben bindend. Für diesen Plan unmittelbar relevant
sind insbesondere 0003, 0006, 0008, 0010, 0012, 0013, 0014 und 0017.

## E1 Policy Engine

Abhängigkeiten: Gate M6

- [x] ActionClass und RiskLevel
- [x] Root-, Path-, Process-, Network- und Git-Policy
- [x] PolicyDecision mit Begründung
- [x] ApprovalRequest und scopegebundene Approval
- [x] Ablauf und Widerruf
- [x] Audit Events

Akzeptanz:

- jede privilegierte Action erhält genau eine PolicyDecision;
- Freigabe für einen Pfad gilt nicht automatisch für andere Pfade;
- Sicherheitsregeln können nicht durch Workspace-Regeln gelockert werden.

Verifiziert am 2026-08-06: Der Domain-Kern leitet Klasse und Risiko aus der geschlossenen
Root-/Path-/Process-/Network-/Git-Action ab und besitzt keine Workspace-Repräsentation zum Lockern
der Systembaseline. Der zentrale Application-Use-Case erzeugt pro Auswertung genau eine begründete
Decision und genau ein content-freies RunEvent. Knowledge-Schema V18 persistiert Requests, Grants,
Decisions, Grant/Widerruf und One-time-Consumption atomar mit der Runsequenz. Der gemeinsame
Storagevertrag belegt Reopen, Pfad-Mismatch ohne Verbrauch, exakte Consumption, Widerruf,
Workspace-Deny und vollständigen CAS-Rollback. Windows-Workspace-Gates, Rustdoc, Node 24.14.0 mit
pnpm 11.9.0, 20 Frontend- und vier Tooltests, Build, 45 Markdown-Dateien mit 66 lokalen Links sowie
der vollständige Linux-`quality`-Job über `act` sind grün.

## E2 Secure File Tools

Abhängigkeiten: E1

- [x] begrenztes Datei-Lesen
- [x] Directory Listing mit Ignore-Policy
- [x] Canonicalization nach Symlinkauflösung
- [x] Special-File-Ablehnung
- [x] Secretklassifikation und Redaction
- [x] Read Evidence

Akzeptanz:

- Traversal- und Symlink-Escape-Fixtures werden abgelehnt;
- Binär- und Großdateien werden nicht unkontrolliert gelesen;
- erlaubte Subtree-Reads funktionieren plattformübergreifend.

Verifiziert am 2026-08-06: Versionierte Domainverträge erzwingen snapshot-/worktreegebundene,
vorwärts paginierte Directory-Seiten mit höchstens 256 direkten Kindern und konkreter
`FileRevision`-Evidence. Die Application-Ports bleiben read-only, abbrechbar und content-frei im
Fehlerfall. `a3-workspace` liest ausschließlich reguläre, kanonisch rootgebundene Dateien in
64-KiB-Blöcken bis 4 MiB, prüft Handle, Ziel und vollständigen Hash erneut und blockiert Binary-
und Secret-Kandidaten. Listings verwenden den ignore-gefilterten Published Index und wenden die
nicht übersteuerbare `DiscoveryPolicy::v1` zusätzlich erneut an. Die öffentliche Contract-Suite
belegt unter Windows vier Fälle einschließlich Junction-Escape; der Linux-Quality-Job belegt alle
fünf Fälle einschließlich Unix-Socket-Ablehnung. Workspace-Clippy, Rustdoc, Node 24.14.0 mit pnpm
11.9.0, 20 Frontend- und vier Tooltests, Build, 45 Markdown-Dateien mit 66 lokalen Links sowie der
vollständige Linux-`quality`-Job sind grün. Der vollständige Windows-Workspace-Test erreichte nur
wegen des unveränderten nativen libSQL-Migrationstests keinen grünen Sammelstatus: dessen Worker
endete nach den drei gemäß Gate zulässigen Versuchen jeweils mit `0xc0000005` vor dem
Abschlussmarker; Assertions und sämtliche E2-Tests blieben grün.

## E3 PatchAction

Abhängigkeiten: E1, E2

- [x] erwarteter Snapshot und Content Hash
- [x] Add, Update, Move und Delete als getrennte Operationen
- [x] Patchvorschau
- [x] atomare Dateioperation soweit möglich
- [x] Konflikt bei Useränderung
- [x] keine automatische Überschreibung
- [x] Post-Patch Change Set

Akzeptanz:

- konkurrierende Useränderung verhindert Anwendung;
- Patch außerhalb Root wird abgelehnt;
- Line-Ending und Encoding-Fixtures bleiben korrekt;
- Diff und Evidence zeigen den tatsächlichen neuen Inhalt.

Verifiziert am 2026-08-06: `PatchActionSchemaVersion::V1` bindet Run, Worktree, Published
Snapshot, TaskStep, Verification sowie erwartete und neue BLAKE3-Hashes an getrennte Add-,
Update-, Move- und Delete-Operationen und einen content-freien Policy-Fingerprint. Der schmale
Application-Port erzwingt eine verbrauchte exakte Approval-Entscheidung. `a3-workspace` prüft
Root, Symlink-/Reparse-Komponenten, Published Revisionen, Live-Hashes und Zielabwesenheit vor der
Vorschau und erneut nach vollständigem Same-Directory-Staging unmittelbar vor der ersten
Mutation. Add und Move überschreiben kein bestehendes Ziel; Update und Delete verwenden atomare
Dateisystemoperationen soweit von der Plattform bereitgestellt. Vollständige und kanonisch
partielle `PatchChangeSet`s tragen die tatsächlichen Revisionen und Invalidierungspfade.

Vier Domain-/Policy-Tests, ein Authorization-Test und sechs öffentliche Workspace-Contracts
prüfen Binary-/Secret-Ablehnung, exakte und global auf 64 KiB begrenzte Vorschau, UTF-8-BOM,
CRLF und Nicht-ASCII-Bytes, alle vier Operationen, tatsächliche Post-Write-Hashes, Useränderung
zwischen Preview und Apply, No-Replace, Symlink-/Junction-Escape und einen späten Konflikt nach
bereits sichtbarer Teilmutation. Formatcheck, Workspace-Clippy mit allen Targets und Features bei
`-D warnings`, Rustdoc bei `-D warnings`, Frontendformat/Lint/Typecheck, 20 Frontend- und vier
Tooltests, Build, 45 Markdown-Dateien mit 66 Links, Dependency-Report und der vollständige
Linux-`quality`-Job über `act` sind grün. Der Windows-Workspace-Sammellauf erreichte wegen des
unveränderten nativen libSQL-`knowledge_contract`-Workers keinen grünen Gesamtstatus: zwei gemäß
Gate zulässige frische Sammelversuche endeten mit `0xc0000005`; der isolierte Wiederholungs-Worker
bestand alle acht Tests mit Abschlussmarker, anschließend bestanden die sonst nicht erreichten
sechs E3-, sieben Project-Catalog- und 26 Shared-Storage-Contracts separat.

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
