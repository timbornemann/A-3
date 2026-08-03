# IPC-Protokoll

Status: verbindliche Baseline
Stand: 2026-08-04

## Zweck und Grenze

Das IPC-Protokoll ist die einzige Datengrenze zwischen der unprivilegierten WebView und dem privilegierten Rust-Kern. `a3-protocol` enthält ausschließlich primitive, versionierte Grenztypen und besitzt keine Abhängigkeit auf Domain, Application, Tauri oder Adapter.

Domain- und Protocol-Typen werden im Desktop-Composition-Root explizit gemappt. Ein Protocol-DTO darf niemals als Domänenobjekt verwendet werden. Die konkrete Tauri- und Serialisierungsbindung wird erst im transportgebundenen F3-Schnitt ergänzt.

## Versionierung

- Jede IPC-Nachricht trägt eine `ProtocolVersion`.
- Die erste und aktuell emittierte Version ist `1`.
- Eine inkompatible Schemaänderung benötigt eine neue Protokollversion und einen dokumentierten Migrations- oder Ablehnungspfad.
- Unbekannte Eingabeversionen werden am IPC-Rand abgelehnt und niemals als ausführbare Eingabe interpretiert.

## Health Response V1

`HealthResponseV1` besitzt folgende logische Felder:

| Feld | Typ | Invariante |
| --- | --- | --- |
| `protocol_version` | `u16` über `ProtocolVersion` | exakt `1` |
| `application_version` | String | stammt aus validierter `ApplicationVersion` |
| `status` | `HealthStatusV1` | im Walking Skeleton `Ready` |

Der Application-Use-Case liefert ein Domain-`Health`. Erst der Composition-Root kopiert die validierte Anwendungsversionskennung in den primitiven Protocol-String und setzt die aktuelle Protokollversion. Dadurch importiert weder die Domain Protocol-Typen noch das Protocol-Crate Domänentypen.
