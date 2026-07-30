🇬🇧 [English](../README.md) | 🇧🇷 [Português](README.pt-BR.md) | 🇪🇸 [Español](README.es.md) | 🇮🇹 [Italiano](README.it.md) | 🇫🇷 [Français](README.fr.md) | 🇩🇪 [Deutsch](README.de.md) | 🇨🇳 [中文](README.zh.md) | 🇯🇵 [日本語](README.ja.md)

<p align="center">
  <img src="../.github/m1nd-logo.svg" alt="m1nd" width="340" />
</p>

**m1nd** gibt deinem Coding Agent ein Brain pro Repository: ein lokaler Code-Graph, der über MCP bereitgestellt wird, eine an den zitierten Code verankerte Memory und ein Vertrauensurteil zu jeder Antwort. „Unzureichende Beweislage“ ist hier eine reale Antwort. Ebenso „Vertraue dem noch nicht, und so kannst du es reparieren“.

Nichts verlässt deine Maschine. Eine Rust-Binary. MIT.

Denke daran wie an ein Röntgenbild deines Repositories, das dein Agent lesen kann: eine Struktur, die alles kombiniert und sagt, wo sich jede Sache befindet, wofür dieses Programm gedacht ist, woran gearbeitet wird, was fertig ist und was noch offen ist. Diese Übersicht ist das, was kein anderes Tool deinem Agenten liefert.

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-mcp"><img src="https://img.shields.io/crates/v/m1nd-mcp.svg?label=crates.io" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://registry.modelcontextprotocol.io/?search=io.github.maxkle1nz/m1nd"><img src="https://img.shields.io/badge/MCP_Registry-official-6d28d9" alt="MCP Registry" /></a>
</p>

<p align="center">Vier Befehle zur Installation: <a href="#sixty-seconds">Sixty seconds</a>. Gründe, um zuerst den Tab zu schließen: <a href="#when-not-to-use-m1nd">When not to use m1nd</a>.</p>

<p align="center">
  <img src="../docs/assets/demo.gif" width="760" alt="Eine echte m1nd-Sitzung: north liefert Vertrauen, Fokus und ehrliche Lücken; seek antwortet mit einem reverify-Urteil; memorize verankert den Fund am Code" />
</p>

<p align="center"><em>Eine echte Sitzung auf dem 6.453-Knoten-Graph dieses Repos (m1nd-mcp 1.4.0): <code>north</code> orientiert, <code>seek</code> antwortet mit einem <code>reverify</code>-Urteil, <code>memorize</code> verankert den Fund am Code.</em></p>

## Das Audit, das dein Agent nicht mehr bezahlt

Du kennst das Ritual. Der Agent öffnet eine Datei, durchsucht sie, öffnet eine andere Datei, durchsucht sie wieder, verbrennt den Großteil seines Kontexts, um überhaupt zu rekonstruieren, was das Repository ist, bevor er mit der eigentlichen Aufgabe beginnt. Mit m1nd wird dieser Durchlauf zu einer einzigen Frage. In weniger als einer Sekunde hat der Agent die Übersicht: was ruft was auf, was bricht was, wo sich alles befindet. Kein Haufen von Trefferlisten, die interpretiert werden müssen. Die verbundene Struktur, bereits zusammengesetzt.

Und er erinnert sich. Zwischen Sitzungen und zwischen Agents. Was ein Agent heute Nacht lernt, übernimmt ein anderer morgen, mit angehängten Belegen und einer Warnung, falls der Code sich seitdem verändert hat. Jede Schlussfolgerung hinterlässt eine Spur, sodass du oder ein nachfolgender Agent immer sehen kannst, was mit diesem Code passiert ist und warum.

Dann geht l1ght noch weiter: Paper, Artikel, RFCs, Entwürfe und Notizen verbinden sich mit den Teilen deines Codes, die sie erklären, innerhalb derselben Struktur. Der Agent erhält den RICHTIGEN Kontext statt den nächstliegenden, und neuen Code zu erfinden, der nicht existiert, wird nicht länger zum Weg des geringsten Widerstands: Die Struktur sagt, was existiert, und das Urteil sagt, wie sehr man selbst das vertrauen sollte.

