```markdown
<p align="center">
  <img src=".github/m1nd-logo.svg" alt="m1nd" width="340" />
</p>

**m1nd** brinda a tu agente de codificación un cerebro por repositorio: un gráfico de código local servido a través de MCP, memoria anclada al código que cita, y un veredicto de confianza en cada respuesta. "Pruebas insuficientes" es una respuesta válida aquí. También lo es "todavía no confíes en esto, y aquí está cómo repararlo".

Nada sale de tu máquina. Un binario de Rust. MIT.

Piensa en ello como una radiografía de tu repositorio que tu agente puede leer: una estructura que combina todo y dice dónde vive cada cosa, para qué sirve ese programa, qué se está trabajando, qué está terminado y qué sigue abierto. Ese panorama es lo que ninguna otra herramienta ofrece a tu agente.

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-mcp"><img src="https://img.shields.io/crates/v/m1nd-mcp.svg?label=crates.io" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://registry.modelcontextprotocol.io/?search=io.github.maxkle1nz/m1nd"><img src="https://img.shields.io/badge/MCP_Registry-official-6d28d9" alt="MCP Registry" /></a>
</p>

<p align="center">Cuatro comandos para instalar: <a href="#sixty-seconds">Sesenta segundos</a>. Razones para cerrar esta página primero: <a href="#when-not-to-use-m1nd">Cuándo no usar m1nd</a>.</p>

<p align="center">
  <img src="docs/assets/demo.gif" width="760" alt="Una sesión real de m1nd: north devuelve confianza, enfoque y lagunas honestas; seek responde con un veredicto de reverify; memorize ancla el hallazgo al código" />
</p>

<p align="center"><em>Una sesión real en el gráfico de 6,453 nodos de este repositorio (m1nd-mcp 1.4.0): <code>north</code> orienta, <code>seek</code> responde con un veredicto de <code>reverify</code>, <code>memorize</code> ancla el hallazgo al código.</em></p>

## La auditoría que tu agente deja de pagar

Conoces el ritual. El agente abre un archivo, busca con grep, abre otro archivo, busca nuevamente, consume la mayor parte de su contexto reconstruyendo lo que es el repositorio, y solo entonces comienza la tarea real. Con m1nd ese barrido se convierte en una sola pregunta. En menos de un segundo el agente tiene el mapa: qué llama a qué, qué rompe qué, dónde vive todo. No es un montón de coincidencias para interpretar. Es la estructura conectada, ya ensamblada.

Y recuerda. Entre sesiones y entre agentes. Lo que un agente aprende hoy, otro agente lo hereda mañana, con las pruebas adjuntas y una bandera si el código cambió desde entonces. Cada conclusión deja un rastro, para que tú, o cualquier agente que venga después, siempre puedas ver qué pasó con ese código y por qué.

Luego l1ght lo lleva más allá: artículos, documentos, RFCs, borradores y notas se conectan a las partes del código que explican, dentro de la misma estructura. El agente obtiene el contexto CORRECTO en lugar del que más se parece y el inventar código que no existe deja de ser el camino de menor resistencia: la estructura dice lo que existe y el veredicto dice cuánto confiar incluso en eso.

Antes de m1nd, una función era solo una función, perdida en algún manual. Ahora vive dentro de la inteligencia del agente, combinada con el código, su historia, sus documentos y sus riesgos. No he encontrado nada similar en ningún otro lugar.

## grep responde buenas preguntas. m1nd responde las más profundas.

Preguntas que tu agente ahora puede hacer y obtener una respuesta estructural:

- ¿Qué se rompe si toco esta función?
- ¿Dónde ocurre realmente la actualización del token en este repositorio?
- ¿Por qué están conectados estos dos archivos, y ese camino es sólido o una suposición?
- ¿Qué aprendió la última sesión sobre este código, y todavía es cierto?
- ¿Qué siempre cambia junto aquí, incluso sin una importación entre ellos?
- ¿Este cambio cruza un límite de arquitectura que no debo cruzar?
- ¿Qué afirmación de este documento implementa esta función?
- ¿El error que acabo de corregir está escondido en otro lugar, como una forma?
- ¿Qué falta aquí que este patrón normalmente tiene?
- ¿Estoy siquiera en el repositorio correcto?
- ¿Debo actuar según esta respuesta o verificarla primero?

Cada una es un verbo en la superficie de MCP (`impact`, `seek`, `why`, `north`, `ghost_edges`, `xray_gate`, `antibody_scan`, `missing`, `trust_selftest`, `predict`), no un truco de indicaciones.

## Y no se detiene en mostrar estructuras

Anticuerpos: un error corregido se convierte en un patrón estructural nombrado y cada sesión posterior busca esa forma en todo el repositorio. Corrígelo una vez, cázalo para siempre.

Ghost edges: archivos que siempre cambian juntos sin importar entre ellos, extraídos de tu historial git. El acoplamiento invisible que rompe refactorizaciones.

Huecos estructurales: `missing` busca el código que no está ahí. La protección, el reintento, el tiempo de espera que normalmente lleva este patrón y que falta en esta instancia.

Hipótesis contra el gráfico: enuncia una afirmación en lenguaje sencillo ("los ajustes pueden llegar al inicio sin validación") y haz que se pruebe contra la estructura activa.

Temblores: archivos cuyo cambio está acelerándose son marcados antes de que alguien registre el informe de error.

Un gráfico cálido: los resultados confirmados refuerzan sus bordes, al estilo hebbiano, para que los caminos que demostraron ser útiles tengan mayor peso para el próximo agente.

Cada uno de estos marca y sugiere; tu compilador y las pruebas aún hacen la demostración.

## m1nd no solo busca. Escribe.

Aquí está la parte que la gente tarda un momento en creer. El gráfico que lee tu repositorio también puede operar sobre él. Tu agente nombra un símbolo y un destino, alrededor de 48 tokens, y `transplant` calcula todo el movimiento desde el gráfico: la región ampliada (los comentarios de documentación y atributos viajan junto), las dependencias clasificadas por sus bordes de llamada (las privadas viajan, las compartidas se quedan y ganan una importación hacia atrás), cada referencia re-clasificada en cada archivo que las nombra. Luego escribe de forma atómica, re-ingiere y devuelve un recibo honesto: qué se movió, qué quedó, qué no pudo resolver. `refs_unresolved` nunca está vacío silenciosamente si algo salió mal.

Es de dos fases, `transplant_preview` antes de `transplant_commit`, y el commit re-valida el hash de cada archivo que planeó tocar, para que nada se asiente en un repositorio que cambió por debajo de él. La zona económica de tu repositorio (backend, esquema, pagos, CI) está protegida del lado del servidor y falla cerrada. Una negativa nunca toca ni un byte y enseña el reintento: una colisión nombra al ocupante, una ruta de módulo inválida se nombra a sí misma, un movimiento entre crates nombra ambas raíces del crate.

Medido en el caso real: la edición de archivo completo costó 12,235 tokens de salida; el transplante costó 48 tokens de entrada y escribió 3 archivos en 1.3 segundos, con el crate compilando del otro lado. Desde 2019, rust-analyzer tiene un problema abierto solicitando movimientos entre archivos.

Límites de v1, declarados claramente: solo Rust, solo `fn` de nivel superior, mismo crate, el archivo de destino debe existir, y las referencias nacidas dentro de macros son invisibles para él. Cada límite es deliberado y está documentado en [docs/TRANSPLANT-PRD.md](docs/TRANSPLANT-PRD.md), junto a 13 archivos de prueba que mantienen el verbo en su lugar.

## ¿Qué pasa cuando no es un agente sino cinco?

Corre varios agentes en el mismo repositorio y el gráfico se convierte en el lugar donde coordinan. Cada sesión se registra como una presencia y, cuando dos de ellos están a punto de trabajar en áreas superpuestas, ambos reciben una advertencia en su próximo paquete de orientación, antes de que cualquiera realice un cambio. El sistema advierte; tú decides.

El trabajo delimitado se ejecuta como misiones, y las misiones responden por sí mismas de una manera que la mayoría de los equipos humanos omiten: cada herramienta de misión informa `non_claims`, la lista de lo que NO fue probado. Una afirmación no puede cerrarse solo con evidencia del gráfico. Requiere una lectura de archivo, una prueba realizada o una sonda en tiempo de ejecución, y la prueba que refuerza esto se llama `graph_only_evidence_is_not_enough`.

Y las defensas no lanzan falsas alarmas. `xray_gate` solo puede decir `blocked` desde un manifiesto de límites ratificado por un humano. Todo lo demás llega como una advertencia con una razón, para que el agente nunca aprenda a ignorar su propia barra de seguridad.

Cada cerebro también tiene un buzón. Un agente que encuentra un defecto real fuera de su propia misión no lo arregla de inmediato y tampoco lo ignora: deja una carta en el buzón de ese repositorio, en disco, junto al código. El próximo agente que trabaje en ese cerebro revisa el buzón y comienza ya sabiendo los defectos que otros agentes encontraron, con contexto adjunto. El conocimiento sobre lo que está roto deja de morir en el desplazamiento de chat. La revisión es un gesto deliberado (CLI o REST, nunca dentro del bucle de consulta), para que las cartas informen el trabajo en lugar de interrumpirlo.

## Nacido primero para el agente

Sin cuentas, sin telemetría y sin API en el camino, lo que también explica por qué el gráfico responde en microsegundos.

El desarrollo de m1nd tampoco es muy normal. Construirlo significó construir todo un flujo de trabajo donde los agentes dirigen, verifican y prueban el trabajo, y la lógica del producto está dirigida al dolor del agente, no al panel del humano. Cuando m1nd se comporta mal en el campo, los agentes que lo usan presentan el informe, y un error confirmado se convierte en una prueba roja antes de que se implemente la corrección. Muy pocos programas comienzan con eso en su diseño inicial. Así que m1nd nace diferente: los verbos, las negativas y los paquetes están diseñados para el lector que realmente los utiliza, y ni siquiera tienes que recordarle al modelo que la herramienta existe. `m1nd hosts apply` instala ganchos de sesión (`SessionStart`, `agentSpawn`, `TaskStart`, por host) que inyectan la orientación al generarse: tu agente, y cada subagente que genera, comienza orientado antes de que alguien escriba una palabra.

Un cerebro por repositorio lo mantiene unido: un gráfico, su propia memoria, su propia persistencia, vinculado a una raíz de repositorio. Un propietario servido aloja muchos cerebros y dirige cada sesión al adecuado; una sesión de un repositorio que no aloja recibe una negativa tipada en lugar de respuestas incorrectas.

## Lo que tu agente obtiene

m1nd envuelve todo el ciclo de tu agente alrededor de un gráfico de tu repositorio que sobrevive a la sesión:

```mermaid
flowchart LR
    B["<b>ANTES</b><br/>nace orientado<br/>mapa + memoria + confianza + lagunas honestas"]
    D["<b>DURANTE</b><br/>veredictos usados mientras trabaja<br/>impacto antes de tocar · actuar / reverificar / abstenerse"]
    A["<b>DESPUÉS</b><br/>memorizado con evidencia<br/>anclado al código real"]
    C["<b>COMPUESTO</b><br/>la siguiente sesión comienza adelantada<br/>cualquier host, cualquier agente"]
    B --> D --> A --> C --> B
