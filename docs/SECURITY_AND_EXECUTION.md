# Sicherheit und kontrollierte Ausführung

Status: verbindliche Baseline  
Stand: 2026-08-06

## Trust Boundary

Die Tauri WebView, Repository-Inhalte, LLM-Ausgaben, Toolausgaben und externe Modellserver gelten als nicht vertrauenswürdig. Nur der Rust-Kern darf privilegierte Betriebssystemaktionen ausführen.

~~~mermaid
flowchart TD
    Untrusted["WebView, Code, Modell"]
    Policy["Typed Commands + Policy Engine"]
    Core["Privilegierter Rust-Kern"]
    OS["Dateien, Prozesse, Netzwerk"]

    Untrusted --> Policy
    Policy --> Core
    Core --> OS
~~~

## Instruktionsvertrauen

Als Workspace-Regeln dürfen ausschließlich explizite Policy-Dateien ausgewertet werden:

- AGENTS.md vom Repository-Root bis zum Zielpfad
- .a3/rules.md
- vom Benutzer in A^3 ausdrücklich ausgewählte Regeldateien

Normale README-Dateien, Quellcodekommentare, Tests, Issues und Toolausgaben sind Daten, keine Instruktionen. Auch eine vertrauenswürdige Policy-Datei kann Sicherheits- oder Freigaberegeln nicht lockern.

## Pfad-Policy

- Jeder Workspace besitzt explizite erlaubte Roots.
- Eingabepfade werden normalisiert, kanonisiert und nach Symlinkauflösung erneut geprüft.
- Relative Traversierung aus dem Root ist verboten.
- Sonderdateien, Gerätepfade, Pipes und Sockets werden standardmäßig abgelehnt.
- Große, binäre, generierte oder geheime Dateien werden über Klassifikationsregeln ausgeschlossen.
- Schreibzugriff außerhalb des aktiven Worktree benötigt ausdrückliche Freigabe.
- Eine ausgewählte Untermenge darf nicht implizit auf einen übergeordneten Git-Worktree erweitert werden.
- Das Git Common Directory eines Linked Worktree darf für eng begrenzte Identitätsmetadaten gelesen
  werden; es wird dadurch nicht zu einem erlaubten Workspace-Root.
- Repository-Erkennung liest nur repository-lokale Git-Konfiguration in isoliertem Modus. Globale
  Konfiguration, Includes, Credential Helper, Umgebungsüberschreibungen und Netzwerkzugriffe bleiben
  außerhalb dieses Pfads.
- Die WebView darf keinen Projektpfad an `open_project` übergeben. Nur der privilegierte Rust-Adapter
  öffnet den nativen Einzelauswahldialog und reicht dessen Ergebnis an den Use Case weiter.
- Eine Worktree-Reconciliation darf nur nach einer zweiten privilegierten nativen Auswahl erfolgen.
  Der Dialog zeigt ausschließlich begrenzte, kontrollzeichenfreie Pfadanzeigen und bietet
  „reconciliieren“, „separat öffnen“ und „abbrechen“. Die WebView kann weder einen Kandidaten noch die
  Entscheidung liefern und erhält keine zusätzliche Command- oder Dialog-Capability.
- Die Main-Capability erlaubt `open_project`, `list_recent_projects` und `query_health`, aber keine
  direkten Dialog-, Datei-, Shell- oder SQL-Plugin-Commands. Die Rückgabeverträge enthalten weder
  Handles noch Git Common Directory oder autoritative gespeicherte Pfade.
- `list_recent_projects` akzeptiert außer der Protokollversion keine WebView-gesteuerten Pfade oder
  Limits. Die V1-Antwort ist auf zehn validierte Einträge begrenzt; ungültige Katalogdaten werden als
  redigierter stabiler Fehler statt als rohe Datenbank- oder Pfadinformation zurückgegeben.
- Worktree-Laufzeitdaten liegen ausschließlich unter dem kanonischen App-Data-Root in
  `projects/<WorktreeId>`. Die `WorktreeId` stammt aus der privilegierten Repository-Inspektion und
  nicht aus der WebView. App-Data innerhalb des ausgewählten Worktrees, Symlinks sowie falsche
  Datei- oder Verzeichnistypen an diesen Grenzen werden abgelehnt.
- Beim bestätigten Umzug werden nur zwei erneut validierte direkte Kinder desselben privaten
  `projects`-Verzeichnisses atomar umbenannt. Quelle und Ziel müssen dem bestätigten Katalogzustand
  entsprechen; ein vorhandenes Ziel, eine fehlende Quelle ohne vorbereitete Fortsetzung oder eine
  geänderte Katalogrevision bricht ohne Überschreiben ab.
