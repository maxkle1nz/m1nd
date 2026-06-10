🇬🇧 [English](../README.md) | 🇧🇷 [Português](README.pt-BR.md) | 🇪🇸 [Español](README.es.md) | 🇮🇹 [Italiano](README.it.md) | 🇫🇷 [Français](README.fr.md) | 🇩🇪 [Deutsch](README.de.md) | 🇨🇳 [中文](README.zh.md) | 🇯🇵 [日本語](README.ja.md)

<p align="center">
  <img src="../.github/m1nd-logo.svg" alt="m1nd" width="400" />
</p>

<h1 align="center">Eine Lokale Mission Runtime für Coding-Agenten</h1>

<p align="center">
  <strong>Dein Coding-Agent hört auf, blind zu starten.</strong><br/>
  <em>Local-first. MCP-native. Graph-Gedächtnis, Trust und Change-Reasoning für Agent-Hosts.</em>
</p>

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-core"><img src="https://img.shields.io/crates/v/m1nd-core.svg" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="../LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="Lizenz" /></a>
  <a href="https://docs.rs/m1nd-core"><img src="https://img.shields.io/docsrs/m1nd-core" alt="docs.rs" /></a>
</p>

<p align="center">
  <a href="https://github.com/openai/codex"><img src="https://img.shields.io/badge/OpenAI_Codex-412991?logo=openai&logoColor=fff" alt="OpenAI Codex" /></a>
  <a href="https://claude.ai/download"><img src="https://img.shields.io/badge/Claude_Code-f0ebe3?logo=claude&logoColor=d97706" alt="Claude Code" /></a>
  <a href="https://cursor.sh"><img src="https://img.shields.io/badge/Cursor-000?logo=cursor&logoColor=fff" alt="Cursor" /></a>
  <a href="https://codeium.com/windsurf"><img src="https://img.shields.io/badge/Windsurf-0d1117?logo=windsurf&logoColor=3ec9a7" alt="Windsurf" /></a>
  <a href="https://github.com/features/copilot"><img src="https://img.shields.io/badge/GitHub_Copilot-000?logo=githubcopilot&logoColor=fff" alt="GitHub Copilot" /></a>
  <a href="https://zed.dev"><img src="https://img.shields.io/badge/Zed-084ccf?logo=zedindustries&logoColor=fff" alt="Zed" /></a>
  <a href="https://github.com/cline/cline"><img src="https://img.shields.io/badge/Cline-000?logo=cline&logoColor=fff" alt="Cline" /></a>
  <a href="https://roocode.com"><img src="https://img.shields.io/badge/Roo_Code-6d28d9?logoColor=fff" alt="Roo Code" /></a>
  <a href="https://github.com/continuedev/continue"><img src="https://img.shields.io/badge/Continue-000?logoColor=fff" alt="Continue" /></a>
  <a href="https://opencode.ai"><img src="https://img.shields.io/badge/OpenCode-18181b?logoColor=fff" alt="OpenCode" /></a>
  <a href="https://aistudio.google.com"><img src="https://img.shields.io/badge/Gemini-4285F4?logo=google&logoColor=fff" alt="Gemini" /></a>
  <a href="https://aws.amazon.com/q/developer"><img src="https://img.shields.io/badge/Amazon_Q-232f3e?logo=amazonaws&logoColor=f90" alt="Amazon Q" /></a>
</p>

---

**m1nd ist eine lokale Mission Runtime für Coding-Agenten — sie steuert die Betriebsschleife, nicht nur den Retrieval.**

> grep findet Text. Vektorsuche findet ähnliche Chunks. `m1nd` gibt Agenten einen lokalen Graphen davon, was verbunden ist, was sich geändert hat, was bricht, was gedriftet ist, und wo weiterzumachen ist.

Drei Dinge koexistieren hier, die kein anderes Tool vereint:

- **Kausaler Code-Graph** — `impact` vor einer Bearbeitung zeigt den Blast Radius, den du nicht gelesen hast; `ghost_edges` bringt Dateien ans Licht, die sich immer zusammen ändern, aber keinen Import teilen.
- **Selbstverifizierendes Gedächtnis** — `memorize` verankert Erkenntnisse an echten Code-Knoten; `cross_verify` markiert sie als veraltet, wenn dieser Code sich ändert.
- **Ein Trust- / Recovery-Layer** — jedes Ergebnis trägt einen Trust-Mode; `trust_selftest` und `recovery_playbook` teilen dem Agenten mit, wann das Workspace-Binding falsch ist und wie er es repariert.

