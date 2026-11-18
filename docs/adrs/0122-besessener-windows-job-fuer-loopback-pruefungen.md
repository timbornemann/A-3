# ADR-0122 Besessener Windows-Job für Loopback-Prüfungen

Status: Accepted  
Datum: 2026-11-09  
Entscheider: Tim Bornemann

Annahme durch Tim Bornemann am 2026-11-09.

## Kontext

[Plan 22/P7](../plans/22-SMALL-LOCAL-MODEL-AGENT-CORE.md#p7-greenfield-mehrere-module-und-große-bestehende-projekte)
verlangt einen Produktbeleg aus einem vollständigen Start-/HTTP-/Stop-Rezept.
Der aktuelle Windows-Adapter beobachtet das Ende des direkten Kindes und
die Abwesenheit des Listeners. `command-group` 5.0.1 kapselt den Job-Handle.
Sein Windows-Wartepfad kann ein Completion-Paket auswerten, ohne die Zahl
der noch aktiven Jobprozesse festzustellen. Ein Nachkomme kann außerhalb
des geprüften Ports weiterlaufen. Der Startpfad erzeugt das suspendierte
Kind vor der Job-Zuordnung. Der Systemhelfer für die TCP-Tabelle nutzt
denselben Startpfad.

## Entscheidung

- Das geschlossene Loopback-Rezept erhält einen privaten, unbenannten
  Windows-Job mit Kill-on-Close und einem nicht vererbbaren Handle.
  Beide Breakaway-Flags bleiben aus. Der
  Adapter übergibt den Job über `PROC_THREAD_ATTRIBUTE_JOB_LIST` bei
  `CreateProcessW`. Die Zuordnung ist Teil der Prozesserzeugung. Der
  bereits geprüfte absolute Programmname, die exakten Argumente, das
  Arbeitsverzeichnis und die reduzierte Umgebung kommen aus dem
  vorhandenen Prozessvertrag. Übergebene Standard-Handles werden
  explizit begrenzt. Job- und Prozess-Handles werden besessen und
  geschlossen. [Erzeugungsattribute][attributes] · [Job Objects][jobs]
- Der Adapter hält den Handle des direkten Kindes bei der
  Eigentumsprüfung von Listener und serverseitigem Ende desselben
  gehaltenen Sockets. Er prüft die Lebendigkeit jeweils vor und nach
  der Zuordnung. Der Stop fordert die Terminierung des privaten Jobs an,
  beobachtet das signalisierte Prozessende, gibt die Referenzen auf
  das Kind frei und prüft unter der vorhandenen Cleanup-Frist den Job
  bis `ActiveProcesses == 0`. Der gebundene Listener muss ebenfalls
  verschwunden sein. Ein unbestimmter Start oder ein unbelegtes Ende
  führt zu `Unknown`, ohne Erfolgsbeleg oder automatische Wiederholung.
  [Job-Accounting][accounting] · [Job-Terminierung][termination]
- Das Modul liest die lokale numerische TCP-Owner-Tabelle über
  `GetExtendedTcpTable`. Die Prüfung erfasst IPv4 und IPv6 für
  konkurrierende Listener, einschließlich Wildcard und IPv4-Mapping,
  und die exakte IPv4-Gegenstelle für die gehaltene Verbindung.
  Puffergröße, Zahl der Abfragen und Auswertung sind begrenzt. Der
  Adapter prüft die Frist vor und nach dem Betriebssystemaufruf.
  Eine unvollständige oder mehrdeutige Tabelle lässt den Nachweis offen.
  Damit entfällt der separate `netstat.exe`-Prozess. [TCP-Owner-Tabelle][tcp]
- Ein einziges benanntes Windows-FFI-Modul in `a3-workspace` kapselt die
  Aufrufe und ihre Speicher- und Handle-Invarianten über das bereits
  im Lockfile gebundene `windows-sys` 0.61.2. Für `a3-workspace` gilt
  `unsafe_code = "deny"`, mit `allow` ausschließlich am benannten
  Windows-Modul. Die übrigen Crates behalten das Workspace-`forbid`. Jeder
  Unsafe-Block erhält einen lokalen Safety-Kommentar. Die öffentliche
  Adaptergrenze bleibt sicher und liefert geschlossene Dispositionen.
- Die genaue Freigabe nach [ADR-0118](0118-appweite-agent-berechtigungsmodi.md),
  der Worktree-Mutationslease, der vorab gespeicherte Journalversuch und
  der begrenzte Evidence-Vertrag gelten auch für diesen Start.
  Entsprechend [ADR-0119](0119-kleinschrittiger-agentenkern-fuer-lokale-modelle.md)
  nutzt bis P11 nur der isolierte Kandidat den neuen Owner.

## Konsequenzen

Der Windows-Adapter kann ein beobachtetes Ende seines gesamten Jobs
vom Tod des direkten Kindes unterscheiden. Die Zuordnung bei der
Erzeugung benötigt Windows 10 beziehungsweise Windows Server 2016
oder neuer. Fehlt die Fähigkeit oder lässt die äußere Job-Hierarchie
die Bindung nicht zu, bleibt der betroffene Nachweis offen. Der Job
begrenzt den Prozesslebenszyklus. Dateisystem- und Netzwerkzugriffe
unterliegen der deklarativen Policy und den OS-Rechten des Prozesses.
[Erzeugungsattribute][attributes] · [Verschachtelte Jobs][nested]

## Verworfene Alternativen

- Das Ende des direkten Kindes, ein leeres TCP-Ergebnis oder ein
  beliebiges Completion-Paket reicht für Nachkommen ohne Listener nicht aus.
- Eine nachträgliche Zuweisung des suspendierten Kindes erzeugt ein
  separates Startfenster. Für dieses Rezept verwendet der Adapter den
  von Windows bereitgestellten Erzeugungsattributpfad.
- Eine neue umfassende Win32-Abstraktion würde Abhängigkeit und
  Prüffläche über die benötigten Start-, Wait- und Job-Aufrufe hinaus
  vergrößern.

## Compliance

- Native Windows-Tests belegen den positiven Start samt besessenem
  Listener, exakten Probes und vollständig beobachtetem Stop.
- Ein kontrolliertes Kind ohne Listener überlebt zunächst den
  direkten Prozess. Der Stop muss dieses Kind erfassen, bevor der
  Adapter Erfolg meldet. Eigene Beobachter-Handles werden freigegeben.
- Abbruch während HTTP, Timeout, fremder Listener, fehlgeschlagene
  Job-Zuordnung und ein unbelegter Stop bleiben getrennte Fälle.
  Ein unbestimmter Effekt bleibt nach Reopen `Unknown`. Abbruch des
  Workers wird separat ohne Erfolgsbeleg geprüft.
- Die Tests laufen mit echten Prozessen auf Windows. Dieselben
  begrenzten HTTP-Verträge laufen weiterhin auf Linux. Die konkrete
  Schnittstelle wird auf Handle-Leaks, Vererbung und Safety geprüft.

[attributes]: https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute
[jobs]: https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects
[accounting]: https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_basic_accounting_information
[termination]: https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-terminatejobobject
[nested]: https://learn.microsoft.com/en-us/windows/win32/procthread/nested-jobs
[tcp]: https://learn.microsoft.com/en-us/windows/win32/api/iphlpapi/nf-iphlpapi-getextendedtcptable
