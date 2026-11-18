# Daten und Persistenz

## Angenommener Zielvertrag nach ADR-0119

Stand: 2026-11-06. Dieser Abschnitt beschreibt die angenommenen Änderungen aus
[ADR-0119](adrs/0119-kleinschrittiger-agentenkern-fuer-lokale-modelle.md) und dem
[Zielvertrag](SMALL_LOCAL_MODEL_AGENT_CORE.md), keine bereits implementierten Tabellen
oder Schemafassungen. Die nachfolgenden konkreten Schemaversionen, Tabellen und Speicherpfade
dokumentieren weiterhin die bestehende Altbaseline. Neue Folgeversionen und physische
Speicherstrukturen werden erst mit ihrer Implementierung festgelegt.

Allgemeine Chats erhalten einen getrennten lokalen Gesprächsspeicher unter dem vorhandenen
geprüften AppData-Root mit dem bestehenden libSQL-Adapter. Chattext gehört weder in den
globalen Projekt-/Settingskatalog noch in projektlokale Knowledge-Datenbanken. Es gibt keine
erfundene Projekt-/Worktreeidentität und keine automatische Übernahme als Projektwissen.
Projektgebundene Ask-/Agent-Verläufe und Aufgaben bleiben worktreeisoliert. Sichtbare,
redactiongeprüfte Nutzernachrichten und Antworten sind Gesprächsdaten; kompilierte
Modellrequests, Systemprompts, Thinkingtraces und Secrets werden nicht gespeichert.
Begrenzte Seiten, Retention, Löschung und Backupgrenzen sind für den neuen Speicher festzulegen.

Der vollständige Originalauftrag bleibt pro Zielrevision unveränderlich im bestehenden
Goal-Store: initial höchstens 64 KiB UTF-8 in höchstens 33 geordneten Teilen zu je 2048 Bytes,
ohne getrennte Mehrbytezeichen. Bereichsreferenzen binden Kriterien an ihre ursprünglichen
Auftragsteile. Übergröße wird vor Aufgabenstart erklärt abgewiesen; die bisherige
8-KiB-Kürzung beziehungsweise GoalObjective allein bilden diesen Ursprung nicht ab.
Eine Präsentationslöschung entfernt weder Originalauftrag noch Goal, Ledger, Journal oder
Nachweise. Es entsteht keine zweite Anforderungsdatenbank.

Die aktive Modellsicht ist eine deterministische, revisionsgebundene Projektion dieses
dauerhaften Zustands. Sie enthält Teilziel, Kriterien, alle geltenden Constraints und
Nicht-Ziele, relevante offene Fehler und Verifikationslage. Nicht sicher ausgeschlossene
Constraints bleiben vollständig; ausgelassene Auftragsteile bleiben dauerhaft offen.
Die vollständige Abdeckung bleibt persistiert, auch wenn nur Zähler, Digest und relevante
Aliase in die begrenzte Teilsicht passen. Neue strukturierte Pläne bilden einen rollierenden
Horizont; offene globale Kriterien führen nach dessen Abschluss im selben Run per
Ledger-/Journal-CAS zu weiterer Planung. Verbrauchte Budgets, ausstehende Planung und
unveränderliche Versuche müssen nach Crash/Reopen erhalten bleiben. Acceptance setzt
vollständige aktuelle Kriterienabdeckung und passende Evidence voraus.

Gemäß [Plan 22](plans/22-SMALL-LOCAL-MODEL-AGENT-CORE.md) sind neue Verträge
in P2 bis P10 nur aus der Kandidaten-Testkomposition mit isoliertem temporärem
AppData erreichbar. [ADR-0119](adrs/0119-kleinschrittiger-agentenkern-fuer-lokale-modelle.md) hebt mit der freigegebenen Ergänzung vom 2026-11-07 die frühere Altübernahme auf:
P11 ist die einzige Produktionsumschaltung mit frisch initialisierten A^3-Daten;
alte Entwicklungsdaten müssen weder konvertiert noch im Produkt lesbar sein.
Bestehende Jobs und Werkzeuge müssen davor terminal beendet sein; ungeklärte
Wirkungen sperren die Umschaltung. Inkompatible Daten werden vor Writes abgewiesen,
nicht automatisch migriert, gelöscht oder überschrieben. Neuinitialisierung,
Versionierung, CAS, atomarer Rollback und Crash-/Unknown-Recovery neuer Runs bleiben
Pflicht. Die folgenden Versions-/Legacybeschreibungen dokumentieren vorhandenen
Baseline- beziehungsweise Kandidatencode, keine neue Upgradepflicht alter Daten.

## P2-Kandidat: unveränderlicher Originalauftrag im Goal-Store

Die Kandidatenschnitte erweitern den bestehenden Goal-Store. Ihre festen
V43- bis V68-Schemaaufbauschritte sind ausschließlich über adapterinterne `cfg(test)`-
Konstruktoren erreichbar. Produktionsmigrationsliste und
`KnowledgeSchemaVersion::CURRENT` bleiben V42. Der Kandidat nimmt nur seinen
exakten V68-Bestand oder ein tatsächlich leeres V0-Schema an. Vorhandene V42- bis
V67-Bestände und V0-Dateien mit Inhalt werden bereits read-only abgewiesen.
Die Prüfung erfolgt nochmals vor Konfiguration. Der anschließende Aufbau einer
leeren Datenbank darf die internen SQL-Schritte wiederverwenden; das ist keine
Altübernahme. Es gibt keinen historischen V43-Öffner. Ein normales Produktopen
weist den neueren Kandidatenbestand weiterhin vor Writes zurück.

Der feste Testvertrag gilt für die tatsächlich benötigten Goal-/Ledger-, Index-,
Evidence-, Allowlist-, Journal-/Action-, Policy- und Recovery-Ports einschließlich
Projektregistrierung. Kalte erste Portaufrufe werden separat geprüft; ein schon
über Goal geöffnetes Mutationhandle darf keinen falschen V42-Opener verdecken.
Search und TaskLens-Claims verwenden für den konkreten Patch-/Contextverbraucher
ebenfalls den privaten Vertrag; ihre kalten Erstaufrufe werden geprüft. Ein
Desktop-Kandidatenrouting und TaskLensWorkspace sind damit noch nicht implementiert. Open, Verify und Reopen prüfen Version, Migrationshistorie
und Projektidentität. Der Entwicklungsneustart folgt der ausdrücklich ergänzten
[ADR-0119](adrs/0119-kleinschrittiger-agentenkern-fuer-lokale-modelle.md).

`GoalContract.original()` unterscheidet ausdrücklich Legacy (`None`) von einem
unveränderlichen vollständigen UTF-8-Ursprung. Der normalisierte `GoalObjective`
ist eine Darstellung, kein rekonstruierter Ursprung. Bei vorhandenem Original
müssen sämtliche Kriterien eine auflösbare Originalbindung derselben Task- und
Goalrevision besitzen. Legacy-Revision, History und Store-Append dürfen einen
vorhandenen Ursprung nicht still entfernen.

| V43-Ergänzung | Persistierte Invariante |
| --- | --- |
| `goal_contract_revisions.original_contract_version` | `NULL` kennzeichnet Legacy; `1` verlangt vollständige Originaldaten. Fehlende Daten werden nicht als Legacy gelesen. |
| `goal_original_headers` | Originaldigest, Gesamtbytezahl und Teileanzahl pro unveränderlicher Goalrevision; höchstens 65536 Bytes und 33 Teile. |
| `goal_original_parts` | Geordnete BLOBs mit halboffenen, lückenlosen UTF-8-Bytegrenzen; je höchstens 2048 Bytes, ohne Normalisierung von Whitespace, CRLF oder Unicode. |
| `goal_criterion_origins` | Originaldigest/-bereich und geschlossene Nachweispflicht direkt am bestehenden Kriterium. Testcommand, Selector, Mindestzahl und Scope beziehungsweise Diffrelation bleiben erhalten. |
| `goal_criterion_diff_paths` | Kanonische geordnete Pfade einer Änderungsinvariante; ihre deklarierte Anzahl gehört zur unveränderlichen Pflicht. |

Der BLAKE3-Digest verwendet den versionierten Kontext
`a3.goal-original-request.v1` und die exakten Originalbytes samt Längenframing.
Task und Revision sind zusätzlich in jeder Domain-Bereichsreferenz gebunden.
Ein identischer Text in einer anderen Revision erlaubt keine Wiederverwendung
alter Referenzen. Das sind Herkunftsanker des Nutzerauftrags, keine Repository-
Evidence und kein Beweis einer richtigen Interpretation.

V44 ergänzt `goal_constraint_origins`, `goal_non_goal_origins` und
`goal_user_decision_origins`. Ein optionaler Bereich hängt direkt am vorhandenen
Constraint-, Nicht-Ziel- oder bestätigten Entscheidungswert; die Tabellen speichern
nur Sammlungseintrag, Digest und Bytegrenzen, keinen zweiten Regeltext.
Gleicher normalisierter Text wird trotz verschiedener Origins als Duplikat
abgewiesen; Wertgleichheit berücksichtigt die Herkunft. `UserDecision` bleibt eine
bestätigte Nutzerwahl: ein Originalverweis beantwortet keine offene Frage.

Die neuen Originzeilen werden nach den vorhandenen Textlisten und vor dem
Originalheader in derselben Goaltransaktion geschrieben. Der FK zur jeweiligen
Textliste gilt sofort, der FK zum noch kommenden Originalheader erst beim Commit.
Seine Anlage schließt weitere Origin-Inserts; Update und Delete sind gesperrt.
Der Writer verlangt für neue annotierte Werte Schema V44 und weist
inkompatible Versionen vor dem ersten Taskwrite als nicht unterstützt zurück. Reader prüfen alle drei Listen, UTF-8-Bereiche, exakten
Digest, Ordinalgrenzen und verwaiste Zeilen; übergroße Digests werden bereits
in der SQL-Projektion vom Payload-Decode ausgeschlossen. V43-DDL und dessen
historischer Migrationseintrag bleiben unverändert.

`GoalOriginalCoverage` rekonstruiert die strukturelle Herkunftsabdeckung aus dem
Goal; der Bericht ist keine zweite persistierte Abdeckungsautorität. Seine Union
umfasst Kriterien und vorhandene Origins aller drei Textlisten; Überschneidungen
zählen einmal. Jeder nicht zugeordnete Bytebereich bleibt als revisions- und
digestgebundene UTF-8-Lücke sichtbar, auch Whitespace. Nur Textregeln ohne Origin
bleiben ausdrücklich ungebunden. Vollständige Byteabdeckung beweist weder die
richtige Interpretation des Auftrags noch seine funktionale Erfüllung.

`validate_successor_of` prüft eine unmittelbar folgende Goalrevision ohne Laden
der gesamten Historie. Revisionserzeugung und History verwenden dieselbe
inhaltliche No-op-Regel: identische Originalbytes, Aussagen, Bereiche und
Nachweispflichten werden durch das bloße erforderliche Owner-Rebind nicht zu
einer inhaltlichen Änderung. Echte Änderungen von Original, Bereichen oder
Pflichten bleiben materiell. Source-beteiligte Store-Appends prüfen die Paargrenze
vor dem Schreiben; historische reine Legacy-Storepfade bleiben unverändert.

`AdmitAgentGoalExecution` verlangt vor Agentenarbeit eine valide und vollständig
zugeordnete Originalquelle. Der ProductionExecutor prüft dies nach den
Goal-/Ledger-Ankern und vor Provider, Recoverytrigger oder wartenden Aktionen;
`ExecuteAgentTurn` prüft es vor Compiler, Provider und Lesewerkzeug erneut.
Ungelöste Lücken und ungebundene Regeln bleiben weiterhin speicherbar, erzeugen
aber keine Agentenausführung. Eine originlose saubere Baseline bleibt bis P11 auf
ihrem bisherigen Pfad; verwaiste Origins werden nicht als Legacy zugelassen.
Die getrennte Reconciliation und ihre bestehenden Cleanup-Owner bleiben erhalten.

Der einzige `DeterministicAcceptanceVerifier::verify` prüft vor Evidence-Reads
jede Originalbindung, die vollständige strukturelle Zuordnung und die ursprüngliche
Nachweispflicht jedes zugeordneten aktiven Schritts. Das ist eine zusätzliche
Admission des Kandidatenvertrags, kein zweiter Verifier- oder Produktionspfad.
Saubere Legacy-Goals durchlaufen dieselbe bisherige Evidenceprüfung. Verwaiste
Origins dürfen nicht in diesen Legacy-Zweig fallen. Offene Zuordnung wird als
`InsufficientEvidence`, eine abgeschwächte Spec oder fehlende Runtime-Rezeptfähigkeit
als `CriterionFailed` abgewiesen. Danach bleiben die vorhandenen Prüfungen für
semantische Evidence, Freshness und offene Hypothesen erforderlich. Source-Goals
verlangen zusätzlich den exakten aktuellen Evidence-Snapshot; unveränderte
Testdatei-Dependencies dürfen eine Änderung am getesteten Produktionscode nicht
überdecken. Der bisherige Legacy-Freshnesspfad bleibt bis zum Cutover unverändert.
Kommende Source-Projektionen müssen dieselbe Snapshotbindung vor Modellarbeit
anwenden; der Abschlusscheck allein öffnet alte Schritte noch nicht automatisch
wieder. Die heutige
Ablehnung führt noch nach Execute; ein crashsicherer rollierender Planungshorizont
ist dadurch nicht bereits implementiert.

Original, Teile, Kriterien und Herkunftspflichten werden in derselben vorhandenen
`IMMEDIATE`-Goaltransaktion geschrieben. Die Headeranlage schließt die zuvor
geschriebenen Goalmetadaten; UPDATE-/DELETE-Guards und deklarierte Kindanzahlen
verhindern nachträgliche Umdeutung oder Erweiterung derselben Revision. Neue
Nutzerziele erhalten eine neue Revision. Legacy-Daten werden nicht nachträglich
umgeschrieben. Beim Lesen werden Digest, kanonische Teile, Reihenfolge, gesamte
Bytezahl, UTF-8-Grenzen, Kriterienzuordnung und verwaiste Datensätze erneut geprüft.
SQL-Projektionen begrenzen BLOB-/Textpayloads vor der Rust-Dekodierung, auch wenn
eine extern beschädigte Datei ihre CHECKs verletzt. Diese Grenzen behaupten
keinen Schutz gegen einen Hostprozess, der Datenbank und Guards beliebig ersetzt.

`MaterializeOriginalTask` prüft Original- und Specbindung vor dem ersten Write und
materialisiert einen Horizont mit einem bis drei Schritten über die bestehenden
Goal-/Ledger-Ports. Die beiden Ports besitzen keine gemeinsame Transaktion:
scheitert der Ledgerinsert, bleibt der gültige Goal mit vollständigem Original
dauerhaft erhalten. Ein ausdrücklicher späterer Aufruf darf nur genau identische
Goal- und anfängliche Ledgerdaten wiederverwenden; kein automatischer Retry und
keine Überschreibung aktiver oder abgeschlossener Arbeit. Die Rückgabe ist keine
Ausführungsfreigabe; ein späterer Runstart muss seine aktuellen Anker revalidieren.

`ExistingTest` kann nicht durch Command/Lint, Diff, Deferred, einen anderen
Command/Selector oder geringere Mindestabdeckung ersetzt werden. `ChangeOnly`
fordert genau seine deklarierte Diffrelation und behauptet keine Laufzeitwirkung.
`RuntimeNeedsRecipe` bleibt offen. Noch ungeplante Kriterien werden aus Goal und
Ledger abgeleitet, nicht in einer zweiten Aufgabenautorität gespeichert.
Vollständiger modellgestützter Intake und rollierender Horizont bleiben weitere
P2-Arbeit; der Speicherschnitt allein belegt
keine Modellinterpretation, Ausführung oder erfolgreiche Aufgabenlösung.

Der native `AlreadySatisfied`-Schnitt führt zwei bereits vorhandene
Python-Unittestfälle über echte Discovery, Commandbestätigung, den tatsächlichen
Ask-Request und `GrantPolicyApproval` aus. Der bestehende Controller erzeugt über
ProcessRunner und BuiltinFactory die Test-Evidence und ihren Ledgernachweis;
nach vollständigem Reopen prüft der einzige Acceptance-Verifier diese Daten und
Done wird journalisiert und erneut geladen. Projekt- und Testdateien bleiben
bytegleich. Kein Passed-Marker oder Prozessresultat wird dafür frei konstruiert.
Das qualifiziert `All` mit Mindestfallzahl; echte `Exact`-Testnamen werden von der
bisherigen Factory weiterhin nicht geliefert. Die beiden Test-Devdependencies
`a3-workspace` und `a3-repo-index` verwenden vorhandene Workspace-Implementierungen
für reale Prozess-/Gitadapter und Indexaufbau; es kommt keine neue Bibliothek hinzu.

Die weiteren nativen Verbraucher unterscheiden fehlgeschlagene Ausführung und
Erfolgsnachweis: falsche Ganzzahldivision führt über einen echten Testprozess zu
FailedExit und einem weiterhin offenen Schritt. Die bisherige Factory erzeugt
dafür keine negative Test-Evidence; Process-/Inspectiondaten dürfen nicht als
erfolgreiche Verifikation ausgegeben werden. Nach Reopen verhindert der
unvollständige Ledger die Acceptance-Anfrage.

Der Änderungsschnitt durchläuft echten Patchpreview, exakte Genehmigung,
Dateiänderung, Indexpublikation und den vorhandenen echten Context Compiler.
Ein Test-Schritt bleibt nach dem Patch InProgress und ohne erfolgreichen
Verifikationsbeleg. Erst der bekannte Testauftrag, ein neu entdeckter aktueller
Catalog samt neuer Bestätigung und eigener exakter Prozessgenehmigung führen
zu zwei echten Tests und Source-Done nach vollständigem Reopen. Ein Patchgrant
oder eine stale Allowlist werden nicht wiederverwendet. Nur die Zieldatei ändert
sich; Testdateien bleiben bytegleich. Dieser Komponententest verwendet die
bestehende 16k/4k-Native-Contextfixture ohne Modellaufruf und qualifiziert weder
das neue Phasenprotokoll noch ein 4k-Modellprofil.

Der direkte Workspace-Tokio-Devlink liefert den tatsächlich vom bestehenden
Checkpoint verlangten Runtime-Handle und Timer. Die scoped current-thread-
Runtime gehört der Native-Testfuture und endet vor dem Windows-Child-Erfolg.
Es wird weder eine neue Bibliothek/Version noch eine Lockerung der produktiven
Checkpoint-/Timeoutgrenze eingeführt.

