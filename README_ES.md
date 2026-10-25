<p align="center">
  <img src="logo.png" width="100" alt="AI Gateway Logo" />
</p>

<h1 align="center">🚀 AI Gateway</h1>

<p align="center">
  <strong>Una pasarela para todas las plataformas de IA</strong><br/>
  Balanceo de carga con varias claves · reintentos y cambio de backend · protocolos OpenAI &amp; Anthropic · sin cambios en el cliente
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-2021-orange?logo=rust" alt="Rust" />
  <img src="https://img.shields.io/badge/TypeScript-6.x-blue?logo=typescript" alt="TypeScript" />
  <img src="https://img.shields.io/badge/Platform-macOS%20%7C%20Windows%20%7C%20Linux-green" alt="Platform" />
  <img src="https://img.shields.io/badge/License-MIT-yellow" alt="License" />
  <a href="https://github.com/keiskeies/ai-gateway/stargazers"><img src="https://img.shields.io/github/stars/keiskeies/ai-gateway?style=social" alt="Stars" /></a>
</p>

<p align="center">
  <a href="README.md">中文</a> ·
  <a href="README_EN.md">English</a> ·
  <a href="README_DE.md">Deutsch</a> ·
  <a href="README_FR.md">Français</a> ·
  <strong>Español</strong> ·
  <a href="README_PT.md">Português</a>
</p>

<p align="center">
  <a href="#-inicio-rápido"><strong>Inicio rápido →</strong></a>
</p>

---

> 🌟 Si este proyecto te ayuda, una estrella ayuda a que otros lo encuentren.

---

## 😩 Qué problema resuelve

- Tienes varias claves del mismo proveedor y quieres usarlas juntas, sin escribir tu propia rotación y lógica de reintentos
- Quieres reintentos automáticos en 429 / 5xx / timeouts del upstream en lugar de reimplementarlos en cada cliente
- OpenAI y Anthropic implican dos SDK, dos URL base y dos formatos de mensaje
- Los niveles gratuitos y los modelos de pago se mezclan, mientras los costes, las cuotas y las causas de fallo están dispersos
- Quieres repartir claves a un equipo sin compartir las credenciales de los proveedores

AI Gateway reúne todo eso en un proceso local: **un punto de entrada, un conjunto de claves, un conjunto de estadísticas**.

---

## 📸 Capturas de pantalla

Todas las capturas son de la aplicación de escritorio actual (tema oscuro).

<p align="center">
  <img src="doc/console-wall.png" width="92%" alt="Panel de monitorización" />
</p>
<p align="center"><em>Monitorización — tira de KPI y tendencias por modelo con ocho métricas (peticiones / fallos / entrada / salida / latencia / TTFT / velocidad / caché) en granularidad de hora, día, mes y año, mapa de calor de actividad (7d / 30d / 12m / Todo), cuota por plataforma, matriz de rendimiento y registro de peticiones. Se admiten pantalla completa y pausa de la actualización</em></p>

<details>
<summary>📱 Mostrar el resto de páginas</summary>

<p align="center">
  <img src="doc/console-traffic.png" width="92%" alt="Tráfico" />
</p>
<p align="center"><em>Tráfico — un resumen de «requiere atención» para límites de débito, fallos y claves inactivas; las peticiones se filtran por modelo, plataforma, clave, estado, tipo de error y rango de latencia, con paginación; el panel de detalle muestra la cadena de intentos (estado y duración de cada intento de backend), barras de TTFT / generación, lectura de caché frente a entrada nueva y coste estimado, y permite copiar el ID o repetir la petición</em></p>

<p align="center">
  <img src="doc/console-models.png" width="92%" alt="Modelos virtuales" />
</p>
<p align="center"><em>Modelos virtuales — estado, número de backends, estrategia, volumen de peticiones, tasa de éxito, latencia media y coste directamente en la lista</em></p>

<p align="center">
  <img src="doc/console-model-routing.png" width="92%" alt="Enrutado del modelo" />
