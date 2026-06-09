🇬🇧 [English](../README.md) | 🇧🇷 [Português](README.pt-BR.md) | 🇪🇸 [Español](README.es.md) | 🇮🇹 [Italiano](README.it.md) | 🇫🇷 [Français](README.fr.md) | 🇩🇪 [Deutsch](README.de.md) | 🇨🇳 [中文](README.zh.md) | 🇯🇵 [日本語](README.ja.md)

<p align="center">
  <img src="../.github/m1nd-logo.svg" alt="m1nd" width="400" />
</p>

<h1 align="center">Un Runtime de Misión Local para Agentes de Código</h1>

<p align="center">
  <strong>Tu agente de código deja de empezar a ciegas.</strong><br/>
  <em>Local-first. MCP-nativo. Memoria en grafo, confianza y razonamiento sobre cambios para hosts de agentes.</em>
</p>

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-core"><img src="https://img.shields.io/crates/v/m1nd-core.svg" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="../LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
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

**m1nd es un runtime de misión local para agentes de código — gobierna el loop operacional, no solo la recuperación de datos.**

> `grep` encuentra texto. La búsqueda vectorial encuentra chunks similares. `m1nd` da a los agentes un grafo local de qué se conecta, qué cambió, qué se rompe, qué ha derivado y dónde retomar.

Tres cosas coexisten aquí en ninguna otra herramienta:

- **Grafo causal de código** — `impact` antes de editar muestra el blast radius que no leíste; `ghost_edges` revela archivos que siempre cambian juntos pero no comparten ningún import.
- **Memoria auto-verificable** — `memorize` ancla hallazgos a nodos reales del código; `cross_verify` los marca como obsoletos cuando ese código cambia.
- **Una capa de confianza y recuperación** — cada resultado lleva un modo de confianza; `trust_selftest` y `recovery_playbook` le dicen al agente cuándo el binding del workspace está mal y cómo recuperarse.

<p align="center">
  <img src="../.github/m1nd-agent-first-map-v2.jpeg" alt="Loop tradicional de agente vs loop anclado en m1nd" width="960" />
</p>

## Inicio Rápido

El camino mínimo feliz — instala desde el fuente (siempre actualizado), verifica la salud, conecta tu host:

```bash
git clone https://github.com/maxkle1nz/m1nd.git && cd m1nd
npm install -g .
m1nd doctor
m1nd install-skills codex          # o: claude / gemini / antigravity / generic
m1nd mcp-config codex --project /your/project
```

O desde el canal beta de npm: `npm install -g @maxkle1nz/m1nd@beta`.

Mapa completo de instalación, paquetes de host, build nativo del runtime y flags de actualización: [docs/AGENT-PACKS.md](../docs/AGENT-PACKS.md) · configuración por cliente: [matriz de integración](../docs/IDE-INTEGRATIONS.md).

### Punto de Entrada del Agente

Los agentes analizan este README. Cuando la sesión MCP del host está obsoleta, vinculada al repositorio equivocado o aún no cargada, usa el CLI neutro de host — lanza un runtime aislado, lo vincula al repositorio y devuelve un único sobre legible por máquina:

```bash
m1nd agent first-minute --repo /your/project --query "understand this system" --json
```

`m1nd agent first-minute` es el primer contacto más seguro para un repositorio nuevo. Delimita el alcance del repositorio, establece confianza, ingiere si es necesario, ejecuta una única pasada de orientación acotada, devuelve anclas candidatas y luego le dice al agente que pruebe directamente desde el fuente, tests, salida del compilador/runtime, logs o sondas.

Dentro de una sesión MCP, la doctrina es este loop de confianza — establece confianza *antes* de creer cualquier recuperación:

```jsonc
// 0. Confía en el binding en una llamada (veredicto antes de la recuperación)
{"method":"tools/call","params":{"name":"trust_selftest","arguments":{"agent_id":"dev"}}}

// 1. Si el veredicto no es full_trust, pide el camino de recuperación determinístico
{"method":"tools/call","params":{"name":"recovery_playbook","arguments":{"agent_id":"dev"}}}

// 2. Construye la verdad del grafo
{"method":"tools/call","params":{"name":"ingest","arguments":{"path":"/your/project","agent_id":"dev"}}}

// 3. Haz una pregunta estructural — los resultados vacíos dicen *por qué*, nunca solo "sin resultados"
{"method":"tools/call","params":{"name":"activate","arguments":{"query":"authentication flow","agent_id":"dev"}}}
```

