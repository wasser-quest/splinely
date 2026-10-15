# Splinely

**Movimiento por trayectorias:** enlace opcional, progreso animado 0–1 y orientación tangente en el inspector, CLI y ambas runtimes. [Uso y límites](docs/motion-paths.md).

[English](README.md) · [Deutsch](README.de.md) · **Español** · [Français](README.fr.md) · [日本語](README.ja.md) · [简体中文](README.zh.md)

![Logotipo de Splinely](assets/splinely-logo.svg)

Splinely es un editor de animación vectorial escrito en Rust. Los proyectos viven en **archivos `.sply`**: formas y trazados, fotogramas clave, huesos, máquinas de estados, seguimiento del puntero y audio, todo en un archivo. El **Splinely Runtime** reproduce los proyectos `.sply` con todo su comportamiento: en el navegador, de forma nativa (Rust/C) o en Splinely Viewer. **Estado actual: un prototipo funcional, no todo el alcance de [PLAN.md](PLAN.md).**

El editor está disponible en **inglés, alemán, español, francés, japonés y chino simplificado**. Sigue el idioma del sistema y se puede cambiar en **Ajustes › General › Idioma**. El japonés y el chino necesitan una fuente CJK instalada (p. ej. Noto Sans CJK). Los documentos detallados de `docs/` están en alemán.

## Cómo funciona Splinely

- **`.sply` es la fuente.** Describe cómo se ven las cosas *y* cómo se comportan: clips, máquinas de estados, transiciones con fundidos y temporizadores, seguimiento del puntero, audio y variables.
- **El Splinely Runtime le da vida.** Un archivo SVG por sí solo es solo una imagen: no tiene máquinas de estados, temporizadores ni seguimiento del puntero. Esa lógica está en el runtime, que existe como runtime de navegador (JavaScript), runtime nativo (`splinely_runtime`, ABI de Rust/C) y en el **Splinely Viewer** de escritorio.
- **Exportaciones:** el *SVG estático* es una imagen fija de un instante. El *reproductor web* (`splinely-cli export`, «Exportar SVG interactivo») es un único archivo autónomo que incluye el proyecto `.sply` junto con el runtime de navegador, así que funciona en cualquier navegador sin servidor.

### Muchas interacciones a la vez

Varias máquinas de estados funcionan en paralelo y cada una **funde** sus estados (`blend` en segundos), de modo que hover, clic y estado de ánimo se superponen con suavidad. Las **transiciones temporizadas** (`after`) cambian solas (parpadear, volver al reposo), las **acciones de salida** se ejecutan al abandonar un estado y los **seguidores** hacen que formas o huesos sigan al puntero (pupilas, cabeza, brazos) por encima de todo lo demás. El runtime respeta el ajuste del sistema *reducir movimiento*. Ejemplo: la [cara curiosa](examples/curious-face.svg): las pupilas siguen al ratón, el hover la inclina, un clic sonríe y un temporizador parpadea, todo sin código propio.

```sh
splinely-cli on face.sply --event click --target face --machine mood --to happy --state face.fill=gold --blend 0.4
splinely-cli on face.sply --machine blink --from open --to shut --after 3 --state eye.scaleY=0.1 --blend 0.08
splinely-cli follow face.sply --target pupil --max 8 --strength 0.15 --smoothing 0.1
```

## Instalar con un LLM

Basta con decirle a un agente de IA con acceso a la terminal (p. ej. Claude Code):

> Instálame Splinely: https://raw.githubusercontent.com/wasser-quest/splinely/main/llms.txt

[`llms.txt`](llms.txt) explica al agente cómo descargar el paquete precompilado adecuado de las releases, comprobar la suma de verificación, descomprimirlo en `~/.local/opt` y trabajar después con `splinely-cli`.

## Primeros pasos

