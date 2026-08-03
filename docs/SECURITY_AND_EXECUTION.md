# Sicherheit und kontrollierte Ausführung

Status: verbindliche Baseline  
Stand: 2026-08-03

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

Freigaben sind aktions- und scopegebunden, zeitlich begrenzt und nicht still wiederverwendbar.

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
- UI erhält nur SecretExists, niemals den Secretwert;
- Context Packs enthalten keine vollständigen Environment Dumps;
- erkannte mögliche Secrets stoppen eine Veröffentlichung oder Synchronisation.

## LLM-Ausgaben

- Ausgabe muss dem erwarteten Schema entsprechen.
- Toolname muss für den Controllerzustand erlaubt sein.
- Parameter durchlaufen dieselbe Validierung wie UI-Eingaben.
- Text in Argumenten wird nie als Shellcode interpretiert.
- unbekannte Felder werden standardmäßig abgelehnt.
- Reparaturversuch maximal einmal und ohne Ausführung des ungültigen Originals.

## Netzwerk und Datenschutz

V1 ist offline-first:

- keine Telemetrie;
- kein Cloud-Sync;
- keine automatische Providererkennung im Netzwerk;
- nur explizit konfigurierte lokale Endpunkte;
- Bindung bevorzugt Loopback;
- bei nicht lokalen Endpunkten klare Warnung und Freigabe.

Quellcode, Embeddings, Projektkarten, Prompts und Logs werden nie ohne ausdrückliche Aktivierung übertragen.

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
- Approval ID, falls vorhanden
- Scope
- Tooltyp
- Zeit, Dauer und Status
- sichere Digests

Nicht gespeichert werden rohe Secrets, vollständige Umgebungen oder uneingeschränkte Prozessausgaben.

