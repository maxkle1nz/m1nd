<p align="center">
  <img src=".github/m1nd-logo.svg" alt="m1nd" width="340" />
</p>

**m1nd** gibt deinem Coding-Agenten ein Gehirn pro Repository: ein lokales Code-Graph, das über MCP bereitgestellt wird, ein Gedächtnis, das an den Code verankert ist, den es zitiert, und ein Vertrauensurteil zu jeder Antwort. "Unzureichende Beweise" ist hier eine echte Antwort. Ebenso wie "vertrau dem noch nicht, und hier ist, wie man es repariert".

Nichts verlässt deine Maschine. Eine Rust-Binärdatei. MIT.

Denke an es wie an ein Röntgenbild deines Repos, das dein Agent lesen kann: eine Struktur, die alles kombiniert und aufzeigt, wo sich jedes Element befindet, wofür dieses Programm da ist, woran gearbeitet wird, was fertig ist und was noch offen steht. Dieses Panorama ist das, was kein anderes Tool deinem Agenten bietet.

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-mcp"><img src="https://img.shields.io/crates/v/m1nd-mcp.svg?label=crates.io" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://registry.modelcontextprotocol.io/?search=io.github.maxkle1nz/m1nd"><img src="https://img.shields.io/badge/MCP_Registry-official-6d28d9" alt="MCP Registry" /></a>
</p>

<p align="center">Vier Befehle zur Installation: <a href="#sixty-seconds">Sechzig Sekunden</a>. Gründe, zuerst den Tab zu schließen: <a href="#when-not-to-use-m1nd">Wann du m1nd nicht verwenden solltest</a>.</p>

<p align="center">
  <img src="docs/assets/demo.gif" width="760" alt="Eine echte m1nd-Sitzung: north liefert Vertrauen, Fokus und ehrliche Lücken; seek antwortet mit einem reverify-Urteil; memorize verankert die Erkenntnis im Code" />
</p>

<p align="center"><em>Eine echte Sitzung im 6.453-Knoten-Graph dieses Repos (m1nd-mcp 1.4.0): <code>north</code> orientiert, <code>seek</code> beantwortet mit einem <code>reverify</code>-Urteil, <code>memorize</code> verankert die Erkenntnis im Code.</em></p>

## Das Audit, das dein Agent nicht mehr bezahlt

Du kennst das Ritual. Der Agent öffnet eine Datei, führt grep aus, öffnet eine andere Datei, führt erneut grep aus, verbrennt den Großteil seines Kontexts, um überhaupt zu rekonstruieren, was das Repo eigentlich ist, und beginnt dann erst mit der eigentlichen Aufgabe. Mit **m1nd** wird dieser Prozess zu einer einzigen Frage. In weniger als einer Sekunde hat der Agent die Karte: Was ruft was auf, was bricht was, wo lebt alles. Kein Haufen von Ergebnissen, die interpretiert werden müssen. Die verbundene Struktur, bereits zusammengefügt.

Und er merkt sich das. Zwischen Sitzungen und zwischen Agenten. Was ein Agent heute Nacht lernt, erbt ein anderer morgen, mit den angehängten Beweisen und einer Flagge, falls sich der Code in der Zwischenzeit geändert hat. Jede Schlussfolgerung hinterlässt eine Spur, sodass du oder jeder andere nachfolgende Agent immer sehen kann, was mit diesem Code passiert ist und warum.

Dann geht **l1ght** noch weiter: wissenschaftliche Arbeiten, Artikel, RFCs, Entwürfe und Notizen werden mit den Teilen deines Codes verbunden, die sie erklären – innerhalb derselben Struktur. Der Agent erhält den richtigen Kontext, statt den ähnlich klingenden nächstgelegenen, und das Erfinden von nicht existierendem Code hört auf, der Weg des geringsten Widerstands zu sein: Die Struktur zeigt, was existiert, und das Urteil sagt, wie viel Vertrauen selbst darin stecken sollte.

Vor **m1nd** war eine Funktion nur eine Funktion, irgendwo in einem Handbuch verloren. Jetzt lebt sie innerhalb der Intelligenz des Agenten, kombiniert mit dem Code, seiner Geschichte, seinen Dokumenten und Risiken. So etwas habe ich nirgendwo anders gefunden.

## grep beantwortet gute Fragen. m1nd beantwortet die tiefergehenden.

Fragen, die dein Agent jetzt stellen und eine strukturelle Antwort darauf erhalten kann:

- Was geht kaputt, wenn ich diese Funktion anpasse?
- Wo genau findet in diesem Repo das Token-Refresh statt?
- Warum sind diese beiden Dateien verbunden, und ist dieser Pfad solide oder nur eine Vermutung?
- Was hat die letzte Sitzung über diesen Code gelernt, und ist das noch wahr?
- Was ändert sich hier immer zusammen, auch ohne sichtbaren Import zwischen ihnen?
- Überschreitet diese Änderung eine Architekturgrenze, die ich nicht überschreiten sollte?
- Welcher Anspruch in diesem Paper wird durch diese Funktion umgesetzt?
- Versteckt sich der Fehler, den ich gerade behoben habe, irgendwo anders in Form eines ähnlichen Musters?
- Was fehlt hier, was dieses Muster normalerweise hat?
- Bin ich überhaupt im richtigen Repo?
- Sollte ich auf diese Antwort reagieren oder sie zuerst verifizieren?

Jede dieser Fragen ist ein Verb auf der MCP-Oberfläche (`impact`, `seek`, `why`, `north`, `ghost_edges`, `xray_gate`, `antibody_scan`, `missing`, `trust_selftest`, `predict`), kein Trick mit Prompts.

## Und es hört nicht auf, nur Strukturen zu zeigen

Antikörper: Ein behobener Fehler wird zu einem benannten strukturellen Muster, und jede spätere Sitzung scannt das Repo nach diesem Muster. Einmal behoben, für immer gejagt.

Geisterkanten: Dateien, die immer zusammen geändert werden, ohne dass zwischen ihnen ein sichtbarer Import existiert, abgeleitet aus der Git-Historie. Die unsichtbare Kopplung, die Refactorings unterbricht.

Strukturelle Lücken: `missing` sucht nach dem Code, der nicht da ist. Die Absicherung, das Retry, der Timeout, den dieses Muster normalerweise hat, aber in diesem Fall fehlt.

Hypothesen gegen den Graphen: Stelle eine Behauptung in einfacher Sprache ("Einstellungen können den Boot erreichen, ohne validiert zu werden") und lasse sie gegen die Live-Struktur testen.

Tremor: Dateien, deren Änderungsrate sich beschleunigt, werden gekennzeichnet, bevor jemand einen Bug-Report einreicht.

Ein warmer Graph: Bestätigte Ergebnisse verstärken ihre Kanten, nach dem Hebb'schen Prinzip, sodass die Wege, die sich als nützlich erwiesen haben, für den nächsten Agenten höher ranken.

Jeder dieser Flags schlägt vor, aber dein Compiler und deine Tests liefern die Beweise.

## m1nd sucht nicht nur. Es schreibt.

Hier kommt der Teil, den die Leute erst glauben müssen. Der Graph, der dein Repo liest, kann auch darauf operieren. Dein Agent benennt ein Symbol und ein Ziel, etwa 48 Tokens, und `transplant` berechnet die gesamte Verschiebung aus dem Graphen: den erweiterten Bereich (doc-Kommentare und Attribute reisen mit), Abhängigkeiten, klassifiziert nach ihren Call-Kanten (private reisen mit, gemeinsam genutzte bleiben und erhalten einen Back-Import), jeder Verweis wird erneut qualifiziert, durch jede Datei, die ihn benennt. Dann schreibt es atomar, liest neu ein und gibt einen ehrlichen Beleg zurück: was verschoben wurde, was blieb, was nicht aufgelöst werden konnte. `refs_unresolved` ist nie stillschweigend leer, wenn etwas schiefgegangen ist.

Es ist zweiphasig, `transplant_preview` vor `transplant_commit`, und das Commit validiert den Hash jeder Datei, die es geplant hatte anzufassen, bevor es irgendetwas ändert. Die kritischen Bereiche deines Repos (Backend, Schema, Zahlungen, CI) sind serverseitig geschützt und versagen im geschlossenen Modus. Ein Ablehnungsvorgang berührt kein Byte und lehrt den Neustart: eine Kollision benennt den Besitzer, ein ungültiger Modulpfad benennt sich selbst, eine Verschiebung zwischen Krates benennt beide Krater-Wurzeln.

Gemessen am realen Fall: Die gesamte Dateibearbeitung kostete 12.235 Output-Tokens; die Transplantation kostete 48 und schrieb 3 Dateien in 1,3 Sekunden, wobei das Krate danach erfolgreich kompiliert wurde. rust-analyzer hat seit 2019 ein Problem im offenen Zustand, indem es verlangt, dass Verschiebungen über Dateien hinweg einfacher werden.

v1-Beschränkungen, klar formuliert: Nur Rust, nur obere `fn`, nur dasselbe Krate, die Zieldatei muss bereits existieren und Verweise, die innerhalb von Makros geboren wurden, sind unsichtbar. Jede dieser Grenzen ist bewusst gesetzt und dokumentiert in [docs/TRANSPLANT-PRD.md](docs/TRANSPLANT-PRD.md), direkt neben 13 Test-Dateien, die das Verb testen.

...