</p>
<p align="center"><em>Detalle del modelo · Enrutado — edita estrategia, reintentos, backoff, max_tokens por defecto y distribución de pesos a la izquierda; la columna vertebral de enrutado dibuja los backends como un grafo ponderado y ordenado por prioridad en el centro; a la derecha se inspeccionan el proveedor, las insignias de capacidades y el tratamiento de los parámetros de razonamiento de un backend</em></p>

<p align="center">
  <img src="doc/console-platforms.png" width="92%" alt="Plataformas" />
</p>
<p align="center"><em>Plataformas — resultado del test de conectividad, número de modelos sincronizados, cuántos modelos virtuales la referencian, volumen de peticiones y tasa de éxito; las plataformas degradadas se cuentan en la cabecera</em></p>

<p align="center">
  <img src="doc/console-platform-detail.png" width="92%" alt="Detalle de la plataforma" />
</p>
<p align="center"><em>Detalle de la plataforma — credenciales y cabeceras reenviadas (las claves guardadas solo se devuelven enmascaradas, nunca en claro), descarga de modelos remotos mediante «Probar conexión», importación selectiva de modelos desde la plataforma, importación/exportación CSV de precios, y qué modelos virtuales dependen de esta plataforma</em></p>

<p align="center">
  <img src="doc/console-keys.png" width="92%" alt="Claves API" />
</p>
<p align="center"><em>Claves API — peticiones, tokens, latencia media, coste y último uso por clave; una clave puede vincularse a un solo modelo virtual</em></p>

<p align="center">
  <img src="doc/console-key-quota.png" width="92%" alt="Cuota de clave" />
</p>
<p align="center"><em>Cuota de clave — caducidad, límite de peticiones, límite de tokens, presupuesto de coste y ventana de cuota (de por vida / día / semana / mes). Alcanzar cualquier límite devuelve 429; el presupuesto de coste solo se aplica a modelos con precio</em></p>

<p align="center">
  <img src="doc/console-playground.png" width="92%" alt="Playground" />
</p>
<p align="center"><em>Playground — elige un modelo virtual y un protocolo (OpenAI / Anthropic) y envía una petición real para verificar el enrutado y el upstream; crea una clave temporal al momento o copia el cURL equivalente</em></p>

<p align="center">
  <img src="doc/console-clients.png" width="92%" alt="Acceso de clientes" />
</p>
<p align="center"><em>Acceso de clientes — toma por tarjetas de las configuraciones locales de Claude Code / Codex / OpenCode / OpenClaw / Pi / Hermes; al expandir se muestra el diff, cada aplicación se respalda y la desconexión restaura el estado</em></p>

<p align="center">
  <img src="doc/console-settings.png" width="92%" alt="Ajustes" />
</p>
<p align="center"><em>Ajustes — puerto y dirección de escucha, valores por defecto de la política de peticiones, seguridad de la cuenta, estadísticas de datos (filas / trazas de intentos / alertas / filas huérfanas / retención de cuerpos de petición), exportación e importación de configuración, y copia de seguridad y restauración completas de la base</em></p>

</details>

---

## ⚡ Funciones principales

### 🎯 Varias claves: combinar varias claves de un mismo proveedor

Una plataforma guarda una sola credencial upstream; un modelo virtual tiene uno o varios backends. Para combinar varias claves:

```
Crear una plataforma por clave → añadirlas todas al mismo modelo virtual → elegir turno rotatorio o menos conexiones
```

```
Tu aplicación
   │
   └──→ Modelo virtual ──┬── Plataforma A (clave 1, 3 RPM)
                         ├── Plataforma B (clave 2, 3 RPM)
                         └── Plataforma C (clave 3, 3 RPM)
```

El rendimiento total es la suma de los límites de cada clave; la pasarela no evita los límites de débito del upstream.

### 🔀 Estrategias de balanceo

| Estrategia | Comportamiento | Ideal para |
|-----------|----------------|------------|
| **Turno rotatorio** | Reparte las peticiones por turnos entre los backends del modelo virtual | Combinar claves / cuentas |
| **Aleatorio ponderado** | Elige un backend según `weight` | Capacidad desigual de los backends |
| **Menos conexiones** | Elige el backend con menos peticiones esperando las cabeceras de respuesta del upstream | Suavizar picos de peticiones cortas |
| **Prioridad** | Siempre el backend activo con el menor valor de `priority` | Un backend principal fijo |

