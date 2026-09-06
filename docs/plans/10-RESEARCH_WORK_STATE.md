# Plan 10: Verbindliche Rechercheführung

Status: Implementierung und Modellläufe abgeschlossen; inhaltliche Praxisabnahme offen.\
Stand: 2026-09-06. Ausgangscommit: `31e9db7`.

Entscheidungen: [ADR-0047](../adrs/0047-verbindlicher-recherchearbeitsstand.md),
[ADR-0048](../adrs/0048-rungebundene-replan-recherche.md),
[ADR-0049](../adrs/0049-core-planpflichten-und-statusnotizen.md),
[ADR-0050](../adrs/0050-verlustfreie-entwurfsuebergabe.md),
[ADR-0051](../adrs/0051-core-fallback-fuer-ungeeignete-zerlegung.md),
[ADR-0052](../adrs/0052-literale-teilpflichten-im-core-fallback.md) und
[ADR-0053](../adrs/0053-core-pflichten-fuer-benannte-codefragen.md) und
[ADR-0054](../adrs/0054-vollstaendig-gelieferte-planbestandsaufnahme.md) und
[ADR-0055](../adrs/0055-libsql-einmalige-verbindungsfreigabe.md) und
[ADR-0056](../adrs/0056-vollstaendige-passende-originalpakete.md) und
[ADR-0057](../adrs/0057-leerer-entwurf-ist-kein-rechercheauftrag.md) und
[ADR-0058](../adrs/0058-kompakte-recherchephasen-fuer-kleine-kontexte.md) und
[ADR-0059](../adrs/0059-idempotente-originalanker-in-rechercheergebnissen.md).
Die ADR-Freigabe ersetzt keine bestandene Abnahme.
Messwerte, Rohberichtzuordnung und getrennte inhaltliche Befunde stehen im
[Verifikationsprotokoll](10-RESEARCH_VALIDATION.md).

## Ziel, Akzeptanz und Grenzen

Ask, Plan und Agent-Vorbereitung teilen denselben dauerhaften Prüfstand:
unveränderter Originalauftrag, stabile Teilpflichten, aktuelle Originalbelege,
deterministische nächste Arbeit und teilfragengebundener Abschluss.
Automatisches Replan verwendet denselben ResearchWorkState im bestehenden Run.

Akzeptiert ist der Schnitt erst mit verifizierten Zustands-/Grenzregressionen,
vollständigen lokalen Qualitätsgates und der dokumentierten Modellmatrix.
Identische bestätigte Analysepakete und adaptive Lesezugriffe dürfen keine
Steuerungsschleife erzeugen. Ungeklärte Pflichtfragen dürfen nicht als beantwortet
erscheinen. Rechercheergebnisse sind keine Implementierungsverifikation.

Kein neuer Index, kein allgemeines UI-Redesign, kein Ersatz des mutierenden
Controllers, keine neue Abhängigkeit und keine Erweiterung von Sicherheitsrechten
oder äußeren Recherchebudgets. Berechtigungsgrenzen und wirklich folgenreiche
fehlende Nutzerentscheidungen bleiben Stopgründe.

## Vertikale Schnitte

- [x] Versionierte Domain-Typen, stabile Teilfragen, explizite epistemische Arten,
      Abhängigkeiten, Ergebnis-/Evidence-Validierung und transitive Invalidierung.
- [x] V5-Phasen Initialize, Analyze, SummarizeOriginals, Design und Legacy-Finalize; getrennte strikte
      Schemas und unabhängige Zulassung, maximal ein Repair je ungültigem Dokument.
- [x] Deterministische Arbeitsauswahl und aktive Teilansichten ohne neue Budgets.
      Kanonische Originalpakete und persistierte Zugriffsschlüssel messen echte Arbeit.
- [x] Begrenzt negative Ergebnisse nur aus vollständigen unabhängigen Quittungen
      im aktuellen Scope; Fehler, Cancellation und Erschöpfung beweisen keine Abwesenheit.
