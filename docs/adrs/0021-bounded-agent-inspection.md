# ADR-0021: Begrenzte taskgebundene Diff- und Verification-Inspektion

Status: Accepted

Datum: 2026-08-13

Entscheider: Tim Bornemann

## Kontext

Plan 06/U6 verlangt vor einer Freigabe genaue Pfade und Änderungen, zwei Diffdarstellungen,
gezielt nachladbare verkürzte Logs sowie nachvollziehbare Beweise für Step- und Must-Kriterien.
Diese Produktansicht liegt an der Grenze zwischen dem privilegierten Rust-Kern und der
unprivilegierten WebView.

E3 besitzt bereits die autoritative `PatchPreview`: Sie ist an Action und Basissnapshot gebunden,
secret-geprüft und auf 16 KiB je Inhaltsseite sowie 64 KiB insgesamt begrenzt. Der mutierende
Controller verwirft diese Vorschau bislang nach der Policyauswertung. E4 hält in einem
`ProcessRunResult` nur secret-geprüfte begrenzte Ausgabe; nach dem Retained Limit werden weitere
Bytes vollständig gedigested, aber verworfen. E6 persistiert bewusst ausschließlich content-freie
Command-, Test-, Diagnostic-, Diff- und UserConfirm-Evidence. Eine nachträgliche Rekonstruktion
exakter Diffinhalte oder Prozessausgabe aus libSQL wäre deshalb weder möglich noch mit der
bestehenden Datenklassifikation vereinbar.

Repositorypfade oder Evidence-IDs aus der WebView dürfen keine allgemeine Source-, Datei-,
Prozess- oder Datenbankbefugnis werden. Gleichzeitig darf die Oberfläche weder Staleness noch
Verification-Erfolg selbst ableiten. Eine dauerhafte Speicherung von Quelltext oder Logs nur für
die Ansicht würde Retention, Datenschutz und die bestehende lokale Evidence-Grenze unnötig
erweitern.

## Entscheidung

- Der Application-Kern definiert einen schmalen `AgentInspectionSink` für bereits validierte
  E3-`PatchPreview`s und vollständig beendete E4-`ProcessRunResult`s. Der mutierende Controller
  veröffentlicht diese Daten an den Sink, bevor die flüchtigen Originale aus seinem Besitz
  fallen. Ein erforderlicher Vorfreigabe-Inspector ist fail-closed: Ohne erfolgreiche Aufnahme
  darf kein nicht anderweitig exakt darstellbarer Patch in `AwaitApproval` wechseln.
- Der Desktop-Composition-Root besitzt genau einen begrenzten flüchtigen Inspection-Store. Jeder
  Datensatz ist an aktives Projekt, Task, Run, Step, Verification-Spec und Snapshot sowie eine
  Core-erzeugte Inspection-Revision gebunden. Projektwechsel, neuer inkompatibler Versuch und
  Shutdown löschen die flüchtige Projektion. Sie wird weder in libSQL noch im Repository
  gespeichert und ist keine fachliche Wahrheitsquelle.
- Exakte Diffinhalte stammen ausschließlich aus der E3-`PatchPreview`. Pfad, vollständiger Hash,
  vollständige Bytezahl, Encoding, Line Endings und Prefix-Trunkierung bleiben erhalten. Der
  Application-Kern leitet daraus deterministisch begrenzte Datei- und Hunkprojektionen ab.
  Unified und Side-by-side sind zwei Darstellungen derselben typisierten Zeilen; die WebView
  berechnet keine Diffsemantik und liest niemals den Live-Worktree nach.
- Autorenmarkierungen bilden eine geschlossene Provenienz: `proposedAgent` für die exakte
  Vorfreigabe-Action, `appliedAgent` nur für ein passendes tatsächliches E3-Change-Set,
  `external` nur für eine explizit als außerhalb des Agenten beobachtete Änderung und sonst
  `unattributed`. Eine Published-Index-Differenz allein wird weder dem Benutzer noch dem Agenten
  zugeschrieben.
- Prozessdetails stammen nur aus dem bereits secret-geprüften `ProcessOutputCapture`. Redacted
  Streams enthalten niemals Text. Nicht redigierte retained Inhalte werden in höchstens
  16-KiB-Seiten ausgeliefert. `pageTruncated` bezeichnet weitere retained Seiten;
  `sourceTruncated` bezeichnet dauerhaft verworfenen Overflow jenseits des E4-Limits. Eine
  Folgeseite wird nur durch eine explizite Nutzeraktion geladen. Nach Neustart bleiben lediglich
  die dauerhaften Digests, Bytezahlen, Limits und Redaction-/Trunkierungsmetadaten sichtbar;
  A^3 führt keinen Prozess still erneut aus, um Logs zu rekonstruieren.
- Dauerhafte Verification-Inspektion verwendet eine eigene read-only Storeoperation. Sie lädt
  genau die vom aktuellen Ledger referenzierten Evidence-IDs zusammen mit dem jüngsten atomar
  publizierten Index, ohne stale Artifacts wegen eines Snapshot-Mismatch auszublenden. Der
  Application-Kern leitet Spec-Semantik, Freshness, Stepzustand und kriteriumsbezogene Beweise
  erneut ab. `Done` zeigt für jedes Must-Kriterium die exakten beweisenden Steps und Evidence-IDs;
  fehlende, fehlgeschlagene oder stale Beweise bleiben getrennt sichtbar. Ein UI-Bool kann weder
  Verification noch Acceptance behaupten.
