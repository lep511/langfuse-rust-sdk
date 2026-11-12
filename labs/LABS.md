# Labs

Two small applications that use this repository's Langfuse tooling from the outside, the way a user would. Each one traces what it does in Langfuse by a different route:

| | [`infinity-project`](infinity-project) | [`rekg`](rekg) |
|---|---|---|
| What it is | Interactive terminal chat with a model on Amazon Bedrock | Script that runs Amazon Rekognition on two images in S3 |
| Language | Rust, edition 2024 | Python 3.13, managed with `uv` |
| Route to Langfuse | The SDK: `langfuse-rust-sdk` 0.11.3 from crates.io, feature `otel` | The CLI: the prebuilt `langfuse` binary of [`langfuse-cli/dist`](../langfuse-cli/dist), run as a subprocess |
| One trace is | One turn of the chat; a conversation is a session | One run of the script |
| AWS service | Bedrock runtime | Rekognition (`us-east-2`) and the S3 bucket it reads |
| Run | `cargo run` | `uv run main.py` |
| Tests | `cargo test`: 98 unit tests | `uv run python -m unittest discover -s tests`: 10 tests |
| Prose docs | The module docs in `src/` (English) | [`rekg/README.md`](rekg/README.md) (Spanish, like its comments and output) |

Neither is part of the SDK's Cargo workspace, so the commands of the root [`CLAUDE.md`](../CLAUDE.md) (`cargo test`, `cargo clippy --workspace ...`) do not build or test them. Run each lab's commands from its own directory.

## Shared setup

- **Langfuse credentials.** Each lab reads its own `.env`: `LANGFUSE_PUBLIC_KEY`, `LANGFUSE_SECRET_KEY` and `LANGFUSE_BASE_URL`. The files are not versioned (the root `.gitignore` ignores `.env`). The ones on this machine also define `LANGFUSE_GATEWAY_HOST` and `LANGFUSE_GATEWAY_KEY`, which neither lab reads.
- **AWS credentials.** Both take the region and credentials from the standard AWS lookup chain (environment, profiles, instance role). `infinity-project` must be allowed to invoke the Bedrock models; `rekg` must be allowed to call Rekognition and to read its bucket.
- **Langfuse is optional in both.** Without credentials, without network or with keys the API rejects, each lab says why it is not traced and otherwise behaves the same.
- **What leaves the machine.** `infinity-project` sends every message, reply, system prompt, tool call and tool result (directory listings) to the Langfuse project of the credentials. `rekg` sends the labels and the text it detects in the images.

## infinity-project

A chat with one agent thread, built on the `infinity-agent-core` runtime with the Bedrock provider. Each line typed at the prompt is sent to the model, and the reply is printed as it streams, with its Markdown rendered for the terminal. Every turn is a Langfuse trace.

### Running it

```sh
cd labs/infinity-project
cargo run
```

- `.env` is loaded from the current directory or the nearest one above it. Variables already set in the environment keep their value. A `.env` that exists and cannot be loaded is a warning under the header.
- The chat starts on `global.anthropic.claude-opus-5-5` (`MODEL_ID` in `src/main.rs`). It fails to start when the provider's catalog has no model with that ID.
- When Langfuse connects, the chat prints `Langfuse session: <url>`; otherwise `Langfuse tracing is off: <reason>`.
- When stdin is not a terminal, each line of the pipe or file is one message: the lines are echoed after the prompt mark, there is no hint line or menu, and the end of the input leaves the chat.

  ```sh
  echo "What is in this directory?" | cargo run -q
  ```

- Like a shell, the chat exits with the status of its last turn: failure when that turn reported an error.

### Commands

A line that starts with `/` is for the chat and is never sent to the model, even when it is not a command (a path like `/home/me/notes.txt what is this?` is rejected, not sent).