- [x] Core-Planpflichten statt Suche nach noch nicht implementierten Features;
      vollständige Designübergabe und verlustfreier Core-Planabschluss.
- [x] V36-Persistenz mit atomarem Checkpoint/Event, Reopen und Legacy-Erhalt;
      read-only Prüfliste mit strikten IPC-/UI-Regressionsfällen.
- [x] Inhaltlicher Agent-Handoff mit konkreten Umsetzungskriterien und
      Invalidierung stale Ergebnisse vor erneuter Verwendung.
- [x] Vollständiger run-/step-/snapshotgebundener Replan-Unterauftrag in V37:
      echte Originalreads, V5-Analyse vor neuer Mutation, kein künstliches Todo
      mit kopierter Änderungsverifikation. Vier Reads bleiben über Reopen erhalten.
- [x] Replan-Hydration ausschließlich aus markierten Originalfenstern; Suchmetadaten,
      falscher Eigentümer und veraltete Bytes können keinen Abschluss autorisieren.
- [x] Modell-/Readfehler erhalten ihre Abrechnung; sichere Stream-Unterursachen
      und getrennte Repairs/Transport-Retries ohne Budgeterneuerung.
- [x] Reale Mehrdatei-/Mehrmodus-Negativregressionen mit Git, Fast Index, Safe Reader
      und libSQL; V36/V37-Migrationen, vollständige Vorgängermatrix und Fehlerrollback.
- [x] Native libSQL-Doppelfreigabe lokalisiert und minimal in der unverändert
      gepinnten Version korrigiert; retry-freier Lebensdauertest mit 1.024 Zyklen besteht.
- [x] Abschließende lokale Qualitätsgates auf dem korrigierten Produktionsstand.
- [x] Lokale 60er-Vorher-/Nachher-Messung mit identischem stärkerem Modell.
- [ ] Inhaltliche Praxisabnahme über alle geprüften Modelle: bekannte falsche oder
      unvollständige Aufrufketten, fehlende konkrete Tests und widersprüchliche
      CSV-Entwürfe trotz erfüllter Begriffrubrik bleiben offen; auch Luna ist hiervon
      nicht pauschal ausgenommen.
- [x] Abschließende Prüfung mit dem eingestellten `gpt-5.6-luna`, Sichtprüfung der
      Antworten und Behebung relevanter Befunde.

## Reproduzierbare Praxisabnahme

Fortsetzung nach Sicherungscommit `35630d6`:

- [x] Den erneut live reproduzierten 8k-Planabbruch durch doppelte Dialogreserve
      korrigieren und mit langem vollständigem Entwurf sowie echten Providerpaketen prüfen
      ([ADR-0060](../adrs/0060-aktueller-pruefstand-vor-optionaler-dialogreserve.md)).
- [x] Luna und die freigegebenen lokalen Modelle nachtesten; Inhaltsbefunde und
      Ablaufresultate getrennt auswerten, weitere konkrete Ursachen direkt korrigieren.
- [x] Leere zusätzliche V5-Statushinweise neutral darstellen, ohne Ergebnis- oder
      Belegprüfung zu umgehen; unabhängiger Decoder und realer Mehrmodusvertrag
      ([ADR-0061](../adrs/0061-neutrale-v5-statushinweise-ohne-repair.md)).
- [x] Den live beobachteten kopierten Core-Testauftrag als Scheinergebnis abweisen;
      Einzelrepair, ehrlichen Fehler und unveränderte Ask-Antworten prüfen
      ([ADR-0062](../adrs/0062-core-arbeitsauftrag-ist-kein-ergebnis.md)).
- [x] Wiederholte gültige V5-Statusquellen ohne Repair übernehmen, Eingabegrenze
      vor Kanonisierung und strikte S-Schreibweise unabhängig prüfen
      ([ADR-0063](../adrs/0063-idempotente-v5-statusquellen.md)).
