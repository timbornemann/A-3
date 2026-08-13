# ADR-0026: Explizite lokale Providerverwaltung und begrenzte Modellerkennung

Status: Accepted

Datum: 2026-08-14

Entscheider: Tim Bornemann

Supersedes: Teile von ADR-0023

## Kontext

ADR-0023 führt genau einen aktiven, credential-freien Modellendpunkt ein. Er wird ohne
Netzwerkzugriff gespeichert und ausschließlich durch ausdrückliche Capability-Probes verwendet.
Die erste Oberfläche stellt diesen Endpunkt jedoch als einzelnes technisches Textfeld dar und
verlangt für Coding, Mapping und Embedding frei eingegebene Modell-IDs. Das ist fehleranfällig und
bildet einen Provider nicht als verständliche, verwaltbare Desktopressource ab.

Ollama stellt mit `/api/tags` einen lokalen Modellkatalog bereit. Eine Abfrage dieses Katalogs ist
keine Capability-Evidence: Ein gelisteter Name beweist weder Structured Output noch Tool Calling,
Kontextgröße oder Embeddingdimension. Sie kann die Auswahl dennoch sicher vereinfachen, wenn sie
bewusst gestartet, begrenzt und weiterhin an den Core-eigenen aktiven Endpunkt gebunden wird.

## Entscheidung

- A^3 zeigt den aktiven Modellendpunkt als verwaltbare Providerverbindung. Erstellen, Bearbeiten
  und Entfernen bleiben optimistische, revisionsgebundene Settings-Mutationen. Speichern oder das
  bloße Öffnen der Settings startet weiterhin keinen Provider-, Netzwerk- oder GPU-Zugriff.
- Die Providerart ist eine geschlossene, versionierte Auswahl. Dieser Slice unterstützt
  ausschließlich `Ollama`. Weitere Arten benötigen einen konkreten Adapter, dieselben Contracts und
  eine Protokollerweiterung; es gibt keinen generischen OpenAI-kompatiblen oder frei
  parametrisierbaren WebView-Netzwerkpfad.
- V1 besitzt weiterhin genau eine aktive Providerverbindung. Das passt zur vorhandenen
  eindeutigen Rollen- und Executorbindung. Mehrere gleichzeitig aktive Providerinstanzen sind
  kein vorgetäuschter UI-Zustand und benötigen vor ihrer Einführung eine eigene Persistenz- und
  Aktivierungsentscheidung.
- Eine Modellerkennung ist ausschließlich eine ausdrückliche Nutzeraktion. Der neue
  Application-Port erhält keine URL aus der WebView. Der Composition Root lädt den aktuellen
  revisionsgebundenen Endpunkt, lehnt Remote-Endpunkte ab und wählt anhand der gespeicherten
  Provideridentität den konkreten Adapter.
- Der Ollama-Adapter autorisiert `GET /api/tags` erneut über dieselbe Local-only-Endpoint-Policy.
  Redirects und Umgebungsproxies bleiben deaktiviert. Gesamttimeout, wakebare Cancellation,
  JSON-Content-Type, ein 512-KiB-Bodylimit und höchstens 256 eindeutige gültige Modell-IDs begrenzen
  den Request. Providerfehlertexte und nicht benötigte Metadaten verlassen den Adapter nicht.
- Das Ergebnis ist eine flüchtige, providerneutrale Auswahlprojektion. Es wird nicht persistiert,
  startet keinen Download und verändert weder Health noch Rollenprofile. Ein Endpoint- oder
  Settings-Revisionswechsel macht das Ergebnis unbrauchbar.
- Coding, Mapping und Embedding werden in der Oberfläche nur aus der aktuellen Katalogauswahl oder
  einem bereits für die Rolle gespeicherten Profil vorbelegt. Die anschließende explizite
  Capability-Probe bleibt unverändert die einzige Aktivierungsgrenze. Modellkatalog, Modellname
  und UI-Auswahl können keine ausführbare Capability setzen.