| Command | What it does |
|---|---|
| `/clear` | Starts a new conversation: a new thread that has seen nothing, an erased screen and scrollback, and a new Langfuse session. The typed-line history and the model stay. |
| `/usage` | Shows the turns, model calls and tokens of the current conversation. |
| `/model` | Lists the provider's models, with their context window and the one in use. |
| `/model <name>` | Changes to a model, from the conversation's next message on. `<name>` is a model's name or ID, or text that only one model has in either, in any case. |
| `/exit` | Leaves the chat, as do Ctrl+C and Ctrl+D. |
| `/help` | Lists the commands. |

- Command names are lowercase and take no arguments, except `/model`: `/usage of what?` answers `/usage takes no arguments.`
- An unknown command names the one it looks like (`/clera is not a command. Did you mean /clear?`): a command it is the beginning of, or one within one typing slip per three characters, counting a swap of neighbours as one slip. Otherwise the answer lists the commands.
- A text that several models have in their name is an error that lists them. A name that matches a model exactly wins over the names it is part of.

The models are those of `infinity-provider-bedrock` 0.2.0, not of this project:

| Name | Model ID | Context |
|---|---|---|
| `claude-fable-5.1` | `global.anthropic.claude-fable-5-1` | 1M |
| `claude-opus-5.5` | `global.anthropic.claude-opus-5-5` | 1M |
| `claude-fable-5` | `global.anthropic.claude-fable-5` | 1M |
| `claude-opus-4.8` | `global.anthropic.claude-opus-4-8` | 1M |
| `claude-opus-4-6 1m` | `global.anthropic.claude-opus-4-6-v1:1m` | 1M |
| `claude-opus-4-6` | `global.anthropic.claude-opus-4-6-v1` | 200k |
| `claude-sonnet-4-6` | `global.anthropic.claude-sonnet-4-6` | 200k |

### The prompt

| Key | What it does |
|---|---|
| Left, Right, Home, End | Move the cursor. |
| Up, Down, Ctrl+P, Ctrl+N | Walk the lines already entered or, on a line with a menu, step through the menu. |
| Tab, Shift+Tab | Step through the menu only. |
| Enter | Enters the line. On `/model` alone, opens the menu of models on the one in use instead. |
| Ctrl+C, Ctrl+D | Leave the chat. During a turn, Ctrl+C interrupts it and leaves; a second one does not wait for the traces to be sent. |
| Ctrl+L | Erases the screen. |

- **Menus.** A `/` and the beginning of a name lists the commands it could become under the prompt, the most used first. After `/model `, the menu lists the models whose name contains what is typed. Each step puts its row on the line; past either end of the list the line is again what was typed. A line brought back from the history has no menu until it is edited.
- **Most used first.** The count of each command run at an interactive prompt is kept in `$XDG_STATE_HOME/infinity-project/command-usage`, or `~/.local/state/infinity-project/command-usage`, as `name count` lines. The file is read again before each write and replaced atomically, so chats open at the same time count together. When it cannot be written, the chat says so once and counts in memory until it ends.
- The command of a line is shown in its own colour as it is typed.

### What it prints

- The conversation goes to stdout; the spinner and the diagnostics (retries, warnings, errors) go to stderr.
- A header rule carries the model ID. After `/model <name>` the title is redrawn in place for as long as the header is still on the screen.
- A reply is printed block by block as each block is complete; the text still being written shows under it on one dim line that is redrawn as it grows.
- `◇` marks a tool call, and each turn closes with a rule that carries its tokens: `310 (200 cache read, 100 cache write) in · 20 out · 330 total tokens`.
- Colours are used only on a terminal, and never when `NO_COLOR` is set. The live preview needs stdout to be a terminal and `TERM` not to be `dumb`.
- Control characters in model output and provider messages are dropped, except line breaks and tabs, so that neither can send escape sequences to the terminal.

Markdown is GitHub-flavoured, parsed with `markdown-rs`: wrapped paragraphs, headings, nested and numbered lists, task lists, quotes, code blocks (not wrapped, so they copy as they are), links with their address, and tables with borders and column alignment whose widest columns are narrowed and wrapped when the table does not fit.