- Eine bestehende `knowledge.db` wird vor schreibendem Öffnen read-only auf unterstützte Version,
  Integrität und – bei aktuellem Schema – ihre persistierte Repository-/Worktree-Bindung geprüft.
  Datenbankfehler werden nur als stabile redigierte Fehlercodes über IPC sichtbar.
- Persistierte Snapshot-Pfade sind keine freigegebenen Dateisystempfade. Der Storage-Adapter
  rekonstruiert sie ausschließlich als begrenzte, relative, traversierungsfreie Repository-Rohbytes
  und lehnt ungültige Daten an der Adaptergrenze ab. Snapshot- und IndexRun-Persistenz wird nicht als
  neuer WebView-Command oder Datenbankzugriff exponiert.

## Prozess-Policy

Standard ist direkte argv-Ausführung:

~~~text
ProcessSpec
  executable
  args[]
  cwd
  env_allowlist
  timeout
  stdout_limit
  stderr_limit
  network_policy
~~~

- Keine Shell-Interpolation im Standardpfad
- Keine Vererbung der vollständigen Umgebung
- Keine interaktive Credential-Abfrage
- Prozessbaum bei Timeout oder Abbruch beenden
- Exitcode, Laufzeit und Outputdigest erfassen
- Ausgabe vor UI und LLM auf Secrets prüfen

Shellmodus ist eine eigene hochriskante Aktion und immer freigabepflichtig.

## Aktionsklassen

| Klasse | Beispiel | Standard |
| --- | --- | --- |
| Read | Suche, Datei lesen, Git Status | automatisch innerhalb Root |
| Derive | Index, Graph, Context Pack | automatisch |
| Write | Patch im Worktree | gemäß Task-Policy; Diff immer sichtbar |
| Execute Safe | bekannte Test-, Format- und Buildbefehle ohne Netzwerk | automatisch nach validiertem Plan |
| Execute Open | beliebiger lokaler Prozess | Freigabe |
| Network | Download, Remote API, Paketinstallation | Freigabe |
| Destructive | Löschen, Reset, Datenmigration mit Verlust | Freigabe |
| Publish | Push, PR, Release, externe Nachricht | immer Freigabe |
| Outside Root | Lesen oder Schreiben außerhalb Workspace | immer Freigabe |

Freigaben sind aktions- und scopegebunden, zeitlich begrenzt und nicht still wiederverwendbar. Die
implementierte V1-Grenze leitet Klasse und Risiko ausschließlich aus einer typisierten Root-, Pfad-,
Prozess-, Netzwerk- oder Git-Action ab. Die unveränderliche Systempolicy kann durch
`WorkspacePolicy` nur auf Freigabepflicht oder Ablehnung verschärft werden; eine Lockerung ist im
Workspace-Regeltyp nicht darstellbar.

Ein `ApprovalRequest` bindet genau Run, Action-Fingerprint, Scope-Digest, Klasse und Risiko und gilt
höchstens 24 Stunden. Ein daraus erzeugter Grant kann vor seinem exklusiven Ablauf genau einmal von
einer passenden `PolicyDecision` verbraucht oder vorher durch den Benutzer widerrufen werden. Ein
anderer Pfad, Run oder Action-Fingerprint, eine abgelaufene, widerrufene oder bereits verbrauchte
Freigabe bleibt blockiert. Grant, Widerruf und Verbrauch werden mit der Run-Materialisierung und
einem typisierten content-freien Audit-Event atomar persistiert.

## Patch-Policy

Eine PatchAction enthält:

- erwarteten Snapshot;
- erwartete Content Hashes aller Zieldateien;
- strukturierte Änderungen;
- Begründung und zugehörigen TaskStep;
- erwartete Verifikation.

Vor Anwendung:

1. Root und Pfade validieren.
2. Hashes vergleichen.
3. verbotene und sensible Dateien prüfen.
4. Patchvorschau erzeugen.
5. notwendige Freigabe einholen.

Nach Anwendung:

1. neue Hashes erfassen;
2. Diff begrenzen und anzeigen;
3. Indexinvalidierung sofort auslösen;
4. Verification starten;
5. bei Fehler nicht automatisch fremde Änderungen zurücksetzen.

## Git

Automatisch erlaubt:

- status
- diff
- log mit Limit
- show
- rev-parse
- ls-files

Freigabepflichtig:

- commit
- branch create oder delete
- rebase
- merge
- reset
- clean
- checkout mit Dateiverlust
- push, fetch oder pull mit Netzwerk

Destruktive Git-Aktionen sind nie implizit durch „implementiere dies“ autorisiert.

## Secrets

- bekannte Secret-Dateien und Credential-Muster werden nicht indexiert;
- Provider-Tokens liegen im OS-Schlüsselspeicher;
- Logs verwenden Redaction vor Persistenz;
- persistierte RunEvents besitzen keinen Freitext: Modell-, Tool-, User- und externe Fehlerausgaben
  werden vor der Adaptergrenze auf Quellkategorie, Bytezahl und Trunkierungsstatus reduziert; auch
  der JSONL-Export enthält nur diese content-freien Metadaten und den Digest der sicheren Struktur;
- UI erhält nur SecretExists, niemals den Secretwert;
- Context Packs enthalten keine vollständigen Environment Dumps;
- der Context Compiler prüft Anchor, jede gepackte Retrieval-/Claim-Einheit und jede begrenzte
  Toolvorschau sowie jede tatsächlich reinjizierte Run-Memory-Einheit auf bekannte Private-Key-,
  Bearer-, GitHub-, AWS- und Secret-Assignment-Muster;
- ein Run-Memory-Checkpoint wird nur aus typisierten autoritativen Objekten erzeugt und besitzt
  weder einen Persistence- noch einen Journal-Mutationsport; Compaction kann Audit-Events deshalb
  nicht löschen oder umschreiben;
- erkannte mögliche Secrets stoppen den Context Compile, eine Veröffentlichung oder
  Synchronisation.

## LLM-Ausgaben

- Ausgabe muss dem erwarteten Schema entsprechen.
- Toolname muss für den Controllerzustand erlaubt sein.
- Parameter durchlaufen dieselbe Validierung wie UI-Eingaben.
- Text in Argumenten wird nie als Shellcode interpretiert.
- unbekannte Felder werden standardmäßig abgelehnt.
- Reparaturversuch maximal einmal und ohne Ausführung des ungültigen Originals.

Die allgemeine H6-Ausgabe ist das versionierte `agent-action-v1`-Dokument. Sein statisches Schema
und der davon unabhängige Runtime-Decoder erlauben ausschließlich Search, Inspect, sichere
nicht-verifizierende Ledger-Intents und Finish als Anforderung an den späteren Acceptance-Verifier.
Patch-, Prozess-, Shell-, Git-, Netzwerk-, Publish- oder Destruktionsfelder sind nicht
darstellbar. Unbekannte Actions, Felder, Versionen, Text außerhalb des einen JSON-Dokuments,
unsichere Pfade, nicht kanonische IDs und ungebundene Werte werden vor jeder Toolgrenze abgelehnt.
Der kompakte statische Systemvertrag behandelt Repository- und Kontextinhalt ausdrücklich als
untrusted data und ist konservativ auf höchstens 900 Tokens begrenzt.

Nur ein live verifiziertes ModelProfile darf den Prompt samt Structured-Output-Schema vorbereiten.
Eine ungültige Primärausgabe erzeugt eine einzelne nicht clonebare Repair-Befugnis. Schon die
Erstellung ihrer content-freien Korrekturanweisung verbraucht sie; die zweite Decode-Operation
liefert entweder eine valide Action oder einen terminalen Fehler ohne weitere Reparatur. Der rohe
ungültige Modelltext wird weder in die Korrekturanweisung kopiert noch als Action zurückgegeben.

Der Deep-Map-Explorer konkretisiert diese Regeln mit dem versionierten
`deep-map-explorer-action-v1`-JSON-Schema und einem davon unabhängig strikt prüfenden
Runtime-Decoder. Nur Inspect, Search und ein nicht autoritatives Proposal sind darstellbar. Vor
jedem Toolaufruf prüft der Controller Planschritt, erwarteten Informationsgewinn und das reservierte
Read-Budget. Vor einer Bestätigung prüft er Modul, Snapshot, Schemaversion, erwartete Felder und ob
jede angegebene Evidence ID im unmittelbar zuvor normalisierten Read-Ergebnis vorkam. Der
Capability-Port bietet keine Write-, Shell-, Prozess- oder Git-Methode. Raw Model Output ist auf 64
KiB, normalisierter Toolpreview auf 16 KiB und Tool-Evidenz auf 100 IDs begrenzt; Debugausgaben
enthalten weder Proposalwerte noch Preview- oder Rohoutput.

