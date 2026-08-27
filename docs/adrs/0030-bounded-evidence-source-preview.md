# ADR-0030: Begrenzte evidence-gebundene Source-Vorschau

Status: Accepted

Datum: 2026-08-27

Entscheider: Tim Bornemann

## Kontext

Die U11-Code-Map soll nicht nur Strukturmetadaten, sondern einen kleinen tatsächlich belegten
Codeausschnitt im Inspector zeigen. Die bestehende U4-Grenze liefert dafür absichtlich nur
Evidence-Metadaten. Ein freier WebView-Dateiread, ein vom Frontend gewählter Pfad oder ein
allgemeiner Source-Browser würde die Trust Boundary aus ADR-0012 auflösen. Repository-Inhalte
bleiben außerdem untrusted und dürfen weder als HTML ausgeführt noch ungeprüft persistiert oder
protokolliert werden.

## Entscheidung

- Die WebView erhält genau einen neuen read-only Use Case für eine bereits vom Core ausgegebene
  Project-Map-Evidence-Auswahl. Der Request enthält keinen frei wählbaren Pfad, Bereich oder
  Größenwert.
- Der Core rekonstruiert und revalidiert die Auswahl gemeinsam mit aktivem Projekt, jüngster
  atomarer Publikation, Modul, Evidence-Mitgliedschaft, Revision und Content Hash. Eine
  Publikations- oder Auswahländerung schließt den Read ohne Teilantwort.
- Source wird ausschließlich für aktuelle Evidence geliefert. Eine `NeedsReview`-Card darf ihre
  weiterhin aktuelle Evidence öffnen; historische `Stale`-Evidence bleibt auf Provenienzmetadaten
  begrenzt.
- Der Workspace-Adapter verwendet die bestehende kanonische Root-, No-Follow-, Dateityp-,
  Größen-, Binary-, Generated- und Secretprüfung. Der Hash wird nach dem Öffnen erneut bestätigt.
- Die Antwort enthält höchstens acht Kontextzeilen vor und nach dem belegten Bereich, insgesamt
  höchstens 64 Zeilen und 16 KiB UTF-8. Sie liefert Plain Text, sichere relative Anzeige,
  Sprachkennung, Zeilennummern und Highlightgrenzen, aber kein HTML.
- Source-Vorschauen werden weder in libSQL noch in Logs, Journal, Telemetrie oder Providerrequests
  geschrieben. Die Capability erzeugt keine Netzwerk-, Shell-, SQL- oder allgemeine
  Dateisystembefugnis.

## Konsequenzen

### Positiv

- Entwickler können eine Aussage unmittelbar gegen einen kleinen aktuellen Codeausschnitt prüfen.
- Freie Pfade, stale Inhalte und sensible Dateien bleiben außerhalb der WebView-Grenze.
- Die bestehende Evidence- und Snapshotautorität bleibt die einzige Grundlage des Reads.

### Negativ

- Große Deklarationen und Dateien werden nur ausschnittsweise angezeigt.
- Historische stale Evidence kann keinen alten Quelltext rekonstruieren.
- Der neue Trust-Boundary-Read benötigt eigene Rust-, IPC-, Frontend- und Plattformverträge.

### Risiken und Gegenmaßnahmen

- Eine kompromittierte WebView erfindet Evidence-IDs — der Core validiert die vollständige
  Core-ausgegebene Auswahl gegen die aktuelle Publikation und antwortet content-frei.
- Die Datei ändert sich zwischen Evidence-Auflösung und Read — Handlemetadaten und vollständiger
  Hash werden nach dem sicheren Öffnen erneut geprüft.
- Source wird als Markup interpretiert — IPC liefert ausschließlich Textzeilen; Svelte rendert sie
  über Textbindung in einem `pre`/`code`-Element.
- Ein Ausschnitt enthält ein Secret — die bestehende nicht übersteuerbare Klassifikation blockiert
  die vollständige Datei vor Ausgabe.

## Verworfene Alternativen

- Ein generischer `read_file(path)`-Command — erweitert die WebView um Pfadautorität.
- Vollständige Dateien an den Browser übertragen — vergrößert Blast Radius, DOM und
  Secret-Exposition ohne Produktbedarf.
- Source dauerhaft neben Module Cards speichern — dupliziert untrusted Inhalt und erzeugt einen
  zusätzlichen Freshness- und Löschvertrag.
- Alte stale Revisionen aus Git rekonstruieren — würde eine neue historische Git-/Dateigrenze
  öffnen und gehört nicht zu U11.

## Compliance

- Negativtests prüfen erfundene und fremde Evidence, Replacement-Publish, Hash-Race,
  Symlink/Junction, Binary, Generated, Secret und Dateien über der bestehenden Secure-Read-Grenze.
- Grenztests prüfen 64 Zeilen, 16 KiB, UTF-8, CRLF, Highlightbereich und content-freie Fehler.
- Component- und Accessibility-Tests prüfen explizites Öffnen, Plain-Text-Rendering und die
  sichtbare Ablehnung historischer stale Evidence.

## Referenzen

- [ADR-0012](0012-safe-tools-and-approval-policy.md)
- [ADR-0024](0024-semantic-design-tokens-and-accessible-themes.md)
- [ADR-0025](0025-bounded-desktop-rendering-and-lifecycle.md)
- [Security und Execution](../SECURITY_AND_EXECUTION.md)
- [IPC-Protokoll](../IPC_PROTOCOL.md)
- [Desktop Product U11](../plans/06-DESKTOP_PRODUCT.md#u11-visuelle-code-map)
