# Qualitätsgates und Definition of Done

Status: verbindliche Baseline  
Stand: 2026-08-11

## Grundsatz

Qualität ist eine überprüfte Eigenschaft. „Sieht korrekt aus“, erfolgreiche Kompilierung oder eine LLM-Einschätzung reichen nicht als Abschlussnachweis.

## Gate pro Änderung

### Rust

- cargo fmt --check
- cargo clippy --workspace --all-targets --all-features mit -D warnings
- relevante Unit- und Integrationstests
- cargo test --workspace --all-features
- Dokumentation für öffentliche APIs und Invarianten

### Frontend

- Formatter
- Linter ohne Warnungen
- TypeScript Typecheck
- Unit- und Component-Tests der Änderung
- Accessibility-Prüfung für neue Interaktionen
- U2-Projects-Contracts prüfen strikt versionierte Commands für Status, Rebuild, den 25er-
  Projektkatalog, Aktivierung, Startwiederherstellung und Entfernen. Katalogreads übertragen nur
  sichere Anzeigen und opake Cursor; Aktivieren und Entfernen akzeptieren ausschließlich eine
  kanonische zuvor gelistete `worktreeId`, niemals Pfade. Storage-Tests müssen mehr als 25 Einträge,
  FTS-Suche, Vor-/Zurück-Paging, Bestandsmigration und Linked-Worktree-Trennung beweisen.
  Lifecycle-Tests verlangen: nur der jüngste Eintrag wird wiederhergestellt, kein Fallback bei
  fehlendem Root oder Identitätskonflikt und ein fehlgeschlagener Wechsel erhält aktives Projekt
  sowie Aktivierungsreihenfolge. Removal-Adaptertests erhalten private `knowledge.db`, stabile
  `ProjectId` und Repositoryinhalte. Component-Tests decken Startreihenfolge, Suche, Navigation,
  Hinzufügen, Wechsel und Bestätigung ab; Fehler-Components dürfen nur bekannte
  `CommandErrorV1`-Codes auf feste Recovery-Schritte abbilden und niemals rohe Adapterdetails
  darstellen.
- Der U3-Fast-Index-Fortschrittscontract prüft die exakte monotone Reihenfolge Discover, Hash,
  Parse, Link, Rank und Publish mit festem Total sechs. Der pfadlose
  `query_index_activity`-Contract darf nur das in-memory Manager-Read-Model liefern; TypeScript
  lehnt unbekannte Felder, fremde Phasen, widersprüchliche Ordinale und falsche Completion ab. Der
  Component-Test hält den letzten publizierten Snapshot während eines laufenden Jobs sichtbar.
- Der U3-Analysecontract verlangt eine lückenlose file-genaue V5-Publikation von Sprache,
  Adapterrevision, Diagnostics und Coverage. Migration V23 muss aus V22 atomar vorrollen und bei
  einem Schemafehler vollständig auf V22 zurückrollen. Der Storage-Roundtrip prüft partielle
  Coverage und sichere Diagnostics exakt; eine historische V4-Publikation ohne Analysezeilen muss
  weiterhin als explizit generisch lesbar bleiben.
- Der pfadlose `query_index_overview`-Contract rekonstruiert ausschließlich den letzten
  `PublishedIndex` und begrenzt die IPC-Antwort auf 64 Diagnostic-Dateien mit je acht Meldungen.
  Rust- und TypeScript-Contracts prüfen exakte Felder, verlustfreie Zähler, 0–10.000 Coverage,
  kontrollzeichenfreie Pfad-/Meldungsanzeigen, Trunkierungswahrheit und widerspruchsfreie
  Aggregatzahlen. Der Component-Test zeigt Dateien, Symbole, Diagnostics, Coverage und einen
  file-lokalen Fehler gleichzeitig mit dem weiterhin lesbaren publizierten Snapshot.
- Der U3-Deep-Map-Contract beweist, dass vor dem ausdrücklichen Start kein Executoraufruf erfolgt,
  nur ein live verifiziertes Structured-Output-Profil als verfügbar projiziert wird und die
  WebView weder Pfad, Profil noch Job-ID liefern kann. Start validiert Token-, Zeit- und
  Read-only-Toolbudget vor dem Scheduler. Ein laufender Versuch muss über `Pausing` zu einem
  validierten `Paused`-Checkpoint gelangen, Resume einen neuen besessenen Versuch ohne Wiederholung
  bestätigter Schritte starten und Cancel den Checkpoint verwerfen. Queued Work darf nicht fälschlich
  als checkpoint-sicher pausiert gelten; Projektwechsel und Shutdown dürfen keinen Worker ablösen.
  Rust-/TypeScript-Contracts lehnen unbekannte Felder, widersprüchliche Zustände, nicht kanonische
  Zähler und unbekannte Fehlercodes ab. Ein Executorfehler muss seine geschlossene content-freie
  Ursache bis zur UI behalten; der Component-Test zeigt dafür eine konkrete Recovery-Hilfe ohne
  Adapterdetails und nennt ausschließlich den tatsächlich konfigurierten Provider. Der gemeinsame
  Explorer-/Claim-Collector verwirft eine abgebrochene Teilantwort, wartet cancellation-fähig eine
  Sekunde und wiederholt exakt einmal ausschließlich `Unavailable`; Deadlineerschöpfung,
  Cancellation und alle nicht transienten Fehler öffnen keinen weiteren Versuch. Ollama-
  Regressionen prüfen, dass ein kurzer Request eines großen Profils nicht das gesamte
  konfigurierte Kontextfenster reserviert und Claim-IDs Core-eigen, wertgebunden und nach der
  Modellantwort erneut validiert sind. Der Component-Test muss außerdem zeigen, dass weder Mount
  noch Polling Modellarbeit startet und der Start exakt das zuvor sichtbare Budget übergibt.
- Der pfadlose U3-Module-Card-Freshness-Contract zählt ausschließlich die jeweils neueste Card pro
  Modul gegen den aktuellen veröffentlichten Run. Storage-Regressionsprüfungen verlangen direkte
  `Stale`-, ein-Hop-`NeedsReview`- und unabhängige `Published`-Zustände, sichtbare entfernte Module
  trotz leerer Remapqueue und das Verdrängen historischer stale Cards nach einer Neupublikation.
  Rust- und TypeScript-Contracts prüfen IDs, verlustfreie Summen, höchstens fünf positive kanonische
  Ursachen und legale Status/Ursachen-Paare. Der Component-Test zeigt `Stale`, `NeedsReview` und
  ihre Ursachen gemeinsam, ohne Card-Inhalt oder autoritative Pfade zu erhalten.
- Der U4-Repository-Tree-Contract liest Root und Unterverzeichnisse ausschließlich aus dem jüngsten
  atomar publizierten Run. Storage-Tests prüfen direkte Kinder, strikte Byteordnung, exakte
  Nachfahrenzähler und File-Hashes, nicht als UTF-8 darstellbare Namen, Vorwärtspaginierung,
  Cancellation, fehlende Verzeichnisse sowie den vollständigen Wechsel nach einem Replacement-
  Publish. Rust- und TypeScript-Grenztests lehnen unbekannte Felder, Traversal- und
  Nichtkanonik-Tokens, indirekte oder doppelte Kinder, widersprüchliche File-Evidence und falsche
  Cursor ab. Der reale Tauri-IPC-Test muss die Capability ohne Pfad- oder Projektparameter
  erreichen; der Component-Test navigiert ein Unterverzeichnis und hält dessen Run-/Snapshotbindung.
- Der U4-Module-Tree-Contract liest ausschließlich die V8-Modulprojektion des jüngsten atomar
  publizierten Runs. Storage-Tests prüfen den expliziten Zustand vor der ersten Publikation,
  primäre Root- und nächste direkte Kindgrenzen, Ausschluss transitiver Nachfahren und
  Graph-Communities aus dem Baum, exakte primäre/Community-, Manifest-, Datei-, Symbol- und
  Featurezahlen, aktuelle Revisions-Evidence, Vorwärtspaginierung, Cancellation, ungültige Eltern
  sowie den vollständigen Wechsel nach einem Replacement-Publish. Rust- und
  TypeScript-Grenztests lehnen unbekannte Felder, nicht kanonische IDs/Pfade/Zähler, Community-
  Knoten, widersprüchliche Manifest-, Representative- und Trunkierungsevidence, Duplikate,
  falsche Reihenfolge, Elternschleifen und Cursor ab. Der reale Tauri-IPC-Test erreicht nur die
  enge Capability ohne Projekt- oder Pfadparameter; der Component-Test navigiert zu einem direkten
  primären Kind, hält Run-, Snapshot-, Eltern- und Cursorbindung und stellt Graph-Communities nur
  als Zähler dar. Kein Modulbaum-Read läuft im Statuspolling.
- Der U4-Module-Dependency-Graph-Contract liest nur die aktuelle atomare V8-Modulprojektion und
  ein auf 4.096 Kanten begrenztes kanonisches zentrumsinzidentes Präfix. Storage-Tests prüfen
  eindeutige Symbol- und Datei-Endpoint-Zuordnung, ungemappte Dateien, Ausschluss von
  Graph-Communities und Hierarchierelationen, evidenzgewichtetes stabiles Nachbarranking,
  Knoten-/Gruppen-/Quelltrunkierung, Cancellation, ungültige Zentren sowie vollständigen Wechsel
  nach Replacement-Publish. Rust- und TypeScript-Grenztests lehnen unbekannte Felder,
  nicht kanonische IDs/Pfade/Counts, mehrdeutige Memberships, Selbst- und Nichtzentrumskanten,
  falsche Reihenfolge, Bounds, Trunkierungswahrheit und widersprüchliche repräsentative
  `GraphEdge`-Evidence ab. Der reale Tauri-IPC-Test erreicht nur die enge Capability ohne Projekt-,
  Pfad- oder Endpointparameter. Der Component-Test lädt erst nach expliziter Modulauswahl, zeigt
  alle Begrenzungs- und Unmapped-Signale und navigiert eine stabile aktuelle Evidence-ID; weder
  Mount noch 500-ms-Statuspolling lösen den Graphread aus.
- Der U4-Module-Runtime-Contract verwendet ausschließlich aktuelle V8-Entrypoint-/Testrollen und
  die bestehende R3-Graphtraversierung. Ein realer libSQL-Publikationsvertrag prüft atomare
  Rangpräfixe, getrennte Formationstrunkierung, Rollen- und Membershipbindung, feste `Calls`- und
  `Tests`-Presets, Cancellation, beschädigte Rollen sowie `publicationChanged` nach einem
  Replacement-Publish. Rust- und TypeScript-Grenztests lehnen unbekannte Felder, nicht kanonische
  Run-/Snapshot-/Modul-/Symbolanker, Grenzen außerhalb 1–256 beziehungsweise 1–100, falsche Rollen,
  Ränge und Trunkierungswahrheit sowie Graphpfade mit falscher Relation, Tiefe, Richtung,
  Kontinuität, Zyklen, Duplikaten oder Zielwiderspruch ab. Der reale Tauri-IPC-Test erreicht nur die
  zwei engen Capabilities ohne Projekt-, Pfad-, Richtungs- oder frei wählbare Relationsparameter.
  Der Component-Test lädt Roots erst nach expliziter Modulauswahl und einen Flow erst nach
  Rootauswahl, bindet ihn an den sichtbaren Run/Snapshot und öffnet genaue Symbol-, Ziel- und
  Kanten-Evidence. Nach `publicationChanged` werden alte Roots und Evidence bis zum erneuten
  atomaren Read ausgeblendet. Kein Runtime-Read läuft im 500-ms-Statuspolling.
