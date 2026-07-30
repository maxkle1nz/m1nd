```markdown
<p align="center">
  <img src=".github/m1nd-logo.svg" alt="m1nd" width="340" />
</p>

**m1nd** gibt deinem Coding-Agenten ein Gehirn pro Repository: ein lokaler Code-Graph über MCP bereitgestellt, eine an den zitierten Code verankerte Speicherfunktion und ein Vertrauensurteil zu jeder Antwort. "Unzureichende Beweise" ist hier eine echte Antwort. Ebenso wie "Vertraue dem noch nicht, und so reparierst du es".

Nichts verlässt deine Maschine. Eine Rust-Binärdatei. MIT.

Stell dir das als ein Röntgenbild deines Repos vor, das dein Agent lesen kann: eine Struktur, die alles kombiniert und sagt, wo sich alles befindet, wofür dieses Programm gedacht ist, woran gearbeitet wird, was fertig ist und was noch offen bleibt. Dieses Panorama ist etwas, das kein anderes Tool deinem Agenten bietet.

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-mcp"><img src="https://img.shields.io/crates/v/m1nd-mcp.svg?label=crates.io" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://registry.modelcontextprotocol.io/?search=io.github.maxkle1nz/m1nd"><img src="https://img.shields.io/badge/MCP_Registry-official-6d28d9" alt="MCP Registry" /></a>
</p>

<p align="center">Vier Befehle zur Installation: <a href="#sixty-seconds">Sechzig Sekunden</a>. Gründe, den Tab zuerst zu schließen: <a href="#when-not-to-use-m1nd">Wann m1nd nicht zu verwenden ist</a>.</p>

<p align="center">
  <img src="docs/assets/demo.gif" width="760" alt="Eine echte m1nd-Sitzung: north liefert Vertrauen, Fokus und ehrliche Lücken; seek beantwortet mit einem Reverify-Urteil; memorize verankert den Fund im Code" />
</p>

<p align="center"><em>Eine echte Sitzung im 6.453-Knoten-Graph dieses Repos (m1nd-mcp 1.4.0): <code>north</code> orientiert, <code>seek</code> beantwortet mit dem Urteil <code>reverify</code>, <code>memorize</code> verankert den Fund im Code.</em></p>

## Das Audit, für das dein Agent nicht mehr bezahlt

Du kennst das Ritual. Der Agent öffnet eine Datei, durchforstet sie, öffnet eine weitere Datei, durchsucht sie erneut und verbraucht den Großteil seines Kontexts damit, überhaupt zu rekonstruieren, was das Repo ist, bevor die eigentliche Aufgabe beginnt. Mit m1nd wird dieser Durchgang zu einer einzigen Frage. Innerhalb weniger Sekunden hat der Agent die Karte: was ruft was auf, was bricht was, wo liegt alles. Kein Haufen zu interpretierender Treffer. Die verbundene Struktur, bereits zusammengebaut.

Und er erinnert sich. Zwischen Sitzungen und zwischen Agenten. Was ein Agent heute Nacht lernt, erbt ein anderer Agent morgen, mit angehängten Beweisen und einer Markierung, falls sich der Code seitdem geändert hat. Jede Schlussfolgerung hinterlässt eine Spur, sodass du oder jeder Agent, der später kommt, immer sehen können, was mit diesem Code geschehen ist und warum.

Dann geht l1ght noch einen Schritt weiter: Aufsätze, Artikel, RFCs, Entwürfe und Notizen werden mit den Teilen deines Codes verbunden, die sie erklären, innerhalb derselben Struktur. Der Agent erhält den RICHTIGEN Kontext anstelle von dem am ähnlichsten klingenden. Das Erfinden von Code, der nicht existiert, hört auf, der Weg des geringsten Widerstands zu sein: Die Struktur sagt, was existiert, und das Urteil gibt an, wie viel Vertrauen sogar darin gesetzt werden kann.

Vor m1nd war eine Funktion nur eine Funktion, verloren in irgendeinem Handbuch. Jetzt lebt sie im Wissen des Agenten, kombiniert mit dem Code, seiner Geschichte, seinen Dokumenten und seinen Risiken. Etwas Vergleichbares habe ich nirgendwo anders gefunden.

## grep beantwortet gute Fragen. m1nd beantwortet die tieferen.

Fragen, die dein Agent jetzt stellen kann und für die er eine strukturelle Antwort erhält:

- Was geht kaputt, wenn ich diese Funktion anfasse?
- Wo passiert tatsächlich die Token-Aktualisierung in diesem Repo?
- Warum sind diese beiden Dateien verbunden, und ist dieser Pfad solide oder eine Vermutung?
- Was hat die letzte Sitzung über diesen Code gelernt, und stimmt das noch?
- Was ändert sich hier immer zusammen, selbst ohne Import zwischen ihnen?
- Überschreitet diese Bearbeitung eine Architekturgrenze, die ich nicht überschreiten sollte?
- Welche Behauptung in diesem Papier wird durch diese Funktion implementiert?
- Versteckt sich der Bug, den ich gerade behoben habe, noch an anderer Stelle in derselben Form?
- Was fehlt hier, was dieses Muster normalerweise hat?
- Bin ich überhaupt im richtigen Repo?
- Soll ich auf diese Antwort reagieren oder sie erst verifizieren?

Jede dieser Fragen entspricht einem Verb auf der MCP-Oberfläche (`impact`, `seek`, `why`, `north`, `ghost_edges`, `xray_gate`, `antibody_scan`, `missing`, `trust_selftest`, `predict`), nicht einem Aufforderungstrick.
```