V45 ergänzt `goal_pending_intakes` innerhalb desselben Goal-Stores. Pending enthält
Task-/Worktreebindung, exakte unveränderte UTF-8-Bytes bis 64 KiB, Digest,
Erstellzeit und eine positive CAS-Version. Die vorhandenen 2048-Byte-Teile werden
beim Laden deterministisch aus denselben Bytes rekonstruiert. Pending besitzt
keinen GoalDraft, keine Kriterien, keinen Ledger und keinen Run. Session-/Queue-
Löschung kann diese Autorität nicht kaskadierend entfernen.

`CaptureGoalIntake` validiert die Rawbytes vor dem ersten Portaufruf. Capture ist
create-only, auch bei identischem wiederholtem Input. Gegenseitige SQL-Insertguards
reservieren die Taskidentität: normales Goal-Create und `MaterializeOriginalTask`
dürfen Pending nicht umgehen. Nach einem verlorenen Commit-Acknowledgement wird
explizit geladen, nicht erneut erzeugt. `load_goal_intake` liefert in einer
konsistenten Lesesicht Pending oder die tatsächlich vorhandene Initial-Goalreferenz.
Letztere behauptet keine frühere Capturehistorie; doppelte Autorität ist ungültig.

`PromoteGoalIntake` prüft dieselbe Task-/Initialreferenz, Capturezeit, exakten
Originalbytes und Digest sowie gültige vollständige Herkunftsdeckung. Der Adapter
wiederholt dies innerhalb derselben `IMMEDIATE`-Transaktion, konsumiert Pending
per CAS-Delete und verwendet danach den bestehenden Task-/Goalwriter. Jeder Fehler
nach dem Delete stellt durch Rollback Pending vollständig wieder her. Beim Commit
existiert genau eine Rawtext-Autorität: Pending oder der unveränderliche Ready-Goal.
V43-/V44-Tabellen, FKs und Header-Versiegelung werden dafür nicht gelockert.

Die Capture-/Promotion-Scheibe führt null Modellaufrufe aus. Vor einer späteren
Intake-Ausführung fehlen noch ein dauerhaftes Fortschritts-, Verbrauchs-, Deadline-
und Besitzerjournal. Eine CAS-Version ersetzt keinen Modellverbrauch. Pending darf bis
dahin nicht durch den alten Modellpfad verarbeitet werden. Die Produktions-
Neueinrichtung und Umschaltung bleiben P9/P11 vorbehalten.

`GoalContractStore::list_pending_goal_intakes` findet Pending ohne gespeicherte
Session-Task-ID wieder. Eine Seite enthält höchstens 32 streng nach Task-ID
geordnete Identitäten; ein einzelner Lookahead bestimmt den nächsten Cursor.
Die SQL-Projektion liest ausschließlich begrenzte IDs desselben gebundenen
Worktrees, keine Originalbytes. Alte Adapter melden ausdrücklich
`UnsupportedSchema`. Jede Seite hat eine eigene konsistente Lesesicht; ein neuer
Durchlauf ab `None` findet auch später eingefügte kleinere IDs. Die Liste ist
keine Ausführungsfreigabe und validiert keine Rawtext-Autorität: vor Verarbeitung
muss ein taskgebundener Load Pending und Original erneut prüfen. Eine inzwischen
promovierte Task darf zwischen Liste und Load verschwinden.

V46 trennt Fortschritt und abschließende Nachweise an bestehenden Schritten:
`task_step_addressed_criteria` speichert Addresses; die bisherige
`task_step_acceptance_criteria` bleibt Proves. Beide Rollen referenzieren dieselbe
Goalrevision und vorhandene Kriterien. Sie sind disjunkt, kanonisch geordnet und
zusammen auf 64 Kriterien pro Schritt begrenzt. Der bestehende Ledgerwriter
speichert beide atomar und liest sie auch nach vollständigem Reopen wieder ein.
Insert-/Updateguards verhindern Rollenüberschneidungen. Ein älterer Writer weist
Addresses vor dem ersten Ledgerwrite mit `UnsupportedSchema` zurück.

`AdmitSourceTaskStep` verlangt für Source-Schritte eine gültige Zuordnung und
eine ausführbare eigene Verifikation. Nur Proves muss zusätzlich die unveränderte
globale Nachweispflicht erfüllen. Materialisierung und Agent-Admission verwenden
dieselbe Regel. Membership und Kontextdarstellung erhalten beide Rollen; die
abschließende Acceptance und ungeplante Nachweispflichten zählen ausschließlich
Proves. Eine erfolgreiche lokale Diffprüfung kann deshalb einen Addresses-Schritt
abschließen, ohne eine funktionale Anforderung als erfüllt auszugeben.

Der native Rollenverbraucher belegt diese Grenze mit echter Patchgenehmigung,
Dateiänderung und Diff-Evidence: der lokale Schritt wird Completed, globale
Acceptance bleibt `InsufficientEvidence`. Erst ein späterer Proves-Schritt mit
zwei tatsächlich ausgeführten bestehenden Tests erlaubt Source-Done nach Reopen.
Testdateien bleiben bytegleich. Der Replan wird in diesem Komponententest explizit
gewählt; automatische Horizontplanung, Modellinterpretation und Modellqualität
sind damit nicht nachgewiesen.

V47 ergänzt einen einzigen optionalen `PendingPlanning`-Checkpoint am Ledger.
Er bindet Run, Goalrevision, Basis-Ledgerrevision, Snapshot sowie ID und Sequenz
des tatsächlichen Verify-nach-Plan-Ereignisses. Diese Ereignisidentität ist der
dauerhafte Corejob; prozesslokale Scheduler-IDs sind keine Wiederanlaufautorität.
Der Zustand ist `NeedsPlan` oder `PlanAppended` mit der nächsten Revision und
höchstens drei tatsächlich angehängten Schritt-IDs. Offene globale Proofkriterien
werden aus dem gebundenen Goal/Ledger abgeleitet, nicht doppelt gespeichert.

Der vorhandene `AgentActionStore` hält drei getrennte atomare CAS-Grenzen:
Planung anfordern, neue Definitionen anhängen, ersten Ready-Schritt aktivieren.
Bis zur Aktivierung bleibt der Marker bestehen. Ein Crash nach dem Append führt
deshalb zur Aktivierung gespeicherter Definitionen und keinem zweiten Plan.
Die bisherige Ledger-first-Schreibreihenfolge bleibt; die Request-Event-FKs sind
deferred. Der intrinsische Ledgerreader benötigt das später im selben Commit
geschriebene Ereignis noch nicht. Nach Run/Event und vor Commit prüft der Adapter
das vollständige Paar gegen den tatsächlichen alten Ledger-/Run-CAS.

Normales Ledger-Create/-Replace darf den Marker nicht anlegen, verändern oder
entfernen und keine neue `HorizonExtension` erzeugen. Ein anderer Run darf die
ausstehende Arbeit desselben Tasks nicht übernehmen. Auch der bestehende atomare
Recoverywriter prüft die Paartransition. Stale oder terminale Marker bleiben als
inertes Audit lesbar; aktive Fortsetzung verlangt Originalsnapshot, aktuellen
PublishedIndex, richtige Ownerbindung und ausführbare Schrittform. Das exakte
Requestereignis wird an seiner gespeicherten Sequenz geladen, unabhängig von
einem auf 64 Ereignisse begrenzten Darstellungsausschnitt.

`HorizonExtension` und `ErrorCorrection` sind Core-bestimmte Revisionsursachen.
Neue IDs oder andere Freitextformulierungen zählen nicht als Fortschritt:
der aktuelle Materialvergleich verwendet Rollen und typisierte Prüfpflicht.
Unterschiedliche Fenster derselben Datei sind darin noch nicht darstellbar;
deren konkrete Sourcewindow-Pflicht folgt mit P4/P5. Alle bisherigen Schritte,
verbrauchte Zähler und ursprüngliche Runzeitgrenze bleiben erhalten. Eine reine
Acceptance-Anfrage darf bei exakt verbrauchtem Modellbudget noch den einzigen
Verifier aufrufen; neue Arbeit sowie echte Budgetüberschreitungen bleiben gesperrt.

Die nativen Transaktionskontrollen verwenden vorbereitete Completed- und
Usage-Metadaten. Ein separater Vier-Schritte-Verbraucher führt drei genehmigte
Dateiänderungen aus, öffnet den Store sowohl nach der Plananforderung als auch
nach dem Anhängen des Plans neu und aktiviert dann den gespeicherten Testschritt.
Zwei bestehende Tests belegen alle vier Originalkriterien. Die verwendete
nichtleere Usage ist kontrollierte Ausgangshistorie. P4 ergänzt dauerhafte
Job- und Verbrauchsreservierungen vor dem ersten Provideraufruf.

V48 speichert den `GoalIntakeReview` als append-only Verlauf im selben Goal-Store.
Jede Revision versiegelt einen tatsächlich bearbeiteten Originalbereich von
höchstens 2048 Bytes zusammen mit bis zu vier typisierten Vorschlägen und einer
offenen Frage. Die Revision zählt maximal 128 Batches und bildet den separaten
Review-CAS. Die Capture-Version bindet weiterhin den exakten Originalauftrag.
Der Adapter revalidiert beide Versionen sowie den unveränderten Verlauf innerhalb
derselben `IMMEDIATE`-Transaktion und schreibt ausschließlich den neuen Batch.

Vorschläge halten Ergebnisanforderungen, Constraints, Nicht-Ziele und ausdrücklich
im Original enthaltene Entscheidungen mit ihrer jeweiligen Quelle fest. Offene
Fragen haben Core-vergebene Identitäten und eigene Quellanker. Die SQL-Dekodierung
prüft vor der Payloadabfrage die Anzahlen, bis zu 64 KiB Vorschlags- und Fragetext
und weitere 64 KiB für ChangeOnly-Pfade. Jeder Batch versiegelt seine Kindzeilen.

Die Promotion lädt in ihrer Schreibtransaktion den tatsächlich letzten Review.
Sie prüft bearbeitete Originalbereiche, offene Fragen und den Erhalt aller
erfassten Vorschläge zusätzlich zur bestehenden Originalabdeckung. Ein breiter
Kriteriumsbereich ersetzt einen erfassten Constraint nicht. Nach der Promotion
liest der Auditpfad die erhaltenen Reviewzeilen gegen den Originalauftrag von
Goal 1. Bis dahin bleibt Pending die Rohtextautorität. P4 ergänzt die gebundene
Modellinterpretation und die auditierbare Konkretisierung von Nachweispflichten.
P9 erfasst tatsächliche Nutzerantworten als geordnete Quellen im Goal-Lebenszyklus.

## P4-Kandidat mit dauerhafter Editreservierung

V49 reserviert den ersten Edit-Aufruf und den höchstens einen Strukturrepair im
bestehenden worktreegebundenen Knowledge-Store. Die `IMMEDIATE`-Transaktion prüft
den aktuellen Goal-Kopf, Run-Sequenz und Modellprofil, Ledger-CAS, aktiven
Schrittversuch, Verification-Spec, jüngste Publikation und ausstehende
Mutationsreconciliation. Erst der Commit erlaubt den einmaligen Provideraufruf.
Eine Reservierung hält die ursprüngliche Run-Frist, den Job-Digest, den
Digest und die Größe der tatsächlichen Wire-Anfrage sowie konservative
Prompt- und Ausgabetokenreserven.

Die einmalige Ergebnisbeobachtung ergänzt nur Status, Zeit und gemeldete
Tokenzähler. Vollständige Antworten und reparierbare Strukturfehler prüfen die
aktuellen Anker nochmals. Veraltete Antworten, unvollständige Usage und
Budgetüberschreitungen liefern keinen verwendbaren Modellvorschlag. Der Adapter
behält je Tokendimension mindestens die Reservierung und gegebenenfalls den
größeren gemeldeten Verbrauch. Eine offene oder unbekannte Reservierung kann
nach Reopen nicht erneut versendet werden. Ein Strukturrepair braucht denselben
logischen Job, einen typisierten Strukturfehler und einen neuen Wire-Digest.

Die Reservierungstabelle speichert keine Prompts, Quellfenster oder Modellantworten.
Ihre Zähler unterscheiden gebuchte Aufrufe von tatsächlichen
`AgentRunUsage`-Interaktionen. Der erste Verbraucher ist der aktive Editauftrag.
Intake-Verbrauch, andere Phasen und die unabhängige Werkzeugabrechnung gehören
zur weiteren P4-Abnahme.

V50 bindet vor dem ersten Intake-Aufruf das Modellprofil und das konfigurierte
Auftragsbudget an Worktree, Pending-CAS, Originaldigest und Erfassungszeit.
Die ursprüngliche Frist beginnt bei der Erfassung. Ein Aufruf bindet seinen
exakten Originalbereich und die Review-Vorgängerrevision. Eine akzeptierte
Beobachtung und der dazugehörige Review-Append werden in derselben
`IMMEDIATE`-Transaktion bestätigt. Ungeklärte Aufrufe bleiben belastet und
sperren die Promotion.

V51 reserviert den strukturellen Einzelrepair für dieselbe ursprüngliche
Fensterbindung und denselben Job mit verändertem Wire-Digest. Der Initialaufruf
muss als strukturell ungültig beobachtet sein. Ungeklärte Aufrufe und fachlich
zurückgewiesene Vorschläge qualifizieren sich nicht. Ein neuer Erstaufruf braucht
einen disjunkten Originalbereich. Überlappungen bleiben auch nach einer anderen
Review-Revision gesperrt. Die initialen und reparierenden Aufrufe zählen in
derselben Frist und demselben Auftragsbudget.

V52 bindet die initiale Planreservierung nach Promotion an die aktuelle
Goal-Revision, den ursprünglichen Digest, das Modellprofil und die exakte
Indexpublikation. Sie verwendet die bereits gebuchte Auftragsfrist und das
verbleibende Modellbudget. Der Store bestätigt die beobachtete Antwort und
die Erstellung des ersten Ready-Ledgers in derselben Transaktion. Reguläre
Quellen- und Größenrückgaben verbrauchen ihren Aufruf ohne Ledger. Nach einer
Planreservierung prüft der Run-Start das unveränderte bestätigte Ledger und
seine Publikation. Die gebuchten Intake- und Planaufrufe bleiben über Reopen
und im folgenden Editbudget sichtbar.

Der bestehende Run-Start verknüpft die übernommenen Intake-Buchungen atomar
mit genau einem Run. Der tatsächliche Startzeitpunkt bleibt sichtbar. Das
Zeitlimit des Runs entspricht der verbleibenden Zeit bis zur ursprünglichen
Frist. Die Edit-Zulassung summiert Intake-, Plan- und Edit-Buchungen für
denselben Auftrag. Die Phasenbuchungen bleiben getrennt von den
Legacy-Modellinteraktionen.

Verknüpfte Kandidaten-Runs begrenzen Core-Werkzeugversuche unabhängig vom
Modell. Der Adapter prüft die aktuelle Publikation, die ursprüngliche Frist und
die Anzahl der vorhandenen ToolAttempt-Zeilen in derselben Transaktion vor
dem nächsten Versuch. Der bestehende Journalweg für einen Safe-Reader-Aufruf
liefert eine `ToolAction` mit Quellenreferenz. Erfolg, Fehler, Abbruch und
Wiederöffnen lassen die gebuchten Versuche nachvollziehbar.

V53 bindet einen Source-Read vor dem Zugriff an den bestehenden ToolAttempt,
den Schrittversuch und die aktuelle Publikation. Der Journal-Append hält den
exakten Originalbeleg, wenn der Safe Reader die gebundene Seite geliefert hat.
Eine Editreservierung kann diesen Beleg an ihren Wire-Digest binden. Der
reproduzierte Quellenfortschritt zählt neue Bytebereiche erst nach einer
akzeptierten aktuellen Beobachtung dieses Aufrufs. Ungesendete, unklare und
veraltete Aufrufe zählen ohne Liefergutschrift. Die Grenzen des Schrittversuchs
bleiben über Reopen gebunden. Der Store hält Bereichs- und Hashbezüge, die
Originalseite bleibt flüchtig.

V54 hält die einmalige gerichtete Aufrufersuche pro aktiver Deklaration und
Schrittversuch in `source_caller_relocalizations`. Der Beleg bindet denselben
ToolAttempt und dieselbe Erwerbsreihenfolge wie ein Source-Read. Ein Treffer
liefert Metadaten, die ein separater Original-Read prüfen kann. Leeres oder
unvollständiges Ergebnis sowie ein unterbrochener Versuch bleiben nach Reopen
unterscheidbar. Jeder davon verbraucht die Suche und gibt keine Bytegutschrift.
Das gemeinsame Budget zählt höchstens acht Erwerbsaktionen und stoppt nach
zwei Aktionen ohne neue aktuelle Quellbereiche. Die exakte Datenbankversion
wird am Store-Eingang geprüft. Interne Phasenadapter prüfen die Mindestversion
ihrer Tabellen.

V55 speichert nach [ADR-0121](adrs/0121-revisionsgebundener-python-ganzzahlbeleg.md)
die optionale geschlossene Quellpflicht in Intake-Finding, Kriteriumsursprung
und Test-Spec. Der versionierte Beleg eines echten Tests bindet zusätzlich
Ziel, vollständige Dateirevision und genaue Funktions- und Ausdrucksbereiche.
Der Inhaltsdigest und die Evidence-ID decken diese Felder ab. Der geladene
Quellbeleg muss zur unveränderten Pflicht und zu einer exakten
Dateiabhängigkeit des Testbefunds passen. Der Kandidat prüft denselben
Snapshot und dieselbe Dateirevision auch nach Reopen. Ein alter Test ohne
Quellpflicht behält sein vorhandenes Evidenceformat und seine Identität.

V56 bindet das geschlossene HTTP-Rezept an Intake-Finding,
Kriteriumsursprung und Verifikationsspezifikation. Der gespeicherte
Beleg trägt die exakte Run-, Schritt-, Tool-, Spec- und Snapshot-ID,
Skriptrevision, Startgrenzen, Startfreigabe und die geordneten
freigegebenen Antworten. Der Decoder kompiliert den exakten Start
aus Rezept und gespeicherten Grenzen. Er prüft die abgeleitete
Evidence-ID, alle zugehörigen Policyentscheidungen und den dauerhaften
erfolgreichen Toolabschluss mit genau diesem Ergebnisdigest. Beim
ersten Schreiben müssen Snapshot und Skriptrevision aktuell publiziert
sein. Der Decoder verwirft fehlende oder gemischte
Nachweise. Die begrenzten versionierten Erweiterungen behalten die
bestehenden Fremdschlüssel und SQL-Basistags. Die Basistags allein
können einen HTTP-Schritt nicht verifizieren. Fehlt das Rezept an
einem Kriterium, bleibt dessen Laufzeitanforderung offen.

