```markdown
<p align="center">
  <img src=".github/m1nd-logo.svg" alt="m1nd" width="340" />
</p>

**m1nd** gibt deinem Coding-Agenten ein Gehirn pro Repository: ein lokaler Codegraph, der über MCP bereitgestellt wird, ein Gedächtnis, das an den Code verankert ist, den es zitiert, und ein Vertrauensurteil zu jeder Antwort. "Ungenügende Beweise" ist hier eine echte Antwort. Ebenso wie "Vertraue dem noch nicht, und hier ist, wie du es reparieren kannst".

Nichts verlässt deine Maschine. Eine Rust-Binärdatei. MIT.

Denke daran wie an ein Röntgenbild deines Repos, das dein Agent lesen kann: Eine Struktur, die alles kombiniert und zeigt, wo sich was befindet, wofür das Programm gedacht ist, woran gearbeitet wird, was erledigt ist und was noch offen bleibt. Dieses Panorama ist das, was dir kein anderes Tool für deinen Agenten bietet.

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-mcp"><img src="https://img.shields.io/crates/v/m1nd-mcp.svg?label=crates.io" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://registry.modelcontextprotocol.io/?search=io.github.maxkle1nz/m1nd"><img src="https://img.shields.io/badge/MCP_Registry-official-6d28d9" alt="MCP Registry" /></a>
</p>

<p align="center">Vier Befehle zur Installation: <a href="#sixty-seconds">Sechzig Sekunden</a>. Gründe, den Tab zuerst zu schließen: <a href="#when-not-to-use-m1nd">Wann m1nd nicht verwendet werden sollte</a>.</p>

<p align="center">
  <img src="docs/assets/demo.gif" width="760" alt="Eine echte m1nd-Sitzung: north liefert Vertrauen, Fokus und ehrliche Lücken; seek antwortet mit einem reverify-Urteil; memorize verankert die Erkenntnis im Code" />
</p>

<p align="center"><em>Eine echte Sitzung auf dem 6.453-Knoten-Graph dieses Repos (m1nd-mcp 1.4.0): <code>north</code> orientiert, <code>seek</code> antwortet mit einem <code>reverify</code>-Urteil, <code>memorize</code> verankert die Erkenntnis im Code.</em></p>

## Das Audit, das dein Agent aufhört zu bezahlen

Du kennst das Ritual. Der Agent öffnet eine Datei, greift darauf zu, öffnet eine andere Datei, greift wieder darauf zu, verbrennt den Großteil seines Kontexts, um überhaupt zu rekonstruieren, was das Repo ist, und beginnt erst dann mit der eigentlichen Aufgabe. Mit m1nd wird dieser Durchlauf zu einer Frage. In weniger als einer Sekunde hat der Agent die Karte: Was ruft was auf, was bricht was, wo lebt alles. Kein Stapel von Übereinstimmungen zum Interpretieren. Die verknüpfte Struktur, bereits zusammengesetzt.

Und er erinnert sich. Zwischen Sitzungen, und zwischen Agenten. Was ein Agent heute Nacht lernt, erbt ein anderer Agent morgen, mit angehängten Beweisen und einer Flagge, wenn sich der Code seitdem verändert hat. Jeder Schluss hinterlässt eine Spur, sodass du, oder jeder nachfolgende Agent, immer sehen kannst, was mit diesem Code passiert ist und warum.

Dann geht l1ght einen Schritt weiter: Papiere, Artikel, RFCs, Entwürfe und Notizen verbinden sich mit den Teilen deines Codes, die sie erklären, innerhalb derselben Struktur. Der Agent erhält den RICHTIGEN Kontext statt dem nächstliegenden, und das Erfinden von Code, der nicht existiert, hört auf, der Weg des geringsten Widerstands zu sein: Die Struktur sagt, was existiert, und das Urteil sagt, wie sehr man sogar dem vertrauen sollte.

Vor m1nd war eine Funktion einfach nur eine Funktion, verloren in irgendeinem Handbuch. Jetzt lebt sie im Verstand des Agenten, kombiniert mit dem Code, seiner Geschichte, seinen Dokumenten und seinen Risiken. Ich habe nichts Vergleichbares irgendwo anders gefunden.

## grep beantwortet gute Fragen. m1nd beantwortet die tiefergehenden.

Fragen, die dein Agent jetzt stellen und für die er eine strukturelle Antwort erhalten kann:

- Was bricht, wenn ich diese Funktion anpasse?
- Wo findet tatsächlich die Token-Auffrischung in diesem Repo statt?
- Warum sind diese beiden Dateien verbunden, und ist dieser Weg solide oder nur eine Vermutung?
- Was hat die letzte Sitzung über diesen Code gelernt, und ist das noch wahr?
- Was ändert sich hier immer zusammen, auch ohne Import zwischen ihnen?
- Überschreitet diese Änderung eine Architekturgrenze, die ich nicht überschreiten sollte?
- Welche Behauptung aus diesem Papier implementiert diese Funktion?
- Versteckt sich der Bug, den ich gerade behoben habe, irgendwo anders, in Form eines Musters?
- Was fehlt hier, was dieses Muster normalerweise hat?
- Bin ich überhaupt im richtigen Repo?
- Sollte ich auf diese Antwort reagieren oder sie zuerst überprüfen?

Jede davon ist ein Verb auf der MCP-Oberfläche (`impact`, `seek`, `why`, `north`, `ghost_edges`, `xray_gate`, `antibody_scan`, `missing`, `trust_selftest`, `predict`) und kein Prompt-Trick.

## Und es hört nicht auf, nur Strukturen zu zeigen

Antikörper: Ein behobener Bug wird zu einem benannten strukturellen Muster, und jede spätere Sitzung scannt nach dieser Form im gesamten Repo. Einmal beheben, für immer jagen.

Ghost Edges: Dateien, die immer zusammen ohne Import zwischen ihnen geändert werden, aus deiner Git-Historie extrahiert. Die unsichtbare Kopplung, die Refactorings bricht.

Strukturelle Lücken: `missing` sucht nach dem Code, der nicht da ist. Die Absicherung, der erneute Versuch, das Timeout, das dieses Muster normalerweise trägt und das in diesem Fall fehlt.

Hypothesen gegen den Graph: Stelle eine Behauptung in einfacher Sprache ("Einstellungen können ohne Validierung zum Boot gelangen") und lasse sie gegen die Live-Struktur testen.

Tremor: Dateien, deren Änderungsrate beschleunigt, werden markiert, bevor jemand den Bug-Report einreicht.

Ein warmer Graph: Bestätigte Ergebnisse verstärken ihre Kanten nach Hebb'schem Stil, sodass die als nützlich erwiesenen Pfade für den nächsten Agenten höher priorisiert werden.

All dies markiert und schlägt vor; dein Compiler und deine Tests übernehmen weiterhin die Beweisführung.

## m1nd durchsucht nicht nur. Es schreibt.

Hier ist der Teil, den die Leute erst beim zweiten Mal glauben. Der Graph, der dein Repo liest, kann auch darauf zugreifen. Dein Agent benennt ein Symbol und ein Ziel, etwa 48 Tokens, und `transplant` berechnet den gesamten Move aus dem Graph: die erweiterte Region (Doku-Kommentare und Attribute reisen mit), Abhängigkeiten klassifiziert nach ihren Kanten (Private reisen mit, Gemeinsame bleiben und erhalten einen Rück-Import), jeder Benutzer wird über jede Datei hinweg neu qualifiziert, die ihn nennt. Dann schreibt es atomar, nimmt es neu auf und gibt eine ehrliche Quittung zurück: Was bewegt wurde, was geblieben ist und was nicht aufgelöst werden konnte. `refs_unresolved` ist nie stillschweigend leer, wenn etwas schiefging.

Es ist zweiphasig: `transplant_preview` vor `transplant_commit`, und das Commit überprüft den Hash jeder Datei, die es berühren wollte, sodass nichts in ein Repo geschrieben wird, das sich zwischenzeitlich geändert hat. Das Herzstück deines Repos (Backend, Schema, Zahlungen, CI) wird serverseitig geschützt und schlägt fehl. Eine Ablehnung berührt kein Byte und lehrt dich den erneuten Versuch: Eine Kollision benennt den Bewohner, ein ungültiger Modulpfad benennt sich selbst, ein crate-übergreifender Move benennt beide crate-Wurzeln.

Gemessen am realen Fall: Die Bearbeitung der gesamten Datei kostete 12.235 Output-Tokens; der transplant-Move kostete 48 Eingabe und schrieb 3 Dateien in 1,3 Sekunden, wobei das crate auf der anderen Seite kompiliert wurde. rust-analyzer hat seit 2019 ein offenes Issue zu Cross-File-Moves.

v1-Grenzen, klar ausgesprochen: Nur Rust, nur top-level `fn`, gleiches crate, die Ziel-Datei muss bereits existieren, und Referenzen, die in Makros geboren sind, bleiben unsichtbar. Jede Grenze ist absichtlich und in [docs/TRANSPLANT-PRD.md](docs/TRANSPLANT-PRD.md) dokumentiert, neben 13 Testdateien, die das Verb daran binden.
```