**Loop de primera sesión, en cuatro movimientos:** `trust_selftest` → `ingest` → `seek`/`audit` → `memorize` el hallazgo duradero para que la próxima sesión empiece adelantada.

## Lo Que m1nd No Es

`m1nd` no es solo:

- una herramienta de búsqueda de código con un índice más grande
- una capa de RAG de repositorio que solo recupera archivos o chunks
- una base de datos de grafo que deja las decisiones de workflow al cliente
- un reemplazo de análisis estático para el compilador, tests o herramientas de seguridad
- un bundle MCP de utilidades sin relación entre sí

Es la capa que convierte esas superficies en un sistema operacional sobre el que un agente puede razonar y actuar. No sirve para lookups de un solo archivo, grep simple o verdad del compilador — usa herramientas simples en esos casos.

## Por Qué los Agentes lo Necesitan

Sin m1nd, cada sesión empieza con loops de grep y reorientación manual; los hallazgos de la semana pasada desaparecieron, y un resultado de búsqueda vacío es indistinguible de un binding de workspace incorrecto. Con m1nd, la sesión comienza con un veredicto de confianza, los hallazgos pasados se cargan automáticamente ya anclados al código que los respalda, y los resultados vacíos dicen *por qué*.

Los agentes en codebases reales no fallan porque no puedan buscar. Fallan porque no tienen un modelo operacional. Reconstruyen contexto desde cero en cada sesión, editan sin conocer el blast radius, y no pueden distinguir un resultado vacío que significa "no existe nada" de uno que significa "repositorio equivocado."

Eso funciona para codebases pequeñas. Se desmorona cuando el proyecto tiene artefactos generados, specs, docs, historial oculto de co-cambio, múltiples agentes y handoffs largos. El problema no es solo el razonamiento del agente — el agente no tiene un modelo duradero de la estructura del codebase. `m1nd` le da uno: un grafo causal de código con spreading activation por dimensiones estructurales, semánticas, temporales y causales, más plasticidad Hebbiana que se acumula por agente entre sesiones.

## Memoria Compuesta (L1GHT)

La mayoría de las herramientas dan al agente mejor *recuperación*. `m1nd` también permite que un agente **produzca conocimiento duradero y legible por máquina** que se acumula entre sesiones y se mantiene honesto respecto al código. L1GHT convierte el conocimiento producido en estructura nativa de grafo que se auto-señaliza cuando el código que cita cambia — las afirmaciones confiantes propagan más activación que las inciertas.

El loop, de principio a fin:

1. **Concluir** — el agente llega a algo duradero (una decisión, un hallazgo verificado, por qué el código es como es) y llama a `memorize` con afirmaciones estructuradas y rutas de `evidence`.

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

2. **Anclar** — m1nd escribe un `.light.md` nativo de grafo bajo `<runtime>/agent-memory/`, lo ingiere (`adapter=light mode=merge`) y resuelve cada ruta de `evidence` al nodo de código real vía arista `grounded_in` — haciendo que el conocimiento viva en el mismo espacio de activación que el código y emerja en `seek` / `activate` / `impact`.
3. **Carga automática** — en cada inicio de sesión futuro, `m1nd` ingiere `agent-memory/` automáticamente y lo reporta en `session_handshake.agent_memory`. Los hallazgos pasados sobreviven a una ingestión `mode=replace` y simplemente *están ahí*.
4. **Auto-señalización de obsolescencia** — `cross_verify(check: ["evidence_freshness"])` re-hashea cada archivo citado y nombra qué afirmaciones se volvieron obsoletas porque su código cambió — así la memoria avisa cuando miente, en lugar de inducir a error.

