🇬🇧 [English](../README.md) | 🇧🇷 [Português](README.pt-BR.md) | 🇪🇸 [Español](README.es.md) | 🇮🇹 [Italiano](README.it.md) | 🇫🇷 [Français](README.fr.md) | 🇩🇪 [Deutsch](README.de.md) | 🇨🇳 [中文](README.zh.md) | 🇯🇵 [日本語](README.ja.md)

<p align="center">
  <img src="../.github/m1nd-logo.svg" alt="m1nd" width="400" />
</p>

<h1 align="center">Intelligence Opérationnelle pour Agents de Coding</h1>

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

**m1nd est une intelligence opérationnelle pour agents de coding — il gouverne la boucle opérationnelle, pas seulement le retrieval.**

<p align="center"><img src="../docs/assets/visuals/01-code-to-graph.png" width="520" alt="Une pile de fichiers épars devient un graphe connecté de ce qui relie quoi" /></p>

> grep trouve du texte. La recherche vectorielle trouve des chunks similaires. `m1nd` donne aux agents un graphe local de ce qui est connecté, ce qui a changé, ce qui casse, ce qui a dérivé, et où reprendre.

Trois choses coexistent ici qu'aucun autre outil ne réunit :

- **Graphe causal du code** — `impact` avant une modification montre le blast radius que vous n'aviez pas lu ; `ghost_edges` fait remonter les fichiers qui changent toujours ensemble mais ne partagent aucun import.
- **Mémoire auto-vérifiante** — `memorize` ancre les résultats à de vrais nœuds de code ; `cross_verify` les signale comme obsolètes quand ce code change.
- **Un layer de trust / recovery** — chaque résultat porte un trust mode ; `trust_selftest` et `recovery_playbook` indiquent à l'agent quand le binding du workspace est incorrect et comment récupérer.

Plus un **runtime d'attention** — `focus` remet à l'agent le working set minimal et borné en budget pour un objectif, avec une queue honnête de ce qu'il a laissé de côté et un signal indiquant si c'est *assez* de contexte pour l'instant.

<p align="center">
  <img src="../.github/m1nd-agent-first-map-v2.jpeg" alt="Boucle agent traditionnelle vs boucle m1nd-grounded" width="960" />
</p>

## Nouveautés de la 1.2.0 — la première release de l'ère OMEGA

La 1.2.0 fait passer la boucle de « récupérer, puis espérer » à **pré-orienter → agir sur des verdicts calibrés → capturer ce que vous avez appris**. Le thème est le même que celui du layer de trust : un *non* honnête vaut mieux qu'une supposition confiante.

- **`north(task)` — pré-orienter en un seul appel.** La nouvelle porte d'entrée compose le trust, le contexte de la tâche (focus nodes + ancres PageRank), la mémoire inter-sessions antérieure, un signal de suffisance, un `next_move`, et `honest_gaps` (ce que m1nd ne sait *pas* encore). `needs_ingest` est une vraie réponse pour un graphe vide. (La composition L1GHT-recall qui replie la mémoire antérieure dans le paquet a atterri sur `main` juste après le tag 1.2.0 — elle n'est pas dans le binaire 1.2.0.)
- **Calibration conforme sur la prédiction.** `calibrate_predict` arme une gate par dépôt ; les verdicts lisent ensuite `act` / `reverify` / `abstain`, où `abstain` signifie *non calibré ou insuffisant* — un signal pour s'arrêter, pas un oui faible. Livré dark : tant que vous ne calibrez pas, les verdicts plafonnent à `reverify`.
- **`trust_envelope` sur `seek`** (livré dark) et un **verdict `closure` sur `why`** — `blocked` signifie que le chemin repose sur un edge non résolu/deviné. **`trust_band: insufficient_evidence`** est désormais distinct d'un risk band : il signifie *aucune preuve*, la réponse honnête de démarrage à froid, pas « risque moyen ».
- **La mémoire a gagné une colonne vertébrale de provenance** — les affirmations portent un âge + un auteur réels, supplantent les affirmations plus anciennes, expirent avec le temps, et respectent un plafond de récence, si bien que la connaissance mémorisée déclare sa propre fraîcheur au lieu de se périmer en silence.
- **Co-change en Jaccard lissé** — `ghost_edges` / `predict` normalisent désormais le couplage au lieu de compter les co-commits bruts (+3 points prouvés par calibration face aux comptes bruts).
- **Version du binaire + empreinte sha** — `--version` affiche `1.2.0 (<sha>)` ; `M1ND_EXPECTED_VERSION` / `M1ND_EXPECTED_SHA` (+ `M1ND_STRICT_VERSION`) permettent à un hôte de détecter et de refuser un binaire qui a dérivé.
- **Instructions MCP agent-natives + field reports local-only.** Les instructions d'`initialize` que chaque hôte reçoit *sont* désormais la boucle opérationnelle ci-dessus. Les agents peuvent laisser un signal de télémétrie par session — `learn` sur un verdict de retrieval, ou une ligne dans `~/.m1nd/field-reports.jsonl` quand m1nd lui-même se comporte mal. Ce fichier est local-only ; **m1nd ne téléphone jamais à la maison.**