V57 hält im frischen Kandidatenbestand zusätzlich die gebundene
Startabdeckung. Rezeptformat V2 bindet den expliziten Start, den
Standardstart mit einem ausgewählten erfolgreichen GET und beide
Fehlerexits unter einer Rezept-ID. Die eigene Prüfsumme und der
geprüfte GET-Index passen in die vorhandenen BLOB-Grenzen. Der
Einzelstartbeleg aus V56 wird für dieses Rezept beim Schreiben und
Lesen abgewiesen. Das vollständige Rezept bleibt im Ledger offen,
bis ein passender Ausführer und der gemeinsame Beleg vorhanden sind.

V58 ergänzt den vollständigen Startbeleg als versioniertes BLOB ohne neue
Tabellen. Er speichert die getrennten Freigabeentscheidungen für Reservierung,
gehaltenen und freien expliziten Start, Standardstart, ungültiges Argument
und jede Probe sowie beide regulären Fehlercodes und die Standardantwort.
Der Decoder rekonstruiert den Skriptpfad aus der unveränderlichen Spec,
prüft die gemeinsame Wirkung aus Rezept, Snapshot, Skripthash und drei
ProcessSpecs sowie sämtliche geordneten aktuellen Freigaben. Der
Unknown-Abschluss und die Evidence werden in derselben Transaktion
geschrieben. Ein gemischter oder unvollständiger Nachweis rollt vollständig
zurück. Produktionsschema V42 bleibt bis P11 eingefroren.

V60 ergänzt im ausschließlich frisch eingerichteten Kandidatenbestand einen
Präsenzmarker an `tool_run_attempts` und die append-only
`loopback_mismatch_diagnostics`. Ein separater binärer V1-Codec ist vor dem
Laden auf 400 bis 133.000 Bytes begrenzt und mit eigener Prüfsumme versiegelt.
Er hält eine vollständige abweichende HTTP-Observation, Originalphase und
Checkposition, Rezept-, Spec-, Run-, Step-, Snapshot-, Skript-, Prozess- und
Attemptbindung sowie alle höchstens 14 geordneten Freigabeentscheidungen.
Der fehlgeschlagene Tool-Journaleintrag, `Failed`/`Applied`, Marker und
Diagnose werden durch einen einzigen Run-Tail-CAS atomar gespeichert.
Der Loader prüft Präsenz beidseitig, den genauen Abschluss samt Zeit und
Digest, aktuellen Originalauftrag/Plan, Quelle, Effekt und konsumierte
Grants unabhängig. Eine regulär neue publizierte Quelle führt zu
`PublishedSnapshotConflict`; beschädigte Bindungen bleiben ein Datenfehler.
Die Diagnose gehört nicht zu `VerificationEvidence` und erfüllt keine
Kriterien. Der erste Slice bleibt terminal; auch der Crash vor dem
nachfolgenden Step-Failure-CAS erlaubt keine generische Wiederholung.
Schema V42 und die historischen Kandidatenaufbauschritte bleiben erhalten;
es gibt keinen Import oder Upgrade alter Entwicklungsdaten nach V60.

V61 wird ebenfalls ausschließlich auf leerem isoliertem Kandidatenbestand
eingerichtet. Die einzige neue Tabelle `http_correction_plan_reservations`
bindet eine aktuelle gespeicherte HTTP-Diagnose an genau einen Planjob und
höchstens einen Strukturrepair im selben nichtterminalen Replan-Run. Original-,
Goal-, Ledger-/CAS-, kausaler Journal-Tail-, Failed-Step-/Toolattempt-, aktuelle
Publikations-/Source- und Profilanker sowie vollständiger Wiredigest, Reserven
und ursprüngliche absolute Frist bleiben unveränderlich. Ein Insert ist
unbeobachtet; genau ein terminales Update hält tatsächliche Providerusage und
gegebenenfalls einen bounded Vorschlag (1–512 Bytes mit Digest). Der Anchor ist
vor Decode/Zuordnung auf 256–133.000 Bytes begrenzt; Rohrequests, Quelltext und
Providerantworten werden hier nicht gespeichert. Accepted ist gespeicherte
Absicht, keine Ledgeränderung oder Ausführungsautorität.

Reservierung und Settlement verwenden `TransactionImmediate` und laden den
vollständigen aktuellen V60-Diagnosevertrag einschließlich konsumierter Grants
im selben Readview neu. Das gemeinsame Runaccounting berücksichtigt Edit und
diese Planphase samt maximalem Zeitwert; Unknown oder unbeobachtete Aufrufe
sperren weitere Dispatches. Der kalte Loader prüft Bindungen, Codec, Usage und
Frist unabhängig sowie die höchstens zwei Zeilen umfassende Lineage: Ein
accepted Repair benötigt die gleiche Initial-Job-/Anchorbindung, einen zeitlich
vorausgehenden beobachteten Strukturfehler, einen anderen Wire und dieselbe
Frist. Ein accepted Initial darf keine zusätzliche Repairzeile besitzen.
Produktionsschema42 und die historische V60-Registrierung bleiben erhalten;
V60-Daten werden weder neu geöffnet noch nach V61 übernommen. Automatische
Mismatch-Zulassung, Plananwendung und Folgeedit bleiben außerhalb dieses Slices.

Der ausschließlich frisch eingerichtete Kandidat V62 ergänzt in derselben Tabelle
`protocol_version INTEGER NOT NULL DEFAULT 2 CHECK(protocol_version=2)` und verwendet
den strikten V2-Vorschlagscodec. Es gibt weiterhin nur Initial und einen möglichen
Strukturrepair pro Diagnose. Modelloutput und gespeicherte kanonische Vorschlagsbytes
enthalten genau Version2, Job und eine skalare Entscheidung: ein tatsächlich
geliefertes Zeilenlabel oder einen der drei Halte. Die private Core-Zuordnung zum
unveränderten Raw-Sourcebereich bleibt flüchtig. Der Fokus ist weder ein Patchfenster
noch Ledger-/Verifikationsautorität. Write, Settlement, Usage und beide kalten
Lineagezeilen prüfen Protokoll2; es gibt keinen V1-Fallback. Bounded Blobs, dieselben
vollständigen aktuellen Bindungen, einmalige Beobachtung, ursprüngliche Runfrist und
gemeinsame Budgets bleiben unverändert. V60-/V61-Registrierungen und SQL bleiben
historisch fest; V42 bis V61 werden vor einer Kandidatenschreiboperation abgewiesen.
Ein bestehender V61-Bestand erhält insbesondere keinen nachträglichen Protokollmarker.

V63 wird nur waehrend der Einrichtung eines leeren Kandidatenbestands aufgebaut.
In derselben Tabelle ersetzt der leere Aufbau den festen Protokoll2-Marker durch
protocol_version=3; bestehende 62-Daten werden vorher abgewiesen. Historische 62-SQL
und Registrierung bleiben unveraendert. Es entsteht weder eine weitere Tabelle noch
ein Import-/Upgradepfad. Der V3-Anchor bindet optional einen exakten aktuellen Task-
Lens-Kontextdescriptor desselben Skripts mit Range, Inspektion und Digest. Das
Angebot ist kein ausgefuehrter Read. Der Vorschlagsblob bleibt 1-512 Bytes; der V3-
Codec enthaelt genau Version, Job, skalare Entscheidung und eine hoechstens 192 UTF-
8-Bytes lange oeffentliche, ungepruefte Basis. Byte-/Gesamtblobgrenze, SecretCandidate
und Offer-/Choicebindung werden kalt erneut geprueft; Schema-maxLength reicht nicht.
Debug enthaelt keinen Basistext. Unsichere Basis wird RejectedReply ohne Vorschlag
und ohne Strukturrepair. Write/Settlement/Usage/beide Cold-Lineagezeilen verlangen
Marker 3 ohne V2-Fallback. Once-only, Quellen, Policy, CAS, urspruengliche Runfrist und
Accounting bleiben erhalten; kein Basisinhalt oder NeedOfferedSource ist Fact,
Evidence, Ledgeraktion, Read-/Edit-/Prozessautoritaet oder automatische Fortsetzung.

Der anschließende kalte HTTP-Korrekturanschluss verwendet diese vorhandenen Zeilen,
Ledgerrevisionen, Attempts und Journalevents; der HTTP-Protokollmarker 3 und alle
Blobbytes bleiben gleich.
Ein eigener enger Herkunftsread prüft aus einem Readview den ausgewählten
Reservierungsschlüssel gegen den Diagnoseanker, beide möglichen Phasenzeilen,
Settlementzeiten, Originalfrist und gemeinsame Kosten. Er bindet genau die
angewandte materielle Revision, den pensionierten Fehlerattempt, dessen echte
Prozess-/Cleanup-/Freigabereceipts und die fest abgeleiteten Aktivierungsereignisse.
Die vorhandenen aktuellen Vorschlags-/Mismatch-Reads bleiben unverändert streng.
Der Consumer muss die sichere aktuelle Handlerseite erneut lesen und den
ursprünglichen kanonischen Job der aktuellen Protokollversion daraus rekonstruieren; gespeicherte Hashes allein
autorisieren keinen Edit.

Nur die drei exakten Aktivierungskanten und optional ein gewöhnlicher gebundener
aktueller Zielread sind für diesen Anfangsumfang zulässig. Dessen SourceReadState
wird in derselben Transaktion aus den bestehenden Acquisitions/Receipts rekonstruiert.
Der letzte Start des Korrektursteps verwendet den bestehenden aktuellen
Published-/Ledger-/Run-CAS. Es gibt keinen neuen historischen Erfolgsbeleg,
Freshnessverzicht oder erneuten Budgetrahmen. Eine frühere Fehlerdiagnose bleibt
Fehlerdaten; erst ein neuer vollständiger HTTP-Beleg könnte das Originalkriterium
schließen. Der spätere Verlust einer noch flüchtigen akzeptierten Patchaktion ist
durch diesen Herkunftsread nicht als vollständige Crash-Recovery gelöst.

Der erste Edit-V2-Kandidat trug die frische Bestandsversion V64 und den
Edit-Jobdigest-Domänenschlüssel `a3 edit phase window v3`. Das Modell erzeugt nur
`code`, eine geschlossene `need`-Wahl oder `tooLarge`; interne Protokoll-/Job- und
Fensterbindungen bleiben im Core. Bereits reservierte Providerwires behalten
ihre exakten Digests, Fristen und das gemeinsame einmalige Kostenaccounting.
Ein fehlgeleiteter gebundener Reply darf keinen anderen Job abschließen.

V64 wird ausschließlich beim Aufbau eines leeren Kandidatenbestands registriert.
Die Editpersistenz speichert bereits Anker, Wires und Outcomes statt roher
Modellantworten; daher benötigt diese Versionsgrenze keine neue Tabelle oder
Spalte. Ein bestehender V63-Bestand wird vor Schreiboperationen als inkompatibel
abgewiesen und erhält keine nachträgliche V64-Registrierung. Es gibt weder
V1-Replyfallback noch Importer, Upgrade oder Neuinterpretation alter Reservierungen.
Historische Versionen und P0-Artefakte bleiben reproduzierbar; Produktionsschema42
bleibt bis zur einmaligen geprüften P11-Umstellung unverändert. Recovery neuer
neuer Runs, CAS, Unknown-Sperre und erhaltene Budgets bleiben verbindlich.

Der V65-Aufbauschritt bindet HTTP-Plan V4 an dasselbe vorhandene
Reservierungsmodell. Sein kanonischer Vorschlag enthält genau `version`, `job`,
`decision` und `intent`; der eigenständig validierte Intent umfasst höchstens
96 UTF-8-Bytes unveränderten einzelnen Text ohne führenden oder abschließenden
Whitespace. Er ist ungeprüfte Planabsicht und
weder Tatsachenbeleg noch Freigabe. Die materielle Korrekturidentität bindet neben
Job und Focus zusätzlich diese exakten Bytes. Die vollständige Stepdefinition
übernimmt sie als beabsichtigtes Ergebnis; der abhängige Proves-Schritt behält
sein unverändertes vollständiges HTTP-Rezept und seine Originalabdeckung.
Kalte Reads rekonstruieren den kanonischen V4-Job und vergleichen Vorschlag,
Identitäten und vollständige Stepdefinitionen unabhängig. Ein geänderter Intent
oder eine alte öffentliche V3-Basis darf keine neue Revision autorisieren.

V65 wird nur in einem leeren Kandidatenbestand aufgebaut. Bestehende V64-Daten
werden vor Schreiboperationen als inkompatibel abgewiesen und nicht aktualisiert.
Der neue leere Aufbauschritt ersetzt ausschließlich den festen
`protocol_version`-Marker derselben Reservierungstabelle durch `DEFAULT 4` und
`CHECK(protocol_version=4)`. V65 fügt keine zusätzliche Tabelle hinzu. Write, Settlement,
Usage und beide möglichen kalten Lineagezeilen verlangen Protokoll4.
V63-/V64-Registrierungen und eingefrorene Belege bleiben historisch erhalten.
Es gibt keinen V3-Fallback, Importer oder Neustart alter Jobs. Edit V2,
Produktionsschema42, ursprüngliche Fristen, Once-only-Settlement, CAS,
Unknown-Sperre und gemeinsames Kostenaccounting behalten ihre Verträge.

V67 ergänzt für einen frischen isolierten Kandidaten ausschließlich den engen
NoChange-Abschluss eines tatsächlich byteidentischen bestehenden Replace-Fensters.
Es gibt keinen V66-Importer und keine Übernahme alter generischer Ablehnungen.
Produktionsschema V42 und eingefrorene V66-Versuche bleiben unverändert erhalten.
Die älteren Initialisierungsschritte einschließlich V66 bleiben historisch gleich;
V67 richtet nur eine leere neue Datenbank ein.

`edit_phase_reservations` erhält das geschlossene optionale `terminal_reply_kind`.
Neue Reservationen beginnen mit NULL. Nur der spezielle Known-Finish-Pfad setzt
`no_change` gleichzeitig mit seiner ersten `rejected_reply`-Beobachtung. Die
bestehende Observe-once-Sperre und zusätzliche Trigger verhindern späteres
Promoten einer bereits generisch abgelehnten Zeile. Das allgemeine Finish und
Plan-/Context-Beobachtungen verleihen diese Autorität nicht.

Die neue append-only Tabelle `edit_phase_no_change_receipts` bindet ihren
kompositen Schlüssel an die tatsächlich beobachtete Editreservation und per
Fremdschlüssel an deren bereits belasteten SourceRead. Normalisierte Felder
halten Formatversion, tatsächliche IndexRunId, vollständige Profilreferenz,
Generierungs-/Kontextlimits, ursprünglichen Goal-Digest, FileRevision/Pfad,
exakte Fenstergrenzen und Fensterdigest sowie den echten Originalbereich.
Modellcode, rohe Antworten und Ersatzbytes werden nicht gespeichert. Die
IndexRunId ist bewusst kein Delete-Fremdschlüssel zu regenerierbaren Indexzeilen.
Der Receipt-Insert verlangt das terminale NoChange-Tag und positive bekannte
Usage; Update und Delete des Receipts sind gesperrt. Tag und Receipt entstehen
innerhalb derselben vorhandenen `Immediate`-Finish-Transaktion oder gar nicht.

Der bestehende Live-Loader `load_edit_no_change` authentifiziert ausschließlich
den noch aktiven exakten Replacejob: unveränderten Originalauftrag, aktuellen
Run/Ledger/Versuch, genaue Publikation und Quelle, tatsächlichen erfolgreichen
Original-/ToolAction-Beleg, Window/Job/Wire und Initial-/Repairhistorie. Die reale Generierungsgrenze bleibt
von der größeren22-Prozent-Reserve getrennt. Fehlende/korrupte Records, fremde
Anker, neuere Calls, Unknown oder abgelaufene Originalfrist verhindern den
Abschluss. Ein NoChange-Tag ohne Receipt ist ein Integritätsfehler, kein Fallback
auf eine generische Ablehnung. NoChange ist keine angenommene Quellenlieferung,
keine VerificationEvidence und erlaubt weder Strukturrepair noch neue Gutschrift.

Der bestehende indexgebundene Ledger-/Journal-Commit erhält einen engen
NoChange-Zweig. In derselben `Immediate`-Transaktion prüft er den Receipt erneut,
vergleicht genau den deterministisch erwarteten Failed-Ledger/Execute→Replan-Run
und das eine Ereignis mit dem tatsächlich geladenen alten Paar und beendet den
alten Read durch `abandoned=1` ohne Credit. Exakte IndexRunId und Snapshot sowie
beide CAS-Dimensionen bleiben gebunden. Ein Fehler rollt Read-Abandon, Ledger und
Journal gemeinsam zurück; es gibt keinen separaten Vorab-Abandon oder zweiten
Commit. Alle Modell-/Toolkosten und der Originalauftrag bleiben erhalten. Dieser
Slice erzeugt keinen Folgeplan und keinen weiteren Modelljob. Seine Prüfungen
und technische Abnahme sind in
`target/plan22-p7-runs/edit-nochange-final-seal-001.json` belegt. Die fachliche
Modellabnahme des Folgepfads und P7 bleiben offen.

Die reine B1-Lesevorstufe verwendet weiterhin genau diese V67-Zeilen; sie ergänzt
keine Tabelle, keinen Codec und keinen Schemaaufbauschritt. Auf dem vorhandenen
`HttpCorrectionPlanStore` erhält `load_closed_no_change_http_origin` einen
fail-closed Default. Ein privater geschlossener SQL-Lesepurpose authentifiziert
die echte erste NoChange-Receipt samt `rejected_reply`/`no_change`, genauem Failed-
Versuch, deterministischem Execute→Replan-Abschluss und `abandoned=1` des einzelnen
Originalreads. Alle sechs Bereichskoordinaten, SourceRead-Scope, erfolgreicher
Toolattempt/ToolAction und unveränderte Goal-/Run-/Ledger-/Profil-/Publikationsanker
müssen übereinstimmen; daraus entsteht kein angenommener Delivery-Credit.

Der ursprüngliche echte HTTP-Failure F und der unveränderte DirectV4- oder echte
S1-/ContextV1-Produzent bleiben getrennt belegt. Für dessen tatsächlichen
`application_tail` P gilt Direct P=F, Context P=F+1: erstes Material P+1,
drei Aktivierungen P+2..P+4, wirklicher TargetRead P+5 und A-Abschluss P+6.
Ledgerrevision und Run bleiben erhalten; die geschlossene Storeversion ist genau
alte Editversion+1. Initial und höchstens ein echter StructuralRepair müssen
vollständig beobachtet, positiv belastet und durch tatsächliches Generationlimit,
Wire-Reserven, gemeinsames ursprüngliches Runbudget und absolute Originalfrist
begrenzt sein. Fehlende/korrupte Herkunft ist ein Fehler und kein älterer Fallback;
Unknown, unbeobachtete Calls und unreconciled Mutation bleiben sperrend.

