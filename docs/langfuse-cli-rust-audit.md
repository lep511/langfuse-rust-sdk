# Auditoría de `langfuse-cli`: ¿aprovecha las ventajas de Rust?

Fecha: 2026-11-12 · Alcance: `langfuse-cli/` (crate `langfuse-rust-cli` 0.1.0, binario `langfuse`) en el commit `4289b18` · Toolchain: rustc 1.99.0, edición 2024.

## Veredicto

No todas. El CLI aprovecha muy bien el sistema de tipos para la gramática de comandos, los errores con código de salida, la seguridad de memoria y el perfil de compilación. Deja sin aprovechar cuatro cosas que Rust ofrece y que aquí tienen efecto medible o visible:

1. **Manejo de errores de E/S en la salida.** Cerrar la tubería produce un pánico (`langfuse completions bash | head -1` termina con exit 101) o un error con exit 1, según el comando. Es el único defecto funcional encontrado.
2. **Propiedad y serialización sin copias.** La salida se clona y se copia varias veces antes de escribirse. Con una respuesta de 52 MiB el pico de memoria es de 198 a 323 MiB; un prototipo que mueve en vez de clonar y serializa directo a stdout lo baja entre un 24 % y un 60 % con salida idéntica byte a byte.
3. **Tipos en lugar de cadenas y JSON dinámico.** El módulo `skills` (port del TypeScript oficial) usa estados como cadenas, relee su propio JSON por clave y analiza el lockfile a mano; el compilador no puede verificar nada de eso.
4. **Lints y CI.** No hay tabla `[lints]`, ni atributos de crate, ni CI en el repositorio; la calidad actual depende de ejecutar a mano los comandos de `CLAUDE.md`.

El resto de hallazgos son menores y están en la sección 4.

## 1. Método

- Lectura completa de `src/` salvo los ficheros de recursos de `src/api/`, de los que leí `mod.rs`, `prompts.rs`, `comments.rs` y `scores.rs` (los demás siguen el mismo patrón y los cubrí con la prueba dinámica de abajo).
- Comprobaciones ejecutadas, todas sobre el código actual:

| Comprobación | Resultado |
|---|---|
| `cargo clippy --all-targets --all-features -- -D warnings` | limpio |
| `cargo fmt --check` | limpio |
| `cargo test` | 81 pruebas, todas pasan |
| `cargo test --no-default-features` | 79 pruebas, todas pasan |
| `cargo audit --no-fetch` (base local con 1422 avisos) | 0 vulnerabilidades en 225 crates |
| `cargo doc --no-deps` | 1 aviso (URL sin enlace en el texto de ayuda de `--host`, `src/cli.rs:69`) |
| clippy `pedantic` + `nursery` sobre lib y bin | 238 avisos (desglose en 3.4) |
| `-W missing_docs` | 80 ítems públicos sin documentar (64 son los structs de argumentos de cada operación) |
| Binario release (aarch64, glibc) | 5 445 272 bytes |
| Arranque (mediana de 50 ejecuciones) | `--version` 2,6 ms; `api prompts list --curl` 2,8 ms; `api schema --json` 5,5 ms (`/bin/true` en el mismo arnés: 0,9 ms) |
| Las 120 operaciones en modo `--curl` | las 120 generan el método correcto y la ruta con todos sus parámetros en orden |
| Copia desechable compilada solo como binario | 0 ítems sin usar: el `pub` generalizado no esconde código muerto (subestimado: ver la corrección en la sección 7) |

- Las mediciones de memoria usan un servidor HTTP local que devuelve listas grandes y `/usr/bin/time` para el pico de memoria residente. El prototipo se hizo en una copia en `/tmp`, ya borrada; no cambié nada del repositorio.

## 2. Lo que sí aprovecha

**Gramática de comandos dirigida por tipos.** Cada operación es un struct con `#[derive(Args)]` que implementa `Operation`, cuyo `const META: OperationMeta` se construye con un builder `const fn` (`src/operation.rs:108-184`). Un `impl<T: Operation> ApiOperation for T` da la vista apta para `dyn` que usa el despacho (`src/operation.rs:231`). Las macros `resource!` y `resources!` generan a la vez el enum de subcomandos, el `match` de despacho y la lista de metadatos (`src/api/mod.rs:17-46`, `172-212`), así que registrar una operación en un sitio y olvidarla en otro no compila. `api schema --json` sale del mismo árbol de clap, sin una segunda fuente de verdad.

