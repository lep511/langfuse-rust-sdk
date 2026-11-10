# Verificación: ¿usa el SDK solo la plataforma Langfuse v4?

Fecha: 2026-11-10 · SDK: `langfuse-rust-sdk` **v0.10.1** (commit `0233ed8`) · Servidor comprobado:
Langfuse Cloud US (`/api/public/health` → `"version":"4.78.0"`). Las correcciones (H1, H2, H5
y H7) se publicaron en **v0.10.2**.

Referencia: el changelog [Langfuse v4](https://langfuse.com/changelog/2026-08-17-langfuse-v4)
(2026-08-17) y las páginas que enlaza:

- [Versions & Compatibility](https://langfuse.com/docs/compatibility)
- [Upgrade to Langfuse v4](https://langfuse.com/faq/all/upgrade-to-langfuse-v4)
- [Deprecated API migration](https://langfuse.com/faq/all/deprecated-api-migration)
- [Self-hosting v3 → v4](https://langfuse.com/self-hosting/upgrade/upgrade-guides/upgrade-v3-to-v4)
- [Public API](https://langfuse.com/docs/api-and-data-platform/features/public-api)
- [Migrate custom ingestion to Langfuse v4](https://langfuse.com/integrations/native/opentelemetry/migration-to-v4)
  (checklist de ingesta v4) y la FAQ
  [How to update traces, observations, and scores?](https://langfuse.com/faq/all/tracing-data-updates),
  revisadas el 2026-11-10 en una segunda pasada
- [Python SDK v4 → v5](https://langfuse.com/docs/observability/sdk/upgrade-path/python-v4-to-v5).
  Python SDK v5 es el SDK oficial "v4-only".

## Veredicto

**Sí: el SDK habla solo con APIs de Langfuse v4.** No llama a ningún endpoint que v4 elimine:

- ni a la ingesta legacy;
- ni a las lecturas v1 de trazas, observaciones, sesiones, scores o métricas;
- ni a los dataset runs.

Toda la ingesta de trazas va por OTLP con la cabecera `x-langfuse-ingestion-version: 4`.

Hubo **una divergencia**, ya corregida (H1): el ejemplo `examples/media.rs` usaba un parámetro que v4
rechaza con 400. Lo demás son observaciones sin impacto (H2 a H6). La recomendación H5 (acotar la
ventana de tiempo en Observations v2) está aplicada desde el 2026-11-10.

La segunda pasada, con la guía de ingesta v4, confirma su checklist punto por punto con un matiz
(H7): la documentación del SDK decía que reintentar una exportación OTLP no duplica spans, y en v4
sí puede duplicarlos. Se corrigió la documentación. Los reintentos no cambian, y el riesgo es el
mismo que en Python SDK v5.

Con las correcciones aplicadas (H1, H2, H5 y H7; última ejecución el 2026-11-10) pasan los 708
tests de `cargo test --all-features` (3 ignorados) y los 531 de `cargo test` (2 ignorados),
`cargo clippy --all-targets --all-features -- -D warnings` sale limpio y compilan todos los
ejemplos (`cargo build --examples --all-features`). Los ejemplos modificados que escriben datos
(`media`, `comments`, `annotation_queues`, `batch_evaluation` y `otel_export_options`) no se
ejecutaron contra la API real; `observations`, que solo lee, sí. También se comprobó en solo
lectura que v4 devuelve `input` como cadena y que aplica las dos formas de acotar la ventana (H5).

## Qué exige Langfuse v4

Datos de las fuentes de arriba:

- **Fecha de corte.** Langfuse Cloud pasa a ser solo v4 el **2026-11-16**: ese día se eliminan las
  APIs, funciones e ingesta legacy. Self-hosted v3 recibe parches de seguridad hasta enero de 2027.
  En self-hosted v4 con `LANGFUSE_MIGRATION_V4_WRITE_MODE=events_only` (el valor por defecto), los
  endpoints deprecados ya devuelven 404.
- **Ingesta.** Trazas solo por `POST /api/public/otel/v1/traces` (OTLP/HTTP), con
  `x-langfuse-ingestion-version: 4`. Sin la cabecera, los datos llegan con hasta 15 minutos de
  retraso.
  - `POST /api/public/ingestion` solo acepta eventos `score-create`.
  - `POST /api/public/traces`, `POST`/`PATCH /api/public/spans`,
    `POST`/`PATCH /api/public/generations` y `POST /api/public/events` devuelven 404.
- **Lecturas eliminadas:**
  - `GET /api/public/traces` y `GET /api/public/traces/{id}`
  - `GET /api/public/observations` y `GET /api/public/observations/{id}`
  - `GET /api/public/sessions` y `GET /api/public/sessions/{id}`
  - `GET /api/public/scores` (+ `/{id}`) y `GET /api/public/v2/scores` (+ `/{id}`)
  - `GET /api/public/metrics` y `GET /api/public/metrics/daily`
  - `GET`/`DELETE /api/public/datasets/{name}/runs/{runName}`, `GET /api/public/datasets/{name}/runs`
  - `GET`/`POST /api/public/dataset-run-items`
- **APIs de v4:**
  - Observations API v2 (`GET /api/public/v2/observations`)
  - Metrics API v2 (`GET /api/public/v2/metrics`)
  - Scores API v3 (`GET /api/public/v3/scores`)
  - Experiments API (`GET /api/public/experiments`, `GET /api/public/experiment-items`)
- **Endpoints que siguen en v4:**
  - `POST /api/public/scores` y `DELETE /api/public/scores/{id}`
  - `DELETE /api/public/traces` y `DELETE /api/public/traces/{id}`
  - `/v2/datasets` y `dataset-items`
  - comments, annotation queues, media, prompts, models, projects, organizations, SCIM,
    llm-connections y blob storage
- **Modelo de datos.** Una tabla de observaciones desnormalizada e inmutable. Los atributos de
  traza (`user_id`, `session_id`, …) se deben propagar **en el cliente** a cada observación: "Langfuse
  v4 expects instrumentation that propagates shared attributes (like userId and sessionId) on the
  client side".
- **Cambios de Python SDK v5 que reflejan el modelo v4:**
  - Fuera `set_trace_io`, `api.trace`, `api.sessions`, `api.legacy.*`, los dataset runs,
    `api.ingestion.batch` y `blocked_instrumentation_scopes`.
  - Scores por `POST /api/public/scores` en lote.
  - Metadata JSON-codificada, incluidas las cadenas.
  - Filtro GenAI con el prefijo `gen_ai.` (punto incluido).
  - Exportación gzip por defecto.

## Comprobación requisito por requisito

| # | Requisito v4 | Estado en el SDK | Evidencia |
|---|---|---|---|
| 1 | Ingesta OTLP con `x-langfuse-ingestion-version: 4` | ✅ Siempre la envía: `ingest_traces` y el exportador del tracer. También con `traces_export_path` cambiado | `src/api/ingestion.rs` (`INGESTION_VERSION`, `post_traces`); `src/otel/exporter.rs:368`; tests `api_ingestion_tests::ingest_traces_sends_ingestion_version_header`, `otel_tests.rs:215` |
| 2 | Sin ingesta legacy (`/ingestion`, `/traces` POST, `/spans`, `/generations`, `/events`) | ✅ No implementada: `models/ingestion.rs` solo tiene tipos OTLP | `grep` sin resultados en `src/`, `tests/`, `examples/` y `README.md` |
| 3 | Sin lecturas v1 (traces, observations, sessions, scores v1/v2, metrics v1/daily) | ✅ Ninguna. `traces` solo expone los dos `DELETE`. `scores` expone `POST`, `DELETE` y `GET /v3/scores` | `src/api/traces.rs`, `src/api/scores.rs`, `src/api/observations.rs`, `src/api/metrics.rs` |
| 4 | Sin dataset runs ni dataset-run-items | ✅ Ninguno. `DatasetWithItems` no tiene runs | `src/api/datasets.rs`, `src/dataset.rs` |
| 5 | Observations API v2, Metrics API v2, Scores API v3, Experiments API | ✅ Las cuatro. `ListExperimentsParams`/`ListExperimentItemsParams` exigen `from_start_time: String`, que el servidor exige (400 sin él, comprobado) | `src/api/{observations,metrics,scores,experiments}.rs` |
| 6 | Metrics v2 sin la vista `traces` | ✅ `MetricsView` solo tiene `observations` y `scores-*`. `Traces` existe solo en `DashboardWidgetViewWithLegacy` (respuestas de widgets *unstable*), para leer widgets antiguos, como indica la spec | `src/models/metrics.rs:6-14`, `src/models/unstable_dashboard_widgets.rs:18-26` |
| 7 | Atributos de traza propagados en el cliente a cada observación | ✅ `user.id`, `session.id`, `langfuse.trace.name`/`tags`/`metadata`, versión y release se fijan solo con `propagate_attributes`. El procesador los copia a cada span en `on_start`. No hay `set_trace_io` ni setters de traza por span. `set_trace_as_public` existe, como en Python | `src/otel/propagation.rs:206,285-308,456`, `src/otel/processor.rs:341-346`, `src/otel/observation.rs:532` |
| 8 | Scores por `POST /api/public/scores`, en lote (no `score-create` vía `/ingestion`) | ✅ La cola de fondo y `scores.create` envían un `[CreateScoreRequest]` | `src/background.rs:291`, `src/api/scores.rs:93` |
| 9 | Experimentos sin dataset-run-items, ligados por OTel | ✅ `run_experiment` solo exporta spans con `langfuse.experiment.*` (dataset, item, versión) y crea scores. Los scores del run llevan `datasetRunId` = ID del experimento (campo vigente en la spec v4). La URL apunta a `/experiments/results` | `src/otel/propagation.rs:239-240`, `src/otel/experiment.rs:200-213` |
| 10 | Evaluación por lotes solo con Observations v2 | ✅ Bucle de cursor sobre `observations.list_with_options`. No tiene `scope` de trazas ni campos de traza | `src/batch_evaluation.rs:558` |
| 11 | Paridad con Python v5 (gzip, metadata, filtro `gen_ai.`, sin `blocked_instrumentation_scopes`) | ✅ gzip por defecto. Metadata con valores JSON (`"x"` entre comillas). Filtro `starts_with("gen_ai.")`. La opción de scopes bloqueados no existe | `src/otel/options.rs:216`, `src/otel/attributes.rs:136-143,189-228`, `src/otel/processor.rs:78-82` |
| 12 | Spec incluida sin rutas legacy | ✅ `api/openapi.yml` (120 operaciones) no tiene ninguna ruta eliminada en v4. Su cabecera exige `x-langfuse-ingestion-version: 4` | `api/openapi.yml:1-20`, `api/operation-coverage-audit.md` |

## Checklist de ingesta v4

La guía [Migrate custom ingestion to Langfuse v4](https://langfuse.com/integrations/native/opentelemetry/migration-to-v4)
enumera lo que necesita una integración para el modelo de datos de v4 ("V4-ready ingestion
checklist"). Revisión del 2026-11-10:

| Requisito de la guía | Estado en el SDK | Evidencia |
|---|---|---|
| Trazas por OTLP/HTTP a `/api/public/otel/v1/traces` | ✅ También con `traces_export_path` cambiado | `src/api/ingestion.rs` (`OTEL_TRACES_PATH`, `post_traces`) |
| Basic Auth con las claves del proyecto y `x-langfuse-ingestion-version: 4` | ✅ | `src/config.rs:374`, `src/api/ingestion.rs:61` |
| Cada operación es un span OTel completo (trace ID, span ID, tiempos y padre) | ✅ Los spans los crea el SDK de OpenTelemetry | `src/otel/observation.rs` (`Observation::start`) |
| Input/output en su observación, y la petición y la respuesta globales en la raíz; sin `langfuse.trace.input`/`output` | ✅ `set_input`/`set_output` en cualquier observación; la raíz de cada item de experimento lleva su input y su output. `grep` sin resultados para `trace.input`/`trace.output` | `src/otel/observation.rs`, `src/otel/experiment.rs:367-370` |
| Contexto de traza copiado a cada span: usuario, sesión, nombre, tags, metadata, versión, release y entorno | ✅ `propagate_attributes` y el procesador en `on_start`. Entorno y release van además en cada observación y en el recurso | `src/otel/propagation.rs:276-310`, `src/otel/processor.rs:341-380`, `src/otel/observation.rs:340-351`, `src/otel/tracer.rs:170-176` |
| Metadata filtrable con `langfuse.trace.metadata.<key>` y `langfuse.observation.metadata.<key>` | ✅ La metadata se aplana por clave. Matiz: con el puente `tracing-layer`, los campos y atributos de `tracing` llegan sin mapear: se ven en la metadata, pero no se filtran | `src/otel/attributes.rs`, `src/otel/tracing_layer.rs:55-58` |
| Tipos de observación reconocidos | ✅ Los diez valores de la guía | `src/otel/observation.rs:57-68` (`ObservationType::as_str`) |
| Cada span se exporta una vez, sin re-ingestar el mismo ID para actualizarlo | ✅ con matiz: los atributos se acumulan y se escriben una sola vez al terminar, pero un reintento tras un fallo ambiguo puede reenviar un lote ya guardado (H7) | `src/otel/observation.rs:764` (`finish`), `src/api/ingestion.rs:70` |
| Se conserva una observación raíz aunque se filtren spans | ✅ El primer span exportado de la aplicación se marca como raíz | `src/otel/processor.rs:382-401` (`langfuse.internal.is_app_root`) |
| Evaluaciones sobre observaciones, no sobre la E/S de la traza | ✅ `run_batched_evaluation` lee Observations v2 y los experimentos puntúan spans | `src/batch_evaluation.rs`, `src/otel/experiment.rs` |

La traza canario de "Validate before cutover" no se envió, porque escribe datos. El ejemplo
`otel_export_options` hace una comprobación parecida: escribe una traza, la lee con
Observations v2 y la borra.

## Comprobaciones contra la API real

Peticiones `GET` de solo lectura con las credenciales de `.env` (Langfuse Cloud US, servidor
4.78.0, 2026-11-10, seis días antes del corte):

| Petición | Estado | Respuesta |
|---|---|---|
| `GET /api/public/v2/observations?fields=core&parseIoAsJson=true` | **400** | `parseIoAsJson=true is no longer supported on the v2 observations…` |
| `GET /api/public/v2/observations?fields=core` (o `parseIoAsJson=false`) | 200 | Normal |
| `GET /api/public/v2/observations?fields=core,io` | 200 | `input` es **siempre una cadena** (JSON serializado o texto), en 14 de 14 observaciones |
| `GET /api/public/traces` | 422 | `_deprecation: "Removed on 2026-11-16. Use GET /api/public/v2/observations?fields=core,basic,usage&fromStartTime=<from>&toStartTime=<to>…"` |
| `GET /api/public/observations` | 200 | Con `_deprecation: "Removed on 2026-11-16…"` |
| `GET /api/public/sessions` | 429 | Con `_deprecation: "Removed on 2026-11-16…"` |
| `GET /api/public/scores`, `GET /api/public/v2/scores` | 200 | Con `_deprecation: "Removed on 2026-11-16. Use GET /api/public/v3/scores…"` |
| `GET /api/public/metrics/daily` | 200 | Con `_deprecation: "…Use GET /api/public/v2/metrics?query=…"` |
| `GET /api/public/dataset-run-items` | 400 | Con `_deprecation: "…Use GET /api/public/experiment-items…; dataset run items are rep[laced]…"` |
| `GET /api/public/v3/scores` | 200 | Normal |
| `GET /api/public/experiments`, `GET /api/public/experiment-items` (sin `fromStartTime`) | 400 | `fromStartTime is required` |
| Las dos anteriores con `fromStartTime` | 200 | Normal |

El SDK no llama a ninguno de los endpoints que responden con `_deprecation`, así que el corte del
2026-11-16 no le afecta.

## Hallazgos

### H1 · Corregido · `parseIoAsJson=true` en `examples/media.rs` (fallaba con 400 en v4)

`examples/media.rs` leía la observación con `parse_io_as_json: Some(true)`. Tenía dos fallos en v4:

1. La petición devuelve **400** (comprobado arriba), así que el ejemplo terminaba con error.
2. Aun sin el parámetro, v4 devuelve `input` como **cadena**. El ejemplo indexaba
   `resolved[0]["content"][1]["image_url"]["url"]` sobre un `Value::String`, obtenía `Null` e
   imprimía una URL vacía.

Corrección:

- El ejemplo ya no envía el parámetro y parsea `input` con `serde_json::from_str` cuando es una
  cadena con JSON.
- El campo `ListObservationsParams::parse_io_as_json` (`src/api/observations.rs`) tiene ahora un
  doc comment: v4 rechaza `Some(true)`, devuelve la E/S como cadenas y el campo debe quedarse en
  `None`.

El campo sigue existiendo porque la spec v4 lo mantiene como parámetro (deprecado; `false` es
válido). Quitarlo rompería la API pública del crate. Si se quiere avisar en compilación, la
opción sería `#[deprecated]` en una versión menor, pero eso rompe a quien compile con
`-D warnings`.

`tests/api_observations_tests.rs` sigue usando `Some(true)`. Es un test de serialización contra
wiremock (comprueba que el SDK manda lo que se le pide), no una llamada real.

### H2 · Corregido (documentación) · Doc de módulo obsoleto en `src/api/ingestion.rs`

El módulo se describía como "Batch ingestion of traces, spans, generations, scores and events",
que es la ingesta legacy. Ahora describe la ingesta OTLP, la única vía de v4.

### H3 · Observación · Una vista `with_options` puede sustituir la cabecera de versión

`RequestOptions::headers` de una vista sustituye a las cabeceras por llamada (`src/http.rs:96-98`,
`headers()` en `src/http.rs:599-605`). Por eso
`client.with_options(RequestOptions::default().with_header("x-langfuse-ingestion-version", ..))`
puede enviar otro valor. Es intencional: es el rodeo del punto 31 del informe de brechas con el
SDK de Python (`python-sdk-gap-report.md`, una nota local que no está en el repositorio). Solo
pasa si el usuario lo pide explícitamente. `ConfigBuilder::header` **no** puede sustituirla.

### H4 · Observación · Un `span_exporter` propio no añade la cabecera

Con `TracingOptions::span_exporter` los spans van al exportador del usuario, y el SDK no puede
añadirle cabeceras. Está documentado en `src/otel/options.rs:185-198`: "When it sends to Langfuse,
add the header `x-langfuse-ingestion-version: 4`". Python se comporta igual con un exportador
propio.

### H5 · Aplicado (2026-11-10) · Pasar `from_start_time` en Observations v2

En la spec, `fromStartTime` y `toStartTime` son opcionales en `GET /api/public/v2/observations`, y
el SDK los modela como `Option`. Pero la guía de migración y los mensajes `_deprecation` del
servidor los presentan como parte de la consulta v4 ("add required `fromStartTime`/`toStartTime`").
Sin cota inferior, el servidor busca desde 2020-01-01. No es un incumplimiento, pero conviene
acotar la ventana por rendimiento, también en los ejemplos `observations`, `comments` y
`annotation_queues`. La guía lo pide explícitamente: "always set `fromStartTime` and `toStartTime`
to keep each request bounded".

Aplicado el 2026-11-10, sin cambiar la API pública: `from_start_time` sigue siendo
`Option<String>`, como en la spec.

- **Ejemplos.** Todos los que listan observaciones pasan ahora `from_start_time`, calculado con
  `chrono` en ISO 8601 con `Z` (`to_rfc3339_opts(SecondsFormat::Secs, true)`). No se pasa
  `to_start_time`: las ventanas acaban en el momento actual.
  - `observations`, `comments` y `annotation_queues`: las últimas 24 horas. Los dos últimos buscan
    la traza más reciente; si no hay ninguna en ese plazo, lo dicen y sugieren `LANGFUSE_TRACE_ID`.
  - `media`, `batch_evaluation` y `otel_export_options`: desde 5 minutos antes de crear su traza.
    `otel_export_options` no estaba en la lista original, pero tenía la misma consulta sin cota.
  - `batch_evaluation` acota también el filtro de `run_batched_evaluation` con una condición
    `startTime >=`.
- **Evaluación por lotes (solo documentación).** `run_batched_evaluation` no envía `fromStartTime`
  (solo `limit`, `cursor`, `fields` y `filter`), y según la spec el `filter` tiene prioridad sobre
  esos parámetros. Por eso la ventana va en el filtro, como condición `datetime` sobre `startTime`.
  Lo explican el doc de `BatchEvaluationOptions::filter` y el del módulo `batch_evaluation`, cuyo
  ejemplo lo muestra. El comportamiento no cambia.
- **Documentación.** `ListObservationsParams::from_start_time` y `to_start_time` tienen doc
  comments: inclusivo y exclusivo, ISO 8601, y una condición `startTime` en `filter` tiene
  prioridad. El README (paginación y evaluación por lotes) y el ejemplo de `src/pagination.rs`
  acotan la ventana.

Comprobado en solo lectura contra la API real (2026-11-10, `limit=50`, `fields=core`):

| Petición `GET /api/public/v2/observations` | Estado | Resultado |
|---|---|---|
| `fromStartTime=<hace 24 h>` (la forma de los ejemplos) | 200 | 15 filas, todas con `startTime` ≥ la cota |
| `fromStartTime=2099-01-01T00:00:00Z` | 200 | 0 filas: el parámetro se aplica |
| `filter` con `startTime >= <hace 24 h>` (la forma de la evaluación por lotes) | 200 | 15 filas, todas con `startTime` ≥ la cota |
| `filter` con `startTime >= 2099-01-01T00:00:00Z` | 200 | 0 filas: la condición se aplica |
| `filter` con `traceId =` y `startTime >=` (el filtro del ejemplo `batch_evaluation`) | 200 | Aceptado |
| `cargo run --example observations` (`fromStartTime` serializado por el SDK) | OK (sale con 0) | 10 observaciones de las últimas 24 h, con cursor a la página siguiente |

### H6 · Observación · Widgets *unstable* con vista `traces`

`DashboardWidgetViewWithLegacy::Traces` (solo en respuestas) sigue existiendo porque la API
unstable puede devolver widgets antiguos con esa vista. La spec dice: "Responses may include the
legacy `traces` value". Para crear o actualizar widgets se usa `DashboardWidgetView`, que no tiene
`traces`. Esto cumple v4.

### H7 · Corregido (documentación) · Los reintentos OTLP pueden duplicar observaciones en v4

La guía de ingesta v4 pide exportar cada span una sola vez. La FAQ
[How to update traces, observations, and scores?](https://langfuse.com/faq/all/tracing-data-updates)
explica por qué: "In Langfuse v4, ingested data is **not deduplicated on the read path**, and the
underlying storage does **not** guarantee consistency (not even eventual consistency) across
records that share an `id`". Reinsertar crea duplicados que inflan las métricas (por ejemplo,
`sum(totalCost)` en la Metrics API).

El SDK reintenta la ingesta OTLP como una petición idempotente (`idempotent: Some(true)` en
`src/api/ingestion.rs:70`): 408, 429, cualquier 5xx, timeouts y conexiones cortadas
(`src/retry.rs:77-82`, `src/http.rs:813`). La documentación lo justificaba así: "spans carry their
IDs, so a repeated export does not duplicate them" (`IngestionClient::ingest_traces` y
`RetryPolicy`). En v4 eso es falso: si el servidor ya había guardado el lote (por ejemplo, un
timeout después de aceptarlo), el reintento duplica sus observaciones.

Comparación con Python SDK v5.2.0, que exporta con el `OTLPSpanExporter` de OpenTelemetry 1.45.2:
reintenta 429, 502, 503 y 504, timeouts y errores de conexión, así que la exposición es la misma.
El SDK de Rust reintenta además 408 y el resto de 5xx (500, 501, …).

Corrección (solo documentación, decisión del 2026-11-10): los doc comments de
`IngestionClient::ingest_traces` y `RetryPolicy` describen ahora el riesgo. Los reintentos no
cambian, porque sin ellos un fallo transitorio perdería los spans. Queda como opción alinearlos con
el exportador de OpenTelemetry, es decir, dejar de reintentar 500, 501 y 505+.

Los scores no están afectados. `LangfuseClient::score` rellena `id` y `timestamp` antes de encolar
(`src/lib.rs:558-565`), así que un lote reintentado repite `id`, `name` y `timestamp`. Según la FAQ,
en ese caso el score se sobrescribe en lugar de duplicarse.

## Cómo volver a verificarlo

```bash
# 1. Ninguna ruta legacy en el código, los tests, los ejemplos ni el README
grep -rnE 'api/public/(ingestion|sessions|observations"|metrics"|metrics/daily|v2/scores|dataset-run-items|spans|generations|events|datasets/[^"]*/runs)' src tests examples README.md

# 2. La cabecera de v4 en la ingesta
cargo test --test api_ingestion_tests
cargo test --features otel --test otel_tests

# 3. Suite completa y lints
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings

# 4. Contra la API real (solo lectura; desde el 2026-11-16 los endpoints legacy deberían dar 404)
curl -s -u "$LANGFUSE_PUBLIC_KEY:$LANGFUSE_SECRET_KEY" "$LANGFUSE_BASE_URL/api/public/health"
curl -s -o /dev/null -w '%{http_code}\n' -u "$LANGFUSE_PUBLIC_KEY:$LANGFUSE_SECRET_KEY" \
  "$LANGFUSE_BASE_URL/api/public/v2/observations?limit=1&fields=core&parseIoAsJson=true"  # 400

# 5. La ventana de tiempo se aplica (solo lectura): con una cota futura, 200 y "data": []
curl -s -G -u "$LANGFUSE_PUBLIC_KEY:$LANGFUSE_SECRET_KEY" "$LANGFUSE_BASE_URL/api/public/v2/observations" \
  --data-urlencode "limit=1" --data-urlencode "fields=core" \
  --data-urlencode 'filter=[{"type":"datetime","column":"startTime","operator":">=","value":"2099-01-01T00:00:00Z"}]'
```