- [x] Abschließende Qualitätsgates und aktualisiertes Prüfprotokoll.
- [x] Späte passende Bestandsdetails an abhängige Entwürfe übergeben, vollständige
      Designentscheidungen schützen und unvermeidbare Auszüge markieren
      ([ADR-0064](../adrs/0064-budgetierte-bestandsuebergabe-an-entwuerfe.md)).
- [x] Zusammengehörige Kommalisten nicht in zusätzliche isolierte Recherchepflichten
      zerlegen; wörtliche Abdeckung, alte Verträge und Quellenprüfung erhalten
      ([ADR-0065](../adrs/0065-zusammenhaengende-listen-im-core-auftrag.md)).
- [x] Den fünfmal reproduzierten Gemma-Originalabdeckungsabbruch mit konkreten
      tatsächlichen E-Ankern im vorhandenen Einzelrepair nachtesten: zweimal 5/5
      Abschlüsse nach Korrektur. Alternative Fenster, fehlende Originale,
      768-Byte-Grenze und unveränderte acht Gruppen unabhängig geprüft.
- [x] Opt-in Testdiagnostik für identische Transkripte und tatsächliche numerische
      Quellenzuordnung ergänzen; keine allgemeine Produktionsprotokollierung,
      Quelltexte, beliebigen Pfade oder freien Modellausgaben in diesen neuen Feldern.

Die Korrekturen sind nach vollständigen Rust-/Frontend-Gates in `030f2f1` gesichert.
Die letzten Zwölfermatrizen haben je 12/12 Abschlüsse ohne Nutzerhalte; die
Begriffrubrik bleibt bei den drei lokalen Modellen mit je 11/12 rot. Auch Lunas
12/12 und seine gesonderte 60/60-Serie beweisen keine widerspruchsfreien Antworten.
Die oben offene inhaltliche Praxisabnahme wird durch diese technischen Nachweise
nicht geschlossen. Zuordnung, Fehlberichte, Sichtprüfung und Einschränkungen
stehen vollständig im [Prüfprotokoll](10-RESEARCH_VALIDATION.md).

[Research evaluation v1](../../fixtures/research-eval-v1/README.md) enthält das
synthetische Mehrdateiprojekt, die vier festen Aufgabenfamilien, drei Formulierungen,
die notwendige Konzept-Rubrik und einen überprüfbaren Baseline-Testpatch.
Fünf Wiederholungen ergeben jeweils 60 Fälle; die Originaldateien bleiben bytegleich.

Der Nutzer hat den konfigurierten Provider zusätzlich ausdrücklich freigegeben
und stärkere installierte lokale Modelle benannt. Lokal wird `ornith-1.5:9b`
mit identischen Profilwerten vorher/nachher geprüft. `qwen38-8k:latest` wurde mit
8.192 Kontext / 2.048 Output nachgetestet; Gemma mit 16.384 / 4.096.
Die gesonderten lokalen Fehlschläge sind im Verifikationsprotokoll ausgewiesen.
Der abschließende ADR-0059-Stand erzielt mit Ornith und Luna jeweils 60/60 Abschlüsse
und Begrifftreffer ohne Nutzerhalte oder adaptive Reads. Gegenüber 37/60 Abschlüssen
der lokalen Baseline ist die Ablaufverbesserung gemessen; die separat dokumentierten
inhaltlichen Gegenbeispiele verhindern weiterhin die vollständige Praxisabnahme.
Es erfolgt keine Installation,
kein Download und keine Änderung der App-Profile. Der konfigurierte Katalog wird
ausschließlich read-only gelesen; Zugangsdaten bleiben im vorhandenen nativen Adapter.

