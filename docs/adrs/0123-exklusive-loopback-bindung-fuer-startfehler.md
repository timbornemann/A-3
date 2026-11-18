# ADR-0123 Exklusive Loopback-Bindung für Startfehler

Status: Accepted  
Datum: 2026-11-09  
Entscheider: Tim Bornemann

Annahme durch Tim Bornemann am 2026-11-09.

## Kontext

[Plan 22/P7](../plans/22-SMALL-LOCAL-MODEL-AGENT-CORE.md#p7-greenfield-mehrere-module-und-große-bestehende-projekte)
verlangt für den A04-Auftrag echte Fehlerexits bei ungültigem Portargument und bei
nicht bindbarem Port. Der bisherige Adapter erkennt einen fremden Listener vor
dem Start. Das schützt den fremden Dienst und liefert keine Beobachtung des
Skriptexits. Der Core braucht eine selbst gehaltene, exakt freigegebene Bindung.
Die gepinnte Rust-Standardbibliothek und die vorhandenen sicheren Socket-APIs
setzen auf Windows kein `SO_EXCLUSIVEADDRUSE`. Winsock verlangt die Option vor
dem Binden, damit ein weiterer Socket den exakten Endpunkt auch mit `SO_REUSEADDR`
nicht übernehmen kann. [Bindungsregeln][bind-rules] · [Socketoption][exclusive]

## Entscheidung

- Das geschlossene Produktrezept darf unter derselben Worktree-Lease einen
  eigenen IPv4-TCP-Socket am literalen Endpunkt `127.0.0.1` und dem im Original
  gebundenen expliziten Port halten. Der Core stellt die Bindung als einzelnen
  exakten, zeitlich begrenzten Policy- und Freigabeschritt dar. Die Freigabe
  bindet Rezept, Schritt, Snapshot, Skriptrevision und Endpunkt. Der Socket
  nimmt keine Verbindungen an. Ein fremd belegter Endpunkt bleibt eine
  Umgebungsgrenze. Der Adapter verwendet und beendet keinen fremden Listener.
- Unter Windows erzeugt und besitzt der sichere Socket-Adapter den nicht
  vererbbaren Socket. Das bereits nach
  [ADR-0122](0122-besessener-windows-job-fuer-loopback-pruefungen.md)
  benannte FFI-Modul darf an diesem geliehenen Socket ausschließlich
  `setsockopt(SOL_SOCKET, SO_EXCLUSIVEADDRUSE, TRUE)` vor `bind` setzen und bei
  Fehlschlag den zugehörigen Winsock-Fehler lesen. Der Aufruf bleibt an den
  privaten Rezeptpfad gebunden. Es gibt keinen Fallback auf ungeschützte
  Bindung. Sichere API, RAII und lokale Safety-Kommentare bleiben verbindlich.
  [Socketoption][exclusive]
- Derselbe FFI-Adapter darf `GetExitCodeProcess` am signalisierten Handle des
  direkt erzeugten Job-Kinds vor jeder vom Core angeforderten Terminierung
  lesen. Für den Fehlerfall zählt nur ein tatsächlich beobachteter numerischer
  Anwendungscode zwischen 1 und 255. Auf Linux zählt nur ein regulärer
  Nichtnull-Exit. Timeout, Abbruch, eigener Kill, nicht eindeutig beobachtetes
  Ende und Ausnahmefehler liefern keinen Fehlerexit-Beleg. Der Beleg behauptet
  die konkrete Beobachtung am Kind und keinen Nachweis über dessen interne
  Fehlerursache. [Windows-Exitstatus][exit-code]
- Der Core hält den Socket während des Startversuchs und des gesamten
  begrenzten Cleanup. Bei Ablauf der Cleanup-Frist schließt er den eigenen
  Socket und hält die unvollständig beobachtete Prozesswirkung als `Unknown`.
  Für einen Beleg muss er das Ende des privaten Jobs oder der besessenen
  Prozessgruppe vor dem Schließen beobachten. Er startet das gebundene Skript
  unter identischer Revision mit der exakt freigegebenen argv-Form. Der
  blockierte Start liegt vor dem freigegebenen Start mit derselben argv-Form
  bei freiem Port. Der Prüffall für das ungültige Argument verwendet die dafür
  freigegebene argv-Form. Ein nicht vollständig beobachteter Effekt verbleibt
  in `Unknown`. Nachweise für den Teilversuch werden ausschließlich gemeinsam
  mit den passenden Start- und HTTP-Beobachtungen desselben Rezepts gewertet.

## Konsequenzen

Der Core kann einen Fehlerexit bei einer von ihm selbst gehaltenen Bindung
beobachten. Die Reservierung betrifft den freigegebenen IPv4-Endpunkt. Die
HTTP-Beobachtungen, die Sperre für fremde Listener und die Release-Regel nach
vollständigem Cleanup folgen weiter dem vorhandenen Controller und Owner.
Weitere lokale, entfernte oder beliebige Portbelegungen erhalten keine API.

## Verworfene Alternativen

- Ein erkannter fremder Listener zeigt nur, dass A^3 vor dem Start angehalten hat.
- Eine freigegebene Reservierung lässt eine Lücke vor dem Bind-Versuch des Kinds.
- Ein zusätzlicher Shell-, Python- oder .NET-Helper würde die genaue Start- und
  Laufzeitgrenze des Rezepts erweitern.

## Compliance

- Native Tests auf Windows und Linux zeigen, dass ein zweiter Socket den
  gehaltenen Endpunkt auch mit Reuse-Option nicht übernehmen kann und nach
  dem Drop des Holders erneut binden kann.
- Echte Python-Fälle trennen gültigen Start, regulären Fehlerexit, Timeout,
  Abbruch und fehlenden Cleanup. Kein Fall leitet einen Erfolg aus dem Exit
  eines von A^3 beendeten Kinds ab.
- Policy- und Persistenztests weisen eine geänderte argv-Liste, einen anderen
  Port, ein fehlendes Teilresultat, veraltete Revision und einen fremden
  Toolversuch ab. Der bisherige Beleg für einen einzelnen HTTP-Start erfüllt
  das neue zusammengesetzte Rezept nicht.

[bind-rules]: https://learn.microsoft.com/en-us/windows/win32/winsock/using-so-reuseaddr-and-so-exclusiveaddruse
[exclusive]: https://learn.microsoft.com/en-us/windows/win32/winsock/so-exclusiveaddruse
[exit-code]: https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getexitcodeprocess