**Errores como valores con código de salida.** `Exit` es un enum `#[repr(u8)]` convertido a `ExitCode` (`src/error.rs:11-36`), `CliError` lleva el código y `From<LangfuseError>` permite propagar con `?`. En unas 10 800 líneas de código que no es de pruebas hay 0 `unwrap()`, 0 `unsafe`, 0 `todo!` y solo 2 `expect` (`src/paginate.rs:160`, `src/skills/api.rs:110`).

**Diseño testeable sin estado global.** `run(args, io)` recibe los flujos y el directorio de trabajo en `Io<'a>` (`src/output.rs:16-32`) y la configuración se resuelve con una función de consulta inyectada (`src/config.rs:42`). Por eso las 81 pruebas corren el CLI completo en proceso contra wiremock, sin tocar el entorno.

**Uso cuidadoso de la biblioteca estándar en `skills`.** `O_NOFOLLOW` al leer (`src/skills/local_skill.rs:291-300`), `create_new(true)` al escribir, `symlink_metadata` en vez de seguir enlaces, verificación de tamaño y SHA-256 de cada fichero descargado, validación de rutas contra escapes del directorio, escritura atómica del lockfile con `sync_all` y `rename`, lecturas acotadas con `take`, y escape de caracteres de control en todo texto externo.

**Compilación y dependencias.** Perfil release con LTO, `codegen-units = 1` y `strip`; tokio `current_thread` con solo `rt` y `macros`; reqwest sin features por defecto y con rustls. El arranque de 2,6 ms confirma que construir el árbol de clap dos veces en `command()` no cuesta nada apreciable.

**Rust moderno y concurrencia donde importa.** Edición 2024, cadenas `if let … && let …`, `let … else`, `is_none_or`. Las descargas de contenido van en lotes concurrentes con `buffered(4)` (`src/skills/api.rs:342-372`). El feature `unstable` se propaga por `cfg` dentro de las macros y ambas configuraciones compilan y pasan clippy.

## 3. Lo que no aprovecha (por prioridad)

### 3.1 Tubería cerrada: pánico o exit 1 (defecto)

Rust ignora `SIGPIPE`, así que escribir en una tubería cerrada devuelve `BrokenPipe` y cada sitio decide qué hacer. Hoy hay tres comportamientos distintos, comprobados con el binario release:

| Comando | Resultado |
|---|---|
| `langfuse completions bash \| head -1` | **pánico** dentro de `clap_complete` ("failed to write completion file"), exit 101 |
| `langfuse api schema --json \| head -c 100` | `Cannot write to stdout: Broken pipe (os error 32)`, exit 1 |
| `langfuse api --help \| head -1` | silencio, exit 0 |

El primero viene de pasar `io.stdout` directamente a `clap_complete::generate` (`src/lib.rs:150`), que hace `expect` sobre la escritura. El segundo, de `write_error` y `write_out` (`src/api/mod.rs:467`, `src/output.rs:78`). Los comandos `skills` descartan el error con `let _ = writeln!(…)` (`src/skills/install.rs:527`, `551`, `563`; `src/skills/push.rs:452`, `562`, `573`, `575`), de modo que tampoco avisan si la escritura falla por otra causa, como disco lleno al redirigir.

El exit 101 queda además fuera del contrato de códigos 0–7 que documenta `src/error.rs`. Lo mismo ocurriría con cualquier otro pánico, porque no hay `panic hook`.

Sugerencia: generar las completions en un `Vec<u8>` y escribirlas por el mismo camino que el resto; centralizar la escritura a stdout en una función que trate `ErrorKind::BrokenPipe` como fin silencioso y propague los demás errores; y un `panic hook` que deje el exit en 1.

### 3.2 Propiedad: clones y copias de toda la respuesta

El código de salida y de paginación es una traducción fiel del TypeScript, donde los objetos se comparten por referencia. En Rust eso se convirtió en `clone()` profundos del JSON:

