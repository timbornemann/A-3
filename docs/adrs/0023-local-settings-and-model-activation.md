# ADR-0023: Lokale Settings-Snapshots und evidenzgebundene Modellaktivierung

Status: Accepted

Datum: 2026-08-13

Entscheider: Tim Bornemann

## Kontext

Plan 06/U8 macht den bereits implementierten `ModelProvider`-Port, die Ollama-kompatible lokale
Adaptergrenze und `ModelProfile` erstmals als Desktopkonfiguration zugänglich. H4 und H5 legen
fest, dass Endpunktvalidierung und Transport im Provideradapter bleiben und ausschließlich ein
realer, begrenzter Structured-Output-Probe ausführbare Modellaktionen freischalten darf. Bisher
gibt es jedoch weder dauerhafte Anwendungssettings noch eine Rollenbindung für Coding, Mapping und
Embedding oder einen festgelegten Lebenszyklus bei einem Endpunktwechsel.

Die WebView ist unprivilegiert. Sie darf deshalb keinen freien Endpunkt unmittelbar für einen
Netzwerkaufruf verwenden, keine Capabilitybehauptung liefern und kein ungeprüftes Profil als
ausführbar markieren. Umgekehrt soll ein bewusst eingetragener nicht lokaler Endpunkt sichtbar
bleiben können, ohne dass bereits das Speichern eine Netzwerkfreigabe oder einen Request auslöst.

Indexignore und sichere Command-Allowlist existieren bereits mit unterschiedlichen Autoritäten:
ADR-0017 erlaubt ausschließlich Repository-Git-Ignores, sichere globale Defaults und
`[discovery].ignore` aus `.a3/project.toml`. E5 bindet eine Benutzerbestätigung an den exakten,
evidenzabhängigen `ProjectCommandCatalog`. Eine allgemeine Settings-Tabelle darf keine dieser
Grenzen duplizieren oder lockern.

## Entscheidung

- Globale Desktopsettings werden hinter einem Application-Port als vollständige, monoton
  revisionierte V1-Snapshots im lokalen `catalog.db` gespeichert. Jeder Snapshot enthält nur
  normalisierte, credential-freie Providerkonfiguration, Profil- und Probe-Metadaten sowie
  content-freie Privacyzustände. Sourceinhalt, Prompts, Antworten, Secrets, Auth-Header und
  Credentials gehören nicht in diesen Store.
- Ein leerer Store entspricht einem gültigen expliziten `Unconfigured`-Snapshot. Fast Index,
  Exact/FTS/Graph, Map und alle anderen modellfreien Ansichten bleiben vollständig nutzbar. Weder
  Appstart noch Settings-Read startet Providererkennung, Netzwerk oder GPU-Arbeit.
- Der konkrete Provideradapter validiert und kanonisiert einen eingegebenen Ollama-kompatiblen
  credential-freien Origin. Der Application-Snapshot speichert den kanonischen Origin zusammen
  mit `LocalLoopback` oder `Remote`. Ein Endpunktwechsel verwirft sämtliche von der vorherigen
  Providerinstanz abgeleiteten Kandidaten, Aktivierungen und Health-Evidence atomar.
- Ein Remote-Origin darf ohne Netzwerkzugriff gespeichert und deutlich als `RemoteBlocked`
  angezeigt werden. U8 führt keinen Probe- oder Modellrequest dorthin aus. Eine spätere Nutzung
  benötigt weiterhin eine exakte requestgebundene Netzwerkfreigabe gemäß ADR-0012; es gibt keinen
  breiten „Remote erlauben“-Schalter und keine wiederverwendbare Hostfreigabe.
- Coding und Mapping besitzen je genau einen optionalen `ModelProfile`-Kandidaten. Ein Probe ist
  eine ausdrückliche Nutzeraktion, lädt den Endpunkt ausschließlich aus dem aktuellen Core-
  Snapshot, verwendet feste Timeouts, Cancellation und begrenzte Antworten und speichert das
  Ergebnis mit Core-eigener Zeit. Nur `ModelStructuredOutputCapability::Verified` wird für die
  jeweilige Rolle aktiviert. `Unavailable`, Providerfehler, widersprüchliche Kontextmetadaten,
  veraltete Settingsrevisionen und Remote-Endpunkte bleiben nicht ausführbar.
