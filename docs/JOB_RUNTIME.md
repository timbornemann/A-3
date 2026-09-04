# A^3 Job-Laufzeit

Status: verbindliche Foundation-Baseline

Stand: 2026-08-04

## Zweck und Grenze

Die Job-Laufzeit führt lange, lokale Application-Aufgaben mit begrenzten Ressourcen aus. Die fachlichen Typen `JobId`, `JobOwner`, `JobStatus` und `Progress` liegen in `a3-domain`. Scheduling, Cancellation, Uhr-Port und Ereignisstrom liegen in `a3-application`. Der Tauri Composition Root besitzt die konkrete Laufzeit und stellt die Systemuhr bereit. Die WebView erhält weder Worker-Handles noch direkten Zugriff auf den Scheduler.

F4 umfasst noch keine Persistenz, Priorisierung, IPC-Freigabe oder Agentenlogik. IDs werden von der aufrufenden Application-Funktion vergeben und müssen innerhalb der Lebensdauer eines Schedulers eindeutig sein.

## Zustandsmodell

~~~mermaid
stateDiagram-v2
    [*] --> Queued
    Queued --> Running
    Queued --> Cancelling
    Running --> Cancelling
    Running --> Succeeded
    Running --> Failed
    Cancelling --> Cancelled
    Succeeded --> [*]
    Failed --> [*]
    Cancelled --> [*]
~~~

`Succeeded`, `Failed` und `Cancelled` sind terminal. Eine Cancellation ist kooperativ: Die Laufzeit setzt den Token und den Status synchron auf `Cancelling`; die Aufgabe muss den Token beobachten und kontrolliert zurückkehren. Eine bereits wartende Aufgabe wird nach einer Cancellation nicht mehr ausgeführt.

## Begrenzung und Backpressure

Jede Scheduler-Instanz besitzt drei validierte, von null verschiedene Grenzen:

- Anzahl eigener Worker;
- Kapazität der wartenden Queue;
- Kapazität des Ereigniskanals.

Die Foundation-Konfiguration im Desktop verwendet zwei Worker, 32 wartende Aufgaben und 256 gepufferte Ereignisse. Eine volle Queue oder ein voller Ereigniskanal wird als typisierter Fehler sichtbar. Fortschritt wird erst als neuer Snapshot-Zustand übernommen, wenn sein Ereignis in den Kanal geschrieben werden konnte. Nicht zustellbare Lifecycle-Ereignisse erhöhen `undelivered_event_count`; der Verlust bleibt damit beobachtbar.

Der begrenzte Ereigniskanal verwendet `crossbeam-channel`, weil der Scheduler neben dem öffentlichen Consumer einen eigenen Consumer für den abschließenden Shutdown-Drain behalten muss; der Standardbibliotheks-Receiver ist nicht teilbar. Die Abhängigkeit bleibt auf `a3-application` begrenzt und wird nicht Teil der Domain.

## Fortschritt und Ereignisse

Fortschritt ist entweder `Indeterminate` oder `Determinate { completed, total }`. Bei bestimmtem Fortschritt ist `total` größer als null, `completed` überschreitet `total` nicht, das Total bleibt während eines Jobs fest und `completed` läuft nicht rückwärts. Nach bestimmtem Fortschritt darf ein Job nicht wieder unbestimmt werden.

Der Scheduler emittiert typisierte Ereignisse für `Queued`, `Started`, `Progressed`, `CancellationRequested`, `Succeeded`, `Failed` und `Cancelled`. Jedes Ereignis trägt Job, Owner, injizierten monotonen Zeitstempel und eine pro Job streng steigende Sequenznummer. Sequenzlücken und der Snapshot-Zähler machen nicht zugestellte Lifecycle-Ereignisse erkennbar.

## Ownership und Shutdown

Der Desktop-Fast-Index konkretisiert den determinierten Fortschritt auf sechs feste Phasengrenzen:
`Discover`, `Hash`, `Parse`, `Link`, `Rank` und `Publish`. Der besitzende Koordinator projiziert nur
Lifecycle, aktuelle Phase und `completed/6` in einen kleinen Mutex-geschützten Read-State.
`query_index_activity` liest ausschließlich diesen Zustand; Polling rekonstruiert weder den Index,
misst Storage noch liest es Repositorydateien. Der Scheduler bleibt alleiniger Besitzer des Jobs.