- Der U4-Module-Card-Detail-Contract liest die deterministisch jüngste dauerhafte Card eines
  explizit ausgewählten aktuellen Primärmoduls in derselben kurzen Transaktion wie die jüngste
  atomare Indexpublikation. Storage-Tests prüfen getrennte aktuelle und historische
  Publikationsanker, die Current→Stale-Invalidierung nach geänderter Evidence, ein-Hop-
  `NeedsReview`, Cancellation sowie die Ablehnung widersprüchlicher Feld-, Claim-, Evidence- und
  Lifecycle-Zeilen. Rust- und TypeScript-Grenztests lehnen unbekannte Felder, nicht kanonische IDs,
  Schema-/Mapperabweichungen, übergroße oder falsch geordnete V1-Felder, Evidence außerhalb des
  Feldes, doppelte Claims und eine Lifecycle-Propagation ab, durch die ein stale oder zu prüfender
  Claim als current erscheinen könnte. Die schema-gebundene Coverage muss Gesamt-, acht Muss- und
  vier Soll-Felder exakt aus den ausgelieferten verifizierten Feldern ableiten; Rust- und
  TypeScript-Contracts prüfen Zähler, ganzzahlige Basispunkte, kanonische Lücken und ihre
  Unabhängigkeit von Confidence und Lifecycle. Der reale Tauri-IPC-Test erreicht ausschließlich
  die enge Capability mit einer Modul-ID und ohne Projekt-, Pfad-, Card-, Claim- oder
  Evidence-Parameter. Der Component-Test lädt erst nach expliziter Modulauswahl, entfernt alte
  Card-Daten während eines Reloads und zeigt Claim-Typ, Confidence, progressive Muss-/Soll-Coverage
  sowie `Current`, `Stale` oder `NeedsReview` unabhängig. Kein Card-Detail-Read läuft im
  500-ms-Statuspolling.
- Der U4-Evidence-Inspector-Contract löst ausschließlich eine Evidence-ID auf, die zur exakt
  verankerten deterministisch neuesten sichtbaren Module Card gehört. Reale libSQL-Fixtures prüfen
  aktuelle File-, Symbol- und Graph-Payloads, stale historische Graph-Provenienz,
  `NeedsReview` mit weiterhin aktueller Evidence, Cancellation, erfundene IDs und
  `selectionChanged` nach Replacement-Publish. Rust- und TypeScript-Grenztests lehnen unbekannte
  Felder, nicht kanonische oder widersprüchliche Run-/Snapshot-/Card-/Modul-/Evidence-Anker,
  abweichende Graph-Evidence-IDs sowie stale Evidence auf einer Current- oder NeedsReview-Card ab.
  Der reale Tauri-IPC-Test erreicht nur die enge Capability ohne Projekt-, Source-, Datei-, SQL-
  oder generische Graphparameter. Der Component-Test startet den Read erst nach Evidence-Klick,
  zeigt Card-Lifecycle und Evidence-Freshness unabhängig und entfernt alte Evidence bei
  Card-Reload oder Auswahlwechsel sofort. Kein Inspector-Read läuft im 500-ms-Statuspolling.
- Der U4-Search-/Task-Lens-Contract führt einen bewusst abgeschickten Suchtext über die echten
  Exact- und Lexical-Adapter derselben Publikation sowie die R4-Fusion. Ein reales No-Embeddings-
  Fixture verlangt aktuelle Evidence, deterministische Wiederholung, mindestens einen Exact-
  Treffer und keinerlei vorgetäuschte Semantic-Evidence. Rust-, IPC- und TypeScript-Tests prüfen
  geschlossene Formen, Kanal-/Zielordnung, Deduplizierung, Run-/Snapshotbindung, Score- und
  Trunkierungswahrheit sowie pfadlose Commands mit Versionsprüfung vor Nutzdatenvalidierung.
  Für die Task Lens lesen Adapter-Contracts eine begrenzte aktuelle Goal-Liste sowie ausgewählten
  Goal Contract und Task Ledger atomar, worktree-isoliert und nach Reopen identisch; Cancellation,
  fehlendes Ledger und Revisionsabweichung sind Pflichtfälle. Der Application-Contract lädt Goal
  und Ledger vor jeder R10-Kompilierung erneut, akzeptiert nur aktive Schritte und leitet beide
  4-KiB-Seeds selbst ab. TypeScript revalidiert höchstens 64 L0–L3-Einträge, Tokenrechnung,
  Retrievalquellen, höchstens 128 aktuelle Claims und deren exakte Evidence. Der Component-Test
  beweist, dass Task-/Lens-Reads erst nach Umschaltung beziehungsweise Auswahl starten, Semantic
  sichtbar „kein Beweis“ bleibt und eine evidencefreie Architekturabsicht als unbewiesene,
  visuell getrennte Hypothese erscheint. Weder Suche noch Task Lens laufen im 500-ms-Statuspolling.
- Der U11-Atlas-Contract prüft 64/128 Übersichtslimits, 32er Fokus, manifest-first Ranking,
  deterministisch identische Antworten, vollständige Trunkierungs-/Unmapped-Zähler, Cancellation,
  Replacement-Publish und Run-/Snapshotbindung. Search V2 prüft optionale eindeutige
  Modulmembership sowie exakt passende File-/Symbol-Selections; TypeScript lehnt unbekannte
  Felder, ungültige Enums/IDs, gemischte Publikationen und übergroße Szenen ab.
- Der ADR-0030-Preview-Contract prüft aktuelle File-, Symbol- und Graph-Evidence, `NeedsReview`,
  stale Auswahl, Hash-Race, Symlink/Junction, Binary, Secret, Generated, Großdatei, erfundene IDs
  sowie exakt 64 Zeilen und 16 KiB. Inhalt erscheint nur nach explizitem Evidence-Klick als Plain
  Text und wird nicht persistiert oder geloggt.
- Der U12-Atlas-Contract prüft Projekt-, Modul-, Datei- und Symbolebene gegen 64/128, 32, 48 und
  Zentrum-plus-31, deterministisches Ranking, alle 13 Relationsarten, Containment, 16
  Boundary-Stubs, vollständige Counts/Trunkierungen, Cancellation, Zwei-Sekunden-Deadline und
  Replacement-Publish. Inventory-Contracts prüfen ausschließlich `files`, `symbols` und `members`,
  feste 50er-Seiten sowie publikations- und scopegebundene Cursor. Flow-Contracts prüfen Richtung,
  Zwei-Hop-/Ein-Hop-Grenzen, Zyklen, 31 Ziele, 4.096 inspizierte Kanten und den vollständigen ersten
  kürzesten Evidence-Pfad.
- Der ADR-0031-Preview-Contract ergänzt erfundene, fremde und stale Indexauswahlen, aktuelle File-,
  Symbol-, Relations- und ungelöste Relationsevidence sowie die erneute exakte Ableitung ihrer
  Evidence-ID. Alle ADR-0030-Fälle und Zeilen-/Bytegrenzen bleiben unverändert Pflicht.
- U12-Decoder lehnen unbekannte Felder, unkanonische IDs/Cursor, doppelte Knoten, ungültige
  Breadcrumbs, widersprüchliche Counts und Trunkierung, gemischte Publikationen sowie übergroße
  Szenen, Inventare und Flows ab. Component- und Browser-Smokes prüfen Auswahl gegen Öffnen,
  Breadcrumb/Escape, jede Ebene, 50er-Inventare, die vier Flow-Presets, Task Lens, Search-Fokus,
  Claim-/Boundary-Kennzeichnung, Pointer-Pan, zeigerzentrierten Mausrad-Zoom, eine viewport-feste
  nichtgrafische Zusammenfassung und Publish-Refresh bei 720 × 520 und 680 × 760. Mehrere direkte
  Relationen zum selben Nachbarknoten dürfen im Inspector keinen doppelten Svelte-Key erzeugen.
  Namespace-Selbstrelationen dürfen weder einen Self-Parent in der Dateiszene erzeugen noch das
  Symbolzentrum duplizieren. Überschreiten externe oder ungelöste Ziele die 16er-Grenze, müssen
  Boundary- und Relationskürzung konsistent zur tatsächlich gerenderten Szene gemeldet werden.
  Bei einem dauerhaft abgewiesenen Detail-Read muss die Projektübersicht ohne neuen Indexlauf
  wieder erreichbar sein.
- Das U12-Browserprofil verwendet 64 Module, 32 Dateien, 48 Symbole, 128 Relationsgruppen,
  31 Flow-Ziele und 32 Feed-Ereignisse. Es rendert davon 24 priorisierte Übersichtsrouten sowie
  32 inzidente Auswahlrouten und meldet Mount, Auswahl, semantischen Zoom, Pan, Feed-Commit,
  Long Tasks und DOM-Zahl; höchstens 1.500 DOM-Knoten und keine sichtbare Blockade über 100 ms sind
  harte Gates.
- Deep Map V2 prüft die Phasenfolge Planning → Exploring → Claiming → Verifying → Publishing,
  aktuelle Module/Aktionen, 32er-Retention, monotone Sequenzen, Pause/Resume ohne Replay,
  Cancel/Fehler und eine Publikationszusammenfassung ausschließlich nach erfolgreichem atomarem
  Publish. Mount und Polling starten weiterhin keine Modellarbeit.
- U11-Component-, Accessibility- und Browser-Smokes prüfen map-first Start ohne Legacy-Tabs,
  Kartenfokus, Submit-Suche, Task Lens, progressive Inspector-/Preview-Reads, harte Presets,
  Live-Feed und Publish-Refresh bei 720 × 520 und 680 × 760, Light/Dark, 3-Pixel-Fokus,
  44-Pixel-Controls, Reduced Motion, Tastaturbedienung und fehlenden Horizontaloverflow. Das
  reproduzierbare 64-Knoten-/128-Routen-Profil berichtet Mount, Auswahl, Pan/Zoom, Feed-Bursts,
  DOM-Obergrenze, Long Tasks sowie initialen und lazy Chunk; Performancegewinne werden nur aus
  Messdaten behauptet.
- Der Agent-Session-Contract prüft, dass eine angenommene Ask-, Plan- oder Agent-Nachricht sofort
  als ausgewählte dauerhafte Session sichtbar ist und genau einen besessenen Job erhält. Die feste
  Scheduler-Fortschrittsskala darf durch Task Lens, Kontextkompilierung, Indexmaterialisierung,
  Patch oder Prozessausführung weder ihr Total wechseln noch regressieren. Worker-, Queue- und
  Persistenzfehler müssen eine laufende Session dauerhaft mit einer sicheren nutzerlesbaren
  Abschlussnachricht beenden. Cancel muss auch ohne aktiven Job sowie bei einem konkurrierenden
  Abschluss idempotent werden. Session-Reads und -Writes verwenden voneinander unabhängige
  libSQL-Operationsverbindungen; ein fremder offener Transaktionskontext darf sie nicht blockieren.
  Component-Tests müssen nach einem transienten Readfehler weiter nicht überlappend pollen, bis die
  terminale Antwort sichtbar ist. Mount und Polling starten weiterhin keine Evidenz- oder
  Modellarbeit. Die Folge aus abgeschlossenem `/diagram`-Turn und anschließendem laufendem
  Ask-Turn muss das bestehende Diagramm über mehrere Polls als dieselbe DOM-Instanz behalten;
  Artefakt-Read und Mermaid-Rendering dürfen dabei nicht erneut starten.