Vor m1nd war eine Funktion einfach nur eine Funktion, verloren in irgendeinem Handbuch. Jetzt lebt sie innerhalb der Intelligenz des Agents, kombiniert mit dem Code, seiner Geschichte, seinen Dokumenten und Risiken. Ich habe so etwas noch nirgendwo anders gefunden.

## grep beantwortet gute Fragen. m1nd beantwortet die tieferen.

Fragen, die dein Agent jetzt stellen kann und dafür eine strukturelle Antwort erhält:

- Was bricht, wenn ich diese Funktion anfasse?
- Wo passiert die Token-Aktualisierung tatsächlich in diesem Repo?
- Warum sind diese beiden Dateien verbunden, und ist diese Verbindung solide oder eine Vermutung?
- Was hat die letzte Sitzung über diesen Code gelernt, und stimmt das noch?
- Was ändert sich hier immer zusammen, selbst ohne direkten Import?
- Überschreitet diese Änderung eine Architekturgrenze, die ich nicht überschreiten sollte?
- Welche Behauptung in diesem Paper implementiert diese Funktion?
- Versteckt sich der gerade gefixte Bug irgendwo anders, als Muster?
- Was fehlt hier, das dieses Muster normalerweise enthält?
- Bin ich überhaupt im richtigen Repository?
- Soll ich auf diese Antwort reagieren oder sie zuerst überprüfen?

Jede dieser Fragen ist ein Verb auf der MCP-Oberfläche (`impact`, `seek`, `why`, `north`, `ghost_edges`, `xray_gate`, `antibody_scan`, `missing`, `trust_selftest`, `predict`), kein Trick mit Prompts.

## Und es zeigt nicht nur die Struktur

Antikörper: Ein gefixter Bug wird zu einem benannten strukturellen Muster, und jede spätere Sitzung sucht nach dieser Form im gesamten Repo. Einmal repariert, für immer aufgespürt.

Geisterkanten: Dateien, die immer zusammen geändert werden, ohne dass ein direkter Import dazwischenliegt, werden aus deiner Git-Historie extrahiert. Die unsichtbare Kopplung, die Refactorings behindert.

Strukturelle Lücken: `missing` sucht nach dem Code, der nicht da ist. Die Absicherung, der Retry, die Timeout-Logik, die dieses Muster normalerweise hat, hier aber fehlt.

Hypothesen gegen den Graph: Formuliere eine Behauptung in einfacher Sprache („Einstellungen können starten, ohne validiert zu werden“) und teste sie gegen die reale Struktur.

Tremor: Dateien, deren Änderungsintensität steigt, werden markiert, bevor jemand einen Bug-Report einreicht.

Ein „warmes“ Grafo: Bestätigte Ergebnisse verstärken ihre Kanten im Hebbian-Stil, sodass die Pfade, die sich als nützlich erwiesen haben, beim nächsten Agent höher bewertet werden.

Jedes dieser Features markiert und schlägt vor. Dein Compiler und deine Tests liefern weiterhin den Beweis.

## m1nd sucht nicht nur. Es schreibt.

Hier kommt der Teil, an dem viele erst mal zögern zu glauben, dass es möglich ist. Der Grafo, der dein Repository liest, kann auch auf ihm operieren. Dein Agent nennt ein Symbol und ein Ziel, etwa 48 Tokens, und `transplant` berechnet den gesamten Verschiebevorgang aus dem Grafo: die erweiterte Region (Doc-Kommentare und Attribute werden mitgenommen), nach Call-Kanten klassifizierte Abhängigkeiten (private werden verschoben, geteilte bleiben und erhalten einen Backimport), jeder Verweis wird über jede Datei hinweg neu qualifiziert, die ihn benennt. Anschließend wird atomar geschrieben, erneut implementiert und eine ehrliche Bestätigung wird zurückgegeben: was verschoben wurde, was geblieben ist, was nicht gelöst werden konnte. `refs_unresolved` ist nie still leer, wenn etwas schiefgegangen ist.

Es läuft in zwei Phasen, `transplant_preview` vor `transplant_commit`, und das Commit validiert den Hash jeder Datei, die geändert werden soll, nochmal neu. Nichts wird angewendet, wenn sich das Repository inzwischen verändert hat. Kritische Bereiche deines Repos (Backend, Schema, Payments, CI) sind serverseitig geschützt und schlagen bei Fehlern fehl, ohne etwas zu ändern.

[...Rest der README unverändert...]