- `src/paginate.rs:68-73` clona el cuerpo entero de cada página y después clona otra vez el array `data`; `82-87` clona `meta`. Nada vuelve a leer ese cuerpo.
- `src/output.rs:106-107` clona las cabeceras y el cuerpo para montar el sobre de `--json`.
- `src/output.rs:114`, `118` y `129` hacen `write_out(io, &format!("{text}\n"))`: copian toda la salida ya formateada solo para añadir un salto de línea.
- El sobre (`108`), `pretty()` (`85`) y `write_ndjson_items` (`145-152`) construyen un `String` completo antes de escribir.

Medición del pico de memoria, con salida idéntica byte a byte entre el binario actual y el prototipo (en `--json` solo difiere la cabecera `date` del servidor de pruebas):

| Modo | Actual | Prototipo | Diferencia |
|---|---|---|---|
| JSON legible, 1 página de 52 MiB | 260 MiB | 198 MiB | −24 % |
| `--json` (sobre) | 323 MiB | 198 MiB | −39 % |
| `--format ndjson` | 198 MiB | 198 MiB | 0 |
| `--format table` | 232 MiB | 232 MiB | 0 |
| `--all`, JSON legible, 10 páginas de 5,2 MiB | 287 MiB | 151 MiB | −47 % |
| `--all --json` | 314 MiB | 151 MiB | −52 % |
| `--all --format ndjson` (streaming) | 65 MiB | 26 MiB | −60 % |

El prototipo cambió unas 60 líneas en dos ficheros:

```rust
// paginate.rs: desmontar la página en vez de clonarla
let Some(Value::Object(mut record)) = result.body.take() else { … };
let Some(Value::Array(data)) = record.remove("data") else { … };

// output.rs: serializar directo a stdout con búfer, y un sobre que toma prestado
#[derive(Serialize)]
struct Envelope<'a> { status: u16, headers: &'a Map<String, Value>, body: Option<&'a Value> }
serde_json::to_writer_pretty(&mut BufWriter::new(&mut *io.stdout), body)?;
```

Alcance real: el tiempo ya es bueno (0,2 s para esa página de 52 MiB) y con respuestas habituales de kilobytes la diferencia no se nota. Importa con `--all --max-items` alto u observaciones con entradas y salidas grandes. El suelo de 198 MiB es el cuerpo en búfer más el árbol `serde_json::Value` que entrega el SDK; bajarlo exigiría una API de streaming en el SDK. No pasé la suite de pruebas sobre el prototipo; solo comparé la salida de los siete modos de la tabla.

Relacionado, y detectado por clippy: 16 `assigning_clones`, 1 `redundant_clone` (`src/skills/api.rs:448`), 1 `needless_collect` (`src/operation.rs:302`) y 6 `format_push_string` (`escape_terminal_text` y `write_colored` en `src/output.rs` crean un `String` temporal por cada fragmento).

### 3.3 Cadenas y JSON dinámico donde caben tipos

**Estados como cadenas.** `Completed.status` es `&'static str` (`src/skills/push.rs:136`) y se compara con literales en `276-280`, `485` y `502-503`. Un error de tecleo en `"labeled"` compila. Un enum con `as_str()` lo impide; el propio fichero ya lo hace bien con `Exclusion` en `publish_filter.rs`.

**Releer el JSON propio.** `summary()` (`src/skills/push.rs:291-337`) recibe el `Map<String, Value>` que el mismo módulo acaba de construir y extrae `reason`, `name`, `changes.added` por clave para formar el texto. Los resultados de instalación se montan con `json!` (`src/skills/install.rs:529-536`). Structs con `#[derive(Serialize)]` conservan el orden de los campos, que es lo que exige la paridad de bytes con el CLI oficial, y dejan que el compilador compruebe los nombres.

**Lockfile analizado a mano.** `src/skills/lock.rs:29-49` y `71-130` recorren un `Value` comprobando claves desconocidas, tipos y positividad. Casi todo eso se expresa de forma declarativa. Esbozo sin compilar:

