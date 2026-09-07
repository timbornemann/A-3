# Plan 10: Verbindliche Rechercheführung

Status: Deterministische Baseline umgesetzt; fortgesetzte Modell- und Implementierungsabnahme läuft.\
Stand: 2026-09-07. Ausgangscommit: `31e9db7`.

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

- [x] Redundante Modellstatusnotiz aus neuen normalen AgentAction-V5-Turns entfernen;
      Core-Fortschritt, strikte Legacy-/Replan-Verträge und alle Aktionsgrenzen erhalten,
      Google und Luna live nachtesten
      ([ADR-0087](../adrs/0087-agentaktionen-ohne-modellstatusnotiz.md)).
      Vollständige lokale Gates, strikte Legacy-/Replan- und echte Providerverträge
      bestehen; Luna erreicht Done. Patch-Wiederholung und Google-Unavailable bleiben offen.

- [x] Lokale Gemini-Ablehnung des vollständigen aktuellen AgentAction-V4-Schemas
      durch bekannte `multipleOf`-Projektion beheben, unabhängige Flow-Grenzen erhalten
      und Google Gemma live nachtesten
      ([ADR-0086](../adrs/0086-gemini-projektion-von-vielfachheitsregeln.md)).
      Rot→Grün, vollständige lokale Gates und echter Nachtest bestätigen die
      Übersetzung. Der damalige Live-Fehler InvalidPublicNote wird durch ADR-0087
      für neue normale Agentturns beseitigt.

- [x] Bereits im aktuellen Index erkennbare Patchkonflikte im bestehenden Einzelrepair
      abfangen; Live-Schreibgrenzen unverändert erhalten und Qwen erneut prüfen
      ([ADR-0085](../adrs/0085-patch-snapshotkonflikte-im-einzelrepair.md)).
      30 Konflikt-/Korrekturkombinationen, vollständige lokale Gates und Luna-Done
      bestehen. Qwen wiederholt den Konflikt im Repair und bleibt gesondert offen.

- [x] `record_result` bei operationalen Specs wie eine konkrete Prüfanforderung führen;
      Read-Evidence darf keinen operationalen Verifying-Schritt erzeugen
      ([ADR-0084](../adrs/0084-ergebnisnotizen-fordern-operationale-verifikation-an.md)).
      Rot→Grün-Regression, vollständige lokale Gates und drei aufeinanderfolgende
      echte Luna-Done-Läufe; andere Modell-/Vorschauprobleme bleiben getrennt offen.

- [x] Bereits bekannte Schritt-/Run-/Worktree-/Spec- und Verifikationscommand-IDs im
      aktuellen Modellschema festlegen; dieselbe Bindung unabhängig decodieren und live prüfen
      ([ADR-0083](../adrs/0083-bekannte-schrittidentitaeten-als-schemakonstanten.md)).
      Lokal vollständig verifiziert; fünf echte Nachtests gelaufen, die getrennten
      InvalidValue-/Preview-/Provider-/Read-Restfehler sind weiterhin offen.

- [ ] Verdeckte Aktionswert-/Patchkonfliktursachen inhaltsfrei unterscheiden,
      regressionsprüfen und die konkret belegten Fortsetzungsfehler beheben.

- [x] Modellaktionen vor Freigabe durch denselben Einzelrepair gegen aktuelle Core-Anker
      prüfen; falsche IDs niemals überschreiben und Sicherheitsgrenzen erneut prüfen
      ([ADR-0082](../adrs/0082-aktuelle-aktionsanker-innerhalb-des-einzelrepairs.md)).

- [x] Portablen Prozessstart ohne undefinierte Tempvariablen ermöglichen, echte
      Prozess-/Sicherheitsverträge und mutierenden Live-Test prüfen
      ([ADR-0081](../adrs/0081-optionale-portable-tempvariablen.md)).

- [x] Offene Fehler-/Hypothesen-Memory vor optionaler Kontextverteilung reservieren;
      reproduzierten RepeatSchemaInPrompt-Abbruch regressionsprüfen und live nachtesten
      ([ADR-0080](../adrs/0080-pflichtmemory-vor-optionaler-kontextverteilung.md)).

- [x] Vorzeitiges Agent-Finish über die vorhandene Step-Verifikation führen und live
      bis zum nachgewiesenen Done prüfen ([ADR-0079](../adrs/0079-abschlussanforderung-verifiziert-zuerst-den-schritt.md)).

- [x] Tatsächlich mutierenden Live-Agent mit gesperrten Tests und unabhängiger
      Verifikation ergänzen; aufgedeckte Budgetinkompatibilität ohne Änderung des
      Modellprofils beheben ([ADR-0077](../adrs/0077-vollstaendige-agent-anker-im-konfigurierten-kontext.md),
      [ADR-0078](../adrs/0078-freien-kontext-vor-pflichtabbruch-nutzen.md)).