Die zurückgegebenen `ClosedHttpNoChangeOrigin`-Werte sind beschreibende Formen,
keine SQLrows oder Ausführungsberechtigungen. Der Adapter prüft gespeicherte
Herkunft; tatsächliche aktuelle Dateibytes erfordern separat SafeRead der exakten
Originalseite und `replay_closed_no_change_edit_job`. Keine der beiden Prüfungen
ersetzt die andere. Die bisherigen Live-/Current-, Integer-/ersten F- und
Unknown-Grenzen bleiben unverändert. B1 schreibt weder Quellencredit noch neue
Reservation, Ledgerrevision oder Journalereignis und führt keinen Modelljob aus.
Die technischen Gates dieser Lesevorstufe bestehen auf dem eingefrorenen Stand:
Windows 2149/0/47 über 103 Ziele, Linux 288/0/21 über dieselben 18 Befehle in vier
getrennt besessenen 900s-Segmenten. Die Zahlen stehen für bestandene,
fehlgeschlagene und ignorierte Testausführungen. Vier native Gruppen mit 30
kontrollierten Varianten prüfen echte kalte Herkunft und negative Bindungen;
zehn unabhängig vor dem Umbau erfasste öffentliche v3-Edit-IDs sind danach
bytegleich. Format, Workspace-Clippy mit verbotenen Warnungen, separater
Verbindungstest, Links und Whitespaceprüfung bestehen. Die Belege liegen in
`target/plan22-p7-runs/closed-nochange-b1-mechanical-gates-001.json`.
Eine folgende kleinere Planung, ihr eigener Read/Edit und die fachliche
Modellabnahme bleiben davon getrennte offene Arbeit; P7 ist nicht abgeschlossen.

### V68: eigener enger V5-Call und Materialanschluss nach A

Der frische isolierte Kandidat68 ergänzt `http_no_change_replan_reservations`
mit festem Protokoll5/Anchor1. Der Primärschlüssel
`(run_id, closed_job_digest, closed_call_kind, call_kind)` erlaubt einen Initial
pro authentischem A-Ursprung und höchstens einen echten Strukturrepair. FK und
unveränderlicher begrenzter Anchor binden die67-Receipt, Abschlussereignis,
Originalauftrag, realen F-/DirectV4-/ContextV1-Produzenten sowie alten Job und
Read. Wire, Reserven, Profil und ursprüngliche absolute Frist bleiben erhalten;
alle Kosten gehen in die bestehende gemeinsame Usage desselben Runs ein.

Insert ist unbeobachtet; genau ein terminales Update ergänzt Outcome und Usage.
Identitätsänderung, Delete und Refund sind gesperrt. Nur der erste tatsächliche
InvalidStructure erlaubt Repair mit gleichem Job/Anchor/Deadline und anderem
Wire. AcceptedReply, InvalidStructure und `no_progress` brauchen bekannte
positive Kosten innerhalb der echten Generationcap. Bei vollständiger gleicher
Auswahl speichert `no_progress` die begrenzte kanonische Auswahl samt Digest;
diese semantische Sackgasse öffnet keinen Repair. Echte kleinere Window-Auswahl
oder Halt erhalten kanonische Metadaten ohne Quelltext; Rohrequest, Rohreply und
SecretCandidate werden nicht gespeichert. Pending/Unknown bleibt sperrend.

Der neue Cold-Read darf nur seine eigenen höchstens zwei V5-Zeilen über den
exakten A-Abschluss N=P+6 hinaus authentifizieren. B1s unveränderte Latest-Finish-
Grenze, alte Current-/First-F-/Integer-/Definition- und Unknown-Verträge werden
nicht erweitert. SQL prüft echte Initial-/Repairgeschichte, positive Usage,
Budget, unverändertes Profil und absolute Originalfrist. App prüft getrennt nach
SafeRead die tatsächlichen alten Dateibytes, historischen v3- und V5-Job sowie
alle sechs ausgewählten Koordinaten und den Textdigest. Kein Shape-DTO ersetzt
eine dieser Prüfungen oder erzeugt Dispatch-/Delivery-Rechte.

Material benötigt keine weitere Tabelle. Der vorhandene `Immediate`-CAS lädt
und authentifiziert denselben echten Ursprung samt akzeptierter V5-Auswahl neu,
rekonstruiert genau eine kleinere Addresses-correction und ihren abhängigen
vollständig unveränderten HTTP-Proof und vergleicht gesamte Ledger-/Run-/Event-
Aggregate. Exakte IndexRunId, Snapshot, Ledger-Storeversion und Journalsequenz
werden vor Writes in derselben Transaktion geprüft. Der alte Failed-Schritt und
der alte ungestartete Pending-Proof bleiben samt Geschichte erhalten und werden
beide als retirierte Schritte geführt. N+1 ist die einzige Materialrevision, N+2..N+4 sind die normalen
drei Controllerkanten; N+5 ist genau ein eigener realer Target-Originalread mit
neuer Scope und ToolAction, ohne alten Credit.

Der dedizierte Applied-Origin-Read authentifiziert nur diesen engen Prefix aus
echten aktuellen Zeilen, ohne FakeExecute oder allgemeinen historischen Zugriff.
Die Material-IndexRunId bindet außerdem den Descriptor sowie tatsächlichen
Readbeginn, Original-Append und normalen Edit-Reserve/erfolgsfähigen Finish.
Ein neues Indexrun mit derselben SnapshotId ist vor diesen Grenzen stale;
Rejected-/Cancelled-/Unknown-Kosten bleiben terminal beobachtbar. Nach dem
gewöhnlichen Patch ist eine neue Publikation hingegen Bestandteil der normalen
Edit-/Publish-/vollständigen HTTP-Proofkette im selben Run.

Die Sonderprüfung bleibt auf die echte ursprüngliche B2-Step-/Ledger-/Versuch-
Scope begrenzt. A-Ledgerrevision und B2-Stepidentität werden aus dem tatsächlichen
V5-/Receipt- und Materialbezug gelesen; eine aktuelle spätere Ledgerrevision
ersetzt diesen Ursprung nicht. Andere vom bestehenden Core zugelassene Scopes
behalten ihre normalen Read-/Edit-, Horizon-, Revalidierungs- und Recoveryverträge.
Ein Reasonstring oder neue IDs allein eröffnen keinen Bypass; historische V5-
Zeilen begründen umgekehrt keine pauschale Sperre künftiger Arbeit. Budgets,
Originalauftrag und tatsächliche frühere Kosten bleiben unverändert.


Kandidat68 öffnet nur exakt68 oder eine tatsächlich leere0-Datenbank. Alte echte
42..67-Fixtures werden read-only abgewiesen; sie enthalten die historischen
SQL-/Namen-/Checksummen einschließlich67-Receipt/Tag,66-Context und65-Protokoll4,
aber noch keine68-Tabelle. Alle acht Schemaöffnungstests sind vor breiteren Gates
gezielt erforderlich. Produktionsschema42 bleibt unverändert. Diese B2-Implementierung
behauptet noch keine ausgeführten Gates, Modellabnahme oder P7-Fertigstellung.

V66 richtet ausschließlich einen frischen, exakt versionsgebundenen
Kandidatenbestand ein. Vorhandene V42- bis V65-Daten werden vor Writes read-only
abgewiesen; es gibt keinen Import oder Upgrade. Produktionsschema42 und die
historischen SQL-/Namens-/Checksumverträge einschließlich V65 bleiben unverändert.
`http_correction_plan_reservations` behält Protokoll4, Primärschlüssel und Guards.

Die separate Tabelle `http_context_intent_reservations` speichert ContextIntent V1
mit festem Protokoll1 und Primärschlüssel `(run_id, diagnostic_digest, call_kind)`
für Initial und einen möglichen Strukturrepair. Ein exakter zusammengesetzter FK
bindet die ausgewählte V4-Parentzeile; Parent-Job-/Vorschlagsdigest und der FK auf
`source_read_acquisitions` binden denselben tatsächlichen Read. Der unveränderliche
Anchor ist auf 137096 Bytes, der kanonische Ready-/Null-Reply auf 1024 Bytes mit
Digest begrenzt. Die neue Tabelle speichert keine Rohrequests oder Quelltexte.

Insert startet unbeobachtet; genau ein terminales Update darf Outcome, Usage und
gegebenenfalls Reply ergänzen. Identitäten, Quelle, Wire, Reserven und Originalfrist
bleiben unveränderlich; Delete ist gesperrt. AcceptedReply und InvalidStructure verlangen
positive vollständige Usage. Der partielle Unique-Index
`http_context_intent_one_accepted_per_source_read` erlaubt je Akquisition höchstens eine
angenommene Antwort einschließlich `intent:null`. InvalidStructure mit anschließend
angenommenem Repair bleibt zulässig. Gemeinsames Runaccounting zählt jede Reservation
einmal, konservativ mit dem größeren reservierten/gemeldeten Verbrauch; offene oder
Unknown-Calls bleiben blockierend.
Die konservative Outputreserve (bei 16k mindestens 3605) ist vom tatsächlichen
Generierungslimit (im Prüfstand 1024) getrennt. Finish übernimmt AcceptedReply-/
InvalidStructure-Meldungen über dem Generierungslimit, aber innerhalb der Reserve
als Unknown ohne Reply; über der Reserve bleibt `budget_overrun` vorrangig. Beide
Zustände behalten die Kosten und sperren weitere Calls. Der kalte Loader prüft das tatsächliche Limit
des gebundenen Profils für jede Initial-/Repairzeile einschließlich einer
vorangegangenen ungültigen Initialantwort; widersprüchliche Usage ist ungültiger Bestand.
Auch die V4/S1-Parenthistorie benötigt bei jeder Context-Fortsetzung das vollständige
unveränderliche Profil, dessen inhaltsgebundene Referenz Run und Parentanker entspricht.
Die ausgewählte angenommene Initial-/Repairzeile und jede vorausgehende ungültige
Initialzeile müssen das tatsächliche Generierungslimit einhalten. Der bestehende
ReadOrigin-Port erhält dafür `&ModelProfile`; keine neue Tabelle, gespeicherte
Ceiling-Spalte, Tagvorgabe oder nachträgliche Neuprobe ersetzt die Bindung. Derselbe
enge Lineageprüfer bedient diesen Context-Zweig mit zwingendem Profil und die
unveränderten Direct-/Origin-Aufrufer mit ihrem bestehenden Vertrag.

Der kalte ReadOrigin authentifiziert getrennt F, R = F+1 und den wirklichen
Originalmarker samt Tool-/Attempt-/Scopebindung. Ein angelegter Toolversuch wird
auch ohne verknüpften Intake-Scope gegen das Run-Aktionslimit gezählt; Unterbrechung
erstattet ihn nicht. Der Context-Loader prüft kanonischen Reply, aktuellen Job,
Parent-/Readzeile, Usage, Originalfrist und tatsächlichen akzeptierten Wiredigest
unabhängig. Erst daraus rekonstruiert er den typisierten Settlement mit Reply,
SourceDeliveryProgress und separater Primärlieferung: 719 neue Kontextbytes und
370 wiederholte Primärbytes im Prüfstand. Vorbereitung, Read oder unangenommene
Reservation erzeugen keine Liefergutschrift. Ready und Null bleiben reine Metadaten
im selben Run mit unverändertem Zustand Replan; sie autorisieren allein keine
Plananwendung oder Ausführung.

Die anschließende Core-Anwendung authentifiziert bei Context zusätzlich den echten
S1-Read und den angenommenen Ready-Settlement. `HttpCorrectionOrigin` trägt danach
explizit Direct- oder Context-Herkunft als geprüfte Daten, ohne neue Tabelle oder
persistiertes Producerfeld. Der bestehende ContextAnchorcodec1 erhält einen exakt
begrenzten Decoder mit Version, Längen, numerischen Grenzen, vollständigem EOF und
kanonischer Re-Encodierung; seine bisherige Bytefolge bleibt unverändert.

Die Materialrevision folgt auf P = F beziehungsweise das echte R = F+1. Der Store
prüft Material bei P+1, die drei vorhandenen Aktivierungskanten bei P+2 bis P+4 und
gegebenenfalls den echten Edit-Zielread bei P+5. Fehlerversuch und Prozess-/Grant-/
Cleanupquittung bleiben an F gebunden. Ein privater, nur aus tatsächlicher
Acquisition/Original/ToolAction gebildeter Read-Token erlaubt den separaten
Applied-Context-Mismatch-Purpose; der generische Current-Purpose lehnt ausgemusterte
Schritte weiterhin ab. Es gibt keinen allgemeinen Historical-/Retired-Bypass.

Der vorhandene Applied-Origin-Port verlangt das vollständige Profil. Sowohl Direct
als auch Context prüfen damit ausgewählte und vorausgehende Modellzeilen gegen
die tatsächliche Generierungsgrenze, zusätzlich zu Reserven und Originalfrist.
Der Loader authentifiziert kanonische Rows, echten Read und akzeptierten Wiredigest;
die Application prüft danach beide aktuellen Originalseiten und rekonstruiert den
Job unabhängig. Keine dieser Metadaten erzeugt neue Editlieferung oder funktionale
Evidence. Der spätere Patch und vollständige HTTP-Proof behalten ihre bisherigen
Autoritäts- und Recoverygrenzen. Die technischen Integrationsgates sind bestanden;
die gemeinsame lokale Modellabnahme bleibt im
[HTTP-Korrekturprüfstand](plans/22-P7_HTTP_CORRECTION_STATUS.md) offen.
Der dort erfasste unveränderte Ornith-Edit wird derzeit als `rejected_reply`
gespeichert. Der genaue fachliche NoChange-Grund steht nur im privaten Testbericht;
ein kalter produktiver Folgeanschluss darf diesen nicht als seine Autorität verwenden.

Der Kandidatencontroller legt den `MachineProcess`-Versuch mit
Skripthash vor dem Start als `Unknown` an. Nach bestätigtem Stop
speichert eine Transaktion den Toolabschluss, `Applied` und den
exakten HTTP-Beleg. Scheitert die Belegprüfung, bleibt der gesamte
Versuch im vorherigen Zustand. Der anschließende Ledger-/Run-CAS
prüft innerhalb seiner Transaktion den neuesten publizierten
Snapshot. Ein Versions- oder Snapshotkonflikt lässt den Schritt
offen. Ein bereits ausgeführtes Rezept am selben aktiven Schritt
fordert Recovery und sperrt eine erneute Ausführung. Die gezielte
Wiederaufnahme lädt den typisierten Beleg des exakten
`Applied`-Versuchs am Journalende. Sie schließt den aktiven Schritt
über den vorhandenen Ledger-/Run-CAS ab, sobald ein frischer
Worktree-Scan denselben Snapshot bestätigt. Die allgemeine Recovery
behält den Journalabschluss bis dahin als letztes Ereignis. Bei
bereits abgeschlossenen HTTP-Schritten prüft sie die typisierte
Evidence samt Rezept, Run, Schritt und aktuellem Snapshot erneut.

## Implementierte Speicher- und Schemaverträge der Altbaseline

## Ergänzung: Knowledge V39

Knowledge-Schema V39 implementiert den in [ADR-0108](adrs/0108-fast-index-laufinspektor.md)
entschiedenen lokalen Diagnosevertrag. `index_trace_runs` beginnt vor Discovery und bindet eine
`IndexTraceId`, monotone Revision, sicheren Auslöser, Laufzustand, Zeitwerte, aktuelle Phase und
vollständige Ergebniszähler an genau einen Worktree. `index_trace_phases` hält exakt die sechs
geordneten Phasen. `index_trace_events` speichert höchstens die bounded Core-Ereignisse und
`index_trace_files` die vollständige Menge der vom sicheren Discovery-Contract berücksichtigten
Dateien einschließlich erkannter Löschungen und höchstens acht geschlossener Diagnosecodes.

Die Tabellen enthalten weder Quelltext noch absolute Pfade, Adaptermeldungen, Providerdaten oder
Secrets. Sie referenzieren bewusst keine regenerierbaren Indexruns oder Snapshots. Ein
Fast-Index-Rebuild lässt sie daher bestehen. Batch-Upserts aktualisieren Journal und Dateizeilen;
ein fehlgeschlagener Journalwrite setzt im Lauf `details_incomplete`, beeinflusst aber die atomare
Indexpublikation nicht. Retention hält maximal einen aktiven und seinen unmittelbar vorherigen
terminalen Trace; nach Abschluss nur den neuesten terminalen. Reopen schließt aktive Traces als
`interrupted` und verwaiste `building`-Indexruns als `failed`, bevor neue Arbeit eingeplant wird.

## Ergänzung: Knowledge V38

Die bestehende immutable Tabelle `agent_replan_research_checkpoints` ergänzt eine
nullable `pending_need`-Spalte (maximal 65.536 UTF-8-Bytes, Adapterformat V1).
Sie enthält nur Q1, begrenzte lexikalische Navigationsziele und die genauen
Originalreferenzen des analysierten Pakets. Der Paketversuch bleibt im bisherigen
ResearchWorkState. Altbestand behält unveränderte Payloads und `NULL`; es wird
keine historische Beleglücke erfunden. Quellenmarker, Run/Schritt/Snapshot und
Journal werden gemeinsam validiert und atomar geschrieben. Direkte Updates und
Deletes bleiben gesperrt. Vor erneuter Kontextaufnahme werden Literale und
Originalbereiche durch die rehydrierten, hashgeprüften Safe-Reader-Seiten erneut
gebunden. Ein Bedarf ist kein Ergebnis und keine ausführbare Aktion.
Siehe [ADR-0092](adrs/0092-dauerhafter-replan-belegbedarf.md).

## Ergänzung: Knowledge V37

`agent_replan_research_checkpoints` speichert den gemeinsamen `ResearchWorkState` unter
Run, Schritt, Snapshot und einem bestehenden Journal-Event. Die unveränderlichen Zeilen
werden mit dem zugehörigen Analyse- oder Read-Event atomar geschrieben. Quellenanker müssen
exakte erfolgreich journalisierte und als Original markierte Spans desselben Runs,
Schritts und Snapshots besitzen. Ein Suchspan ohne Originalmarker reicht nicht. Ein ungültiger
Anker rollt auch das Event zurück. Originaltext, Providerantworten und Credentials werden
nicht gespeichert. Reopen stellt denselben Prüfstand einschließlich Read-Zähler wieder her.
Siehe [ADR-0048](adrs/0048-rungebundene-replan-recherche.md).

`agent_replan_originals` markiert ausschließlich die genaue Tool-Evidence-ID tatsächlich
gelieferter Originalseiten im selben Read-/Checkpoint-Commit. Die Tabelle enthält keine
Source-Bytes. Der Wiederanlauf lädt höchstens acht markierte Spans des exakten Runs, Schritts
und Snapshots und liest sie nach erneuter Hashprüfung durch den Safe Reader. Such- und
Graphmetadaten werden nicht zu Originalreads umgedeutet. Ein leerer EOF-Read erhält keine
Originalmarkierung. Direkte Updates und Deletes bleiben gesperrt.