<p align="center">
  <img src="../.github/m1nd-agent-first-map-v2.jpeg" alt="Traditionelle Agentenschleife vs. m1nd-grounded-Schleife" width="960" />
</p>

## Schnellstart

Der minimale Happy-Path — aus den Quellen installieren (immer aktuell), Gesundheit prüfen, Host verbinden:

```bash
git clone https://github.com/maxkle1nz/m1nd.git && cd m1nd
npm install -g .
m1nd doctor
m1nd install-skills codex          # oder: claude / gemini / antigravity / generic
m1nd mcp-config codex --project /your/project
```

Oder aus dem npm-Beta-Kanal: `npm install -g @maxkle1nz/m1nd@beta`.

Vollständige Installationskarte, Host-Packs, nativer Runtime-Build und Update-Flags: [docs/AGENT-PACKS.md](../docs/AGENT-PACKS.md) · Client-für-Client-Einrichtung: [Integrationsmatrix](../docs/IDE-INTEGRATIONS.md).

### Agent-Einstiegspunkt

Agenten parsen dieses README. Wenn die Host-MCP-Session veraltet ist, an das falsche Repository gebunden ist oder noch nicht geladen wurde, verwende die host-neutrale CLI — sie startet eine isolierte Runtime, bindet sie an das Repository und gibt einen einzigen maschinenlesbaren Envelope zurück:

```bash
m1nd agent first-minute --repo /your/project --query "understand this system" --json
```

`m1nd agent first-minute` ist der sicherste erste Kontakt für ein neues Repository. Es scoped das Repository, stellt Trust her, ingested bei Bedarf, führt einen begrenzten Orientierungsdurchgang durch, gibt Kandidaten-Anker zurück, und sagt dem Agenten dann, direkt aus Quellen, Tests, Compiler-/Runtime-Ausgabe, Logs oder Proben zu beweisen.

Innerhalb einer MCP-Session ist die Doktrin diese Trust-Schleife — Trust *vor* dem Vertrauen in irgendwelchen Retrieval herstellen:

```jsonc
// 0. Binding in einem Aufruf prüfen (Urteil vor dem Retrieval)
{"method":"tools/call","params":{"name":"trust_selftest","arguments":{"agent_id":"dev"}}}

// 1. Wenn das Urteil nicht full_trust ist, den deterministischen Recovery-Pfad anfordern
{"method":"tools/call","params":{"name":"recovery_playbook","arguments":{"agent_id":"dev"}}}

// 2. Graph-Wahrheit aufbauen
{"method":"tools/call","params":{"name":"ingest","arguments":{"path":"/your/project","agent_id":"dev"}}}

// 3. Eine strukturelle Frage stellen — leere Ergebnisse sagen *warum*, niemals nur "keine Ergebnisse"
{"method":"tools/call","params":{"name":"activate","arguments":{"query":"authentication flow","agent_id":"dev"}}}
```

**Erste-Session-Schleife, in vier Zügen:** `trust_selftest` → `ingest` → `seek`/`audit` → `memorize` das dauerhafte Ergebnis, damit die nächste Session voraus startet.

## Was m1nd Nicht Ist

`m1nd` ist nicht nur:

- ein Code-Suchtool mit einem größeren Index
- ein Repo-RAG-Layer, der nur Dateien oder Chunks abruft
- eine Graphdatenbank, die Workflow-Entscheidungen dem Client überlässt
- ein Ersatz für statische Analyse durch Compiler, Tests oder Sicherheits-Tools
- ein MCP-Bundle unzusammenhängender Hilfsmittel

Es ist der Layer, der diese Oberflächen in ein operatives System verwandelt, über das ein Agent nachdenken und durch das er handeln kann. Nicht für Einzeldatei-Lookups, einfaches grep oder Compiler-Wahrheit — verwende dort einfache Tools.

## Warum Agenten es Brauchen

Ohne m1nd beginnt jede Session mit grep-Schleifen und manuelle Reorientierung; die Erkenntnisse der letzten Woche sind weg, und ein leeres Suchergebnis ist nicht von einem falschen Workspace-Binding zu unterscheiden. Mit m1nd beginnt die Session mit einem Trust-Urteil, vergangene Erkenntnisse laden automatisch bereits verankert an dem Code, der sie unterstützt, und leere Ergebnisse sagen *warum*.

