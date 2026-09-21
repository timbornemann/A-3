# ADR-0112: Nachgelagerte evidenzgebundene Greenfield-Verifikation

Status: Accepted  
Datum: 2026-09-21  
Entscheider: Tim Bornemann

Freigabe: ausdrückliche Annahme aller für den Auftrag erforderlichen ADRs und Auftrag, den
Agenten bis zu einem tatsächlich funktionsfähigen, evidenzgebundenen Ausführungspfad umzubauen.

## Kontext

Ein Agent-Auftrag in einem veröffentlichten leeren Worktree kann nach ADR-0111 inzwischen eine
ehrliche Bestandsaufnahme, einen begrenzten Entwurf und einen Core-validierten Arbeitsplan
erzeugen. Bei der Materialisierung besitzt der leere Index jedoch noch keinen manifestbelegten
Command-Katalog. ADR-0042 verlangt dann für jeden Schritt `UserConfirm`.

Dieser Fallback ist im aktuellen produktiven Pfad nicht ausführbar: Der Mutationscontroller kann
nach einem Patch nur Diff- oder Prozessevidence erzeugen, und kein produktiver Use Case erzeugt
die für einen `UserConfirm`-Schritt erforderliche `UserConfirmationEvidence`. Ein erfolgreicher
Patch kehrt deshalb zu `Execute` zurück, ohne den Schritt abschließen zu können. `Finish` kann
ebenfalls keine fehlende Step-Evidence ersetzen. Der Agent würde erneut das Modell befragen,
obwohl die offene Grenze vollständig deterministisch bekannt ist.

Ein im leeren Projekt dauerhaft eingefrorener Command-Katalog ist außerdem zu früh: Erst sichere
Dateiänderungen können ein Manifest oder eine erkennbare Teststruktur erzeugen, aus der ein
aktueller Prüfweg abgeleitet werden kann. Weder das Modell noch ein Freitextplan dürfen daraus
freie Prozesse, Paketinstallation oder Erfolgsbehauptungen ableiten.

## Entscheidung

Diese ADR ersetzt ausschließlich den `UserConfirm`-Fallback für bei der Materialisierung noch
nicht automatisch prüfbare Schritte aus ADR-0042. Alle übrigen Entscheidungen von ADR-0042,
ADR-0079, ADR-0109 und ADR-0111 bleiben bestehen.

- Ein Änderungsschritt ohne passenden aktuellen Projektcheck erhält eine
  `DiffInvariant`-Verifikation. Sie beweist ausschließlich, dass der exakt freigegebene Patch auf
  dem erwarteten Snapshot vollständig angewendet und neu indiziert wurde. Sie beweist kein
  Laufzeitverhalten.
- Ein Testschritt ohne passenden aktuellen Projektcheck erhält eine geschlossene, typisierte
  nachgelagerte Command-Verifikation mit einer geordneten Menge zulässiger Command-Arten. Sie
  enthält keine argv, keine ausführbare Command-ID und keine Modellautorität.
- Unmittelbar bevor ein solcher Testschritt beginnt, rekonstruiert der Core den Command-Katalog
  aus dem dann aktuellen Published Index. Genau ein passender Befehl wird durch eine atomare
  Planrevision an den Schritt gebunden. Der alte Schritt bleibt als Historie erhalten. Diese
  Bindung erhält einen eigenen Application-Use-Case; sie wird weder als modellgetriebener Replan
  ausgegeben noch in `ContinueVerifiedAgentPlan` oder im Desktop-Executor ad hoc mutiert.
- Die Command-Discovery darf zusätzlich zu Manifesten nur explizit implementierte, geschlossene
  Standardbibliothek-Adapter verwenden. Der erste Adapter erkennt aktuelle Python-Quellen und
  `unittest`-Testdateien und bietet ausschließlich den direkten Offline-Prozess
  `python -B -m unittest discover` im belegten Projektverzeichnis an. Repositoryinhalt bleibt
  untrusted; die Ausführung benötigt weiterhin eine exakte Nutzerfreigabe.