Messwerte unterscheiden tatsächlichen Abschluss, notwendige Konzeptabdeckung,
Nutzerhalt, adaptive Reads, identische adaptive Wiederholungen, Modellaufrufe,
übertragene Kontext-UTF-8-Bytes und Laufzeit. Die Rubrik beweist keine semantische
Wahrheit beliebiger Modellaussagen. Öffentliche Fixture-Antworten bleiben zur Sichtprüfung
erhalten. Ergebnisse unterschiedlicher Modelle sind kein Harness-Geschwindigkeitsvergleich.

## Bereits nachgewiesene Korrekturen

- Gleiche Frage plus gleiches tatsächlich geliefertes Paket wird nach gültiger Analyse
  nicht erneut an das Modell geschickt. Fehlgeschlagene Antworten quittieren kein Paket.
- Bestehende Fakten und künftiger Entwurf haben getrennte Phasen, Belegregeln und
  Reparaturhinweise. Die Bestandsanalyse fordert keinen vorgezogenen Entwurf an.
- Der Regressionstest für fehlende benannte Originale in Plan-Q1 scheiterte zunächst.
  Der Core prüft deren vollständige Quellenabdeckung jetzt vor dem Übergang zu
  belegfreien Designphasen, sodass der Einzelrepair die noch passende Bestandsphase hat.
- Der zunächst rote Originalpaket-Test besteht: Vollständig gelesene benannte Dateien,
  die einschließlich Header gemeinsam passen, verdrängen konkurrierende Funktionsfragmente.
  Imports und Initialisierung bleiben sichtbar, ohne zusätzliche Reads oder größeres Budget.
  Expliziter Zeilenfokus und progressive Auswahl bei Überlauf bleiben unverändert.
- Der Core-Plan injiziert nicht aufgelöste neue Symbole nicht mehr als zusätzliche
  Verzeichnis-Suchpflicht. Der vollständige Nutzerauftrag bleibt erhalten; die Ask-
  Navigation bleibt unverändert. Auch hierfür liegt eine Rot→Grün-Regression vor.
- Die Domain prüft Ergebnisart und Frageart jetzt in beiden Richtungen, auch bei Reopen.
  Negative Suche und Bestandsinterpretation können keinen Entwurf abschließen.
- Leerer Designfortschritt wird einmal repariert und niemals als neue Leserunde
  interpretiert. Erfolgreicher und erneut ungültiger Repair sind für Plan/Agent mit
  echtem Index, Reader und Store abgesichert; tatsächlich nötige Nutzerfragen bleiben möglich.
- Vollständige relative Originalpfade werden bereits in ihrer eigenen Teilfrage
  auf Quellenabdeckung geprüft. Der Einzelrepair erhält konkrete aktuelle E-Gruppen,
  ohne erfundene Quellen, neue Reads oder mehr Reparaturbudget.
- Wiederholte gültige E-Anker werden nach vollständiger Einzelprüfung als Menge
  übernommen. Ein Widerspruch zwischen Schema und Decoder verbraucht so nicht mehr
  den Repair. Decoder und echter Ask-/Plan-/Agent-Controller wurden rot→grün geprüft;
  falsche, zusätzliche, übergroße oder nicht ausgelieferte Belege bleiben abgewiesen.
- Der tatsächliche 8k/2k-Vertrag besteht nach Reduktion redundanter Phaseninstruktionen,
  kompakter Arbeitsansicht und Vorrang von Originalen vor abgeleiteter Navigation:
  Ask, Plan und Agent-Vorbereitung erhalten die fünf zusammengehörigen Methodenkörper
  ohne größeres Kontextfenster. Der echte Modellnachtest bleibt davon getrennt.
- Späte Designentscheidungen bleiben vollständig in abhängigen Schritten und im Plan;
  kein zusätzlicher Finalizer darf Fehlerpolitik oder Testgarantien verändern.
- Eine formal gültige, aber reine Design-Initialzerlegung einer expliziten Codefrage
  erhält den Originalauftrag über den eng begrenzten Core-Fallback. Ungültige
  Dokumente und bereits gespeicherte Verträge werden nicht umgeschrieben.
