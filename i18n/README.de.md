```markdown
<p align="center">
  <img src=".github/m1nd-logo.svg" alt="m1nd" width="340" />
</p>

**m1nd** gibt deinem Coding-Agenten ein Gehirn pro Repository: einen lokalen Codegraphen, der über MCP bereitgestellt wird, ein Gedächtnis, das an den zitierten Code gebunden ist, und ein Vertrauensurteil zu jeder Antwort. "Ungenügende Beweise" ist hier eine echte Antwort. Ebenso wie "Vertraue dem noch nicht, und hier ist, wie du es reparieren kannst."

Nichts verlässt deine Maschine. Eine Rust-Binärdatei. MIT.

Denke daran als ein Röntgenbild deines Repositories, das dein Agent lesen kann: eine Struktur, die alles kombiniert und angibt, wo jedes Teil sich befindet, wofür das Programm gedacht ist, woran gearbeitet wird, was abgeschlossen ist und was noch offen ist. Dieses Panorama ist das, was kein anderes Tool deinem Agenten liefert.

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-mcp"><img src="https://img.shields.io/crates/v/m1nd-mcp.svg?label=crates.io" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://registry.modelcontextprotocol.io/?search=io.github.maxkle1nz/m1nd"><img src="https://img.shields.io/badge/MCP_Registry-official-6d28d9" alt="MCP Registry" /></a>
</p>

<p align="center">Vier Befehle zur Installation: <a href="#sixty-seconds">Sechzig Sekunden</a>. Gründe, warum du den Tab zuerst schließen solltest: <a href="#when-not-to-use-m1nd">Wann du m1nd nicht verwenden solltest</a>.</p>

<p align="center">
  <img src="docs/assets/demo.gif" width="760" alt="Eine echte m1nd-Sitzung: north liefert Vertrauen, Fokus und ehrliche Lücken; seek beantwortet mit einem reverify-Urteil; memorize verankert die Ergebnisse im Code" />
</p>

<p align="center"><em>Eine echte Sitzung auf dem 6.453-Knoten-Graphen dieses Repositories (m1nd-mcp 1.4.0): <code>north</code> orientiert, <code>seek</code> beantwortet mit einem <code>reverify</code>-Urteil, <code>memorize</code> verankert Ergebnisse im Code.</em></p>

## Das Audit, für das dein Agent aufhört zu zahlen

Du kennst das Ritual. Der Agent öffnet eine Datei, führt `grep` aus, öffnet eine andere Datei, `grep` nochmal, verbraucht den größten Teil seines Kontextes, um überhaupt zu rekonstruieren, was das Repository ist, und beginnt erst dann mit der eigentlichen Aufgabe. Mit m1nd wird diese Erkundung zu einer einzigen Frage. Innerhalb von weniger als einer Sekunde hat der Agent die Übersicht: was ruft was auf, was bricht was, wo alles zu finden ist. Kein Haufen von Übereinstimmungen, die interpretiert werden müssen. Eine verbundene Struktur, die bereits zusammengestellt wurde.

Und es merkt sich alles. Zwischen Sitzungen und zwischen Agenten. Was ein Agent heute Abend lernt, übernimmt morgen ein anderer Agent, zusammen mit angehängten Belegen und einer Markierung, wenn sich der Code seitdem verändert hat. Jede Schlussfolgerung hinterlässt eine Spur, so dass du oder jeder spätere Agent immer sehen kann, was mit diesem Code passiert ist und warum.

Dann geht l1ght noch weiter: wissenschaftliche Arbeiten, Artikel, RFCs, Entwürfe und Notizen sind mit den Teilen deines Codes verbunden, die sie erklären, innerhalb derselben Struktur. Der Agent erhält den RICHTIGEN Kontext anstelle eines nächstgelegenen. Das Erfinden von nicht existierendem Code hört auf, der Weg des geringsten Widerstandes zu sein: Die Struktur zeigt, was existiert, und das Urteil sagt aus, wie viel Vertrauen selbst dem zu schenken ist.

Vor m1nd war eine Funktion nur eine Funktion, verloren in einem Handbuch. Jetzt lebt sie in der Intelligenz des Agenten, kombiniert mit dem Code, seiner Geschichte, seinen Dokumenten und seinen Risiken. Ich habe so etwas noch nirgends sonst gefunden.

## grep beantwortet gute Fragen. m1nd beantwortet tiefere.

Fragen, die dein Agent jetzt stellen kann und auf die er eine strukturelle Antwort erhält:

- Was bricht, wenn ich diese Funktion ändere?
- Wo findet tatsächlich eine Token-Aktualisierung in diesem Repository statt?
- Warum sind diese beiden Dateien miteinander verbunden und ist dieser Pfad solide oder nur eine Vermutung?
- Was hat die letzte Sitzung über diesen Code gelernt und ist es immer noch wahr?
- Was ändert sich hier grundsätzlich immer zusammen, selbst ohne Import zwischen ihnen?
- Überschreitet diese Änderung eine Architektursgrenze, die ich nicht überschreiten sollte?
- Welche Behauptung in diesem Papier wird von dieser Funktion implementiert?
- Versteckt sich der soeben behobene Fehler irgendwo anders, in ähnlicher Form?
- Was fehlt hier, was dieses Muster normalerweise hat?
- Bin ich überhaupt im richtigen Repository?
- Sollte ich auf diese Antwort reagieren oder sie zuerst verifizieren?

Jede dieser Fragen ist ein Verb auf der MCP-Oberfläche (`impact`, `seek`, `why`, `north`, `ghost_edges`, `xray_gate`, `antibody_scan`, `missing`, `trust_selftest`, `predict`), kein Trick mit Eingabeaufforderungen.

## Und es bleibt nicht dabei, nur die Struktur zu zeigen

Antikörper: Ein behobener Fehler wird ein benanntes strukturelles Muster, und jede spätere Sitzung scannt nach dieser Form im gesamten Repository. Einmal behoben, für immer danach suchen.

Unsichtbare Verknüpfungen: Dateien, die immer zusammen geändert werden, auch wenn zwischen ihnen kein Import existiert, werden aus deiner Git-Historie extrahiert. Die unsichtbare Kopplung, die Refaktorisierungen bricht.

Strukturelle Lücken: `missing` sucht nach dem Code, der nicht vorhanden ist. Der Wächter, das Wiederholungsmuster, das Timeout, das dieses Muster normalerweise mit sich bringt und dieser Instanz fehlt.

Hypothesen gegen den Graphen: Formuliere eine Behauptung in einfacher Sprache ("Einstellungen können ohne Validierung den Bootvorgang erreichen") und lass sie gegen die Live-Struktur überprüfen.

Unruhe: Dateien, deren Änderungsrate sich beschleunigt, werden markiert, bevor jemand den Fehlerbericht einreicht.

Ein "warmer" Graph: Bestätigte Ergebnisse verstärken ihre Verknüpfungen, Hebb'scher Stil, so dass die nützlichen Pfade beim nächsten Agenten höher bewertet werden.

Jede dieser Funktionen macht Vorschläge und gibt Hinweise. Dein Compiler und deine Tests übernehmen weiterhin die Beweisführung.

## m1nd sucht nicht nur. Es schreibt.

Hier ist der Teil, den man mehrfach lesen muss, um ihn zu glauben. Der Graph, der dein Repository liest, kann auch auf es zugreifen. Dein Agent nennt ein Symbol und ein Ziel, etwa 48 Tokens, und `transplant` berechnet die gesamte Verschiebung aus dem Graphen: die erweiterte Region (Kommentartexte und Attribute werden mitgenommen), Abhängigkeiten, die durch ihre Aufrufkanten klassifiziert werden (private wandern mit, geteilte bleiben und erhalten einen Rückimport), und jede Referenz wird in allen Dateien, die sie nennen, aktualisiert. Dann schreibt es atomar, liest neu ein und gibt eine ehrliche Quittung zurück: was verschoben wurde, was geblieben ist, was es nicht lösen konnte. `refs_unresolved` ist niemals still leer, wenn etwas schiefgelaufen ist.

Es gibt zwei Phasen: `transplant_preview` vor `transplant_commit`, und das Commit überprüft den Hash jeder Datei, die es geplant hatte zu bearbeiten, erneut, so dass nichts in ein geändertes Repository geschrieben wird. Der Wert deiner Anwendung (Backend, Schema, Zahlungen, CI) ist serverseitig geschützt und scheitert sicher. Eine Verweigerung berührt kein Byte und lehrt den erneuten Versuch: Bei einer Kollision wird der aktuelle Besitzer benannt, ein ungültiger Modulpfad benennt sich selbst, ein crate-übergreifendes Verschieben benennt beide crate-Wurzeln.

Messung am realen Fall: Die komplette Dateiänderung benötigte 12.235 Ausgabe-Tokens, die Transplantation benötigte 48 Eingaben, schrieb 3 Dateien in 1,3 Sekunden und das crate wurde danach fehlerfrei kompiliert. Der `rust-analyzer` hat seit 2019 ein Problem mit Dateiübergreifenden Verschiebungen.

v1-Beschränkungen, klar formuliert: Nur Rust, nur `fn` auf oberster Ebene, nur innerhalb eines crates, die Zieldatei muss bereits existieren, und Verweise, die in Makros entstehen, sind für es unsichtbar. Jede Einschränkung ist absichtlich und dokumentiert in [docs/TRANSPLANT-PRD.md](docs/TRANSPLANT-PRD.md), neben 13 Testdateien, die die Verben detailliert abdecken.
[Truncated]
```
