🇬🇧 [English](../README.md) | 🇧🇷 [Português](README.pt-BR.md) | 🇪🇸 [Español](README.es.md) | 🇮🇹 [Italiano](README.it.md) | 🇫🇷 [Français](README.fr.md) | 🇩🇪 [Deutsch](README.de.md) | 🇨🇳 [中文](README.zh.md) | 🇯🇵 [日本語](README.ja.md)

<p align="center">
  <img src="../.github/m1nd-logo.svg" alt="m1nd" width="400" />
</p>

<h1 align="center">Un Runtime de Mission Local pour Agents de Coding</h1>

<p align="center">
  <strong>Votre agent de coding arrête de démarrer à l'aveugle.</strong><br/>
  <em>Local-first. MCP-native. Mémoire en graphe, trust et raisonnement sur les changements pour les hôtes d'agents.</em>
</p>

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-core"><img src="https://img.shields.io/crates/v/m1nd-core.svg" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="../LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="Licence" /></a>
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

**m1nd est un runtime de mission local pour agents de coding — il gouverne la boucle opérationnelle, pas seulement le retrieval.**

> grep trouve du texte. La recherche vectorielle trouve des chunks similaires. `m1nd` donne aux agents un graphe local de ce qui est connecté, ce qui a changé, ce qui casse, ce qui a dérivé, et où reprendre.

Trois choses coexistent ici qu'aucun autre outil ne réunit :

- **Graphe causal du code** — `impact` avant une modification montre le blast radius que vous n'aviez pas lu ; `ghost_edges` fait remonter les fichiers qui changent toujours ensemble mais ne partagent aucun import.
- **Mémoire auto-vérifiante** — `memorize` ancre les résultats à de vrais nœuds de code ; `cross_verify` les signale comme obsolètes quand ce code change.
- **Un layer de trust / recovery** — chaque résultat porte un trust mode ; `trust_selftest` et `recovery_playbook` indiquent à l'agent quand le binding du workspace est incorrect et comment récupérer.

<p align="center">
  <img src="../.github/m1nd-agent-first-map-v2.jpeg" alt="Boucle agent traditionnelle vs boucle m1nd-grounded" width="960" />
</p>

## Démarrage Rapide

Le chemin minimal fonctionnel — installer depuis les sources (toujours à jour), vérifier la santé, connecter votre hôte :

```bash
git clone https://github.com/maxkle1nz/m1nd.git && cd m1nd
npm install -g .
m1nd doctor
m1nd install-skills codex          # ou : claude / gemini / antigravity / generic
m1nd mcp-config codex --project /your/project
```

Ou depuis le canal npm beta : `npm install -g @maxkle1nz/m1nd@beta`.

