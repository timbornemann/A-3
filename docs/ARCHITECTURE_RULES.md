# Architektur- und Code-Regeln

Status: verbindlich  
Stand: 2026-11-06

[ADR-0119](adrs/0119-kleinschrittiger-agentenkern-fuer-lokale-modelle.md) ist
angenommen. Diese Regeln gelten für die neuen Verträge aus
[Plan 22](plans/22-SMALL-LOCAL-MODEL-AGENT-CORE.md). Die heutige SourceGuided-
Produktkomposition bleibt bis P11 als eingefrorene Altbaseline ausgewählt;
P2–P10 verwenden ausschließlich eine Kandidaten-Testkomposition und isolierte
Daten. Historische Implementierungsbeschreibungen begründen keinen zweiten
produktiven Steuerungspfad. Sicherheits- und Recoveryregeln bleiben verbindlich.
Für den Entwicklungsneustart gilt zusätzlich [ADR-0119](adrs/0119-kleinschrittiger-agentenkern-fuer-lokale-modelle.md): keine Übernahme
oder Produktlesbarkeit alter Entwicklungsdaten; frischer Datenbestand in P11.
Die eingefrorene P0-Reproduktion und Recovery neuer Runs bleiben erhalten.

## 1. Verantwortung und Kohäsion

1. Jede Datei, jedes Modul und jedes Crate MUSS eine klar benennbare Hauptverantwortung besitzen.
2. Ein Modul DARF NICHT gleichzeitig Fachentscheidung, Persistenz, UI und Transport behandeln.
3. Eine Struktur wird geteilt, wenn Teile aus unterschiedlichen Gründen geändert oder unterschiedlich getestet werden.
4. Eine Struktur wird NICHT allein wegen einer beliebigen Zeilenzahl geteilt.
5. Use Cases SOLLTEN als kleine Orchestratoren lesbar sein; komplexe Regeln gehören in Domain Services oder Value Objects.

## 2. Domänenmodell

1. IDs, Hashes, Pfade, Tokenmengen, Modellnamen und Zustände MÜSSEN eigene Typen erhalten.
2. Boolesche Parameter mit unklarer Bedeutung sind verboten; Enums oder Optionsobjekte sind zu verwenden.
3. Zustandsübergänge MÜSSEN über Methoden erfolgen, die Invarianten prüfen.
4. Persistenz- und API-DTOs DÜRFEN NICHT als Domänenobjekte wiederverwendet werden.
5. Zeit und ID-Erzeugung MÜSSEN über injizierbare Ports laufen, wenn sie Verhalten beeinflussen.

## 3. Schichten und Abhängigkeiten

1. Abhängigkeiten zeigen nach innen: Adapter → Application → Domain.
2. Die Domain kennt keine Frameworks.
3. Anwendungscode kennt keine SQL-Statements, HTTP-Endpunkte oder Tauri-Handles.
4. Cross-Crate-Zyklen sind verboten.
5. Plattformabfragen außerhalb dedizierter Adapter sind verboten.
6. Ein neuer Port ist gerechtfertigt, wenn eine externe Grenze, zwei Implementierungen oder ein notwendiger Test-Seam existiert.
7. Spekulative Interfaces ohne konkreten Verbraucher sind zu vermeiden.

## 4. Fehler

1. Erwartbare Fehler MÜSSEN typisiert sein.
2. Panics sind ausschließlich für nachweislich unerreichbare interne Zustände zulässig.
3. Auf erreichbaren Produktionspfaden sind unwrap, expect, todo und unimplemented verboten.
4. Fehler erhalten am Adapter- oder Use-Case-Rand Kontext, ohne Geheimnisse oder vollständigen Quellcode zu protokollieren.
5. Kontrolle über formatierte Fehlermeldungen ist verboten.
6. UI-Fehler verwenden stabile Fehlercodes plus sichere, lokalisierbare Meldungen.

## 5. Async, Jobs und Ressourcen

1. Jeder lange Job besitzt JobId, Owner, Status, Fortschritt und CancellationToken.
2. Kein Task wird fire-and-forget gestartet.
3. Channels und Parallelität sind begrenzt.
4. Subprozesse besitzen Timeout, Outputlimit und kontrolliertes Beenden des Prozessbaums.
5. Große Dateien werden gestreamt oder begrenzt gelesen.
6. Datenbanktransaktionen bleiben kurz; LLM-, Datei- oder Prozessaufrufe innerhalb einer offenen Transaktion sind verboten.
7. Backpressure ist sichtbar und darf keine stillen Datenverluste erzeugen.
8. Produktspezifische Pausezustände DÜRFEN den gemeinsamen Scheduler-Automaten nicht umgehen. Eine
   Deep Map gilt erst nach terminaler kooperativer Cancellation und validiertem Checkpoint als
   pausiert; Resume ist ein neuer besessener Job mit unverändertem Budget.