```rust
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Selector { Label(String), Version(NonZeroU64) }   // {"label": …} o {"version": …}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedSkill { path: String, name: SkillName, selector: Selector, version: NonZeroU64 }
```

Los mensajes de error cambiarían; las pruebas actuales solo fijan el código de salida 6 para esos casos, pero si el texto es parte del contrato con el CLI oficial hay que mantener el análisis manual.

**Invariantes revalidadas en vez de codificadas.** `is_skill_name(&str) -> bool` se llama en cinco sitios (`src/skills/api.rs:176`, `416`; `install.rs:86`; `lock.rs:89`; `local_skill.rs:469`) y la condición "versión positiva" en otros cinco (`api.rs:177`, `418`; `lock.rs:43`, `96`; `install.rs:115`). Un newtype `SkillName` con constructor validado y `NonZeroU64` para versiones convierten esas comprobaciones en una sola, en la frontera. `content_length` se comprueba `< 0` y luego se convierte con `as u64` (`src/skills/api.rs:427-435`); `u64::try_from` hace ambas cosas.

**Valores de enum duplicados.** Los flags cerrados se declaran como listas de cadenas para clap (`STATUS` en `src/api/annotation_queues.rs:14`) y luego se convierten al enum del SDK en ejecución con `wire_enum` (`src/args.rs:59`), una conversión que "no puede fallar" pero que el compilador no garantiza. `comments create --object-type` no valida (`src/api/comments.rs:29-30`) mientras que `comments list --object-type` sí (`72-73`).

### 3.4 Lints y CI ausentes

`Cargo.toml` no tiene tabla `[lints]`, `src/lib.rs` no tiene atributos de crate y el repositorio no tiene configuración de CI. El código pasa hoy los lints por defecto porque alguien ejecuta los comandos; nada lo exige.

Desglose de los 238 avisos de `pedantic` + `nursery`: `missing_errors_doc` 50, `must_use_candidate` 49, `doc_markdown` 22, `use_self` 22, `assigning_clones` 16, `return_self_not_must_use` 15, `future_not_send` 9, `or_fun_call` 9, `format_push_string` 6, `missing_const_for_fn` 6, y 34 repartidos en otros 16 lints. La mayoría es estilo; los que señalan trabajo real son los de clones y conversiones citados en 3.2 y 3.3.

Sugerencia mínima, que con `-D warnings` obliga a corregir unos 28 puntos:

```toml
[lints.rust]
unsafe_code = "forbid"

[lints.clippy]
assigning_clones = "warn"
redundant_clone = "warn"
needless_collect = "warn"
format_push_string = "warn"
cast_possible_truncation = "warn"
cast_sign_loss = "warn"
```

Y un flujo de CI con los cinco comandos de `langfuse-cli/CLAUDE.md` más `cargo audit`.

## 4. Hallazgos menores