## Démarrage Rapide

Le chemin minimal fonctionnel — installer depuis les sources (toujours à jour), vérifier la santé, connecter votre hôte :

```bash
git clone https://github.com/maxkle1nz/m1nd.git && cd m1nd
npm install -g .
m1nd doctor
```

Ensuite, câblez votre hôte — les deux mêmes commandes, une par hôte (`codex`, `claude`, `gemini`, `antigravity`, `generic`) :

| Hôte | Installer le pack d'agent | Câbler la config MCP |
|---|---|---|
| Codex | `m1nd install-skills codex` | `m1nd mcp-config codex --project /your/project` |
| Claude Code | `m1nd install-skills claude --project /your/project` | `m1nd mcp-config claude --project /your/project` |
| Gemini | `m1nd install-skills gemini --project /your/project` | `m1nd mcp-config gemini --project /your/project` |
| Antigravity | `m1nd install-skills antigravity --project /your/project` | `m1nd mcp-config antigravity --project /your/project` |
| Generic | `m1nd install-skills generic --project /your/project` | `m1nd mcp-config generic --project /your/project` |

Ou depuis npm : `npm install -g @maxkle1nz/m1nd`.

Carte d'installation complète, packs d'hôtes, build du runtime natif et flags de mise à jour : [docs/AGENT-PACKS.md](../docs/AGENT-PACKS.md) · configuration client par client : [matrice d'intégration](../docs/IDE-INTEGRATIONS.md).

### Point d'Entrée des Agents

<p align="center"><img src="../docs/assets/visuals/02-north-one-call.png" width="520" alt="north(task) : un seul appel d'entrée renvoie tout le paquet orienté" /></p>

Les agents analysent ce README. Dans une session MCP, la porte d'entrée est un seul appel — `north(task)` compose le trust, le contexte de tâche, la mémoire inter-sessions antérieure, un signal de suffisance, un `next_move`, et `honest_gaps` (ce que m1nd ne sait *pas* encore) en un seul paquet. S'il rapporte `needs_ingest` (graphe vide), ou si vous êtes sur un binaire plus ancien, repliez-vous sur la boucle de trust explicite — établir le trust *avant* de faire confiance à tout retrieval :

```jsonc
// 0. Trust the binding in one call (verdict before retrieval)
{"method":"tools/call","params":{"name":"trust_selftest","arguments":{"agent_id":"dev"}}}

// 1. If the verdict is not full_trust, ask for the deterministic recovery path
{"method":"tools/call","params":{"name":"recovery_playbook","arguments":{"agent_id":"dev"}}}

// 2. Build graph truth
{"method":"tools/call","params":{"name":"ingest","arguments":{"path":"/your/project","agent_id":"dev"}}}

// 3. Ask a structural question — empty results say *why*, never just "no results"
{"method":"tools/call","params":{"name":"activate","arguments":{"query":"authentication flow","agent_id":"dev"}}}
```

**Boucle première session, en quatre mouvements :** `north` (ou `trust_selftest` → `ingest`) → `seek`/`audit` → `memorize` le résultat durable pour que la prochaine session parte en avance.

Quand il n'y a pas de session MCP vivante où appeler `north` — elle est obsolète, liée au mauvais dépôt, ou pas encore chargée — utilisez plutôt la CLI host-neutral comme échappatoire. Elle lance un runtime isolé, le lie au dépôt, et retourne une seule enveloppe lisible par machine qui délimite le périmètre, établit le trust, ingère si nécessaire, retourne des ancres, et fait le handoff vers la preuve directe :

```bash
m1nd agent first-minute --repo /your/project --query "understand this system" --json
```

### Servir un seul graphe, attacher plusieurs agents

<p align="center"><img src="../docs/assets/visuals/10-attach-core.png" width="520" alt="Un processus propriétaire détient le graphe vivant ; de nombreux agents s'attachent au même cœur" /></p>

Le Démarrage Rapide ci-dessus câble un serveur stdio par hôte — parfait pour un seul agent, mais chaque processus charge son propre graphe et détient sa propre lease. Le déploiement pour lequel m1nd est conçu, c'est un seul propriétaire, plusieurs agents attachés. Un seul processus propriétaire détient le graphe vivant :

```bash
m1nd-mcp --serve --no-gui --port 1337 --runtime-dir /your/project/.m1nd
```

Chaque agent s'attache ensuite comme un fin pont stdio↔HTTP — il ne charge **aucun** graphe, ne construit aucun moteur, et ne prend **aucune** lease :

```bash
m1nd-mcp --attach http://127.0.0.1:1337 --stdio    # or set M1ND_ATTACH_URL and omit the flag
```

N'importe quel nombre de ponts pointent vers l'unique propriétaire et partagent son unique graphe vivant, si bien que ce qu'un agent `memorize` est immédiatement rappelé par un autre — pas de réingest, pas de copie par agent. Les requêtes passent par localhost, donc ça reste local-first (le bind reste `127.0.0.1` sauf si vous optez pour `--bind 0.0.0.0`). Un `seek` à chaud via le pont a mesuré ≈0.7ms sur un petit graphe sur une seule machine — ordre de grandeur, pas une garantie : l'attach ajoute un aller-retour localhost, et la latence croît avec la taille du graphe et la charge.

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

<p align="center"><img src="../docs/assets/visuals/06-l1ght-anchored.png" width="520" alt="La mémoire est ancrée au code réel ; quand le code change, la mémoire se signale d'elle-même" /></p>

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

<p align="center"><img src="../docs/assets/visuals/03-verdicts-doors.png" width="520" alt="Chaque résultat est un verdict — act, reverify ou abstain — comme des portes que l'agent choisit" /></p>

C'est la chose la plus défendable que m1nd fait, et aucun concurrent ne la propose. La doctrine : **la crédibilité vient de l'honnêteté, pas de toujours gagner.**

- **`trust_selftest`** retourne un verdict *avant* tout retrieval : `full_trust`, `needs_ingest`, `wrong_workspace_binding`, `stale_binding_suspected`, ou `degraded_host_tool_surface`. L'agent sait s'il doit continuer, ingérer, rebinder, ou faire un fallback.
- **`agent_runtime_contract`** est présent dans chaque réponse de retrieval, portant un `trust_mode`. Un résultat vide est désambiguïsé — lié au mauvais dépôt vs. genuinement rien là — jamais silencieusement rapporté comme « aucun résultat. »
- **Les tableaux `non_claims`** sont présents sur chaque outil de mission. m1nd dit à l'agent ce qu'il n'a *pas* prouvé.
- **`mission_verify` peut dire non — et le fait, dans du code testé.** Il rejette les preuves uniquement issues du graphe : une affirmation ne peut pas se fermer sans une lecture de fichier, une exécution de test, ou une sonde runtime. Le test s'appelle littéralement `graph_only_evidence_is_not_enough`.
- **`recovery_playbook`** retourne une liste d'étapes déterministe et ordonnée pour réparer le binding.

La preuve de l'engagement est ce qui a été supprimé pour lui : `savings` et `resonate` ont été retirés de la surface annoncée en beta.7 parce qu'un outil qui prétend toujours gagner n'est pas crédible. Aucun concurrent — ni mem0, Zep, Letta, Sourcegraph, ni aucun MCP code-graph — ne propose un layer qui dit à l'agent ce à quoi il ne faut *pas* faire confiance et comment récupérer.

**La boucle de field-triage se referme sur elle-même.** La télémétrie de session que les agents laissent dans `~/.m1nd/field-reports.jsonl` (local-only — m1nd ne téléphone jamais à la maison) n'est pas un log passif : les reports sont triés, et un bug de terrain *confirmé* devient un cas de batterie rouge **avant** le fix, si bien que la régression est prouvée, pas seulement décrite. Cette boucle a déjà tourné une fois de bout en bout : deux bugs remontés du terrain sont devenus des cas de batterie en échec puis des fixes mergés — `north` compose désormais le L1GHT recall dans son paquet de mémoire, et le sentinel de graphe `temp` se résout vers un vrai tempdir au lieu de joncher le répertoire de travail.

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

<p align="center"><img src="../docs/assets/visuals/04-impact-web.png" width="520" alt="impact trace le rayon d'impact à travers la toile de code connecté avant que vous n'éditiez" /></p>

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

<p align="center"><img src="../docs/assets/visuals/12-battery-arches.png" width="520" alt="Chaque affirmation repose sur son propre arc prouvé — la batterie de capacités, reproductible" /></p>

Chaque ligne est calibrée exactement à ce qui a été mesuré. m1nd ne met pas en avant des chiffres d'économies ou de ROI — c'est le principe.

| Affirmation | Résultat | Source / calibration |
|---|---|---|
| Latence `activate` / `impact` | `activate` ~1µs, `impact` sub-µs sur un graphe synthétique de 1K nœuds | Benchmarks Criterion — **reproduisez-le vous-même : `cargo bench -p m1nd-core`** (mesuré `activate_1k_nodes` ≈1.4µs, `impact_depth3` ≈0.5µs sur un Mac Apple-silicon) ; [méthodologie](https://m1nd.world/wiki/benchmarks.html) ; ordre de grandeur, dépendant du matériel. |
| Matrice linguistique | appels + imports cross-fichier pour 10 langages (+ Ruby cross-fichier) | Vérifié de bout en bout dans un seul ingest polyglotte ; tests par langage dans `m1nd-ingest`. Voir [Couverture Linguistique](#couverture-linguistique). |
| Échantillon de validation post-écriture | 12/12 classifiés correctement | Vérification runtime interne. |
| Bug-hunt avec graines | 16/20 au premier round accepté de défauts semés `humanize` (m1nd-trained) ; `m1nd-basic` et direct chacun 8/15 | Preuve produit interne, `public_claim_worthy=false` — pas un benchmark universel. |
| Auto-vérification de la mémoire | prouvée en direct de bout en bout | `memorize` → `grounded_in` → signal de freshness sur fichier modifié → survit à replace → boot auto-load. |
| Batterie de capacités vs grep | 37/37 passent ; en face-à-face 16 victoires m1nd / 12 égalités / **0 victoire grep** | Harness in-repo `scratchpad/m1nd_battery.py` (37 cas, ingest frais + vérité-terrain PASS/FAIL + face-à-face `rg`). **Reproduire : `python3 scratchpad/m1nd_battery.py ./target/release/m1nd-mcp . --suite m1nd`.** Calibration : un seul dépôt (m1nd lui-même), cas auto-rédigés ; ~5 des égalités sont des outils structurels notés face à un proxy grep littéral qui ne peut pas exprimer ce à quoi ils répondent. |
| Calibration conforme (`predict`) | act-band ≈32% de précision @ ≈13.5% de couverture (α=0.10) | Sur l'historique git propre de m1nd (n≈9.2k prédictions held-out), +3pts face aux comptes bruts après le passage au Jaccard lissé. Calibration : un seul dépôt, un signal grossier basé sur des comptes — la gate s'abstient surtout aujourd'hui, **par design** : l'abstention est la sortie honnête d'un signal faible, pas un échec. |

<details>
<summary><strong>Plus de visuels — la série complète des mécanismes</strong></summary>
<br/>
<p align="center">
  <img src="../docs/assets/visuals/05-one-graph-fountain.png" width="380" alt="Un graphe partagé alimente chaque agent attaché, comme une fontaine commune" />
  <img src="../docs/assets/visuals/07-supersede-shelf.png" width="380" alt="Le savoir remplacé est mis de côté, pas supprimé — l'affirmation la plus récente prime" />
</p>
<p align="center">
  <img src="../docs/assets/visuals/08-calibration-earned.png" width="380" alt="La calibration se gagne par dépôt avant que les verdicts puissent lire act" />
  <img src="../docs/assets/visuals/09-closure-bridge.png" width="380" alt="Une affirmation ne se ferme que lorsque la preuve fait le pont — une lecture de fichier, un test ou une sonde" />
</p>
<p align="center">
  <img src="../docs/assets/visuals/11-triage-loop.png" width="380" alt="Les rapports de terrain alimentent une boucle de triage qui transforme le défaut en test avant le correctif" />
</p>
</details>

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

Versions actuelles des crates : `m1nd-core`, `m1nd-ingest`, `m1nd-mcp` tous en `1.2.0` (`m1nd-openclaw` est versionné indépendamment en `0.1.0`).

<p align="center">
  <img src="../.github/m1nd-architecture-overview-v2.jpeg" alt="Aperçu de l'architecture m1nd" width="960" />
</p>

Pour la federation, les perspectives, RETROBUILDER, la coordination multi-agent, et la référence complète du pack agent et opérateur, voir le [wiki canonique](https://m1nd.world/wiki/), [docs/AGENT-PACKS.md](../docs/AGENT-PACKS.md), et [EXAMPLES.md](../EXAMPLES.md).

## Contribuer

Les contributions sont bienvenues sur les extracteurs et adapters, le tooling MCP/runtime, les benchmarks, la documentation, et les algorithmes de graphe. Voir [CONTRIBUTING.md](../CONTRIBUTING.md).

## Licence

MIT. Voir [LICENSE](../LICENSE).