9. Dasselbe gilt für Agentenarbeit: Der flüchtige Produktzustand `Paused` darf erst nach
   terminaler Scheduler-Cancellation, Executor-Bestätigung und H11/E8-Revalidierung erscheinen.
   Er erweitert weder den fachlichen Agent-Controller noch dessen persistente Zustandsmaschine.
   Resume und Replan starten ausschließlich nach erfolgreichem Recovery-CAS einen neuen
   Scheduler-eigenen Versuch mit frischeren Ledgerankern.

## 6. Persistenz

1. Schemaänderungen erfolgen ausschließlich über versionierte Vorwärtsmigrationen.
2. Jeder Index-Delta-Commit ist atomar.
3. Repository-abgeleitete Daten sind regenerierbar und von nicht regenerierbaren Task- und Entscheidungsdaten getrennt.
4. Jede persistierte Evidenz enthält ihren Snapshot- und Inhaltsbezug.
5. Ein Embedding enthält Modell-ID, Dimensionszahl, Normalisierungsvariante und Body-Hash.
6. Rohes SQL bleibt im Storage-Adapter.
7. Geheimnisse gehören nie in die Projekt-DB.

## 7. Index und Retrieval

1. Deterministische Quellen werden vor semantischen Quellen abgefragt.
2. Exakte Identifier-, Pfad-, Referenz- und Testtreffer haben Vorrang vor Vektorähnlichkeit.
3. Vektorähnlichkeit allein darf keinen Claim belegen.
4. Gelöschte oder veränderte Dateien invalidieren abhängige Daten, bevor sie auslieferbar sind.
5. Retrieval muss für identische Query-, Snapshot- und Policy-Versionen reproduzierbar sein.
6. Der Context Compiler dedupliziert überlappende Ausschnitte.
7. Jede Context-Einheit trägt Herkunft, Aktualität und Tokenkosten.

## 8. LLM-Integration

1. Provider-Payloads bleiben im Provider-Adapter.
2. Modellfähigkeiten werden über ein ModelProfile beschrieben und nicht aus dem Modellnamen erraten.
3. Werkzeugeingaben, strukturierte Phasenaufträge und Deep-Map-Ausgaben verwenden versionierte
   Schemas und strikte Decoder. Freier Chat benötigt keine Coding-JSON-Fähigkeit;
   Streamvalidierung, Outputlimit, Timeout und Cancellation bleiben erforderlich.
4. Jede Ausgabe wird vor Ausführung validiert.
5. Eine Reparaturanfrage ist maximal einmal zulässig.
6. Temperatur, Kontextlimit, Outputreserve und Stopbedingungen sind Teil eines versionierten Laufprofils.
7. Prompts enthalten keine vollständigen, unkontrollierten Werkzeugausgaben.
8. Der LLM-Aufruf darf keinen eigenen Sicherheitsstatus oder Claim-Faktstatus setzen.
9. Der Core bestimmt pro Aufruf genau einen kleinen Phasenauftrag. Bekannte IDs, Hashes und
   deterministische Zustandskanten bleiben Core-Aufgaben. Plan-/Editphasen dürfen regulär
   begrenztes NeedEvidence oder TooLarge zurückgeben, ohne Mutation oder Strukturrepair.
10. Der vollständige Originalauftrag bleibt unveränderlich im Goal-Store, initial höchstens
    64 KiB UTF-8. Jeder Agentturn erhält eine revisionsgebundene aktive Teilsicht mit seinen
    Kriterien, sämtlichen möglicherweise geltenden Constraints, relevanten offenen Fehlern
    und begrenzter Gesamtabdeckung. Ausgelassene Anforderungen bleiben dauerhaft offen.
11. Mindestens 22 Prozent Kontext oder das höhere konfigurierte Outputlimit bleiben reserviert.
    Tatsächlich gesendetes Schema, Wiederholungen, Protokoll und Thinking zählen mit.
    Nicht passende Pflichtteile führen zu Zerlegung oder erklärtem Kapazitätshalt.
12. Readanzahl ersetzt keine aktuelle Quellabdeckung. Quellenfortschritt wird über tatsächlich
    gelieferte neue relevante Bereiche erfasst; erschöpfte Suche erzwingt keine Änderung.

## 9. Workspace-Werkzeuge

