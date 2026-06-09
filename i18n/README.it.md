🇬🇧 [English](../README.md) | 🇧🇷 [Português](README.pt-BR.md) | 🇪🇸 [Español](README.es.md) | 🇮🇹 [Italiano](README.it.md) | 🇫🇷 [Français](README.fr.md) | 🇩🇪 [Deutsch](README.de.md) | 🇨🇳 [中文](README.zh.md) | 🇯🇵 [日本語](README.ja.md)

<p align="center">
  <img src="../.github/m1nd-logo.svg" alt="m1nd" width="400" />
</p>

<h1 align="center">Un Runtime di Missione Locale per Agenti di Coding</h1>

<p align="center">
  <strong>Il tuo agente di coding smette di partire alla cieca.</strong><br/>
  <em>Local-first. MCP-native. Grafo di memoria, trust e ragionamento sui cambiamenti per agent host.</em>
</p>

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-core"><img src="https://img.shields.io/crates/v/m1nd-core.svg" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="../LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="Licenza" /></a>
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

**m1nd è un runtime di missione locale per agenti di coding — governa il ciclo operativo, non solo il retrieval.**

> grep trova testo. La ricerca vettoriale trova chunk simili. `m1nd` dà agli agenti un grafo locale di cosa è connesso, cosa è cambiato, cosa si rompe, cosa è andato in drift, e dove riprendere.

Tre cose che qui coesistono e non si trovano in nessun altro strumento:

- **Grafo causale del codice** — `impact` prima di modificare mostra il blast radius che non avevi letto; `ghost_edges` fa emergere i file che cambiano sempre insieme ma non condividono alcun import.
- **Memoria auto-verificante** — `memorize` ancora i risultati a nodi reali del codice; `cross_verify` li segnala come obsoleti quando quel codice cambia.
- **Un layer di trust / recovery** — ogni risultato porta un trust mode; `trust_selftest` e `recovery_playbook` dicono all'agente quando il binding del workspace è sbagliato e come recuperare.

<p align="center">
  <img src="../.github/m1nd-agent-first-map-v2.jpeg" alt="Ciclo agente tradizionale vs ciclo m1nd-grounded" width="960" />
</p>

## Avvio Rapido

Il percorso minimo funzionante — installa dai sorgenti (sempre aggiornato), verifica la salute, collega il tuo host:

```bash
git clone https://github.com/maxkle1nz/m1nd.git && cd m1nd
npm install -g .
m1nd doctor
m1nd install-skills codex          # oppure: claude / gemini / antigravity / generic
m1nd mcp-config codex --project /your/project
```

Oppure dal canale npm beta: `npm install -g @maxkle1nz/m1nd@beta`.

Mappa di installazione completa, pack per host, build del runtime nativo e flag di aggiornamento: [docs/AGENT-PACKS.md](../docs/AGENT-PACKS.md) · configurazione client per client: [matrice di integrazione](../docs/IDE-INTEGRATIONS.md).

### Punto di Ingresso per Agenti

Gli agenti analizzano questo README. Quando la sessione MCP host è obsoleta, associata al repository sbagliato, o non ancora caricata, usa la CLI host-neutral — lancia un runtime isolato, lo associa al repository e restituisce un unico envelope leggibile da macchina:

```bash
m1nd agent first-minute --repo /your/project --query "understand this system" --json
```

`m1nd agent first-minute` è il primo contatto più sicuro con un nuovo repository. Definisce lo scope del repository, stabilisce il trust, esegue l'ingest se necessario, fa un passaggio di orientamento delimitato, restituisce anchor candidati, e poi dice all'agente di provare direttamente da sorgente, test, output del compilatore/runtime, log o probe.

All'interno di una sessione MCP, la dottrina è questo ciclo di trust — stabilisci il trust *prima* di fidarti di qualsiasi retrieval:

```jsonc
// 0. Verifica il binding in una sola chiamata (verdetto prima del retrieval)
{"method":"tools/call","params":{"name":"trust_selftest","arguments":{"agent_id":"dev"}}}

// 1. Se il verdetto non è full_trust, chiedi il percorso di recovery deterministico
{"method":"tools/call","params":{"name":"recovery_playbook","arguments":{"agent_id":"dev"}}}

// 2. Costruisci la verità del grafo
{"method":"tools/call","params":{"name":"ingest","arguments":{"path":"/your/project","agent_id":"dev"}}}

// 3. Poni una domanda strutturale — risultati vuoti dicono *perché*, mai solo "nessun risultato"
{"method":"tools/call","params":{"name":"activate","arguments":{"query":"authentication flow","agent_id":"dev"}}}
```

**Ciclo prima sessione, in quattro mosse:** `trust_selftest` → `ingest` → `seek`/`audit` → `memorize` il risultato duraturo così la sessione successiva parte avvantaggiata.

## Cosa m1nd Non È

`m1nd` non è solo:

- uno strumento di ricerca del codice con un indice più grande
- un layer di RAG sul repository che recupera solo file o chunk
- un database a grafo che lascia le decisioni di workflow al client
- un sostituto dell'analisi statica per compilatore, test o strumenti di sicurezza
- un bundle MCP di utility non correlate

È il layer che trasforma quelle superfici in un sistema operativo su cui un agente può ragionare e agire. Non per ricerche su singolo file, semplici grep, o verità del compilatore — usa strumenti semplici in quei casi.

## Perché gli Agenti ne Hanno Bisogno

Senza m1nd, ogni sessione inizia con loop di grep e riorientamento manuale; i risultati della settimana scorsa sono persi, e un risultato di ricerca vuoto è indistinguibile da un binding workspace sbagliato. Con m1nd, la sessione inizia con un verdetto di trust, i risultati passati si caricano automaticamente già ancorati al codice che li supporta, e i risultati vuoti dicono *perché*.

Gli agenti su codebase reali non falliscono perché non sanno cercare. Falliscono perché non hanno un modello operativo. Ricostruiscono il contesto da zero ad ogni sessione, modificano senza conoscere il blast radius, e non riescono a distinguere un risultato vuoto che significa "non esiste nulla" da uno che significa "repository sbagliato."

Funziona per codebase piccoli. Crolla quando il progetto ha artefatti generati, spec, doc, cronologia nascosta di co-change, più agenti e handoff lunghi. Il problema non è solo il ragionamento dell'agente — l'agente non ha un modello duraturo della struttura del codebase. `m1nd` glielo dà: un grafo causale del codice con spreading activation attraverso dimensioni strutturali, semantiche, temporali e causali, più plasticità Hebbiana che si accumula per agente tra le sessioni.

## Memoria Composta (L1GHT)

La maggior parte degli strumenti dà all'agente un *retrieval* migliore. `m1nd` permette anche all'agente di **creare conoscenza durevole e leggibile dalla macchina** che si accumula tra le sessioni e rimane onesta rispetto al codice. L1GHT trasforma la conoscenza creata in struttura graph-native che si auto-segnala quando il codice che cita cambia — le affermazioni sicure diffondono più attivazione di quelle incerte.

Il ciclo, dall'inizio alla fine:

1. **Concludi** — l'agente raggiunge qualcosa di duraturo (una decisione, un risultato verificato, perché il codice è così) e chiama `memorize` con affermazioni strutturate e percorsi `evidence`.

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

2. **Ancora** — m1nd scrive un `.light.md` graph-native sotto `<runtime>/agent-memory/`, lo ingerisce (`adapter=light mode=merge`), e risolve ogni percorso `evidence` al nodo reale del codice tramite un edge `grounded_in` — così la conoscenza vive nello stesso spazio di attivazione del codice e emerge in `seek` / `activate` / `impact`.
3. **Auto-load** — all'inizio di ogni sessione futura, `m1nd` ingerisce `agent-memory/` automaticamente e lo riporta in `session_handshake.agent_memory`. I risultati passati sopravvivono a un ingest `mode=replace` e sono semplicemente *lì*.
4. **Auto-segnala la staleness** — `cross_verify(check: ["evidence_freshness"])` ri-hash ogni file citato e nomina quali affermazioni sono diventate obsolete perché il loro codice è cambiato — così la memoria ti dice quando mente invece di ingannarti.