| Tema | Dónde | Qué ofrece Rust |
|---|---|---|
| Limpieza manual de temporales | `src/skills/install.rs:286-313`, `384-403`; `src/skills/lock.rs:204-214` | Un guard con `Drop` (`tempfile::TempDir` y `NamedTempFile::persist`, ya dev-dependency) limpia también ante pánico o cancelación. Ni eso ni lo actual limpian con Ctrl-C; haría falta un manejador de señal. |
| `&mut self` en cascada | `src/skills/api.rs:105-111` | La inicialización perezosa del cliente obliga a `&mut self` en todos los métodos y al `expect`. Con `OnceCell` serían `&self`; hoy `fetch_all_contents` clona `Settings` y crea un `Remote` nuevo (`354`) solo para llamar a `failure`, que solo lee `host`. |
| Peticiones en serie | `src/skills/install.rs:442-478`, `src/skills/push.rs:467-477` | Un manifiesto por skill, uno tras otro. El `buffered(n)` que ya se usa para contenidos serviría aquí una vez resuelto lo anterior. |
| `Option` más `expect` | `src/paginate.rs:51`, `160` | `let last = loop { …; break result; };` hace que el compilador demuestre que hay valor. |
| Invariantes de `OperationMeta` en ejecución | `src/execute.rs:125-133`, `src/operation.rs:285-289` | `PUT` sin cuerpo de respuesta es un error en ejecución y el nombre de un parámetro de ruta solo se comprueba con `debug_assert!`. Como `META` es `const`, un `assert!` en el builder `const fn` lo convierte en error de compilación. Hoy las 120 operaciones son correctas (verificado). |
| Rutas como `String` | `src/skills/mod.rs:77`, `81`, `116`, `148`; 8 usos de `to_string_lossy` | `PathBuf` y `OsString` aceptan rutas que no son UTF-8; con `String` clap las rechaza y las conversiones con pérdida pueden apuntar a otro fichero. |
| Subproceso git con sondeo | `src/skills/publish_filter.rs:96-167` | Dos hilos y un bucle `try_wait` con `sleep(5 ms)`. `tokio::process` con `timeout` y `kill_on_drop` lo resuelve sin hilos, a cambio de más features de tokio. |
| `map_err` redundante | `src/config.rs:128-135`, `src/api/mod.rs:458` | `?` ya aplica `From<LangfuseError>`; sobran cinco `.map_err(CliError::from)`. |
| Entorno leído directamente | `src/lib.rs:80-81` | `NO_COLOR` y `TERM` se leen del proceso, a diferencia del resto de la configuración, que es inyectable. |
| Superficie pública | `src/lib.rs:7-19` | Todo es `pub mod` en un crate publicado: es API con semver. `pub(crate)` salvo `run`, `command`, `output::Io` y `error::Exit` la reduce. |
| Sin `Debug` | `Io`, `Remote`, `Context` | Tres tipos públicos sin `Debug`. |

## 5. Orden sugerido

1. Tubería cerrada y pánicos (3.1): es un defecto visible y el cambio es pequeño.
2. `[lints]` y CI (3.4): barato, y evita regresiones de todo lo demás.
3. Tipar los resultados y estados de `skills` (3.3): es donde un cambio futuro puede romper la paridad de bytes sin que el compilador avise.
4. Propiedad y streaming en la salida (3.2): unas 60 líneas, ganancia medida, relevante solo con respuestas grandes.
5. El resto de 3.3 y la sección 4, cuando se toque cada fichero.

## 6. Lo que no verifiqué

- Comportamiento en macOS y Windows; todo se ejecutó en Linux aarch64.
- Los binarios musl de `dist/` (no los reconstruí; sus tamaños son los ya publicados: 5,2 MB aarch64 y 6,3 MB x86_64).
- `cargo audit` con la base actualizada: usé la copia local sin descargar.
- Llamadas a la API real de Langfuse: solo servidores locales.
- El port TypeScript oficial, para confirmar qué mensajes exactos forman parte del contrato de paridad.

## 7. Seguimiento (2026-11-12)

Todos los puntos de las secciones 3 y 4 están resueltos y publicados como la versión 0.2.0 del CLI, con los binarios de `dist/` reconstruidos. Lo que se decidió no hacer, y por qué, está en 7.3.

Verificación del estado final: `cargo fmt --check`; los dos `cargo clippy … -D warnings` con la tabla `[lints]` nueva; 102 pruebas (100 sin `unstable`); `cargo doc` con `-D warnings`; `cargo audit --no-fetch` sin vulnerabilidades en 226 crates. Los bytes de salida se compararon con los del binario anterior: el esquema `api schema --json`, la ayuda de los 160 comandos y los diez modos de la tabla de memoria son idénticos.

### 7.1 Qué se hizo

