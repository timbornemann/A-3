# ADR-0090: Wertgebundene Claim-Leseidentitäten

Status: Accepted

Datum: 2026-09-07

Entscheider: Tim Bornemann (Vorabannahme der mit Plan 10 verbundenen ADRs)

Supersedes: Die unveränderte Accesskey-Darstellung aus
[ADR-0089](0089-replan-leseduplikate-im-einzelrepair.md) ausschließlich für
Claim-Inspektionen; alle anderen Schlüssel und äußeren Grenzen bleiben erhalten.

## Befund

`replan_research::read_key` hasht bei Claim-Inspektionen die Debug-Darstellung des
Ziels. `ModuleCardClaimId` gibt dort bewusst nur `ModuleCardClaimId(redacted)` aus.
Zwei unterschiedliche Claim-IDs erzeugen deshalb denselben dauerhaften Accesskey
und der zweite legitime Claim-Read wird als Duplikat abgewiesen. Debug-Ausgaben
dürfen weder fachliche Identität darstellen noch zur Behebung dieses Fehlers
private IDs offenlegen. Der Befund ist unabhängig von der Ursache der bisherigen
öffentlichen Coding-Lives, in denen kein konkretes Claim-Ziel nachgewiesen wurde.

## Entscheidung

Neue Claim-Read-Schlüssel verwenden die getrennte Derivationsdomäne
`a3.replan-claim-read.v2` und die unveränderten 32 kanonischen Bytes der Claim-ID.
Nur der resultierende Hash liegt im bestehenden ResearchWorkState. Die öffentlichen
Debug-/Display-, Journal- und Export-Redaktionsgrenzen ändern sich nicht. Andere
Readtypen behalten ihre bisherigen Schlüssel; keine Quittung wird umgeschrieben.

Der frühere kollidierende Claim-Schlüssel ist ein konstanter V1-Hash. Ein offener
Checkpoint mit dieser alten Quittung kann die tatsächlich gelesene Claim-ID nicht
mehr belegen. Der Core erfindet sie nicht und gibt den belegten Readplatz nicht frei.
Weitere Claim-Lesevorschläge in diesem Prüfstand werden deshalb als
`AmbiguousLegacyClaim` gesperrt. Ein anderer Readtyp bleibt unter den normalen
Grenzen möglich. Nach Neustart gelten dieselbe Mehrdeutigkeit und dieselben Zähler.

Diese geschlossene Legacy-Ablehnung darf wie ein nachgewiesenes Duplikat den
vorhandenen Einzelrepair nutzen, solange noch Readbudget besteht: Der feste Hinweis
fordert eine andere relevante read-only Zugangsmöglichkeit, keine weitere Claim-ID.
Ein zweiter Claim-Vorschlag bleibt nach dem einzigen Repair terminal. Die Regel
berechtigt niemals dazu, ein unbekanntes altes Ziel als ungelesen zu behandeln.
Vier Reads, aktuelle Anker, Safe Reader, Policy und Verifikation bleiben unabhängig
erzwungen. Auch eine Claim-Inspektion oder Korrektur ist kein Beweis einer Aussage.

Es gibt keine Storage-Migration, neue Datenbankspalte, Quelle im Checkpoint, neue
Abhängigkeit, größere Modellprofile oder weitere Reparaturrunde. Der bestehende
Run kann die historische Mehrdeutigkeit nicht durch bloßes Wiederaufnehmen verlieren.

## Nachweis

Rot→Grün muss verschiedene gleich lange Claim-IDs unterscheiden, dieselbe ID weiter
sperren und die vierte echte Lesung als letzte zulassen. Konstanten-V1-Fixtures und
ein real wieder geöffneter Store müssen alte Quittungen und Zähler verlustfrei halten;
Claim-Reads bleiben darin gesperrt, andere legale Reads verfügbar. Primär-/Repairtests
prüfen Legacy-Claim→anderer Read und Legacy-Claim→Claim ohne zusätzliche Capability.
Keine private ID, Pfad oder Rohantwort darf im Reparaturhinweis/Fehlertext erscheinen.
Die vollständigen lokalen Gates und die Modellnachtests bleiben erforderlich.

Referenzen: [ADR-0048](0048-rungebundene-replan-recherche.md),
[Plan 10](../plans/10-RESEARCH_WORK_STATE.md).