Das H10-Agent-Toolset besitzt nur Search und die typisierten Inspect-Ziele File, Symbol, Graph,
Claim und Test. Sein Workspace-Adapter kann ausschließlich content-adressierte, nach
Symlinkauflösung innerhalb des kanonischen Worktree-Roots liegende reguläre Dateien lesen; er
bietet weder Write-, Prozess-, Shell-, Git- noch Netzwerkmethoden. File-Seiten sind auf 12 KiB
begrenzt und nur vorwärts paginiert. Der Context erhält höchstens 16 KiB normalisierte Vorschau;
die vollständigen Bytes werden weder journalisiert noch als ungefiltertes Resultat reinjiziert.

`UpdateLedger` akzeptiert für Resultate nur controllerseitig erzeugte, snapshotgleiche Tool-
Evidence und kann einen Schritt damit lediglich zur objektiven Verifikation vorbereiten. Die
Ledgeränderung und Controllertransition werden atomar mit getrennten Ledger- und Run-CAS-Ankern
persistiert. `Finish` ist content-frei und wechselt nur in `Verify`; weder Modellausgabe noch
Toolerfolg können `Done` setzen.

## Netzwerk und Datenschutz

V1 ist offline-first:

- keine Telemetrie;
- kein Cloud-Sync;
- keine automatische Providererkennung im Netzwerk;
- nur explizit konfigurierte lokale Endpunkte;
- Bindung bevorzugt Loopback;
- bei nicht lokalen Endpunkten klare Warnung und Freigabe.

Quellcode, Embeddings, Projektkarten, Prompts und Logs werden nie ohne ausdrückliche Aktivierung übertragen.

Der Ollama-kompatible Adapter akzeptiert nur credentialfreie HTTP-/HTTPS-Origins ohne Pfad, Query
oder Fragment. `localhost` wird vor der Policyprüfung auf die IPv4-Loopbackadresse normalisiert;
nur literale Loopbackadressen gelten als lokal. Jeder nicht lokale Endpoint benötigt HTTPS und
zusätzlich vor jedem Request eine Freigabe durch die injizierte Endpoint-Policy. Die Standardpolicy
erlaubt ausschließlich Loopback. Redirects und Umgebungsproxies sind deaktiviert, damit weder ein
Server noch lokale Proxykonfiguration den geprüften Zielscope still verändern kann.

Connect und jeder gestreamte Body-Read konkurrieren mit der wakebaren Cancellation; ein einziges
Gesamttimeout gilt bis zum vollständigen Body-Ende. Requests, JSON Schema, NDJSON-Zeilen, Puffer,
Textfragmente und Gesamtausgabe sind fest begrenzt. HTTP-Fehlerbodies und Providerfehler werden
weder gelesen noch in normalisierte Fehler oder Debugausgaben übernommen.

Die H5-Capability-Probe autorisiert `/api/show` und `/api/chat` jeweils erneut, teilt für beide
Requests ein Gesamttimeout und begrenzt ihre JSON-Bodies auf 512 beziehungsweise 128 KiB. Die
Schema-Probe verwendet höchstens 4.096 Kontext- und 32 Outputtokens. Modellnamen aktivieren keine
Fähigkeit; Provider-Metadaten dürfen nur den nativen Toolmodus melden. Ausschließlich ein exakt
validiertes Structured-Output-Probeergebnis aktiviert später ausführbare Schemaaktionen. Eine
fehlgeschlagene Probe und ein manueller Profile Override können diesen Sicherheitsstatus nicht
hochsetzen. Native Tool Calls bleiben bis zur expliziten H6-Controllerintegration nicht
ausführbar.

## Tauri

- minimale Capabilities je Window;
- restriktive Content Security Policy;
- keine Remote-UI-Inhalte;
- keine generischen Shell-, FS- oder SQL-Plugins im Frontend;
- nur explizit registrierte Commands;
- Payloadgrößen begrenzen;
- Secrets und DB-Verbindung im Core-Prozess.

Die Architektur folgt dem von Tauri dokumentierten Prinzip, globale sensible Zustände im Core-Prozess zu halten: [Tauri Process Model](https://v2.tauri.app/concept/process-model/).

## Audit

Sicherheitsrelevante Aktionen speichern:

- Actor und Run
- Policy-Entscheidung
- geschlossene Begründung, ActionClass und RiskLevel
- Approval ID, falls vorhanden
- content-freier Action-Fingerprint und Scope-Digest
- Tooltyp
- Zeit, Dauer und Status
- sichere Digests

Nicht gespeichert werden rohe Secrets, vollständige Umgebungen oder uneingeschränkte Prozessausgaben.