- Der gemeinsame Recherche-Contract bindet jeden Ask-, Plan- und Agent-Vorbereitungsturn vor dem
  Modellaufruf an genau einen veröffentlichten Index. Controllerverträge prüfen den Ein-Turn-Pfad,
  mehrstufige Symbol-/Aufrufer-/Source-Folgen, die exakten Standard-/Gründlich-Grenzen, höchstens
  vier sequenzielle Aktionen pro Entscheidung, genau einen Reparaturversuch, Deduplizierung, zwei
  stagnierende Runden, Timeout, Cancellation und Fortsetzung mit neuem Indexanker. Vertrags- und
  Source-Search-Tests prüfen case-insensitive TODO-/FIXME-Treffer außerhalb eines Dateipräfixes,
  Treffer- und Ergebnisgrenzen sowie ehrliche `limited`-Negativaussagen; die Implementierung
  erzwingt zusätzlich 2.000 Dateien, 32 MiB, 30 Sekunden und Cancellation. V31-Migrations- und
  Storagetests prüfen Neuinstallation, V30→V31 ohne Backfill, Legacy-Lesbarkeit, öffentliche
  Notizquellen, atomaren Ergebnis-/Zitatabschluss, Rollback und Presentation Delete. Rust-IPC,
  TypeScript und Capabilitytests erlauben nur Session,
  Usersequenz und opake Cursor/Source-Referenzen; Pfade, Evidence-, Snapshot-, Index-, Worktree-
  und Provider-IDs sowie Prompts, Rohantworten, Source-Inhalt und Chain-of-Thought bleiben aus der
  normalen Projektion ausgeschlossen. Component-Tests prüfen Live-Aktion, Task-Lens-Gründe,
  vollständige/begrenzte Suche, verwendete/zusätzliche Quellen, stale Preview und `notRecorded`.
  Kontrollierte Timer prüfen zusätzlich die append-only Staffelung über höchstens 900 Millisekunden,
  genau einen aktiven Timeline-Schritt, terminale Fehler-/Abbruchzustände, Turnwechsel ohne alte
  Timer, das einmalige Einklappen nach 700 Millisekunden und sofortige Darstellung bei Reduced
  Motion. Der Workspace-Test muss den Übergang von der Live-Karte zur frisch abgeschlossenen,
  zunächst geöffneten Rechercheprojektion belegen.
- Der ADR-0043-Contract verlangt Ask-Research-Decision V3 mit geschlossenem Evidence-Status und
  paginiertem `inspectPath.start_line`. Eine als `incomplete` markierte Antwort sowie eine nicht
  gelesene, eindeutig genannte Indexdatei müssen weitere Recherche oder
  `AwaitingContinuation` auslösen. Regressionstests führen mindestens zwei Task-Lens-Runden in
  demselben Conversationjob aus und verbieten regressierenden Schedulerfortschritt. Read-Retries
  sind auf einmal pro Operation und vier pro Abschnitt begrenzt; ein Modell-Retry verbraucht eine
  reguläre Entscheidung. Cancellation und nicht transiente Source-Ablehnungen dürfen niemals
  wiederholt oder in einen Erfolg umgedeutet werden.
- Der ADR-0044-Regressionsvertrag belegt, dass explizit genannte aktuelle Dateien vor Task Lens
  und historischem Conversation-Kontext gelesen werden. Die Task-Lens-Baseline umfasst höchstens
  zwölf, die historische Revalidierung höchstens acht Quellen; adaptive Reads behalten
  Kontextkapazität. Tests mit kleinem Evidence-Fenster müssen zeigen, dass eine neue priorisierte
  Quelle alten Baseline-Kontext verdrängt und als Core-aufgelöstes `S`-Ziel im nächsten
  Modellturn erscheint. Eine dateihaltige `searchIndex`-Aktion muss das eindeutige Ziel direkt
  lesen; eine Nullrunde muss den Zugang wechseln, ohne Stagnations-, Aktions- oder Zeitgrenzen zu
  lockern.
- Recherche-Fortsetzungsregressionen bilden wiederholte Router-/Server-Reads nach: pro
  Entscheidung genau ein aktuelles Evidence-Paket, keine doppelten Ausschnitte oder künstlichen
  Fortschrittszähler, kein Rückfall vom fokussierten späteren Bereich zum Dateianfang, konkrete
  Kontextkürzung und Auswertung vor der expliziten Stagnationsfortsetzung. Sie prüfen außerdem
  die ursprüngliche Frage bei verschachtelten Legacy-Fortsetzungen, Priorität zuletzt belegter
  Quellen, vollständige Bereichsrevalidierung, neue Fragen ohne alte offene Ziele sowie erhaltene
  Teilantworten und Zitate. Der Component-Test verlangt einen kompakten Fortsetzungseintrag.
- Conversation-Regressions müssen belegen, dass ein zweites ungültiges strukturiertes Ergebnis
  nach genau einem Repair als `AwaitingContinuation` mit erhaltenen Quellen endet. `/diagram`
  muss eine zweite feste Formatierungsentscheidung reservieren und bei weiterem Fehler Antwort
  und Quellen erhalten. Die Chatoberfläche darf während progressiver Timeline-Höhenänderungen
  keinen `ResizeObserver`-getriebenen Scrollwrite ausführen; Pointer-, Touch- oder Wheelinteraktion
  muss eine noch ausstehende einmalige Positionierung abbrechen.
- Der ADR-0039-Contract prüft den vollständigen Slash-Katalog, Modusmatrix, leere Themen, `//`-
  Escape, höchstens zwei unterschiedliche Linsen und die Ablehnung unbekannter oder mehrfacher
  Hauptaufträge vor Sessionappend und Jobstart. V32-Tests decken Neuinstallation, V31→V32,
  atomare Command-/Linsenpersistenz, atomaren Antwort-/Zitat-/Diagrammabschluss, Reopen und
  Presentation Delete ab. Die fünf neuen Analyseaktionen müssen ihre festen Ergebnisgrenzen,
  Indexbindung, Cancellation, Deduplizierung und ehrliche Trunkierung belegen; keine davon darf
  Ask oder Plan eine freie Prozess-, Pfad- oder Netzwerkcapability geben. Diagrammverträge prüfen
  Source-Referenzen, Topologie- und Größenlimits, Mermaid-Kompilierung, Strict/Lazy Rendering,
  zusätzliche SVG-Sanitisierung, begrenztes PNG, path-freien nativen Export und Ablehnung aktiver
  Inhalte. Component-Tests prüfen gefilterte Tastaturpalette, entfernbare Chips, feste Command-
  Tiefe, getrennte Darstellung des Auftrags, Render-Retry, Theme und Export. Die Mermaid-Lazy-
  Erweiterung muss initiales Bundle-, DOM-, Reduced-Motion- und Schmalfensterbudget weiterhin
  einhalten.
- Die Diagramm-Regression muss eine reale Flowchart-Kante mit einer Methodensignatur wie
  `p.on_task_created(task_data)` durch Core-Kompilierung und Mermaid 11 rendern. Bereits
  persistierte unquotierte V32-Kanten werden ausschließlich durch die enge lokale
  Kompatibilitätsnormalisierung akzeptiert. Ein danach verbleibender Parsefehler bietet eine neue
  typisierte, evidence-gebundene Erzeugung an und darf weder rohen Mermaid-Code ausführen noch eine
  allgemeine Modell- oder Dateicapability öffnen.
- Der ADR-0040-Contract reproduziert einen Turn mit zwölf gefundenen und drei verwendeten Quellen,
  erzwingt einen kohärenten Projektionsread und verbietet die falsche Leermeldung bei temporär
  fehlenden Sourcedetails. Fake-Timer-Tests prüfen gruppierte Runden, logische Vorbereitung,
  Live→Antwort ohne Remount, genau einen Auto-Collapse und Reduced Motion. Marker-Tests lassen
  Code inert und verlangen außerhalb von Code die exakte Gleichheit zu `source_refs`.
- Der Conversation-Scroll-Contract simuliert wachsende Live-Inhalte: Am Ende folgt der Viewport
  deterministisch, eine Aufwärtsgeste hält die Position trotz weiterer Resize-Ereignisse stabil,
  und erst die Rückkehr ans Ende aktiviert das Folgen wieder. Zusätzliche Quellen sind standardmäßig
  kompakt eingeklappt; verwendete Quellen und Inline-Zitate bleiben direkt erreichbar. Ein bereits
  sichtbarer Rechercheweg darf bei einem verkürzten, fehlenden oder älteren Poll-Ergebnis desselben
  Turns weder aus dem DOM verschwinden noch seine sichtbaren Schritte zurücksetzen; erst eine
  monotone append-only Projektion ersetzt den letzten vollständigen Stand. Mehrseitige Quellen
  werden vor dem sichtbaren Austausch vollständig geladen, ohne die natürliche Höhe der Timeline
  festzuschreiben. Ein Layout-Resize allein darf eine manuell gelöste Scrollbindung nicht
  reaktivieren.
- Der ADR-0041-Contract prüft vorwärts und rückwärts wählbare nächste Modi, die Planpflicht nach
  unterbrochener Agent-Kontinuität sowie eine dauerhaft wiederherstellbare FIFO mit Session- und
  Worktreegrenzen. Migrationstests decken Neuinstallation und V32→V33 samt Rollback ab;
  Storageverträge prüfen Revisionen, Reihenfolge, Claim-Retry, Entfernen und Pause/Fortsetzen.
  Desktoptests müssen belegen, dass terminale Agent-Run-Zustände einen neuen Planstart nicht
  blockieren, ein veralteter Plan verständlich zu einem Nutzerhaltepunkt führt und Fehler oder
  Abbruch eine vorhandene Queue pausieren. Component-Tests prüfen Stufenleiste, Queue-Leiste,
  Core-selektierbare Modi, die ausschließlich taskgebundene Agentenlauf-Seitenleiste und einen
  überlagerungsfreien, per Escape schließbaren Header-Aktionsbereich.
- Der erste U5-Agent-Workspace-Contract prüft die vollständige Goal-Neuanlage mit ausschließlich
  Core-generierten Task- und Kriterien-IDs sowie immutable Revisionen gegen einen sichtbar
  gebundenen Vorgänger. Application-Tests lehnen WebView-IDs bei Revision eins, erfundene
  Kriterien-IDs und stale Editoren ab; der gemeinsame reale libSQL-Vertrag erhält Must und Should,
  alte Revisionen, Worktree-Isolation und den aktuellen Contract über Reopen. IPC- und
  TypeScript-Tests erzwingen exakte V1-Felder, UTF-8-Bytegrenzen, Kardinalität, Eindeutigkeit,
  Revisionsverkettung und Versionsprüfung vor Content. Debug- und Recovery-Tests dürfen keinen
  nutzerverfassten Goal- oder Adaptertext ausgeben. Component-Tests beweisen, dass ohne aktives
  Projekt kein Read startet, Neuanlage nur `null`-Kriterien-IDs sendet, Revisionen stabile IDs
  behalten und der neu geladene aktuelle Goal Contract samt Must-/Should-Kennzeichnung sichtbar
  bleibt. Der Ledger-Component-Contract bindet den Read an dieselbe sichtbare `TaskId`, zeigt
  Revision und Store-Version, markiert nur einen tatsächlich laufenden/wartenden/verifizierenden/
  blockierten Schritt als aktuell und hält ihn gemeinsam mit dem Goal im Sticky Anchor. Fehlendes
  Ledger und Goal-Revisionsabweichung bleiben explizit; Speichern startet weder Ledger noch Run
  noch Modellarbeit.