- [ ] Disjunkte V7-Phasenantworten gegen leeren Fortschritt implementieren und mit
      striktem Legacy-/Provider-/Mehrmodusvertrag sowie unveränderten öffentlichen
      Modellfällen nachtesten ([ADR-0075](../adrs/0075-disjunkte-rechercheantworten-statt-leerfortschritt.md)).
- [ ] Den live gescheiterten flachen V7-Unionarm durch eine discriminator-first
      Ergebnisdarstellung korrigieren, ohne Bedarf oder Evidenzregeln zu entfernen
      ([ADR-0076](../adrs/0076-entscheidungstyp-vor-ergebnisdaten-im-v7-vertrag.md)).

Fortsetzung mit Mehrprovider-Einstellungen (ADR-0066):

- [x] Explizite Live-Testauswahl für Luna und Google Gemma aus bestehenden freigegebenen
      Providerslots; keine Änderung an Rollen, Credentials, Origins oder Benutzerkatalog.
- [ ] Live-Nachtest von `gpt-5.6-luna`, `gemma-4-26b-a4b-it`, `ornith-1.5:9b`,
      `qwen38-8k:latest`, `gpt-oss:20b` und `granite4.2:8b`; lokale Modelle strikt nacheinander.
- [ ] Inhaltliche Defekte anhand der Originale und unveränderter Testfragen reproduzieren,
      gezielte Regressionen und Korrekturen statt pauschaler Budget-/Promptvergrößerung.
- [x] Leere GPT-OSS-Probe durch den unterstützten Ollama-Wirewert korrigieren,
      Offline-Grenzen und reale Recherche nachtesten
      ([ADR-0067](../adrs/0067-ollama-gpt-oss-wire-thinking.md)).
- [ ] Gemini-Schema-Zurückweisung an der Adaptergrenze korrigieren, ohne Core-
      Zulassung oder Budgets zu lockern; echte Nachtests getrennt auswerten
      ([ADR-0068](../adrs/0068-gemini-schema-arraygrenzen.md)).
- [x] Unnötige Bestätigungsfragen im Core-Testentwurf nach fertigem Änderungsentwurf
      mit enger Phase, Einzelrepair und echtem Mehrmodus-/Modellnachtest abfangen
      ([ADR-0069](../adrs/0069-core-testentwurf-ohne-bestaetigungsschleife.md)).
- [x] Gehostetes Gemma 4 mit dokumentiertem explizitem minimalem Thinking prüfen;
      unveränderte Budgets und kontrollierter Live-Vergleich, keine vermutete Abhilfe
      als Erfolg zählen ([ADR-0070](../adrs/0070-gemma-api-explizites-minimal-thinking.md)).
- [x] Live-Agent-Umsetzung auf isolierter Fixture einschließlich tatsächlicher Verification
      prüfen: Luna erreicht auf einer gesperrten Bugfix-Fixture nachweislich Done. Die
      Wiederholung scheitert jedoch; weitere Modell-/Aufgabenabnahme bleibt ausdrücklich offen.
- [x] Modellgenerierte Verwaltungsnotizen aus dem aktuellen V6-Vertrag entfernen;
      Status und nächste Pflicht aus dem zugelassenen Core-Prüfstand ableiten,
      Legacy- und Evidence-Grenzen erhalten und vergleichbar live nachtesten
      ([ADR-0071](../adrs/0071-core-eigene-recherchestatusangaben.md)).
- [x] Gezielte weitere Belege im V6-Analyze über einen eng typisierten,
      originalgebundenen Bedarf erhalten; keine freie Statusprosa und kein neuer
      Tool-/Budgetweg ([ADR-0072](../adrs/0072-typisierter-belegbedarf-ohne-statusprosa.md)).
- [x] Bereits gelieferte Navigationsstellen langer Helfer im vorhandenen Originalcache
      erhalten, ohne neue Seiten zu sperren oder Budgets zu vergrößern
      ([ADR-0073](../adrs/0073-originalgebundene-navigationsstellen-im-kontext.md)).
- [x] Originalgebundenen Belegbedarf auch in V6-Planbestandsaufnahme erhalten;
      leeren Fortschritt weiterhin abweisen und alle drei Modi prüfen
      ([ADR-0074](../adrs/0074-konkreter-belegbedarf-in-planbestandsaufnahme.md)).
- [x] Erneute vollständige Qualitätsgates und dokumentierte verbleibende Grenzen
      für ADR-0068 bis ADR-0070 und Rubrik v2. Weitere Korrekturen brauchen neue Gates.

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