Questo ciclo è stato provato live end-to-end: `memorize` → edge `grounded_in` → segnale di freshness su file modificato → sopravvive a `mode=replace` → boot auto-load. Stai chiudendo una missione delimitata? Passa `write_light_memory: true` a `mission_close` per persistere le sue affermazioni verificate allo stesso modo. L'abitudine è documentata nelle `instructions` del server che ogni client MCP riceve all'`initialize` — host-agnostic, nessun plugin client-specifico richiesto.

## Il Layer di Trust / Onestà

Questa è la cosa più difendibile che m1nd fa, e nessun concorrente la offre. La dottrina: **la credibilità viene dall'onestà, non dal vincere sempre.**

- **`trust_selftest`** restituisce un verdetto *prima* di qualsiasi retrieval: `full_trust`, `needs_ingest`, `wrong_workspace_binding`, `stale_binding_suspected`, o `degraded_host_tool_surface`. L'agente sa se procedere, eseguire ingest, rebindare, o fare fallback.
- **`agent_runtime_contract`** è presente in ogni risposta di retrieval, portando un `trust_mode`. Un risultato vuoto è disambiguato — associato al repository sbagliato vs. genuinamente niente lì — mai riportato silenziosamente come "nessun risultato."
- **Array `non_claims`** presenti su ogni tool di missione. m1nd dice all'agente cosa *non* ha provato.
- **`mission_verify` può dire no — e lo fa, nel codice testato.** Rifiuta prove solo da grafo: un'affermazione non può chiudersi senza una lettura di file, un'esecuzione di test, o un probe di runtime. Il test si chiama letteralmente `graph_only_evidence_is_not_enough`.
- **`recovery_playbook`** restituisce un elenco di passi deterministico e ordinato per riparare il binding.

La prova dell'impegno è ciò che è stato eliminato per esso: `savings` e `resonate` sono stati rimossi dalla superficie pubblicizzata nella beta.7 perché uno strumento che afferma sempre di vincere non è credibile. Nessun concorrente — né mem0, Zep, Letta, Sourcegraph, né alcun MCP code-graph — offre un layer che dice all'agente cosa *non* fidarsi e come recuperare.

## Copertura Linguistica

Il ragionamento sul grafo (`impact`, `why`, `predict`, `trace`, `taint_trace`) è valido solo quanto l'estrattore. m1nd risolve sia gli **edge `calls`** (call graph) che i **`imports` cross-file** (risoluzione delle dipendenze file→file) per linguaggio. La matrice sotto è stata provata live in un singolo ingest poliglotta:

| Linguaggio | `calls` | import cross-file |
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
| C# | ✅ | — (i namespace non mappano 1:1 ai file) |
| Swift | ✅ | — |

Tutte le righe ✅ sono verificate end-to-end (un import `caller`→`callee` si risolve e il caller emette edge di chiamata). Gli altri linguaggi cadono sull'estrattore generico (solo `contains`). Gli import non risolvibili (pacchetti esterni, gem, stdlib, header di sistema) sono onestamente lasciati irrisolti anziché indovinati.

## Mappa delle Capacità

La superficie MCP live si evolve con i rilasci. Usa `tools/list` per il conteggio esatto degli strumenti e i nomi nella tua build corrente.