- Ein neu entdeckter, noch nicht dauerhaft bestätigter Befehl wird als exakte Process-Aktion im
  vorhandenen Approval Center gezeigt. `AllowOnce` gilt nur für Run, Step, Snapshot, executable,
  argv, Arbeitsverzeichnis, Netzwerkverbot und Ressourcenlimits. Es erzeugt keine allgemeine
  Allowlist und führt noch nichts aus. Eine Core-eigene, nicht klonbare Preparation-Capability
  bindet dafür ausschließlich einen Befehl aus dem aktuellen Katalog an den aktuellen Step und
  verschärft dessen Policy auf `RequireApproval`; ohne diese Capability darf ein nicht bestätigter
  Katalogbefehl nie `ProcessPlanBinding::Validated` erhalten. Eine bereits aktuelle dauerhafte
  Command-Bestätigung bleibt verwendbar.
- Ist beim Start des Testschritts weiterhin kein belegter Prüfweg verfügbar, hält der Core den
  Lauf mit einem dauerhaften, verständlichen `AwaitingUser`-Blocker an. In diesem Zustand wird das
  Modell nicht erneut gefragt und darf weder Dateien, Befehle noch Erfolg erfinden.
- Paketinstallation, Netzwerk, Shellmodus, freie argv, externe Writes, Git-Push und Publikation
  bleiben ausgeschlossen beziehungsweise benötigen ihre unveränderte ausdrückliche Freigabe.

### Geschlossene Implementierungsgrenzen

- `VerificationTarget` erhält eine eigene persistierbare Deferred-Variante; sie ist weder
  `UserConfirm` noch bereits ausführbar. Die dazugehörige Knowledge-Schema-Migration erweitert
  nur die typisierte Step-Verification-Tabelle und bewahrt ältere Ledger unverändert.
- Die Auflösung geschieht beim Übergang vom verifizierten Vorgängerschritt zum noch nicht
  gestarteten Deferred-Schritt. Der Core ersetzt diesen Ready-Schritt mit neuer Step- und
  Verification-ID, remappt seine Nachfolger und committed Ledgerrevision plus Run-Event atomar,
  bevor irgendein neuer Modellturn kompiliert wird.
- Der Python-Adapter wird nur angeboten, wenn der aktuelle Index mindestens eine Python-Quelle
  und mindestens eine kanonische `test*.py`-Datei unter der Paketwurzel enthält. Seine
  Discovery-Evidence ist begrenzt und kanonisch; uneindeutige Wurzeln oder überschrittene Grenzen
  liefern keinen Command statt einer geratenen Auswahl.
- `python -B -m unittest discover` wird nicht über den generischen Exitcode als strukturierter
  Testerfolg ausgegeben. Ein geschlossener Adapter wertet vollständig drainte, begrenzte
  `unittest`-Ausgabe und Exitstatus aus und erzeugt nur bei konsistentem Fallzähler und Erfolg
  passende `TestEvidence`. Unbekannte, abgeschnittene oder widersprüchliche Ausgabe bleibt
  unverifiziert.
- Die einmalige Prozessfreigabe verwendet die vorhandenen Approval-Request-, Grant-, CAS-,
  Ablauf- und Consume-once-Verträge. IPC und WebView erhalten keine Provider-, Worktree-, Run-,
  Snapshot- oder Policy-Identitäten und keine Möglichkeit, argv zu verändern.

## Konsequenzen

### Positiv

- Ein leerer Worktree kann sichere Add-Patches schrittweise ausführen und jeden geänderten
  Snapshot belegen, ohne Verhalten aus einem Diff abzuleiten.
- Erst nach den Änderungen vorhandene Prüfwege werden aus aktuellem Repositorybeleg statt aus
  einem veralteten Startkatalog gebunden.
- Fehlende Verifizierbarkeit endet als konkreter menschlicher Haltepunkt statt als Modellloop.
- Ein Greenfield-Python-Projekt kann ohne Paketinstallation echte Standardbibliothek-Tests über
  den normalen Approval-, Process- und Evidence-Pfad ausführen.

### Negativ

- Verification-Spec und Ledger benötigen eine neue typisierte Deferred-Variante sowie eine
  zusätzliche atomare Planrevision vor einem Testschritt.