1. Dateizugriff ist auf kanonisierte, genehmigte Roots begrenzt.
2. Symlinks werden nach Auflösung erneut gegen die Root-Grenze geprüft.
3. Patches verwenden erwartete Ausgangshashes, damit konkurrierende Änderungen erkannt werden.
4. Prozesse werden argv-basiert und ohne Shell gestartet.
5. Umgebungsvariablen werden über eine Allowlist weitergegeben.
6. Toolausgaben werden typisiert, größenbegrenzt und mit Trunkierungsmetadaten versehen.
7. Mutationen erzeugen vor dem nächsten Modellturn ein aktualisiertes Change Set.
8. Add- und Move-Patches dürfen fehlende Elternverzeichnisse nur innerhalb der kanonischen Root
   erzeugen; vorhandene Link-/Reparse-Komponenten bleiben gesperrt und Ziele werden nie ersetzt.
9. Modellseitige Bereichsedits binden ausschließlich tatsächlich gelieferte aktuelle Bytes.
   Der Core erhält alle übrigen Bytes und BOM/EOL und kompiliert denselben vollständigen
   PatchUpdate vor der bestehenden Vorschau-, Policy-, Lease-, Hash- und Recoverygrenze.
   Fremde Aliase, Überlappung, stale Bytes und unscharfer Ersatz bleiben ausgeschlossen.

## 10. Frontend und IPC

1. Das Frontend ist ein unprivilegierter Client.
2. Tauri-Commands sind eng, typisiert und use-case-orientiert; generische execute-, sql- oder filesystem-Commands sind verboten.
3. UI-State ist darstellungsbezogen. Fachzustand bleibt im Rust-Kern.
4. IPC-Nachrichten sind versioniert und validiert.
5. Große Datenmengen werden paginiert oder gestreamt.
6. Jeder Eventtyp besitzt eine dokumentierte Reihenfolge- und Wiederverbindungssemantik.
7. Der Core gibt Secrets niemals an die WebView zurück. Eine explizite Credential-Eingabe darf
   nur kurzzeitig in einem unkontrollierten Passwortfeld und einem dedizierten one-way IPC-Request
   existieren; sie wird sofort geleert und besitzt keine lesbare Responseprojektion.
8. Die Agent-Activity-Projektion MUSS ihren Run aus einem durable Ledger-Versuch ableiten. Die
   WebView DARF keine Run- oder Snapshot-ID wählen. Es werden höchstens die letzten 64
   zusammenhängenden, inhaltsfreien Journalereignisse ausgeliefert; Modellantwort beziehungsweise
   Aktionsauswahl und tatsächliches `ToolAction` MÜSSEN unterschiedliche sichtbare Typen bleiben.
9. Agent-Recovery MUSS Task-gebunden bleiben: Die WebView darf weder Run, Snapshot, Step, Event-ID
   noch Zeitpunkt wählen. Resume, Replan und Cancel MÜSSEN den bestehenden H11/E8-Commit mit
   Published-Snapshot-, Ledger- und Run-CAS verwenden. Resume DARF stale Evidence oder eine
   unbekannte Mutationswirkung niemals umgehen; Cancel MUSS weiterhin erreichbar bleiben.
10. Solange der aktuelle Prozess einen Agent-Worker in `Queued`, `Running`, `Pausing` oder
    `Cancelling` besitzt, DARF Recovery denselben Task nicht als verlassen inspizieren. Die
    WebView erhält nur eine content-freie Managerprojektion. Pause und Cancel stoppen zuerst den
    Worker; Cancel verwendet anschließend weiterhin die exakt sichtbaren Ledgeranker.
11. Approval-Control MUSS Task-, Presentation- und Ledger-gebunden sein. Die WebView DARF keine
    Request-, Grant-, Run-, Snapshot-, Step-, Process-, Policy-, Event-ID oder Zeit als Autorität
    liefern. AllowOnce DARF keinen Agent-Versuch starten; Continue MUSS die interne exakte
    One-time-Grant-ID im Core auswählen. Deny MUSS den wartenden Step und Run atomar ohne
    Toolwirkung schließen. Env-Werte und breite wiederverwendbare Freigaben DÜRFEN die Grenze
    nicht überschreiten.
12. Die initiale WebView MUSS große, nicht unmittelbar sichtbare Produktflächen als lokale
    Lazy-Chunks laden. Lade-, Fehler- und Wiederholungszustände bleiben zugänglich; Lazy Loading
    DARF keine fachliche Autorität oder zusätzliche privilegierte Capability erzeugen.