- Der U5-Agent-Activity-Contract leitet den aktiven oder letzten Run ausschließlich aus retained
  Task-Ledger-Versuchen ab und revalidiert Goal, Ledger sowie materialisierten Run nach dem
  begrenzten Read. Application-Tests prüfen die Run-Auswahl und das exakt zusammenhängende Fenster
  der letzten 64 Ereignisse. IPC und TypeScript akzeptieren nur Protokollversion plus `TaskId`,
  geschlossene Zustände, positive Decimal-/Budgetgrenzen, monotone Zeit und Sequenzen sowie
  höchstens 256 aktuelle Blocker; unbekannte verschachtelte Felder und erfundene Run-/Pfadparameter
  werden abgelehnt. Component-Tests zeigen Goal und aktuellen Step weiter gemeinsam, alle sechs
  Run-Budgetdimensionen, Context-/Snapshotanker, Freigabeblocker, problematische inhaltsfreie
  Eventcodes und terminale Zustände. Eine `ModelInteraction` mit Aktionsauswahl bleibt sichtbar
  „noch keine Ausführung“; nur `ToolAction` wird als echte Ausführungsaktion bezeichnet. Rohes
  Modell-, Tool-, Fehler- oder Sourcematerial überschreitet IPC nicht.
- Der U5-Agent-Recovery-Contract leitet den steuerbaren Run ausschließlich aus `TaskId` und aktivem
  retained Ledger-Versuch ab. Query und Mutation dürfen keine Run-, Snapshot-, Step-, Event- oder
  Worktree-ID aus der WebView akzeptieren; die Mutation bindet nur eine geschlossene
  Pause-/Resume-/Replan-/Cancel-Aktion an die exakt sichtbare Ledgerrevision und -Storeversion. H11/E8-
  Storage-Contracts bleiben der atomare Nachweis für Published-Snapshot-, Ledger- und Run-CAS,
  stale Evidence, Unknown-Reconciliation und Reopen. Rust-/TypeScript-Contracts lehnen unbekannte
  Felder, nicht kanonische Anker und widersprüchliche Outcome-/Controllerzustände ab. Der
  Component-Test muss Resume bei stale Evidence deaktivieren, Replan erklären, Cancel erreichbar
  halten und nach Erfolg den dauerhaften terminalen Zustand neu laden. Der Capability-Test muss
  jeden tatsächlich registrierten Agent-/Task-Lens-Command explizit allowlisten und weiterhin
  generische Dialog-, Datei-, Shell-, SQL-, Provider- und Netzwerkrechte ausschließen.
  Manager-Contracts müssen zusätzlich beweisen, dass Mount und Polling keine Arbeit starten,
  Queued Work keinen sicheren Pause-Checkpoint behauptet, ein laufender Versuch erst nach
  terminaler Scheduler-Cancellation plus Executor-Bestätigung und H11-Revalidierung `Paused`
  erreicht, ein Resume mit strikt neuerer Ledger-Store-Version einen neuen besessenen Job startet
  und Cancel exakt die sichtbaren Anker verwendet. H11 darf einen im aktuellen Prozess lebenden
  Worker nicht inspizieren. Projektwechsel und Shutdown dürfen keinen Worker ablösen.
- Der U6-Diff-/Verification-Inspector-Contract lädt mit `query_agent_inspection` ausschließlich die
  ausgewählte `TaskId`; Projekt, Worktree, Run, Step, Verification-Spec, Snapshot, Pfad, Process und
  Evidence bleiben Core-eigen. Application-Tests prüfen eine exakte bounded E3-Patchprojektion,
  gemeinsame Hunkzeilen für Unified und Side-by-side, getrennte Vorschlags-/Angewandt-/Extern-/
  Unattributed-Provenienz, content-freie Test-/Build-/Diagnostic-Zeilen, Redaction sowie das
  Entfernen flüchtiger Inhalte bei neuem Run, Projektwechsel und Shutdown. Durable Verification
  wird konsistent doppelt gelesen und gegen den jüngsten Published Snapshot neu ausgewertet; stale
  Evidence bleibt sichtbar, kann aber keinen Must-/Should-Beweis liefern. IPC- und TypeScript-
  Contracts lehnen unbekannte Felder, nicht kanonische IDs, Pfade und Zahlen, inkonsistente
  Dateioperationen, Hunkkoordinaten, Zähler, Trunkierung, Termination, Step-/Attempt-Historie,
  höchstens 128 kanonisch geordnete Diff-Evidence-Pfade sowie
  Proofs ohne abgeschlossenen Step und frische bestandene Evidence ab. Logtext wird erst nach
  expliziter Auswahl über die zuvor emittierte positive Inspection-Revision, Inspection-ID,
  `stdout|stderr` und einen exakten Bytecursor geladen; vier bis 16.384 Byte garantieren bounded
  UTF-8-Fortschritt. `pageTruncated` bietet gezieltes Nachladen, `sourceTruncated` markiert dauerhaft
  verworfene Ausgabe, und redigierte Seiten bleiben leer. Component-Tests müssen exakte Pfade und
  Hunkparität beider Layouts, verlässliche Attribution, bedarfsgesteuertes Cursor-Paging, getrennte
  Trunkierung, sichtbare stale Steps/Evidence und die exakten Step-/Evidence-IDs jedes bewiesenen
  Muss-Kriteriums im Done-Zustand zeigen. Der Capability-Test erlaubt nur die beiden engen
  Read-Commands und keine Datei-, Shell-, SQL-, Provider-, Approval- oder Mutationsbefugnis.
- Der U7-Approval-Center-Contract verbindet die flüchtige exakte E3-/E4-Präsentation nur mit einem
  bereits dauerhaften Request und revalidiert Task, Goal, Ledger, Run, Step, Snapshot, Request und
  optionalen Grant. Der reale Harness muss belegen, dass AwaitApproval den Worktree unverändert
  lässt, AllowOnce einen exakten Grant speichert ohne auszuführen, Continue nur die interne
  Grant-ID weitergibt, Verbrauch einmalig bleibt und Deny Step `Blocked` plus Run `Failed` atomar
  ohne Toolwirkung committed. Der gemeinsame Storagevertrag lädt den Grant auch nach Reopen exakt
  über seine Request-ID.
- Rust-IPC und TypeScript akzeptieren für Query nur `TaskId`, für Control zusätzlich ausschließlich
  die sichtbare Presentation-Revision, Ledgerrevision/-Storeversion und eine geschlossene Aktion.
  Unbekannte Authority-Felder, nicht kanonische Anker, widersprüchliche Lifecycle-/Control- oder
  Outcome-/Runtime-Paare, inkonsistente Patchpfade, übergroße argv sowie ungültige Env-Namen werden
  abgelehnt. Request-, Grant-, Run-, Snapshot-, Process-, Policy- und Event-IDs oder Env-Werte
  dürfen nicht aus der WebView stammen beziehungsweise in sie gelangen.
- Component-Tests müssen Aktion, Risiko, Scope, Grund, Ablauf, verlustfreie Pfade oder die exakten
  argv-Tokens samt CWD, Env-Namen, Limits, Modus, Plan-/Netzwerkbindung und Specification-ID zeigen.
  AllowOnce und Deny bleiben neutral und nicht vorausgewählt; Bestätigung ist bis zur expliziten
  Wahl deaktiviert. GrantStored darf nicht selbst Continue auslösen, und Active muss Continue und
  Revoke getrennt anbieten. Der Capability-Test erlaubt nur die beiden engen Approval-Commands und
  weiterhin keine generische privilegierte Capability.
- Der U8-Settings-Contract prüft den expliziten modellfreien Nullzustand, append-only
  V1-Snapshots, monotone Revisionen, CAS-Konflikte, Reopen und die atomare Invalidierung aller
  Profile und Probe-Evidence nach einem Endpointwechsel. Ein Read-, Mount- oder Starttest muss
  belegen, dass dabei weder Provider, Netzwerk noch GPU-Arbeit beginnt.
- Provider-Contracts prüfen offline die Kanonisierung credential-freier lokaler Origins, die
  sichtbare und nicht ausführbare Remote-Klassifikation, feste Timeouts, Cancellation, begrenzte
  Antworten und Core-eigene Zeit. Coding und Mapping dürfen nur nach realem Structured-Output-
  Erfolg verifiziert sein; Embedding zusätzlich nur nach einem endlichen, nicht leeren Vektor mit
  beobachteter gültiger Dimension. Fehler, Abbruch, stale Revision und Remote bleiben nicht
  ausführbar.
- Credential-Contracts prüfen Set/Load/Delete, CAS, Generation-Mismatch, Keyring-Ausfall,
  unterbrochene Store-/Delete-Phasen, wiederholbare Recovery, Profilinvalidierung und redigierte
  Fehler. Ein Rohscan des v5-Katalogs und seiner Begleitdateien darf den Test-Key nirgends finden.
  Ein ignorierter nativer Smoke verwendet pro Lauf einen isolierten Account und löscht ihn über
  einen Drop-Guard; CI führt ihn auf Windows, macOS und unter einer isolierten Linux-Secret-Service-
  Session aus.
- Der ADR-0026-Discovery-Contract prüft `GET /api/tags` vollständig offline: keine WebView-URL,
  erneute Local-only-Policy, ein Gesamttimeout, wakebare Cancellation, begrenztes JSON sowie
  validierte, deduplizierte und kanonisch sortierte Modell-IDs. Die flüchtige Auswahl darf kein
  Profil oder Capability-Evidence erzeugen. TypeScript lehnt stale Revisionen, unbekannte
  Providerarten, unsortierte beziehungsweise doppelte IDs und zusätzliche Felder ab.
- Der U8-Projektsettings-Contract liest Ignorequellen ausschließlich über die validierte
  Repositorykonfiguration und rekonstruiert den Command-Katalog aus dem jüngsten Published Index.
  Query und Confirmation prüfen Katalog-ID, Revision, Store-CAS, bekannte Command-IDs und
  Projektlebenszyklus; stale Evidence verändert keine Allowlist und führt keinen Command aus.
- Rust-IPC, TypeScript-Decoder und Capabilitytests erlauben nur die engen U8-Commands. Probe-
  Requests dürfen weder Endpoint, Provider-, Profil- oder Capability-ID noch Healthstatus oder
  Zeit tragen; Projektsettings dürfen keine Pfade, Worktree-IDs, argv oder freien Commands
  annehmen. Component-Tests zeigen den weiter nutzbaren modellfreien Indexbrowser, eine nicht nur
  farbliche Remote-Warnung, explizite Probe-/Abbruchaktionen, validierte Limits, fail-closed
  Privacy, read-only Indexignore und die evidence-gebundene Commandauswahl.
  Settings-Component-Tests prüfen zusätzlich die horizontale Bereichsnavigation ohne zweite
  Sidebar, den zusammenhängenden Ablauf Provider verbinden und die dabei ausdrücklich ausgelöste
  erste Modellerkennung → rollenbezogene Dropdown-Auswahl, Provider-CRUD über Modale, die one-way
  Gemini-Key-Eingabe mit ausschließlich content-freiem Sternchen-Platzhalter nach erfolgreichem
  Speichern und die Abwesenheit freier Modell-IDs. Nach einem Reload müssen gespeicherte
  Rollenprofile unabhängig vom flüchtigen Katalog sichtbar bleiben; `Modelle aktualisieren` lädt
  nur auf einen späteren ausdrücklichen Klick neue Auswahlwerte. Jeder sichtbare Rollen- und
  Providerstatus führt außerdem über eine zugängliche Statushilfe zu einer content-freien
  Erklärung und einem konkreten nächsten Schritt; insbesondere erläutert `Capability fehlt` die
  fehlende Live-Verifikation von strukturiertem JSON, ohne rohe Providerantworten preiszugeben.
