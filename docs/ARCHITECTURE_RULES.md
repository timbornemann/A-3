# Architektur- und Code-Regeln

Status: verbindlich  
Stand: 2026-08-03

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
3. Werkzeugauswahl und Deep-Map-Ausgaben verwenden versionierte JSON-Schemas.
4. Jede Ausgabe wird vor Ausführung validiert.
5. Eine Reparaturanfrage ist maximal einmal zulässig.
6. Temperatur, Kontextlimit, Outputreserve und Stopbedingungen sind Teil eines versionierten Laufprofils.
7. Prompts enthalten keine vollständigen, unkontrollierten Werkzeugausgaben.
8. Der LLM-Aufruf darf keinen eigenen Sicherheitsstatus oder Claim-Faktstatus setzen.

## 9. Workspace-Werkzeuge

1. Dateizugriff ist auf kanonisierte, genehmigte Roots begrenzt.
2. Symlinks werden nach Auflösung erneut gegen die Root-Grenze geprüft.
3. Patches verwenden erwartete Ausgangshashes, damit konkurrierende Änderungen erkannt werden.
4. Prozesse werden argv-basiert und ohne Shell gestartet.
5. Umgebungsvariablen werden über eine Allowlist weitergegeben.
6. Toolausgaben werden typisiert, größenbegrenzt und mit Trunkierungsmetadaten versehen.
7. Mutationen erzeugen vor dem nächsten Modellturn ein aktualisiertes Change Set.

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
16. Die Auswahl eines Conversation-Modus gilt nur für das nächste unabhängige Work Item und DARF
    keinen laufenden Besitzer umschalten. Dauerhafte Folgenachrichten MÜSSEN begrenzt, FIFO-geordnet
    und vor Persistenz sowie Start Core-validiert sein. Ein Wechsel aus Agent zu Ask oder Plan
    verwirft die Ausführbarkeit des früheren Plans; spätere Mutation benötigt eine neue exakte
    Planfreigabe.
17. Ein bestätigter Conversation-Plan MUSS vor der Task-Materialisierung in einen begrenzten,
    Core-validierten Arbeitsplan mit einzeln verifizierbaren Schritten übersetzt werden. Neue
    Evidence darf ausschließlich über eine append-only Ledger-Revision weitere Schritte
    einfügen oder offene Schritte ersetzen; abgeschlossene Schritte werden weder umgeschrieben
    noch stillschweigend entwertet. Automatische Replans bleiben endlich und genau eine Mutation
    darf weiterhin den Worktree besitzen.
18. Eine Conversation-Antwort DARF bei einer ausdrücklich als unvollständig ausgewiesenen
    Evidence-Lage nicht abgeschlossen werden, solange das feste Rechercheprofil einen weiteren
    Schritt zulässt. Eindeutig genannte aktuelle Indexdateien bilden eine Core-geprüfte
    Mindestabdeckung. Transiente Read- und Modell-Retries MÜSSEN innerhalb der bestehenden
    Entscheidungs-, Zeit- und Aktionsgrenzen bleiben; sie dürfen weder Cancellation noch eine
    Source-, Policy- oder Capability-Ablehnung umgehen.
19. Der flüchtige Conversation-Recherchekontext MUSS aktuelle explizite Repositoryziele vor
    allgemeiner Task-Lens-Auswahl und historischem Sessionwissen priorisieren. Spätere adaptive
    Reads MÜSSEN nutzbaren Kontextplatz erhalten. Revalidierte alte Evidence darf eine aktuelle
    eindeutige Zielquelle weder verdrängen noch deren erneuten sicheren Read ersetzen.

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
- `Ask`, `Plan` und `Agent` sind Core-erzwungene Capability-Envelopes. Eine Beschriftung oder
  deaktivierte Schaltfläche im Frontend gilt nicht als Sicherheitsgrenze.
- Eine Planumsetzung bindet immer die exakte sichtbare Session- und Planrevision; Pfade, Befehle,
  Approval-IDs und Providerdaten werden nicht aus freiem Chattext übernommen.
- Slash Commands sind geschlossene Core-Arbeitsprofile. Sie DÜRFEN Recherchefokus, Ergebnisform
  und Verification präzisieren, aber weder Modusgrenzen noch Tool-, Prozess-, Netzwerk-, Datei-
  oder Mutationsberechtigungen erweitern. Die WebView ist für Parsing und Modusprüfung niemals
  Autorität.
- Diagramme MÜSSEN aus einem typisierten, source-gebundenen Modellobjekt deterministisch kompiliert
  werden. Rohe Modell-Mermaid und von der WebView gewählte Exportpfade DÜRFEN keine privilegierte
  Grenze passieren.