13. Cursorpaginierte Baumprojektionen DÜRFEN im Frontend höchstens eine validierte Seite behalten.
    Große gleichförmige Zeilenprojektionen MÜSSEN mit konstanter DOM-Obergrenze gefenstert werden;
    serverseitige Payload-, Graph- und Evidence-Grenzen bleiben unabhängig davon verbindlich.
14. Poll- und Renderbursts MÜSSEN einen App-Mount-gebundenen Scheduler mit nicht überlappenden
    Polls, genau einem vorgemerkten Rerun und Latest-Wins-Commit pro Animationsframe verwenden.
    Projektbezogene Commits tragen eine monotone Generation. Projektwechsel und Unmount MÜSSEN
    alte Commitbuffer, Listener, Timer und Beobachter freigeben.
15. Frontend-Performanceclaims benötigen getrennte deterministische Verträge, einen realen
    Browserprofil-Lauf und eine native App-Prozessmessung. Synthetische DOM-Zeit allein ist kein
    Beleg für Interaktionslatenz oder Idle-RAM.
16. Die Modi Chat, Ask und Agent sind Core-erzwungene Capability-Envelopes. Chat arbeitet ohne
    Projekt und ohne Projektwerkzeuge; Ask liest ausschließlich das gebundene Projekt und besitzt
    keine Mutation oder allgemeinen Prozess-/Netzwerktools. Modusauswahl gilt nur für das nächste
    unabhängige Work Item und DARF keinen laufenden Besitzer umschalten. Folgenachrichten bleiben
    begrenzt, FIFO-geordnet und vor Persistenz sowie Start Core-validiert. Frühere Ausführungsanker
    werden nicht übernommen. Alte Entwicklungs-Plan-Sessions müssen gemäß ADR-0119 nicht im neuen
    Produkt lesbar bleiben; alte Freigaben erzeugen keine neue Ausführungsautorität.
17. Agentplanung MUSS strukturierte kleine Umsetzungsschritte an Originalanforderungen und passende
    Verifikation binden. Initial gelten höchstens fünf Ergebnisgruppen und drei konkret vorbereitete
    Schritte; nur der nächste Schritt erhält ausführbare Ressourcen. Neue Evidence darf über
    append-only Ledgerrevisionen weitere Schritte einfügen oder offene Schritte ersetzen.
    Replans ändern den Arbeitszuschnitt oder die Evidenzpflichten tatsächlich und erhalten die
    Abdeckung sowie abgeschlossene History. Eine Bitte, zunächst nur zu planen, bleibt ein
    Mutationsverbot bis zur ausdrücklichen Umsetzungsauswahl. Automatische Replans bleiben endlich;
    genau eine Mutation darf den Worktree besitzen.
18. Quellengebundene Antworten müssen aktuelle Belege und offene Grenzen unterscheiden. Eine
    ausdrücklich unvollständige Evidenzlage wird innerhalb des verbleibenden Budgets gezielt
    weiterbearbeitet oder als begründeter Halt/Teilstand gezeigt, niemals als richtige vollständige
    Antwort. Aktuelle explizite Indexziele bilden eine Core-geprüfte Mindestabdeckung; ein leerer
    veröffentlichter Index ist eine abgeschlossene Bestandsaufnahme. Transiente Read-/Modell-Retries
    bleiben innerhalb bestehender Entscheidungs-, Zeit- und Aktionsgrenzen und dürfen Cancellation,
    Source-, Policy- oder Capability-Ablehnung sowie den Einzelrepair nicht umgehen.
19. Der flüchtige Conversation-Recherchekontext MUSS aktuelle explizite Repositoryziele vor
    allgemeiner Task-Lens-Auswahl und historischem Sessionwissen priorisieren. Spätere adaptive
    Reads MÜSSEN nutzbaren Kontextplatz erhalten. Revalidierte alte Evidence darf eine aktuelle
    eindeutige Zielquelle weder verdrängen noch deren erneuten sicheren Read ersetzen.

20. Appweite Agent-Berechtigungen MÜSSEN als typisierter Modus mit append-only Revision im
    globalen Katalog liegen. Die WebView darf ausschließlich versionierte Query-/CAS-Update-
    Commands nutzen. Eine Modellaktion darf weder den Modus wählen noch eine Einmalfreigabe
    simulieren. Automatische Entscheidungen tragen die aktuelle Revision; Werkzeugadmission
    revalidiert sie. Wartende nun erlaubte Aktionen werden gegen aktuelle Anker neu bewertet.
    Strengere Workspace-Regeln, Capability-Grenzen und unbekannte/destruktive/veröffentlichende
    Wirkungen bleiben verbindlich. Siehe [ADR-0118](adrs/0118-appweite-agent-berechtigungsmodi.md).