- Der U9-Designsystemvertrag extrahiert globale und sämtliche scoped Svelte-Styles. Außerhalb der
  zentralen Tokenquelle sind Hex-, RGB- und benannte Rohfarben verboten. Light und Dark müssen
  dieselben semantischen Rollen für Text, Flächen, Status, Fokus, Info, Erfolg, Warnung, Gefahr,
  Hypothese und Code definieren; normative Text-/Flächenpaare erreichen rechnerisch mindestens
  WCAG AA 4,5:1.
- Der U9-DOM-Contract prüft genau einen Main-Landmark, eindeutige IDs, lückenlose
  Überschriftenebenen, benannte native Interaktionen, Skip-Link, textuell oder per `aria-pressed`
  erkennbare Togglezustände und das Verbot positiver `tabindex`-Werte. Svelte-Typecheck und ESLint
  müssen ohne Accessibilitywarnung bestehen. Schriftgrößen in Pixeln sind verboten; Controls
  haben mindestens 44 CSS-Pixel Höhe.
- Reduced Motion deaktiviert Smooth Scrolling und begrenzt Animationen und Transitions auf einen
  einzelnen praktisch sofortigen Durchlauf. Der Browser-Smoke schaltet Light und Dark real um,
  misst einen sichtbaren 3-Pixel-Fokusring und prüft bei halbierter CSS-Viewportbreite als
  200-Prozent-Reflow-Äquivalent null horizontale Dokumentüberbreite. Agent-, Settings- und
  Fehlerflächen müssen in beiden Themes visuell lesbar bleiben.
- Der M8-Nutzerworkflow besitzt zwei komplementäre Verträge. Das reale Offline-Coding-Eval führt
  kleine Rust-, TypeScript-/Modul- und Python-Aufgaben sowie Replan und Context Compaction durch
  Policy, exakte Freigabe, Patch, Reindex, kataloggebundenen Test, Verification-Evidence und
  Acceptance bis zum durable `Done`. Der Desktop-Component-Contract beginnt ohne Projekt, öffnet
  den nativen Projektpfad ausschließlich durch die injizierte enge Open-Grenze, liest danach den
  aktiven Core-Zustand neu und bindet Goal, Ledger, terminale Activity sowie Must-Evidence an
  dieselbe Task-ID. Globales `Done` darf nur gemeinsam mit exakten frischen Step- und Evidence-IDs
  sichtbar werden.

### Persistenz

- Migration von leerer DB
- Upgrade aus jeder unterstützten Vorgängerversion
- Rollback des Appstarts bei fehlgeschlagener Migration ohne Datenverlust
- Contract-Tests gegen temporäre DB
- Goal-Contract-Contracts prüfen atomare initiale Erstellung, Linked-Worktree-Isolation,
  lückenlose Compare-and-Append-Revisionen, Konflikte konkurrierender Writer, unveränderte
  Auditstände und exakte Wiederherstellung nach Reopen.
- Task-Ledger-Contracts prüfen atomare Erstellung und Compare-and-Swap-Ersetzung,
  Linked-Worktree-Isolation, unveränderliche Definitionen sowie Versuchs- und Replan-Historie,
  fehlgeschlagene und erfolgreiche Verifikation, transitive Evidence-Invalidierung und exakte
  Wiederherstellung des vollständigen Aggregats nach Reopen.
- Run-Journal-Contracts prüfen atomaren Start, Linked-Worktree-Isolation, genau einen Gewinner bei
  konkurrierenden Appends derselben Sequenz, lückenloses Paging, atomare Run-Materialisierung,
  Redaction ohne Secret-Fixture, deterministischen begrenzten JSONL-V1-Export und exakte
  Wiederherstellung nach Reopen. Jeder neue Run behält dabei seine ModelProfile-ID und -Version;
  V14-Adaptertests erlauben ausschließlich migrierte Legacy-Nullpaare und lehnen partielle
  Profilreferenzen ab. V16-Contracts prüfen zusätzlich den atomaren Tool-Event-/Metadaten-/
  Evidence-Append ohne Raw Preview sowie den gemeinsamen Ledger-/Run-Commit mit getrennten
  Compare-and-Swap-Konflikten.
- V17-Recovery-Contracts schließen einen Store mit laufendem Toolversuch, öffnen ihn neu und
  verlangen Interrupted plus monotone Retry-Nummer. Sie prüfen frische und stale
  Verification-Evidence, Resume-Ablehnung auf altem Snapshot, transitives Step-Reopen,
  Resume/Replan/Cancel sowie den vollständigen Rollback von Published-Snapshot-, Ledger- und
  Run-Sequenzkonflikten. Nur der atomare Toolresultat-/Journalpfad darf einen Versuch als
  erfolgreich abschließen.
- V18-Policy-Contracts prüfen begründete Decision und Request nach Reopen, exakten Pfad-Scope,
  Mismatch ohne Grantverbrauch, einmalige Consumption, Widerruf, restriktive Workspace-Regeln und
  vollständigen Rollback von Decision, Request, Event und Runprojektion bei veraltetem
  Runsequenz-CAS.
- V19-Command-Allowlist-Contracts prüfen leeren Anfangszustand, exakte Confirmation, Reopen,
  monotone Revisionen, vollständigen Rollback eines veralteten CAS und Worktree-Isolation. Die
  Migrationstests prüfen zusätzlich V18→V19-Rollback, unveränderliche Revisionen und die allein
  erlaubte Worktree-Reconciliation-Cascade.
- V20-Verification-Contracts prüfen alle fünf typisierten Evidence-Varianten, Must-/Should-
  Acceptance, Timeout und Cancellation ohne Teilwrite, idempotentes Append, Reopen sowie gezielte
  Freshness-Ablehnung nach einer betroffenen Indexpublikation. Migrationstests decken leeres
  Schema, jeden Vorgänger bis V19 und vollständigen V19→V20-Rollback ab.
- V21-Contracts prüfen die rückwärtskompatible Rekonstruktion historischer V1-Actionklassen, alle
  sechs V2-Actionklassen und den atomaren Erfolgsabschluss eines mutierenden Toolversuchs zusammen
  mit content-freiem Journal-Event und Runprojektion einschließlich vollständigem Rollback bei
  Runsequenzkonflikt. Migrationstests decken leeres Schema, jeden Vorgänger bis V20 und
  vollständigen V20→V21-Rollback ab.
- V22-Recovery-Contracts prüfen atomaren Mutationsbeginn, `Applied`, `NotApplied` und `Unknown`,
  Reopen, worktreeweite Sperre, vollständige Snapshot-Reconciliation und das weiterhin notwendige
  Recovery-`Replan` einschließlich CAS-Rollback. Migrationstests decken leeres Schema, jeden
  Vorgänger bis V21, die konservative Übernahme laufender V21-Versuche und vollständigen
  V21→V22-Rollback ab.
- Rebuild trennt regenerierbare und dauerhafte Daten korrekt. V28-Contracts prüfen Backfill und
  Rollback, Sequenz eins beim ersten Lauf, erhaltenen High-Water-Mark nach Rebuild und Reopen,
  stale Start-CAS ohne Teilmutation sowie typisierten Überlauf. Der vollständige Fast-Index-
  Contract verlangt bei unverändertem Snapshot einen neuen Run-Anker und anschließend Deep Map
  `Ready`; historische Module Cards bleiben unverändert an den alten Anker gebunden.
- V29-Contracts prüfen Neuinstallation, V28→V29, atomare Planpersistenz, unveränderte Lesbarkeit
  historischer Läufe und vollständigen Rollback. Dashboard-Verträge prüfen die 20/50/50-Grenzen,
  projektgebundene Cursor, geschlossene Phasen- und Modulzustände, sichere Zielauflösung und das
  Fehlen von Prompts, Modellantworten, Source, Providerdaten, Budgets, Snapshots sowie internen IDs.
  Komponentenprüfungen decken Live-Aktualisierung ohne überlappende Reads, erhaltene Auswahl,
  veröffentlichte Cards, Fehlerhilfe, kompakte Historie und Atlas-Fokus bei 720×520 und 680×760 ab.
  Zeitgesteuerte Regressionen prüfen zusätzlich, dass ein angenommener Start den neuesten Lauf
  auswählt, verständliche aktuelle Aktivität und fortgeschriebene Planschritte sichtbar werden,
  der Gesamtzustand bis zum Abschluss nachzieht und weder ein langsamer Status-Read einen zweiten
  Read startet noch ein temporärer Publikations-Read-Fehler einen Managerzustand fälschlich als
  terminalen Ausführungsfehler ausgibt. Storage-Regressionen halten dazu eine echte
  Index-Publish-Transaktion offen und belegen, dass Live-Journal, Publikationsstatus und aktuelle
  Indexprojektion weiterhin über getrennte Transaktionskontexte lesbar bleiben; ein paralleler
  Journal-Write wartet begrenzt und wird anschließend erfolgreich abgeschlossen. Resize-Updates
  von Atlas und Inspector-Split werden außerhalb des `ResizeObserver`-Delivery-Zyklus auf genau
  einen Frame zusammengeführt, damit das Öffnen der Live-Details keine rekursive Layoutschleife
  erzeugt.
- Der Windows-libSQL-Test-Harness führt native In-Memory-Tests, jede unabhängige
  Storage-Contract-Phase und jeden libSQL-basierten inkrementellen Index-Contract in einem eigenen
  Worker aus; dieselbe Isolation schützt die Retrieval-Evalbaseline. Erfolg gilt erst nach dem
  Abschlussmarker hinter der letzten Assertion; nur
  `STATUS_ACCESS_VIOLATION` darf höchstens zweimal mit einem frischen Worker wiederholt werden.
  Assertion- und Vertragsfehler werden nie wiederholt. Verwaiste, exakt mit der Worker-PID
  präfixierte Testverzeichnisse werden nach dessen Prozessende entfernt.

### Index und Retrieval

- Golden Fixture für Parseränderungen
- deterministische Wiederholung ergibt identische normalisierte Resultate
- Löschung, Umbenennung und Syntaxfehler getestet
- Graphzyklen terminieren; kürzeste Pfade, Hopgrenze, Resultlimit und Beziehungsevidenz sind getestet
- Fusion-Golden fixiert Policyversion, Stable-ID-Deduplizierung, alle Signale und Exact-vor-Semantic
- Modulbildungs-Contracts prüfen verschachtelte Monorepo-Manifeste, manifestlose Pfadgrenzen,
  deterministische Wiederholung, SCC-Communities, genau eine primäre Membership, aktuelle
  Membership-Evidence, zentrale Symbole, Entrypoints, Tests, Repository Card, Cancellation und
  abgelehnte Progressausgabe
- Die mehrsprachige Deep-Map-Golden indiziert die Rust-, TypeScript- und Python-Produkt-Fixtures
  bis zum atomar veröffentlichten Index und fixiert aktuelle Modul-Evidence, vollständige
  leere-Coverage-Planung, Budget, Schrittverifikation und deterministische Wiederholung.
- Semantic-Card-/Embedding-Contracts prüfen BodyHash-Kanonik, Profil-/Dimensionsisolation,
  Redaction, Cancellation, Disabled ohne Adapterzugriff, persistentes Reopen, native
  dimensionsgebundene Vector-Capability, begrenzten linearen Fallback und semantikexklusiven Rebuild
- Claim-Verifier-Contracts prüfen das strikt versionierte Claim-Schema, exakte Evidence-Auflösung
  gegen den aktuell veröffentlichten Index, Ablehnung erfundener oder veralteter IDs, sichtbare
  Widersprüche, getrennte Classification und Confidence sowie ausschließlich verifizierte,
  atomare Card-Publikation mitsamt Evidence und Lexical-Search-Projektion