- Rein repositorybezogene Initialzerlegungen über benannte Dateien verwenden ebenfalls
  wörtliche Core-Pflichten statt überlappender zusätzlicher Modellaufträge. Ein freier
  Zusammenfassungsauftrag kann so keine bereits beantwortete Recherche blockieren.
- Satzzeichen hinter Dateinamen erzeugen keine erfundenen Fehlpfade. Derselbe Audit-Prompt
  benötigte vor der Korrektur zwölf adaptive Reads, danach null (Luna-Einzelläufe).
  Wörtlich zitierte Sonderzeichen in Pfaden bleiben erhalten.
- Replan-Originalmarker werden unabhängig von flüchtigen Quellseiten gespeichert,
  bei Reopen erneut über den Safe Reader geprüft und bei Hash-/Scopewechsel abgelehnt.
- Die Storage-Regression für einen unmarkierten Suchspan wurde zunächst rot und
  besteht mit der engen Originalmarker-/Step-/Snapshotbindung.
- Die fehlende V36→V37-Vorgängerprüfung wurde ergänzt; alle 38 gezielten
  Upgrade-/Identitätsprüfungen und der zusätzliche V37-Fehlerrollback bestehen.

## Gate-Protokoll und Grenzen

Erfolgreiche Befehle auf dem korrigierten Stand:

- `cargo fmt --all --check`
- `cargo test -p a3-domain research_work --offline --locked` (10 Tests)
- `cargo test -p a3-application research --offline --locked` (35 Tests)
- `cargo test -p a3-desktop --lib research --offline --locked -- --test-threads=1`
  (83 Tests, drei ausdrücklich opt-in Live-Tests ignoriert)
- `cargo test -p a3-storage-libsql --lib checkpoint_and_event_commit --offline --locked -- --nocapture --test-threads=1`
- `cargo test --workspace --all-features --offline --locked -- --test-threads=1`
  (einschließlich nativer Lebensdauer-, Grenz-, Migrations-, gemeinsamer Storage- und Doc-Tests)
- `cargo clippy --workspace --all-targets --all-features --offline --locked -- -D warnings`
- `pnpm ci:frontend`, `pnpm check:links`, `git diff --check`
- `pnpm report:dependencies` mit `CARGO_NET_OFFLINE=true`

Ein zwischenzeitlich fehlgeschlagener Speichertest konstruierte die inzwischen zu Recht
abgelehnte Ergebnisart für eine Entwurfsfrage. Sein Negativfall verwendet jetzt die passende
Art mit unverändert ungültigem Originalbezug, damit weiterhin die unabhängige SQL-Quellenprüfung
und deren atomarer Rollback getestet werden. Danach besteht der vollständige Workspace erneut.

`pnpm ci:frontend` besteht mit Formatter, Lint, Typecheck, 373 Tests, 14 bestehenden
Skips, fünf Tooltests und Build. Die lokale Node-Version 25.6.1 weicht vom
festgelegten 24.14.0 ab; bestehende BigInt-Buildwarnungen bleiben sichtbar.
Native plattformübergreifende UI-/Releaseabnahme bleibt dem CI-/Releasegate vorbehalten.
Ältere Windows-libSQL-Worker-Retries sind kein Nachweis nativer Stabilität.
Der jetzt belegte doppelte Connection-Drop wird über den dokumentierten
[lokalen Patch](../../vendor/LIBSQL_PATCH.md) beseitigt. Die abschließenden Gates und
ADR-0056-Praxisläufe enthalten diesen Patch; alte Crash-Teilberichte bleiben sichtbar.

Keine Providerkonfiguration verändert, keine privaten Projekte für Modelltests
verwendet, keine Benutzer-Knowledge-Datenbank migriert. Bis zu diesem historischen
ADR-0059-Gate kein Commit; der anschließend ausdrücklich beauftragte Sicherungscommit
ist `35630d6`. Kein Push oder Release.
