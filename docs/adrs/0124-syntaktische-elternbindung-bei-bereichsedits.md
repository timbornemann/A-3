# ADR-0124: Syntaktische Elternbindung bei Bereichsedits

Status: Accepted  
Datum: 2026-11-17  
Entscheider: Tim Bornemann

Annahme durch Tim Bornemann am 2026-11-17.

## Kontext und belastbarer Gegenfall

ADR-0119 gibt dem Core die Verantwortung für kleine, aktuelle und exakt gebundene
Editaufträge. Der bestehende Vertrag schützt Ausgangshash, Fenster und sämtliche
Außenbytes. Er verspricht bisher keine syntaktische Einbettung des Ersatzes.
Insbesondere ersetzt der bytegleiche Außenbereich keine Syntax- oder Funktionsprüfung.

Der unveränderte Source008-Versuch `http-same-run-edit3-qwen-001` hatte tatsächlich
16384 Kontexttokens und 1024 Ausgabetokens in Plan, Context und Edit. Plan und Context
lieferten den engen Intent, den LF-Fehler in `do_GET` zu ändern. Edit erhielt das
vollständige 370-Byte-Fenster der Methode, den Intent, alle aktiven Regeln und die
Anweisung `code=whole window; keep indent`. Das Modell lieferte dennoch 1670 Bytes
Programmtext. Der Core ersetzte exakt `[719,1089)` der 1743-Byte-Ausgangsdatei;
719 Präfix- und 654 Suffixbytes blieben identisch. Die vollständige Ergebnisdatei
hat 3043 Bytes und SHA-256
`9aa7403f4057d25f4bd4d1f99da824317891731beb3ebda821997f94f3fc6517`.

Das unveränderte `_other`-Deklarationsheader beginnt vorher bei Byte1094, danach
wegen des bekannten Deltas1300 bei Byte2394. Vorher gehört er zur Klasse `Handler`,
deren unverändertes Header bei Byte88 beginnt. Danach steht er im Modul, innerhalb
eines `if __name__`-Blocks. Die Außenbytes blieben erhalten, ihre syntaktische
Zuordnung änderte sich. Der Code ist syntaktisch parsebar. Ein bloßer Syntaxcheck
oder eine Größenprüfung erfasst diesen konkreten Fehler daher nicht.

Die unabhängige vollständige Skriptprüfung lehnte die Ausführung außerdem wegen
des zweiten Serverstarts nach dem ersten KeyboardInterrupt ab. Der Versuch endet
korrekt ohne HTTP-Erfolgsbeleg und ohne Done. Ein einzelnes Modellergebnis belegt
weder eine Fehlerquote noch, dass Edit V3 oder fehlende Scope-Metadaten die Ausgabe
verursacht hätten.

## Abgrenzung zur bereits zulässigen Projektion

Der bereits im Core belegte Scope – Operation, Pfad, aktuelle Callable und Bereich –
darf unter ADR-0119 knapp in die Editprojektion übernommen werden. Diese
Darstellung erzeugt keine neue Zulassungsregel und ersetzt keine Pflichttexte.
Sie benötigt weder zusätzliche Modellentscheidungen noch neue Antwortfelder.
Diese ADR entscheidet ausschließlich die nachfolgende zusätzliche mechanische
Zulassung. Ihre Implementierung beginnt erst nach ausdrücklicher Annahme.

## Entscheidung

### 1. Eng qualifizierter erster Verbraucher

Der Core kann einen Existing/Replace-Job als vollständige aktuelle Funktions- oder
Methodendeklaration qualifizieren, wenn der publizierte Index, die tatsächlichen
Originalbytes und der vorhandene Sprachadapter diesen Umfang eindeutig belegen.
Der erste Verbraucher ist dieser allgemeine Deklarationsfall; Pfadnamen,
HTTP-Methoden, Modellnamen und der obige Dateihash sind keine Produktregeln.

Vollständigkeit umfasst dabei die bereits geprüfte vollständige Zeilenhülle einer
Parserdeklaration, einschließlich vorhandener Einrückung und abschließendem LF.
Der vorhandene `http_correction_definition_page_matches` zeigt diese getrennte
mathematische Bindung. Exakte Rangegleichheit aus dem SourceOffer-Helper
`is_full_declaration` entscheidet ein anderes Angebot und darf nicht als
Scope-Qualifikation wiederverwendet werden. `need` ohne Angebot bedeutet nicht,
dass keine vollständige aktuelle Callable belegt wäre. Der SourceOffer-Vertrag
wird durch diese ADR nicht nebenbei geändert.

Generische Bytefenster, V6-Halbfenster, Append und Add werden dadurch nicht
stillschweigend zu syntaktisch qualifizierten Jobs. Ihre bisherigen Verträge und
Erkenntnisgrenzen bleiben erhalten. Bei unsupported oder unvollständigem
Ausgangsparse darf der Core keinen qualifizierten Scope behaupten. Eine bereits
erteilte Qualifikation darf nach einer schlechten Antwort nicht automatisch auf
einen schwächeren Bytevertrag zurückfallen.

### 2. Geprüfte Elternbindung der unveränderten Deklarationen

