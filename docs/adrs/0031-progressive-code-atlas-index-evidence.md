# ADR-0031: Progressiver Code Atlas und aktuelle Index-Evidence

Status: Accepted

Datum: 2026-08-27

Entscheider: Tim Bornemann

## Kontext

U11 zeigt eine begrenzte Modulübersicht und erlaubt Source-Vorschauen ausschließlich für Evidence,
die bereits zu einer sichtbaren verifizierten Module Card gehört. Für das Verständnis eines
Repositories müssen Nutzer jedoch deterministisch vom Modul zu Dateien, Typen und Symbolen
zoomen, direkte Beziehungen untersuchen und aktuelle Indexobjekte auch dann gegen Source prüfen
können, wenn Deep Map dafür noch keinen Claim erzeugt hat. Eine vollständige Graphkopie in der
WebView oder ein freier Datei- beziehungsweise Bereichsread würde ADR-0025 beziehungsweise
ADR-0030 widersprechen.

## Entscheidung

- Der Core stellt getrennte, streng versionierte und feste Read-Models für Atlas-Szene,
  Entity-Kontext, 50er-Inventarseiten und die geschlossenen Flow-Presets `callers`, `callees`,
  `tests` und `dataAccess` bereit. Requests enthalten ausschließlich Protokollversion,
  Core-ausgegebene typisierte Auswahlen und bei Inventaren einen publikations- und
  scopegebundenen opaken Cursor; niemals Pfade, Run-/Snapshot-IDs oder frei wählbare Limits.
- Der semantische Zoom besitzt die festen Ebenen Projekt, Modul, Datei und Symbol. Pro Antwort
  gelten 64 Module/128 Relationsgruppen, 32 Dateien, 48 Symbole, Zentrum plus 31 Nachbarn,
  16 Boundary-Stubs und 31 Flow-Ziele. Ein Adapter inspiziert höchstens 4.096 Graphkanten und
  bricht cancellable nach spätestens zwei Sekunden ab.
- `Contains` und `Defines` bestimmen räumliche Verschachtelung. `Imports`, `Exports`,
  `Implements`, `Extends`, `Builds` und `Configures` bilden die Standardarchitekturrouten.
  `Calls`, `Tests`, `Reads` und `Writes` werden nur in den passenden Flow-Szenen geöffnet;
  `Documents` bleibt Inspectorinformation.
- Ranking bleibt modellfrei und publikationsgebunden. Dateien priorisieren Manifest, Entry Point,
  öffentliches oder exportiertes Symbol, Test, gespeicherten Symbolrang und Pfadbytes. Symbole
  priorisieren Architekturtyp, Sichtbarkeit, Entry-/Testrolle, gespeicherten Rang und `SymbolId`.
  Beziehungen priorisieren Evidencezahl, Confidence, Relationsklasse und stabile IDs.
- Nicht aufgelöste oder nicht repository-lokale Ziele erscheinen nur als begrenzte Boundary-Stubs
  mit geschlossenem Grund, Provider, Confidence und aktueller Relationsevidenz. Sie werden nicht zu
  Facts oder künstlichen Modulen hochgestuft.
- ADR-0030 wird um aktuelle File-, Symbol- und Relationsevidence des statischen Index erweitert.
  Eine Vorschauauswahl muss zuvor durch einen U12-Read ausgegeben worden sein und wird gegen das
  aktive Projekt, die jüngste Publikation, Modulmembership, Revision, Hash und Evidence-ID erneut
  validiert. Die bestehenden Grenzen bleiben unverändert: acht Kontextzeilen je Seite, höchstens
  64 Zeilen und 16 KiB UTF-8 Plain Text sowie erneute Root-, No-Follow-, Dateityp-, Größen-,
  Binary-, Generated- und Secretprüfung.
- U12 fügt keine Persistenzmigration und keine Abhängigkeit hinzu. Karten- und Cursorzustand bleibt
  reine Präsentationslogik. Source-Inhalt wird weder gespeichert noch geloggt oder an ein Modell
  übertragen.