21. Neue Arbeitspläne dürfen ausschließlich Umsetzungsschritte besitzen. Die Verifikations-
    entscheidung unterscheidet vorhandene Prüfungen, kurze Funktionskontrollen und begründete
    Testerstellung. Prüfabsichten dürfen keine Testartefakte implizieren; ein fehlender Command
    erzwingt kein Scaffolding. Diff-Evidence darf keine Laufzeitbehauptung verifizieren.
    Siehe [ADR-0117](adrs/0117-auftragsbezogene-tests-und-verifikation.md).
22. Nach Abschluss des aktuellen Planungshorizonts führen offene globale Kriterien im selben Run
    über dauerhafte Ledger-/Journal-CAS von Verify zu Plan. Verbrauch und ausstehende Planung bleiben
    crashsicher; kein neuer Run und kein Budgetreset. Done verlangt vollständige Originalabdeckung
    und passende aktuelle Ergebnisnachweise. Ein externer Evaluator ersetzt kein produktives Rezept.

## 11. Tests

1. Domain-Invarianten benötigen Unit- und gegebenenfalls Property-Tests.
2. Jeder Port mit mehreren Adaptern erhält eine gemeinsame Contract-Suite.
3. Parser und Indexer benötigen Golden Fixtures.
4. Persistenzmigrationen werden von leerer DB und von jeder unterstützten Vorgängerversion getestet.
5. Fehlerbehebungen benötigen Regressionstests.
6. Zeit-, Parallelitäts- und Cancellation-Verhalten wird deterministisch getestet.
7. Plattformcode benötigt mindestens einen Smoke-Test auf jeder betroffenen Plattform.

## 12. Wartbarkeit

1. Öffentliche APIs bleiben klein und dokumentiert.
2. Sichtbarkeit ist so eng wie möglich.
3. Duplizierung darf lokal bestehen, bis die gemeinsame Abstraktion fachlich klar ist.
4. Kommentare erklären Gründe, Invarianten und Risiken, nicht offensichtlichen Syntaxablauf.
5. Abhängigkeiten werden mit minimalen Features eingebunden.
6. Generierter Code liegt klar getrennt und wird nicht manuell editiert.
7. Veraltete Feature Flags, Migrationspfade und Kompatibilitätsschichten erhalten ein Entfernungskriterium.

## 13. Review-Check

Vor Abschluss MUSS Codex beantworten können:

- Welche Verantwortung hat jede geänderte Einheit?
- Welche Architekturgrenze wurde berührt?
- Welche Invariante schützt der Code?
- Welche Evidenz beweist die Korrektheit?
- Wie reagiert der Code auf Abbruch, Fehler und konkurrierende Änderung?
- Welche Daten oder Berechtigungen kann ein kompromittiertes Frontend erreichen?
- Welche Messung stützt eine Performanceaussage?
- Welche Dokumentation oder ADR wurde aktualisiert?
- Der chatbasierte Agent Workspace ist ausschließlich eine begrenzte Präsentationsprojektion.
  Sessiontext, Modus und UI-Status ersetzen niemals Goal Contract, Task Ledger, Agent Run,
  Journal, Evidence, Policy oder Verification.
- `Chat`, `Ask` und `Agent` sind Core-erzwungene Capability-Envelopes. Eine Beschriftung oder
  deaktivierte Schaltfläche im Frontend gilt nicht als Sicherheitsgrenze.
- Planung ist interne Agentenarbeit; Umsetzung bindet die aktuelle Goal-/Ledgerrevision und den
  tatsächlichen Projektscope. Ein Plan-only-Constraint wird ausdrücklich aufgehoben, bevor Mutation
  zulässig ist. Pfade, Befehle, Approval-IDs und Providerdaten werden nicht aus freiem Chattext
  als Autorität übernommen. Legacy-Planfreigaben werden niemals automatisch migriert.
- Slash Commands sind geschlossene Core-Arbeitsprofile. Sie DÜRFEN Recherchefokus, Ergebnisform
  und Verification präzisieren, aber weder Modusgrenzen noch Tool-, Prozess-, Netzwerk-, Datei-
  oder Mutationsberechtigungen erweitern. Die WebView ist für Parsing und Modusprüfung niemals
  Autorität.
- Diagramme MÜSSEN aus einem typisierten, source-gebundenen Modellobjekt deterministisch kompiliert
  werden. Rohe Modell-Mermaid und von der WebView gewählte Exportpfade DÜRFEN keine privilegierte
  Grenze passieren.