Agenten auf echten Codebasen scheitern nicht, weil sie nicht suchen können. Sie scheitern, weil sie kein operatives Modell haben. Sie bauen Kontext jede Session von Grund auf neu auf, editieren ohne den Blast Radius zu kennen, und können ein leeres Ergebnis, das „nichts existiert" bedeutet, nicht von einem unterscheiden, das „falsches Repo" bedeutet.

Das funktioniert für kleine Codebasen. Es bricht zusammen, wenn das Projekt generierte Artefakte, Specs, Docs, versteckte Co-Change-Historie, mehrere Agenten und lange Handoffs hat. Das Problem ist nicht nur das Reasoning des Agenten — der Agent hat kein dauerhaftes Modell der Codebasis-Struktur. `m1nd` gibt ihm eines: einen kausalen Code-Graphen mit Spreading Activation über strukturelle, semantische, zeitliche und kausale Dimensionen, plus Hebbianische Plastizität, die pro Agent über Sessions hinweg aufbaut.

## Zusammengesetztes Gedächtnis (L1GHT)

Die meisten Tools geben einem Agenten besseres *Retrieval*. `m1nd` erlaubt einem Agenten auch, **dauerhafte, maschinenlesbare Erkenntnisse zu verfassen**, die sich über Sessions hinweg aufbauen und gegenüber dem Code ehrlich bleiben. L1GHT verwandelt verfasstes Wissen in graph-native Struktur, die sich selbst markiert, wenn sich der Code ändert, den es zitiert — sichere Behauptungen verbreiten mehr Aktivierung als unsichere.

Die Schleife, von Anfang bis Ende:

1. **Schlussfolgern** — der Agent erreicht etwas Dauerhaftes (eine Entscheidung, ein verifiziertes Ergebnis, warum Code so ist wie er ist) und ruft `memorize` mit strukturierten Behauptungen und `evidence`-Pfaden auf.

```jsonc
memorize({
  "agent_id": "dev",
  "node_label": "AuthTokenFlow",
  "claims": [
    { "label": "TokenValidator", "text": "validates JWTs via HMAC",
      "confidence": "high", "evidence": ["src/auth/token.rs"] }
  ]
})
```

2. **Verankern** — m1nd schreibt eine graph-native `.light.md` unter `<runtime>/agent-memory/`, ingest sie (`adapter=light mode=merge`), und löst jeden `evidence`-Pfad zum echten Code-Knoten über eine `grounded_in`-Kante auf — so lebt das Wissen im selben Aktivierungsraum wie Code und taucht in `seek` / `activate` / `impact` auf.
3. **Auto-Load** — bei jedem zukünftigen Session-Start ingest `m1nd` `agent-memory/` automatisch und meldet es in `session_handshake.agent_memory`. Vergangene Erkenntnisse überleben einen `mode=replace`-Ingest und sind einfach *da*.
4. **Staleness selbst markieren** — `cross_verify(check: ["evidence_freshness"])` re-hasht jede zitierte Datei und benennt, welche Behauptungen veraltet sind, weil sich ihr Code geändert hat — so sagt das Gedächtnis, wann es lügt, anstatt dich irrezuführen.

Diese Schleife wurde live von Ende zu Ende bewiesen: `memorize` → `grounded_in`-Kante → Freshness-Flag auf bearbeiteter Datei → überlebt `mode=replace` → Boot-Auto-Load. Schließt du eine begrenzte Mission? Übergib `write_light_memory: true` an `mission_close`, um seine verifizierten Behauptungen auf dieselbe Weise zu persistieren. Die Gewohnheit ist in den Server-`instructions` dokumentiert, die jeder MCP-Client bei `initialize` erhält — host-agnostic, kein client-spezifisches Plugin erforderlich.

## Der Trust- / Ehrlichkeits-Layer

Das ist das Verteidigungswürdigste, was m1nd tut, und kein Konkurrent liefert es. Die Doktrin: **Glaubwürdigkeit kommt von Ehrlichkeit, nicht davon, immer zu gewinnen.**