Der Deep-Map-Produktzustand ergänzt darüber bewusst `Pausing` und `Paused`, ohne den verbindlichen
Scheduler-Automaten um einen nicht terminalen Pausezustand zu erweitern. `pause_deep_map` ist nur
für einen tatsächlich laufenden Versuch zulässig und fordert kooperative Cancellation an. Erst
nach dem terminalen Schedulerstatus `Cancelled` und einem gegen den unveränderten `ExplorePlan`
validierten `ExplorerCheckpoint` wird der Produktzustand `Paused`; dann läuft keine Modellarbeit.
`resume_deep_map` erzeugt einen neuen Scheduler-eigenen Versuch mit demselben Startbudget und setzt
am ersten unbestätigten Planschritt fort. Cancel verwirft den Checkpoint. Projektwechsel,
Projektentfernung und Shutdown fordern ebenfalls Cancellation an und lassen keinen detached Worker
zurück.

Der Agent Workspace verwendet nach ADR-0020 dieselbe Trennung. Ein begrenzter
`AgentRunManager` im Desktop-Composition-Root besitzt die Scheduler-Jobs und projiziert
`Idle`, `Queued`, `Running`, `Pausing`, `Paused`, `Cancelling`, `Succeeded`, `Failed` oder
`Cancelled`; diese Projektion ist weder der persistente Agent-Controller noch eine neue
Wahrheitsquelle. Polling und WebView-Mount erzeugen keine Arbeit. Während ein Worker lebt, liefert
Recovery ausschließlich Task-, Ledger- und Produktzustand aus einem read-only Core-Read und führt
keine H11-Neustartinspektion aus.

Ask-, Plan- und Agent-Vorbereitung besitzen je Job genau eine feste, monotone äußere
Fortschrittsskala. Wiederholte Task-Lens-Läufe übernehmen Cancellation, melden ihre pro Lauf neu
beginnenden Phasen aber nicht in die äußere Scheduler-Skala; Indexmaterialisierung meldet dort nur
Cancellation. Ein produktiver Agentenversuch
besitzt entsprechend ausschließlich die monotone Turn-Skala. Untergeordnete Kontext-, Index-,
Patch- und Prozessoperationen dürfen diese Scheduler-Skala weder ersetzen noch auf einen kleineren
Wert zurücksetzen. Nach Annahme einer Nachricht wird jede laufende Session dauerhaft in
`Completed`, `AwaitingUser`, `AwaitingPlanReview`, `Failed` oder `Cancelled` überführt. Das gilt
auch bei einem Workerfehler, einer vollen Queue und einem Abbruch ohne noch sichtbaren Job-Snapshot;
ein konkurrierender Abschluss wird über die Session-Revision idempotent aufgelöst.

Ask, Plan und Agent-Vorbereitung verwenden innerhalb des Conversationjobs denselben endlichen
Read-only-Controller. `Standard` erlaubt höchstens 6 Modellentscheidungen, 12 Read-Aktionen und 5
Minuten; `Gründlich` höchstens 12 Entscheidungen, 24 Reads und 15 Minuten. Pro Entscheidung werden
ein bis vier Aktionen sequenziell ausgeführt. Identische Aktionen werden belastet, aber nicht
erneut ausgeführt; zwei Runden ohne neue Evidence enden in `AwaitingContinuation`. Die letzte
Entscheidung darf keine weitere Suche anfordern, und über den ganzen Abschnitt existiert genau ein
Schema-Reparaturversuch. Jeder Fortschritt und jede öffentliche Notiz wird sofort als V31-Event
gespeichert; Cancellation, Timeout oder Fehler erhalten bereits gefundene Source-Metadaten. Der
erfolgreiche Abschluss committet Ergebnis, Zitate, terminales Event und Sessionrevision atomar.
Kein Worker wird detached und kein UI-Poll startet neue Arbeit.

ADR-0043 ergänzt innerhalb dieses unveränderten Rahmens ein begrenztes Recovery-Verhalten.
Vorübergehend nicht verfügbare Source-Reads und Quelltextsuchen werden einmal am selben Schritt
wiederholt, insgesamt höchstens viermal je Rechercheabschnitt. Bleibt der Read erfolglos, wird das
begrenzte Ergebnis der nächsten Entscheidung übergeben, damit ein anderer Read-only-Suchweg
gewählt werden kann. Ein transienter Modellfehler darf genau einmal erneut versucht werden und
verbraucht dabei eine reguläre Modellentscheidung. Cancellation, Zeitablauf, Schema-Reparatur,
Stagnation und die festen Profilgrenzen werden dadurch nicht erweitert.