### The model's tool

`list_directory` lists one directory per call: directories first, files with their size in bytes, symlinks marked as such, and at most 500 entries. It is read-only, never returns file contents, and is limited to the directory the chat was started in: `..`, absolute paths and symlinks that lead out of it are refused. The runtime's built-in sleep tools are also enabled, among them `sleep_until_event_or_input`.

### What goes to Langfuse

| What | Name | Content |
|---|---|---|
| Trace and its root agent observation, one per turn | `chat-turn` | The message as input and the reply's text as output. Tagged `infinity-project`. |
| Generation, one per call to the model | `completion` | The model ID; the request as sent (system prompt, messages, tool definitions); `max_tokens` and the provider's parameters; the output's text, tool-call and thinking blocks; the time of the first chunk; token usage. |
| Tool observation, one per tool the runtime runs | The tool's name | Its arguments and its result, from the generation that calls it to the model call that receives the result. |
| Session, one per conversation | `chat-<id>` | Its turns. `/clear` starts a new one. |

- Usage is reported under the keys Langfuse prices: `input` (not through the cache), `output`, `total`, and `input_cache_read` and `input_cache_creation` when there are any.
- A turn that reports an error is recorded with it. One interrupted with Ctrl+C has level `WARNING` and the status `interrupted by the user`, with the reply so far as output.
- A model call the thread makes on its own between turns (a timer it set, a compaction) is a trace of its own.
- The credentials are checked when the chat starts (`auth_check`), because a rejected export in the background would only be a log line. `LANGFUSE_TRACING_ENABLED=false` turns tracing off, and the other variables of `TracingOptions::from_env` apply.
- The queued traces are sent when the chat exits, also after an error.

### How it is built

| Module | What it holds | Tests |
|---|---|---|
| `main.rs` | The chat loop: connects, reads lines, runs turns, shuts down. | 0 |
| `command.rs` | Parsing of the `/` lines and the suggestion for an unknown command. | 12 |
| `input.rs` | The `rustyline` editor on its own thread: keys, command highlighting, the menu as the editor's hint. | 7 |
| `menu.rs` | What the menus list and what each key means on them. | 20 |
| `model.rs` | The provider's models and the one in use (`ModelChoice`, the runtime's `ModelSource`). | 7 |
| `tally.rs` | How often each command has been run, kept between chats. | 6 |
| `ui.rs` | Terminal output: header, streamed reply, preview line, tool calls, diagnostics, footer. | 7 |
| `markdown.rs` | Markdown to styled lines, at once (`render`) or as it arrives (`MarkdownStream`). | 20 |
| `screen.rs` | Follows the cursor down the screen to know whether the header is still on it. | 7 |
| `usage.rs` | Token accounting: `UsageMeter` wraps the provider and counts every call. | 2 |
| `langfuse.rs` | Tracing: `TracedProvider` wraps the provider and records generations and tool calls. | 5 |
| `tools.rs` | `list_directory`. | 5 |

What the code does not show at a glance:

- **The terminal's own screen, never the alternate one.** A reply longer than the screen is read by scrolling back with the mouse wheel, so the lines that scroll off must stay the terminal's to keep. Starting the chat scrolls what was on the screen out of the way without erasing it. This rules out the alternate screen, scroll regions, a pinned header and mouse reporting, and it is why the chat cannot take itself off the terminal when it ends.
- **The provider is wrapped twice**: `BedrockProvider`, then the usage meter, then the Langfuse tracer. Usage is counted there, where the calls are made, because the runtime's own events report only the last call of a round: a round that runs a tool and calls the model again would be counted once.
- **Cache writes are derived.** The provider reports cache reads and drops cache writes, so they are recovered as what the total leaves unexplained: `total − output − input − cache read`.
- **The provider finds its turn through shared state**, not through tracing context: the model is called from the thread's task, where no context of the chat's task applies.
- **A turn spans several rounds** when the model calls tools. It ends when the thread's driver reports itself idle, or when a round ends on `sleep_until_event_or_input`, which leaves the driver parked. The runtime reports a failed round only as an `Error: …` info message, so that prefix is the failure signal.
- **A response that calls several tools records one tool observation**: the runtime runs the first call and ignores the others.
- **Streaming Markdown only reads whole lines** (`2.` is a list item until it goes on as `2.5 million`), and gives a block once nothing that follows can change it. Lists are given item by item and code blocks line by line.
- **The line editor runs on a plain thread**, not `spawn_blocking`: the runtime waits for its blocking tasks at shutdown, and a `readline` waiting for input would never let it. The agent system runs on local tasks (`LocalSet`), and `.env` is loaded before the runtime starts its threads.
- **Nothing of a conversation is kept**: the conversation and state stores are in memory. Only the command counts outlive a chat.

### Dependencies and project files