- Modell-Probe und Modellkatalog-Abfrage teilen im Composition Root genau einen besessenen,
  kooperativ abbrechbaren Modelloperations-Slot. Dadurch laufen keine konkurrierenden Settings-
  Netzwerkoperationen gegen denselben revisionsgebundenen Endpunkt.

## Konsequenzen

### Positiv

- Nutzer wählen tatsächlich installierte lokale Modelle aus einer kompakten Liste statt
  provider-native Namen fehleranfällig abzutippen.
- Die Oberfläche kann Providerverwaltung und Rollenmodelle getrennt und erweiterbar darstellen,
  ohne Providerdetails in Domain oder WebView-Netzwerkrechte zu verschieben.
- Offline-first und die evidenzgebundene Modellaktivierung bleiben erhalten.

### Negativ

- Nach Appneustart oder Endpointwechsel muss die flüchtige Modellliste bewusst neu geladen werden.
- V1 verwaltet weiterhin nur eine aktive Providerverbindung.
- Ein Modell kann gelistet sein und die anschließende Capability-Probe trotzdem nicht bestehen.

### Risiken und Gegenmaßnahmen

- Ein manipulierter Provider liefert sehr viele oder ungültige Einträge — Body-, Anzahl-,
  Identitäts-, Deduplizierungs- und Sortiergrenzen werden vor IPC erzwungen.
- Die UI verwendet eine Liste nach einem Settingswechsel — Ergebnis und Probe sind an dieselbe
  sichtbare Settingsrevision gebunden; Mutationen verwerfen die Liste sofort.
- Die Erkennung wird als Hintergrundscan missverstanden — sie startet nur über den klar benannten
  Nutzer-Button und besitzt einen sichtbaren Abbruchpfad.
- Ein künftiger Provider wird nur in der UI ergänzt — die geschlossene Protocol-Union und der
  Composition-Root-Match lehnen unbekannte Arten ab, bis ein konkreter Adapter komponiert ist.

## Verworfene Alternativen

- Ollama beim Appstart oder beim Öffnen der Settings automatisch abfragen — verletzt die
  startfreie Offline-Grenze aus ADR-0023.
- Endpoint oder Providerpayload an den Discovery-Command übergeben — erzeugt eine freie
  WebView-Netzwerkfähigkeit.
- Modellnamen aus bekannten Familien vorschlagen — beweist weder lokale Installation noch eine
  Capability.
- Den Modellkatalog als Profilevidence persistieren — vermischt flüchtige Verfügbarkeit mit der
  echten Structured-Output- beziehungsweise Embedding-Probe.
- Eine generische OpenAI-kompatible Providerform ohne implementierten Adapter anzeigen — täuscht
  Nutzbarkeit vor und schwächt die geschlossene Providergrenze.

## Compliance

- Application- und Adaptertests prüfen providerneutrale Ordnung, Limits, Deduplizierung,
  Cancellation, Timeout, Policyablehnung vor Netzwerk und invalide Antworten vollständig offline.
- IPC-Tests lehnen Endpoint, Provider-ID, Modellname, Capability, Zeit und unbekannte Felder im
  Discovery-Request ab.
- Component-Tests belegen modellfreien Start, explizite Provideranlage, bewusste Erkennung,
  Dropdown-Auswahl, Abbruch, Verwerfen nach Settingswechsel und unveränderte Probe-Aktivierung.
- Die Tauri-Capability enthält nur die schmalen Settings-, Discovery- und Cancellation-Commands
  und weiterhin keine generische Netzwerk- oder HTTP-Capability.

## Referenzen

- [ADR-0002](0002-tauri-rust-svelte-desktop.md)
- [ADR-0012](0012-safe-tools-and-approval-policy.md)
- [ADR-0018](0018-model-provider-port-ownership.md)
- [ADR-0023](0023-local-settings-and-model-activation.md)
- [Architektur](../ARCHITECTURE.md)
- [Security und Execution](../SECURITY_AND_EXECUTION.md)
- [Desktop Product U8](../plans/06-DESKTOP_PRODUCT.md#u8-settings-und-model-health)