Este loop ha sido probado en vivo de extremo a extremo: `memorize` → arista `grounded_in` → flag de frescura en archivo editado → sobrevive a `mode=replace` → carga automática en el boot. ¿Cerrando una misión acotada? Pasa `write_light_memory: true` a `mission_close` para persistir sus afirmaciones verificadas de la misma manera. El hábito está documentado en las `instructions` del servidor que cada cliente MCP recibe en `initialize` — agnóstico de host, sin plugin específico de cliente requerido.

## La Capa de Confianza y Honestidad

Esta es la cosa más defendible que hace m1nd, y ningún competidor la entrega. La doctrina: **la credibilidad viene de la honestidad, no de siempre ganar.**

- **`trust_selftest`** devuelve un veredicto *antes* de cualquier recuperación: `full_trust`, `needs_ingest`, `wrong_workspace_binding`, `stale_binding_suspected` o `degraded_host_tool_surface`. El agente sabe si debe proceder, ingerir, rebindear o retroceder.
- **`agent_runtime_contract`** acompaña cada respuesta de recuperación, llevando un `trust_mode`. Un resultado vacío está desambiguado — vinculado al repositorio equivocado versus genuinamente nada ahí — nunca reportado silenciosamente como "sin resultados."
- **Arrays `non_claims`** se envían en cada herramienta de misión. m1nd le dice al agente lo que *no* probó.
- **`mission_verify` puede decir no — y lo hace, en código testado.** Rechaza evidencia solo de grafo: una afirmación no puede cerrarse sin una lectura de archivo, una ejecución de test o una sonda de runtime. El test se llama literalmente `graph_only_evidence_is_not_enough`.
- **`recovery_playbook`** devuelve una lista de pasos determinística y ordenada para reparar el binding.

La prueba del compromiso está en lo que se sacrificó por él: `savings` y `resonate` fueron retirados de la superficie anunciada en beta.7 porque una herramienta que siempre afirma ganar no es creíble. Ningún competidor — ni mem0, Zep, Letta, Sourcegraph ni ningún MCP de grafo de código — entrega una capa que le dice al agente en qué *no* confiar y cómo recuperarse.

## Cobertura de Lenguajes

El razonamiento de grafo (`impact`, `why`, `predict`, `trace`, `taint_trace`) es tan bueno como el extractor. m1nd resuelve tanto **aristas `calls`** (grafo de llamadas) como **`imports` entre archivos** (resolución de dependencia archivo→archivo) por lenguaje. La matriz a continuación fue probada en vivo en una única ingestión políglota:

| Lenguaje | `calls` | imports entre archivos |
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
| C# | ✅ | — (los namespaces no mapean 1:1 a archivos) |
| Swift | ✅ | — |

Todas las filas ✅ están verificadas de extremo a extremo (un import `caller`→`callee` resuelve y el caller emite aristas de llamada). Otros lenguajes caen de vuelta al extractor genérico (solo `contains`). Los imports no resolvibles (paquetes externos, gems, stdlib, cabeceras de sistema) se dejan honestamente sin resolver en lugar de adivinarlos.

## Mapa de Capacidades

La superficie MCP activa evoluciona con los releases. Usa `tools/list` para la cuenta exacta de herramientas y nombres en tu build actual.