| Area | Cosa abilita | Strumenti rappresentativi |
|---|---|---|
| Fondamenta del grafo | ingerire codice, mantenere lo stato del grafo, diagnosticare la continuità della sessione, rinforzare i percorsi utili, e rilevare il drift dei pesi tra sessioni | `trust_selftest`, `session_handshake`, `recovery_playbook`, `ingest`, `health`, `doctor`, `learn`, `warmup`, `drift` |
| Retrieval e orientamento | cercare per testo, percorso, intento, struttura, o relazione prima delle letture manuali dei file | `audit`, `search`, `glob`, `seek`, `activate`, `why`, `trace` |
| Doc e binding della conoscenza | ingerire doc universali o `L1GHT` graph-native, poi collegare i concetti al codice | `ingest(adapter="universal"\|"light")`, `document_resolve`, `document_provider_health`, `document_bindings`, `document_drift`, `auto_ingest_*` |
| Navigazione e continuità | mantenere route stateful, handoff, baseline, e memoria investigativa tra sessioni | `perspective_*`, `trail_*`, `coverage_session`, `boot_memory`, `persist` |
| Mission control e disciplina della prova | mantenere una route delimitata, registrare eventi, passare dall'orientamento sul grafo alla prova diretta, fare handoff, e chiudere con gap espliciti | `mission_start`, `mission_event`, `mission_next`, `mission_verify`, `mission_handoff`, `mission_close` |
| Pianificazione e prova dei cambiamenti | ragionare su impatto, co-change, passi mancanti, percorsi di fallimento, e affermazioni strutturali | `impact`, `predict`, `validate_plan`, `missing`, `hypothesize`, `counterfactual`, `differential` |
| Qualità, sicurezza e architettura | rilevare pattern, percorsi di taint, confini di trust, duplicazioni, violazioni di layer, flussi di tipo e target di refactoring | `scan`, `scan_all`, `heuristics_surface`, `antibody_*`, `taint_trace`, `type_trace`, `trust`, `layers`, `layer_inspect`, `twins`, `fingerprint`, `flow_simulate`, `epidemic`, `tremor`, `refactor_plan` |
| Tempo, runtime e lavoro multi-repo | ispezionare la cronologia git, drift, edge di co-change nascosti, overlay di runtime e riferimenti cross-repo | `timeline`, `diverge`, `ghost_edges`, `runtime_overlay`, `external_references`, `federate`, `federate_auto` |
| Operazioni e monitoraggio | verificare lo stato del repo, verificare la verità grafo-vs-disco, eseguire watch daemon, persistere lo stato, e far emergere alert durevoli | `audit`, `cross_verify`, `daemon_*`, `alerts_*`, `panoramic`, `metrics`, `report`, `persist`, `diagram`, `help` |
| Preparazione ed esecuzione di modifiche chirurgiche | estrarre contesto connesso compatto, anteprima delle scritture, e applicare modifiche graph-aware | `surgical_context`, `surgical_context_v2`, `view`, `batch_view`, `edit_preview`, `edit_commit`, `apply`, `apply_batch` |

**Livelli:** 27 strumenti essenziali sono pubblicizzati di default per ridurre il costo di selezione degli strumenti; imposta `M1ND_TOOL_TIER=full` per pubblicizzare la superficie completa (100+ strumenti: RETROBUILDER, perspectives, federation, daemon). Alcuni strumenti (`resonate`, `savings`, `lock_*`) rimangono chiamabili per nome ma non sono sulla superficie pubblicizzata. Gli strumenti nascosti sono sempre chiamabili via `tools/call` — il livello controlla solo ciò che `tools/list` espone.

## I Cicli Operativi

Il pack per agenti è parte del prodotto, non documentazione decorativa. m1nd è più potente quando l'agente riceve il *ciclo operativo*, non solo un endpoint del grafo. Cinque protocolli nominati sono inclusi nel pack:

- **Avvio Sessione** — `trust_selftest` → `recovery_playbook` se il trust non è pieno → `ingest` se necessario → `seek`/`audit`.
- **Ricerca** — `ingest` → `activate(query)` → `why(source, target)` → `missing(topic)` → `learn(feedback)` → `memorize` qualsiasi risultato duraturo.
- **Modifica Codice** — `impact(node)` per il blast radius → `predict(node)` → `counterfactual(nodes)` → `surgical_context_v2` → `memorize` la decisione e il perché.
- **Analisi Approfondita** — `fingerprint`, `diverge`, `ghost_edges`, `taint_trace`, `twins`, `refactor_plan`, `runtime_overlay` (la lente RETROBUILDER) per accoppiamento nascosto, percorsi di sicurezza, duplicati strutturali e calore di runtime.
- **Memoria** — persisti conclusioni durevoli con `memorize`, portando `confidence` e percorsi `evidence`.

Mission Control è disciplina della prova, non un elenco di funzionalità. `mission_next` restituisce esattamente una mossa più guardrail `do_not`; `mission_verify` rifiuta affermazioni solo da grafo; `mission_close` spinge sempre l'agente a persistere la conoscenza verificata e registra gap e non-claim. In modalità `bug_hunt`, MC0 richiede un `direct_sweep` finale dopo i risultati verificati prima della chiusura, così gli agenti controllano lo spazio negativo.