ADR-0044 ändert weder Scheduler- noch Profilgrenzen. Vor der ersten Modellentscheidung werden
explizit genannte aktuelle Dateien vorrangig gelesen, danach höchstens zwölf aktuelle Task-Lens-
Ziele und zuletzt höchstens acht revalidierte historische Quellen. Enthält eine spätere
`searchIndex`-Aktion einen eindeutigen Dateinamen oder ein eindeutiges Pfadsuffix, führt der Core
den sicheren Dateiread vor der erneuten Lens-Kompilierung aus. Eine Nullrunde erhält einen
verpflichtenden Wechselhinweis; nur zwei aufeinanderfolgende Runden ohne neue Evidence führen
weiterhin zu `AwaitingContinuation`. Runden mit Erkenntnisgewinn können das vollständige feste
Profil nutzen.

Slash-Command-Nachrichten verwenden dieselben besessenen Conversationjobs und die festen
ADR-0038-Budgets. Die zusätzlichen Analyseaktionen zählen wie bestehende Reads; ein Command
erzeugt keinen zweiten Scheduler und keinen eigenen offenen Loop. `/diagram` reserviert innerhalb
des Profils die letzte verfügbare Modellentscheidung für die typisierte Artefaktformatierung,
sodass die letzte Entscheidung weiterhin keine neue Suche anfordern kann. Antwort, Zitate,
Diagramme, terminales Event und Sessionrevision werden gemeinsam abgeschlossen; Cancellation oder
Fehler exportieren kein unvollständiges Artefakt.

Ein zielpflichtiger Command ohne Ziel schließt seinen kurzen besessenen Job deterministisch mit
`AwaitingUser` ab, ohne Recherchemodell, Tool oder Agent-Run zu öffnen. Die direkte Nutzerantwort
wird nur dann als Command-Fortsetzung übernommen, wenn sie auf genau diesen Rückfrage-Turn folgt;
der Core rekonstruiert Hauptauftrag und Linsen aus der append-only V32-Projektion, validiert sie
erneut und startet anschließend einen gewöhnlichen budgetierten Conversationjob.

Mutierende Command-Pläne materialisieren bestätigte, eigenständig änderbare Top-Level-Punkte als
sequenzielle Ledger-Schritte. Jeder besitzt eine eigene Verification-Spezifikation. Nach einem
erfolgreich verifizierten Schritt committet `ContinueVerifiedAgentPlan` den nächsten
`Ready`-Schritt und den Übergang `Verify → Execute` in derselben Ledger-/Run-CAS-Transaktion. Es
läuft weiterhin höchstens eine mutierende Aktion im Worktree; erst ohne verbleibenden bereiten oder
aktiven Schritt beginnt die Acceptance-Prüfung.

ADR-0042 wendet diese sequenzielle Materialisierung auf jeden bestätigten Agent-Arbeitsplan an.
Wechselt der produktive Controller nach einem neuen Befund in `Replan`, bleibt derselbe
Schedulerjob Besitzer: Er schließt den aktiven Versuch, committet genau eine neue Ledgerrevision,
durchläuft erneut `Localize → Plan` und startet dann den ersten neuen `Ready`-Schritt. Flüchtige
Read-Ergebnisse werden vor diesem Neustart verworfen. Höchstens acht automatische Replans und das
bestehende 64-Turn-Limit verhindern einen offenen Loop. Ein echter Richtungsblocker beendet den
Worker erfolgreich am menschlichen Haltepunkt `AwaitingUser`; ein interner Executorfehler wird
weiterhin als `Failed` projiziert.

ADR-0041 ergänzt vor dem Conversation-Scheduler eine dauerhafte FIFO-Projektion. Während ein
Conversationjob den Slot besitzt, validiert der Core weitere Nachrichten vollständig und
persistiert sie, ohne den Besitzer zu unterbrechen. Ein erfolgreicher terminaler Abschluss gibt
den Slot frei und stößt genau den ältesten fortsetzbaren Eintrag an. Rückfrage, Planreview oder
Approval bleiben menschliche Haltepunkte; Fehler und Abbruch pausieren die Queue. Nach einem
Prozessneustart wird
eine gefundene Queue ebenfalls pausiert sichtbar gemacht und erst nach expliziter Fortsetzung in
den flüchtigen Dispatchsatz aufgenommen. Ein beim Dispatch fehlgeschlagener Claim wird append-only
zurück auf `queued` gesetzt und pausiert, statt die Nachricht zu verlieren.