```

La puerta principal es una sola llamada. `north(task)` devuelve toda la orientación en un único paquete antes de cualquier recuperación:

```jsonc
{"method":"tools/call","params":{"name":"north",
  "arguments":{"agent_id":"dev","task":"fortalecer el flujo de validación de tokens JWT"}}}
```

```jsonc
{
  "binding": { "trust_mode": "full_trust", "ok": true },      // veredicto antes de la recuperación
  "memory": [                                                 // recordado de una sesión ANTERIOR
    { "claim": "AuthTokenFlow", "source_agent": "authbot", "age_ms": 221, "stale": false }
  ],
  "sufficiency": { "state": "gathering", "top_score": 0.64 },
  "next_move": "Llama a `surgical_context` en el nodo de enfoque principal antes de editar.",
  "honest_gaps": []                                           // nada retenido en este gráfico
}
```

Mientras el agente trabaja, `impact` muestra el radio de acción antes de que se asiente un cambio, `why` explica una conexión y admite cuando el camino está basado en una suposición, y `xray_gate` advierte antes de que un cambio cruce un límite de arquitectura. Cuando el trabajo termina, `memorize` escribe la conclusión junto con la evidencia que la respalda. La siguiente sesión comienza con las conclusiones de la sesión anterior ya en mano, en cualquier host MCP: Claude Code, Codex, Cursor, Gemini, Zed, 22 hosts en total.

Nunca ejecutas ninguno de estos verbos tú mismo. Lo hace el agente. Tu superficie es un pequeño CLI de configuración y luego sigues comunicándote con tu agente como siempre.

## Sesenta segundos

El paquete npm es el instalador. El runtime nativo es un binario separado de Rust que el paso 1 obtiene como un lanzamiento firmado.

```bash
# 1 · instala el runtime nativo (firmado, verificado, con retrocesos)
npx -y @maxkle1nz/m1nd update apply --yes