| Punto | Cambio | Comprobación |
|---|---|---|
| 3.1 Tubería cerrada | Toda escritura a stdout pasa por `Io::out`, `Io::out_with` o `Io::status`; `CliError::stdout` convierte la tubería cerrada en fin silencioso con exit 0 y cualquier otro fallo en exit 1. Las completions se generan en memoria. `--all` deja de pedir páginas cuando se cierra la tubería. | Binario release: `completions bash \| head -1`, `api schema --json \| head -c 100`, `api --help \| head -1` dan exit 0 sin stderr; redirigir a `/dev/full` da exit 1 con mensaje. Pruebas nuevas con escritores que fallan. |
| 3.1 Pánicos | `run` los captura (exit 1) y vacía stdout, de modo que un fallo en el vaciado final también se informa. | Prueba con un escritor que entra en pánico y otra con uno que falla al vaciar. |
| 3.2 Propiedad y streaming | `paginate.rs` desmonta cada página en vez de clonarla; `output.rs` serializa directo a un `BufWriter`, con el sobre de `--json` tomando prestado y el JSON coloreado escrito en el flujo. | Tabla de 7.2. |
| 3.3 Estados y JSON de `skills` | Enums `Status` y `Reason` y un enum `Report` con `Serialize` por comando; el texto se forma desde campos tipados. | Tres pruebas `*_output_bytes` escritas antes del cambio fijan el JSON y el texto byte a byte; pasaron sin tocarlas. |
| 3.3 Lockfile | `Selector`, `LockedSkill` y `SkillsLock` con serde derivado y `deny_unknown_fields`. | 22 casos inválidos en la prueba unitaria; el texto escrito no cambió. |
| 3.3 Invariantes | `SkillName` y `Version` en `src/skills/types.rs`; `u64::try_from` para `content_length`. | Ya no hay ninguna llamada a `is_skill_name` ni comprobación `> 0` de versiones. |
| 3.3 Enums duplicados | `args::wire::<T>(valores)`: el flag tiene el tipo del enum del SDK y un valor listado que no sea variante se rechaza al analizar. | Prueba con las cinco operaciones afectadas. |
| 3.4 Lints | Tabla `[lints]`: `unsafe_code = "forbid"`, `missing_docs`, `missing_debug_implementations` y seis lints de clippy. Corregidos los 19 puntos que señaló. | Ambos clippy con `-D warnings`. |
| 3.4 CI | `.github/workflows/langfuse-cli.yml`: formato, los dos clippy, los dos test, documentación y `cargo audit`. | YAML validado y cada paso ejecutado en local. **No se ha ejecutado en GitHub.** |
| Temporales | `paths::Temporary`, un guard con `Drop`, para el directorio de staging y el temporal del lockfile. | Prueba unitaria; la prueba que busca restos `.install-` sigue pasando. |
| `&mut self` y peticiones en serie | `Remote` con `OnceCell` y métodos `&self`; manifiestos de `install` y versiones `latest` de `push` con `buffered(4)`. | Prueba con nueve skills: resultados y lockfile en orden, y el primer error en orden es el que se informa. |
| `Option` más `expect` | `let last = loop { … break result }` en `paginate.rs`. | No queda ningún `unwrap`, `expect` ni `unreachable!` fuera de pruebas. |
| Invariantes de `OperationMeta` | `assert!` en el builder `const fn` y en las macros de operación; `resource!` fuerza la evaluación de cada `META`. | Un `PUT` sin cuerpo de respuesta y un parámetro de ruta mal escrito ya no pasan `cargo check` (probado y revertido). `tests/operations_tests.rs` recorre las 120 operaciones. |
| Rutas como `String` | `PathBuf` en los argumentos de `skills`; ningún `to_string_lossy`. | Prueba con un nombre de fichero no UTF-8. |
| Subproceso git | `tokio::process` con `timeout` y `kill_on_drop`; sin hilos ni sondeo. | La prueba de reglas de `.gitignore` sigue pasando. |
| `map_err` redundante | Quitados los cinco. | — |
| Entorno leído directamente | `NO_COLOR` y `TERM` se leen al construir `Io` (`color_allowed`), con una función pura probada. | Prueba unitaria. |
| Superficie pública | Módulos privados; la API es `run`, `command`, `Io` y `Exit`. | `cargo doc` sin avisos. |
| Sin `Debug` | `Debug` manual para `Io`, `Remote` (sin credenciales) y `Context`. | `missing_debug_implementations` activo. |

### 7.2 Mediciones, antes y después

Pico de memoria residente con el mismo servidor local; la salida es idéntica en todos los modos.