Pause ist nur für `Running` zulässig. Sie fordert Scheduler-Cancellation an; `Paused` folgt erst
auf den terminalen Schedulerstatus `Cancelled`, die Executor-Rückgabe `Cancelled` und eine
erfolgreiche H11/E8-Inspektion eines weiterhin nichtterminalen Runs. Resume beziehungsweise
Replan committen zuerst den bestehenden Snapshot-/Ledger-/Run-CAS und reichen danach einen neuen
besessenen Versuch mit einer strikt neueren Ledger-Store-Version ein. Cancel stoppt zuerst den
Worker und committed anschließend H11 gegen die zuvor sichtbaren Ledgeranker. Projektwechsel,
Entfernung und Composition-Root-Shutdown quieszen die besessene Arbeit; ohne verifizierte
Agent-Executor-Capability wird kein Manager und keine Modellarbeit erzeugt.

Der Scheduler besitzt jeden Worker-Thread und akzeptiert nach Beginn des Shutdowns keine Arbeit mehr. `Drain` beendet die Queue kontrolliert und wartet anschließend auf alle Worker. `CancelAndWait` fordert zusätzlich für alle aktiven Jobs Cancellation an und wartet ebenfalls auf alle Worker. Der Destruktor verwendet als Sicherheitsnetz `CancelAndWait`; es gibt keinen detached Worker-Pfad. Der Desktop führt die asynchronen Futures seiner Index-, Deep-Map- und Agent-Jobs innerhalb der Tauri-eigenen Tokio-Laufzeit aus. Damit besitzen auch scheduler-eigene Worker den Reactor-Kontext, den HTTP-Provider benötigen; ein allgemeiner Future-Executor ohne diesen Kontext ist an dieser Composition-Root-Grenze unzulässig.

Der Shutdown-Report enthält die Anzahl gejointer Worker, geordnete finale Job-Snapshots und noch nicht konsumierte Ereignisse. Ein Panic innerhalb einer Aufgabe wird an einer Laufzeitgrenze abgefangen und als `Failed` abgeschlossen; ein Panic außerhalb dieser Grenze wird als Shutdown-Fehler gemeldet.

## Zeit und deterministische Prüfung

`a3-application` hängt ausschließlich vom `JobClock`-Port ab. Der Desktop injiziert eine monotone, auf `Instant` basierende Systemuhr. Tests verwenden eine atomar steuerbare `FakeClock`, damit Ereignisreihenfolge und Zeitstempel unabhängig von der Wanduhr reproduzierbar bleiben.

## Optionaler Embedding-Batchjob

`SemanticEmbeddingBatchJob` ist ein Scheduler-eigener, lokaler und regenerierbarer Job. Im
Defaultzustand `GenerateSemanticEmbeddings::disabled()` existiert weder eine Provider- noch eine
Cache-Capability. Der aktivierte Zustand besitzt beide Ports zusammen mit einer injizierten
Creation-Clock und einem positiven, höchstens zweiminütigen Provider-Requesttimeout.

Ein Job verarbeitet höchstens 512 kanonische Cards desselben Snapshots. Cachelookup verwendet nur
den exakten Card-/Profil-/BodyHash-Schlüssel; Misses werden in Providerbatches von höchstens 64
Karten verarbeitet. Cancellation wird vor Cachezugriff, vor und nach jedem Providerrequest sowie
vor dem persistierenden Batchcommit beobachtet. Progress zählt bereits vorhandene und erfolgreich
persistierte Cards gegen ein unveränderliches Total. Falsche Ergebnisanzahl, Dimension, NaN,
Unendlichkeit oder Nullvektor beendet den Job vor Storage als `Failed`; kooperative Cancellation
endet als `Cancelled`. Kein Worker wird detached.

Der libSQL-Cache führt Lookup und Suche in höchstens zwei Sekunden aus; persistierende Batches und
der ausschließlich regenerierbare Semantic-Rebuild besitzen eine Fünf-Minuten-Obergrenze. Jede
Zeile beziehungsweise jeder Delete-Batch beobachtet Cancellation. Store-Batches werden atomar
zurückgerollt. Der Rebuild löscht ausschließlich regenerierbare Tabellen in referenziell sicherer
Reihenfolge und einzeln committeten 4.096-Zeilen-Batches; dadurch bleibt auch ein abgebrochener
Lauf gültig und kann idempotent fortgesetzt werden. `SemanticCacheRebuildControl` erhält
deterministischen Row-Progress. Weder Snapshot- noch deterministische Retrievalprojektionen werden
verändert.