| Área | Qué permite | Herramientas representativas |
|---|---|---|
| Fundación del grafo | ingerir código, mantener estado del grafo, diagnosticar continuidad de sesión, reforzar rutas útiles y detectar drift de peso entre sesiones | `trust_selftest`, `session_handshake`, `recovery_playbook`, `ingest`, `health`, `doctor`, `learn`, `warmup`, `drift` |
| Recuperación y orientación | buscar por texto, ruta, intención, estructura o relación antes de lecturas manuales de archivo | `audit`, `search`, `glob`, `seek`, `activate`, `why`, `trace` |
| Documentos y binding de conocimiento | ingerir docs universales o `L1GHT` nativo de grafo y vincular conceptos de vuelta al código | `ingest(adapter="universal"\|"light")`, `document_resolve`, `document_provider_health`, `document_bindings`, `document_drift`, `auto_ingest_*` |
| Navegación y continuidad | mantener rutas con estado, handoffs, baselines y memoria de investigación entre sesiones | `perspective_*`, `trail_*`, `coverage_session`, `boot_memory`, `persist` |
| Mission control y disciplina de prueba | mantener una ruta acotada, registrar eventos, pasar de orientación por grafo a prueba directa, hacer handoff y cerrar con brechas explícitas | `mission_start`, `mission_event`, `mission_next`, `mission_verify`, `mission_handoff`, `mission_close` |
| Planificación y prueba de cambios | razonar sobre impacto, co-cambio, pasos faltantes, rutas de fallo y afirmaciones estructurales | `impact`, `predict`, `validate_plan`, `missing`, `hypothesize`, `counterfactual`, `differential` |
| Calidad, seguridad y arquitectura | detectar patrones, rutas de taint, fronteras de confianza, duplicación, violaciones de capa, flujos de tipo y objetivos de refactorización | `scan`, `scan_all`, `heuristics_surface`, `antibody_*`, `taint_trace`, `type_trace`, `trust`, `layers`, `layer_inspect`, `twins`, `fingerprint`, `flow_simulate`, `epidemic`, `tremor`, `refactor_plan` |
| Tiempo, runtime y trabajo multi-repo | inspeccionar historial git, drift, aristas ocultas de co-cambio, overlays de runtime y referencias entre repositorios | `timeline`, `diverge`, `ghost_edges`, `runtime_overlay`, `external_references`, `federate`, `federate_auto` |
| Operaciones y monitoreo | auditar estado del repo, verificar verdad grafo-vs-disco, ejecutar watches de daemon, persistir estado y surfacear alertas duraderas | `audit`, `cross_verify`, `daemon_*`, `alerts_*`, `panoramic`, `metrics`, `report`, `persist`, `diagram`, `help` |
| Preparación y ejecución de edición quirúrgica | extraer contexto conectado compacto, previsualizar escrituras y aplicar ediciones conscientes del grafo | `surgical_context`, `surgical_context_v2`, `view`, `batch_view`, `edit_preview`, `edit_commit`, `apply`, `apply_batch` |

**Niveles:** 27 herramientas esenciales se anuncian por defecto para reducir el costo de selección de herramientas; establece `M1ND_TOOL_TIER=full` para anunciar la superficie completa (100+ herramientas: RETROBUILDER, perspectives, federación, daemon). Algunas herramientas (`resonate`, `savings`, `lock_*`) siguen siendo llamables por nombre pero no están en la superficie anunciada. Las herramientas ocultas siempre son llamables vía `tools/call` — el tiering solo controla lo que `tools/list` surfacea.

## Los Loops Operacionales

El paquete de agente es parte del producto, no documentación decorativa. m1nd es más poderoso cuando el agente recibe el *loop operacional*, no solo un endpoint de grafo. Cinco protocolos nombrados se entregan en el paquete:

- **Inicio de Sesión** — `trust_selftest` → `recovery_playbook` si la confianza no es total → `ingest` si es necesario → `seek`/`audit`.
- **Investigación** — `ingest` → `activate(query)` → `why(source, target)` → `missing(topic)` → `learn(feedback)` → `memorize` cualquier hallazgo duradero.
- **Cambio de Código** — `impact(node)` para blast radius → `predict(node)` → `counterfactual(nodes)` → `surgical_context_v2` → `memorize` la decisión y el porqué.
- **Análisis Profundo** — `fingerprint`, `diverge`, `ghost_edges`, `taint_trace`, `twins`, `refactor_plan`, `runtime_overlay` (la lente RETROBUILDER) para acoplamiento oculto, rutas de seguridad, duplicados estructurales y calor de runtime.
- **Memoria** — persiste conclusiones duraderas con `memorize`, llevando `confidence` y rutas de `evidence`.

Mission Control es disciplina de prueba, no una lista de funcionalidades. `mission_next` devuelve exactamente un movimiento más guardrails `do_not`; `mission_verify` rechaza afirmaciones solo de grafo; `mission_close` siempre insta al agente a persistir conocimiento verificado y registra brechas y non-claims. En modo `bug_hunt`, el MC0 requiere un `direct_sweep` final directo después de los hallazgos verificados antes del cierre, para que los agentes verifiquen el espacio negativo.