## Konsequenzen

### Positiv

- Nutzer können Architektur, Dateien, Typen und direkte Beziehungen in einer konsistenten Karte
  untersuchen und jede aktuelle strukturelle Auswahl gegen einen kleinen Sourceausschnitt prüfen.
- Alle Szenen bleiben konstant begrenzt, deterministisch wiederholbar und atomar an eine
  Publikation gebunden.
- Verifizierte Deep-Map-Claims ergänzen den Atlas, ersetzen aber keine statische Indexautorität.

### Negativ

- Große Ebenen und Inventare benötigen explizite weitere Reads und zeigen Trunkierung.
- Die vier Read-Models und die erweiterte Source-Auswahl vergrößern die Contract- und Testfläche.
- Externe und dynamische Ziele bleiben bewusst unaufgelöst; U12 erzeugt keine Laufzeitwahrheit.

### Risiken und Gegenmaßnahmen

- Eine kompromittierte WebView erfindet eine Auswahl — der Core revalidiert Selection-Art,
  Membership, Evidence-ID und aktuelle Publikation; widersprüchliche oder stale Auswahlen bleiben
  content-frei.
- Ein Flow wächst durch Zyklen — stabile Besuchsmengen, feste Hopgrenzen, 4.096 inspizierte Kanten
  und 31 Ziele terminieren die Traversierung.
- Eine Karte suggeriert Vollständigkeit — jede Antwort enthält vollständige Gesamtzahlen,
  Trunkierungsflags, inspizierte Kanten und unzugeordnete beziehungsweise ungelöste Counts.
- Claim-Badges suggerieren eine KI-Bestätigung — ein Badge erscheint nur bei exakter
  Evidence-ID-Übereinstimmung mit einem aktuellen verifizierten Claim; Card-Lifecycle und
  Confidence bleiben getrennt sichtbar.

## Verworfene Alternativen

- Vollständigen Datei-/Symbol-/Relationsgraph in die WebView laden — verletzt ADR-0025 und wächst
  mit dem Repository.
- Pfade oder Bereiche im Preview-Request akzeptieren — erweitert die WebView um Dateiautorität.
- Force-Simulation oder Code-Stadt — ist nicht deterministisch genug, erschwert Tastaturzugang und
  löst die Informationshierarchie nicht.
- Index-Evidence erst nach einem Deep-Map-Lauf nutzbar machen — koppelt deterministische
  Repositoryerkenntnisse unnötig an Modellarbeit.

## Compliance

- Rust-Verträge prüfen Ranking, Ebenengrenzen, identische Antworten, Cursorbindung,
  Replacement-Publish, Cancellation, Deadline, alle 13 Relationstypen und Flowzyklen.
- Source-Preview-Verträge prüfen fremde und erfundene Auswahl, stale Evidence, Hash-Race,
  Symlink/Junction, Binary, Generated, Secret und exakte Zeilen-/Bytegrenzen.
- TypeScript-Decoder lehnen unbekannte Felder, doppelte IDs, gemischte Publikationen,
  widersprüchliche Counts und übergroße Szenen ab.
- Component-, Accessibility- und Browserprofile prüfen semantischen Zoom, Inventarpaging,
  Flow-Modi, Tastaturbedienung, höchstens 1.500 DOM-Knoten und fehlenden Horizontaloverflow bei
  720 × 520 sowie 680 × 760.

## Referenzen

- [ADR-0025](0025-bounded-desktop-rendering-and-lifecycle.md)
- [ADR-0030](0030-bounded-evidence-source-preview.md)
- [Architekturregeln](../ARCHITECTURE_RULES.md)
- [Security und Execution](../SECURITY_AND_EXECUTION.md)
- [IPC-Protokoll](../IPC_PROTOCOL.md)
- [Desktop Product U12](../plans/06-DESKTOP_PRODUCT.md#u12-progressiver-code-atlas)
