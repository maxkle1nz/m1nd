🇬🇧 [English](../README.md) | 🇧🇷 [Português](README.pt-BR.md) | 🇪🇸 [Español](README.es.md) | 🇮🇹 [Italiano](README.it.md) | 🇫🇷 [Français](README.fr.md) | 🇩🇪 [Deutsch](README.de.md) | 🇨🇳 [中文](README.zh.md) | 🇯🇵 [日本語](README.ja.md)

<p align="center">
  <img src="../.github/m1nd-logo.svg" alt="m1nd" width="340" />
</p>

**m1nd** le da a tu coding agent un brain por repositorio: un grafo de código local servido sobre MCP, una memoria anclada al código que cita y un veredicto de confianza en cada respuesta. "Evidencia insuficiente" es una respuesta válida aquí. También lo es "no confíes en esto todavía, y aquí está cómo repararlo".

Nada sale de tu máquina. Un binario en Rust. MIT.

Piénsalo como una radiografía de tu repositorio que tu agente puede leer: una estructura que combina todo y dice dónde vive cada cosa, para qué sirve ese programa, en qué se está trabajando, qué está hecho y qué sigue pendiente. Ese panorama es lo que ninguna otra herramienta le entrega a tu agente.

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-mcp"><img src="https://img.shields.io/crates/v/m1nd-mcp.svg?label=crates.io" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://registry.modelcontextprotocol.io/?search=io.github.maxkle1nz/m1nd"><img src="https://img.shields.io/badge/MCP_Registry-official-6d28d9" alt="MCP Registry" /></a>
</p>

<p align="center">Cuatro comandos para instalar: <a href="#sixty-seconds">Sesenta segundos</a>. Razones para cerrar esta pestaña primero: <a href="#when-not-to-use-m1nd">Cuándo no usar m1nd</a>.</p>

<p align="center">
  <img src="../docs/assets/demo.gif" width="760" alt="Una sesión real de m1nd: north devuelve confianza, enfoque y brechas honestas; seek responde con un veredicto de verificar de nuevo; memorize ancla el hallazgo al código" />
</p>

<p align="center"><em>Una sesión real en el grafo de 6,453 nodos de este repositorio (m1nd-mcp 1.4.0): <code>north</code> orienta, <code>seek</code> responde con un veredicto de <code>reverify</code>, <code>memorize</code> ancla el hallazgo al código.</em></p>

## La auditoría que tu agente deja de pagar

Conoces el ritual. El agente abre un archivo, busca con grep, abre otro archivo, busca de nuevo, quema la mayor parte de su contexto tratando de reconstruir qué es el repositorio y solo entonces comienza la tarea real. Con m1nd ese barrido se convierte en una pregunta. En menos de un segundo, el agente tiene el mapa: qué llama a qué, qué rompe a qué, dónde vive todo. No una pila de coincidencias para interpretar. La estructura conectada, ya ensamblada.

Y lo recuerda. Entre sesiones y entre agentes. Lo que un agente aprende esta noche, otro agente lo hereda mañana, con la evidencia adjunta y una bandera si el código ha cambiado desde entonces. Cada conclusión deja un rastro, por lo que tú, o cualquier agente que venga después, siempre puede ver qué pasó con ese código y por qué.

Luego l1ght lo lleva más lejos: artículos, RFCs, borradores y notas se conectan a las partes de tu código que explican, dentro de la misma estructura. El agente obtiene el contexto CORRECTO en lugar del que suena más cercano, y dejar de inventar código que no existe se convierte en el camino menos viable: la estructura dice qué existe, y el veredicto dice cuánto confiar incluso en eso.

Antes de m1nd, una función era solo una función, perdida en algún manual. Ahora vive como parte de la inteligencia del agente, combinada con el código, su historial, sus documentos y sus riesgos. No he encontrado nada como eso en ningún otro lugar.

## grep responde buenas preguntas. m1nd responde las más profundas.

Preguntas que tu agente ahora puede hacer y obtener una respuesta estructural:

- ¿Qué se rompe si toco esta función?
- ¿Dónde ocurre realmente la actualización de tokens en este repositorio?
- ¿Por qué están conectados estos dos archivos, y ese camino es sólido o una conjetura?
- ¿Qué aprendió la última sesión sobre este código y sigue siendo verdad?
- ¿Qué siempre cambia junto aquí, incluso sin una importación entre ellos?
- ¿Esta edición cruza un límite arquitectónico que no debería cruzar?
- ¿Qué afirmación de este artículo implementa esta función?
- ¿El error que acabo de corregir se está ocultando en otro lugar como una forma?
- ¿Qué falta aquí que este patrón generalmente tiene?
- ¿Estoy siquiera en el repositorio correcto?
- ¿Debería actuar sobre esta respuesta o verificarla primero?

Cada una es un verbo en la superficie de MCP (`impact`, `seek`, `why`, `north`, `ghost_edges`, `xray_gate`, `antibody_scan`, `missing`, `trust_selftest`, `predict`), no un truco de redacción.

## Y no se detiene en mostrar estructura

Anticuerpos: un error corregido se convierte en un patrón estructural nombrado y cada sesión posterior busca esa forma en todo el repositorio. Arréglalo una vez, encuéntralo para siempre.

Conexiones ocultas: archivos que siempre cambian juntos sin una importación entre ellos, extraídas de tu historial de git. Las asociaciones invisibles que rompen los refactorizados.

Huecos estructurales: `missing` busca el código que no está. La protección, el reintento, el tiempo de espera que este patrón generalmente conlleva y que esta instancia no tiene.

Hipótesis contra el grafo: plantea una afirmación en lenguaje natural ("settings puede llegar a boot sin validación") y haz que se pruebe contra la estructura en vivo.

Temblores: los archivos cuyo ritmo de cambio se está acelerando se marcan antes de que alguien lo señale como un problema.

Un grafo cálido: los resultados confirmados refuerzan sus conexiones, estilo hebbiano, para que los caminos que probaron ser útiles se clasifiquen más alto para el próximo agente.

Cada una de esas funciones sugiere y señala; tu compilador y pruebas aún hacen la validación.