Die ausdrücklich aktivierte Modellprüfung kann Settings aus einem bestehenden Katalog
mit SQLite-Read-only-Flags laden. Dieser Weg erstellt und migriert keine Datenbank, öffnet
keine Projektdatenbanken und liest keine Secrets; Providerauflösung nutzt den bisherigen
Credential-Adapter. Das ist keine neue WebView-Berechtigung.

Status: verbindliche Baseline  
Stand: 2026-08-29

## Entscheidung

V1 verwendet lokal eingebettetes libSQL über einen KnowledgeStore-Port. Cloud-Synchronisation ist nicht Teil von V1. Die neue Turso-Engine darf später als zweiter Adapter evaluiert werden.

Begründung:

- libSQL ist vollständig SQLite-kompatibel und von Turso als belastbare Wahl für mission-kritische Nutzung beschrieben;
- FTS und ein nativer DiskANN-Vektorindex stehen heute zur Verfügung;
- die neue Turso-Engine ist technisch attraktiv, aber Full-Text Search ist derzeit experimentell und schnelle approximative Vektorindizes stehen laut Projekt-Roadmap noch aus;
- der Port verhindert eine fachliche Bindung an eine Engine.

## Implementierter Storage-Unterbau

Der S2-Unterbau liegt im Infrastruktur-Crate `a3-storage-libsql`:

- Ein typisiertes `StorageLayout` nimmt ausschließlich einen absoluten, kanonisierten App-Data-Root an und begrenzt den globalen Katalog auf `catalog.db` innerhalb dieses Roots. `ProjectStorageLayout` leitet daraus ausschließlich über die validierte `WorktreeId` den Pfad `projects/<WorktreeId>/knowledge.db` ab. Verzeichnisse, Datenbankdatei und kanonische Elternbeziehungen werden geprüft; Symlinks und falsche Dateitypen werden abgelehnt. App-Data innerhalb des ausgewählten Worktrees ist unzulässig.
- Der Adapter verwendet die stabile libSQL-Version 0.9.29 ausschließlich mit dem lokalen `core`-Feature. Remote-, Replikations- und Synchronisationsfunktionen sind nicht aktiviert.
- Ein vorhandener Katalog oder eine vorhandene Worktree-Datenbank wird zunächst mit `SQLITE_OPEN_READ_ONLY` auf Schema-Version und Integrität geprüft. Eine unbekannt neuere Version wird vor Connection-Konfiguration oder Migration abgelehnt und nicht verändert.
- Schreibende Connections erzwingen `foreign_keys = ON`, WAL-Journaling, `synchronous = NORMAL`, einen Busy-Timeout von fünf Sekunden und `trusted_schema = OFF`; die Werte werden nach dem Setzen zurückgelesen.
- Der Adapter cached pro Projekt den geprüften lokalen Datenbank-Handle, teilt aber keine aktive
  Transaktions-Connection zwischen unabhängigen Operationen. Map-/Deep-Map-Reads, Journalwrites,
  Index-Publish und Module-Card-Publish beziehen je einen konfigurierten Operationskontext. WAL
  hält Reads während eines Writes verfügbar; konkurrierende Writer werden über den begrenzten
  Busy-Timeout serialisiert. Dadurch können Polling, Journal und Veröffentlichung weder eine
  verschachtelte Transaktion auf derselben Connection erzeugen noch einander fälschlich als
  Laufzeitfehler erscheinen lassen.
- `PRAGMA user_version` ist die monotone Schema-Version. `schema_migrations` hält zusätzlich Name und versionierten BLAKE3-Checksum jeder Migration fest.
- Jede Migration läuft in einer eigenen `IMMEDIATE`-Transaktion. Ein fehlgeschlagener Migrationskörper wird explizit zurückgerollt.
- Knowledge-Schema V1 persistiert genau eine unveränderliche Bindung aus `RepositoryId` und
  `WorktreeId`. Knowledge-Schema V2 normalisiert diese Bindung vorwärtskompatibel in `repositories`
  und `worktrees` und ergänzt unveränderliche `snapshots`, `snapshot_adapter_revisions`,
  `snapshot_changes` und `index_runs`. Knowledge-Schema V3 ersetzt die Identitäts-Fremdschlüssel durch
  `ON UPDATE CASCADE`, damit ausschließlich der kontrollierte Reconciliation-Pfad Repository- und
  Worktree-ID samt abhängigen Snapshots und IndexRuns transaktional umschreiben kann. Jede
  Wiederöffnung prüft sowohl die V1-Bindung als auch die normalisierte Projektion gegen die frisch
  inspizierte Projektidentität. Linked Worktrees erhalten weiterhin getrennte Datenbanken.
- Katalogschema V3 normalisiert stabile `projects`, veränderliche `repository_observations` und
  `recent_worktrees`. Letztere speichern außerdem die `WorktreeAnchorId`. Dauerhafte
  `worktree_reconciliations` halten den exakt bestätigten Quell-/Zielzustand und den Status
  `prepared` oder `completed`, sodass ein Abbruch zwischen Dateisystem- und DB-Schritt sicher
  fortgesetzt wird. Eine normale Projekterkennung oder ein Reconciliation-Abschluss aktualisiert
  Katalog und Öffnungsreihenfolge in genau einer `IMMEDIATE`-Transaktion.
- Katalogschema V6 ergänzt einen Index für die unbegrenzte Aktivierungsreihenfolge und eine
  externe FTS5-Projektion mit Trigram-Tokenizer ausschließlich über die sichere Root-Anzeige.
  Insert-, Update- und Delete-Trigger halten die Suche synchron; die Vorwärtsmigration baut sie
  aus vorhandenen `recent_worktrees` neu auf. Katalogreads bleiben auf feste 25er-Seiten begrenzt
  und geben nur opake Cursor aus. Autoritative Pfadbytes werden ausschließlich für einen
  Core-intern aufgelösten Aktivierungstarget dekodiert und nie als IPC-Autorität projiziert.
- `ProjectId`, `RepositoryId`, `WorktreeId`, `WorktreeAnchorId` und Remote-Fingerprints werden als
  32-Byte-Werte gespeichert. Autoritative Pfade werden auf Windows als UTF-16LE und auf Unix als rohe
  OS-Bytes gespeichert. Die separat gespeicherte Anzeigeprojektion ist kontrollzeichenfrei und auf
  32.768 Zeichen begrenzt.
- Der aus den konkreten Open- und Project-Catalog-Use-Cases abgeleitete `KnowledgeStore`-Port nimmt nur
  Domain-/Application-Typen an. Der zusammengesetzte libSQL-Adapter öffnet und prüft zuerst die
  identitätsgebundene `knowledge.db` und aktualisiert erst danach den globalen Katalog atomar. Ein
  Knowledge-Fehler erzeugt deshalb weder ein erfolgreiches Open-Ergebnis noch einen neuen
  Recent-Eintrag. Scheitert erst der Katalogschreibvorgang, darf die bereits gebundene leere
  Worktree-Datenbank bestehen bleiben; sie wird bei der nächsten Öffnung erneut vollständig geprüft.
  Weder libSQL-Typen noch SQL verlassen den Adapter.
- Der schmale `KnowledgeIndexStore` ergänzt dieselbe Application-Grenze um Snapshot- und
  IndexRun-Persistenz, ohne bestehende Project-Open-Verbraucher zu verbreitern. Snapshots werden nur
  als exakt nächste Worktree-Generation mit dem unmittelbaren Parent akzeptiert und samt kanonisch
  geordneten Adapterrevisionen und Pfadänderungen atomar angehängt. Repository-Pfade sind relative,
  slash-separierte Rohbytes; sie müssen normalisiert, traversierungsfrei und verlustlos sein. Eine
  read-only Projektion rekonstruiert den wirksamen aktuellen Pfad-/Content-Hash-Stand aus der
  unveränderlichen Delta-Kette. Sie validiert die Generation-/Parent-Kette sowie Pfad-, Änderungsart-
  und Hashwerte erneut an der Adaptergrenze und bleibt über einen Appneustart erhalten.
- Pro Worktree darf höchstens ein `building`-IndexRun bestehen. Er kann ohne Veröffentlichung als
  `failed` oder `cancelled` enden oder ausschließlich zusammen mit einem vollständigen, exakt
  passenden `IndexPublication` atomar auf `published` wechseln. Die run-gebundenen Datei-, Symbol-,
  Kanten-, Kandidaten-, Rank-, Modul-, Membership- und Repository-Card-Zeilen werden vor dem
  Statuswechsel in derselben Transaktion geschrieben.
  Leser rekonstruieren nur den jüngsten veröffentlichten Run in einer konsistenten Read-Transaktion.
  Ein erfolgreicher Ersatz entfernt die regenerierbaren Projektionszeilen älterer Runs noch in
  derselben Transaktion in Batches von höchstens 1.024 Zeilen. Ein Rollback stellt damit auch die
  vorherige Projektion wieder her; Snapshot- und IndexRun-Metadaten bleiben erhalten.
  Breite Symbol-, Kanten-, Kandidaten- und Rankzeilen werden über parametrisierte Mehrzeilen-Inserts
  mit höchstens 30.000 SQL-Parametern und 1.024 Zeilen pro Cancellation-Checkpoint geschrieben.
- Knowledge-Schema V9 veröffentlicht ausschließlich einen vom deterministischen R9-Verifier
  erzeugten `VerifiedModuleCardBatch`. Eine `IMMEDIATE`-Transaktion speichert Cards, Feldgruppen,
  Feldwerte, feldgenaue Evidence IDs, Claims, Claim-Evidence und strukturierte Prädikate gemeinsam
  mit der vollständigen File-, Symbol- oder Graphkanten-Provenienz. Run und Snapshot müssen dem
  jüngsten atomar publizierten Index entsprechen; ein zweiter Publish, eine fremde Evidence ID oder
  ein unvollständiger Lexical-Marker wird vor Mutation abgelehnt. Dieselbe Transaktion füllt
  `card_fts` und aktualisiert dessen erwartete Zeilenzahl. Cancellation, Deadline und höchstens 64
  monotone Progressereignisse werden bis unmittelbar vor Commit geprüft. Nach erfolgreichem Commit
  bleibt der Receipt erfolgreich, auch wenn Cancellation erst danach eintrifft.
- Knowledge-Schema V11 ergänzt den dauerhaften Task-Zielanker. `tasks` hält pro Worktree nur
  Erstellzeit und den aktuellen Revisionszeiger; `goal_contract_revisions` sowie die geordneten
  Acceptance-Criteria-, Constraint-, Non-Goal- und User-Decision-Zeilen sind append-only. Initiale
  Erstellung und jede Compare-and-Append-Revision laufen in einer `IMMEDIATE`-Transaktion. Der
  Adapter rekonstruiert beim Lesen ausschließlich Domain-Typen, validiert Sequenzen und Grenzen
  erneut und gibt weder Rows noch SQL an Application oder UI weiter.
- Knowledge-Schema V12 ergänzt pro Task genau ein materialisiertes, revisionsgebundenes Task Ledger.
  Schrittdefinitionen, Abhängigkeiten, erwartete Evidenz, Versuche, Verifikationen, Stale-Ursachen
  und Replan-Historie liegen in normalisierten relationalen Tabellen. Jede Ersetzung läuft in einer
  `IMMEDIATE`-Transaktion mit monotoner Store-Version; ein veralteter Writer oder der Versuch,
  bestehende Definitionen, terminale Versuche oder Replans umzuschreiben, wird vor Mutation
  abgelehnt. Leser rekonstruieren und validieren den vollständigen Domain-Aggregatzustand innerhalb
  einer konsistenten Read-Transaktion.
  ADR-0042 benötigt dafür keine neue Tabelle: Der aus einem bestätigten Conversation-Plan
  Core-kompilierte Arbeitsplan wird als mehrere vorhandene `task_steps` persistiert. Automatische
  Anpassungen verwenden ausschließlich die bestehende append-only Replan-Transaktion; pensionierte
  Definitionen und Versuche bleiben lesbar, neue adaptive und ersetzende Schritte erhalten frische
  IDs, und `agent_runs.task_ledger_revision` wird im selben Compare-and-Swap fortgeschrieben.
- Knowledge-Schema V13 ergänzt `agent_runs` als aktuelle relationale Laufprojektion und
  `run_events` als append-only Auditjournal. Runstart sowie jeder folgende Eventappend aktualisieren
  Event und Materialisierung atomar in einer `IMMEDIATE`-Transaktion. Der Tail wird per
  Sequenz-Compare-and-Swap geschützt; Snapshot, Goal-Contract-Revision und Task-Ledger-Revision
  werden vor Mutation erneut validiert. Materialisierter Zustand wird ohne Journal-Replay gelesen.
- Knowledge-Schema V14 ergänzt jeden neuen `agent_runs`-Datensatz um `ModelProfileId` und
  `ModelProfileVersion`. Beide Felder sind gemeinsam gesetzt oder gemeinsam leer; Insert- und
  Update-Trigger verhindern halbe Referenzen. Ausschließlich aus V13 migrierte Bestandsruns dürfen
  als expliziter Legacyfall ohne Profilbezug lesbar bleiben. Der Storage-Adapter erstellt keinen
  neuen Legacyrun und validiert ID-Länge sowie unterstützte Schemaversion erneut beim Lesen.
- Knowledge-Schema V15 ergänzt unveränderliche Turn-, Token-, Action-, Zeit- und Repair-Limits
  sowie materialisierte kumulative Verbrauchszähler in `agent_runs`. Jeder neue
  `model_interaction`-Event trägt seine Prompt-/Outputtokens, die optionale einzige Actionklasse
  und den Verbrauch des einmaligen Repairpfads. Checks und Trigger verhindern ungültige
  Kardinalitäten, halbe Turn-Charges, Charges auf anderen Eventtypen und spätere Budgetänderungen.
- Knowledge-Schema V16 ergänzt `tool_runs` und `tool_evidence`. Ein normalisierter Read-Tool-Lauf
  wird gemeinsam mit seinem `tool_action`-Event atomar geschrieben und bleibt über ToolRunId,
  Runsequenz, Ergebnisstatus und -digest sowie Vorher-/Nachher-Snapshot nachvollziehbar. Evidence
  besteht ausschließlich aus geordneten, content-adressierten File- oder Source-Span-Locators;
  Suchanfrage, Source-Text und begrenzte Toolvorschau werden nicht persistiert. Fremdschlüssel und
  ein Trigger binden jeden Toollauf an genau sein typisiertes Journal-Event.
- Knowledge-Schema V17 ergänzt `tool_run_attempts` als content-freie Lifecycle-Projektion. Vor
  jedem Toolaufruf wird ein `in_flight`-Versuch mit Run-, Snapshot- und logischer ToolRunId
  committed. Nur der atomare Toolresultat-/Journal-Commit darf ihn auf den Ergebnisstatus setzen;
  Grenzfehler enden explizit als `failed`, `cancelled` oder `denied`. Beim Neustart werden
  verbliebene Versuche als `interrupted` abgeschlossen, während ein Retry derselben logischen
  ToolRunId eine monotone neue Versuchsnummer erhält.
- Knowledge-Schema V18 ergänzt `approval_requests`, `approval_grants` und `policy_decisions`.
  Requests und Entscheidungen sind append-only; Grants besitzen ausschließlich den Übergang
  `active` nach `consumed` oder `revoked`. Trigger binden jede Entscheidung sowie Grant und
  Widerruf an ihr typisiertes `approval_recorded`-RunEvent und prüfen Run, Action-Fingerprint,
  Scope-Digest, Klasse, Risiko und Zeitfenster erneut. Entscheidung, optionaler Request,
  Runprojektion, Event und optionaler One-time-Verbrauch werden gemeinsam per Runsequenz-CAS in
  einer `IMMEDIATE`-Transaktion committed. Grant und Widerruf besitzen je eine eigene atomare
  User-Audit-Transaktion.
- Knowledge-Schema V19 ergänzt `command_allowlist_revisions` und
  `command_allowlist_entries`. Jede Benutzerbestätigung bindet Worktree, monotonen CAS-Stand,
  exakten Katalog-Digest, Zeitpunkt und eine kanonische nicht leere Teilmenge von höchstens 256
  Command-IDs. Revisionen und Einträge sind append-only; ausschließlich die kontrollierte
  Worktree-Reconciliation darf ihre Worktree-ID über Fremdschlüssel-Cascade umschreiben.
- Knowledge-Schema V20 ergänzt Must-/Should-Kriterien, Goal-revisionsgebundene Step-Mappings,
  operationale VerificationSpec-Zeilen und immutable `verification_evidence`. Getrennte strikte
  Variantentabellen speichern content-freie Command-/Test-/Diagnostic-/Diff-/UserConfirm-Semantik
  sowie kanonische Present-/Absent-Abhängigkeiten. Diff-Quellen unterscheiden strikt autorisierte
  Patchresultate von geordneten Published-Index-Vergleichen. Stable IDs werden beim Reopen aus allen
  gespeicherten Feldern erneut abgeleitet; ein gleiches Evidence-ID-Append ist nur bei exakt
  identischem Artifact idempotent. Append und Acceptance-Read sind abbrechbar, besitzen ein
  Zeitlimit und prüfen bei mengenabhängigen Zeilen feste Checkpoints. Acceptance liest Evidence
  und den weiterhin aktuellen Published Index begrenzt und in konsistenten Transaktionen.
- U6 fügt kein persistentes Schema hinzu. Exakte E3-Patchpräfixe und bereits secret-geprüfter
  retained E4-Logtext leben ausschließlich in einem begrenzten, task-/rungebundenen Speicher des
  Desktopprozesses und werden bei Projekt- oder Runwechsel sowie Shutdown verworfen. Die separate
  read-only Verification-Inspektion lädt nur die vom aktuellen Ledger referenzierten bestehenden
  V20-Artefakte zusammen mit dem jüngsten Published Index; sie darf stale Artefakte lesen, ändert
  sie aber nicht und leitet Semantik, Freshness und Must-/Should-Beweise im Application-Kern neu ab.
- Knowledge-Schema V21 ergänzt `run_events.turn_action_kind_v2` als geschlossene, nullable
  Projektion der sechs `AgentAction`-V2-Varianten. Ein Trigger verhindert gleichzeitig gesetzte
  V1-/V2-Spalten und Actionwerte auf anderen Eventtypen; historische V1-Zeilen bleiben unverändert
  lesbar. Erfolgreiche Mutationen schließen ihren bereits dauerhaften Toolversuch nach einem
  typisierten Toolresultat gemeinsam mit content-freiem `tool_action`-Event und Runprojektion in
  einer `IMMEDIATE`-Transaktion ab. Dadurch ist kein erfolgreich markierter Versuch ohne sein
  Journalereignis sichtbar.