- Embedding besitzt ein getrenntes `EmbeddingModelProfile`. Seine Aktivierung verlangt einen
  realen begrenzten Provider-Probe, der mindestens einen endlichen, nicht leeren Vektor liefert
  und die exakte Dimension innerhalb der bestehenden Domainlimits beobachtet. Ein frei
  eingegebener Dimensionswert oder Modellname ist keine Capability-Evidence.
- Provider Health ist keine automatische Hintergrundüberwachung. Die Oberfläche zeigt
  `NotConfigured`, `NotChecked`, `Checking`, `Healthy`, `CapabilityLimited`, `Unreachable`,
  `Cancelled` oder `RemoteBlocked` ausschließlich aus dem aktuellen Endpunkt und dem letzten
  expliziten Probe. Ein Healthstatus kann keine Profilaktivierung ersetzen.
- Kontext-, Output-, Parallelitäts- und Embedding-Batchlimits werden als vorhandene Domain-
  Newtypes validiert. Sampling, Stopsequenzen, Tokenzählung und Schema-Grounding erhalten feste,
  dokumentierte V1-Defaults, solange U8 dafür keine eigene erweiterte Oberfläche anbietet.
- Die Privacyprojektion ist fail-closed und macht die realen Produktgrenzen sichtbar: Telemetrie,
  Cloud-Synchronisierung, automatische Providererkennung, Prompt-/Antwort-Logging und Remote-
  Requests ohne exakte Freigabe sind aus. Nicht implementierte Fähigkeiten werden nicht als
  wirkungslose editierbare Schalter dargestellt.
- Projektbezogene Settings werden separat aus dem aktiven, Core-eigenen Projekt abgeleitet.
  Indexignore zeigt die validierten ausschließenden Muster aus `.a3/project.toml` sowie die
  unveränderlichen Git-/Safety-Quellen read-only. U8 schreibt keine Repositorykonfiguration
  außerhalb des normalen Patch- und Diffpfads.
- Die Command-Allowlist-Ansicht leitet den Katalog erneut aus dem jüngsten atomar publizierten
  Index ab. Eine Bestätigung akzeptiert nur die sichtbare Katalogrevision, Store-CAS-Version und
  ausgewählte Command-IDs; der Core lädt Index und Katalog vor dem Commit erneut. Stale Evidence,
  ein Projektwechsel oder ein unbekannter Command bewirken keine Änderung.
- Die IPC-Grenze ist versioniert und geschlossen. Endpunktkonfiguration akzeptiert nur den
  einzutragenden Origin und die erwartete Settingsrevision. Probe-Commands akzeptieren Rolle,
  opaque Modell-ID und validierbare Ressourcenlimits, aber weder Endpunkt, Provider-ID,
  Capabilitystatus, Profil-ID, Healthstatus noch Zeit. Projektsettings akzeptieren keine Pfade,
  Worktree-IDs oder frei erzeugten Commands.

## Konsequenzen

### Positiv

- Ein WebView-Reload oder Appneustart verliert keine bestätigte lokale Konfiguration, während der
  komplett modellfreie Startzustand weiterhin ein vollwertiger Indexbrowser ist.
- Capability-Evidence kann nicht durch Modellnamen, UI-Felder oder manuelle Statusänderungen
  ersetzt werden. Ein Endpunktwechsel kann alte Evidence nicht still weiterverwenden.
- Remote-Konfiguration ist ehrlich sichtbar, öffnet aber weder eine Netzwerkgrenze noch einen
  pauschalen Approvalpfad.
- Indexignore und Command-Allowlist behalten ihre vorhandenen, stärkeren Autoritäten und werden
  nicht zu allgemeinen Preferences degradiert.

