# Auftragsumfang und Agent-Berechtigungen

Aktueller Implementierungsstand von [Plan 21](plans/21-TASK-SCOPE-AND-PERMISSIONS.md)
nach Annahme von [ADR-0117](adrs/0117-auftragsbezogene-tests-und-verifikation.md) und
[ADR-0118](adrs/0118-appweite-agent-berechtigungsmodi.md). Historische ADRs und
persistierte Verifikationsergebnisse behalten ihren ursprünglichen Vertrag.

## Umsetzung und Prüfung

Die Recherche entscheidet über vorhandene Prüfungen, kurze Funktionskontrollen oder
ausdrücklich begründete Testerstellung. Neue Tests, Frameworks und Testmanifeste
benötigen einen Auftrag oder ein konkretes Komplexitäts-, Regressions- oder
Sicherheitsrisiko mit kurzer Begründung. Ein ausdrücklicher Testverzicht hat Vorrang.
Der ursprüngliche Auftrag bleibt in Recherche, Ausführung und Replan erhalten.

Der Plancompiler führt Umsetzung (`Change`) und Prüfausführung (`Check`) getrennt.
Eine Prüfabsicht autorisiert keine Testimplementierung. Ein Plan darf nur Änderungen
enthalten; Greenfield bündelt diese, ohne Testsuite oder Testabhängigkeiten anzuhängen.
Die historische interne Phase `DesignTests` enthält jetzt diese Verifikationsentscheidung.
Die historische Markdownüberschrift `Test Plan` und das Research-Feld `tests` bleiben
für die Lesbarkeit bestehender Pläne erhalten; ihre Liste darf leer sein.

Ohne passenden Prüfcommand erhält eine Änderung eine typisierte Diff-Invariante.
Ein eigenständiger Prüfschritt ohne ausführbaren Command bleibt offen und benötigt
eine ehrliche neue Planung. Der Core erzeugt dafür keine Testdateien. Die feste
Python-AST-Prüfung liest höchstens 16 belegte Python-Dateien mit `-I -B`, ohne ein
Prüfskript dauerhaft anzulegen. Projektdefinierte Skripte benötigen weiterhin einen
bestätigten Katalogeintrag oder eine exakte Einzelabfrage.

Diff-Evidence bestätigt angewendete Änderungen. Laufzeitaussagen benötigen die
passende operationale Evidence. Eine reine Änderungsaufgabe kann mit dieser
ausgewiesenen Grenze enden; eine geforderte Laufzeitprüfung wird dadurch nicht erfüllt.
Die Regel zur Testerstellung ist eine semantische Auftragsregel. Sie ist keine
Dateinamensperre, die jeden möglichen Testinhalt in einem Modellpatch erkennen kann.

## Einstellungen und Zulassung

`AgentPermissionSettings` enthält den typisierten Modus und eine positive Revision
bis `i64::MAX`. Catalog V10 ergänzt eine unveränderliche Revisionshistorie mit
`AskPermissions` als initialer Revision 1. Updates vergleichen die sichtbare Revision
atomar (CAS). Reopen und konkurrierende Schreiber behalten dieselbe Regel.
Die schmalen Settings-IPC-Verträge V1 enthalten nur Modus und Revision; Modelle und
Repositorytexte haben keinen Settings-Command.

Der Schalter bleibt außerhalb des austauschbaren Eingabe-/Freigabebereichs sichtbar.
Appweite Events synchronisieren Ansichten; ältere Events werden verworfen. Bei einem
CAS-Konflikt wird der aktuelle Stand geladen. Die Auswahl aktiviert den Modus sofort.
Ask- und Plan-Chats erhalten dadurch keine ausführbaren Werkzeuge.

| Aktion | Ask permissions | Full machine |
| --- | --- | --- |
| Lesen/Suchen im Projekt | automatisch | automatisch |
| Nichtdestruktive Patches und eingeordnete externe Dateiänderungen | Einzelabfrage | automatisch |
| Bestätigte Prüfkommandos und geschlossene Core-Prozessrezepte | Einzelabfrage | automatisch |
| Begrenztes HTTP-GET | Einzelabfrage | automatisch |
| Unbekannte Programme, Skripte, Parameter oder Shell | Einzelabfrage | Einzelabfrage |
| Löschen, Destruktion, Push und Veröffentlichung | Einzelabfrage | Einzelabfrage |

Strengere Workspace-Regeln behalten Vorrang. Automatische Policyentscheidungen tragen
den exakten Aktionsfingerprint, Scope, Modus und die Revision (Knowledge V41).
Sie erzeugen keinen fingierten `ApprovalGrant`. Der Core lädt die Einstellungen bei
Policyprüfung und unmittelbar vor dem Werkzeugbeginn erneut. Eine veraltete
automatische Entscheidung verhindert die Wirkung. Schon gestartete Aktionen dürfen
kontrolliert enden. Ein Wechsel zu Full machine weckt den bestehenden, besessenen
Run-Manager; wartende nun erlaubte Aktionen werden vollständig neu vorbereitet.
Pausierte Aufgaben und weiterhin freigabepflichtige Aktionen bleiben wartend.