- Task-Lens-Contracts prüfen kanonische Goal-/Step-/Fehler-/Pfadseeds, Exact-vor-FTS-vor-Graph/Test-
  vor-Claim-vor-Semantic-Reihenfolge, L0 bis L3, Budget und sichtbare Trunkierung, Digest-
  Determinismus, Indexdelta, Cancellation/Deadline, Produktionscode mit Regressionstest,
  ausgeschlossene Großmodule sowie null stale Fact Leakage
- Invalidierungs-Contracts prüfen direkte Evidence-Änderung vor dem nächsten Read, `Stale` für
  eigene und `NeedsReview` nur für direkt abhängige Cards, Parser-/Mappergründe, stabile
  Direkt-vor-Abhängig-Remapreihenfolge, Queue-Cancellation und -Ersetzung, Erhalt unabhängiger
  aktueller Claims sowie null stale Fact Leakage nach Task-Lens-Rebuild
- Retrieval-Eval zeigt keinen unbegründeten Recall-Rückgang
- keine stale Evidence in Facts

### Model Provider

- Die gemeinsame dev-only Streaming-Contract-Suite prüft den neutralen Stub und den konkreten
  Ollama-Adapter auf exakte Provideridentität, begrenzte Ereignisfolge, genau eine terminale
  Completion am Streamende und dieselbe erwartete `ProviderEvent`-Projektion.
- Der allgemeine Application-Port besitzt keine Ollama-, HTTP- oder Adapter-Payloadtypen; der
  Cargo-Graph zeigt ausschließlich `a3-provider` → `a3-application` → `a3-domain`.
- Der neutrale Stubprovider emittiert exakt skriptbare Events und Fehler, wartet wakebar auf
  Cancellation und speichert ausschließlich content-freie Aufrufmetadaten.
- Der Ollama-Stubserver prüft die exakte Requestabbildung, fragmentierte chunked NDJSON-Antworten,
  Eventreihenfolge, terminale Usage und sauberes Body-Ende vollständig offline.
- Cancellation beendet Connect oder Body-Read und schließt die laufende Response; das
  Gesamttimeout wird vor Headern und während eines stockenden Response-Bodys als `TimedOut`
  normalisiert.
- Endpoint-Contracts prüfen localhost-Normalisierung, abgelehnte Credentials/Pfade, HTTPS-Pflicht
  für Remote, Local-only als Standard und Ablehnung vor jedem Netzwerkversuch ohne Policyfreigabe.
- Parser-Negativtests lehnen Modell-/Rollenabweichung, Tool Calls, zu große oder ungültige NDJSON-
  Daten, fehlenden Abschluss und Daten nach `done` ab. Prompt, Output, Endpoint und Provider-
  Fehlerbody dürfen nicht in Debug- oder normalisierte Fehlertexte gelangen.
- Gemini und Ollama normalisieren nur HTTP 408, 429 sowie retry-fähige 5xx-Status als
  `Unavailable`; 4xx, Redirects und 501 bleiben `Rejected`. Unit-Contracts halten diese
  Klassifikation fest, damit der providerneutrale Deep-Map-Retry keine dauerhaft ungültige Anfrage
  wiederholt.
- ModelProfile-Tests prüfen alle V1-Limits, deterministische ID-Ableitung, konservative UTF-8-
  Bytezählung, kanonische redigierte Stopbedingungen und Overrides, die Capability-Evidenz nicht
  verändern können. Jeder neue Run behält Profil-ID und Schemaversion nach Reopen.
- Die neutrale Capability-Stub-Suite belegt, dass weder Modellname noch manueller Override eine
  fehlgeschlagene Structured-Output-Probe hochstufen und dass explizite Providerkontextgrenzen vor
  Profilerzeugung gelten.
- Der Ollama-Stubserver prüft `/api/show`, das exakte kleine `/api/chat`-Schema, Profiloptionen,
  erfolgreiche und schemawidrige Probeantworten, Cancellation vor Netzwerk und ein gemeinsames
  Gesamttimeout über beide Requests. Metadaten mit mehreren abweichenden Kontextgrenzen werden
  abgelehnt; nur die exakte Capability `tools` setzt den nicht ausführbaren nativen Toolmodus.
- Der Gemini-Stubserver prüft den kanonischen Auth-Host, Ablehnung fremder HTTPS-Origins vor
  Netzwerk, `x-goog-api-key` und `x-goog-api-client`, begrenzte Pagination samt Token-Schleifen,
  Methodfilter, `responseJsonSchema`, SSE-Fragmentierung, Candidate-0-/Thought-Filter sowie strikte
  Finish- und Blockgründe. Die echte Structured-Output-Probe bleibt synthetisch und begrenzt,
  reserviert aber 256 Outputtokens, damit standardmäßig denkende Gemini-2.5-/3.x-Modelle das
  sichtbare Probeergebnis zuverlässig ausgeben können. Ein produktionsnaher Deep-Map-Contract
  prüft, dass der Adapter lokale Schema-Metadaten und die vom Google-Subset nicht unterstützten,
  später erneut validierten Schlüssel entfernt, `oneOf` kollisionssicher nach `anyOf` übersetzt
  und nur erreichbare `$defs`/`$ref` erhält. Eine weitere Regression verdichtet strukturell
  identische Tuple-Items unter Beibehaltung ihrer vereinigten Enums und exakten Arraygrenzen. Der
  SSE-Contract unterscheidet transiente Fehlerobjekte wie 503 von dauerhaften 400-Fehlern, ohne den
  Providertext freizugeben. Die Unit-Regression übersetzt zusätzlich die vollständigen Explorer-
  und Claim-Schemas; unbekannte Schlüssel bleiben abgelehnt. Embedding-Tests verwenden nur
  entdeckbare, nicht abgekündigte Modell-IDs; Gemini-Tool-Calls bleiben ohne eigenen
  Function-Calling-Probe `Disabled`.
- Der ignorierte Test `stored_user_key_lists_streams_structures_and_embeds_against_google` ist der
  einzige Live-Smoke: Er lädt den bereits von A^3 gespeicherten Benutzer-Key aus dem OS-Keyring und
  prüft nach separater Netzwerkfreigabe `gemini-flash-latest`, `gemini-3.7-flash` und den
  vorhandenen `gemini-pro-latest`-Alias über Capability-Probe, SSE, ein kleines Schema und die
  produktionsnahe erste Deep-Map-Anfrage sowie Embedding ausschließlich am kanonischen
  Google-Origin. CI und normale Testläufe führen ihn nie aus.
- Der OpenAI-Stubserver prüft die Responses-API sowohl für schemafreie Agent-Conversation als auch
  für Strict Structured Output. Der Wire-Contract darf aus den vollständigen Explorer-, Claim- und
  AgentAction-Schemas kein `const`, `oneOf`, `prefixItems` oder `uniqueItems` übertragen, muss
  `additionalProperties: false` und vollständige `required`-Mengen erhalten und feste Tuple-Items
  nur unter unveränderten exakten Arraygrenzen verdichten. Unbekannte Dialektschlüssel, optionale
  Objektfelder und fremde Referenzen müssen vor Netzwerkzugriff scheitern; der unveränderte
  A^3-Decoder bleibt für die engere positions- und evidencegebundene Antwort autoritativ.

### AgentAction und Prompt

- Der ADR-0042-Arbeitsplan-Contract kompiliert Listen- und nummerierte Einträge aus
  `Implementation Changes` und `Test Plan` in höchstens 64 geordnete atomare Schritte, weist jede
  Definition einer aktuellen Verification zu und lehnt leere beziehungsweise übergroße Pläne vor
  Task-Erzeugung ab. Replan-Regressionen müssen den aktiven Versuch zuerst schließen, die neue
  Ledger-Revision atomar persistieren, abgeschlossene Schritte erhalten und ein neues sichtbares
  Planlücken-Todo vor den ersetzten Nachfolgern einfügen. Ein Richtungsblocker muss zu
  `AwaitingUser`, ein interner Executorfehler weiterhin zu `Failed` führen.

- Domain-Tests prüfen Grenzen und Redaction für Search, paged File Inspect, Testselektor,
  nicht-verifizierende Ledger-Intents sowie die eindeutige Mutationsklassifikation. V1 bleibt als
  read-only Historienvertrag lesbar; V2 ergänzt ausschließlich strukturierte ApplyPatch- und
  kataloggebundene Run-Aktionen.
- Schema- und Decoder-Tests akzeptieren alle sechs V2-Top-Level-Actions und sämtliche fünf Inspect-
  Ziele, lehnen aber unbekannte Toolnamen und Felder, Trailing Text, Traversalpfade, rohe argv-/
  Shellfelder, nicht kanonische IDs, widersprüchliche Patchanker sowie übergroße oder
  kontrollzeichenhaltige Werte ab. V1 bleibt getrennt rückwärtskompatibel dekodierbar; Schema und
  Decoder werden unabhängig geprüft und jede Objektebene ist geschlossen.
- Prompttests zählen den statischen Vertrag mit dem ModelProfile-Counter gegen das feste
  900-Token-Budget, blockieren Profile ohne verifizierten Structured Output und vergleichen die
  optionale kanonische Schemawiederholung mit demselben Provider-Schema.
- Repair-Tests belegen eine nicht clonebare, bei Anweisungserzeugung verbrauchte Befugnis, keine
  Wiederholung geheim markierter ungültiger Rohbytes und terminale Ablehnung eines ebenfalls
  ungültigen zweiten Dokuments.
- Die Gate-M6-End-to-End-Abnahme indiziert und publiziert die Rust-, TypeScript- und Python-
  Produkt-Fixtures real und führt je Fixture zwei neutrale Modellturns über Context Compiler,
  SearchTool, durable Tool-Evidence, Ledger-Verifikation, Run Journal und Acceptance-Verifier bis
  `Done`. Der Repository-Dateibaum bleibt bytegleich. Ein Negativlauf über denselben Stack verlangt
  nach ungültiger Primär- und Reparaturausgabe null Toolaufrufe, null durable Toolversuche und null
  Tool-Journalereignisse.
- Die Gate-E7-End-to-End-Abnahme führt reale Patch- und Prozesspfade über libSQL, zentrale Policy,
  Approval, Workspace-Adapter, Fast Index, Context Compiler, Verification Engine und Run Journal.
  Sie belegt einen unveränderten Worktree während `AwaitApproval`, genau einen Worktree-Lease,
  unmittelbares Reindexieren sichtbarer Patchänderungen, ausschließlich neuen Snapshotkontext,
  Diff-Completion erst nach typisierter Evidence und `Replan` nach der zweiten identischen
  fehlgeschlagenen Run-Aktion.
- Die Gate-E9-Coding-Evaluation führt fünf vollständig lokale Python-Fixtures über denselben realen
  Mutation-, Index-, Context-, Evidence-, Acceptance- und libSQL-Pfad. Das versionierte Golden
  `fixtures/agent-coding-eval-v1/expected-results.json` fixiert kleinen Bugfix, atomare
  Zwei-Modul-Änderung, reine Testergänzung, roten Plan mit Replan und zwischenzeitlicher
  Nutzeränderung sowie eine zweistufige Fortsetzung nach Context Compaction. Zwei unabhängige
  vollständige Durchläufe müssen dieselbe geordnete Ergebnisprojektion liefern. Jeder erfolgreiche
  Fall lädt Goal, Ledger samt Store-Version und Run erneut aus dem Store und weist Goal, Step,
  Patch, Evidence und Verification nach. Ein roter Must-Test wird bereits vom typisierten
  Acceptance-Request als `IncompleteLedger` abgelehnt und kann keinen `Done`-Zustand erzeugen.

### Compaction