Carte d'installation complète, packs d'hôtes, build du runtime natif et flags de mise à jour : [docs/AGENT-PACKS.md](../docs/AGENT-PACKS.md) · configuration client par client : [matrice d'intégration](../docs/IDE-INTEGRATIONS.md).

### Point d'Entrée des Agents

Les agents analysent ce README. Quand la session MCP hôte est obsolète, liée au mauvais dépôt, ou pas encore chargée, utilisez la CLI host-neutral — elle lance un runtime isolé, le lie au dépôt, et retourne une seule enveloppe lisible par machine :

```bash
m1nd agent first-minute --repo /your/project --query "understand this system" --json
```

`m1nd agent first-minute` est le premier contact le plus sûr pour un nouveau dépôt. Il délimite le dépôt, établit le trust, ingère si nécessaire, effectue un passage d'orientation borné, retourne des ancres candidates, puis dit à l'agent de prouver directement depuis la source, les tests, la sortie compilateur/runtime, les logs ou des sondes.

Dans une session MCP, la doctrine est cette boucle de trust — établir le trust *avant* de faire confiance à tout retrieval :

```jsonc
// 0. Vérifier le binding en un seul appel (verdict avant le retrieval)
{"method":"tools/call","params":{"name":"trust_selftest","arguments":{"agent_id":"dev"}}}

// 1. Si le verdict n'est pas full_trust, demander le chemin de recovery déterministe
{"method":"tools/call","params":{"name":"recovery_playbook","arguments":{"agent_id":"dev"}}}

// 2. Construire la vérité du graphe
{"method":"tools/call","params":{"name":"ingest","arguments":{"path":"/your/project","agent_id":"dev"}}}

// 3. Poser une question structurelle — les résultats vides disent *pourquoi*, jamais juste "aucun résultat"
{"method":"tools/call","params":{"name":"activate","arguments":{"query":"authentication flow","agent_id":"dev"}}}
```

**Boucle première session, en quatre mouvements :** `trust_selftest` → `ingest` → `seek`/`audit` → `memorize` le résultat durable pour que la prochaine session parte en avance.

## Ce que m1nd N'Est Pas

`m1nd` n'est pas seulement :

- un outil de recherche de code avec un index plus grand
- un layer de RAG sur le dépôt qui ne récupère que des fichiers ou des chunks
- une base de données en graphe qui laisse les décisions de workflow au client
- un remplacement d'analyse statique pour le compilateur, les tests ou les outils de sécurité
- un bundle MCP d'utilitaires sans rapport

C'est le layer qui transforme ces surfaces en un système opérationnel sur lequel un agent peut raisonner et agir. Pas pour les recherches mono-fichier, les grep simples, ou la vérité compilateur — utilisez des outils simples dans ces cas.

## Pourquoi les Agents en ont Besoin

Sans m1nd, chaque session commence par des boucles de grep et une réorientation manuelle ; les résultats de la semaine passée sont perdus, et un résultat de recherche vide est indiscernable d'un mauvais binding de workspace. Avec m1nd, la session commence par un verdict de trust, les résultats passés se chargent automatiquement déjà ancrés au code qui les supporte, et les résultats vides disent *pourquoi*.

Les agents sur de vraies bases de code n'échouent pas parce qu'ils ne savent pas chercher. Ils échouent parce qu'ils n'ont pas de modèle opérationnel. Ils reconstruisent le contexte depuis zéro à chaque session, modifient sans connaître le blast radius, et ne peuvent pas distinguer un résultat vide qui signifie « rien n'existe » d'un qui signifie « mauvais dépôt. »

Ça fonctionne pour les petites bases de code. Ça s'effondre quand le projet comporte des artefacts générés, des specs, des docs, un historique de co-change caché, plusieurs agents, et de longs handoffs. Le problème n'est pas seulement le raisonnement de l'agent — l'agent n'a pas de modèle durable de la structure de la base de code. `m1nd` lui en donne un : un graphe causal du code avec spreading activation à travers des dimensions structurelles, sémantiques, temporelles et causales, plus une plasticité Hebbienne qui se compose par agent à travers les sessions.

## Mémoire Composée (L1GHT)

La plupart des outils donnent à l'agent un meilleur *retrieval*. `m1nd` permet aussi à un agent de **créer des connaissances durables et lisibles par machine** qui se composent à travers les sessions et restent honnêtes par rapport au code. L1GHT transforme les connaissances créées en structure graph-native qui s'auto-signale quand le code qu'elle cite change — les affirmations confiantes diffusent plus d'activation que les incertaines.

La boucle, du début à la fin :

1. **Conclure** — l'agent atteint quelque chose de durable (une décision, un résultat vérifié, pourquoi le code est ainsi) et appelle `memorize` avec des affirmations structurées et des chemins `evidence`.

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

2. **Ancrer** — m1nd écrit un `.light.md` graph-native sous `<runtime>/agent-memory/`, l'ingère (`adapter=light mode=merge`), et résout chaque chemin `evidence` vers le vrai nœud de code via un edge `grounded_in` — ainsi la connaissance vit dans le même espace d'activation que le code et remonte dans `seek` / `activate` / `impact`.
3. **Auto-chargement** — à chaque démarrage de session future, `m1nd` ingère `agent-memory/` automatiquement et le rapporte dans `session_handshake.agent_memory`. Les résultats passés survivent à un ingest `mode=replace` et sont simplement *là*.
4. **Auto-signalement de la staleness** — `cross_verify(check: ["evidence_freshness"])` re-hash chaque fichier cité et nomme quelles affirmations sont devenues obsolètes parce que leur code a changé — ainsi la mémoire vous dit quand elle ment au lieu de vous tromper.

Cette boucle a été prouvée en direct de bout en bout : `memorize` → edge `grounded_in` → signal de freshness sur fichier modifié → survit à `mode=replace` → boot auto-load. Vous fermez une mission bornée ? Passez `write_light_memory: true` à `mission_close` pour persister ses affirmations vérifiées de la même façon. L'habitude est documentée dans les `instructions` du serveur que chaque client MCP reçoit à l'`initialize` — host-agnostic, aucun plugin client-spécifique requis.

## Le Layer de Trust / Honnêteté

C'est la chose la plus défendable que m1nd fait, et aucun concurrent ne la propose. La doctrine : **la crédibilité vient de l'honnêteté, pas de toujours gagner.**

- **`trust_selftest`** retourne un verdict *avant* tout retrieval : `full_trust`, `needs_ingest`, `wrong_workspace_binding`, `stale_binding_suspected`, ou `degraded_host_tool_surface`. L'agent sait s'il doit continuer, ingérer, rebinder, ou faire un fallback.
- **`agent_runtime_contract`** est présent dans chaque réponse de retrieval, portant un `trust_mode`. Un résultat vide est désambiguïsé — lié au mauvais dépôt vs. genuinement rien là — jamais silencieusement rapporté comme « aucun résultat. »
- **Les tableaux `non_claims`** sont présents sur chaque outil de mission. m1nd dit à l'agent ce qu'il n'a *pas* prouvé.
- **`mission_verify` peut dire non — et le fait, dans du code testé.** Il rejette les preuves uniquement issues du graphe : une affirmation ne peut pas se fermer sans une lecture de fichier, une exécution de test, ou une sonde runtime. Le test s'appelle littéralement `graph_only_evidence_is_not_enough`.
- **`recovery_playbook`** retourne une liste d'étapes déterministe et ordonnée pour réparer le binding.

La preuve de l'engagement est ce qui a été supprimé pour lui : `savings` et `resonate` ont été retirés de la surface annoncée en beta.7 parce qu'un outil qui prétend toujours gagner n'est pas crédible. Aucun concurrent — ni mem0, Zep, Letta, Sourcegraph, ni aucun MCP code-graph — ne propose un layer qui dit à l'agent ce à quoi il ne faut *pas* faire confiance et comment récupérer.

## Couverture Linguistique

Le raisonnement sur le graphe (`impact`, `why`, `predict`, `trace`, `taint_trace`) vaut ce que vaut l'extracteur. m1nd résout à la fois les **edges `calls`** (call graph) et les **`imports` cross-fichier** (résolution de dépendances fichier→fichier) par langage. La matrice ci-dessous a été prouvée en direct dans un seul ingest polyglotte :

| Langage | `calls` | imports cross-fichier |
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
| C# | ✅ | — (les namespaces ne mappent pas 1:1 aux fichiers) |
| Swift | ✅ | — |

Toutes les lignes ✅ sont vérifiées de bout en bout (un import `caller`→`callee` se résout et le caller émet des edges d'appel). Les autres langages tombent sur l'extracteur générique (uniquement `contains`). Les imports non résolvables (paquets externes, gems, stdlib, en-têtes système) sont honnêtement laissés non résolus plutôt que devinés.

## Carte des Capacités

La surface MCP live évolue avec les versions. Utilisez `tools/list` pour le nombre exact d'outils et les noms dans votre build actuelle.

| Domaine | Ce qu'il permet | Outils représentatifs |
|---|---|---|
| Fondation du graphe | ingérer du code, maintenir l'état du graphe, diagnostiquer la continuité de session, renforcer les chemins utiles, et détecter la dérive des poids entre sessions | `trust_selftest`, `session_handshake`, `recovery_playbook`, `ingest`, `health`, `doctor`, `learn`, `warmup`, `drift` |
| Retrieval et orientation | chercher par texte, chemin, intention, structure, ou relation avant les lectures manuelles de fichiers | `audit`, `search`, `glob`, `seek`, `activate`, `why`, `trace` |
| Docs et binding de connaissance | ingérer des docs universels ou du `L1GHT` graph-native, puis lier les concepts au code | `ingest(adapter="universal"\|"light")`, `document_resolve`, `document_provider_health`, `document_bindings`, `document_drift`, `auto_ingest_*` |
| Navigation et continuité | maintenir des routes stateful, des handoffs, des baselines, et la mémoire d'investigation à travers les sessions | `perspective_*`, `trail_*`, `coverage_session`, `boot_memory`, `persist` |
| Mission control et discipline de preuve | maintenir une route bornée, enregistrer des événements, passer de l'orientation sur graphe à la preuve directe, faire un handoff, et fermer avec des gaps explicites | `mission_start`, `mission_event`, `mission_next`, `mission_verify`, `mission_handoff`, `mission_close` |
| Planification et preuve des changements | raisonner sur l'impact, le co-change, les étapes manquantes, les chemins d'échec, et les affirmations structurelles | `impact`, `predict`, `validate_plan`, `missing`, `hypothesize`, `counterfactual`, `differential` |
| Qualité, sécurité et architecture | détecter les patterns, les chemins de taint, les frontières de trust, la duplication, les violations de layer, les flux de types, et les cibles de refactoring | `scan`, `scan_all`, `heuristics_surface`, `antibody_*`, `taint_trace`, `type_trace`, `trust`, `layers`, `layer_inspect`, `twins`, `fingerprint`, `flow_simulate`, `epidemic`, `tremor`, `refactor_plan` |
| Temps, runtime et travail multi-dépôt | inspecter l'historique git, la dérive, les edges de co-change cachés, les overlays runtime, et les références cross-dépôt | `timeline`, `diverge`, `ghost_edges`, `runtime_overlay`, `external_references`, `federate`, `federate_auto` |
| Opérations et monitoring | vérifier l'état du dépôt, vérifier la vérité graphe-vs-disque, exécuter des watches daemon, persister l'état, et faire remonter des alertes durables | `audit`, `cross_verify`, `daemon_*`, `alerts_*`, `panoramic`, `metrics`, `report`, `persist`, `diagram`, `help` |
| Préparation et exécution d'éditions chirurgicales | extraire un contexte connecté compact, prévisualiser les écritures, et appliquer des éditions graph-aware | `surgical_context`, `surgical_context_v2`, `view`, `batch_view`, `edit_preview`, `edit_commit`, `apply`, `apply_batch` |

**Niveaux :** 27 outils essentiels sont annoncés par défaut pour réduire le coût de sélection des outils ; définissez `M1ND_TOOL_TIER=full` pour annoncer la surface complète (100+ outils : RETROBUILDER, perspectives, federation, daemon). Quelques outils (`resonate`, `savings`, `lock_*`) restent appelables par nom mais ne sont pas sur la surface annoncée. Les outils cachés sont toujours appelables via `tools/call` — le niveau contrôle uniquement ce que `tools/list` expose.

## Les Boucles Opérationnelles

Le pack d'agent fait partie du produit, pas de la documentation décorative. m1nd est le plus puissant quand l'agent reçoit la *boucle opérationnelle*, pas seulement un endpoint de graphe. Cinq protocoles nommés sont fournis dans le pack :

- **Démarrage de Session** — `trust_selftest` → `recovery_playbook` si le trust n'est pas complet → `ingest` si nécessaire → `seek`/`audit`.
- **Recherche** — `ingest` → `activate(query)` → `why(source, target)` → `missing(topic)` → `learn(feedback)` → `memorize` tout résultat durable.
- **Modification de Code** — `impact(node)` pour le blast radius → `predict(node)` → `counterfactual(nodes)` → `surgical_context_v2` → `memorize` la décision et le pourquoi.
- **Analyse Approfondie** — `fingerprint`, `diverge`, `ghost_edges`, `taint_trace`, `twins`, `refactor_plan`, `runtime_overlay` (la lentille RETROBUILDER) pour le couplage caché, les chemins de sécurité, les doublons structurels, et la chaleur runtime.
- **Mémoire** — persister les conclusions durables avec `memorize`, portant `confidence` et chemins `evidence`.

Mission Control est de la discipline de preuve, pas une liste de fonctionnalités. `mission_next` retourne exactement un mouvement plus des guardrails `do_not` ; `mission_verify` rejette les affirmations uniquement issues du graphe ; `mission_close` pousse toujours l'agent à persister les connaissances vérifiées et enregistre les gaps et non-claims. En mode `bug_hunt`, MC0 requiert un `direct_sweep` final après les résultats vérifiés avant la fermeture, pour que les agents vérifient l'espace négatif.

**Mise en garde :** `predict` a **uniquement un fallback structurel** tant que `ghost_edges` n'a pas chargé la matrice de co-change git — exécutez `ghost_edges` en premier quand vous avez besoin de la vraie probabilité de co-change.

## Preuves

Chaque ligne est calibrée exactement à ce qui a été mesuré. m1nd ne met pas en avant des chiffres d'économies ou de ROI — c'est le principe.

| Affirmation | Résultat | Source / calibration |
|---|---|---|
| Latence `activate` / `impact` | `activate` sub-µs, `impact` sub-ms | Benchmarks Criterion dans `m1nd-core/benches/` sur un graphe synthétique de 1K nœuds — [méthodologie](https://m1nd.world/wiki/benchmarks.html) ; traiter comme ordre de grandeur. |
| Matrice linguistique | appels + imports cross-fichier pour 10 langages (+ Ruby cross-fichier) | Vérifié de bout en bout dans un seul ingest polyglotte ; tests par langage dans `m1nd-ingest`. Voir [Couverture Linguistique](#couverture-linguistique). |
| Échantillon de validation post-écriture | 12/12 classifiés correctement | Vérification runtime interne. |
| Bug-hunt avec graines | 16/20 au premier round accepté de défauts semés `humanize` (m1nd-trained) ; `m1nd-basic` et direct chacun 8/15 | Preuve produit interne, `public_claim_worthy=false` — pas un benchmark universel. |
| Auto-vérification de la mémoire | prouvée en direct de bout en bout | `memorize` → `grounded_in` → signal de freshness sur fichier modifié → survit à replace → boot auto-load. |

## Limites

`m1nd` complète plutôt que remplace votre LSP, compilateur, test runner, scanners de sécurité et stack d'observabilité. Il est le plus utile avant la recherche, la revue ou une modification, et chaque fois que les docs, l'impact ou la continuité comptent.

Il est **moins utile** quand :

- la recherche exacte de texte répond déjà à la question
- la vérité du compilateur ou du runtime est la seule chose dont vous avez besoin
- la tâche est une action locale banale sur fichier sans incertitude structurelle

**A besoin d'alimentation :** `trust` et `tremor` démarrent avec des priors neutres jusqu'à ce que le feedback `learn` / les données `ghost_edges` s'accumulent, et `predict` a besoin que `ghost_edges` soit chargé avant que son signal de co-change soit significatif. Ils s'améliorent avec l'usage ; ils sont honnêtes sur le fait d'être non informés au démarrage.

## Architecture en Un Coup d'Œil

Trois crates Rust core plus un bridge auxiliaire :

- **`m1nd-mcp`** — le serveur MCP et la surface du runtime opérationnel.
- **`m1nd-core`** — le moteur de graphe : un `WavefrontEngine` faisant du spreading activation, de la plasticité Hebbienne, de l'adjacence CSR, et des ghost edges dérivés de git.
- **`m1nd-ingest`** — adapters d'extraction, de routage et de construction de graphe (code, docs universels, L1GHT).
- **`m1nd-openclaw`** — bridge auxiliaire OpenClaw (lane Unix-socket, versioning indépendant).

Versions actuelles des crates : `m1nd-core`, `m1nd-ingest`, `m1nd-mcp` tous `0.9.0-beta.7`.

<p align="center">
  <img src="../.github/m1nd-architecture-overview-v2.jpeg" alt="Aperçu de l'architecture m1nd" width="960" />
</p>

Pour la federation, les perspectives, RETROBUILDER, la coordination multi-agent, et la référence complète du pack agent et opérateur, voir le [wiki canonique](https://m1nd.world/wiki/), [docs/AGENT-PACKS.md](../docs/AGENT-PACKS.md), et [EXAMPLES.md](../EXAMPLES.md).

## Contribuer

Les contributions sont bienvenues sur les extracteurs et adapters, le tooling MCP/runtime, les benchmarks, la documentation, et les algorithmes de graphe. Voir [CONTRIBUTING.md](../CONTRIBUTING.md).

## Licence

MIT. Voir [LICENSE](../LICENSE).