## Geschlossene Maschinenverträge

Normale ausführende Turns verwenden AgentAction V6. Historische V1–V5 bleiben streng
lesbar und erhalten keine Maschinenaktion. Jede `machine`-Aktion enthält einen V1-
Untervertrag und die aktuelle Schritt-ID. Choice/Arguments sind ebenfalls auf die
jeweilige Phase und ihre Capability begrenzt. Der Core bestimmt Wirkung und Risiko;
das Modell kann diese nicht angeben. Die WebView bekommt keine Shell-/Dateisystem-API.

- **Datei V1:** genau eine absolute externe reguläre Textdatei lesen, vollständig
  schreiben oder hashgebunden löschen. Maximal 64 KiB. Der Core bindet den kanonischen
  Elternpfad und Zielnamen, verweigert Symlinks/Reparse-Ziele, Projektpfade, Secrets,
  generierte/besondere Dateien und UNC-Pfade. Er legt keine Verzeichnisbäume an.
  Anlage verlangt belegte Abwesenheit; Änderung und Löschung verlangen den erwarteten
  Hash. Staging, No-Replace-Anlage und Prüfung nach der Wirkung bleiben verbindlich.
- **Prozess V1 / ProcessSpec V2:** höchstens 32 Argumente und insgesamt 8192 Bytes.
  CWD ist der Projektroot; Environmentnamen kommen nur aus dem Composition Root.
  Laufzeitlimit 30 Sekunden, stdout/stderr je 16 KiB, bestehende Prozessbaumbeendigung
  und Cancellation. Automatische Zusatzrezepte sind ausschließlich
  `python/python3/git/node --version` (auch `.exe`). Pfadqualifizierte Programme und
  zusätzliche Parameter sind unbekannt. Auch ein über PATH gefundenes Programm im
  Projekt darf kein Systemrezept erben. Unbekannte Prozesswirkungen bleiben nach der
  Ausführung konservativ `Unknown`, selbst bei Exit 0. Es gibt derzeit kein automatisch
  zugelassenes Installationsrezept; Installer benötigen Einzelabfrage.
- **HTTP-GET V1:** exakte secretfreie URL bis 4096 Bytes; HTTPS oder HTTP mit wörtlicher
  Loopback-IP. Keine Credentials, frei wählbaren Header, Requestbodies, Cookies,
  geerbten Proxies, Redirects oder automatischen Retries. Verbindungslimit 5 Sekunden,
  Gesamtlimit 15 Sekunden, UTF-8-Antwort maximal 64 KiB mit Secret-Prüfung. Der native
  Core besitzt einen begrenzten, stets gejointen Worker mit eigener vollständiger
  Runtime während der Anfrage; er übernimmt keinen möglicherweise I/O-losen
  Tokio-Kontext des Aufrufers und hält keine Verbindung über diesen Worker hinaus.
  Er meldet tatsächlichen
  Status, Bodyhash und begrenzten Antworttext.

Datei- und HTTP-Adapter prüfen automatische Settingsentscheidungen auch an ihrer
Wirkungsgrenze. Alle Maschinenaktionen teilen den Worktree-Mutationslease mit Patches
und Prozessen. Full machine erhöht keine Betriebssystemrechte und stellt keinen
OS-Netzwerksandkasten bereit. Synchrone lokale Dateisystemaufrufe besitzen kein hartes
OS-I/O-Zeitlimit; Datei- und Ausgabelimits sowie Cancellation zwischen Abschnitten
bleiben wirksam.

Approval Query V2 präsentiert den kanonischen Zielumfang, Operation und Hashanker.
Die historische Query V1 liefert für neue Maschinenaktionen `Unavailable`.
Entscheidungen bleiben opaque, revisioniert und aktionsgebunden; die UI sendet keine
ausführbaren Pfade oder Argumente zurück.
Die bestehende Activity V1 behält ihre geschlossene Auswahl: Maschinenaktionen tragen
dort keine `selectedAction`. Der tatsächliche Maschinenumfang erscheint über die
neuen Freigabe-/Recovery-Verträge; das Run-Journal erhält seine echte Maschinenklasse.

## Unklare Wirkung und Wiederherstellung

Knowledge V42 ergänzt Maschinenaktionsarten, unveränderliche externe Scopes und
Recovery-Acknowledgements. Die Vorwärtsmigration erhält alle früheren Mutationszeilen
und ihre Guards. Vor der Wirkung werden Versuch, exakter Fingerprint, Schritt,
Ressourcenidentität und bounded Ziel als `Unknown` atomar gespeichert. Quelltext,
Requestbody, Environment und rohe Ausgaben gehören nicht in diesen Scope.

