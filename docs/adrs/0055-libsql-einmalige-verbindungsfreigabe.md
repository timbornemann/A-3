# ADR-0055: Einmalige libSQL-Verbindungsfreigabe

Status: Accepted\
Datum: 2026-09-06\
Freigabe: Nutzerauftrag zur Recherche-Stabilisierung einschließlich zugehöriger ADRs.

## Befund

Die reale Windows-Matrix und Offline-Verträge stürzten mit `0xc0000005` ab.
Das zur unveränderten Testbinary passende PDB lokalisiert die Fehleradresse in
`sqlite3SafetyCheckSickOrOk`. Der Ausnahme-Stackscan enthält `sqlite3_close_v2`,
`Connection::disconnect`, `Connection::drop`, `LibsqlConnection::drop` und den
Research-Store-Eventpfad. Ein Stackscan allein ist kein vollständig abgewickelter Stack.

Der lokal vorhandene, durch Cargo.lock gepinnte libsql-0.9.29-Quelltext belegt
unabhängig davon die doppelte Freigabe: Der äußere `LibsqlConnection::drop`
ruft `conn.disconnect()` auf; danach läuft automatisch der innere
`Connection::drop`, der dieselbe Methode erneut aufruft. Bei alleinigem Besitzer
bleibt `Arc::get_mut(drop_ref)` in beiden Aufrufen erfolgreich. Der native Zeiger
wird beim ersten Schließen nicht zurückgesetzt. Dies ist ein Use-after-free-Risiko,
nicht ein Modell- oder Budgetproblem und nicht auf Tests begrenzt.

## Entscheidung

A^3 verwendet dieselbe Version mit denselben Features und transitiven Versionen
über einen nachvollziehbaren lokalen Cargo-Patch. Das bereits vorhandene
Registry-Archiv wird nach SHA-256-Abgleich gegen Cargo.lock nach `vendor/libsql-0.9.29`
übernommen. Die einzige funktionale Änderung entfernt den redundanten äußeren
Drop-Block. Der innere RAII-Besitzer bleibt allein zuständig. Keine neue unsafe-
Operation, FFI-Schnittstelle, Abhängigkeit, Netzwerkverbindung oder öffentliche API
wird hinzugefügt; unveränderter Drittanbieter-Quelltext bleibt außerhalb des
A^3-Workspace-Lintumfangs. Herkunft und Patchgrenze werden separat dokumentiert.

Ein Versionsupgrade ohne geprüften lokalen Kandidaten würde zusätzliche Änderungen
und neue Downloads verlangen. Cache-Manipulation wäre nicht reproduzierbar.
Leaks, künstlich offen gehaltene Statements, gemeinsam genutzte Transaktionsverbindungen
und weitere Crash-Retries verbergen oder verschieben den Fehler und werden verworfen.

## Verifikation und Pflege

Ein nativer Integrationstest prüft wiederholte Öffnung, Transaktionen, Connection-
Klone und lebende Rows nach Freigabe der öffentlichen Connection, ausdrücklich ohne
Crash-Retry. Wegen allocatorabhängiger Sichtbarkeit des Use-after-free ist ein
bestandener Test vor dem Patch kein Gegenbeweis zum Quelltextbefund. Die entfernte
zweite Drop-Stelle und die exakte Upstream-Differenz werden zusätzlich geprüft.
Alle gemeinsamen Storage-Verträge, Workspace-Gates und Research-Modelltests bleiben
verbindlich. Ein späteres Upstream-Upgrade muss den lokalen Patch ausdrücklich
ablösen; die native Lebensdauerregression bleibt erhalten. Kein privater
Datenbestand wird zum Test geöffnet oder migriert.