**Avvertenza:** `predict` ha **solo fallback strutturale** finché `ghost_edges` non carica la matrice di co-change git — esegui `ghost_edges` prima quando hai bisogno della reale probabilità di co-change.

## Evidenze

Ogni riga è calibrata esattamente a ciò che è stato misurato. m1nd non guida con numeri di risparmio o ROI — questo è il punto.

| Affermazione | Risultato | Fonte / calibrazione |
|---|---|---|
| Latenza `activate` / `impact` | `activate` sub-µs, `impact` sub-ms | Benchmark Criterion in `m1nd-core/benches/` su un grafo sintetico da 1K nodi — [metodologia](https://m1nd.world/wiki/benchmarks.html); trattare come ordine di grandezza. |
| Matrice linguistica | chiamate + import cross-file per 10 linguaggi (+ Ruby cross-file) | Verificato end-to-end in un singolo ingest poliglotta; test per linguaggio in `m1nd-ingest`. Vedi [Copertura Linguistica](#copertura-linguistica). |
| Campione di validazione post-scrittura | 12/12 classificati correttamente | Controllo di runtime interno. |
| Bug-hunt con semi | 16/20 al primo round accettato di difetti seminati `humanize` (m1nd-trained); `m1nd-basic` e diretto ciascuno 8/15 | Evidenza di prodotto interno, `public_claim_worthy=false` — non un benchmark universale. |
| Auto-verifica della memoria | provata live end-to-end | `memorize` → `grounded_in` → segnale di freshness su file modificato → sopravvive a replace → boot auto-load. |

## Limiti

`m1nd` complementa piuttosto che sostituire il tuo LSP, compilatore, test runner, scanner di sicurezza e stack di osservabilità. È più utile prima della ricerca, della revisione o di una modifica, e ogni volta che doc, impatto o continuità sono importanti.

È **meno utile** quando:

- la ricerca esatta di testo risponde già alla domanda
- la verità del compilatore o del runtime è l'unica cosa di cui hai bisogno
- il task è un'azione locale banale su file senza incertezza strutturale

**Necessita di alimentazione:** `trust` e `tremor` partono con prior neutri finché non si accumula feedback da `learn` / dati da `ghost_edges`, e `predict` ha bisogno che `ghost_edges` sia caricato prima che il suo segnale di co-change sia significativo. Migliorano con l'uso; sono onesti sull'essere non informati all'avvio.

## Architettura in Sintesi

Tre crate Rust core più un bridge ausiliario:

- **`m1nd-mcp`** — il server MCP e la superficie del runtime operativo.
- **`m1nd-core`** — il motore del grafo: un `WavefrontEngine` che fa spreading activation, plasticità Hebbiana, adiacenza CSR e ghost edge derivati da git.
- **`m1nd-ingest`** — adapter di estrazione, routing e costruzione del grafo (codice, doc universali, L1GHT).
- **`m1nd-openclaw`** — bridge ausiliario OpenClaw (lane Unix-socket, versioning indipendente).

Versioni correnti dei crate: `m1nd-core`, `m1nd-ingest`, `m1nd-mcp` tutti `0.9.0-beta.7`.

<p align="center">
  <img src="../.github/m1nd-architecture-overview-v2.jpeg" alt="Panoramica architettura m1nd" width="960" />
</p>

Per federation, perspectives, RETROBUILDER, coordinamento multi-agente e il riferimento completo del pack agenti e operatore, consulta il [wiki canonico](https://m1nd.world/wiki/), [docs/AGENT-PACKS.md](../docs/AGENT-PACKS.md) e [EXAMPLES.md](../EXAMPLES.md).

## Contribuire

I contributi sono benvenuti su estrattori e adapter, tooling MCP/runtime, benchmark, doc e algoritmi di grafo. Vedi [CONTRIBUTING.md](../CONTRIBUTING.md).

## Licenza

MIT. Vedi [LICENSE](../LICENSE).