Ein vollständiger Repositoryscan kann externe Wirkungen nicht versöhnen. Die UI zeigt
deshalb eine menschliche, exakt auf den dauerhaften Scope bezogene Entscheidung:
Dateizustand lesen und neu planen oder bei Prozess/HTTP die verbleibende Ungewissheit
bestätigen und neu planen. Diese IPC-Verträge V1 enthalten nur Task, Ledgerrevision,
Storeversion, Fingerprint und geschlossene Auswahl. Abbruch bleibt über die
vorhandenen Laufsteuerungen erreichbar.

Bei Dateien liest der Core genau den gespeicherten kanonischen Scope unter aktueller
Policy. Einmalfreigaben sind echte verbrauchte Grants; automatische Beobachtung nutzt
eine echte aktuelle Policyentscheidung. Hash oder Abwesenheit, Entscheidung und Scope
werden gebunden gespeichert. Nicht beobachtbare Prozess-/Netzwerkwirkungen bleiben
ausdrücklich unbekannt. Die Zustimmung beweist keinen Erfolg.

Danach folgen vollständiger Repositoryscan und Replan; die ursprüngliche Aktion wird
nicht wiederholt. Neustarts nach Beobachtung, Reconciliation, Retirement,
Planrevision, Localization und vor neuem Schrittbeginn lesen die dauerhaften
Zwischenstände und setzen denselben Replan fort. Historische Versuche bleiben
`Unknown/Replanned`. Andere oder veraltete Scopes werden abgewiesen.

Maschinenquittungen sind bounded reale Beobachtungen im aktuellen Modellkontext.
Sie imitieren keine Repositoryrevision und schließen keine Repository-Verifikation
automatisch ab. Antwortbodies werden nach Neustart nicht als dauerhaft bestätigte
Evidence rekonstruiert. Eine reine externe Beobachtung ersetzt daher keinen passenden
Abschlussbeleg des Aufgabenledgers.

## Prüfnachweise und Plattformgrenzen

Die Windows-Abnahme nutzt echte Dateien, Prozesse, LibSQL, einen lokalen HTTP-Server
und den erzeugten Hello-World-Server. Der unabhängige HTTP-Prüfer läuft außerhalb des
Agenten und bestätigt die tatsächliche Ausgabe. UI-Komponentenprüfungen decken
appweite Revisionen, Fokus/Entwurf/Scroll, Tastatur und stale Antworten ab; die
Browserfixture zeigt beide Themes und kleine Fenster ohne Maschinenwirkungen.

Linux/macOS wurden für diesen Auftrag ausdrücklich aus der lokalen Abnahme genommen.
Ihre bestehende CI-Matrix bleibt erhalten. Die vollständigen A^3-Qualitätsgates und
die konkreten lokalen Ergebnisse werden im [Planpaket](plans/21-TASK-SCOPE-AND-PERMISSIONS.md)
festgehalten.

Die folgenden Kapazitätsmessungen betreffen den vollständigen SingleAction-Vertrag.
Das vollständige V6-Schema ist größer als V5. Gemeinsame skalare Schematypen und der
aktuelle Schrittanker werden einmal definiert; das statische Systembudget bleibt
unverändert. Die reproduzierbare Kontextfixture mit 16K Fenster, langem Ziel, offener
Fehlermemory und vollständig wiederholtem Schema erhielt vor der Kompaktierung
`AllocationOverflow` (System/Schema 9160 gezählte Bytes). Danach passen alle Pflichtanker
(8437 gezählte Bytes in derselben Pflichtankerfixture). Die Originalquellenfixture
zählt 8385 System-/Schemabytes; für deren optionalen Quelltext
reicht das verbleibende Budget weiterhin nicht. Er wird ehrlich ausgelassen und der
Pack als unvollständig markiert. Dieselbe Fixture mit 32K liefert den aktuellen
Quelltext. Die 8K-/16K-Fixtures mit Schema im Formatfeld behalten aktuelle Originale.
Output-/Sicherheitsreserven und Tokenzählstrategie werden dafür nicht verkleinert.

Die produktive SourceGuided-Ausführung verwendet kleine Phasenverträge statt dieses
vollständigen Wire-Schemas. ContextCompilerPolicyVersion V10 reserviert deshalb den
größten möglichen tatsächlichen Phasenvertrag einschließlich Formatfeldschema und
Schemawiederholung. Die Generation ist auch an Mutation-Folgekontexte und Digest gebunden;
vor jedem Provideraufruf gelten weiterhin die konkreten Budgetprüfungen. Lange aktuelle
Schritte werden vollständig bewahrt. Die entsprechende 16K-/32K-Repeat-Schema-Regression
liefert aktuelle Originale zusammen mit langem Ziel und offener Fehlermemory. Replan
behält seinen eigenen eingeschränkten Vertrag. Die gespeicherten Modellprofile und
beide Reserven werden nicht verändert.