# 2 · confirma que está visible (imprime un veredicto JSON; bueno luce como "status": "ok")
npx -y @maxkle1nz/m1nd doctor

# 3 · conecta tu host: configuración MCP + los ganchos de sesión que hacen a m1nd ambiental
npx -y @maxkle1nz/m1nd hosts apply --host claude --project . --yes

# 4 · primer valor: el paquete de orientación para TU repositorio, solo lectura, sin configuración del host tocada
npx -y @maxkle1nz/m1nd agent first-minute --repo . --query "mapea este repositorio" --json
```

El paso 1 verifica la firma con [`cosign`](https://docs.sigstore.dev/cosign/system_config/installation/), así que instala eso primero si no está en tu PATH. Si prefieres el registro de fuentes y aceptas saltarte la verificación, `cargo install m1nd-mcp` también funciona. ¿Prefieres ver antes de escribir? `hosts plan` imprime todo lo que `hosts apply` tocaría, y no escribe nada. Aún no hay comando de desinstalación; `hosts plan` funciona también como lista de qué remover manualmente.

Los ganchos del paso 3 son lo que hacen a m1nd ambiental: el paquete de orientación se inyecta en cada inicio de sesión y generación de subagente, y el agente se conduce a sí mismo desde allí. ¿Instalas desde un agente en lugar de una terminal? Hay una sección paralela legible por máquina en [`llms-install.md`](llms-install.md).

Un lanzamiento truncado o manipulado no puede instalarse en tu máquina, y una mala actualización está a un paso de retroceder: el actualizador verifica la firma contra la identidad exacta del build, luego el SHA-256 y el tamaño, antes de tocar cualquier cosa. Si falla la verificación, se rechaza en lugar de caer en una ruta no verificada. Detalles en [docs/AGENT-PACKS.md](docs/AGENT-PACKS.md).

## Si desaparezco

m1nd es MIT y no hay ningún servidor para perder. El runtime es un binario de Rust que ya está en tu disco. La memoria que escribe es markdown simple bajo `agent-memory/`, legible y buscable sin m1nd instalado. El gráfico se deriva de tu código y se reconstruye desde cero en cualquier máquina. Si este proyecto se detiene mañana, conservas los archivos y pierdes una herramienta. Eso es deliberado. Es por eso que la memoria es markdown y por qué no hay nube entre tu agente y su propio conocimiento.

## Por qué confiar en las respuestas

Por esto construí m1nd. Las capas de recuperación de información son buenas para responder. Casi ninguna es buena para negarse. m1nd trata la negativa como un resultado de primera clase:

```jsonc
// trust_selftest en un runtime no vinculado. El veredicto ES la instrucción de reparación:
{
  "ok": false,
  "verdict": "needs_ingest",          // nunca un simple "sin resultados"
  "next_action": "call_ingest",
  "recovery_playbook": {
    "steps": [ { "action": "Haz una llamada a ingest para el repositorio pretendido en este mismo binding." } ]
  }
}
```

Un acierto de `seek` lleva una lectura de suficiencia y un sobre de confianza. Cuando aún no se ha medido ningún calibrado, el sobre limita su propio veredicto a `reverify` en lugar de exagerar. La puerta de `predict` está ajustada para cobertura (α=0.10); en el historial de este repositorio eso cae aproximadamente a un tercio de precisión en la banda de `act`, y la mayor parte del tiempo se abstiene, que es la salida honesta de una señal débil. `abstain` dice al agente que se detenga. `insufficient_evidence` significa no hay evidencia alguna, que es distinto de riesgo medio, y la API mantiene los dos separados.

Dos herramientas, `savings` y `resonate`, fueron eliminadas completamente en beta (manejadores, tipos y archivos de estado, todo desaparecido) porque devolvían un éxito con cada entrada que les daba, y una herramienta que nunca pierde ha dejado de medir. Esa es la barra que cada afirmación en este archivo debe superar.

El vecino más cercano que conozco es GitHub Copilot Memory (vista previa pública, 2026): almacena hechos con citas de código y los vuelve a verificar contra la rama actual antes de usar. Esa es detección de desactualización real y merece crédito. También está en la nube, es binario y vive dentro de Copilot. Lo que aún no he encontrado en ningún lugar es el resto del veredicto: un graduado `act` / `reverify` / `abstain` con calibrado por repositorio, negativas tipadas que transportan un plan de reparación, en un gráfico local que cualquier agente de MCP puede compartir. He revisado los documentos públicos de Mem0, Zep, Letta, Cognee, Supermemory y Copilot Memory, hasta julio de 2026. ¿Conoces uno más cercano? Abre un problema y lo enlazaré aquí.

## Memoria que sabe cuándo está desactualizada

La mayoría de las capas de memoria almacenan texto y tienen esperanza. m1nd ancla la memoria al gráfico. Cuando un agente llama a `memorize`, cada camino de `evidence` de la afirmación se resuelve al nodo real del código, para que la nota salga cada vez que el agente toca ese código, sin que nadie recuerde que existe:

```jsonc
memorize({
  "agent_id": "authbot",
  "node_label": "AuthTokenFlow",
  "claims": [{
    "label": "TokenValidator",
    "text": "TokenValidator valida JWTs mediante HMAC. Rota claves solo a través de KMS.",
    "confidence": "high", "evidence": ["src/auth/token.rs"]
  }]
})
```

Como la memoria está anclada, puede ser auditada contra la realidad. `cross_verify` re-hashea cada archivo citado y nombra qué afirmaciones quedaron desactualizadas porque su código cambió. Las afirmaciones cargan antigüedad y autor, reemplazan afirmaciones más antiguas y caducan. Este ciclo es probado end-to-end en vivo en este repositorio: memorizar, anclar, editar el archivo citado, ver que la afirmación se auto-marca, sobrevivir a una reinserción completa, auto-cargar en el próximo inicio. Mata el proceso, empieza uno nuevo fresco, y el primer `north` ya lleva las afirmaciones de la sesión anterior con procedencia adjunta.

## Un solo gráfico para código y conocimiento (l1ght)

l1ght es la segunda vía del mismo motor: los documentos se convierten en nodos del gráfico en el mismo espacio de activación que el código, así que una consulta atraviesa ambos. No es una carpeta RAG pegada. Hay 7,400 líneas de adaptadores dedicados en este árbol: Markdown, HTML, PDF, texto plano, RST y JSON, más rutas académicas para BibTeX, DOI/Crossref, JATS papers, RFCs y patentes.

Diferentes personas obtienen diferentes productos de la misma vía:

- Un investigador coloca una carpeta de PDFs y DOIs junto al código de análisis y pregunta qué documento contradice la afirmación que implementa esta función.
- Un estudiante recorre un capítulo de texto y el código de ejercicio como un solo gráfico, y el agente explica cada uno en términos del otro.
- Un profesor ingiere las notas del curso una vez; el agente de cada estudiante responde desde el mismo corpus fundamentado en lugar de improvisar.
- Un ingeniero vincula RFCs y documentos de diseño con las funciones que los implementan; la sección de especificaciones está a un salto del código.
- Un codificador amateur toma su pila de archivos de chat y notas dispersas que deja de ser una carpeta y se convierte en memoria que el agente realmente consulta en medio de la edición.

Mismo binario, mismos verbos MCP, misma capa de confianza. `seek` en un gráfico mixto devuelve código y documentos en una sola respuesta clasificada.

## Cuándo no usar m1nd

Algunas razones honestas para cerrar esta página:

- Repositorios pequeños. Con menos de unos cientos de archivos, grep ya es económico y los bordes del gráfico se reducen hacia nada. Una medición independiente de herramientas gráficas comparables en un repositorio de ~110 archivos puso la ventaja en aproximadamente 20 por ciento. Real, pero no vale la pena correr un runtime por tan poco.
- Preguntas difusas. Un gráfico de símbolos responde "qué conecta con qué". No responde "por qué esto parece lento". La búsqueda agéntica es mejor para preguntas abiertas.
- Verdad del compilador y runtime. Tu LSP, tus pruebas y tu generador de perfiles son correctos y m1nd está adivinando. m1nd señala; ellos prueban.
- Tareas pequeñas. Un archivo y veinte líneas no necesitan una ingesta. Saltarlo.
- `predict` actualmente se abstiene la mayor parte del tiempo. Calibrado en el historial de este repositorio alcanza aproximadamente un tercio de precisión en la banda de `act` con baja cobertura. La abstención es la salida honesta de una señal débil, y ahora mismo también es la mayor parte de la salida.

m1nd complementa al compilador, las pruebas y tus herramientas de seguridad. No las reemplaza.

## Evidencia

Todo lo anterior está incluido en la versión actual; los documentos bajo `docs/` marcados como PRD son la intención del diseño, mantenidos etiquetados separados. Cada fila está limitada a exactamente lo que se midió. m1nd no lidera con ahorro de tokens o ROI, y eso es deliberado: esos son los números menos refutables en esta categoría.

| Afirmación | Resultado | Reproducir / límite |
|---|---|---|
| Latencia del gráfico | ~1.4µs `activate`, ~0.5µs `impact` en un gráfico sintético de 1K nodos | `cargo bench -p m1nd-core` en Apple silicon. Orden de magnitud solamente, dependiente del hardware. |
| Batería de capacidades vs grep | 37/37 pasa; 16 victorias, 12 empates frente a grep | `python3 scratchpad/m1nd_battery.py ./target/release/m1nd-mcp . --suite m1nd`. Un repositorio (este), casos auto-autorizados. |
| `predict` ajustado por cobertura | aproximadamente un tercio de precisión en la banda de `act` con baja cobertura (α=0.10) | Medido en el historial git de este repositorio, n≈9.2k predicciones retenidas. La puerta generalmente se abstiene, por diseño. |
| Auto-verificación de memoria | ciclo de 6 pasos probado en vivo | memorizar → anclar → marca de frescura en archivo editado → sobrevive reemplazo → auto-carga en inicio. |
| Persistencia entre reinicios y fallos | la puerta dirige el binario real vía stdio en cuatro inicios limpios y un `kill -9` | `m1nd-mcp/tests/persist_runtime_root.rs`. Revertir cualquiera de las correcciones de inicio lo hace rojo con un mensaje nombrando la regresión. |

## Un gráfico, muchos agentes

Para un agente, el servidor stdio de [Sesenta segundos](#sixty-seconds) es todo lo que necesitas, y el agente puede llamar `ingest` directamente sobre un gráfico vacío. Para trabajo real, ejecuta un propietario servido que mantenga el gráfico en vivo, y adjunta cada agente a él como un puente ligero:

```bash
m1nd-mcp --serve --no-gui --port 1337 --runtime-dir /tu/proyecto/.m1nd
m1nd-mcp --attach auto --stdio     # cada agente: sin carga de gráficos, sin arrendamiento, memoria compartida
```

Lo que un agente memoriza, otro lo recuerda inmediatamente, y las advertencias de presencia y colisión descritas antes funcionan a través de este mismo propietario. También aloja cerebros por repositorio y renderiza la interfaz web. Las consultas permanecen en localhost; cada vinculación fuera del loopback se rechaza hasta que exista transporte autenticado.

Una puerta que debes conocer: un propietario servido rechaza `ingest` genérico para repositorios que no alberga ya. Crear un nuevo cerebro en un propietario servido es un gesto governado y falla cerrado por diseño. Para una primera sesión en un nuevo repositorio, usa la ruta stdio o `m1nd agent first-minute`. Conéctate al propietario una vez que aloje tu repositorio. Guía completa de despliegue: [docs/deployment.md](docs/deployment.md).

## Cobertura del lenguaje

Extractores dedicados cubren más de veinte lenguajes, así que un repositorio poliglota no llega medio mapeado: Python y TypeScript hasta Elixir, Haskell y Zig, encaminados por extensión de archivo en `m1nd-ingest`. La tabla abajo representa la afirmación más estricta, probada end-to-end en una sola ingesta poliglota: bordes del gráfico de llamadas más resolución de importaciones entre archivos.

| Lenguaje | `calls` | importaciones entre archivos |
|---|:---:|:---:|
| Rust | ✅ | ✅ |
| Python | ✅ | ✅ |
| JavaScript / TypeScript | ✅ | ✅ |
| Go | ✅ | ✅ |
| Java | ✅ | ✅ |
| C / C++ | ✅ | ✅ |
| Kotlin | ✅ | ✅ |
| PHP | ✅ | ✅ |
| Scala | ✅ | ✅ |
| Ruby | ⏳ | ✅ |
| C# | ✅ | los espacios de nombres no mapean 1:1 con archivos |
| Swift | ✅ | todavía no |

Las importaciones irresolubles (paquetes externos, stdlib, headers del sistema) se dejan sin resolver en lugar de adivinarlas. Todo lo demás recae en un extractor genérico con sólo bordes `contains`.

## El humano es el segundo lector

La mayoría de las herramientas de desarrollo están creadas para una persona y luego crecen una API. m1nd opera al revés: el agente es el usuario, y los verbos son sus verbos.

Esa elección da forma al diseño de formas que puedes comprobar. Las negativas están tipadas y transportan un plan de recuperación, porque el lector que actúa sobre ellas es una máquina. Un mensaje de error que necesita interpretación humana es un fallo de diseño aquí. El mismo paquete de orientación que el agente lee como `north` se representa para ti como una tarjeta corta en la conversación y como el Árbol Viviente en la interfaz web servida (tu repositorio dibujado como un árbol navegable, notas memorizadas ancladas a él): calculado una vez, proyectado por lector, para que la vista humana nunca se desvíe hacia una segunda verdad.

Los humanos son bienvenidos. Simplemente eres el segundo lector, y el sistema es más honesto con ambos lectores por ello.

## Cómo se construye este repositorio

Lee el registro de commits con un poco de escepticismo, luego lee esto. Soy Max. Construyo m1nd dirigiendo un sistema de agentes de codificación, bajo reglas más estrictas que la mayoría de los equipos humanos con los que he trabajado:

- Cada cambio importante comienza como un especificación confrontada por un modelo oracle independiente antes de escribir el código. Las objeciones se registran dentro de los archivos de especificación.
- Cada corrección aterriza con una prueba que fue demostrada como fallida primero. Una prueba que nunca ha sido roja no demuestra nada.
- El revisor nunca es el autor. Cada mano de agente trabaja en un worktree aislado.
- Una puerta verde es un candidato. El gesto de aterrizaje es mío, y respondo por cada línea.
- Las leyes son nombres de prueba: `letter_cannot_color_the_store`, `gate_zero_cannot_land`, `graph_only_evidence_is_not_enough`.
- El árbol tiene 2,462 funciones de prueba y la puerta completa corre verde en Linux, macOS y Windows.

La pregunta del escéptico ("ningún humano escribe tanto y tan rápido") es correcta. Ningún humano lo hace. Un humano dirigiendo un sistema de pruebas de agentes lo hace. Este árbol es lo que salió. La capa de confianza de m1nd nació de esa práctica diaria: necesité que mis propios agentes dejaran de confiar en respuestas desactualizadas antes de poder enviar cualquier cosa a este ritmo.

## Arquitectura de un vistazo

Tres crates principales de Rust más auxiliares: `m1nd-mcp` (el servidor MCP y superficie del runtime), `m1nd-core` (el motor del gráfico: activación propagada, plasticidad hebbiana, CSR adjacency, ghost edges derivados de git), `m1nd-ingest` (extractores y adaptadores para código, documentos y memoria). Tu agente ve 48 herramientas por defecto en lugar de 130+, así que elige la correcta más seguido y paga por una lista de herramientas más corta en cada solicitud; toda la superficie está a un env var de distancia (`M1ND_TOOL_TIER=full`), y la clasificación solo recorta el menú anunciado, nunca la disponibilidad.

<p align="center">
  <img src=".github/m1nd-architecture-overview-v2.jpeg" alt="m1nd architecture overview" width="880" />
</p>

La profundidad vive en el [wiki](https://m1nd.world/wiki/), [docs/AGENT-PACKS.md](docs/AGENT-PACKS.md), [EXAMPLES.md](EXAMPLES.md) y [CHANGELOG.md](CHANGELOG.md).

## Traducciones

🇧🇷 [Português](i18n/README.pt-BR.md) · 🇪🇸 [Español](i18n/README.es.md) · 🇮🇹 [Italiano](i18n/README.it.md) · 🇫🇷 [Français](i18n/README.fr.md) · 🇩🇪 [Deutsch](i18n/README.de.md) · 🇨🇳 [中文](i18n/README.zh.md) · 🇯🇵 [日本語](i18n/README.ja.md)

Las traducciones siguen al texto en inglés con algún retraso. Cuando existan discrepancias, el inglés es canónico.

## Contribuciones

Las contribuciones son bienvenidas en extractores, adaptadores, herramientas MCP, benchmarks, documentación y algoritmos de gráficos. Véase [CONTRIBUTING.md](CONTRIBUTING.md). Hay una sala activa en [CodeRooms](https://coderooms.com/github/maxkle1nz/m1nd) si quieres hablar primero. Y si leíste hasta aquí y quieres intentarlo: [cuatro comandos](#sixty-seconds).

## Licencia

MIT. Véase [LICENSE](LICENSE).
```
