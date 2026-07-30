<p align="center">
  <img src=".github/m1nd-logo.svg" alt="m1nd" width="340" />
</p>

**m1nd** donne à votre agent de codage un cerveau par dépôt : un graphe de code local servi via MCP, une mémoire ancrée au code qu’elle cite, et un verdict de confiance pour chaque réponse. "Preuves insuffisantes" est une réponse valide ici. Tout comme "ne faites pas confiance à cela pour l'instant, voici comment y remédier".

Rien ne quitte votre machine. Un seul binaire Rust. Licence MIT.

Considérez cela comme une radiographie de votre dépôt que votre agent peut lire : une structure combinant tout, indiquant où se trouve chaque élément, à quoi sert tel ou tel programme, ce sur quoi on travaille, ce qui est terminé et ce qui reste à faire. Cette vue d’ensemble est une chose qu’aucun autre outil ne met à disposition de votre agent.

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-mcp"><img src="https://img.shields.io/crates/v/m1nd-mcp.svg?label=crates.io" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://registry.modelcontextprotocol.io/?search=io.github.maxkle1nz/m1nd"><img src="https://img.shields.io/badge/MCP_Registry-official-6d28d9" alt="MCP Registry" /></a>
</p>

<p align="center">Quatre commandes pour installer : <a href="#sixty-seconds">Soixante secondes</a>. Raisons de fermer cet onglet pour l'instant : <a href="#when-not-to-use-m1nd">Quand ne pas utiliser m1nd</a>.</p>

<p align="center">
  <img src="docs/assets/demo.gif" width="760" alt="Une session m1nd réelle : north retourne la confiance, la concentration et les lacunes honnêtes ; seek fournit des réponses avec un verdict de re-vérification ; memorize ancre la découverte au code" />
</p>

<p align="center"><em>Une session réelle sur le graphe de 6 453 nœuds de ce dépôt (m1nd-mcp 1.4.0) : <code>north</code> s'oriente, <code>seek</code> répond avec un verdict <code>reverify</code>, <code>memorize</code> ancre la découverte au code.</em></p>

## L'audit que votre agent cesse de payer

Vous connaissez le rituel : l’agent ouvre un fichier, fait un grep, en ouvre un autre, fait un autre grep, brûle l’essentiel de son contexte à reconstruire ce que le dépôt contient réellement, et commence seulement après à accomplir la tâche. Avec m1nd, ce balayage devient une seule question. En moins d’une seconde, l’agent a la carte : ce qui appelle quoi, ce qui bloque quoi, où tout se trouve. Pas un tas de correspondances à interpréter, mais une structure connectée, déjà assemblée.

Et il se souvient. Entre les sessions, et entre les agents. Ce qu’un agent apprend ce soir, un autre agent l’hérite demain, avec les preuves attachées et un drapeau si le code a entre-temps évolué. Chaque conclusion laisse une trace, pour que vous puissiez, ou n'importe quel agent qui suit, toujours voir ce qui est arrivé à ce code et pourquoi.

Ensuite, l1ght va plus loin : articles, RFC, brouillons et notes se connectent aux parties de votre code qu’ils expliquent, au sein de la même structure. L’agent obtient le contexte ADÉQUAT plutôt que le premier qui semble correspondant, et l’invention de code inexistant cesse d’être la solution de facilité : la structure montre ce qui existe, et le verdict indique à quel point on peut avoir confiance.

Avant m1nd, une fonction n'était qu'une fonction, perdue dans un manuel quelconque. Maintenant, elle vit dans l'intelligence de l'agent, combinée avec le code, son historique, ses documents et ses risques. Je n’ai rien trouvé de semblable ailleurs.

## grep répond aux bonnes questions. m1nd répond aux questions plus profondes.

Des questions que votre agent peut désormais poser et pour lesquelles il peut obtenir une réponse structurée :

- Qu'est-ce qui casse si je modifie cette fonction ?
- Où le rafraîchissement de jeton a-t-il réellement lieu dans ce dépôt ?
- Pourquoi ces deux fichiers sont-ils connectés, ce chemin est-il solide ou bien une hypothèse ?
- Qu'a appris la dernière session sur ce code, est-ce toujours pertinent ?
- Qu'est-ce qui change toujours ensemble ici, même sans import entre eux ?
- Cette modification traverse-t-elle une frontière architecturale que je ne devrais pas traverser ?
- Quelle déclaration dans cet article cette fonction implémente-t-elle ?
- Le bug que je viens de corriger se cache-t-il ailleurs, sous une forme différente ?
- Qu'est-ce qui manque ici et que ce modèle inclut habituellement ?
- Suis-je dans le bon dépôt ?
- Dois-je agir sur cette réponse ou la vérifier avant ?

Chacune de ces questions est un verbe sur la surface MCP (`impact`, `seek`, `why`, `north`, `ghost_edges`, `xray_gate`, `antibody_scan`, `missing`, `trust_selftest`, `predict`), et non une manipulation de prompt.

## Et il ne s'arrête pas à révéler la structure

Anticorps : un bug corrigé devient un modèle structurel nommé, et chaque session postérieure analyse la présence de cette forme dans tout le dépôt. Corrigez-le une fois, traquez-le à jamais.

Liens fantômes : des fichiers qui changent toujours ensemble sans lien d’import entre eux, extraits de votre historique git. Le couplage invisible qui rend les refactorisations hasardeuses.

Trous structurels : `missing` cherche le code absent : la protection, la ré-essaye, le délai d'attente que ce modèle inclut généralement mais qui manque ici.

Hypothèses contre le graphe : énoncez une déclaration en langage simple ("les paramètres peuvent atteindre l’amorçage sans validation") et testez-la par rapport à la structure en temps réel.

Tremor : les fichiers dont la modification s’accélère sont signalés avant qu’un rapport de bug ne soit déposé.

Un graphe "chaud" : les résultats confirmés renforcent leurs relations, à la manière de la plasticité hebbienne, de sorte que les chemins qui se sont avérés utiles soient priorisés pour l'agent suivant.

Chacun de ces éléments signale et suggère, cependant ce sont toujours vos compilateurs et vos tests qui réalisent les validations.

## m1nd ne fait pas que chercher. Il écrit.

Voici l’aspect que les gens ont du mal à croire tout de suite : le graphe qui lit votre dépôt peut aussi y opérer. Votre agent nomme un symbole et une destination, environ 48 tokens, et `transplant` calcule tout le déplacement à partir du graphe : la région élargie (les commentaires de documentation et attributs voyagent avec), les dépendances réparties selon leurs arcs d’appel (les privées voyagent, les partagées restent et reçoivent un retour d'import), chaque référenceur est requalifié selon chaque fichier qui le nomme. Ensuite, il effectue une opération atomique, réabsorbe les modifications et fournit un reçu honnête : ce qui a bougé, ce qui est resté, et ce qu'il n'a pas pu résoudre. `refs_unresolved` n’est jamais silencieusement vide quand quelque chose cloche.

Cela fonctionne en deux phases : `transplant_preview` avant `transplant_commit`, et la validation re-vérifie le hash de chaque fichier qu'il prévoyait de toucher, de sorte que rien ne soit appliqué à un dépôt ayant changé entre-temps.