**Sin compilar:** cada [release](https://github.com/wasser-quest/splinely/releases/latest) incluye paquetes precompilados para Linux (`splinely-vX.Y.Z-x86_64-linux.tar.gz` con editor, CLI, visor, runtime y `libsplinely_runtime.so`). Instalación: [docs/INSTALL-BINARIES.md](docs/INSTALL-BINARIES.md).

Windows x86_64 (`.zip`) y macOS Apple Silicon/Intel (`.tar.gz`) están **configurados, pero no probados en hardware real**: el código solo se ha comprobado a nivel de tipos desde Linux; los binarios no están firmados/notarizados. Detalles: [docs/INSTALL-BINARIES.md](docs/INSTALL-BINARIES.md).

Desde el código fuente:

Requisitos: Rust/Cargo; un entorno gráfico/de ventanas funcional para la ventana del escritorio. La primera compilación descarga las dependencias. Node.js y Python 3 solo hacen falta para los scripts de comprobación (`scripts/check.sh`).

En Linux, compilar el editor y el visor requiere los paquetes de desarrollo de GTK/WebKit, ALSA y D-Bus:

```sh
# Fedora
sudo dnf install gtk3-devel webkit2gtk4.1-devel libsoup3-devel alsa-lib-devel dbus-devel libxkbcommon-devel
# Debian/Ubuntu
sudo apt install libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev libasound2-dev libdbus-1-dev libxkbcommon-dev pkg-config
```

```sh
git clone https://github.com/wasser-quest/splinely.git
cd splinely
cargo run -p splinely_editor --bin splinely
# Exportar sin ventana de escritorio:
cargo run -p splinely_editor --bin splinely -- --demo-export /tmp/splinely-demo.svg
```

El editor arranca con un lienzo vacío. **Arrastra un archivo `.sply`**, usa **Archivo → Abrir …** (Ctrl+O, diálogo del sistema) o **Recientes**, o inicia directamente: `cargo run -p splinely_editor --bin splinely -- examples/demo.sply`. Los cambios sin guardar y los borradores de dibujo/código se confirman antes de cambiar de proyecto y al cerrar la ventana; el título muestra el archivo y `•` si hay cambios sin guardar. **Guardar** conserva el proyecto editable; el SVG es solo una exportación opcional.

**Diseñar / Animar / Avanzado** separan dibujo, animación y código. Selección, Pluma, Puntos del trazado y Mano alzada son herramientas exclusivas; las opciones adicionales permanecen plegadas. **Vista previa**, **Reproducir** y **Abrir Splinely Viewer** están siempre a mano. [Uso, gestos y límites](docs/usability.md).

El JavaScript propio solo se ejecuta tras una autorización explícita por sesión en **Avanzado → JavaScript**, nunca en el lienzo nativo. En **Animar → Audio** se puede activar una vista previa limitada con `ffplay` de las señales de audio globales; la preparación ocurre en segundo plano.

## Un solo archivo de proyecto: `.sply`

Geometría, capas, huesos, fotogramas clave, estados, pesos de mezcla, variables, audio incrustado y código propio quedan **juntos en un archivo**. No hace falta JSON/SVG adicional ni carpeta de recursos. Por defecto `.sply` se guarda en **binario** compacto (sin pérdida, validado estrictamente, con CRC); `.json`/`.splinely.json` siguen siendo JSON legible y el formato se detecta automáticamente al abrir (también los `.sply` JSON antiguos). `splinely-cli cat`/`convert` muestran o escriben JSON. La aplicación/runtime y los plugins instalados no forman parte del archivo de proyecto. [Formato y compatibilidad](docs/sply-format.md).

En **Animar**, **Animar fácil** (fotograma clave/preajuste) y **Máquina de estados guiada** (clic/hover, visibilidad, inicio de clip) te guían en los primeros pasos. La mezcla ponderada y los fundidos temporizados siguen disponibles junto a las herramientas expertas. [Paso a paso y límites de mezcla](docs/animation-workflow.md).

```sh
splinely-cli new --demo --output scene.sply
splinely-cli convert legacy.splinely.json --output scene.sply
splinely-viewer scene.sply
# Solo para código propio revisado:
splinely-viewer examples/desert-stickfight.sply --trusted-js
```

## Splinely Viewer

`cargo build -p splinely_viewer` compila el WebView SVG nativo real. En el editor, **▶ Splinely Viewer** abre una instantánea del proyecto actual con runtime, estados y audio incrustado; el lienzo silencioso de resvg permanece aparte. JS propio solo tras autorizarlo en Avanzado. Directamente, sin exportar antes un SVG: `cargo run -p splinely_viewer --bin splinely-viewer -- examples/brown-owl.sply --trusted-js` (revisa antes el código incluido). Los SVG existentes también son compatibles. Los SVG activos de terceros no son de confianza. Linux necesita GTK3/WebKitGTK; [detalles y pruebas](crates/splinely_viewer/README.md).

Las exportaciones SVG incluyen **Created with Splinely**, la versión de la aplicación/esquema y metadatos legibles por máquina (`urn:splinely:metadata`). No hay marca de agua visible; la etiqueta se puede editar y no prueba la autenticidad.

## Usar el runtime

[Recetario del runtime](docs/runtime-cookbook.md): reproducir/pausar/reiniciar, reloj de clip y global, incrustación en HTML, audio y solución de problemas. `splinely.snapshot()` devuelve el estado del runtime como datos JSON copiados; `splinely.reportError(error)` usa el canal de errores común con historial limitado.

## Runtime independiente del navegador

`splinely_runtime` ofrece el núcleo de documentos de Splinely como runtime nativo en Rust, ABI de C (`cdylib`) y proceso de líneas JSON `splinely-runtime`. Otros lenguajes pueden enviar eventos, controlar el tiempo/los pesos de los clips y leer nodos/huesos evaluados. El anfitrión se encarga del dibujo, la detección de clics y el audio: sin navegador, pero tampoco como intérprete general de JavaScript para SVG. [API y ejemplos en Python](docs/native-runtime.md).

```sh
cargo build -p splinely_runtime
python3 examples/native-runtime/smoke.py --library target/debug/libsplinely_runtime.so --binary target/debug/splinely-runtime
```

## Flujo de trabajo con la CLI

La CLI y el editor comparten el mismo núcleo de documentos. Los resultados de la CLI son JSON; los errores aparecen como JSON en stderr (con un `hint` para el siguiente paso) y devuelven un código de salida de error. No se necesita proveedor de IA, clave de API ni servidor.

**Para empezar:** `splinely-cli guide` muestra el flujo breve con recetas probadas. Sin `--output`, los comandos de edición guardan en el propio archivo de entrada; `check` encuentra errores típicos (formas ocultas, clips que nunca se reproducen, texto ilegible …) con una corrección ejecutable, `batch` ejecuta muchos pasos de forma atómica, `clip` gestiona clips y `on` hace las formas interactivas.

```sh
F=/tmp/scene.sply
cargo run -p splinely-cli -- new --output $F --width 800 --height 450 --background "#0b1d3a"
cargo run -p splinely-cli -- node add $F --kind circle --id sun --x 400 --y 420 --radius 60 --fill orange --pivot center
cargo run -p splinely-cli -- keyframe $F --clip sunrise --target sun --property y --time 3 --by -260 --easing ease-out --autoplay --loop
cargo run -p splinely-cli -- on $F --event click --target sun --set sun.fill=gold
cargo run -p splinely-cli -- check $F
cargo run -p splinely-cli -- render $F --time 1.5 --output /tmp/scene.png
```

Con JSON y merge patches:

```sh
cargo run -p splinely-cli -- new --demo --output /tmp/draft.splinely.json
cargo run -p splinely-cli -- validate /tmp/draft.splinely.json
cargo run -p splinely-cli -- inspect /tmp/draft.splinely.json
cargo run -p splinely-cli -- evaluate /tmp/draft.splinely.json --time 0.5
printf '%s\n' '{"name":"Mi animación"}' > /tmp/splinely-patch.json
cargo run -p splinely-cli -- apply /tmp/draft.splinely.json --patch /tmp/splinely-patch.json --output /tmp/final.splinely.json
cargo run -p splinely-cli -- export /tmp/final.splinely.json --output /tmp/final.svg
```

`schema` devuelve una guía del formato con ejemplos completos, no un JSON Schema formal. `apply` usa merge patches RFC 7396: los arrays se reemplazan por completo. `evaluate` no ejecuta acciones de estado, JS ni audio; los clips inactivos no se reproducen. Detalles: [CLI.md](CLI.md), [CORE_API.md](CORE_API.md). Para otros LLM: [guía de autoría](LLM.md) y [prompt para copiar](prompts/splinely-author.md); `schema` y `capabilities` enumeran las funciones realmente admitidas.

## Ejemplo completo sin conexión

- [examples/demo.splinely.json](examples/demo.splinely.json): proyecto editable con un círculo, un movimiento de tres segundos por fotogramas clave, un cambio de estado al hacer clic y una señal WAV incrustada a **1,5 segundos de tiempo global de transporte**.
- [examples/demo.svg](examples/demo.svg): la exportación real de la CLI con un breve manejador `customJs` autorizado explícitamente. Al hacer clic en el círculo se desbloquea el audio y se reinician el movimiento y el transporte; con el primer clic el círculo se vuelve naranja.
- El sonido es **un tono sintético de 0,4 segundos parecido a sílabas, sin palabras habladas**. WAV PCM, mono, 8 kHz, 16 bits; sin archivos de audio externos.

```sh
# Reproducible: CLI new --demo → apply → validate → export --trusted-js
bash scripts/generate-demo.sh
# Exportación por defecto: el código solo se guarda; el control de inicio desbloquea el audio sin código de usuario.
cargo run -p splinely-cli -- export examples/demo.splinely.json --output /tmp/demo-safe.svg
# Solo tras revisar tu propio código:
cargo run -p splinely-cli -- export examples/demo.splinely.json --trusted-js --output /tmp/demo-trusted.svg
```

Abre `demo.svg` directamente en el navegador como `file://`, haz clic en **Start audio / animation** y después en el círculo. No lo incrustes como `<img>`: ahí no se ejecutan los scripts de SVG. Sin scripts se ve el gráfico inicial estático. Los navegadores/CSP pueden bloquear scripts o la compilación dinámica de código; el audio necesita una interacción real del usuario y un decodificador compatible.

El runtime incluido y el archivo de ejemplo no necesitan bibliotecas externas ni recursos de red. **No es una sandbox**: el código de usuario autorizado puede hacer peticiones de red y otras acciones del navegador. Por defecto `customJs` se conserva pero no se ejecuta; `rawSvg` queda solo como metadatos. No exportes/abras proyectos desconocidos con `--trusted-js`.

## Vectores, capas y huesos

- **Capas:** selección, orden, visibilidad y bloqueo de edición. Colores y transparencia están en el inspector.
- **Trazados:** doble clic en el borde de un trazado inserta un ancla sin alterar la forma; Mayús vincula las tangentes Bézier simétricamente, Mayús+Alt conserva la longitud vecina. [Detalles](docs/path-editing.md). Edita la sintaxis de trazados SVG y también anclas/puntos de control Bézier gráficos; **Pluma** dibuja con clic y arrastre (Intro termina, Escape cancela). **Snap 10** ajusta localmente las posiciones al soltar. Convierte rectángulos/círculos en trazados y devuélvelos mientras no cambien. La importación SVG es limitada a propósito y rechaza la apariencia no admitida en lugar de perderla en silencio.
- **Huesos:** selecciona/mueve articulaciones, edita padres y vínculos, anima la rotación. Vinculación rígida e IK limitado de dos huesos para articulaciones hijas directas sin desfase; sin deformación de mallas. Carga `examples/rig.splinely.json`; `examples/rig.svg` muestra la misma animación en el navegador.
- **Navegación:** F/Inicio ajusta la escena; +/- hace zoom, 0 vuelve al 100 %. Botón central o Espacio + botón izquierdo desplazan la vista sin cambiar formas; Espacio sin arrastrar inicia/detiene al soltar. Las reglas se pueden mostrar/ocultar y se mantienen en coordenadas del documento. La línea de tiempo se puede ocultar. **Vista previa** separa los eventos de la escena de la edición.
- **Mano alzada/tableta:** el modo a mano alzada crea trazados SVG editables, un trazo por paso de deshacer. Trazos sensibles a la presión solo si hay eventos de fuerza; sin promesa general de hardware/controladores. [Notas sobre tabletas](docs/tablet.md).
- **Plugins instalables:** coloca un `splinely-plugin.json` en la carpeta de plugins y reinicia: herramientas de formas declarativas y paletas se cargan solas, sin compilar. [Formato, carpetas y ejemplos](docs/drop-in-plugins.md); nunca se ejecutan scripts/bibliotecas de terceros automáticamente.
- **Plugins:** extensiones ECS estáticas en Rust con recursos propios y una fase de decoración. [Plantilla](plugin-templates/editor/README.md); `splinely-cli plugin` genera código fuente pero nunca lo ejecuta. Los plugins nativos son código de confianza, no una sandbox. → [docs/plugins.md](docs/plugins.md) (hover-wiggle, `plugin list|validate|apply`, `export --plugin … --trusted-js`).

```sh
cargo run -p splinely-cli -- import-svg drawing.svg --output drawing.splinely.json
cargo run -p splinely-cli -- rig-demo --output /tmp/rig.splinely.json
cargo run -p splinely-cli -- export /tmp/rig.splinely.json --output /tmp/rig.svg
```

Crea y organiza formas con la CLI y compruébalas como PNG (misma lógica que el editor):

```sh
cargo run -p splinely-cli -- node add scene.sply --kind circle --x 200 --y 150 --radius 40 --fill tomato --output scene.sply
cargo run -p splinely-cli -- arrange align scene.sply --nodes node-1,node-2 --edge top --output scene.sply
cargo run -p splinely-cli -- render scene.sply --output scene.png --width 800
```

Más comandos de formas, organización, huesos, capas y conversión: [CLI.md](CLI.md). `keyframe` añade claves sueltas de nodos/huesos sin parches de arrays completos.

## Implementado y pendiente

| Implementado | No implementado / pospuesto |
| --- | --- |
| Rectángulo, círculo, texto, trazados, polígono/estrella/línea; selección múltiple y por rectángulo, tiradores de escala/rotación, guías inteligentes y manuales, menú contextual, paleta de comandos (Ctrl+K), ajustes (Ctrl+,) con atajos reasignables, tema claro/oscuro, idioma de la interfaz y cuadrícula visible, modo de grabación (auto-key), historial, medidas junto al puntero, línea de tiempo visual con fotogramas clave arrastrables y menú de transiciones, papel cebolla configurable, edición de texto en el lienzo, colores del documento, plantillas para proyectos nuevos, exportación PNG/GIF, cuentagotas, degradados lineales/radiales, grupos anidados (vista de árbol, exportación `<g>`), texto a trazado, alinear/distribuir/voltear, operaciones booleanas que conservan curvas, morphing de trazados, degradados animados, orden de capas, copiar/pegar, colores/transparencia, conversión, deshacer/rehacer | El morphing entre formas de topología distinta es aproximado (subtrazados que crecen/encogen), sin correspondencia exacta de agujeros |
| Cargar/guardar proyectos mediante archivo temporal + renombrado; guardado automático cada 30 s con diálogo de recuperación | Migraciones históricas del esquema, cuotas de recursos completas |
| Línea de tiempo, fotogramas clave, easing, relojes de clip independientes, estados paralelos, clips ponderados y fundidos cruzados temporizados (`blendClip`) | Mezcla aditiva, blend tree/blend space visual |
| Modelo de estados, guardas/acciones, acciones de salida, transiciones temporizadas, fundidos entre estados (varias máquinas a la vez), seguimiento del puntero, grafo de estados editable, edición JSON y vista previa nativa con hover | Conexiones libres por arrastre, disposición automática del grafo, movimiento a lo largo de trazados |
| Eventos del navegador, API de clips/transporte, recursos y señales de audio incrustados | Ejecución nativa de JS o latencia de audio garantizada; la vista previa nativa solo cubre señales globales vía ffplay |
| Exportación SVG estática/interactiva, importación vectorial SVG limitada, campos de código JS/SVG en bruto | **Sin importador completo para SVG arbitrarios**; texto, degradados e imágenes SVG data-URI se importan con un informe de lo aproximado/descartado; las imágenes rasterizadas se validan pero no se importan; clips/máscaras/filtros/archivos externos se rechazan; el SVG en bruto sigue siendo metadatos, con aviso de conflicto si difiere del dibujo ([detalles](docs/svg-import.md)) |
| Proyección ECS de Bevy, plugins estáticos registrables, huesos y vínculos rígidos | Plugins nativos dinámicos, skinning de mallas, cadenas IK generales, traducción automática de sistemas Bevy a código de navegador, vista previa aislada no fiable |

La vista previa nativa renderiza SVG con resvg: trazados, esquinas redondeadas y transformaciones se ven. El texto usa fuentes locales del sistema y puede variar entre navegadores. Las formas se seleccionan por su contorno exacto; las formas invisibles (opacidad 0) no se pueden pulsar. El esquema 1 se ha ampliado; los proyectos antiguos cargan con valores por defecto para los campos nuevos. Las versiones antiguas de Splinely no entienden los campos nuevos. El archivo `.sply` (JSON por dentro) es la referencia; la exportación SVG no es un viaje de ida y vuelta sin pérdidas.

## Comprobaciones

```sh
bash scripts/check.sh
```

El script ejecuta las pruebas del workspace, las pruebas del runtime en Node, la generación/evaluación/exportación real de la demo con la CLI y una comprobación en Python de XML/metadatos/WAV/recursos. También revisa los archivos de ejemplo incluidos. **No sustituye una prueba real en navegador/audio.** Además, `python3 scripts/svg-corpus.py` comprueba SVG reales instalados y `python3 scripts/browser-smoke.py examples/demo.svg /tmp/splinely-svg-corpus/manifest.json` prueba Firefox con comparación de píxeles. Las pruebas cubren JSON estricto, trazados, capas, huesos, ciclo de vida de plugins, regresiones de seguridad del runtime y cada idioma de la interfaz. La salida de audio real y su latencia no se verifican. Reproducción y aceptación manual: [docs/browser-tests.md](docs/browser-tests.md). Capturas de la GUI nativa y verificación de artefactos: [docs/verification.md](docs/verification.md).

## Demostraciones animadas

[Vista general](examples/showcase.html) · [SVG interactivos, eventos directos y estados invisibles](docs/interactive-svg.md)

- **[Sand / Accord](examples/desert-stickfight.html)** ([`.sply` editable](examples/desert-stickfight.sply)): corto original de 20 segundos con dos figuras con huesos en el estilo desértico de Cactus Runner, cinco fases de lucha y un final amistoso; reproducir/pausar/repetir. [Proyecto, generación y prueba real en Firefox](docs/desert-stickfight.md).
- [Cactus Runner](examples/cactus-runner.html): figura con huesos que corre, cactus que se desplazan y salto con clic con mezcla real de pesos caminar/saltar. [Detalles/prueba](docs/cactus-runner.md).
- [Cinder / Flux – Stickfight](examples/stickfight.html): pelea animada original de ocho segundos entre dos rigs con esquivas/contraataques, sombras/chispas y reproducir/pausar/reiniciar. `python3 scripts/generate-stickfight.py`, prueba: `python3 scripts/stickfight-smoke.py`.
- [Búho marrón](examples/brown-owl.html): un búho en una rama con estados directos de clic/hover; al pasar el ratón los ojos siguen el puntero y se levantan los penachos; un clic le hace cerrar los ojos y alzar las alas. Prueba: `python3 scripts/owl-smoke.py`.
- [Aoi dice hola](examples/anime-hello.html): personaje original de inspiración anime con mano articulada. **Un WAV generado localmente va incrustado en el SVG**; no hace falta conexión. Un clic desbloquea el audio. Reproducir: `python3 scripts/generate-anime-demo.py` (requiere `espeak-ng`/`espeak` local); prueba en navegador: `python3 scripts/anime-smoke.py`.
- [Cara curiosa](examples/curious-face.svg) ([`.sply`](examples/curious-face.sply)): tres máquinas de estados a la vez – el hover la inclina, un clic funde a una sonrisa, un temporizador parpadea – mientras las pupilas siguen al ratón. Sin código propio. Generar: `python3 scripts/generate-curious-face.py`, prueba: `python3 scripts/face-smoke.py`.
- [Gota de agua sobre una esfera azul](examples/water-drop.html): escena SVG autónoma con squash/stretch, brillos y ondas; reproducir/pausar.
- [Figuras aleatorias](examples/random-figures.html): reproducibles por semilla, secuencia automática de estados con rótulos. Pulsa **Play**. Los cambios de estado siguen el final de los clips.
- Ambos HTML incrustan el SVG vecino mediante `object`; sin API de servidor. El servidor de pruebas es solo una ayuda local.
- Generar: `splinely-cli water-drop --help`, `splinely-cli random-figures --help`; ejemplos completos en [CLI.md](CLI.md). Prueba en navegador: `python3 scripts/showcase-smoke.py --duration 35`.

## Logotipo

Logotipo SVG original: una S dibujada como spline Bézier con anclas, un tirador y una estela de movimiento.

## Licencia

MIT, consulta [LICENSE](LICENSE).