- IPC-Reads akzeptieren nur Protokollversion, die bereits ausgewählte Task-ID und bei Detailseiten
  Core-emittierte Inspection-/Log-IDs sowie begrenzte Cursor. Run-, Step-, Snapshot-, Pfad-,
  Command-, Process-, Policy- oder frei wählbare Evidence-IDs sind keine allgemeinen
  Requestfelder. Der Core revalidiert jede ID gegen die aktuelle taskgebundene Projektion.
- U6 bleibt rein lesend. Approval, Ablehnung, erneute Verification, Prozesswiederholung,
  Dateizugriff und Mutation gehören nicht zu diesen Commands.

## Konsequenzen

### Positiv

- Der Benutzer sieht vor einer Freigabe exakt den tatsächlich geprüften bounded Patch statt eines
  später neu gelesenen oder vom Frontend berechneten Diffs.
- Quelltext und Logs erhalten keine neue dauerhafte Retention; WebView-Reads bleiben eng an einen
  sichtbaren Task und eine Core-Projektion gebunden.
- Trunkierung unterscheidet nachladbare retained Seiten von absichtlich verworfenem Overflow.
- Stale Evidence bleibt sichtbar, kann aber nie wie ein aktueller Must-Beweis erscheinen.
- Provenienz wird nur dort behauptet, wo Action oder tatsächliches Change-Set sie belegen.

### Negativ

- Nach einem Appneustart sind exakte Vorfreigabeinhalte und retained Logtexte nicht mehr
  verfügbar; durable Metadaten und Verification-Evidence bleiben erhalten.
- Der Desktop hält zusätzlich eine kleine flüchtige Projektion und muss sie mit Projekt- und
  Agent-Lifecycle gemeinsam quiescen.
- Ein Diff über sehr große Dateien kann nur den bereits von E3 begrenzten exakten Präfix zeigen;
  vollständiger Hash und Bytezahl machen die Begrenzung überprüfbar, ersetzen aber keinen
  Volltext.

### Risiken und Gegenmaßnahmen

- Ein alter Inspector wird nach Task-, Run- oder Projektwechsel weiterverwendet — jede Detailquery
  revalidiert Task und Inspection-Revision; Lifecyclewechsel löschen den Store.
- Secrettext erreicht die WebView — nur bereits klassifizierte `PatchFileContent`-Previews und
  `ProcessOutputContent::Retained` werden aufgenommen; Redaction hat Vorrang und Negativtests
  verwenden bekannte Secretmuster.
- Ein Hunk suggeriert Vollständigkeit trotz Prefixgrenze — jede Datei und Seite trägt getrennte
  `contentTruncated`, `pageTruncated` und `sourceTruncated`-Signale samt vollständiger Bytezahl und
  Hash.
- Eine Published-Index-Differenz wird fälschlich einem Akteur zugeschrieben — ohne exakte
  Patch-/Observer-Provenienz lautet die Markierung zwingend `unattributed`.
- Inspection-Aufnahme blockiert Agentenarbeit — der flüchtige Store ist speichergebunden,
  synchron, ohne I/O und überschreibt ausschließlich die ältere Projektion desselben Tasks.

## Verworfene Alternativen

- Quelltext und vollständige Logs in libSQL persistieren — erweitert Datenklassifikation,
  Retention und Secret-Risiko ohne fachliche Notwendigkeit.
- Diff und Staleness in Svelte berechnen — dupliziert Fachlogik an der unprivilegierten Grenze und
  könnte widersprüchliche Acceptanceanzeigen erzeugen.
- Aktuelle Dateien für die Vorfreigabe erneut lesen — kann Useränderungen mit der ursprünglich
  geprüften Action vermischen und würde einen generischen Source-Read nahelegen.
- Trunkierte Prozessausgabe automatisch durch Wiederholung nachladen — mutiert oder belastet die
  Umgebung ohne neue Freigabe und reproduziert nicht zwingend dasselbe Ergebnis.
- Jede Indexdifferenz als Benutzeränderung markieren — behauptet eine nicht vorhandene
  Akteursprovenienz.

## Compliance

- Application-Contracts prüfen Patch-/Task-/Snapshotbindung, deterministische Hunkbildung,
  zuverlässige Provenienz, Fresh/Stale-Neubewertung und vollständige Must-Kriterienprojektion.
- Desktop- und IPC-Tests lehnen freie Pfade, Run-/Step-/Snapshot-/Evidence-IDs, unbekannte Felder,
  alte Inspection-Revisionen, übergroße Cursor und fremde Tasks ab.
- Frontendtests prüfen Unified-/Side-by-side-Parität, explizites Log-Nachladen,
  `pageTruncated`/`sourceTruncated`, Redaction, stale Dominanz und Must-Beweise bei `Done`.
- Security-Negativtests belegen, dass Secretkandidaten, redigierte Streams und verworfener
  Overflow niemals als Text in der Antwort erscheinen.
- Projektwechsel und Shutdown werden mit leerem flüchtigem Inspection-Store verifiziert.

## Referenzen

- [ADR-0002](0002-tauri-rust-svelte-desktop.md)
- [ADR-0003](0003-modular-monolith-and-dependencies.md)
- [ADR-0008](0008-epistemic-memory-and-invalidation.md)
- [ADR-0012](0012-safe-tools-and-approval-policy.md)
- [ADR-0013](0013-goal-contract-ledger-and-event-journal.md)
- [ADR-0019](0019-durable-mutation-reconciliation.md)
- [ADR-0020](0020-agent-runtime-ownership-and-pause.md)
- [Architektur](../ARCHITECTURE.md)
- [Domainmodell](../DOMAIN_MODEL.md)
- [Security und Execution](../SECURITY_AND_EXECUTION.md)
- [Desktop Product U6](../plans/06-DESKTOP_PRODUCT.md#u6-diff-und-verification)
