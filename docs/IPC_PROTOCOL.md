# IPC-Protokoll

Status: verbindliche Baseline

Stand: 2026-08-04

## Zweck und Grenze

Das IPC-Protokoll ist die einzige Datengrenze zwischen der unprivilegierten WebView und dem
privilegierten Rust-Kern. `a3-protocol` enthält ausschließlich primitive, versionierte Grenztypen und
besitzt keine Abhängigkeit auf Domain, Application, Tauri oder Adapter.

Domain- und Protocol-Typen werden im Desktop-Composition-Root explizit gemappt. Ein Protocol-DTO darf
niemals als Domänenobjekt verwendet werden. Die DTOs werden mit Serde als JSON gebunden; Feldnamen am
WebView-Rand verwenden `camelCase`. Eingaben lehnen unbekannte Felder ab.

## Versionierung

- Jede IPC-Nachricht trägt eine `ProtocolVersion`.
- Die erste und aktuell emittierte Version ist `1`.
- Eine inkompatible Schemaänderung benötigt eine neue Protokollversion und einen dokumentierten
  Migrations- oder Ablehnungspfad.
- Unbekannte Eingabeversionen werden am IPC-Rand abgelehnt und niemals als ausführbare Eingabe
  interpretiert.

## Gemeinsamer V1-Request

Die Commands `query_health`, `open_project` und `list_recent_projects` erhalten genau ein Argument
`request`. Ihr V1-Request enthält ausschließlich:

| JSON-Feld | Typ | Invariante |
| --- | --- | --- |
| `protocolVersion` | `u16` über `ProtocolVersion` | für V1 exakt `1` |

Zusätzliche Felder, ein fehlender Request oder ein nicht numerischer Versionswert werden vor
Ausführung des jeweiligen Use Cases abgelehnt. Insbesondere akzeptiert `open_project` keinen Pfad und
`list_recent_projects` weder einen Pfad noch ein WebView-gesteuertes Limit.

## Health Response V1

`query_health` liefert:

| Feld | Typ | Invariante |
| --- | --- | --- |
| `protocolVersion` | `u16` über `ProtocolVersion` | exakt `1` |
| `applicationVersion` | String | stammt aus validierter `ApplicationVersion` |
| `platform` | `PlatformV1` | `windows`, `linux`, `macOs` oder `unsupported` |
| `status` | `HealthStatusV1` | im Walking Skeleton `Ready` |

## Open Project Response V1

`open_project` öffnet genau einen nativen Ordnerdialog im privilegierten Prozess. Erkennt der Kern
danach genau einen evidenzbasierten Umzugskandidaten, darf er zusätzlich einen nativen
Bestätigungsdialog mit „reconciliieren“, „separat öffnen“ und „abbrechen“ anzeigen. Beide Abbruchpfade
liefern `result.kind` `cancelled`. Nach erfolgreicher normaler Registrierung oder bestätigter
Reconciliation lautet es `opened` und enthält `project`:

| Feld | Typ | Invariante |
| --- | --- | --- |
| `repositoryId` | String | 64-stellige kleingeschriebene Hex-ID |
| `worktreeId` | String | 64-stellige kleingeschriebene Hex-ID |
| `worktreeRootDisplay` | String | nicht autoritativ, kontrollzeichenfrei, höchstens 32.768 Zeichen |
| `head` | `GitHeadV1` | `born` mit Objekt-ID oder `unborn` mit vollständiger Referenz |

Der bestehende V1-Vertrag enthält absichtlich keine `ProjectId`; seine JSON-Form bleibt durch die
Katalogregistrierung und Reconciliation unverändert. Kandidat, Evidence, Katalogrevision und
Bestätigungsentscheidung sind interne Application-/Adaptertypen und keine IPC-Felder oder Commands.

## Recent Projects Response V1

`list_recent_projects` liefert most-recent-first höchstens zehn Einträge. Jeder Eintrag enthält eine
64-stellige kleingeschriebene `projectId` und unter `project` dieselbe sichere
`ProjectSummaryV1`-Projektion wie `open_project`. Autoritative gespeicherte Pfadbytes, Git Common
Directory, Remote-URLs, Datenbankzeilen und Adapterfehler werden nicht übertragen.

Die UI lädt diese Projektion beim Start und nach einem erfolgreichen Open erneut. Auswahl, erneutes
Öffnen oder Entfernen eines Katalogeintrags sind nicht Teil dieses V1-Teilschnitts.

## Command Error V1

Ein syntaktisch gültiger Requestfehler erhält einen sicheren, serialisierbaren Fehler:

| JSON-Feld | Typ | V1-Invariante |
| --- | --- | --- |
| `protocolVersion` | `u16` | exakt `1` |
| `code` | `ErrorCodeV1` | stabiler maschinenlesbarer Code |
| `message` | String | feste sichere Meldung ohne interne Fehlerdetails |

Neben den Projektinspektionsfehlern unterscheidet V1 lokale Storage-Nichtverfügbarkeit, Korruption,
eine neuere nicht unterstützte Schemaversion, ungültige persistierte Daten und einen
Projektidentitätskonflikt. Die Fehlermeldung enthält keine SQL-Texte, Enginefehler oder Rohpfade.

## Tauri-Capability

Die Desktop-Capability `main-capability` erlaubt dem Hauptfenster ausschließlich
`allow-query-health`, `allow-open-project` und `allow-list-recent-projects`. Es gibt keine generische
Datei-, Dialog-, Shell- oder SQL-Capability.