> «Latencia» está disponible en la interfaz, pero hoy usa la implementación de turno rotatorio; la latencia aún no influye en la selección. «Prioridad» tampoco baja automáticamente si falla el backend principal. Ver [Limitaciones conocidas](#-limitaciones-conocidas).

### 🌐 Protocolos OpenAI y Anthropic

```
SDK OpenAI / herramientas compatibles OpenAI ──→ POST /v1/chat/completions ──┐
                                                                             ├──→ enrutado → upstream
SDK Anthropic ─────────────────────────────────→ POST /v1/messages ──────────┘
```

| Ruta | Protocolo | Nota |
|---|---|---|
| `/v1/chat/completions` | OpenAI | Punto de entrada principal |
| `/v1/completions` | OpenAI | Ruta antigua, se analiza como chat |
| `/v1/messages` | Anthropic | Formato nativo de Anthropic |
| `/v1/models` | OpenAI | Pública; devuelve los modelos virtuales y las capacidades |

Las respuestas sin streaming del mismo protocolo se reenvían tal cual (solo se sustituye `model` por el nombre del modelo virtual); las respuestas entre protocolos se reconstruyen desde el formato unificado, y el streaming se convierte bloque a bloque.

### 🧠 Razonamiento: decide el cliente

Sean cuales sean los parámetros de razonamiento que envíe el cliente, la pasarela los traduce al formato que entiende el upstream. Si el cliente no envía ninguno, la pasarela no activa el razonamiento por sí sola.

| El cliente envía | El upstream es Claude | El upstream es un modelo de razonamiento compatible con OpenAI |
|---|---|---|
| `reasoning_effort: "high"` (OpenAI) | `thinking: {type: "enabled", budget_tokens: 16384}` | `reasoning_effort: "high"` |
| `thinking: {budget_tokens: 10000}` (Anthropic) | Se conserva; el presupuesto se recorta si `max_tokens` es demasiado pequeño | Se asigna al esfuerzo correspondiente |

- Estilo por backend: `Auto` / OpenAI effort / Anthropic thinking / OpenRouter / Qwen / GLM / vLLM Qwen / desactivado
- `Auto` resuelve según el tipo de upstream: `thinking` para Anthropic, `reasoning_effort` en el resto; `param_overrides` se fusiona al final y prevalece
- Las firmas de Anthropic se reproducen sin cambios; los bucles de herramientas entre protocolos usan una caché de firmas en memoria (30 minutos, indexada por `tool_use.id`), y el razonamiento se desactiva en vez de fabricar una firma cuando no hay coincidencia
- `/v1/models` expone `capabilities` / `supported_parameters` / `reasoning` para modelos de razonamiento; que un cliente muestre un control de razonamiento sigue dependiendo de su propia tabla de capacidades

### 🛡️ Reintentos y cambio de backend

- Reintentos por ruta: `max_retries` y `backoff_ms` se ajustan en la interfaz; los tipos de error reintentados están fijados a 429 / 5xx / 408
- Por defecto se reintentan **429 / 5xx / 408 (respuestas de timeout del upstream)** con backoff exponencial (500 ms × 2ⁿ)
- Cada intento vuelve a pasar por el balanceador; con turno rotatorio suele ir al siguiente backend
- Las peticiones en streaming usan un `stream_timeout_secs` distinto y más amplio (1800 s por defecto), para que las generaciones largas no se corten por el tiempo de espera normal

> Los errores de conexión (backend inalcanzable, fallo de conexión, timeout de red) no se reintentan por defecto, y no hay expulsión guiada por health-check. Si necesitas «cambiar automáticamente cuando un backend está caído», coloca los backends en un mismo modelo virtual con turno rotatorio / menos conexiones, o desactiva manualmente el backend que falla. Ver [Limitaciones conocidas](#-limitaciones-conocidas).

### 📊 Monitorización

Un perfil de rendimiento completo para cada modelo en granularidad de hora / día / mes / año:

- **Ocho tendencias**: peticiones / fallos / entrada / salida / latencia / TTFT / velocidad de salida (tok/s) / aciertos de caché
- **Matriz de rendimiento**: tasa de éxito, duración, velocidad y tasa de caché de todos los modelos de un vistazo, ordenados por la métrica elegida
- **Registro de peticiones**: entrada / entrada nueva / salida / lectura de caché / duración / TTFT / velocidad por petición, con filtros por modelo y estado y paginación
- **Tasa de caché**: `aciertos / (aciertos + entrada nueva)`, el beneficio real del caché de prefijo
- **Coste y cobertura**: coste por petición desde la tabla de precios y proporción de peticiones con precio; los modelos sin precio se marcan como tales

---

### 🧾 Detalles de solicitudes y retención

- **Una fila por solicitud**: ruta de entrada, URL de reenvío y modelo upstream por intento, cabeceras, cuerpo de solicitud/respuesta, tokens, TTFT, caché, coste y créditos; los reintentos y cambios de backend se ven uno a uno
- **Sin valores sensibles en la base de datos**: las cabeceras se capturan mediante una lista blanca; `Authorization` / `x-api-key` / `Cookie` siempre se excluyen; las credenciales en los parámetros de la URL de reenvío (`key` / `token` / `sig`, ...) se enmascaran
- **La captura de cuerpos es opt-in**: solicitud y respuesta tienen interruptores separados, con un límite de 64 KB por registro; los streams guardan el SSE upstream en bruto y las solicitudes entre protocolos guardan la respuesta upstream en bruto, no el resultado convertido
- **Retención**: desactivada o 30/90/180/365 días; se ejecuta al arrancar y una vez al día. El mínimo es 30 días — menos borraría uso que las cuotas mensuales de las claves todavía cuentan

### 🎟️ Presupuestos de créditos upstream

- **Presupuesto de plataforma + precio en créditos por modelo**: la plataforma puede definir un presupuesto natural diario/semanal/mensual (por ejemplo, ModelScope 250 créditos/día) y el modelo un precio por solicitud (1 o 2 créditos)
- **Enrutado consciente de créditos**: las plataformas con saldo insuficiente se omiten; cuando todas se agotan, la puerta de enlace devuelve 429 con la próxima hora de reinicio en lugar de usar en silencio una cuenta por encima del límite
- **Libro mayor separado**: cargos, liberaciones, ajustes y reinicios tienen su fila; purgar el historial de solicitudes nunca “devuelve” créditos ya consumidos
- **Visible y ajustable**: el registro de solicitudes muestra los créditos de cada una; la lista/detalle de plataforma muestra restante/total y modelos sin precio; los créditos bajos/agotados llegan al centro de notificaciones y se pueden ajustar manualmente

## 🧱 Limitaciones conocidas

Estos son límites de implementación fáciles de sobreestimar:

1. **«Latencia» es turno rotatorio**: la opción existe, pero la selección sigue usando turno rotatorio y la latencia no entra en el orden (`src/lb/mod.rs`).
2. **«Prioridad» no degrada al fallar**: siempre usa el backend activo con el menor valor de `priority`; un reintento lo vuelve a elegir y los backends de menor prioridad quedan inactivos.
3. **Sin expulsión guiada por health-check**: las comprobaciones de conectividad solo sirven para mostrar y alertar y no afectan al enrutado; el campo `fallback` de la ruta no está conectado al camino de la petición.
4. **Los errores de conexión no se reintentan por defecto**: solo 429 / 5xx / 408; los timeouts de red y los fallos de conexión cuentan como `ConnectionError` y no están en la lista.
5. **Se sigue necesitando una clave aunque no haya ninguna configurada**: sin claves API, la pasarela no valida el valor, pero la petición debe llevar igualmente una cabecera `Authorization` o `x-api-key` (el valor no se valida); en cuanto existe una clave, se exige una clave válida.
6. **«Menos conexiones» termina en las cabeceras de respuesta**: el contador se libera en cuanto llegan las cabeceras del upstream, así que no cubre la duración de un cuerpo en streaming y no es un recuento real de concurrencia de streaming.
7. **Los cuerpos de solicitud/respuesta están desactivados por defecto**: dos interruptores separados con un límite de 64 KB por registro; los detalles de respuesta guardan la respuesta upstream en bruto, los streams solo los primeros 64 KB, y las solicitudes entre protocolos no son el resultado convertido que vio el cliente.
8. **El auto-purge dura al menos 30 días**: desactivado, la purga es manual; borrar solo libera páginas de SQLite y no reduce el archivo (ejecuta `VACUUM` aparte).
9. **Los créditos son un libro mayor local**: se reservan/confirman al tener éxito y se liberan al fallar, con ajuste manual; las ventanas usan el día/semana/mes natural local del servidor y pueden diferir de la factura upstream.

---

## 🆚 Comparación

| | AI Gateway | Proxy inverso Nginx | One API | LiteLLM |
|---|---|---|---|---|
| **Forma** | App de escritorio + servidor mono-binario | Proxy inverso generalista | Servidor | Servicio Python |
| **Gestión visual** | ✅ Consola integrada | ❌ | ✅ Consola web | ⚠️ Hay que activar la Admin UI |
| **Balanceo de carga** | ✅ 4 estrategias efectivas + opción de latencia | ✅ Turno rotatorio / hashing de upstream | ⚠️ Básico | ✅ |
| **Protocolo OpenAI** | ✅ | ✅ Reenvío | ✅ | ✅ |
| **Protocolo Anthropic** | ✅ Nativo | ❌ Solo reenvío, sin conversión | ✅ Conversión | ✅ Conversión |
| **Reintentos ante fallos** | ✅ 429 / 5xx / 408 | ⚠️ Requiere `proxy_next_upstream` | ✅ | ✅ Reintentos + fallbacks |
| **Lista de modelos remota** | ✅ Se obtiene al probar la conexión | ❌ | ❌ | ⚠️ Según configuración |
| **App de escritorio** | ✅ macOS / Windows / Linux | ❌ | ❌ | ❌ |
| **Implementación** | Rust | C | Go | Python |

> Basado en la documentación pública y la forma por defecto de cada proyecto; consulta el proyecto correspondiente para más detalle.

---

## ✨ Más funciones

- 🏪 **15 presets de plataformas**: OpenAI · Anthropic · DeepSeek · NVIDIA · Moonshot · ZhipuAI · Qwen · Doubao · SiliconFlow · Groq · Google Gemini · Together AI · OpenRouter · Ollama · Azure OpenAI. Los presets rellenan la URL base y la lista de modelos; la clave la sigues aportando tú. También se puede añadir a mano cualquier endpoint compatible con OpenAI / Anthropic
- 🔍 **Sincronización de modelos remotos**: «Probar conexión» obtiene la lista de modelos del upstream; la sincronización solo añade los modelos que faltan en local y no sobrescribe precios, capacidades ni nombres existentes
- 🏷️ **Insignias de capacidades**: razonamiento / visión / embedding / rerank / herramientas, editables por modelo y en lote; las insignias de un modelo virtual son la intersección de las capacidades de sus backends
- 🔗 **Compatibilidad con reasoning_content**: salidas de cadena de pensamiento de plataformas como NVIDIA NIM; parámetros de razonamiento OpenAI / Anthropic traducidos en ambos sentidos
- 📡 **Cabeceras reenviadas personalizadas**: añade cabeceras arbitrarias por plataforma, sobrescribiendo `Authorization` / `x-api-key` cuando coinciden
- 🎛️ **Playground**: elige un modelo y un protocolo, envía una petición real para verificar el enrutado y el upstream, crea una clave temporal o copia el cURL equivalente
- 🔌 **Acceso de clientes**: toma y restauración con un clic de las configuraciones locales de Claude Code / Codex / OpenCode / OpenClaw / Pi / Hermes; solo se tocan los campos gestionados por AI Gateway, cada aplicación se respalda, con vista previa del diff y editor de configuración sin formato
- 🌙 **Temas claro / oscuro / sistema**; interfaz en chino e inglés
- 🧭 **Ocho páginas**: Tráfico · Monitorización · Modelos virtuales · Plataformas · Claves API · Playground · Acceso de clientes · Ajustes. En la página Tráfico, los filtros, la paginación y los rangos de tiempo viven en la URL, así que actualizar y navegar adelante/atrás conservan el estado
- 🧾 **Trazas de intentos y repetición**: cada intento de backend de una petición queda registrado; la retención de cuerpos de petición está desactivada por defecto y, una vez activada, una petición guardada puede repetirse tal cual
- 🔔 **Centro de notificaciones**: claves que caducan, fallos repetidos de plataformas, excesos de cuota y copias de seguridad obsoletas se guardan y son accesibles desde la campana de la cabecera
- 💰 **Estimación de costes y presupuestos**: coste por petición desde la tabla de precios con ratio de cobertura; las cuotas de clave admiten ventanas deslizantes y rechazo al superar el límite
- 🔑 **Cuotas por clave**: caducidad, límite de peticiones, límite de tokens, presupuesto de coste y ventana (de por vida / día / semana / mes) por clave, opcionalmente vinculada a un modelo virtual
- 🗄️ **Copia de seguridad y restauración de la base**: snapshot, descarga y borrado; restaurar en el sitio conserva antes un snapshot de seguridad adicional
- 📤 **Configuración y CSV**: la exportación de configuración excluye los secretos upstream por defecto (se activa explícitamente si hace falta) y muestra una vista previa antes de importar; el tráfico se exporta a CSV con los filtros activos; los precios de plataforma se importan y exportan como CSV
- ⚡ **Actualización en vivo**: las nuevas peticiones y alertas llegan por SSE, con vuelta a sondeo cada 30 segundos si se cae la conexión
- 🖥️ **Cerrar la ventana de escritorio no detiene la pasarela**: solo destruye la WebView; al restaurar desde la bandeja se vuelve a la página que estaba abierta

---

## 🔐 Autenticación

El panel (tráfico y estadísticas, modelos virtuales, plataformas, claves API, Playground, acceso de clientes, ajustes) requiere iniciar sesión:

- **Primer arranque**: el panel abre «Crear cuenta de administrador». La inicialización solo acepta peticiones desde la máquina local (127.0.0.1), para que un par de la LAN no pueda reclamar la cuenta primero
- **Sesiones**: guardadas en una cookie HttpOnly SameSite=Strict, válida 7 días. Cerrar sesión desde el menú de usuario arriba a la derecha
- **Cambiar contraseña**: Ajustes → Seguridad de la cuenta. Los demás dispositivos se desconectan al instante; el dispositivo actual recibe una sesión nueva
- **Olvidé la contraseña**: detén la aplicación, borra las filas de las tablas `users` y `sessions` y reinicia para volver a ejecutar la inicialización

| Ruta | Autenticación |
|---|---|
| `/v1/chat/completions`, `/v1/completions`, `/v1/messages` | Clave API de la pasarela (`Authorization: Bearer ...` o `x-api-key`) |
| `/v1/models` | Pública |
| `/health` | Pública |
| `/api/auth/status`, `/api/auth/login`, `/api/auth/setup` | Pública (`setup` queda de hecho limitado a la máquina local) |
| El resto de `/api/*` (plataformas / modelos / modelos virtuales / claves API / acceso de clientes / ajustes / estadísticas / copias) | Sesión de administrador |

> Sin ninguna clave API configurada, la pasarela no valida el valor, pero las peticiones deben llevar igualmente una cabecera `Authorization` o `x-api-key` (el valor no se valida); en cuanto existe una clave, se exige una clave válida. Las credenciales se guardan en SQLite (hashes Argon2id); `admin_token` en `config.toml` es un campo histórico reservado.

---

## 🚀 Inicio rápido

### Opción 1: descargar la app de escritorio (recomendado)

Obtén el instalador de tu plataforma desde [Releases](https://github.com/keiskeies/ai-gateway/releases). La app de escritorio está disponible para macOS (arm64 / x64), Windows x64 y Linux x64.

### Opción 2: compilar desde el código fuente

```bash
git clone https://github.com/keiskeies/ai-gateway.git
cd ai-gateway

# Compila primero el frontend; la salida va a ../static/
cd frontend && npm install && npm run build
cd ..

# Servidor autónomo
cargo run
# Abre http://localhost:1994

# O la app de escritorio Tauri
cargo install tauri-cli
cargo tauri dev
```

> El modo servidor sirve `static/` directamente, así que compila el frontend antes de ejecutar desde el código fuente. La compilación del frontend vacía `static/`.

### Cuatro pasos

```
0️⃣ Crear la cuenta de administrador → iniciar sesión
1️⃣ Añadir una plataforma → elegir preset + introducir la clave API → guardar
2️⃣ Crear un modelo virtual → elegir estrategia + añadir backends → activar
3️⃣ Cambiar base_url → apuntar a http://localhost:1994/v1 → listo
```

Para combinar varias claves: repite el paso 1 con cada clave del mismo proveedor y luego añade todas esas plataformas al mismo modelo virtual en el paso 2.

---

## 📖 Uso

```python
# SDK OpenAI — cambia solo base_url
from openai import OpenAI
client = OpenAI(
    base_url="http://localhost:1994/v1",  # ← cambia esta línea
    api_key="your-gateway-key"            # ← clave API creada en la pasarela
)
response = client.chat.completions.create(
    model="your-virtual-model",           # ← nombre del modelo virtual
    messages=[{"role": "user", "content": "hello"}]
)

# SDK Anthropic — misma idea
import anthropic
client = anthropic.Anthropic(
    base_url="http://localhost:1994",     # ← cambia esta línea (el SDK añade /v1/messages)
    api_key="your-gateway-key"
)
response = client.messages.create(
    model="your-virtual-model",
    max_tokens=1024,
    messages=[{"role": "user", "content": "hello"}]
)
```

<details>
<summary>🔧 Ejemplos cURL</summary>

```bash
# Compatible con OpenAI
curl http://localhost:1994/v1/chat/completions \
  -H "Authorization: Bearer YOUR_GATEWAY_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"your-virtual-model","messages":[{"role":"user","content":"hello"}]}'

# Compatible con Anthropic
curl http://localhost:1994/v1/messages \
  -H "x-api-key: YOUR_GATEWAY_KEY" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{"model":"your-virtual-model","messages":[{"role":"user","content":"hello"}],"max_tokens":1024}'

# Lista de modelos virtuales (pública)
curl http://localhost:1994/v1/models
```

</details>

---

## 🎯 Escenarios típicos

| Escenario | Cómo | Resultado |
|-----------|------|-----------|
| **Combinar niveles gratuitos** | Una plataforma por cuenta, todas en un modelo virtual, turno rotatorio | El rendimiento se suma dentro del límite de cada clave |
| **Controlar costes** | Dirigir el volumen alto a un modelo virtual respaldado por un modelo más barato y cambiar al premium cuando haga falta | Los modelos caros solo atienden las peticiones que lo necesitan |
| **Estabilidad** | Varios backends detrás de un modelo virtual y reintentos en 429 / 5xx / 408 | Los límites temporales y los altibajos del upstream se absorben |
| **Mezclar plataformas** | Un modelo virtual para OpenAI, otro para Claude y otro para Qwen | Un solo SDK y una sola URL base para todos |
| **Compartir con un equipo** | Una pasarela más cuotas y vinculación de modelo por clave | Las credenciales upstream siguen privadas; el uso es trazable |

---

## 🏗️ Arquitectura

```
┌──────────────────────────────────────────────────┐
│                    AI Gateway                     │
│                                                  │
│  ┌──────────┐  ┌───────────┐  ┌──────────────┐  │
│  │  OpenAI  │  │ Anthropic │  │  Admin Web   │  │
│  │ Endpoint │  │ Endpoint  │  │   Consola    │  │
│  └─────┬────┘  └─────┬─────┘  └──────────────┘  │
│        │              │                           │
│  ┌─────▼──────────────▼─────┐                     │
│  │ Conversión · Enrutado ·  │                     │
│  │ Reintentos               │                     │
│  └──┬────┬────┬────┬────┬──┘                     │
│     │    │    │    │    │                         │
│  ┌──▼─┐┌─▼──┐┌▼───┐┌▼──┐┌▼────┐                │
│  │Deep││Qwen││GLM ││GPT││Claude│  ← N backends  │
│  │Seek││    ││    ││-4o││     │                   │
│  └────┘└────┘└────┘└───┘└─────┘                │
└──────────────────────────────────────────────────┘
```

---

## ⚙️ Configuración

`config.toml` se crea en el primer arranque; los cambios hechos en Ajustes se escriben de vuelta en ese archivo. Un archivo existente nunca se sobrescribe.

```toml
[server]
host = "0.0.0.0"
admin_port = 1994
log_level = "info"

[database]
path = "data/ai-gateway.db"

[security]
encrypt_key = "ai-gateway-default-key-change-me"  # cámbiala
admin_token = ""                                  # campo histórico reservado

[defaults]
lb_strategy = "RoundRobin"
max_retries = 2
retry_backoff_ms = 500
request_timeout_secs = 120
stream_timeout_secs = 1800
test_connection_timeout_secs = 10
capture_request_bodies = false
```

> Los cambios de puerto y dirección de escucha requieren reiniciar; el resto de ajustes se aplican al instante. `capture_request_bodies` controla si se guardan los cuerpos de petición en la base y está desactivado por defecto.

---

## 🛠️ Stack técnico

| Capa | Tecnología |
|---|---|
| Backend | Rust 2021 · Actix-Web 4 · Reqwest |
| Base de datos | SQLite (rusqlite con `bundled`, sin necesidad de SQLite del sistema) · pool r2d2 |
| Frontend | React 19 · TypeScript 6 · Mantine 7 · Recharts · Vite 8 |
| Escritorio | Tauri 2 |

---

## 🗺️ Hoja de ruta

- [x] 📊 Monitorización: tendencias por modelo / registro de peticiones / tasa de caché
- [x] 🔑 Cuotas detalladas por clave API: vinculación a modelo · caducidad · límites de peticiones / tokens / coste · ventanas deslizantes
- [x] 🔔 Comprobaciones de conectividad de plataformas y centro de notificaciones (solo visualización y alertas; aún no se usan para el enrutado)
- [ ] ⏱️ Conectar una medición real de latencia a «Latencia»
- [ ] 🩺 Expulsión guiada por health-check y degradación de prioridad
- [ ] 🧭 Conectar el campo `fallback` de la ruta al camino de la petición
- [ ] 🌐 Pasarela multimodal (endpoints de traducción / TTS / embedding)
- [ ] 🐳 Despliegue con Docker en un comando
- [ ] ☁️ Sincronización de configuración en la nube

> ¿Quieres una función? [Abre una issue](https://github.com/keiskeies/ai-gateway/issues).

---

## 🤝 Contribuir

1. Haz un fork de este repositorio
2. Crea una rama de función (`git checkout -b feature/amazing-feature`)
3. Haz commit de tus cambios (`git commit -m 'Add some amazing feature'`)
4. Sube la rama (`git push origin feature/amazing-feature`)
5. Abre una pull request

---

## 📂 Estructura del proyecto

```
ai-gateway/
├── src/                  # Backend Rust (biblioteca + binario autónomo)
│   ├── api/              # API REST y de administración
│   ├── db/               # Capa de base de datos (pool r2d2)
│   ├── proxy/            # Handlers de proxy, conversión de streaming, caché de firmas
│   ├── lb/               # Selector de balanceo de carga
│   ├── protocol/         # Adaptadores OpenAI / Anthropic y parámetros de razonamiento
│   ├── clients/          # Toma de configuración de clientes locales (Claude Code / Codex / …)
│   └── models/           # Modelos de datos
├── frontend/             # Frontend React (compila a ../static/)
├── src-tauri/            # Envoltorio de escritorio Tauri
├── e2e/                  # Pruebas end-to-end con Playwright + pytest
├── doc/                  # Capturas de pantalla
├── config.toml           # Configuración de ejecución
└── data/                 # Base SQLite y copias de seguridad
```

---

## ⭐ Historial de estrellas

<p align="center">
  <a href="https://star-history.com/#keiskeies/ai-gateway&Date">
    <img src="https://api.star-history.com/svg?repos=keiskeies/ai-gateway&type=Date" alt="Star History Chart" width="600" />
  </a>
</p>

---

## 📜 Licencia

[MIT License](LICENSE)

---

<p align="center">
  ¿Te resulta útil? Deja una <a href="https://github.com/keiskeies/ai-gateway">⭐ estrella</a>.
</p>