Für den qualifizierten Job bestimmt der Core aus dem aktuellen Sprachparse die
Deklarationsheader außerhalb des editierbaren Bereichs und deren lexikalische
Deklarationseltern. Die Bindung enthält Ausgangsrevision und Hash, exakten Bereich,
aktuelle Index-/Snapshotidentität, Adapter-/Grammatikrevision und die betreffende
Core-Regelversion. Das Modell konstruiert oder bestätigt diese Bindung nicht.

Nach der vorhandenen begrenzten Decodierung und exakten EOL-/Bytekompilierung
wird der vollständige Kandidatenpuffer flüchtig geparst. Ein Header vor dem Fenster
bleibt an derselben Byteposition; ein Header danach wird ausschließlich um das
arithmetisch bekannte Byte-Delta verschoben. Es gibt keine Suche nach ähnlichem
Text und kein Rebinding auf ein gleichnamiges Symbol. Lokale Parser-IDs dürfen
nicht über zwei unterschiedliche Parsebilder hinweg gleichgesetzt werden.

Die unveränderten Header müssen am berechneten Ort dieselbe deklarative Art und
dieselbe außerhalb des Fensters verankerte Elternkette behalten. Im Gegenfall
verliert `_other` seinen unveränderten Klasseneltern. Dies ist eine mechanische
Ablehnung vor einer nach außen verfügbaren Patch-Proposal, Preview oder Policy-
Freigabe. Der vollständige Kandidat wird dabei weder geschrieben noch publiziert.

Die Regel friert keine Namen oder Deklarationsanzahl innerhalb des Fensters ein.
Ein Umbenennen der bearbeiteten Methode, neue gültige Körper, geschachtelte Helfer
oder das erlaubte Entfernen der bearbeiteten Deklaration bleiben möglich, soweit
die unveränderten Außendeklarationen ihre Eltern behalten. Ob die jeweilige
Änderung fachlich beauftragt und richtig ist, entscheiden weiterhin Goal,
Planabdeckung, Policy und passende Verifikation.

### 3. Begrenzte Aussage

Die erste Regel prüft die durch den bestehenden Adapter belegten
Deklarationseltern. Sie beweist weder unveränderte Namensauflösung noch gleiches
Laufzeitverhalten oder sämtliche anonymen Sprachblöcke. Ein vollständiges
Programm an Modulebene kann dieselbe äußere Elternkette besitzen; dieser Fall
ist dadurch nicht allgemein ausgeschlossen. Für eine stärkere Ein-Callable-
Grammatikpflicht wäre ein gesondert begründeter Vertrag erforderlich.

Es gibt insbesondere kein pauschales Verbot von `import`, `class`, `main` oder
`__name__`, keine Namensliste, kein Größenverhältnisheuristik und kein automatisches
Einrücken, Abschneiden, Fuzzy-Ersetzen oder Erweitern des gebundenen Fensters.
Ein positiver Elternvergleich ist keine VerificationEvidence für die Funktion.

### 4. Application-/Adaptergrenze

Die Application orchestriert die Prüfung im gemeinsamen Abschluss eines
qualifizierten `PreparedEditPhase`/`ExecuteEditPhase`, bevor eine Patch-Proposal
an den Aufrufer gelangt. Kein ausschließlich nativer Testhelper und kein
HTTP-spezifischer Fixturezweig entscheidet die Produktzulassung.

Sie verwendet den vorhandenen `LanguageAdapter::parse`-Port, seine versionierten
`LanguageParseInput`/`LanguageParseResult` und die vorhandenen Symbolbereiche und
`Contains`-Beziehungen. Tree-sitter-Knoten bleiben im Adapter; Domain und
Application importieren keine Grammatik- oder Python-AST-Typen. Ein neuer Dienst,
Interpreterprozess, Syntaxpaket, globale Parserinstanz oder paralleler Executor
ist nicht vorgesehen. Reicht die bestehende Coverage eines Adapters für den
beanspruchten Elternbeleg nicht, lautet das Ergebnis unbewiesen, nicht bestanden.

Der Kandidatenparse trägt den tatsächlichen Kandidatenhash und bleibt ausdrücklich
eine flüchtige Ableitung aus der gebundenen Ausgangsrevision. Er ist kein
publiziertes Original, kein neuer Quellenfortschritt und keine neue Evidence.
Der Vergleich verwendet typisierte Werte; Error-Displaytexte steuern keine Kante.

### 5. Fehler, Ressourcen und Settlement

Ein belegter Elternwechsel führt zu einem neuen typisierten, nicht reparierbaren
Editfehler, beispielsweise `ScopeFrameMismatch`. Er wird über den bestehenden
RejectedReply-Abschluss samt bekannter Usage abgerechnet. Er ist kein
InvalidStructure und eröffnet keinen JSON-Repair, semantischen Retry, NoChange-
Replan, Quellenkredit oder automatischen neuen Call. Nichtbeweisbarkeit nach einer
vorherigen Qualifikation hält ebenfalls vor der Proposal an. Cancellation und
Deadlinefehler behalten ihre bisherigen eigenen Klassifikationen.

