# ADR-0012: Sichere typisierte Werkzeuge und zentrale Freigabepolicy

Status: Accepted  
Datum: 2026-08-03

## Kontext

Ein Coding-Agent benötigt Dateizugriff, Patches und Prozesse. LLM-Ausgabe und Repository-Inhalte sind nicht vertrauenswürdig. Eine generische Shell oder ein generischer IPC-Dateizugriff würde die Sicherheitsgrenze auflösen.

## Entscheidung

- Werkzeuge sind schmale, versionierte, typisierte Use Cases.
- Alle Aktionen passieren eine zentrale Policy Engine.
- Pfade werden nach Symlinkauflösung gegen erlaubte Roots geprüft.
- Patches tragen erwartete Content Hashes.
- Prozesse verwenden argv ohne Shell und eine Env-Allowlist.
- Toolausgaben sind begrenzt, redigiert und strukturiert.
- Read und Derive innerhalb des Roots sind automatisch.
- sichere Tests, Builds und Formatter dürfen nach validiertem Plan automatisch laufen.
- Netzwerk, Paketinstallation, Shellmodus, Destruktion, Outside Root und Publishing benötigen ausdrückliche Freigabe.
- Git Push, Merge, Release und externe Writes sind nie implizit autorisiert.

## Konsequenzen

### Positiv

- kleiner Blast Radius
- Aktionen sind auditierbar und testbar.
- Prompt Injection kann nicht direkt Privilegien erlangen.

### Negativ

- weniger Flexibilität als eine freie Shell
- plattformspezifische Prozessbeendigung und Pfadprüfung

### Risiken und Gegenmaßnahmen

- zu viele Freigaben — sichere Klassen eng und messbar erweitern.
- Toolumgehung — Frontend und Modell erhalten keine alternativen privilegierten APIs.

## Verworfene Alternativen

- generisches execute(command_string) — Injection- und Escapingrisiko.
- uneingeschränkter Tauri-FS- oder Shell-Pluginzugriff — umgeht Use Cases.
- vollständige Autonomie — nicht mit sicherem lokalen Entwicklerwerkzeug vereinbar.

## Compliance

Negativtests für Traversal, Symlinks, unbekannte Felder, Shellzeichen, Netzwerknutzung und Approval Scope.