- `Cargo.toml` has an empty `[workspace]` table, which keeps the project out of the SDK's workspace. The SDK comes from crates.io at 0.11.3, so a change to the SDK in this repository reaches the chat only once it is published and the version here is raised.
- Direct dependencies: `infinity-agent-core`, `infinity-provider-bedrock`, `infinity-provider-protocol` and `rap-protocol` 0.2.0; `langfuse-rust-sdk` 0.11.3 (`otel`); `rustyline` 19 (`custom-bindings` only); `richrs` 0.2.1; `markdown` 1.0.0; `tokio` 1; `async-trait`, `futures-core`, `dotenvy`, `serde_json`, `unicode-width`.
- `Cargo.lock` is not versioned (the root `.gitignore` ignores it), nor is `target/`.
- `.claude/skills/` holds two project skills: `rust-best-practice` (`cargo add` for dependencies; `cargo fmt`, clippy and the tests before finishing) and `find-crate-source` (`cargo agents crate-info <crate>` to find a dependency's source).

```sh
cargo test                                # 98 tests, no network
cargo clippy --all-targets -- -D warnings
cargo fmt
```

## rekg

A script that asks Amazon Rekognition for the labels of one image and the text of another, both in S3, and records each run in Langfuse through the Langfuse CLI of this repository. It has no Langfuse SDK: everything goes through the `langfuse` binary, which makes it a test of the CLI from a program's point of view.

### Running it

```sh
cd labs/rekg
uv run main.py
```

What a run does is fixed in `main.py`:

| Setting | Value |
|---|---|
| Region | `us-east-2` |
| Bucket | `data-bucket-4938-40001` |
| Confidence threshold | 75% (`MIN_CONFIDENCE`) |
| Labels | `detect_labels` on `ap_19073628668117.jpg`, at most 10 labels above the threshold |
| Text | `detect_text` on `receipt.jpeg`, the lines (not the words) above the threshold |

It prints the threshold, the labels, the lines of text and the URL of the trace, in Spanish.

### The CLI launcher

`bin/langfuse` is the project's CLI, a POSIX shell script:

- It runs `../../langfuse-cli/dist/langfuse-$(uname -m)-unknown-linux-musl`, the static binary for the machine. Those binaries are versioned, for Linux x86_64 and ARM64 only, and are `langfuse-rust-cli` 0.2.0 built on SDK 0.11.3 (`dist/VERSION`).
- It passes the project's `.env` with `--env`, whose values take precedence over the environment. Without a `.env`, the CLI reads the environment.
- `LANGFUSE_CLI=/path/to/binary` uses another binary. A binary that is missing or not executable is exit code 127.

```sh
bin/langfuse api health get
bin/langfuse api help
```

### What goes to Langfuse

`main.py` opens a trace (`rekg.tracing.Trace`), and leaving its `with` block sends it:

| What | Command | Content |
|---|---|---|
| Trace `rekg`, tagged `rekg` | `langfuse api otel export-traces` | Root observation: the bucket and the threshold as input; the labels and the lines of text as output. |
| Observations `rekognition.detect_labels` and `rekognition.detect_text` | The same request | The Rekognition request as input; the results shown, with their confidence, as output; `request_id` and `model_version` as metadata. |
| Score `confidence` on each observation | `langfuse api scores create` | The mean confidence (%) of its results, as a `NUMERIC` score. None when there are no results. |

The calls, in order:

1. `langfuse --json api projects list`, for the project ID of the trace's URL. It comes first so that credentials that do not work show before anything is sent.
2. `langfuse api health get --curl`, which prints the request without sending it, for the base URL.
3. `langfuse --json api otel export-traces --body-file -`, with the trace on stdin.
4. `langfuse --json api scores create --body-file -`, when there are scores. A 207 response means some were rejected and is reported with the reason for each.

- The trace is an OTLP/JSON `ExportTraceServiceRequest` built by hand: random 32-hex trace and 16-hex span IDs, `service.name` `rekg`, and the `langfuse.*` attributes. The trace's name and tags go on every span, because Langfuse filters and aggregates by observation.
- Metadata values are JSON-encoded, or the text `"3.0"` would arrive as the number 3.
- A score's `id` is `<traceId>-<spanId>-<name>` and its timestamp is the end of its observation, so a retry by the CLI does not duplicate it.
- Request bodies go through stdin, never on the command line.

### When something fails

- **The CLI fails** (no binary, no credentials, no network, an API error): the script prints the same as without Langfuse, exits with code 0, and says why on stderr (`Langfuse: …`). When the first call fails, nothing is sent.
- **Rekognition fails**: the exception is recorded on its observation and on the root as level `ERROR` with its message, the trace is still sent, and the exception goes on to end the script.
- Each CLI call is given 120 seconds; the CLI itself cuts a request at 30 seconds and retries it twice.

### Reading the runs back

```sh
# The latest runs
bin/langfuse api observations list --name rekg --limit 5 \
  --fields core,basic --format table --columns traceId,startTime,latency,level

# The observations of a trace, and its scores
bin/langfuse api observations list --trace-id <traceId> \
  --fields core,basic,io --format table --columns name,level,latency,output
bin/langfuse api scores list --trace-id <traceId> \
  --fields details,subject --format table --columns name,value,subject.id,comment
```

Langfuse takes a few seconds to show a trace it has just received, and a little longer for its scores.

### How it is built

| File | What it holds |
|---|---|
| `main.py` | The script: the two Rekognition calls inside a `Trace`. |
| `src/rekg/tracing.py` | `Trace` and `Observation`: collect a run and send it as OTLP/JSON plus scores. |
| `src/rekg/langfuse_cli.py` | Runs the launcher and reads its output: `run`, `api`, `base_url`, `trace_url`, `LangfuseCliError`. |
| `bin/langfuse` | The launcher. |
| `tests/test_tracing.py` | The tests. |
| `pyproject.toml`, `uv.lock`, `.python-version` | Python 3.13 and one dependency, `boto3` (locked at 1.44.1); built with `uv_build`. |

```sh
uv run python -m unittest discover -s tests   # 10 tests, no network
```

The tests replace the CLI with a fake one in a temporary directory that logs every invocation (arguments and body) and answers what the test configures. `TraceTests` (7) cover the calls and their order, the OTLP attributes, the score body, a recorded exception, and the failures: a CLI error, an HTTP error, rejected scores and a missing binary. `LauncherTests` (3) run a copy of `bin/langfuse`: with a `.env`, without one, and with a missing binary.

### Left from the scaffold

- The `rekg` console script of `pyproject.toml` points at `rekg:main`, which only prints `Hello from rekg!`. The program is `main.py`.
- The package description is still `Add your description here`.
- The bucket, the images, the region and the threshold are constants in `main.py`, which runs at import: there are no arguments or configuration.
