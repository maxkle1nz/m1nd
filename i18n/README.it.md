<p align="center">
  <img src=".github/m1nd-logo.svg" alt="m1nd" width="340" />
</p>

**m1nd** dà al tuo agente di codifica un cervello per repository: un grafico locale del codice servito tramite MCP, memoria ancorata al codice citato e un verdetto di fiducia per ogni risposta. "Prove insufficienti" è una vera risposta qui. Così come "non fidarti ancora di questo, e ecco come risolverlo".

Nulla esce dalla tua macchina. Un unico binario Rust. MIT.

Pensalo come un radiografia del tuo repository che l'agente può leggere: una struttura che combina tutto e dice dove vive ogni cosa, a cosa serve quel programma, su cosa si sta lavorando, cosa è completato e cosa è ancora aperto. Questo panorama è qualcosa che nessun altro strumento offre al tuo agente.

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-mcp"><img src="https://img.shields.io/crates/v/m1nd-mcp.svg?label=crates.io" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://registry.modelcontextprotocol.io/?search=io.github.maxkle1nz/m1nd"><img src="https://img.shields.io/badge/MCP_Registry-official-6d28d9" alt="MCP Registry" /></a>
</p>

<p align="center">Quattro comandi per installare: <a href="#sixty-seconds">Sixty seconds</a>. Ragioni per chiudere prima questa scheda: <a href="#when-not-to-use-m1nd">When not to use m1nd</a>.</p>

<p align="center">
  <img src="docs/assets/demo.gif" width="760" alt="A real m1nd session: north returns trust, focus and honest gaps; seek answers with a reverify verdict; memorize anchors the finding to code" />
</p>

<p align="center"><em>Una sessione reale sul grafico di 6.453 nodi di questo repository (m1nd-mcp 1.4.0): <code>north</code> orienta, <code>seek</code> risponde con un verdetto <code>reverify</code>, <code>memorize</code> ancora il risultato al codice.</em></p>

## L'audit che il tuo agente smette di pagare

Conosci il rituale. L'agente apre un file, usa grep, apre un altro file, usa grep di nuovo, brucia la maggior parte del suo contesto ricostruendo cosa sia il repository, e solo allora inizia il compito vero e proprio. Con m1nd quella scansione diventa una sola domanda. In meno di un secondo l'agente ha la mappa: cosa chiama cosa, cosa rompe cosa, dove si trova tutto. Non un mucchio di corrispondenze da interpretare. La struttura connessa, già assemblata.

E ricorda. Tra sessioni, e tra agenti. Quello che un agente impara stasera, un altro agente eredita domani, con le prove allegate e un avviso se il codice è cambiato nel frattempo. Ogni conclusione lascia una traccia, così che tu o qualsiasi agente che venga dopo possa sempre vedere cosa è successo a quel codice e perché.

Poi l1ght va oltre: articoli, note, RFC e bozze si collegano alle parti del tuo codice che spiegano, all'interno della stessa struttura. L'agente ottiene il contesto GIUSTO invece di quello più vicino per suono, e inventare codice che non esiste smette di essere la strada di minor resistenza: la struttura dice cosa esiste, e il verdetto dice quanto fidarsi persino di quello.

Prima di m1nd, una funzione era solo una funzione, persa in qualche manuale. Ora vive all'interno dell'intelligenza dell'agente, combinata con il codice, la sua storia, i suoi documenti e i suoi rischi. Non ho trovato nulla di simile altrove.

## grep risponde a buone domande. m1nd risponde a quelle più profonde.

Domande che il tuo agente ora può fare e ottenere una risposta strutturale:

- Cosa si rompe se modifico questa funzione?
- Dove avviene realmente il refresh del token in questo repository?
- Perché questi due file sono collegati, e quel percorso è solido o una supposizione?
- Cosa ha imparato l'ultima sessione su questo codice, ed è ancora vero?
- Cosa cambia sempre insieme qui, anche senza un'importazione tra loro?
- Questa modifica attraversa un confine architetturale che non dovrei attraversare?
- Quale affermazione in questo articolo è implementata da questa funzione?
- Il bug che ho appena fissato si nasconde altrove, come schema?
- Cosa manca qui che di solito è presente in questo pattern?
- Sono nel repository giusto?
- Devo agire su questa risposta o verificarla prima?

Ognuna è un verbo sulla superficie MCP (`impact`, `seek`, `why`, `north`, `ghost_edges`, `xray_gate`, `antibody_scan`, `missing`, `trust_selftest`, `predict`), non un trucco con prompt.

## E non si ferma al mostrare la struttura

Antibody: un bug risolto diventa un pattern strutturale nominato, e ogni successiva sessione cerca quella configurazione in tutto il repository. Risolvilo una volta, cercalo per sempre.

Ghost edges: file che cambiano sempre insieme senza una connessione di importazione, estratti dalla cronologia git. L'accoppiamento invisibile che ostacola i refactor.

Lacune strutturali: `missing` cerca il codice che non c'è. La guardia, il retry, il timeout che questo pattern di solito include e che manca in questo caso.

Ipotesi contro il grafico: esprimi una pretesa in linguaggio semplice ("le impostazioni possono raggiungere l'avvio senza convalida") e falla testare contro la struttura attuale.

Tremor: file la cui velocità di cambiamento è in accelerazione vengono segnalati prima ancora che qualcuno segnali il problema.

Un grafico caldo: i risultati confermati rafforzano i loro legami, stile Hebbiano, così che i percorsi che si sono dimostrati utili ottengono priorità maggiore per il prossimo agente.

Ognuno di questi segnali e suggerisce, il tuo compilatore e i tuoi test fanno ancora la prova.

## m1nd non solo cerca, ma scrive.

Ecco la parte che le persone impiegano un momento a credere. Il grafico che legge il tuo repository può anche operare su di esso...