- Der Domain-Contract kompiliert dasselbe Langlauffixture 64-mal neu aus Goal, Ledger, Run,
  Published Index und Original-Claims. Goal-Referenz, Step-/Attempt-/Run-/Evidence-Quellen, offene
  fehlgeschlagene Verifikation und aktive Hypothesen müssen in jeder Projektion erhalten bleiben;
  stale beziehungsweise evidence-inkompatible Claims bleiben ausgeschlossen. Ein Claim aus einem
  älteren Source-Run bleibt nach einem unabhängigen Publish als Provenienz erhalten, wenn seine
  konkrete Evidence im aktuellen Index weiterhin auflösbar ist.
- Der `RunMemoryCheckpoint` akzeptiert keinen früheren Checkpoint als Eingang. Gleiche
  autoritative Eingaben erzeugen denselben Digest; eine neue Ledger-/Event-Materialisierung ändert
  ihn. Die nur gelesene `RunEventSequence` bleibt unverändert, während der bestehende
  Run-Journal-Contract weiterhin alle Audit-Events nach Reopen nachweist.
- Der Context-Contract prüft die tatsächliche Reinjection von Step Result, offenem Fehler und
  Hypothese mit originalen IDs, deterministische Claim-Deduplizierung sowie die lückenlose
  konservative Budgetrechnung. Run Memory wird vor der Task Lens gegen `CodeAndEvidence`
  reserviert; unpassende Run-/Snapshot-Bindungen und Secret-Kandidaten werden abgelehnt.

### Security Boundary

- Negativtests für Traversal, Symlinks und unerlaubte Roots
- ungültige IPC- und LLM-Payloads abgelehnt; Goal-Contract-V1 fixiert in Rust exakte Schlüssel und
  eine stabile JSON-Form, während der TypeScript-Runtimeparser zusätzlich IDs,
  Revisionsmetadaten, UTF-8-Byte- und Listengrenzen sowie eindeutige Inhalte erneut prüft
- Approval- und Policy-Tests: abgeleitete Klassen/Risiken, unverrückbare Systembaseline,
  Pfad-Scope-Mismatch, Ablauf, Widerruf, One-time-Consumption und ungültige persistierte Formen
- gemeinsamer Storagevertrag für PolicyDecision, Request, Grant, Reopen und atomaren Run-/Approval-
  CAS-Rollback; jede Auswertung erzeugt genau ein typisiertes Audit-Event
- Secure-File-Tool-Contracts prüfen einen erlaubten verschachtelten Read samt exakter Span-Evidence,
  vorwärts paginierte direkte Directory-Kinder aus einem snapshot- und worktreegebundenen
  Published Index sowie konkrete Evidence für abgeleitete Verzeichnisse. Nicht publizierte
  Ignore-Dateien und selbst künstlich publizierte Built-in-Secret-/Generated-Pfade bleiben aus der
  Ausgabe ausgeschlossen.
- Negativverträge lehnen nicht konstruierbare Traversalpfade, einen realen Symlink-/Junction-Escape,
  Unix-Sockets als Sonderdateien, Binary-Präfixe, Secret-Kandidaten und Dateien oberhalb von 4 MiB
  ohne Preview oder Quelldaten im Fehler ab. Windows und der Linux-Quality-Job führen dieselbe
  öffentliche Port-Suite aus; der Unix-Sonderdateifall ist plattformspezifisch zusätzlich aktiv.
- PatchAction-Contracts prüfen kanonische getrennte Add-, Update-, Move- und Delete-Operationen,
  Snapshot- und Hashbindung, exakten Approval-Fingerprint, Binary-/Secret-Ablehnung sowie
  unveränderte UTF-8-BOM-, CRLF- und Nicht-ASCII-Bytes. Die öffentliche Workspace-Port-Suite prüft
  die begrenzte Vorschau, tatsächliche Post-Patch-Hashes, No-Replace, Useränderung zwischen Preview
  und Apply, Symlink-/Junction-Escape und ein explizites partielles Change-Set nach spätem Konflikt.
- ProcessRunner-Contracts kompilieren dasselbe argv-basierte Fixture auf Windows, Linux und macOS.
  Sie prüfen unveränderte Argumentgrenzen trotz Shell-Metazeichen, kanonisches CWD und Executable,
  eine geleerte Umgebung mit expliziter Allowlist, Timeout eines Endlosprozesses, Beendigung eines
  erzeugten Kindprozesses bei Cancellation und lückenlos terminierende Stream-Events. Ein
  Mehr-MiB-Ausgabestrom muss trotz kleinem Retained Limit vollständig gedraint werden; Secret-
  Kandidaten dürfen weder im Resultat noch in Stream-Events erscheinen.
- Command-Discovery-Akzeptanz veröffentlicht reale Rust-, TypeScript-Monorepo- und
  Python-Fixtures über den Fast Index. Sie prüft Cargo `--offline --locked`, eindeutige
  Package-Manager-Evidence, Root- und Package-CWD, Python-Modulbefehle, die Abwesenheit jeder
  Installationskategorie sowie plan-ungebundene, nicht automatisch erlaubte `ProcessSpec`-
  Vorschauen. Das Node-Fixture besitzt bewusst keine Lockdatei; dennoch wird kein Installversuch
  erzeugt oder gestartet.
- Mutationsgrenztests lehnen rohe Modell-argv und Shellfelder ab, serialisieren alle Patch- und
  Prozessaktionen desselben Worktrees, persistieren Policy und Approval vor Ausführung und geben
  nach einer sichtbaren Patchänderung niemals Kontext auf Basis des alten Snapshots aus.
- E8-Failure-Recovery-Verträge prüfen Patchkonflikt, partiellen Patch, fehlgeschlagene
  Command-Verifikation, Timeout, Providerabbruch, Store-Unverfügbarkeit und -Korruption,
  Cancellation vor und nach Prozessstart sowie den Crash zwischen sichtbarem Patch und
  Journalabschluss. Jede Wirkung besitzt eine exakte Disposition; ein `Unknown` übernimmt per
  Vollscan fremde Änderungen und sperrt bis Reconciliation plus Replan jede weitere Mutation.
- Secret-Redaction-Test
- Prozessabbruch und Outputlimit getestet

## Testpyramide

| Ebene            | Zweck                                           |
| ---------------- | ----------------------------------------------- |
| Domain Unit      | Invarianten und Zustandsübergänge               |
| Property         | Parser-, Pfad-, Hash- und Zustandskombinationen |
| Adapter Contract | gleiche Semantik je Provider oder Store         |
| Golden Fixture   | stabile Index- und Context-Ergebnisse           |
| Integration      | DB, Workspace, Modellstub und Controller        |
| End-to-End       | Desktop-Workflow auf kleinem Fixture-Repo       |
| Evaluation       | reale Coding-Aufgaben und Retrievalqualität     |
| Platform Smoke   | Windows, Linux und macOS                        |

Tests müssen offline und deterministisch laufen, außer explizit markierten optionalen Provider-Benchmarks.

## Referenz-Fixtures

Mindestens:

- kleines Rust-Workspace-Projekt
- TypeScript-Monorepo
- Python-Package
- gemischtes Repository mit generierten und ignorierten Dateien
- Repository mit Symlinks
- Repository mit absichtlichen Parsefehlern
- großes synthetisches Repository für Performance

Fixtures enthalten keine inkompatibel lizenzierten oder vertraulichen Quellen.

## Performancebudgets

Die Budgets gelten auf einer dokumentierten Referenzmaschine mit 8 CPU-Kernen, 32 GB RAM und NVMe; LLM-Server und Modellgewichte werden bei App-RAM separat ausgewiesen.

| Messung                          |                          Ziel für V1 |
| -------------------------------- | -----------------------------------: |
| Desktop bis interaktiv, warm     |                            P95 ≤ 2 s |
| Idle-RAM ohne Modellserver       |                             ≤ 200 MB |
| Fast Index, 100.000 LOC cold     |                           P95 ≤ 30 s |
| Ein-Datei-Indexdelta             |                            P95 ≤ 2 s |
| exakte oder FTS-Suche            |                         P95 ≤ 100 ms |
| Context Compile ohne LLM         |                         P95 ≤ 300 ms |
| UI-Interaktion während Indexlauf | keine sichtbare Blockade über 100 ms |
| Cancellation-Reaktion            |      ≤ 500 ms plus Prozessbeendigung |

Diese Zahlen sind Releaseziele. Wird ein Ziel nicht erreicht, braucht der Release eine dokumentierte Abweichung, Messdaten und einen konkreten Folgetask.

Idle-RAM bezeichnet den privaten residenten Speicher des vollständigen App-Prozessbaums nach
Warm-up; Provider- und Modellserverprozesse werden über die Prozessbaumgrenze ausgeschlossen und
getrennt ausgewiesen. Auf Windows ist dies die Summe von `Working Set - Private` für den nativen
A^3-Prozess und alle seine WebView2-Kinder. Das gesamte Working Set und Private Bytes werden als
Diagnosewerte mitgeführt, aber nicht als Idle-RAM ausgegeben, weil sie gemeinsam genutzte Seiten
mehrfach beziehungsweise auch nicht residente Reservierungen zählen.

U10 besitzt das reale lokale Browserprofil
`apps/desktop/performance/u10-index-burst.html`. Es verwendet den produktiven `UiScheduler` für 30
Samples mit je 10.000 gleichartigen Indexcommits und misst Enqueue-P95, Event-Loop-
Interaktions-P95, Long Tasks, Pending-Commit-Obergrenze und gerenderte DOM-Zeilen. Auf Windows 11
Pro Build 26200, AMD Ryzen 9 5900XT, 32 GiB RAM und Chromium 151 ergab der Lauf vom 2026-08-13
Enqueue-P95 1,1 ms, Interaktions-P95 1,3 ms, null Long Tasks, genau einen pending und einen
gerenderten Commit sowie 50 gerenderte Zeilen. Damit bleibt der synthetische Indexburst unter der
100-ms-Blockadegrenze; der Lauf ersetzt weiterhin nicht den UX-Smoke eines echten Indexjobs.

Das reproduzierbare native Profil
`apps/desktop/performance/measure-u10-idle-ram.ps1` baut auf dem Releasebinary auf, wärmt 15 Sekunden
auf und erfasst 30 Samples im Abstand von einer Sekunde. Auf derselben Maschine lag der private
residente Median des stabilen achtteiligen A^3-/WebView2-Prozessbaums am 2026-08-13 bei 121,475 MiB,
der Sample-Peak bei 122,734 MiB. Ein parallel beobachteter `ollama`-Prozess lag außerhalb dieses
Baums und wurde getrennt ausgewiesen. Zur Diagnose betrugen das gesamte Working Set im Median
411,738 MiB und Private Bytes 276,406 MiB. Das lokale Idle-RAM-Profil besteht damit das
200-MB-Budget; es ersetzt nicht die abschließende V1-Referenzmessung auf der definierten
8-Core-Maschine.

Der U10-Produktionsbuild reduzierte den initialen JavaScript-Chunk gegenüber dem unmittelbar vor
U10 gemessenen Stand von 420,17 kB roh/117,63 kB gzip auf 279,29 kB roh/77,81 kB gzip. Graph,
Settings, Agent Workspace, Inspektor und Approval Center liegen in getrennten lokalen Lazy-Chunks;
die Profilseite ist kein Build-Entry und wird nicht in das Produktbundle aufgenommen.