Es gelten die vorhandenen Parsergrenzen einschließlich 4MiB Quellpuffer,
2s Parsezeit, 500ms Poolwartezeit und begrenzten Knoten, Tiefe, Symbolen und
Diagnostiken. Parsing besitzt denselben Owner, Fortschritt, Cancellation und
verbleibenden Runzeitraum. Parserfehler, abgeschnittene Coverage oder fehlender
Elternbeleg liefern kein positives Ergebnis. Es gibt keinen Prozessaufruf und
keine offene SQL-Transaktion während des Parsens. Bereits kaputte oder nicht
unterstützte Projekte werden nicht pauschal für sämtliche Byteedits gesperrt;
der Core muss den Umfang der tatsächlich vorhandenen Qualifikation sichtbar
halten und darf ihn nicht als Syntaxbeweis ausgeben.

Ausgangshash, aktuelle Publikation und Anker werden vor der Verwendung der
Bindung geprüft; der vorhandene E3-Pfad revalidiert vor Mutation weiterhin die
Livebytes. Ein alter Parse oder ein gleichbleibender Snapshot bei neuer
IndexRunId darf nicht stillschweigend weiterverwendet werden. Die Regel erteilt
keine Ausführungsbefugnis und ersetzt keine bestehende Unknown-Recovery.

### 6. Versionierung und kalte Grenzen

Die Regel und die konkrete Elternbindung müssen Teil der Core-eigenen aktuellen
Request-/Jobbindung sein. Die tatsächliche Implementierung weist nach, welche
vorhandene Requestbindung dies vollständig abdeckt. Eine neue Pflicht darf auf
keinen Fall allein als ungebundener Kommentar wirken.

Diese ADR verlangt keinen neuen Modellantwortschlüssel und keinen vorsorglichen
Edit-Wire-, Kontextprotokoll- oder Datenbankversionssprung. Änderungen daran sind
nur bei einem konkret nachgewiesenen Identitäts- oder Persistenzbedarf zulässig.
Der eingefrorene historische v3-Geometrieencoder und seine zehn Goldens bleiben
bytegleich. Ein kalter AcceptedReply-/Kostenbeleg ist niemals ein rekonstruierter
Eltern-Pass oder eine ausführbare Patch-Proposal. Vor einer späteren Ausführung
muss die aktuelle Bindung erneut bewiesen werden; RejectedReply bleibt nicht
repairbar. Budgets und ursprüngliche Deadline bleiben vollständig erhalten.

## Verträge und Folgen

- ADR-0119 bleibt der Vertrag für kleine Core-Jobs, vollständige Originalautorität,
  passende Ergebnisnachweise, endliche Budgets und Kandidatenkomposition bis P11.
- ADR-0015 bleibt der vorhandene versionierte LanguageAdapter-/Tree-sitter-Vertrag.
- ADR-0120 bleibt der exakte Preview-/E3-Vertrag. Policy, Fingerprint, einmalige
  Freigabe, Mutationlease, Livehash, Publish und Recovery werden nicht ersetzt.
- ADR-0121 zeigt eine andere, geschlossene Funktionsevidencepflicht. Diese ADR
  verallgemeinert deren Python-Ganzzahlbeleg nicht zu einem Syntax-Erfolgsorakel.
- ADR-0117 und ADR-0118 sowie die vollständigen Secret-/Root-/Permissionsgrenzen
  bleiben unverändert. Es werden keine Tests oder Abhängigkeiten im Nutzerprojekt
  angelegt und keine neuen Prozessrechte vorausgesetzt.

Die zusätzliche Zulassung kann ungeeignete Modellausgaben früher abweisen. Das
beweist keine höhere Modell-Erfolgsquote; diese wird erst mit neuen eingefrorenen
Inputs und frischen lokalen Versuchen geprüft. Sie kann zusätzliche Parsearbeit
kosten. Es wird keine Geschwindigkeitsverbesserung behauptet.

## Verifikation vor Implementierungsabschluss

Die beigefügte geplante Regressionsmatrix enthält den exakten Source9aa-Gegenfall,
gültigen kleinen Körperedit, Rename, Delete, neue geschachtelte Deklarationen,
Kommentare/Strings, Add/Append/Teilfenster, Stale/Cold, Unknown, Ressourcen und
Cancellation. Der Gegenfall muss echte RejectedReply-Abrechnung ohne Proposal,
Policy oder Mutation zeigen; der gültige Fall behält zusätzlich die vollständige
produktive HTTP-Verifikation. Fake-/Parserchecks geben keinen Modellkredit.

Erst nach Annahme werden ein vertikaler Anwendungspfad und die notwendigen
Adapter-/Storegrenzen implementiert und unabhängig reviewed. Danach folgen
gezielte native Vertragsfälle, unveränderte Legacy-/Byte-/EOL-/Policy-Regressionen,
Workspace, Clippy und aktuelle Windows-/Linux-Belege. Frische registrierte lokale
Modelle bleiben getrennte fachliche Prüfungen. Source008, beide Originalversuche
und ihre Messbelege bleiben unverändert; dieses Dokument schließt P7 nicht ab.