| Modo | Antes | Después | Diferencia |
|---|---|---|---|
| JSON legible, 1 página de 52 MiB | 260 MiB | 198 MiB | −24 % |
| JSON con color | 275 MiB | 198 MiB | −28 % |
| `--json` (sobre) | 322 MiB | 198 MiB | −39 % |
| `--format ndjson` | 197 MiB | 198 MiB | 0 |
| `--format table` | 231 MiB | 232 MiB | 0 |
| `--all`, JSON legible, 10 páginas de 5,2 MiB | 287 MiB | 151 MiB | −47 % |
| `--all --json` | 313 MiB | 151 MiB | −52 % |
| `--all --format ndjson` | 64 MiB | 30 MiB | −52 % |
| `--all --format table` | 219 MiB | 232 MiB | **+6 %** |

`--all --format table` sube 13 MiB. La tabla domina ese modo, y antes aprovechaba la memoria que liberaban los clones de cada página; ahora no hay clones que liberar y el pico iguala al de una sola página.

| Métrica | Antes | Después |
|---|---|---|
| Pruebas (con y sin `unstable`) | 81 y 79 | 102 y 100 |
| `unwrap`, `expect`, `unreachable!` fuera de pruebas | 2 | 0 |
| `to_string_lossy` | 8 | 0 |
| `.clone()` fuera de pruebas | 262 | 232 |
| Conversiones numéricas con `as` | 12 | 4 |
| Avisos de clippy `pedantic` + `nursery` | 238 | 118 |
| Binario release (aarch64, glibc) | 5 445 272 bytes | 5 510 904 bytes (+1,2 %) |
| `dist/` aarch64 musl | 5 226 056 bytes | 5 310 832 bytes (+1,6 %) |
| `dist/` x86_64 musl | 6 288 784 bytes | 6 365 272 bytes (+1,2 %) |
| Arranque de `--version` | 2,6 ms | 2,6 ms |
| Dependencias en `Cargo.lock` | 225 | 226 (`signal-hook-registry`, por `tokio::process`) |

### 7.3 Decisiones y desviaciones

- **Ctrl-C sigue sin manejador.** Uno de tokio dejaría el proceso sin poder interrumpirse durante lecturas bloqueantes, como `--body-file -` esperando en la terminal. Los guards cubren error, pánico y cancelación; ante Ctrl-C puede quedar un directorio `.<nombre>.install-<uuid>`.
- **El backup de una reinstalación no es un guard.** Si el intercambio falla y la copia anterior tampoco puede volver a su sitio, el backup es la única copia y debe quedarse en disco.
- **`comments create --object-type` no valida.** El spec lo declara cadena libre en el cuerpo y enum solo en la consulta.
- **Lockfile: solo objetos.** Los structs derivados con serde también aceptan un array con los campos en orden, así que `[1, []]` habría sido un lockfile válido. Lo detectó una prueba; `lock.rs` exige ahora objetos antes de deserializar.
- **Mensajes de valor vacío.** Con `PathBuf`, clap rechazaba `--directory ""` con su propio texto. Un parser de rutas que admite la cadena vacía conserva los mensajes del CLI oficial.
- **Fuera del informe:** `langfuse api schema` sin `--json` emitía códigos ANSI aunque la salida no fuera una terminal; ahora sigue `--color`.

Cambios visibles para quien usa el CLI, además de los de 3.1: las completions de zsh y fish completan ficheros en `--directory` y `--lockfile`; el detalle de los errores del lockfile es el de serde (el prefijo `Cannot read <ruta>:` y el exit 6 no cambian); un fichero con nombre no UTF-8 dentro de una skill es un error explícito al publicar.

### 7.4 Corrección al informe

La sección 1 decía que el `pub` generalizado no escondía código muerto. El experimento lo subestimaba: en un binario, lo que es `pub` en la raíz se da por alcanzable. Al hacer privados los módulos, el compilador señaló `CliError::with_hint` y, con él, el campo `hint`, que nunca se rellenaba. Ambos se eliminaron.

### 7.5 Pendiente

- Comprobar la primera ejecución del flujo de CI en GitHub, que arranca con el push de esta versión.
- Publicar 0.2.0 en crates.io, si se quiere: aquí solo se subió la versión y se reconstruyó `dist/`.
- macOS y Windows siguen sin probar.
- Los 118 avisos restantes de `pedantic` y `nursery` son de estilo y no están activados.