S11 besitzt dafür den reproduzierbaren ignorierten Release-Test
`incremental_index_performance::one_file_delta_meets_the_two_second_p95_target`. Das Fixture umfasst
200 Rust-Dateien und 100.000 LOC; jede der 30 Stichproben misst vom gleich großen Ein-Datei-Write über
Watcher-Debounce, Git-Discovery, BLAKE3-Bestätigung, Ein-Datei-Parse, vollständiges Link/Rank und
atomisches libSQL-Publish. Auf Windows 11 Pro, AMD Ryzen 9 5900XT, 32 GiB RAM und Samsung 970 EVO
Plus NVMe wurden am 2026-08-05 P50 1,202 s und P95 1,305 s gemessen; Watcher-P95 betrug 389 ms und
Refresh-/Publish-P95 922 ms. Die gemessene Ausgangsversion mit zeilenweisen SQL-Aufrufen lag bei
P95 15,286 s, ein erster 900-Parameter-Batch bei 14,493 s. Erst höchstens 30.000 Parameter,
1.024 Zeilen pro Cancellation-Checkpoint und transaktionale Retention supersedeter Projektionen
erreichten das Budget. Diese lokale Messung ersetzt nicht die abschließende V1-Referenzmessung auf
der oben definierten 8-Core-Maschine.

R11 wiederholte dieses Gate nach Erweiterung des atomaren Publishes um Card-Invalidierung. Der
erste 30-Sample-Lauf öffnete die inzwischen größere Knowledge-Datenbank weiterhin für jeden
Snapshot- und Run-Schritt neu und verfehlte das Ziel mit P50 2,299 s, P95 3,362 s,
Watcher-P95 391 ms und Refresh-/Publish-P95 3,047 s. Nach getrenntem, auf vier Worktrees begrenztem
Wiederverwenden bereits vollständig identitäts- und policygeprüfter Mutationshandles erreichte
derselbe unveränderte Release-Test am 2026-08-06 P50 816 ms, P95 884 ms, Watcher-P95 394 ms und
Refresh-/Publish-P95 491 ms. Ein isolierter Diagnoselauf maß den neuen Invalidierungsabschnitt bei
leerem Cardbestand mit rund 0,7 ms pro Publish; daraus wird kein allgemeiner Geschwindigkeitsclaim
für große Cardbestände abgeleitet.

R1 besitzt den reproduzierbaren ignorierten Release-Test
`exact_search_performance::exact_symbol_search_meets_the_100_millisecond_p95_target`. Das Fixture
enthält 50.000 Symbole als Projektion von 100.000 strukturellen Zeilen. Auf derselben lokalen
Windows-11-Maschine wurden am 2026-08-05 für den vor R1 notwendigen vollständigen Index-Load mit
anschließendem Namensscan über fünf Samples P50 652,8 ms und P95 656,8 ms gemessen. Die
indexgestützte Exact Query über 30 Samples erreichte nach begrenztem Wiederverwenden vollständig
verifizierter, identitätsgebundener Datenbankhandles P50 37,0 ms und P95 39,7 ms. Die erste Messung
mit erneutem Open, Migration und Integritätsprüfung pro Query lag bei P50 554,0 ms und P95 570,5 ms.
Auch diese lokale Messung ersetzt nicht die abschließende V1-Referenzmessung.

R2 verwendet dasselbe Fixture und denselben Release-Test für eine absichtlich falsch geschriebene
FTS-Query. Die erste breite Trigram-`OR`-Messung lag bei P50 194,1 ms und P95 195,9 ms; eine reine
Reduktion auf 512 nachbewertete Kandidaten erreichte P50 169,1 ms und P95 201,8 ms und verfehlte das
Gate weiterhin. Die begrenzte Ein-Fehler-Abfrage mit zusätzlichem Endanker erreichte am 2026-08-05
über 30 Samples P50 34,9 ms und P95 35,3 ms. Der unveränderte vollständige Index-Load plus Scan lag
in diesem Lauf über fünf Samples bei P50 1,145 s und P95 1,189 s; Exact Search erreichte P50 38,3 ms
und P95 41,5 ms.

R6 erweitert dasselbe reproduzierbare Fixture um eine primäre Membership für alle 50.000 Symbole
und lädt beim alten Full-Index-Vergleich zusätzlich die vollständige V8-Modulprojektion. Der Lauf
vom 2026-08-05 ergab über fünf Full-Load-/Scan-Samples P50 1,425 s und P95 1,452 s gegenüber
P50 1,145 s und P95 1,189 s vor R6. Über jeweils 30 Querysamples lagen Exact Search bei
P50 38,9 ms und P95 60,6 ms sowie FTS bei P50 36,2 ms und P95 39,5 ms; beide bleiben unter dem
100-ms-Gate. Die Messung dokumentiert damit den zusätzlichen vollständigen Loadaufwand der
evidenzgebundenen Membershipzeilen, ohne daraus eine Geschwindigkeitsverbesserung abzuleiten.

R10 erweitert denselben ignorierten Release-Test um 30 vollständige Task-Lens-Compiles ohne LLM.
Das 50.000-Symbole-Fixture publiziert zusätzlich einen aktuellen, symbolgebundenen Fact; jede
Stichprobe umfasst aktuelle Run-Prüfung, Exact, FTS, Graph/Test, Claim-Rekonstruktion, Fusion,
Budgetierung und Digest. Der unveränderte erste Stand rekonstruierte und kopierte den vollständigen
Index pro Lens und erreichte am 2026-08-06 P50 1,745 s und P95 2,168 s. Eine auf einen Eintrag
begrenzte Shared-Index-Capability, die vor jeder Ausgabe den dauerhaften neuesten Run prüft und bei
Publish/Rebuild aktualisiert wird, erreichte auf derselben lokalen Maschine mit dem verifizierten
Fact P50 251,101 ms und P95 267,307 ms. Im selben finalen Lauf lagen Exact Search bei P95 50,811 ms,
FTS bei P95 37,762 ms und die absichtlich weiterhin tiefe vollständige Indexkopie bei P95 1,254 s.
Damit besteht die Task-Lens-Context-Vorstufe das 300-ms-Gate; die lokale Messung ersetzt nicht die
abschließende V1-Referenzmessung.

H7 erweitert dasselbe Release-Fixture um 30 vollständige Context-Compiles. Jede Stichprobe umfasst
den unveränderten Task-Lens-Pfad sowie Anchor, Bereichsbudgetierung, Zoom-/Claim-/Tool-Packing,
Freshness-, Secret- und Gesamtbudgetprüfung, Promptkonstruktion und `ContextDigest`; ein LLM-Aufruf
ist ausdrücklich nicht enthalten. Im Lauf vom 2026-08-06 lagen die direkte Task Lens bei
P50 134,457 ms und P95 141,473 ms sowie der vollständige Context Compile bei P50 158,352 ms und
P95 215,220 ms. Im selben Prozess lagen Exact bei P95 31,899 ms und FTS bei P95 39,808 ms. Damit
besteht der vollständige H7-Pfad das 300-ms-Gate; die lokale Messung ersetzt weiterhin nicht die
abschließende V1-Referenzmessung.

Modellmetriken werden separat erfasst:

- Time to First Token
- Prompt-Tokens
- Output-Tokens
- Tokens pro Sekunde
- Toolerfolg beim ersten Versuch
- Taskerfolg

## Retrieval- und Agentenevaluation

Der R7-Contract erzeugt aus derselben veröffentlichten Index- und Coverage-Projektion zweimal den
identischen vollständigen `ExplorePlan` und fixiert Manifest → Entrypoint/Zentralsymbol → offene
Modul-Coverage als V1-Golden-Reihenfolge. Separate Grenzfälle belegen Snapshot-/Schemaschutz,
Unknown-Module-Ablehnung, das Überspringen bereits vollständig abgedeckter Module, alle drei
Budgetdimensionen sowie Cancellation, Budget-, Coverage-, Stagnations- und Gain-Stopgründe. Kein
Test ersetzt einen Index durch Modelloutput.

Die Gate-M4/M5-Retrievalbaseline V1 läuft offline über den echten Index-/Publish-/libSQL-Suchpfad
des gemischten Rust-/TypeScript-/Python-Fixtures. Ihre reviewbare Golden-Datei bindet sechs Exact-,
Lexical- und Graphfälle mit sieben Erwartungen an Kanal, native Begründung und Top-5-Rang. Sie
verlangt 100 Prozent Recall@5, fixiert MRR 0,9285, weist aktuelle Run-/Snapshot-/Revision-Bindung
nach und normalisiert zwei Wiederholungen bytegleich. Der spätere Q1-Umfang ergänzt darauf
aufbauend Agenten-, User-Edit-, Stale-Evidence- und Compaction-Aufgaben.

Der Gate-M4/M5-No-Embeddings-Contract führt den aktuellen Anwendungskern über einen real
publizierten gemischten Index, vollständige Deep-Map-Planung und budgetierte Task-Lens-Kompilierung
aus. Zwei Wiederholungen müssen identisch und aktuell sein; mindestens Exact und Graph müssen
vertreten sein, während `SourceChannel::Semantic` ohne injizierten Semantic-Port ausgeschlossen
bleibt. Ein nicht leerer Card-Batch muss außerdem im konstruktiv provider- und cachelosen
`GenerateSemanticEmbeddings::disabled()`-Pfad vollständig als deaktiviert enden.

Ein versioniertes Eval-Set enthält:

- Symbol finden
- Architekturfrage beantworten
- Fehler lokalisieren
- kleinen Bug beheben
- API über mehrere Module ändern
- Test ergänzen
- Änderung nach zwischenzeitlichem User-Edit fortsetzen
- lange Aufgabe nach Context Compaction fortsetzen

Mindestbedingungen vor V1:

- keine stale Facts in 100 Prozent der Invalidierungstests;
- Goal Contract bleibt in 100 Prozent der Langlauf-Fixtures erhalten;
- keine Mutation außerhalb des erlaubten Roots;
- alle Muss-Aufgaben des Eval-Sets besitzen reproduzierbare Baselines;
- Qualitätswerte dürfen durch einen Release nicht unbemerkt sinken.

## Cross-Platform-Matrix

CI baut und testet:

- Windows x86_64
- Linux x86_64
- macOS Apple Silicon
- macOS x86_64, solange unterstützt und praktikabel

Jeder Matrixjob startet nach dem nativen Releasebuild das unveränderte A^3-Binary in der
plattformzugehörigen System-WebView. Der M8-Smoke verlangt ein sichtbares, prozessgebundenes
Fenster mit mindestens 720 × 520 Punkten sowie einen mindestens 4 KiB großen Screenshot. Windows
und Linux prüfen zusätzlich Stichprobenfarbvarianz beziehungsweise Bildstandardabweichung gegen
eine leere Fläche. Linux läuft dafür in einem isolierten Xvfb-Display; nur der Smoke-Prozess
deaktiviert WebKitGTK Accelerated Compositing, damit die WebView-Fläche im prozessgebundenen
Fensterbild enthalten ist. macOS löst das konkrete Fenster über CoreGraphics auf. Screenshot und
dimensionsgebundener JSON-Bericht werden getrennt
für Linux x86_64, Windows x86_64, macOS ARM64 und macOS x86_64 aufbewahrt. Ein nativer Build ohne
diesen WebView-Nachweis erfüllt das Desktop-Plattformgate nicht.

Plattformspezifische Installer werden auf der Zielplattform erzeugt und signiert, sobald Distributionsidentitäten verfügbar sind.

## Definition of Done

Ein Arbeitspaket ist Done, wenn:

- alle Akzeptanzkriterien nachweisbar erfüllt sind;
- Architekturregeln und relevante ADRs eingehalten sind;
- erforderliche Tests existieren und bestehen;
- relevante Performancebudgets gemessen sind;
- Fehler-, Abbruch- und Sicherheitswege getestet sind;
- Dokumentation und Schemas aktuell sind;
- finaler Diff keine fremden Änderungen, Secrets oder Debugreste enthält;
- Restunsicherheiten offen dokumentiert sind.

Ein Checklistenpunkt darf erst danach abgehakt werden.
