# IPC-Protokoll

Status: verbindliche Baseline
Stand: 2026-08-04

## Zweck und Grenze

Das IPC-Protokoll ist die einzige Datengrenze zwischen der unprivilegierten WebView und dem privilegierten Rust-Kern. `a3-protocol` enthält ausschließlich primitive, versionierte Grenztypen und besitzt keine Abhängigkeit auf Domain, Application, Tauri oder Adapter.

Domain- und Protocol-Typen werden im Desktop-Composition-Root explizit gemappt. Ein Protocol-DTO darf niemals als Domänenobjekt verwendet werden. Die DTOs werden mit Serde als JSON gebunden; Feldnamen am WebView-Rand verwenden `camelCase`. Eingaben lehnen unbekannte Felder ab.

## Versionierung

- Jede IPC-Nachricht trägt eine `ProtocolVersion`.
- Die erste und aktuell emittierte Version ist `1`.
- Eine inkompatible Schemaänderung benötigt eine neue Protokollversion und einen dokumentierten Migrations- oder Ablehnungspfad.
- Unbekannte Eingabeversionen werden am IPC-Rand abgelehnt und niemals als ausführbare Eingabe interpretiert.

## Health Request V1

Der Tauri-Command heißt `query_health`. Sein einziges Argument ist `request` mit diesem Inhalt:

| JSON-Feld | Typ | Invariante |
| --- | --- | --- |
| `protocolVersion` | `u16` über `ProtocolVersion` | für V1 exakt `1` |

Zusätzliche Felder, ein fehlender Request oder ein nicht numerischer Versionswert werden vor Ausführung des Use Cases abgelehnt.

## Health Response V1

`HealthResponseV1` besitzt folgende logische Felder:

| Feld | Typ | Invariante |
| --- | --- | --- |
| `protocolVersion` | `u16` über `ProtocolVersion` | exakt `1` |
| `applicationVersion` | String | stammt aus validierter `ApplicationVersion` |
| `platform` | `PlatformV1` | `windows`, `linux`, `macOs` oder `unsupported` |
| `status` | `HealthStatusV1` | im Walking Skeleton `Ready` |

Der Application-Use-Case liefert ein Domain-`Health`. Erst der Composition-Root kopiert die validierte Anwendungsversionskennung in den primitiven Protocol-String und setzt die aktuelle Protokollversion. Dadurch importiert weder die Domain Protocol-Typen noch das Protocol-Crate Domänentypen.

## Command Error V1

Ein syntaktisch gültiger Request mit einer nicht unterstützten Protokollversion erhält einen sicheren, serialisierbaren Fehler:

| JSON-Feld | Typ | V1-Wert |
| --- | --- | --- |
| `protocolVersion` | `u16` | `1` |
| `code` | `ErrorCodeV1` | `unsupportedProtocolVersion` |
| `message` | String | sichere Meldung ohne interne Fehlerdetails |

Die Desktop-Capability `main-capability` erlaubt dem Hauptfenster ausschließlich `allow-query-health`. Das Walking Skeleton registriert keine generische Datei-, Shell- oder SQL-Capability.