- **`trust_selftest`** gibt ein Urteil *vor* jedem Retrieval zurück: `full_trust`, `needs_ingest`, `wrong_workspace_binding`, `stale_binding_suspected`, oder `degraded_host_tool_surface`. Der Agent weiß, ob er fortfahren, ingestieren, rebinden oder zurückfallen soll.
- **`agent_runtime_contract`** begleitet jede Retrieve-Antwort und trägt einen `trust_mode`. Ein leeres Ergebnis wird disambiguiert — an falsches Repo gebunden vs. genuinement nichts vorhanden — niemals still als „keine Ergebnisse" gemeldet.
- **`non_claims`-Arrays** werden bei jedem Mission-Tool geliefert. m1nd teilt dem Agenten mit, was es *nicht* bewiesen hat.
- **`mission_verify` kann Nein sagen — und tut es, in getestetem Code.** Es lehnt Graph-only-Beweise ab: eine Behauptung kann sich nicht schließen ohne einen Dateilesevorgang, einen Testlauf oder eine Runtime-Probe. Der Test heißt buchstäblich `graph_only_evidence_is_not_enough`.
- **`recovery_playbook`** gibt eine deterministische, geordnete Schritt-Liste zurück, um das Binding zu reparieren.

Der Beweis für das Engagement ist das, was dafür gestrichen wurde: `savings` und `resonate` wurden in beta.7 aus der beworbenen Oberfläche entfernt, weil ein Tool, das immer behauptet zu gewinnen, nicht glaubwürdig ist. Kein Konkurrent — weder mem0, Zep, Letta, Sourcegraph, noch irgendein Code-Graph-MCP — liefert einen Layer, der dem Agenten sagt, was er *nicht* vertrauen soll und wie er sich erholt.

## Sprachabdeckung

Graph-Reasoning (`impact`, `why`, `predict`, `trace`, `taint_trace`) ist nur so gut wie der Extraktor. m1nd löst sowohl **`calls`-Kanten** (Call-Graph) als auch **Cross-File-`imports`** (Datei→Datei-Abhängigkeitsauflösung) pro Sprache auf. Die Matrix unten wurde live in einem einzelnen polyglottes Ingest bewiesen:

| Sprache | `calls` | Cross-File-Imports |
|---|:---:|:---:|
| Rust | ✅ | ✅ (`mod`/`use crate::`) |
| Python | ✅ | ✅ |
| JavaScript / TypeScript | ✅ | ✅ |
| Go | ✅ | ✅ (package) |
| Java | ✅ | ✅ (FQCN + wildcard) |
| C / C++ | ✅ | ✅ (`#include "..."`) |
| Kotlin | ✅ | ✅ (package) |
| PHP | ✅ | ✅ (PSR-4) |
| Scala | ✅ | ✅ (package) |
| Ruby | ⏳ | ✅ (`require_relative`) |
| C# | ✅ | — (Namespaces bilden nicht 1:1 auf Dateien ab) |
| Swift | ✅ | — |

Alle ✅-Zeilen sind von Ende zu Ende verifiziert (ein `caller`→`callee`-Import löst auf und der Caller emittiert Call-Kanten). Andere Sprachen fallen auf den generischen Extraktor zurück (nur `contains`). Nicht auflösbare Imports (externe Pakete, Gems, Stdlib, System-Header) werden ehrlich unaufgelöst gelassen, anstatt geraten zu werden.

## Fähigkeiten-Karte

Die Live-MCP-Oberfläche entwickelt sich mit den Releases. Verwende `tools/list` für die genaue Tool-Anzahl und Namen in deinem aktuellen Build.

