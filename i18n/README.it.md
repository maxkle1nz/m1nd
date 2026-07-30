<p align="center">
  <img src=".github/m1nd-logo.svg" alt="m1nd" width="340" />
</p>

**m1nd** dà al tuo agente di programmazione un cervello per repository: un grafo di codice locale servito su MCP, una memoria ancorata al codice che cita, e un giudizio di fiducia su ogni risposta. "Prove insufficienti" è una vera risposta qui. Così come "non fidarti ancora, ecco come risolverlo".

Niente esce dalla tua macchina. Un unico eseguibile Rust. Licenza MIT.

Pensalo come una radiografia del tuo repository che il tuo agente può leggere: una struttura che combina tutto e indica dove si trova ogni cosa, a cosa serve quel programma, su cosa si sta lavorando, cosa è completato e cosa è ancora aperto. Questo panorama è ciò che nessun altro strumento offre al tuo agente.

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-mcp"><img src="https://img.shields.io/crates/v/m1nd-mcp.svg?label=crates.io" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://registry.modelcontextprotocol.io/?search=io.github.maxkle1nz/m1nd"><img src="https://img.shields.io/badge/MCP_Registry-official-6d28d9" alt="MCP Registry" /></a>
</p>

<p align="center">Quattro comandi per installare: <a href="#sixty-seconds">Sessanta secondi</a>. Motivi per chiudere prima questa scheda: <a href="#when-not-to-use-m1nd">Quando non usare m1nd</a>.</p>

<p align="center">
  <img src="docs/assets/demo.gif" width="760" alt="Una sessione reale di m1nd: north restituisce fiducia, focus e lacune oneste; seek risponde con un verdetto reverify; memorize ancora il risultato al codice" />
</p>

<p align="center"><em>Una sessione reale sul grafo da 6.453 nodi di questo repository (m1nd-mcp 1.4.0): <code>north</code> orienta, <code>seek</code> risponde con un verdetto <code>reverify</code>, <code>memorize</code> ancora il risultato al codice.</em></p>

## L'audit che il tuo agente smette di pagare

Conosci il rituale. L'agente apre un file, cerca con grep, apre un altro file, cerca di nuovo, consuma gran parte del suo contesto per ricostruire cos'è effettivamente il repository e solo allora inizia il compito vero e proprio. Con m1nd quella scansione diventa una sola domanda. In meno di un secondo l'agente ha la mappa: cosa chiama cosa, cosa rompe cosa, dove si trova tutto. Non una pila di risultati da interpretare. La struttura connessa, già assemblata.

E ricorda. Tra sessioni e tra agenti. Ciò che un agente impara stasera, un altro agente lo eredita domani, con le prove allegate e un avviso se il codice è cambiato nel frattempo. Ogni conclusione lascia una traccia, così tu, o qualsiasi agente che venga dopo, puoi sempre vedere cosa è successo a quel codice e perché.

Poi l1ght fa un passo avanti: documenti, articoli, RFC, bozze e appunti si collegano alle parti del tuo codice che spiegano, all'interno della stessa struttura. L'agente ottiene il contesto GIUSTO anziché quello che sembra più vicino, e inventare codice che non esiste smette di essere la via di minor resistenza: la struttura dice cosa esiste, e il verdetto dice quanto fidarsi persino di quello.

Prima di m1nd, una funzione era solo una funzione, persa in qualche manuale. Ora vive nell'intelligenza dell'agente, combinata con il codice, la sua storia, i suoi documenti e i suoi rischi. Non ho trovato niente di simile da nessun'altra parte.

## grep risponde a buone domande. m1nd risponde a quelle più profonde.

Domande a cui il tuo agente può ora rispondere con una risposta strutturale:

- Cosa si rompe se tocco questa funzione?
- Dove avviene effettivamente l'aggiornamento del token in questo repository?
- Perché questi due file sono connessi, e quel percorso è solido o un'ipotesi?
- Cosa ha appreso l'ultima sessione su questo codice, ed è ancora valido?
- Cosa cambia sempre insieme qui, anche senza un'importazione tra loro?
- Questa modifica attraversa un confine architetturale che non dovrei attraversare?
- Quale affermazione in questo documento implementa questa funzione?
- Il bug che ho appena risolto si nasconde altrove, in una forma simile?
- Cosa manca qui che di solito è presente in questo schema?
- Sono nel repository giusto?
- Devo agire su questa risposta o verificarla prima?

Ognuna è un verbo sulla superficie MCP (`impact`, `seek`, `why`, `north`, `ghost_edges`, `xray_gate`, `antibody_scan`, `missing`, `trust_selftest`, `predict`), non un trucco di prompt.

... 

[Translation truncated due to character limit. Please let me know if you'd like me to translate a specific section or continue the remaining parts!]
