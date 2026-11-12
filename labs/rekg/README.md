# rekg

Laboratorio con Amazon Rekognition: detecta las etiquetas de una imagen y el
texto de otra, ambas en S3. Cada corrida queda registrada en
[Langfuse](https://langfuse.com) a través del CLI de Langfuse de este
repositorio ([`langfuse-cli`](../../langfuse-cli/README.md)).

```sh
uv run main.py
```

Hacen falta credenciales de AWS con permiso para Rekognition y para leer el
bucket, y las de Langfuse en `.env`.

## El CLI de Langfuse

`bin/langfuse` es el CLI del proyecto. Ejecuta el binario estático de
`../../langfuse-cli/dist` que corresponde a la máquina
(`langfuse-aarch64-unknown-linux-musl` en ARM64) y le pasa el `.env` de este
directorio con `--env`, así que no hay que exportar nada:

```sh
bin/langfuse api health get
bin/langfuse api help
```

- El CLI lee de `.env` las variables `LANGFUSE_PUBLIC_KEY`,
  `LANGFUSE_SECRET_KEY` y `LANGFUSE_BASE_URL`, que tienen precedencia sobre las
  del entorno. `.env` no se versiona.
- `LANGFUSE_CLI=/ruta/al/binario` usa otro binario.

## Qué se envía a Langfuse

`main.py` abre una traza (`rekg.tracing.Trace`) y, al terminar, la envía con el
CLI (`rekg.langfuse_cli`):

| Qué | Comando | Contenido |
|---|---|---|
| Traza `rekg`, con el tag `rekg` | `langfuse api otel export-traces` | Observación raíz: el bucket y el umbral de confianza como entrada; las etiquetas y las líneas de texto como salida |
| Observaciones `rekognition.detect_labels` y `rekognition.detect_text` | (la misma petición) | La petición a Rekognition como entrada, los resultados mostrados con su confianza como salida, y `request_id` y `model_version` como metadatos |
| Score `confidence` de cada observación | `langfuse api scores create` | La confianza media (%) de sus resultados |

El texto detectado en las imágenes sale, por lo tanto, hacia el proyecto de
Langfuse de las credenciales.

Al final el programa imprime la URL de la traza. Si el CLI falla (faltan las
credenciales, no hay red, la API rechaza la petición), el programa muestra lo
mismo que sin Langfuse, termina con código 0 y dice el motivo por la salida de
errores. Una excepción dentro de una observación queda en la traza con nivel
ERROR.

Para ver las corridas desde la terminal:

```sh
# Las últimas corridas
bin/langfuse api observations list --name rekg --limit 5 \
  --fields core,basic --format table --columns traceId,startTime,latency,level

# Las observaciones de una traza y sus scores
bin/langfuse api observations list --trace-id <traceId> \
  --fields core,basic,io --format table --columns name,level,latency,output
bin/langfuse api scores list --trace-id <traceId> \
  --fields details,subject --format table --columns name,value,subject.id,comment
```

Langfuse tarda unos segundos en mostrar una traza recién enviada, y algo más
sus scores.

## Pruebas

```sh
uv run python -m unittest discover -s tests
```

No usan la red: un CLI falso anota lo que `rekg` le envía.
