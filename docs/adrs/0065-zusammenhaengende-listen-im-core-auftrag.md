# ADR-0065: Zusammenhängende Listen im Core-Auftrag

Status: Accepted\
Datum: 2026-09-06\
Freigabe: Fortgesetzter Nutzerauftrag zu Plan 10 einschließlich direkter Modelltestkorrekturen und zugehöriger ADRs.\
Ersetzt ausschließlich die Komma-/Doppelpunkt-Segmentierung aus ADR-0052/0053.

## Befund

Orniths ADR-0063-Matrix `eval-1788709222744.jsonl`, Storage 0:1, hält bei der isolierten
Pflicht `Umgebungsvariable,`. Andere abgeschlossene Antworten nennen die Variable und
ihre Verwendung bereits. Die ursprüngliche Formulierung war eine zusammengehörige
Aufforderung mit Aufzählung. Der Core schaltet nach der ersten Satzgrenze dauerhaft
auf Kommasplitting um und erzeugt daraus einzelne Nominalfragmente ohne Verb.
Das erhöht Modellaufrufe und eröffnet voneinander isolierte, überlappende Recherche.
Der Livebefund beweist nicht, dass jede Liste semantisch vollständig beantwortet wird.

## Entscheidung

Kurze Core-Aufträge werden weiterhin ausschließlich wörtlich, geordnet und außerhalb
von Zitaten segmentiert. Kommas und Doppelpunkte sind keine Pflichtgrenzen mehr:
Aufzählungen und ihre Einleitung bleiben zusammen, auch nach früheren Sätzen.
Satzende, Semikolon und Zeilenumbruch bleiben die bestehenden mechanischen Grenzen.
Die Grenze von sechs Pflichten, der unveränderte zusammenhängende Rest und die
512-Byte-Grenze bleiben erhalten. Es werden keine neuen Worte, Unterfragen oder
Abhängigkeiten ergänzt und keine Ergebnisse aus fremden Fragen übernommen.

Dies gilt nur beim bereits zugelassenen Einfrieren neuer Core-Codefragen. Gemischte
Modellverträge, gespeicherte Verträge und Core-Planpflichten werden nicht umgeschrieben.
Jede verbleibende Pflicht braucht weiterhin ihr eigenes validiertes Ergebnis; alle
genannten Originale behalten ihre Quellenanforderungen. Kein Budget, Repair, Recht
oder impliziter Abschluss wird hinzugefügt. Vollständigkeit natürlicher Antworten
bleibt eine getrennte Qualitätsfrage, keine durch Segmentierung bewiesene Tatsache.

## Verifikation

Direkte Rot→Grün-Regressionen prüfen die echte Storage-Frage, eine Doppelpunktliste,
Dateilisten nach dem ersten Satz, Zitate, Unicode, Reihenfolge und unveränderte Bytes.
Ein echter Mehrmodusvertrag mit Git, Fast Index, Safe Reader und libSQL prüft einen
Audit-Auftrag mit Kommaliste: zwei zusammenhängende Ask-Pflichten statt zusätzlich
isolierter Listenteile, vollständige Quellen, Abschluss und bytegleiche Originale.
Die bisherigen Grenzen-, Mixed-/Restore- und Mehrmodusregressionen bleiben bestehen.
Die identische Modellmatrix und Inhaltsprüfung werden separat nachgetestet.
