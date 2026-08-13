# ADR-0022: Taskgebundenes Approval Center mit expliziter Fortsetzung

Status: Accepted

Datum: 2026-08-13

Entscheider: Tim Bornemann

## Kontext

Plan 06/U7 verlangt eine informierte und spezifische Freigabeansicht für privilegierte
Agentenaktionen. E1 und ADR-0012 besitzen bereits die autoritative Policyentscheidung, einen an
Run, Action-Fingerprint, Scope, Klasse und Risiko gebundenen `ApprovalRequest` sowie einen
höchstens 24 Stunden gültigen und genau einmal verbrauchbaren `ApprovalGrant`. E7 wechselt erst
nach dauerhafter Policyentscheidung in `AwaitApproval` und darf Patch oder Prozess vor einer
passenden Freigabe nicht ausführen.

Die dauerhafte E1-Projektion ist absichtlich content-frei. Sie enthält deshalb weder exakte
Repositorypfade noch Patchbegründung, Executable, argv oder Arbeitsverzeichnis. Diese Daten nur für
die Oberfläche dauerhaft zusätzlich zu speichern, würde die bestehende Retention- und
Secret-Grenze erweitern. Die WebView darf umgekehrt keine freie Request-, Run-, Prozess-, Pfad-,
Policy- oder Approval-ID als neue privilegierte Auswahl liefern.

Die Formulierung „einmal erlauben, scopegebunden erlauben oder ablehnen“ darf ADR-0012 nicht zu
einer pauschalen oder still wiederverwendbaren Scopefreigabe lockern. Ein Pfadscope ohne den exakten
Action-Fingerprint könnte beispielsweise anderen Patchinhalt autorisieren. Die bestehende
`ProjectCommandAllowlist` bestätigt separat einen evidence-gebundenen Katalog sicherer Commands;
sie ist ausdrücklich kein wiederverwendeter `ApprovalGrant` und keine Lockerung der zentralen
Policy.

## Entscheidung

- U7 stellt „einmal erlauben“ als eine einzige exakte, zugleich aktions- **und** scopegebundene
  V1-Freigabe dar. Es gibt keinen breiteren wiederverwendbaren Approval-Modus. Die Oberfläche
  erklärt sichtbar, dass ausschließlich die angezeigte Aktion, der angezeigte Scope und der
  aktuelle Run einmal autorisiert werden.
- Der Application-Kern definiert einen schmalen `AgentApprovalSink` für bereits validierte
  freigabepflichtige E7-Aktionen. Der mutierende Controller veröffentlicht die Darstellung erst,
  nachdem der `ApprovalRequest` dauerhaft gespeichert wurde, und bevor der Schritt in
  `AwaitApproval` wechselt. Kann die erforderliche Darstellung nicht aufgenommen werden, bleibt
  die Aktion fail-closed und erreicht keine Werkzeuggrenze.
- Der Desktop-Composition-Root besitzt genau einen begrenzten flüchtigen Approval-Store. Ein
  Datensatz ist an aktives Projekt, Task, Run, Step, Verification-Spec, Snapshot,
  Approval-Request, Action-Fingerprint und Scope-Digest gebunden. Ein neuer inkompatibler Run,
  Projektwechsel und Shutdown verwerfen ihn. Er wird weder in libSQL noch im Repository
  gespeichert und ist keine fachliche Wahrheitsquelle.
- Eine Patchdarstellung enthält die secret-geprüfte begrenzte E3-Begründung und jeden exakten
  Quell-/Zielpfad samt Add-, Update-, Move- oder Delete-Form. Die vollständige Diffdarstellung
  bleibt beim U6-Inspector. Eine Prozessdarstellung enthält den vollständigen validierten
  `ProcessSpec`: Executable, getrennte argv-Werte, relatives Arbeitsverzeichnis,
  Environment-**Namen** ohne Werte, Timeout, Outputgrenzen, Execution Mode, Planbindung und
  deklarativen Network Scope. Bereits bei Konstruktion erkannte Secret-Kandidaten erreichen diese
  Projektion nicht.
