# ADR-0041: Sichere Moduswechsel und dauerhafte Nachrichtenwarteschlange

- Status: Angenommen
- Datum: 2026-09-04
- Ergänzt: ADR-0033, ADR-0038
- Ersetzt: ausschließlich die Inspector-Spiegelung des Recherchewegs aus ADR-0040

## Kontext

Eine A^3-Session konnte ihren Modus bisher nur implizit und vorwärts wechseln. Während einer
laufenden Recherche war der Composer gesperrt; weitere Arbeitsaufträge gingen deshalb verloren
oder verleiteten zum Abbruch des aktiven Laufs. Gleichzeitig spiegelte der Inspector den bereits
im Chat sichtbaren Rechercheweg und verdrängte die operativen Ansichten für Fortschritt,
Änderungen und Review. Der Start einer gerade veröffentlichten Planrevision konnte außerdem mit
dem noch nicht terminal sichtbaren Vorgängerjob kollidieren. Terminale Agentenläufe wurden dabei
fälschlich weiterhin als Laufzeitbelegung behandelt.

## Entscheidung

### Modus ist eine Eigenschaft des nächsten Work Items

Ask, Plan und Agent bleiben unveränderte Capability-Grenzen. Die Oberfläche zeigt sie jederzeit
als Stufenleiste und unterscheidet den aktuell ausgeführten Modus vom Zielmodus der nächsten
Nachricht. Eine Auswahl unterbricht keinen laufenden Job. Der Core übernimmt den Zielmodus erst,
wenn die Nachricht tatsächlich startet.

Ein Rückwechsel beginnt ein unabhängiges Work Item und entfernt dessen Task-, Run- und
Plananker. Ein später gewählter Agent-Auftrag wird zunächst als Plan vorbereitet, wenn seit der
letzten Agentenarbeit Ask oder Plan gewählt wurde. Mutation ist erst nach Freigabe der exakt
sichtbaren neuen Planrevision möglich. Moduswahl verleiht keine Policy-, Prozess-, Datei- oder
Netzwerkberechtigung.

### Dauerhafte FIFO

Validierte Nachrichten, die nicht sofort starten können, werden in Knowledge V33 als begrenzte
append-only Queue-Einträge und Zustandsereignisse gespeichert. Pro Session gelten 16 wartende
Nachrichten und 1 MiB Inhalt, pro Worktree 64 Nachrichten und 4 MiB. Zielmodus,
Rechercheauswahl, validiertes Slash-Command-Profil, begrenzter Nachrichtentext und Core-Zeitpunkt
werden vor dem Vormerken geprüft.

Die Reihenfolge ist unveränderlich. Nicht gestartete Einträge dürfen entfernt werden. Rückfragen,
Planprüfung, Approval, Pause, Fehler und Abbruch sind menschliche Haltepunkte. Eine unmittelbare
Antwort auf einen Haltepunkt hat Vorrang vor der FIFO; nach Fehler oder Abbruch erfordert die Queue
eine ausdrückliche Fortsetzung. Nach einem Prozessneustart bleibt sie sichtbar und pausiert, bis
der Nutzer sie fortsetzt. Presentation Delete entfernt Queue-Präsentationsdaten, Archivierung
behält und pausiert sie.

### Laufbesitz und Planübergabe

Der Scheduler-Koordinator entscheidet über Start oder Queue. `Queued`, `Running`, `Pausing`,
`Paused` und `Cancelling` belegen die Agentenlaufzeit; `Succeeded`, `Failed` und `Cancelled` sind
historische Anzeigezustände und blockieren keinen neuen Lauf. Eine ImplementPlan-Anforderung am
Ende des Planjobs wird an die exakte Session-, Plan- und Indexrevision gebunden und nach dem
terminalen Vorgängerjob genau einmal übernommen. Ein Konflikt darf keine partiellen Goal-,
Ledger- oder Runanker erzeugen.

### Eine Rechercheansicht, eine operative Seitenleiste

Recherche und Quellen bleiben ausschließlich turngebunden im Chat. Die Spiegelung aus ADR-0040
wird nicht mehr verwendet. Die rechte Seitenleiste heißt „Agentenlauf“, enthält Fortschritt,
Änderungen und Review und erscheint erst nach erfolgreicher Task-Materialisierung. Header-Menü und
Seitenleistenschalter liegen in einem normalen Aktionsbereich und dürfen sich nicht überlagern.

## Folgen

- V1- bis V3-IPC bleiben kompatibel; V4-Submit und V3-Sessionprojektion tragen Zielmodus und Queue.
  Der V2-Planstart liefert einen geschlossenen semantischen Ausgang statt UI-seitig gedeuteter
  Fehlertexte.
- Queue-Referenzen sind opak und an Worktree, Session und Queue-Revision gebunden.
- Persistenz und Scheduler werden komplexer, aber der Nutzer verliert während laufender Arbeit
  keine validierten Folgeaufträge mehr.
- Recherche-, Approval-, Verification- und Mutationsautoritäten bleiben unverändert.
- Der Inspector lädt keine zweite Rechercheprojektion und besitzt keine eigenen Recherchetimer.

## Nicht entschieden

- Queue-Einträge werden nicht umsortiert.
- Es gibt keine parallele Mutation und keine zusätzliche Agentenberechtigung.
- Rohes internes Denken, Providertranskripte und Quelltext werden nicht in der Queue gespeichert.