- Knowledge-Schema V22 ergänzt `mutation_attempts` als content-freie Projektion jedes mutierenden
  Toolversuchs. Action-Fingerprint, Aktionsart und die Disposition `Applied`, `NotApplied` oder
  `Unknown` werden gemeinsam mit dem Versuchslifecycle gespeichert. Der atomare Beginn setzt
  `Unknown/required`; ein erfolgreicher Abschluss schreibt zusätzlich den normalisierten
  Resultatdigest in `tool_runs`, das ToolEvent und die Runprojektion und setzt erst dann
  `Applied`. Ein worktreeweites `Unknown/required` oder `Unknown/reconciled` sperrt weitere
  Mutationsbeginne. Reconciliation bindet einen vollständigen Published Snapshot, ohne die
  historische Disposition zu ändern; erst der atomare Recovery-Übergang `Replan` setzt
  `Unknown/replanned`. V21-Bestandsversuche, die beim Upgrade noch laufen, werden konservativ als
  unklassifiziertes `Unknown/required` übernommen.
- Knowledge-Schema V23 nimmt die file-genaue Fast-Index-Analyse in denselben atomaren Publish auf.
  `index_file_analyses` bindet Sprache, Adapterrevision und optionale Parse-Coverage exakt an die
  publizierte `FileRevision`; `index_parse_diagnostics` speichert ausschließlich kanonische Codes,
  Severity, begrenzte sichere Meldungen und Quellbereiche ohne Source-Auszug. Jede V5-Publikation
  muss alle Dateien genau einmal abdecken. Diagnostics sind insgesamt auf 2.000.000 begrenzt,
  werden bei Replacement und Rebuild mit entfernt und bei Reads erneut gegen Dateirevision,
  Sequenz und Coverage validiert. Historische Index-Snapshots bis V4 bleiben lesbar und liefern
  explizit keine strukturelle Analyse, statt Erkenntnisse zu erfinden.
- Knowledge-Schema V24 persistiert die versionierten Agent-Session-Projektionen und ihre begrenzten
  Einträge. Knowledge-Schema V25 ergänzt davon getrennt `deep_map_runs`, die materialisierten
  `deep_map_steps` und das append-only `deep_map_events`-Journal. Runstart, Planmaterialisierung und
  jeder Eventappend laufen projektgebunden in Transaktionen; Eventsequenz und terminale Zustände
  verhindern Replay oder spätere Umschreibung. Nichtterminale Läufe werden beim nächsten
  Projektstart als `interrupted` reconciled. Journalfehler steuern weder Modellarbeit noch atomare
  Card-Publikation; die Runprojektion markiert dann ausschließlich `details_incomplete`.
  Reads bleiben auf 20 Läufe und 50 chronologische Einträge pro Seite begrenzt. Gespeichert werden
  nur geschlossene Phasen, Aktionen, Diagnosen, feste Budgetreservierungen und sichere
  Provider-/Modell-/Profilreferenzen — keine Prompts, Modellantworten, Chain-of-Thought,
  Providerpayloads, Source-Inhalte, Credentials oder rohe Fehlertexte.
- Knowledge-Schema V26 repariert ausschließlich die regenerierbare Suchprojektion historischer,
  bereits vollständig veröffentlichter Module Cards. Fehlende `card_fts`-Zeilen werden
  deterministisch aus den unveränderlichen `module_card_field_values` aufgebaut; der
  `lexical_search_projections.card_count` wird nur aktualisiert, wenn Card- und FTS-Anzahl danach
  exakt übereinstimmen. Die kanonischen Cards, Claims und Evidence bleiben unverändert. Repair und
  Markerwechsel erfolgen in derselben Migrationstransaktion und rollen bei jedem Widerspruch
  vollständig zurück.
- Knowledge-Schema V27 wiederholt diese begrenzte Reparatur für Installationen, in denen ein späterer
  Fast-Index-Rebuild die regenerierbare Card-FTS-Projektion eines unveränderten, erneut verwendeten
  Index-Ankers entfernt hatte. Berücksichtigt werden nur Cards eines aktuell veröffentlichten Runs
  mit vorhandenem Lexical-Marker, veröffentlichter Lifecycle-Zeile und vollständig materialisierten
  Feldwerten. Zusätzlich rekonstruiert jede neue Fast-Index-Publikation die Card-Suchprojektion
  desselben unveränderten Ankers innerhalb ihrer atomaren Publish-Transaktion. Stimmen Card-Anzahl,
  Modulbezug, Lifecycle, Feldmaterialisierung, FTS-Anzahl oder Marker nicht exakt überein, wird die
  Fast-Index-Publikation zurückgerollt; ein neuer Index-Anker ohne Cards bleibt dagegen `Ready`.
- Knowledge-Schema V28 ergänzt `index_run_sequence_cursors` als kleine dauerhafte
  worktree-lokale High-Water-Projektion. Die Vorwärtsmigration übernimmt pro Worktree die höchste
  vorhandene Run-Sequenz; ein neuer Worktree beginnt bei eins. Ein Fast-Index-Rebuild löscht
  weiterhin die regenerierbaren `index_runs`, erhält aber den Cursor. Der Core liest ausschließlich
  den exakt nächsten typisierten Wert, leitet daraus deterministisch die Run-ID ab und übergibt
  beide an den Adapter. Cursorfortschreibung und `building`-Insert erfolgen in derselben
  `IMMEDIATE`-Transaktion; stale Starts werden ohne Teilmutation abgewiesen und ein Überlauf wird vor
  der Indexarbeit typisiert beendet. Integritätsreads akzeptieren nach einem Rebuild eine lückenlose
  retained Runfolge, die größer als eins beginnt, verlangen jedoch weiterhin den exakten Marker.
- Knowledge-Schema V29 ergänzt neue Deep-Map-Pläne um `deep_map_step_targets` und
  `deep_map_step_fields`. Pro Schritt wird genau eine geschlossene Modul-, Symbol- oder
  Datei-Evidence-Referenz sowie die kanonische Folge vorgesehener `ModuleCardField`-Werte in
  derselben Plantransaktion gespeichert. Card-Inhalte, Source und Modellantworten verbleiben
  außerhalb des Journals. Historische V28-Pläne bleiben unverändert lesbar und liefern bewusst
  keine erfundenen Ziel- oder Felddetails.
- Knowledge-Schema V30 ergänzt die append-only Tabellen `agent_ask_research_turns`,
  `agent_ask_research_events`, `agent_ask_research_sources` und
  `agent_ask_research_citations`. Ein Turn bindet genau eine sichtbare Ask-Usernachricht an den
  damaligen veröffentlichten Indexanker. Events enthalten nur geschlossene Phase, Zustand,
  sichere Aktionsbeschreibung, optionalen Suchtext und Vollständigkeit; Quellen enthalten nur
  `FileRevision`, optionalen Range/Symbol sowie geschlossene Art und Auswahlgrund. Antwort,
  terminales Event und die Teilmenge tatsächlich zitierter Source-Referenzen committen zusammen
  mit der nächsten Sessionrevision in einer `IMMEDIATE`-Transaktion. Die Migration erzeugt für
  alte Ask-Sessions keine Traces. Presentation Delete entfernt die referenzierenden V30-Zeilen per
  Session-Entry-Cascade; Archivierung lässt sie bestehen. Quelltext, Prompt, Modellrohantwort,
  Chain-of-Thought, Providerdaten, Budgets, interne Scores und historische Source-Snapshots werden
  nicht gespeichert.
- Knowledge-Schema V31 ergänzt die generischen append-only Tabellen `agent_work_trace_turns`,
  `agent_work_trace_events`, `agent_work_trace_notes`, `agent_work_trace_sources`,
  `agent_work_trace_note_sources`, `agent_work_trace_citations` und
  `agent_work_trace_links`. Sie tragen Ask-, Plan- und Agent-Vorbereitung, die gewählte feste
  Recherche-Tiefe und begrenzte öffentliche Arbeitsnotizen. Beobachtungen und Schlussfolgerungen
  referenzieren ihre ursprünglichen aktuellen Sources; Hypothesen bleiben unbelegt. Task- und
  Run-Verknüpfungen sind interne Navigation, keine WebView-Autorität. V30 bleibt unverändert
  lesbar und wird ohne Backfill als Legacy projiziert. Ergebnis, Abschlussereignis, Zitate,
  Sessionrevision und vorhandene interne Links committen atomar. Presentation Delete entfernt
  V31 über die bestehende Session-Entry-Cascade, Archivieren erhält die Daten. Quelltext, Prompts,
  Rohantworten und internes Denken werden weiterhin nicht persistiert. Öffentliche Notizen
  kanonisieren ihre Source-Verweise vor dem Schreiben in ursprünglicher Reihenfolge. Mehrere
  historische Bereiche können nach vollständiger Revalidierung denselben aktuellen Beleg
  erhalten; dessen Verknüpfung wird genau einmal angelegt. Die bestehende Unique-Constraint
  bleibt erhalten, ebenso das Limit vor Deduplizierung. Dafür ist keine Migration erforderlich.
- Knowledge-Schema V32 ergänzt `agent_slash_command_invocations`,
  `agent_slash_command_lenses`, `agent_diagram_artifacts` und
  `agent_diagram_artifact_sources`. Ein Command-Aufruf bindet genau einen bereits Core-validierten
  Katalogeintrag und seine geordneten Linsen atomar an den User-Entry. Ein Diagramm gehört zu genau
  einem Work-Trace-Turn und speichert nur begrenzte Metadaten, Core-kompiliertes Mermaid sowie die
  turnlokalen Source-Zuordnungen; Modell-Mermaid und gerenderte PNG-/SVG-Nutzdaten werden nicht
  persistiert. Antwort, Zitate und bis zu drei Diagramme schließen in derselben Transaktion ab.
  Presentation Delete entfernt V32 über die Session-Entry-Cascade, Archivieren erhält die Daten.
  Die Migration führt keinen Backfill für alte Nachrichten oder Diagramme aus.
- ADR-0040 benötigt keine Knowledge-Migration. Detail, Zitate und erste Quellenpage werden aus
  V30–V32 in einer gemeinsamen Read-Transaktion projiziert. Öffentliche `S`-Labels entstehen
  flüchtig aus der vorhandenen turnlokalen Ordinalzahl; weder Labels noch Antwortanreicherung oder
  Quelltext werden zusätzlich persistiert.
- Knowledge-Schema V33 ergänzt `agent_message_queue_items` und
  `agent_message_queue_events`. Der unveränderliche Eintrag enthält ausschließlich Sessionbindung,
  FIFO-Position, Zielmodus, geschlossene Rechercheauswahl, den bereits begrenzten Nachrichtentext
  und den Core-Zeitpunkt. Vormerken, Start, Entfernen, Pausieren und Fortsetzen werden als
  append-only Zustandsereignisse mit einer monotonen Queue-Revision gespeichert. Aktuell wartende
  Nachrichten sind pro Session auf 16 Einträge und 1 MiB sowie pro Worktree auf 64 Einträge und
  4 MiB begrenzt. Archivierung erhält und pausiert sie; Presentation Delete entfernt ihre
  Präsentationsdaten über die Sessionbindung. Die Migration erzeugt keinen Backfill.
- Häufige Status- und Dashboard-Reads verwenden die geprüfte, aktuelle Indexprojektion aus dem
- Knowledge-Schema V35 erweitert nach ADR-0046 die Eventsequenz der generischen Work Traces
  von 64 auf 1.024. Events, öffentliche Notizen und ihre Source-Verknüpfungen werden in einer
  `IMMEDIATE`-Transaktion vollständig in gleichartig geprüfte Ersatztabellen kopiert und erst
  danach in Kind-/Elternreihenfolge ausgetauscht. Fremdschlüssel bleiben eingeschaltet;
  Update-Guards und sämtliche bestehenden Bezüge bleiben erhalten. Der getestete Fehler nach
  dem Tabellenaustausch rollt Daten, Trigger, Checksum-Historie und Version auf V34 zurück.
  Es werden keine fachlichen Zeilen entfernt oder zusätzliche Inhalte gespeichert. V30-Legacy
  bleibt unverändert. Detailreads begrenzen ausschließlich die Projektion auf die neuesten
  64 Ereignisse in chronologischer Reihenfolge, nicht das gespeicherte Journal. Phase, endliche
  Verbrauchszähler und geschlossene Diagnosecodes dürfen in sicheren Aktionsbeschreibungen
  stehen; rohe Modell-/Providerfehler oder Prompts weiterhin nicht.
- Knowledge-Schema V36 ergänzt nach ADR-0047 `agent_research_work_checkpoints`. Ein streng
  versioniertes, auf 512 KiB begrenztes Adapterdokument enthält höchstens 32 unveränderliche
  Teilfragen, Ergebnisse, ursprüngliche Datei-/Hash-/Bereichsbezüge sowie begrenzte Core-
  Versuchsschlüssel. Es wird atomar mit seinem bestehenden Work-Trace-Event geschrieben.
  Zusätzlich werden höchstens 256 adaptive Zugriffsidentitäten mit Teilfrage,
  Publikationshash, kanonischem Argumenthash, Capability-Klasse, Startzähler und getrenntem
  Core-Ergebnis gespeichert. Vor dem Aufruf wird ein offener Versuch quittiert, danach sein
  Ergebnis; ein Abbruch lässt ihn offen. Es werden keine Such-/Quelltexte im Zugriffskatalog
  dupliziert. Fehlende Katalogfelder älterer V36-Checkpoints bedeuten einen leeren Katalog,
  niemals nachträglich erfundene Suchergebnisse.
  Fragen und Auftrag sind eingefroren, Revisionen monoton; Update und direktes Delete sind
  gesperrt. Die bestehende explizite Präsentationslöschung darf den Prüfstand ausschließlich
  über die Foreign-Key-Kaskade seines bereits entfernten Work-Trace-Elternereignisses löschen.
  Die Source-Identität muss demselben Worktree und derselben Session angehören. Der genaue
  Ergebnisbereich stammt aus dem tatsächlich ausgelieferten Safe-Reader-Fenster; er kann
  den engeren Navigationsanker eines Symbols überragen. Ein Ergebnisbereich wird niemals
  aus einer Modell-Zeilenangabe berechnet. Originalfenster, zum Abgleich gelieferte Zitatfelder
  und rohe Modellantworten werden nicht gespeichert. Öffentliche Ergebnisformulierungen können
  kurze Codebeispiele enthalten; ihre Provenienz bleiben die geprüften Originalreferenzen.
  Der letzte Prüfstand wird unabhängig vom 64-Event-
  Anzeigefenster geladen. Historische Traces erhalten keinen erfundenen Backfill.
  Rebuilds des Index löschen diese nicht regenerierbare Recherchehistorie nicht.
  Core-abgeleitete Untersuchungsgrenzen verwenden dieselben Ergebnis-/Ausschlussfelder;
  ihr Grenzanker ist der Core-berechnete Publikationsscope. Reopen, Workspace-Projektion und
  Agent-Handoff vergleichen diesen explizit und verlangen zugehörige Zugriffsquittungen.
  Frühere Scopes bleiben als Historie erhalten, werden aber weder mit aktuellen Quittungen
  vermischt noch als Sperre einer neuen Untersuchung verwendet. Ein Scopewechsel macht auch
  quellenlose negative Ergebnisse stale. Stale Ausschlüsse dürfen bei einer Fortsetzung keinen
  neuen Abschluss autorisieren. ADR-0111 benötigt keine weitere Migration: Die bestehende
  Capability-Klasse erhält den geschlossenen Wert `IndexInventory` für die Core-Prüfung des
  vollständigen Dateibestands eines gebundenen veröffentlichten Graphen. Nur `NoMatch` mit dem
  scopegleichen Q1-Ergebnis darf den leeren Inventory-Nachweis bilden.
- Häufige Status- und Dashboard-Reads verwenden die geprüfte, aktuelle Indexprojektion aus dem
  Store-Cache, sofern ihr Run-Anker exakt passt. Ein Cache-Miss wird in einem eigenen konsistenten
  Read-Kontext rekonstruiert; Card- und Atlas-Autorität bleiben dadurch unverändert.
- Die dev-only Suite `a3-storage-contract-tests` prüft Katalog, Snapshot-Ketten, Linked-Worktree-
  Isolation, Publish, Rebuild, IndexRun-Übergänge, Policy-/Approval-Lifecycle, die
  projektbezogene Command-Allowlist und alle fünf Verification-Evidence-Varianten ausschließlich
  über die Application-Ports. Der libSQL-Adapter liefert nur eine Factory für temporäre
  App-Data-Roots; engine-spezifische Migration-, Crash-, Korruptions- und Schema-Negativtests
  bleiben getrennt. Jeder weitere Storageadapter muss dieselbe Suite ausführen.
- Der Desktop-Composition-Root öffnet `catalog.db` im privaten Tauri-App-Data-Verzeichnis und injiziert
  denselben Store in Open-, Project-Catalog- und Index-Use-Case. Nach einem erfolgreichen Project Open besitzt
  ein begrenzter Koordinator den Worktree-Watcher, reicht serialisierte Refresh-Jobs an den
  Scheduler weiter und leert dessen begrenzten Ereigniskanal. Beim Project Open wird die zugehörige
  `knowledge.db` innerhalb dieses privaten Roots geöffnet. Die WebView erhält keine DB-Verbindung und
  keinen autoritativen gespeicherten Pfad.
- Für einen eindeutigen Umzugskandidaten zeigt der privilegierte Desktop-Adapter einen nativen Dialog
  mit „reconciliieren“, „separat öffnen“ und „abbrechen“. Nach Bestätigung persistiert der Adapter
  zuerst die exakte Absicht, verschiebt dann `projects/<alte WorktreeId>` atomar innerhalb des privaten
  `projects`-Verzeichnisses, schreibt die Knowledge-Identität transaktional um und aktualisiert den
  Katalog zuletzt. Vor jedem Schritt werden Kandidat, Revision, Quell- und Zielzustand erneut geprüft;
  ein vorhandenes Ziel wird nie überschrieben.

Discovery und Hashing bilden den bestätigenden Vorlauf des Fast Index. Der lokale
Adapter liefert eine versionierte, deterministisch sortierte Projektion relevanter tracked und
untracked Dateien, liest deren Inhalt innerhalb fester Einzel- und Gesamtgrenzen vollständig und
bildet BLAKE3-basierte `FileRevision`s. Der Snapshot-Builder vergleicht sie mit dem aus der
persistierten Delta-Kette rekonstruierten Dateistand, erzeugt ausschließlich bei Inhalts-, HEAD-,
Schema- oder Adapteränderungen die exakt nächste Generation und liefert sie für ein atomisches
Append. Im inkrementellen Pfad übernimmt der Snapshot-Builder unveränderte Revisionen aus der
persistierten Baseline und hasht nur neue oder vom Watcher gemeldete Pfade; ein sichtbares
Eventverlustsignal erzwingt den Vollscan. Parser und Graph erzeugen den vollständigen
deterministischen Publish-Input; Knowledge-Schema V8 veröffentlicht die daraus abgeleiteten Datei-,
Symbol-, Kanten-, Kandidaten-, Rank-, Exact-, FTS-, Modul- und Repository-Card-Projektionen atomar.
Index-Schema V5 und Knowledge-Schema V23 ergänzen darin die exakte file-genaue Sprache,
Adapterrevision, Parse-Coverage und sichere Diagnostics. Knowledge-Schema V9 ergänzt danach
ausschließlich deterministisch verifizierte Module Cards,
Claims und ihre vollständige Evidence-Provenienz.
Die Reconciliation entscheidet trotz persistierter Evidenz nie
selbstständig: Sie benötigt einen eindeutigen Kandidaten und die privilegierte native Bestätigung.