- Der taskgebundene Read akzeptiert ausschließlich Protokollversion und die bereits ausgewählte
  `TaskId`. Der Core leitet Run und Step aus dem aktuellen Ledger ab, lädt Request und optionalen
  Grant aus dem Policy-Store, revalidiert sämtliche Anker gegen die flüchtige Projektion und liest
  die durable Aktivität danach erneut. Ohne exakte Darstellung, nach Projekt-/Runwechsel oder bei
  konkurrierender Änderung wird kein entscheidbarer Approval-Zustand ausgegeben.
- Die Mutation akzeptiert ausschließlich `TaskId`, die zuvor ausgegebene positive
  Approval-Revision, sichtbare Ledgerrevision/-Storeversion und genau eine geschlossene Aktion:
  `allowOnce`, `deny`, `continue` oder `revoke`. Request-, Grant-, Run-, Step-, Snapshot-, Pfad-,
  Prozess-, Policy-, Event- und Zeitidentitäten bleiben Core-eigen und werden unmittelbar vor
  jeder Wirkung neu abgeleitet.
- `allowOnce` erzeugt den bestehenden exakten `ApprovalGrant` und sein content-freies Audit
  dauerhaft, startet aber noch keine Agentenarbeit. Erst die getrennte bewusste Aktion `continue`
  darf den Scheduler mit einer Core-internen Approval-ID fortsetzen. Dadurch kann der Nutzer einen
  aktiven Grant vor Verbrauch tatsächlich widerrufen; Mount, Polling und Grant-Erzeugung starten
  weder Modell noch Werkzeug.
- Die Application-Laufzeitanforderung unterscheidet eine normale Ausführung von einer
  Approval-Fortsetzung und trägt die interne `ApprovalId` nur über den privilegierten
  `AgentRunExecutor`-Port. Die WebView erhält diese ID nicht. Der Executor muss den Grant erneut
  laden und E7 darf ihn weiterhin nur durch eine Policyentscheidung mit identischem Run,
  Fingerprint, Scope, Klasse und Risiko genau einmal verbrauchen.
- `deny` blockiert den wartenden Ledger-Schritt mit einer festen content-freien Nutzerbegründung
  und committed den bestehenden `ApprovalDenied`-Übergang atomar mit Ledger und Run. Der
  Controller endet sauber in `Failed`; U5 bietet anschließend den expliziten Replan- oder
  Cancel-Pfad. Eine Ablehnung kann keine Werkzeuggrenze öffnen.
- `revoke` verwendet ausschließlich den bestehenden atomaren E1-Widerruf. Ablauf, Verbrauch,
  Widerruf und konkurrierende Änderungen werden vom Core abgeleitet und als getrennte
  nicht-manipulierbare Zustände angezeigt.
- Keine Entscheidung ist vorausgewählt. Zulassen und Ablehnen werden als gleichwertige bewusste
  Optionen mit sichtbarer Wirkung dargestellt; die Bestätigung bleibt deaktiviert, bis der Nutzer
  selbst eine Option wählt. Risiko, Klasse, Policygrund, Ablauf, Scope und genaue Aktion stehen vor
  den Controls.

## Konsequenzen

### Positiv

- Eine kompromittierte WebView kann nur die exakt sichtbare taskgebundene Entscheidung anfordern
  und keine andere Aktion, keinen anderen Scope und keinen anderen Run auswählen.
- Der Nutzer sieht vor seiner Entscheidung die tatsächlich policygeprüfte Aktion und kann eine
  gespeicherte Freigabe noch widerrufen, bevor ein Scheduler-Versuch beginnt.
- ADR-0012 bleibt unverändert streng: Es gibt weder pauschale Pfadfreigaben noch still
  wiederverwendbare Grants.
- Ablehnung besitzt einen dauerhaften, nachvollziehbaren Blocked-/Failed-Zustand ohne
  Werkzeugeffekt.

### Negativ

- Nach einem Appneustart fehlt die exakte flüchtige Aktion. Ein wartender Run muss dann über den
  bestehenden Replan- oder Cancel-Pfad fortgeführt werden, statt eine unvollständig dargestellte
  Freigabe anzubieten.
