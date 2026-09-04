# ADR-0043: Core-gesteuerte Evidence-Vertiefung und begrenztes Retry

Status: Accepted

Datum: 2026-09-05

Entscheider: Tim Bornemann

Ergänzt: ADR-0038. Die dort festgelegten Recherche-, Zeit-, Aktions-, Evidence- und
Sicherheitsgrenzen bleiben unverändert.

## Kontext

Die Mehr-Runden-Recherche konnte eine formal gültige Antwort veröffentlichen, obwohl ihre eigene
öffentliche Arbeitsnotiz noch eine wesentliche Evidence-Lücke auswies. Dateinamen ohne `@` wurden
nicht zuverlässig als direkte Indexziele behandelt, und `inspectPath` konnte nur den Dateianfang
lesen. Dadurch blieb A^3 bei konkreten Fragen zu späteren Funktionen zu früh stehen.

Eine zweite Fehlerklasse entstand bei wiederholter Task-Lens-Nutzung: Der interne Phasenzähler
begann pro Lens-Kompilierung erneut bei null und regressierte dadurch die monotone Fortschrittsskala
des besitzenden Conversationjobs. Einzelne vorübergehende Source-, Search- oder Modellfehler
beendeten außerdem den gesamten Rechercheabschnitt, obwohl dessen Read-Aktionen wiederholbar und
seiteneffektfrei sind.

## Entscheidung

- Ask-Research-Decision V3 ergänzt den geschlossenen Status `sufficient | incomplete`. Solange
  das feste Profil weitere Entscheidungen erlaubt, darf der Core eine als `incomplete`
  gekennzeichnete Antwort nicht veröffentlichen. Er fordert stattdessen eine andere konkrete
  Read-only-Aktion an. Auf der letzten Entscheidung führt `incomplete` zu
  `AwaitingContinuation` mit erhaltenem Arbeitsstand.
- Repositorydateien, die im Nutzerauftrag als `@pfad`, eindeutiger Dateiname oder eindeutiges
  Pfadsuffix genannt werden, werden gegen den gebundenen Index aufgelöst und vor der freien
  Task-Lens-Auswahl gelesen. Eine Antwort darf nicht abgeschlossen werden, solange ein so
  aufgelöstes Ziel keine aktuelle sichere Source besitzt.
- `inspectPath` V3 bindet neben dem Pfad eine positive einbasierte `start_line`. Damit kann der
  Controller große Dateien über mehrere getrennte, jeweils höchstens 200 Zeilen umfassende Reads
  untersuchen. Der sichere Reader gibt den exakten nächsten Seitenanfang nur turnlokal in den
  Modellkontext zurück; er wird nicht persistiert oder an die WebView gegeben. Die WebView erhält
  dadurch weiterhin keine freie Pfadcapability; der Pfad wird nur innerhalb des gepinnten Index
  aufgelöst.
- Vorübergehend nicht verfügbare Source-Reads und Quelltextsuchen werden einmal am betroffenen
  Schritt wiederholt. Über einen Rechercheabschnitt sind höchstens vier solche Read-Retries
  zulässig. Bleibt ein Read nicht verfügbar, wird dies als begrenztes Aktionsergebnis in die
  nächste Modellentscheidung gegeben, damit ein anderer sicherer Suchweg gewählt werden kann.
  Cancellation bleibt terminal; Policy-, Secret-, Binary-, Größen- und Stale-Ablehnungen werden
  nicht durch Retry umgangen.
- Ein vorübergehend fehlgeschlagener Modellentscheid wird höchstens einmal erneut angefordert und
  verbraucht eine weitere der bereits fest begrenzten Modellentscheidungen. Ungültige strukturierte
  Ausgabe besitzt weiterhin über den ganzen Abschnitt nur den einen Reparaturversuch aus ADR-0038.
- Untergeordnete Task-Lens-Läufe übernehmen weiterhin Cancellation vom Conversationjob, melden
  ihren bei jeder Runde neu beginnenden Fortschritt aber nicht an dessen monotone äußere Skala.
  Der Conversationbesitzer meldet ausschließlich seine groben festen Fortschrittspunkte.
- Der Evidence-Status steuert nur den endlichen Recherchecontroller. Er wird weder als fachliche
  Evidence noch als Ausführungsautorität persistiert. V30 bis V33 und die öffentlichen
  Arbeitsnotizen bleiben unverändert lesbar; eine Knowledge-Migration ist nicht erforderlich.

## Konsequenzen

### Positiv

- Eine explizit erkannte Evidence-Lücke führt zu weiterer Eigeninitiative statt zu einer
  vorschnellen Antwort oder einer Bitte um bereits vorhandene Dateien.
- Ask, Plan und Agent-Vorbereitung können Funktionen in späteren Dateiabschnitten systematisch
  nachlesen und profitieren gemeinsam vom gleichen Verhalten.
- Ein transienter Fehler verliert weder vorherige Quellen noch den bisherigen Rechercheweg.
- Wiederholte Recherche-Runden können den Schedulerfortschritt nicht mehr rückwärts setzen und
  dadurch den Job abbrechen.

### Negativ

- Das Modellformat und `inspectPath` erhalten eine neue inkompatible Schema-Version.
- Materiale Lücken benötigen unter Umständen zusätzliche Modellentscheidungen und Reads innerhalb
  des unveränderten Profils.
- Semantische Relevanz bleibt nicht allein aus Zitatordinalen beweisbar; der Core erzwingt sichere
  Herkunft, explizite Zielabdeckung und die Selbstauskunft, nicht die fachliche Wahrheit des LLM.

### Risiken und Gegenmaßnahmen

- Falsche `sufficient`-Selbstauskunft: explizit genannte Indexziele werden zusätzlich durch den
  Core als harte Abdeckung geprüft; Antworten ohne erforderliche Zitate bleiben unzulässig.
- Retry-Schleifen: ein Retry je Read, vier je Abschnitt, eine erneute Modellanforderung sowie die
  bestehenden Entscheidungs-, Aktions-, Stagnations- und Zeitbudgets bleiben harte Grenzen.
- Erweiterte Dateiinspektion: jede Seite wird erneut gegen FileRevision, kanonischen Worktreeroot,
  Secret-, Binary-, Größen- und UTF-8-Regeln validiert.

## Compliance

- Codec-Tests prüfen V3, den geschlossenen Evidence-Status und positive paginierte Pfadreads.
- Desktop-Regressions prüfen, dass zwei Task-Lens-Runden denselben Conversationjob nicht durch
  regressierenden Fortschritt abbrechen.
- Controllernahe Tests prüfen, dass `incomplete` sowie fehlende explizite Zielabdeckung weitere
  Recherche verlangen.
- Prompt- und Pfaderkennungstests prüfen verpflichtende Vertiefung, alternative Suchwege und
  Dateinennungen ohne `@`.
- Workspace-Tests, Clippy und Rustdoc bleiben Abschlussgates.

## Referenzen

- [ADR-0030](0030-bounded-evidence-source-preview.md)
- [ADR-0037](0037-nachvollziehbare-adaptive-ask-recherche.md)
- [ADR-0038](0038-agentische-mehr-runden-recherche.md)
- [ADR-0039](0039-evidenzgebundene-slash-commands.md)
- [Job-Laufzeit](../JOB_RUNTIME.md)
- [Memory und Context](../MEMORY_AND_CONTEXT.md)
- [Security und Ausführung](../SECURITY_AND_EXECUTION.md)
- [Plan 06](../plans/06-DESKTOP_PRODUCT.md)