Quellen:

- [libSQL](https://docs.turso.tech/libsql)
- [libSQL AI and Embeddings](https://docs.turso.tech/features/ai-and-embeddings)
- [Turso Database Repository](https://github.com/tursodatabase/turso)
- [Turso Rust SDK Reference](https://docs.turso.tech/sdk/rust/reference)

## Speicherorte

A^3 schreibt den regenerierbaren Index standardmäßig nicht in das Repository.

~~~text
OS app data/A3/
├── catalog.db
├── logs/
└── projects/
    └── worktree-id/
        ├── knowledge.db
        ├── artifacts/
        └── backups/
~~~

Plattformpfade werden ausschließlich über Tauri- beziehungsweise OS-Path-APIs bestimmt.

Optional darf ein Repository folgende kontrollierte Dateien enthalten:

~~~text
.a3/
├── project.toml
└── rules.md
~~~

Diese Dateien sind menschenlesbare Projektkonfiguration und keine Runtime-Datenbank.

Discovery V1 liest optional ein strikt begrenztes Schema:

~~~toml
[discovery]
ignore = ["private/**", "fixtures/generated/"]
~~~

Die Muster verwenden Gitignore-Syntax, sind aber nur ausschließend; `!`-Negationen und unbekannte
Konfigurationsfelder sind ungültig. Eingebaute Secret-, Vendor-, Generated-, Binary- und
Größenregeln können nicht aus dem Repository aufgehoben werden. Benutzerweite Git-Konfiguration
außerhalb des ausgewählten Worktrees wird nicht automatisch gelesen.

## Datenklassifikation

| Klasse | Beispiele | Regenerierbar | Standard-Sync |
| --- | --- | --- | --- |
| Catalog | Projektliste, UI-Einstellungen | teilweise | aus |
| Index | Dateien, Symbole, Kanten, FTS | ja | nie |
| Semantic Cache | Cards, Embeddings | ja | nie |
| Task State | Ziele, Schritte, Decisions | nein | aus |
| Audit | Run Events, Tooldigests | teilweise | aus |
| Secrets | Provider-Tokens | nein | verboten in DB |

Secrets werden über den jeweiligen OS-Schlüsselspeicher verwaltet. Für Provider-Credentials ist
dies die einzige autoritative Ablage; ein Datei-, Datenbank- oder Environment-Fallback ist
verboten. Der Katalog enthält nur `credential_requirement`, den internen Lifecycle
`not_required | missing | storing | configured | deleting` und eine monotone Generation. Der
öffentliche Status wird daraus und aus einem generationsgebundenen Keyring-Leseversuch abgeleitet.

## Logisches Schema

### Katalog

- schema_migrations
- desktop_settings_revisions (append-only, Katalogschema v5)
- desktop_llm_profiles
- desktop_embedding_profiles
- projects
- repository_observations
- recent_worktrees
- worktree_reconciliations

### Projektidentität und Snapshots

- repositories
- worktrees
- snapshots
- snapshot_adapter_revisions
- snapshot_changes
- index_runs

### Deterministischer Index

- files
- file_revisions
- symbols
- symbol_edges
- unresolved_edges
- module_projections
- modules
- module_manifests
- module_members
- module_membership_evidence
- module_central_symbols
- module_entrypoints
- module_tests
- repository_card_entrypoints
- parse_diagnostics
- ranking_projections

### Suche und Semantik

- exact_search_projections
- exact_search_symbols
- exact_search_manifests
- lexical_search_projections
- symbol_fts
- path_fts
- card_fts
- semantic_cards
- semantic_card_snapshots
- embedding_profiles
- embeddings
- retrieval_policies

### Knowledge und Evidenz

- module_cards
- module_card_fields
- module_card_field_values
- module_card_field_evidence
- claims
- evidence_refs
- claim_evidence
- claim_relations
- decisions

### Aufgaben und Runs

Chatbasierte Agent-Sessions liegen in derselben privaten worktree-lokalen Knowledge-Datenbank als
append-only `agent_session_revisions` und begrenzte `agent_session_entries`. Die aktuelle Revision
enthält Modus, Präsentationsstatus, optionalen Work-Item-/Task-Anker, aktuelle Planrevision und
Tombstone. Entries sind lückenlos sequenziert und können User Message, Assistant Summary, Plan,
Activity oder Final Report sein. Die Präsentationslöschung entfernt nach einem dauerhaften
Tombstone ausschließlich Entries; Goal, Ledger, Run, Journal, Evidence und Approval bleiben
unverändert erhalten.

Inhaltsfreie globale UI-Präferenzen werden getrennt im Katalog als revisionierte
`ui_preference_revisions` gespeichert. Sie enthalten nur Breiten und Collapse-Zustände der
Sessionleiste und des Inspectors, nie Projektinhalt, Chattext oder Pfade.

- tasks
- goal_contract_revisions
- acceptance_criteria
- goal_contract_constraints
- goal_contract_non_goals
- goal_contract_user_decisions
- task_ledgers
- task_steps
- task_step_dependencies
- task_step_expected_evidence
- task_step_attempts
- task_step_attempt_evidence
- task_step_verifications
- task_step_verification_evidence
- task_step_stale_evidence
- task_ledger_replans
- task_ledger_replan_retirements
- task_ledger_replan_additions
- agent_runs
- run_events
- context_packs
- tool_runs
- tool_run_attempts
- mutation_attempts
- verification_runs
- approval_requests
- approval_grants
- policy_decisions
- command_allowlist_revisions
- command_allowlist_entries

## Schlüsselinvarianten

- Fremdschlüssel sind aktiviert.
- Jede projektspezifische Zeile ist einem Worktree zugeordnet.
- Snapshots bilden pro Worktree eine lückenlose, unveränderliche Generationenkette; Parent und
  Generation werden vor jedem Append gegen den letzten persistierten Snapshot geprüft.
- Snapshot-Pfadänderungen und Adapterrevisionen sind innerhalb eines Snapshots eindeutig und werden
  in kanonischer Reihenfolge rekonstruiert.
- Der wirksame Dateistand ist die jeweils jüngste Änderung pro Pfad; `Upsert` setzt den aktuellen
  Content Hash, `Delete` entfernt den Pfad und behält den vorherigen Hash zur Invalidation.
- Pro Worktree existiert höchstens ein laufender `building`-IndexRun; seine Sequenz ist lückenlos und
  worktree-lokal.
- File Revision ist über WorktreeId, normalisierten Pfad und Content Hash eindeutig.
- Veröffentlichte Indexdaten besitzen SnapshotId und IndexRunId.
- Die Exact-Search-Projektion besitzt pro veröffentlichtem Run genau einen Versionsmarker mit
  erwarteter Symbol- und Manifestanzahl. Qualifizierte Namen decken exakt alle veröffentlichten
  Symbole ab; Manifestzeilen referenzieren die aktuelle File Revision desselben Runs.
- Die Lexical-Search-Projektion besitzt pro veröffentlichtem Run genau einen Versionsmarker mit
  erwarteter Symbol-, Pfad- und Card-Anzahl. `symbol_fts` und `path_fts` decken exakt die aktuelle
  Run-Projektion ab; nach R9-Publish deckt `card_fts` exakt die publizierten Cards dieses Runs ab.
- Graphabfragen lesen ausschließlich `file_revisions`, `symbols`, `symbol_edges` und qualifizierte
  Namen desselben jüngsten veröffentlichten Runs. Seeds und jedes erreichte Ziel müssen in dieser
  vollständigen Run-Projektion vorhanden sein; historische oder gelöschte IDs werden nicht
  aufgelöst.
- EvidenceRef besitzt eine typabhängige, validierte Nutzlast. Persistierte Card-Evidence behält
  zusätzlich die vollständige File Revision, Symbolrevision oder Graphkante des Verify-Zeitpunkts.
- Claim-Evidence ist many-to-many.
- Fact, Observation und Hypothesis sowie `Active` und Confidence werden in getrennten Spalten
  gespeichert; Schema-Checks verhindern, dass Architekturabsicht als Fact persistiert wird.
- Verifizierte Cards, Claims und Evidence sind dauerhaft. Ein Rebuild entfernt ihre regenerierbare
  `card_fts`-Projektion, aber nicht ihre Herkunft; R11 übernimmt deren spätere Invalidierung.
- Der Evidence Inspector benötigt keine weitere Migration. Eine kurze Deferred-Transaktion bindet
  die jüngste Publikation und R11-Cardauswahl an `module_card_field_evidence` und rekonstruiert
  genau eine bereits in V9 atomar gespeicherte File-, Symbol- oder Graph-Payload. Aktuelle
  Evidence wird zusätzlich gegen die `file_revisions`, `symbols` oder `symbol_edges` desselben
  jüngsten Runs aufgelöst; historische Payloads dürfen ausschließlich für eine stale Card als
  stale Provenienz ausgegeben werden.
- Embedding ist über SemanticCardId, ModelProfileId und BodyHash eindeutig.
- Eine Semantic Card besitzt pro Snapshot genau eine Body-Revision; derselbe Body darf über
  Snapshots hinweg denselben regenerierbaren Cacheeintrag wiederverwenden.
- Persistierte Profilmetadaten müssen vollständig zur abgeleiteten ModelProfileId passen; ein
  Provider-, Modell- oder Dimensionswechsel erzeugt daher einen getrennten Cachekorridor.
- Jeder Task besitzt bei Erstellung atomar eine valide initiale Goal-Contract-Revision. Der
  aktuelle Zeiger darf nur auf die exakt nächste, zeitlich nicht rückläufige Revision wechseln;
  alte Revisionen und ihre geordneten Inhalte bleiben unverändert lesbar. Ein konkurrierender
  Writer mit veraltetem Vorgänger wird abgelehnt.
- RunEvent ist über RunId und Sequenz eindeutig und append-only. Sequenz eins ist ausschließlich
  `RunStarted`; Zustands- und Ledgerübergänge besitzen vollständig typisierte Vorher-/Nachherwerte.
- `agent_runs` und der neu angehängte `run_events`-Datensatz wechseln in derselben Transaktion.
  Ein veralteter Writer verliert den Compare-and-Swap und hinterlässt weder Event noch Teilzustand.
- Jeder neue `agent_runs`-Datensatz referenziert eine vollständige `ModelProfileId` zusammen mit
  genau der unterstützten Profilschemaversion. Ein migrierter Legacyrun hat beide Werte `NULL`;
  gemischte Nullzustände sind durch Trigger und Adaptervalidierung ausgeschlossen.
- Das am Runstart gewählte Budget ist unveränderlich. Turn-, Action- und Repairzähler werden mit
  genau dem zugehörigen `model_interaction`-Event in derselben Sequenz-CAS-Transaktion erhöht;
  `action_count` und `repair_count` können nie größer als `turn_count` sein.
- Run-Event-Payloads enthalten nur geschlossene Codes, grobe Outcomes, content-freie
  Redaktionsmetadaten und den Digest dieser sicheren Struktur. Freitext, Modelloutput, Tooloutput,
  externe Fehlertexte und Secretwerte sind in diesem Schema nicht darstellbar.
- Jeder privilegierte Policyversuch besitzt genau eine unveränderliche `policy_decisions`-Zeile und
  genau ein passendes content-freies RunEvent. `ApprovalRequired` referenziert einen exakt
  metadata- und zeitgleichen Request; `ApprovalGranted` referenziert einen noch aktiven Grant mit
  identischem Run, Action-Fingerprint, Scope, Klasse und Risiko.
- Approval-Requests gelten höchstens 24 Stunden. Grants dürfen nur innerhalb des Requestfensters
  entstehen und nur vor ihrem exklusiven Ablauf verbraucht oder widerrufen werden. Consumption
  referenziert genau die erlaubende Decision; Grant und Widerruf referenzieren genau ihr explizites
  User-Audit-Event. Terminale Grants und append-only Requests/Decisions können nicht umgeschrieben
  oder gelöscht werden.
- Jede Command-Allowlist-Revision ist append-only und pro Worktree monoton. Ihre Einträge sind
  ordinal lückenlos, eindeutig und auf 256 begrenzt. Ein veralteter erwarteter Revisionsstand
  rollt die gesamte Bestätigung zurück; ein aktueller Command-Katalog akzeptiert sie nur bei
  identischem Worktree, Katalog-Digest und bestätigter Command-ID.
- Context Pack speichert keine Secrets und kann nach Retention-Policy komprimiert werden.

## Transaktionen

Eine einzelne DB-Transaktion darf:

- ein File Delta samt Symbolen und Kanten anwenden;
- Invalidationen für dieses Delta markieren;
- einen vollständigen Indexlauf veröffentlichen;
- einen Task-Schritt samt Verification abschließen;
- genau einen RunEvent anhängen und die zugehörige Run-Materialisierung aktualisieren;
- genau eine PolicyDecision, ihren optionalen ApprovalRequest, das Audit-Event und den optionalen
  Approval-Verbrauch per Runsequenz-CAS committen;
- genau einen Approval-Grant oder Widerruf zusammen mit seinem User-Audit-Event committen;
- einen Toolrun und sein Ergebnis protokollieren.
- einen erfolgreichen Mutationsversuch gemeinsam mit Event und Runprojektion abschließen.

Sie darf keine Datei lesen, kein Modell aufrufen und keinen Prozess abwarten.

## FTS

FTS indiziert getrennte Felder und gewichtet Namen und Signaturen höher als freie Beschreibung:

- Symbolname
- qualifizierter Name
- Signatur
- Dateipfad
- Module-Card-Titel
- Module-Card-Zweck

Rohcode wird nicht pauschal vollständig in FTS dupliziert. Für gezielte Textsuche bleibt ein schneller Repository-Searcher zuständig.

Knowledge-Schema V6 verwendet den eingebetteten FTS5-Trigram-Tokenizer. Untrusted Querytext wird
zuerst in begrenzte alphanumerische Tokens und daraus in ausschließlich abgeleitete, gequotete
Trigram-Klauseln transformiert; der vollständige Ausdruck bleibt ein gebundener Parameter. FTS
liefert nur den Kandidatenkorridor. Die finale Gewichtung, Mindestschwelle und stabile Reihenfolge
erfolgen deterministisch im Adapter. Pfade bleiben im Treffer als verlustlose Bytes erhalten; nur
ihre Suchprojektion ist UTF-8 beziehungsweise eine deterministische Prozentkodierung.

## Vektoren

Embeddings werden nur für normalisierte Semantic Cards und ausgewählte Symbole gespeichert.

Pflichtmetadaten:

- Provider und Modell
- Dimensionszahl
- Datentyp und Quantisierung
- Normalisierung
- Body Hash
- Erstellzeit

Knowledge-Schema V7 speichert den kanonischen Cardkörper, seine Snapshot-Zuordnung, alle
vektorformenden Profilfelder und den normalisierten Float32-Vektor in portabler
Little-Endian-Darstellung. Der exakte Cachelookup verwendet ausschließlich
`(SemanticCardId, ModelProfileId, BodyHash)` und validiert Dimension, Endlichkeit und L2-Norm auch
beim Lesen erneut.

Die libSQL-Capability wird für die konkrete Profildimension mit einer isolierten In-Memory-
`FLOAT32(dim)`-Tabelle und einem `libsql_vector_idx(..., 'metric=cosine')` geprüft. Bei verfügbarer
Erweiterung werden höchstens 4.096 snapshot- und profilgebundene Cachezeilen in eine kurzlebige
DiskANN-Kandidatenprojektion übernommen. Die Projektion liefert ausschließlich Kandidaten; die
endgültige Cosine-Normalisierung, Reihenfolge und Trunkierung erfolgen deterministisch im Rust-
Adapter. Schlägt Capability oder Projektion fehl, vergleicht derselbe begrenzte Korridor linear.
Ein abgeschnittener Korridor oder ein Resultlimit wird im `VectorSearchResult` sichtbar markiert.

Der Vektorindex ist optional. Bei fehlender Unterstützung bleiben Exact Search, FTS und Graph
unverändert nutzbar; auch semantische Kandidaten bleiben über den begrenzten linearen Fallback
verfügbar.

## Migrationen

Das implementierte Knowledge-Schema V21 ergänzt V20 um die rückwärtskompatible Speicherung aller
sechs `AgentAction`-V2-Klassen und den atomaren Erfolgsabschluss mutierender Toolversuche. Neue
Model-Events schreiben ausschließlich `turn_action_kind_v2`; alte V1-Zeilen werden über eine
`COALESCE`-Projektion weiterhin exakt rekonstruiert. Der gemeinsame Adapter- und E7-
Integrationsvertrag belegt, dass erst die content-freie Toolaktion samt Journal- und Runprojektion
einen Versuch auf `succeeded` setzt und ein Runsequenzkonflikt den Versuch vollständig zurückrollt.
Migrationstests decken leeres Schema, jeden Vorgänger bis V20 und vollständigen Rollback eines
fehlgeschlagenen V20→V21-Upgrades ab.

Das implementierte Knowledge-Schema V20 ergänzt V19 um die typed Verification Engine. Die
Migration erweitert bestehende Acceptance Criteria rückwärtskompatibel mit `requirement = must`,
bindet neue Step-Criterion-Mappings an exakt dieselbe Goal-Revision und persistiert operationale
Specs getrennt von lesbaren Legacy-Spezifikationen. Elf neue normalisierte Tabellen bewahren
Mappings, Specs, schema-versionierte Header, Processmetadaten, strukturierte Testfälle,
Diagnosezahlen, strikt disjunkte Patch-/Published-Index-Diffquellen, Diffpfade,
Userbestätigungen und Present-/Absent-Freshness-Abhängigkeiten. Der
gemeinsame
Adaptervertrag belegt Timeout und Cancellation ohne Teilwrite, idempotentes Append, Reopen aller
Varianten und beider Diffquellen, Soll-Nichtblockierung,
Acceptance und gezielte Stale-Erkennung nach einer neuen Indexpublikation. Migrationstests decken
leeres Schema, jeden Vorgänger bis V19 und vollständigen Rollback eines fehlgeschlagenen
V19→V20-Upgrades ab.

Das implementierte Knowledge-Schema V19 ergänzt V18 um die dauerhafte projektbezogene
Command-Allowlist. `command_allowlist_revisions` und `command_allowlist_entries` sind append-only,
streng worktreegebunden und erlauben nur der kontrollierten Identitäts-Reconciliation ein
Worktree-ID-Cascade. Der gemeinsame Adaptervertrag belegt leeren Anfangszustand, Confirmation,
Reopen, monotone Revision, vollständigen CAS-Rollback und Worktree-Isolation. Migrationstests
decken leeres Schema, jeden Vorgänger bis V18, den vollständigen Rollback eines fehlgeschlagenen
V18→V19-Upgrades sowie die allein zulässige Reconciliation-Cascade ab.

Das implementierte Knowledge-Schema V18 ergänzt V17 um die zentrale Policy- und
Approval-Persistenz. `approval_requests` und `policy_decisions` sind unveränderlich und
append-only; `approval_grants` erlaubt nur einen einmaligen terminalen Übergang. Relationale Checks,
Fremdschlüssel und Trigger binden Requests, Grants, Entscheidungen sowie Grant-/Widerruf-Events an
denselben Run und erzwingen exakte Action-/Scope-Metadaten und das ursprüngliche Ablaufzeitfenster.
Der gemeinsame Adaptervertrag belegt Reopen, Pfad-Mismatch ohne Grantverbrauch, einmalige
Consumption, Widerruf, restriktive Workspace-Policy und vollständigen Rollback eines veralteten
Run-CAS. Migrationstests decken leeres Schema, jeden Vorgänger bis V17 und den vollständigen
Rollback eines fehlgeschlagenen V17→V18-Upgrades ab.

Das implementierte Knowledge-Schema V17 ergänzt V16 um dauerhafte Toolversuche und atomare
Run-Recovery. Bestehende V16-Toolläufe werden als abgeschlossener erster Versuch übernommen; ein
partieller Unique Index erlaubt je logischer ToolRunId höchstens einen laufenden Versuch. Die
Recovery liest Run und Ledger nach Reopen, vergleicht abgeschlossene Verification-Evidence mit dem
jüngsten atomar veröffentlichten Index und committed Resume, Replan oder Cancel in einer kurzen
`IMMEDIATE`-Transaktion. Diese Transaktion vergleicht Published Snapshot, Ledger-Store-Version und
RunEvent-Sequenz, bevor Ledgerprojektion, Runmaterialisierung und Recovery-Event gemeinsam sichtbar
werden. Der gemeinsame Adaptervertrag simuliert Appabbruch, Interrupted-Markierung, Retry,
Snapshotwechsel, stale Step-Reopen sowie Snapshot- und Run-CAS-Rollback; der getestete V16→V17-
Fehlerfall lässt Schema und `user_version` vollständig auf V16.

Das implementierte Knowledge-Schema V16 ergänzt V15 um die dauerhafte, bounded Tool-Evidence.
Der libSQL-Adapter schreibt Tool-Event, Runprojektion, Toolmetadaten und höchstens 100 typisierte
Evidence-Locators in einer `IMMEDIATE`-Transaktion. Ein separater atomarer Action-Port ersetzt bei
Ledger-mutierenden `UpdateLedger`-Ergebnissen zusätzlich die Task-Ledger-Projektion und hängt die
resultierende Controllertransition mit zwei Compare-and-Swap-Ankern in derselben Transaktion an.
Adaptervertrag und Repositorytest belegen Reopen, Konflikt-Rollback und die Abwesenheit roher
Toolvorschauen; der getestete V15→V16-Fehlerfall rollt vollständig auf V15 zurück.

Das implementierte Knowledge-Schema V15 ergänzt V14 um unveränderliche Controllerbudgets,
kumulative Verbrauchszähler und vollständige per-Turn-Charges. Der gemeinsame Adaptervertrag
belegt die atomare Aktualisierung von Event und Nutzung sowie ihre exakte Wiederherstellung nach
Reopen. Migrationstests decken leeres Schema, jeden Vorgänger bis V14 und den vollständigen
Rollback eines fehlgeschlagenen V14→V15-Upgrades ab.

Das implementierte Knowledge-Schema V14 ergänzt V13 um den dauerhaften ModelProfile-Bezug jedes
neuen Agentenlaufs. Die Migration erhält bestehende Runprojektionen mit einem expliziten
Legacy-Nullpaar und installiert Guards gegen partielle Referenzen. Domain- und Adaptertests belegen
Profil-ID/Schemaversion nach Reopen, journalunabhängiges Lesen des Legacyfalls und die Ablehnung
eines einzelnen gesetzten Felds. Der generische Upgradevertrag migriert weiterhin jede unterstützte
Vorgängerversion bis V21 in einer eigenen atomaren Migration.

Das implementierte Knowledge-Schema V13 ergänzt V12 um `agent_runs` und `run_events`. Strikte
Checks erzwingen die Startsequenz, geschlossene Event-, State-, Outcome- und Redaction-Werte,
kontiguierliche Ledgerrevisionen sowie vollständige optionale Feldgruppen. Fremdschlüssel binden
jeden Run an Task, konkrete Goal-Contract-Revision, Task Ledger und aktuellen Snapshot; jedes Event
referenziert einen vorhandenen Snapshot. Der gemeinsame Adaptervertrag prüft atomaren Start,
Linked-Worktree-Isolation, konkurrierende Sequenz-CAS-Appends, Paging, Redaction, deterministischen
Export und exakte Wiederherstellung nach Reopen. Der getestete V12→V13-Fehlerfall rollt vollständig
auf V12 zurück.

Das implementierte Knowledge-Schema V12 ergänzt V11 um die relationale Task-Ledger-Projektion.
Strikte Checks und Fremdschlüssel begrenzen IDs, Texte, Sequenzen, Statuswerte und
Verifikationsformen. Create und Replace schreiben immer die vollständige Projektion atomar; Replace
verwendet Compare-and-Swap auf einer separaten Store-Version und bewahrt die append-only
Versuchs- und Replan-Historie. Der gemeinsame Adaptervertrag prüft Worktree-Isolation,
Konfliktablehnung, fehlgeschlagene und erfolgreiche Verifikation, transitive Evidence-Invalidierung,
Replan sowie die exakte Rekonstruktion nach Reopen. Der getestete V11→V12-Fehlerfall rollt
vollständig auf V11 zurück.

Das implementierte Knowledge-Schema V11 ergänzt V10 um `tasks`, `goal_contract_revisions`,
`acceptance_criteria`, `goal_contract_constraints`, `goal_contract_non_goals` und
`goal_contract_user_decisions`. Strikte Checks spiegeln die Domainlängen und die Grenze von 64
geordneten Einträgen; Fremdschlüssel schützen Task, Vorgänger und aktuellen Revisionszeiger. Die
initiale zyklische Task-/Revision-Bindung wird ausschließlich innerhalb einer deferred-FK-
Transaktion hergestellt. Der getestete V10→V11-Fehlerfall rollt vollständig auf V10 zurück;
Migrationstests decken weiterhin jede unterstützte Vorgängerversion ab.

Das implementierte Knowledge-Schema V10 ergänzt V9 um `module_card_lifecycle`, `claim_lifecycle`,
`evidence_invalidations` und `module_remap_queue`. Historische Card-, Claim- und Evidence-Zeilen
bleiben unverändert auditierbar; separate Lifecyclezeilen verhindern, dass eine alte Published-
Repräsentation nach einer Invalidierung erneut sichtbar wird. Der neue Index-Run, direkte
Evidence-Prüfung, Card-/Claim-Übergänge und Queue-Updates werden in derselben `IMMEDIATE`-
Transaktion publiziert. Bestehende Queueeinträge werden auf den neuen Ziel-Run weitergeschrieben,
entfernte Module gelöscht und eine erfolgreich ersetzte Card atomar aus der Queue genommen.

Der Queue-Reader validiert aktuellen Run und Snapshot, Priorität/Grund, stabile Reihenfolge,
Modul-Eindeutigkeit sowie ein Limit von höchstens 256 Einträgen. Ein getesteter V9→V10-Rollback
lässt die V9-Datenbank unverändert; Migrationstests decken weiterhin jede unterstützte
Vorgängerversion ab.

Wiederholte Indexmutationen verwenden einen separaten, identitätsgeprüften und auf vier Worktrees
begrenzten Adaptercache. Read- und Mutationshandles bleiben getrennt; die bestehende
Ein-Mutation-pro-Worktree-Regel sowie `IMMEDIATE`-Transaktionen bleiben die
Serialisierungsgrenzen. Reconciliation verwirft beide Cacheklassen, bevor Storage verschoben oder
umgebunden wird.

Das implementierte Knowledge-Schema V9 ergänzt V8 um `module_cards`, `module_card_fields`,
`module_card_field_values`, `module_card_field_evidence`, `evidence_refs`, `claims`,
`claim_evidence` und `claim_relations`. Alle Identitäten bleiben an den verifizierenden
`source_index_run_id` und `SnapshotId` gebunden. Der Herkunfts-Run ist bewusst kein
Delete-Fremdschlüssel: Fast-Index-Rebuilds dürfen die dauerhaften Claims nicht vernichten.
Snapshots bleiben referenziell geschützt; alle abhängigen Cardtabellen besitzen dagegen strikte
komposite Schlüssel. Ein getesteter V8→V9-Rollback lässt die V8-Datenbank unverändert. Die
Publisher-Contractprobe belegt atomaren SQL-Fehlerrollback, Cancellation vor Mutation,
Progressfehler, maximal 64 monotone Fortschrittsereignisse, getrennte Fact-/Hypothesis-
Persistenz, exakte FTS-Zeilenzahl, Duplicate-Rejection und Erhalt der Claims nach Index-Rebuild.

Das implementierte Knowledge-Schema V8 ergänzt V7 um die vollständige run-gebundene
`ModuleProjection`. Ein Marker hält `ModulePolicyVersion`, Datei-/Symbol-/Modul-/Membershipzahlen,
Sprachmaske und Repository-Card-Trunkierung. Primärmodule, Manifestrevisionen, genau eine primäre
Membership pro Symbol, zusätzliche Graph-Memberships mit aktuellen `EvidenceRef`s sowie begrenzte
Zentral-, Entrypoint- und Testlisten werden in strikt typisierten Tabellen gespeichert. Reader
rekonstruieren die Domainaggregate und prüfen Snapshot, Counts, Symbolabdeckung, aktuelle
File Revisions und die Zugehörigkeit jeder Graph-Evidence erneut.

V8 verwendet für seine Projektionshierarchie absichtlich keine tiefen Delete-Cascades. Replacement
und Rebuild löschen die Kindtabellen in referenziell sicherer Reihenfolge und begrenzten Batches,
bevor Modulmarker, Symbole oder File Revisions entfernt werden. Migration aus jeder unterstützten
Vorgängerversion, fehlgeschlagener V7→V8-Upgrade, atomarer Rollback, Replacement, Reopen und Rebuild
sind getestet. Index-Schema V4 erzwingt beim nächsten Compilerlauf die neue vollständige Projektion;
ein Publisher kann Graph und Ranking nicht mehr ohne passende Module und Repository Card sichtbar
machen. Ein aus V7 migrierter veröffentlichter Run mit Index-Schema V1 bis V3 bleibt für seine
vorhandenen Exact-/FTS-/Graphkanäle lesbar, wird aber nicht als vollständiger `PublishedIndex`
ausgegeben; erst der erzwungene V4-Rebuild liefert wieder die vollständige Projektion. Fehlt der
Modulmarker dagegen bei einem V4-Snapshot, gilt die Datenbank als inkonsistent.

Das implementierte Knowledge-Schema V7 ergänzt V6 um `semantic_cards`,
`semantic_card_snapshots`, `embedding_profiles` und `embeddings`. Alle Tabellen sind strikt,
referenzieren existierende Snapshots und trennen Cachezeilen durch Card-, Body- und Profil-ID.
V7 besitzt einen getesteten Vorwärtsupgrade- und Rollbackpfad; der Rebuild löscht ausschließlich
diese regenerierbaren Tabellen in referenziell sicherer Reihenfolge, einzeln committeten begrenzten
Batches und mit determiniertem Row-Progress. Ein Abbruch kann damit ohne ungültigen Zwischenzustand
idempotent fortgesetzt werden.

Knowledge-Schema V6 ergänzt V5 um `lexical_search_projections`, `symbol_fts`,
`path_fts` und das für R5 vorbereitete, noch leere `card_fts`. Exact- und Lexical-Projektion werden
gemeinsam mit File Revisions, Graph und Ranking in derselben Publish-Transaktion sichtbar. Ein aus
einem älteren Schema migrierter, bereits veröffentlichter Run besitzt absichtlich keinen neueren
Projektionsmarker und liefert den kanalbezogenen Zustand `ProjectionUnavailable`, statt
unvollständige Treffer vorzutäuschen. Index-Schema V3 erzwingt beim nächsten Compilerlauf eine
Neupublikation der lexikalischen Projektion.

R3 benötigt keine zusätzliche Migration: Knowledge-Schema V4 speichert bereits jede aufgelöste
Kante mit kanonischer Sequenz, typisierten Endpunkten, Relation, Provider, Confidence, Resolution,
Snapshot und `EvidenceRef`. Der Graph-Reader validiert zusätzlich den Exact-Projektionsmarker des
Runs, weil Symbolziele ihre containment-abgeleiteten qualifizierten Namen als Domainobjekt tragen.

- Nur Vorwärtsmigrationen
- monotone ganzzahlige Schema-Version
- jede Migration in eigener Transaktion, soweit die Engine dies unterstützt
- Backup vor destruktiver oder nicht trivial rückrollbarer Migration
- Tests von leerem Schema und unterstützten Upgradepfaden
- App startet bei neuerer unbekannter DB-Version ausschließlich read-only mit verständlichem Fehler
- keine automatische Datenlöschung zur Fehlerbehebung

## Retention und Wiederaufbau

Der lokale libSQL-0.9.29-Adapter verwendet den in
[ADR-0055](adrs/0055-libsql-einmalige-verbindungsfreigabe.md) dokumentierten
Cargo-Patch zur einmaligen nativen Connection-Freigabe. Quelle und Features bleiben
gepinnt; unabhängige Operationsverbindungen, Transaktionsisolation, Schema und
Storage-Ports bleiben unverändert. Der native Lebensdauertest läuft ohne Crash-Retry.

- Index und Embeddings dürfen sicher gelöscht und aufgebaut werden.
- „Aus A^3 entfernen“ löscht nur den exakt identitätsgebundenen `recent_worktrees`-Eintrag und
  offene Reconciliation-Absichten dieses Worktrees. `projects`, `repository_observations`, andere
  Linked Worktrees und `projects/<WorktreeId>/knowledge.db` bleiben bestehen. Wiederöffnen verwendet
  deshalb dieselbe stabile `ProjectId` und alle privaten Knowledge-, Task-, Decision- und
  Evidence-Daten weiter; Repositorydateien gehören nie zum Löschumfang.
- Der Desktop-Rebuild entfernt über `KnowledgeIndexStore::rebuild_regenerable_index` nur die
  regenerierbaren deterministischen Indexprojektionen und ihre IndexRun-Zeilen. Snapshotkette,
  verifizierte Herkunft, Tasks, Decisions, User-Evidence und Repositoryinhalte bleiben erhalten;
  anschließend wird ein vollständiger lokaler Refresh angefordert.
- Task, Decisions und User-Evidence benötigen Backup vor Cleanup.
- Vollständige Toollogs können nach Policy gekürzt werden; Digest, Status, relevante Evidence und Verifikation bleiben.
- Strukturierte RunEvents selbst werden nicht gekürzt: Die Retention-Policy
  `PreserveAuditEvents` behält das content-freie Auditjournal vollständig. Rohe Modell-, Tool- und
  Fehlertexte gelangen nie in das Journal und müssen deshalb dort nicht nachträglich gelöscht
  werden. Ein Rebuild oder Cleanup darf weder `agent_runs` noch `run_events` entfernen.
- `a3.run-journal.jsonl` V2 exportiert zusätzlich die unveränderlichen Budgets, den kumulativen
  Verbrauch und die content-freien per-Turn-Charges. Es schreibt eine Headerzeile und danach exakt
  einen kanonischen, schema-versionierten JSON-Datensatz pro Event. Der Export ist auf 10.000
  Events und 8 MiB begrenzt, paginiert Storagezugriffe mit höchstens 256 Events, unterstützt
  Cancellation und monotonen Progress und enthält ausschließlich dieselben content-freien Felder
  wie die DB.
- Ein Rebuild darf keine Task-Historie oder Decisions verlieren.


## Frischer Kandidat70: gebundene Edit-V3-Anfragen und geschlossene Halte

Der enge Edit-/Need-Schnitt reserviert Kandidatenhead70; Produktion bleibt42. Das
neue `plan22_candidate_v70.sql` wird ausschließlich auf frischem isoliertem Bestand
aufgebaut. Bestehende42..69-Daten werden vor Writes zurückgewiesen. Die historischen
SQL-Dateien68/69, Namen/Checksummen und der V6-Protokoll-PK bleiben bytegleich und
reproduzierbar; es gibt keinen Import oder Upgrade.

`edit_phase_jobs` ergänzt dieselbe vorhandene Editreservierung mit Requestdigest,
echter Generation-/Kontextgrenze und optionalem geschlossenen `halt_kind`.
Primärschlüssel und FK sind `(run_id, job_digest, call_kind)`. Die Zeile entsteht im
selben Reserve-TX vor Provider-I/O mit leerem Halt. Identität und Caps sind
unveränderlich. Der Halt wird im vorhandenen Finish-TX gemeinsam mit der exakten
Reservierungsbeobachtung und allen Usage-Kosten gesetzt; eine fehlgeschlagene CAS
rollt beides zurück. Spätere Updates oder Löschung sind gesperrt. Generisches Finish
kann eine V3-Reservierung nicht am typisierten Vertrag vorbei abschließen. NoChange
behält seinen getrennten, bestehenden Spezialabschluss.

Gespeichert werden nur `definition`, `caller`, `test`, `unknown_need` oder `too_large`,
keine Rawantwort, freien Neednamen oder Toolanweisung. Auch Test und Unknown sind
Modellanfragen, keine Projektwahrheit und kein NoChange. IncompleteUsage,
Cancel/Stale/Unknown oder Überschreitung der echten Generationgrenze setzen keinen
Halt. Vorreservierte oder tatsächlich höhere Kosten bleiben erhalten; die Original-
frist wird nicht verlängert. Kalte Loader prüfen Request/Profile/Wire/Usage/Frist
und aktuellen Ausführungsscope erneut und erzeugen ausschließlich einen
beschreibenden Receipt, ohne Fortsetzungs- oder Aktionsautorität.

Der normale FollowupPlan-Targetentwurf004 ist auf einen frischen71-Head und
die gebundene Need70-Baseline angepasst, bleibt aber unintegriert. Seine später
ausgewählte Integration muss sämtliche Root-Fixturekorrekturen erhalten. Er ist
nicht Bestandteil dieses Need-Slices; Recovery und normaler Greenfield-Folgeplan
bleiben offene Arbeit.