| Bereich | Was es ermöglicht | Repräsentative Tools |
|---|---|---|
| Graph-Fundament | Code ingestieren, Graph-Zustand pflegen, Session-Kontinuität diagnostizieren, nützliche Pfade stärken, und Cross-Session-Gewichtsdrift erkennen | `trust_selftest`, `session_handshake`, `recovery_playbook`, `ingest`, `health`, `doctor`, `learn`, `warmup`, `drift` |
| Retrieval und Orientierung | nach Text, Pfad, Absicht, Struktur oder Beziehung suchen vor manuellen Datei-Lesevorgängen | `audit`, `search`, `glob`, `seek`, `activate`, `why`, `trace` |
| Docs und Wissensbindung | universelle Docs oder graph-natives `L1GHT` ingestieren, dann Konzepte zurück an Code verknüpfen | `ingest(adapter="universal"\|"light")`, `document_resolve`, `document_provider_health`, `document_bindings`, `document_drift`, `auto_ingest_*` |
| Navigation und Kontinuität | statusbehaftete Routen, Handoffs, Baselines und Untersuchungsgedächtnis über Sessions hinweg pflegen | `perspective_*`, `trail_*`, `coverage_session`, `boot_memory`, `persist` |
| Mission Control und Beweisdisziplin | eine begrenzte Route pflegen, Ereignisse aufzeichnen, von Graph-Orientierung zu direktem Beweis wechseln, Handoff durchführen und mit expliziten Lücken schließen | `mission_start`, `mission_event`, `mission_next`, `mission_verify`, `mission_handoff`, `mission_close` |
| Änderungsplanung und -beweis | über Impact, Co-Change, fehlende Schritte, Fehlerpfade und strukturelle Behauptungen nachdenken | `impact`, `predict`, `validate_plan`, `missing`, `hypothesize`, `counterfactual`, `differential` |
| Qualität, Sicherheit und Architektur | Muster, Taint-Pfade, Trust-Grenzen, Duplikation, Layer-Verletzungen, Typ-Flows und Refactoring-Ziele erkennen | `scan`, `scan_all`, `heuristics_surface`, `antibody_*`, `taint_trace`, `type_trace`, `trust`, `layers`, `layer_inspect`, `twins`, `fingerprint`, `flow_simulate`, `epidemic`, `tremor`, `refactor_plan` |
| Zeit, Runtime und Multi-Repo-Arbeit | Git-Historie, Drift, versteckte Co-Change-Kanten, Runtime-Overlays und Cross-Repo-Referenzen inspizieren | `timeline`, `diverge`, `ghost_edges`, `runtime_overlay`, `external_references`, `federate`, `federate_auto` |
| Betrieb und Monitoring | Repo-Zustand prüfen, Graph-vs-Disk-Wahrheit verifizieren, Daemon-Watches ausführen, Zustand persistieren und dauerhafte Alerts fördern | `audit`, `cross_verify`, `daemon_*`, `alerts_*`, `panoramic`, `metrics`, `report`, `persist`, `diagram`, `help` |
| Chirurgische Edit-Vorbereitung und -Ausführung | kompakten verbundenen Kontext ziehen, Schreibvorgänge voranschauen und graph-aware Edits anwenden | `surgical_context`, `surgical_context_v2`, `view`, `batch_view`, `edit_preview`, `edit_commit`, `apply`, `apply_batch` |

**Tiering:** 27 essentielle Tools werden standardmäßig beworben, um die Tool-Auswahlkosten zu reduzieren; setze `M1ND_TOOL_TIER=full`, um die vollständige Oberfläche zu bewerben (100+ Tools: RETROBUILDER, Perspectives, Federation, Daemon). Einige Tools (`resonate`, `savings`, `lock_*`) bleiben per Namen aufrufbar, sind aber nicht auf der beworbenen Oberfläche. Versteckte Tools sind immer über `tools/call` aufrufbar — Tiering kontrolliert nur, was `tools/list` exposes.

## Die Betriebsschleifen

Das Agent-Pack ist Teil des Produkts, keine dekorative Dokumentation. m1nd ist am stärksten, wenn der Agent die *Betriebsschleife* erhält, nicht nur einen Graph-Endpunkt. Fünf benannte Protokolle werden im Pack geliefert:

- **Session-Start** — `trust_selftest` → `recovery_playbook` wenn Trust nicht vollständig ist → `ingest` falls nötig → `seek`/`audit`.
- **Recherche** — `ingest` → `activate(query)` → `why(source, target)` → `missing(topic)` → `learn(feedback)` → `memorize` jedes dauerhafte Ergebnis.
- **Code-Änderung** — `impact(node)` für Blast Radius → `predict(node)` → `counterfactual(nodes)` → `surgical_context_v2` → `memorize` die Entscheidung und das Warum.
- **Tiefenanalyse** — `fingerprint`, `diverge`, `ghost_edges`, `taint_trace`, `twins`, `refactor_plan`, `runtime_overlay` (die RETROBUILDER-Linse) für versteckte Kopplung, Sicherheitspfade, strukturelle Duplikate und Runtime-Wärme.
- **Gedächtnis** — dauerhafte Schlussfolgerungen mit `memorize` persistieren, mit `confidence` und `evidence`-Pfaden.

Mission Control ist Beweisdisziplin, keine Feature-Liste. `mission_next` gibt genau einen Zug plus `do_not`-Guardrails zurück; `mission_verify` lehnt Graph-only-Behauptungen ab; `mission_close` drängt den Agenten immer, verifizietes Wissen zu persistieren, und zeichnet Lücken und Non-Claims auf. Im `bug_hunt`-Modus erfordert MC0 einen abschließenden `direct_sweep` nach verifizierten Erkenntnissen vor dem Schließen, damit Agenten den Negativraum prüfen.