**Advertencia:** `predict` tiene **fallback solo estructural** hasta que `ghost_edges` cargue la matriz de co-cambio de git — ejecuta `ghost_edges` primero cuando necesites probabilidad real de co-cambio.

## Evidencias

Cada fila está calibrada exactamente a lo que se midió. m1nd no lidera con números de ahorro o ROI — ese es el punto.

| Afirmación | Resultado | Fuente / advertencia |
|---|---|---|
| Latencia de `activate` / `impact` | sub-µs `activate`, sub-ms `impact` | Benchmarks Criterion en `m1nd-core/benches/` en un grafo sintético de 1K nodos — [metodología](https://m1nd.world/wiki/benchmarks.html); trátalo como orden de magnitud. |
| Matriz de lenguajes | calls + imports entre archivos para 10 lenguajes (+ Ruby entre archivos) | Verificado de extremo a extremo en una única ingestión políglota; tests por lenguaje en `m1nd-ingest`. Ver [Cobertura de Lenguajes](#cobertura-de-lenguajes). |
| Muestra de validación post-escritura | 12/12 clasificados correctamente | Verificación de runtime interna. |
| Caza de bugs con seeds | 16/20 en la primera ronda aceptada de defectos con seed `humanize` (entrenado con m1nd); `m1nd-basic` y directo cada uno 8/15 | Evidencia interna de producto, `public_claim_worthy=false` — no es un benchmark universal. |
| Auto-verificación de memoria | probado en vivo de extremo a extremo | `memorize` → `grounded_in` → flag de frescura en archivo editado → sobrevive a replace → carga automática en el boot. |

## Límites

`m1nd` complementa en lugar de reemplazar tu LSP, compilador, test runner, escáneres de seguridad y stack de observabilidad. Es más útil antes de búsqueda, revisión o cambio, y siempre que docs, impacto o continuidad importen.

Es **menos útil** cuando:

- la búsqueda exacta de texto ya responde la pregunta
- la verdad del compilador o runtime es lo único que necesitas
- la tarea es una acción local trivial en archivo sin incertidumbre estructural

**Necesita alimentarse:** `trust` y `tremor` comienzan con priors neutros hasta que el feedback de `learn` / los datos de `ghost_edges` se acumulen, y `predict` necesita que `ghost_edges` esté cargado primero para que su señal de co-cambio sea significativa. Mejoran con el uso; son honestos sobre estar sin información al arrancar.

## Arquitectura de un Vistazo

Tres crates core en Rust más un bridge auxiliar:

- **`m1nd-mcp`** — el servidor MCP y la superficie de runtime operacional.
- **`m1nd-core`** — el motor de grafo: un `WavefrontEngine` que hace spreading activation, plasticidad Hebbiana, adyacencia CSR y aristas ghost derivadas de git.
- **`m1nd-ingest`** — extracción, enrutamiento y adapters de construcción de grafo (código, docs universales, L1GHT).
- **`m1nd-openclaw`** — bridge auxiliar OpenClaw (canal Unix socket, versionado independientemente).

Versiones actuales de los crates: `m1nd-core`, `m1nd-ingest`, `m1nd-mcp` todos en `0.9.0-beta.7`.

<p align="center">
  <img src="../.github/m1nd-architecture-overview-v2.jpeg" alt="Resumen de la arquitectura de m1nd" width="960" />
</p>

Para federación, perspectives, RETROBUILDER, coordinación multiagente y la referencia completa de paquete de agente y operador, consulta la [wiki canónica](https://m1nd.world/wiki/), [docs/AGENT-PACKS.md](../docs/AGENT-PACKS.md) y [EXAMPLES.md](../EXAMPLES.md).

## Contribuir

Las contribuciones son bienvenidas en extractores y adapters, tooling MCP/runtime, benchmarks, documentación y algoritmos de grafo. Ver [CONTRIBUTING.md](../CONTRIBUTING.md).

## Licencia

MIT. Ver [LICENSE](../LICENSE).