### Negativ

- Das Speichern eines Remote-Origins macht ihn noch nicht nutzbar.
- Nach jedem Endpunktwechsel müssen alle drei Rollen erneut geprüft werden.
- Indexignore ist in U8 nur sichtbar; Änderungen erfolgen weiterhin als normale überprüfbare
  Repositoryänderung an `.a3/project.toml`.
- Provider Health wird nur nach einer bewussten Probe aktuell und ist keine permanente
  Verfügbarkeitsanzeige.

### Risiken und Gegenmaßnahmen

- Settings ändern sich während eines Probe — Snapshotrevision und Endpunkt werden vor Persistenz
  erneut per Compare-and-Swap geprüft; ein spätes Ergebnis wird verworfen.
- Ein Provider liefert sehr große oder manipulierte Antworten — bestehende Transportlimits,
  Strict-Schema-Validierung, Dimensionsgrenzen und Timeout/Cancellation bleiben am Adapterrand.
- Eine alte Allowlist erscheint weiter gültig — Katalog-ID bindet jede Manifestrevision; Query und
  Mutation klassifizieren abweichende IDs als stale und führen keinen Command aus.
- Settings enthalten versehentlich Credentials — die Endpointgrammar verbietet Userinfo, Query,
  Fragment und Pfade; Debug- und IPC-Tests prüfen Redaction und unbekannte Felder.

## Verworfene Alternativen

- Profile aus Modellnamen ableiten — liefert keine Capability-Evidence und widerspricht H5.
- Endpunkt direkt im Probe-Command annehmen — würde eine freie WebView-Netzwerkfähigkeit schaffen.
- Remote-Provider über einen dauerhaften Toggle erlauben — widerspricht der exakten
  Approvalbindung aus ADR-0012.
- Persönliche globale Indexignore-Muster in `catalog.db` speichern — verändert Discovery V1 und
  widerspricht ADR-0017.
- Beliebige Commands in Settings eingeben — umgeht den evidenzgebundenen E5-Katalog.
- Provider automatisch beim Appstart prüfen — verletzt Offline-by-default und kann unerwartet
  Netzwerk- oder GPU-Arbeit auslösen.

## Compliance

- Application- und Storage-Contracts prüfen leeren Start, Snapshot-CAS, Reopen, Endpunktwechsel
  mit Profilinvalidierung, Probe-Erfolg/-Fehler/-Abbruch und die Aktivierungssperre für nicht
  verifizierte Profile.
- Provider-Contracts prüfen lokale Endpointkanonisierung, Credential- und Remotegrenzen,
  strukturierte Capabilityantworten sowie begrenzte Embeddingdimensionen vollständig offline.
- IPC-Tests lehnen Endpoint- oder Capabilityfelder in Probe-Requests, unbekannte Felder, ungültige
  Limits, freie Projektpfade und stale Revisionen ab.
- Component-Tests belegen den modellfreien Indexbrowser, die deutliche Remote-Warnung, bewusste
  Probe/Abbruch-Aktionen, Rollenstatus, Ressourcenlimits, Privacygrenzen, Indexignore und die
  evidence-gebundene Command-Auswahl.

## Referenzen

- [ADR-0002](0002-tauri-rust-svelte-desktop.md)
- [ADR-0004](0004-libsql-local-persistence.md)
- [ADR-0005](0005-worktree-scoped-storage.md)
- [ADR-0011](0011-local-model-provider-abstraction.md)
- [ADR-0012](0012-safe-tools-and-approval-policy.md)
- [ADR-0017](0017-bounded-repository-discovery.md)
- [ADR-0018](0018-model-provider-port-ownership.md)
- [Architektur](../ARCHITECTURE.md)
- [Domainmodell](../DOMAIN_MODEL.md)
- [Security und Execution](../SECURITY_AND_EXECUTION.md)
- [Desktop Product U8](../plans/06-DESKTOP_PRODUCT.md#u8-settings-und-model-health)