**Vorbehalt:** `predict` hat **nur strukturellen Fallback** bis `ghost_edges` die Git-Co-Change-Matrix lädt — führe `ghost_edges` zuerst aus, wenn du echte Co-Change-Wahrscheinlichkeit brauchst.

## Nachweise

Jede Zeile ist genau auf das kalibriert, was gemessen wurde. m1nd führt keine Einsparungs- oder ROI-Zahlen an — das ist der Punkt.

| Behauptung | Ergebnis | Quelle / Einschränkung |
|---|---|---|
| `activate` / `impact`-Latenz | sub-µs `activate`, sub-ms `impact` | Criterion-Benchmarks in `m1nd-core/benches/` auf einem synthetischen 1K-Knoten-Graph — [Methodik](https://m1nd.world/wiki/benchmarks.html); als Größenordnung behandeln. |
| Sprachmatrix | Calls + Cross-File-Imports für 10 Sprachen (+ Ruby Cross-File) | Von Ende zu Ende in einem einzelnen polyglottes Ingest verifiziert; sprachspezifische Tests in `m1nd-ingest`. Siehe [Sprachabdeckung](#sprachabdeckung). |
| Post-Write-Validierungsstichprobe | 12/12 korrekt klassifiziert | Interner Runtime-Check. |
| Geseedeter Bug-Hunt | 16/20 in der ersten akzeptierten `humanize`-Seed-Defekt-Runde (m1nd-trained); `m1nd-basic` und direkt je 8/15 | Interne Produktnachweise, `public_claim_worthy=false` — kein universeller Benchmark. |
| Gedächtnis-Selbstverifizierung | live von Ende zu Ende bewiesen | `memorize` → `grounded_in` → Freshness-Flag auf bearbeiteter Datei → überlebt replace → Boot-Auto-Load. |

## Einschränkungen

`m1nd` ergänzt deinen LSP, Compiler, Test-Runner, Sicherheitsscanner und Observability-Stack, anstatt sie zu ersetzen. Es ist am nützlichsten vor der Suche, Überprüfung oder Änderung, und immer wenn Docs, Impact oder Kontinuität wichtig sind.

Es ist **weniger nützlich** wenn:

- exakte Textsuche die Frage bereits beantwortet
- Compiler- oder Runtime-Wahrheit das Einzige ist, was du brauchst
- die Aufgabe eine triviale lokale Datei-Aktion ohne strukturelle Unsicherheit ist

**Braucht Fütterung:** `trust` und `tremor` starten mit neutralen Priors, bis `learn`-Feedback / `ghost_edges`-Daten sich ansammeln, und `predict` braucht `ghost_edges` geladen, bevor sein Co-Change-Signal bedeutsam ist. Diese verbessern sich mit der Nutzung; sie sind ehrlich darüber, beim Start uninformiert zu sein.

## Architektur auf einen Blick

Drei Kern-Rust-Crates plus eine auxiliäre Bridge:

- **`m1nd-mcp`** — der MCP-Server und die operative Runtime-Oberfläche.
- **`m1nd-core`** — die Graph-Engine: ein `WavefrontEngine`, der Spreading Activation, Hebbianische Plastizität, CSR-Adjacency und Git-abgeleitete Ghost Edges durchführt.
- **`m1nd-ingest`** — Extraktions-, Routing- und Graph-Konstruktions-Adapter (Code, universelle Docs, L1GHT).
- **`m1nd-openclaw`** — auxiliäre OpenClaw-Bridge (Unix-Socket-Lane, unabhängig versioniert).

Aktuelle Crate-Versionen: `m1nd-core`, `m1nd-ingest`, `m1nd-mcp` alle `0.9.0-beta.8`.

<p align="center">
  <img src="../.github/m1nd-architecture-overview-v2.jpeg" alt="m1nd Architektur-Übersicht" width="960" />
</p>

Für Federation, Perspectives, RETROBUILDER, Multi-Agent-Koordination und die vollständige Agent-Pack- und Operator-Referenz, siehe das [kanonische Wiki](https://m1nd.world/wiki/), [docs/AGENT-PACKS.md](../docs/AGENT-PACKS.md) und [EXAMPLES.md](../EXAMPLES.md).

## Beitragen

Beiträge sind willkommen bei Extraktoren und Adaptern, MCP/Runtime-Tooling, Benchmarks, Docs und Graph-Algorithmen. Siehe [CONTRIBUTING.md](../CONTRIBUTING.md).

## Lizenz

MIT. Siehe [LICENSE](../LICENSE).