- libSQL benötigt eine vorwärtsgerichtete Knowledge-Migration; Domain-, Storage-, Context-,
  Protocol- und TypeScript-Projektionen müssen die neue geschlossene Variante exhaustiv abbilden.
- Das Approval Center muss neben dauerhaft bestätigten Katalogbefehlen auch einen exakt
  snapshotgebundenen einmaligen Katalogbefehl darstellen können.
- Weitere Sprachen benötigen jeweils einen eigenen geschlossenen Discovery-Adapter; es gibt
  keinen generischen Modellfallback.

### Risiken und Gegenmaßnahmen

- Diff-Evidence wird als Verhaltensbeweis missverstanden — sie ist nur für Änderungsschritte
  zulässig; Testschritte verlangen weiterhin operationale Evidence.
- Repositorytests führen untrusted Code aus — direkte argv, Netzwerkverbot, Ressourcenlimits,
  exakte Freigabe und bestehende Prozessisolation bleiben verpflichtend.
- Ein später Katalog könnte stale sein — Bindung, Approval und Ausführung revalidieren denselben
  aktuellen Snapshot und scheitern bei jeder Abweichung geschlossen.
- Automatische Planrevision könnte offene Arbeit verlieren — nur der noch nicht gestartete
  Deferred-Schritt wird ersetzt; IDs, Revision und pensionierter Vorgänger bleiben im Journal.

## Verworfene Alternativen

- Alle Greenfield-Schritte per Diff als erfolgreich markieren — würde Laufzeitverhalten und Tests
  ohne Evidence behaupten.
- `UserConfirm` beibehalten und das Modell weiterlaufen lassen — der produktive Evidence-Erzeuger
  fehlt; Wiederholung kann die Grenze nicht lösen.
- Freie Modellbefehle zulassen — widerspricht der argv-, Policy- und Evidence-Grenze.
- Schon bei der ersten Materialisierung ein Framework oder Manifest erfinden — wäre keine
  repositorybelegte Entscheidung und könnte Installation oder Netzwerk voraussetzen.

## Compliance

- Domain-Tests trennen Änderungsschritt/Diff, Testschritt/Deferred und verbieten Deferred-argv.
- Materialisierungs- und Storage-Contracts prüfen Roundtrip, Migration und unveränderte bestehende
  Projekte mit manifestbelegten Commands.
- Planfortsetzungstests prüfen neue Identitäten, Dependency-Remapping, pensionierte Historie,
  atomaren CAS und dass vor der Bindung kein Modellturn und kein Prozess beginnt.
- Command-Discovery- und Parsertests prüfen kanonische Python-Wurzeln, direkte argv, `-B`,
  fehlende/zu viele/mehrdeutige Testdateien, abgebrochene oder widersprüchliche Ausgabe und echte
  `unittest`-Fallzahlen.
- Executor-Tests verwenden einen echten leeren Git-Worktree: Planmaterialisierung, Add-Patch,
  exakte Freigabe, Reindex, Diff-Evidence, nächste Schritte, nachgelagerte Command-Bindung,
  einmalige Process-Freigabe, echte Test-Evidence und Acceptance bis `Done`.
- Negativtests prüfen fehlenden Adapter, stale Snapshot, veränderte Tests, manipulierte argv,
  verweigerte Freigabe, Netzwerk- und Shellanforderung sowie den modellfreien `AwaitingUser`-Halt.
- Ein ausdrücklich gestartetes Live-Fixture mit dem konfigurierten Coding-Modell darf erst nach
  physischer Zusatzprüfung von Dateien und Verhalten als grün gelten.

## Referenzen

- [ADR-0012](0012-safe-tools-and-approval-policy.md)
- [ADR-0042](0042-adaptiver-agent-arbeitsplan.md)
- [ADR-0079](0079-abschlussanforderung-verifiziert-zuerst-den-schritt.md)
- [ADR-0109](0109-autonomer-agent-einstieg.md)
- [ADR-0111](0111-leerer-index-bestandsaufnahme.md)
- [Plan 16](../plans/16-AGENT-GREENFIELD-AND-EXISTING.md)
