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

Der Scheduler besitzt jeden Worker-Thread und akzeptiert nach Beginn des Shutdowns keine Arbeit mehr. `Drain` beendet die Queue kontrolliert und wartet anschließend auf alle Worker. `CancelAndWait` fordert zusätzlich für alle aktiven Jobs Cancellation an und wartet ebenfalls auf alle Worker. Der Destruktor verwendet als Sicherheitsnetz `CancelAndWait`; es gibt keinen detached Worker-Pfad.

Der Shutdown-Report enthält die Anzahl gejointer Worker, geordnete finale Job-Snapshots und noch nicht konsumierte Ereignisse. Ein Panic innerhalb einer Aufgabe wird an einer Laufzeitgrenze abgefangen und als `Failed` abgeschlossen; ein Panic außerhalb dieser Grenze wird als Shutdown-Fehler gemeldet.

## Zeit und deterministische Prüfung

`a3-application` hängt ausschließlich vom `JobClock`-Port ab. Der Desktop injiziert eine monotone, auf `Instant` basierende Systemuhr. Tests verwenden eine atomar steuerbare `FakeClock`, damit Ereignisreihenfolge und Zeitstempel unabhängig von der Wanduhr reproduzierbar bleiben.