- Das Speichern und das Verwenden einer Freigabe sind zwei bewusste Interaktionen.
- Der Desktop hält zusätzlich eine kleine flüchtige, privilegierte Aktionsprojektion.

### Risiken und Gegenmaßnahmen

- Request oder Ledger ändert sich zwischen Anzeige und Klick — Core und Store prüfen Approval-
  Revision, Ledger-CAS, Run-Sequenz, Snapshot und Requestanker erneut; das Ergebnis ist
  `activityChanged` statt einer Wirkung.
- Ein Grant läuft unmittelbar vor `continue` ab oder wurde parallel widerrufen — die erneute
  Statusprüfung blockiert den Scheduler; E7 prüft den Grant vor der Werkzeuggrenze nochmals.
- Prozessargumente enthalten Geheimnisse — die bestehende `ProcessSpec`-Konstruktion lehnt
  Secret-Kandidaten ab, Environment-Werte werden nie projiziert, und unbekannte oder
  kontrollzeichenhaltige IPC-Felder werden verworfen.
- Ein laufender Worker konkurriert mit Approval-Control — der Composition-Root verwendet dieselbe
  taskweite Operationssperre wie U5; der Manager akzeptiert keine zweite besessene Arbeit.

## Verworfene Alternativen

- Wiederverwendbarer Pfad- oder Klassengrant — könnte anderen Inhalt oder eine andere Aktion
  autorisieren und widerspricht ADR-0012.
- `ProjectCommandAllowlist` als Approval behandeln — vermischt evidenzgebundene Command Discovery
  mit dem einmaligen zentralen Grant und könnte Workspace-Policy scheinbar lockern.
- Grant und Ausführung in einem UI-Klick — lässt praktisch kein Widerrufsfenster und macht die
  Wirkung weniger eindeutig.
- Request-, Run- oder Approval-ID aus der WebView auswählen lassen — erweitert eine enge
  taskgebundene Entscheidung zu einer allgemeinen Policy-Lookup- oder Mutationsbefugnis.
- Exakte Aktionen dauerhaft in libSQL speichern — erweitert Source-/argv-Retention ohne
  fachliche Notwendigkeit.

## Compliance

- Application-Contracts prüfen Patch- und `ProcessSpec`-Vollständigkeit, Request-/Run-/Step-/
  Snapshotbindung, Ablauf, Widerruf, One-time-Consumption, Denial zu Blocked/Failed sowie
  konkurrierende Ledger- und Grantänderungen.
- Desktop- und IPC-Tests lehnen freie Request-, Grant-, Run-, Step-, Snapshot-, Pfad-, Prozess-,
  Policy-, Event- und Zeitfelder, unbekannte Felder und alte Approval-Revisionen ab.
- Component-Tests zeigen Aktion, Risiko, Scope, Policygrund, Ablauf, exakte Pfade beziehungsweise
  argv vor allen Controls, beginnen ohne Auswahl und trennen Speichern, Fortsetzen und Widerruf.
- Capability-Tests erlauben nur den engen Read und die taskgebundene Mutation; generische Datei-,
  Shell-, SQL-, Provider-, Netzwerk- und Policy-APIs bleiben ausgeschlossen.
- Projektwechsel, neuer Run und Shutdown werden mit leerer flüchtiger Approval-Projektion
  verifiziert.

## Referenzen

- [ADR-0002](0002-tauri-rust-svelte-desktop.md)
- [ADR-0003](0003-modular-monolith-and-dependencies.md)
- [ADR-0010](0010-single-controller-state-machine.md)
- [ADR-0012](0012-safe-tools-and-approval-policy.md)
- [ADR-0013](0013-goal-contract-ledger-and-event-journal.md)
- [ADR-0019](0019-durable-mutation-reconciliation.md)
- [ADR-0020](0020-agent-runtime-ownership-and-pause.md)
- [ADR-0021](0021-bounded-agent-inspection.md)
- [Architektur](../ARCHITECTURE.md)
- [Domainmodell](../DOMAIN_MODEL.md)
- [Security und Execution](../SECURITY_AND_EXECUTION.md)
- [Desktop Product U7](../plans/06-DESKTOP_PRODUCT.md#u7-approval-center)
