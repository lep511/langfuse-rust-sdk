# Checking whether the API is out of date

How to find out whether this SDK is behind the Langfuse public API, using
`scripts/update_openapi.py` and the files in `api/`, and how to implement
whatever the check finds following the conventions of this repository
(`CLAUDE.md`).

## Quick answer

```bash
python3 scripts/update_openapi.py --dry-run
```

- `No differences: api/openapi.yml is already up to date.` → nothing to do.
- Anything else → the spec moved. Read the report (step 2), then follow
  [Implementing the changes](#implementing-the-changes).

`--dry-run` writes no file, so it is safe to run at any time.

## What "out of date" means here

There are two separate questions, and each has its own source of truth:

| Question | Answered by |
|---|---|
| Is `api/openapi.yml` behind the spec Langfuse Cloud serves? | `scripts/update_openapi.py` |
| Does the SDK implement everything in `api/openapi.yml`? | `api/operation-coverage-audit.md` and the test suite |

The repository keeps one invariant: **the SDK implements every operation of
the committed `api/openapi.yml`** (120 today, 21 of them under
`/api/public/unstable/` and behind the `unstable` feature). The script only
answers the first question; the invariant is what makes its answer enough.
Do not commit a newer spec without the code that implements it.

## The files involved

| File | Role | Edited by |
|---|---|---|
| `scripts/update_openapi.py` | Downloads the spec, compares it with the local copy, prints a report | — |
| `api/openapi.yml` | The spec the SDK implements, byte for byte as downloaded | the script only, never by hand |
| `api/openapi-updates.txt` | Dated log: one entry per non-dry run, newest last, each holding what changed since the entry before it | the script only (append) |
| `api/operation-coverage-audit.md` | Every operation of the spec, the SDK method that implements it and a wiremock test that exercises it | by hand, when operations change |

The spec comes from `https://cloud.langfuse.com/generated/api/openapi.yml`.
The AI Gateway (`src/gateway.rs`) is not part of it and is not covered by
this check.

## Prerequisites

- Python 3 with PyYAML: `pip install pyyaml` (the script exits with a clear
  error without it).
- Network access to `cloud.langfuse.com`. No Langfuse credentials are needed.
- The script can run from any directory (it resolves `api/` relative to its
  own location); the other commands of this document assume the repository
  root.

## Step by step

### 1. Run the check without writing anything

```bash
python3 scripts/update_openapi.py --dry-run
```

The script downloads the spec, refuses anything that is not a valid OpenAPI
document, and compares it with `api/openapi.yml`. The last line confirms that
nothing was modified:

```text
Downloaded https://cloud.langfuse.com/generated/api/openapi.yml (465472 bytes)

No differences: api/openapi.yml is already up to date.
Dry run: neither api/openapi.yml nor api/openapi-updates.txt was modified.
```

### 2. Read the report

The first line after `Downloaded …` tells which case you are in:

| Output starts with | Meaning | SDK work |
|---|---|---|
| `No differences: … already up to date.` | The files are byte-identical | None |
| `Nothing new: no operation, parameter, schema, property or enum value was added.` and no section | Only formatting or ordering differs | None (update the spec file) |
| `Nothing new: …` followed by `Removed` / `Changed`, or by `N documentation-only changes …` | Nothing was added, but existing items changed | Review each line; often doc comments only |
| One or more `New …` sections | The API grew | Implement them |
| `No previous spec to compare with at …` | `api/openapi.yml` is missing or unreadable | Restore it from git first |

When there are differences, the report groups them in sections, each with a
count. This sample was produced by comparing the current spec with a
deliberately trimmed copy, to show every kind of line (these are not real
upstream changes):

```text
New operations (2)
  + GET /api/public/v2/evaluators/{evaluatorId}/versions (evaluators_listVersions)
  + DELETE /api/public/dataset-items/{id} (datasetItems_delete)

New parameters (1)
  + GET /api/public/v3/scores: query parameter `environment` (string, nullable, optional)

New schemas (1)
  + PricingTierAttributeCondition: object with 4 properties

New properties (1)
  + ScoreConfig.description: string, nullable

New enum values (1)
  + BlobStorageIntegrationFileType: PARQUET

New union variants (1)
  + PricingTierCondition: PricingTierAttributeCondition

Removed (1)
  - property ScoreConfig.legacyFlag

Changed (2)
  ~ ScoreConfig.minValue: type "integer" -> "number"
  ~ ScoreConfig: now required: isArchived

Operations: 118 -> 120 (21 unstable), schemas: 330 -> 331
```

- `+` lines are additions, `-` removals, `~` changes (`old -> new`, with
  `(unset)` for a keyword that was absent).
- `Other additions` (not shown above) holds new responses, content types,
  headers, tags, security schemes, `allOf` members and discriminator mappings.
- The last line counts operations, unstable operations and schemas, as
  `old -> new` when a number moved. Compare it with the counts quoted in
  `README.md`.
- List items are matched by identity (parameter name, `$ref`, enum value), so
  a reordered list is not reported.

### 3. Look at documentation-only changes

Changes to `description`, `summary`, `title`, `example`, `examples` and
`externalDocs` are only counted by default. List them with:

```bash
python3 scripts/update_openapi.py --dry-run --verbose
```

They do not change what the API accepts or returns, but a changed description
sometimes documents a behaviour the SDK's doc comments should mention (see
[Documentation-only changes](#documentation-only-changes)).

### 4. Check the SDK against the committed spec

When step 1 says "no differences", the invariant above says the SDK is
complete. To verify it rather than trust it, compare the operations of the
spec with the rows of the coverage audit (run from the repository root):

```bash
python3 - <<'EOF'
import re, yaml
from pathlib import Path
spec = yaml.safe_load(Path("api/openapi.yml").read_bytes())
methods = ("get", "put", "post", "delete", "options", "head", "patch", "trace")
ops = {f"{m.upper()} {p}" for p, item in spec["paths"].items() for m in item if m in methods}
audit = set(re.findall(
    r"\| `((?:GET|PUT|POST|DELETE|PATCH|HEAD|OPTIONS|TRACE) /[^`]+)` \|",
    Path("api/operation-coverage-audit.md").read_text(),
))
print(len(ops), "operations in the spec,", len(audit), "in the audit")
for op in sorted(ops - audit): print("  missing from the audit:", op)
for op in sorted(audit - ops): print("  no longer in the spec:", op)
EOF
```

Expected today: `120 operations in the spec, 120 in the audit` and no other
line. Then `cargo test --all-features` runs the wiremock test named in every
audit row.

### 5. Apply the update when you are ready to implement it

Only once you intend to do the SDK work (on a branch):

```bash
python3 scripts/update_openapi.py
git diff --stat api/
```

This overwrites `api/openapi.yml` and appends the dated report, always with
the documentation-only changes, to `api/openapi-updates.txt`. From here on the
log entry is your work list.

## Things to know about the script

- **The exit code does not tell whether the spec changed.** It is `0` with or
  without differences and `1` only on failure (download, invalid document,
  write error), in which case `api/openapi.yml` is left untouched. To script
  the check, match the output:

  ```bash
  python3 scripts/update_openapi.py --dry-run --verbose | tee report.txt
  grep -q "^No differences" report.txt   # exit 1 = the spec moved
  ```

- **A non-dry run always appends a log entry**, even `No differences`. Use
  `--dry-run` for routine checks. If you ran it for real with nothing to
  update, either commit the entry as a record of the check or drop it with
  `git restore api/openapi-updates.txt`.
- **The report is shown once.** A real run overwrites the spec, so running
  again reports `No differences`. The full report stays in
  `api/openapi-updates.txt`. To reproduce it against the committed spec:

  ```bash
  git show HEAD:api/openapi.yml > /tmp/openapi-head.yml
  python3 scripts/update_openapi.py --dry-run --verbose --output /tmp/openapi-head.yml
  ```

- **Never edit `api/openapi.yml` by hand.** Each run compares with the file
  the previous run wrote; a manual edit would show up in the next report as an
  upstream change.
- **Other options**: `--url` (any URL serving the spec, including `file://`
  paths and
  `https://raw.githubusercontent.com/langfuse/langfuse/<ref>/web/public/generated/api/openapi.yml`
  to compare with a given Langfuse commit or tag), `--output` (the file to
  compare with and overwrite), `--report` (the log file), `--timeout`
  (seconds, default 60). With `--output` outside the repository, a non-dry run
  still logs to `api/openapi-updates.txt` unless `--report` is given.

## Implementing the changes

### Triage: what each report section means for the SDK

Handle the lines that can break existing calls first, then the missing
features, then the documentation.

| Priority | Report line | Why it matters | Action |
|---|---|---|---|
| 1 | `New union variants` on a schema the SDK reads | Tagged unions are strict: a response with an unknown variant fails to deserialize | Add the variant to the `#[serde(tag = "...")]` enum |
| 1 | `Changed`: `no longer required: x` or `nullable (unset) -> true` on a response schema | A non-`Option` field fails when the server omits it or sends `null` | Make the field `Option<T>` |
| 1 | `Changed`: `type a -> b`, `format`, `$ref` | The Rust type may no longer parse or serialize correctly | Change the field type |
| 1 | `Changed`: `now required: x` on a request schema, or a new required parameter | Requests built without it are rejected | Add it to the struct and to its `new(required...)` constructor |
| 1 | `Removed`: operation, property, parameter, enum value | The SDK calls or expects something that is gone | Remove or relax it (see below) |
| 2 | `New operations` | Endpoint missing from the SDK | New method, or new resource |
| 2 | `New parameters`, `New properties` | Unknown response fields are ignored, so nothing breaks, but callers cannot send or read them | Add the field |
| 2 | `New enum values` | `wire_enum!` enums parse them as `Unknown`; they cannot be sent | Add the variant |
| 2 | `New schemas` | Only matters through the operation or property that refers to it | Model it with that operation or property |
| 2 | `Other additions` | A new response status or content type may need another HTTP helper | Check the helper (JSON vs no content) |
| 3 | Documentation-only changes | Behaviour notes | Update doc comments when they describe behaviour |

A schema can be used in requests, in responses or in both: find it in
`src/models/` before deciding which row applies.

### A new operation on an existing resource

1. **Method** in `src/api/<resource>.rs`, returning `Result<T, LangfuseError>`.
   The path starts with `/api/public/` and every interpolated path parameter
   goes through `encode_path_segment`.
2. **HTTP helper that matches the spec response.** JSON body: `get`,
   `get_with_query`, `post`, `put`, `patch`, `delete_with_response`,
   `delete_with_body_response`. 204 or ignored body: `post_no_response`,
   `post_no_response_with_headers`, `patch_no_content`, `delete_no_content`,
   `delete_no_content_with_query`, `delete_with_body`. The JSON helpers
   deserialize every 2xx body, so they fail on an empty 204 response.
3. **Models** in `src/models/<resource>.rs` (conventions below); query-param
   structs live next to the client in `src/api/`.
4. **Pagination.** A list method returning `Paginated` / `CursorPaginated`
   gets a `<method>_pages` companion (params by value, `paginate_pages` /
   `paginate_cursor`), its params get `impl_page_query!` /
   `impl_cursor_query!`, and the route is added to the wiring tests of
   `tests/pagination_tests.rs` (`every_page_based_method_paginates`,
   `every_cursor_based_method_paginates`, or
   `every_unstable_page_based_method_paginates`).
5. **Blocking client**: `python3 scripts/generate_blocking.py`. Never edit
   `src/blocking/api.rs` by hand; `tests/blocking_tests.rs` fails when a
   public method of `src/api` has no wrapper.
6. **Test** in `tests/api_<resource>_tests.rs` with `wiremock` and the helpers
   of `tests/common/mod.rs` (`test_client()`, `json(status, body)`,
   `single_request`, `body_json`, `query_pairs`, `query_value`): assert the
   method, path, query and body sent, and parse a response shaped like the
   spec's. Tests never touch the network.
7. **Bookkeeping**: see [Records to update](#records-to-update).

### A new resource (a new tag in the spec)

Everything above, plus what `CLAUDE.md` lists for a new resource:

- model structs in `src/models/` and a client in `src/api/`, both registered
  in their `mod.rs`;
- a field and its construction in `LangfuseClient` (`src/lib.rs`);
- the field in `client_with_custom_config` (`tests/config_tests.rs`);
- a `tests/api_<resource>_tests.rs` wiremock file;
- serde tests for its enums in `tests/serde_new_resources_tests.rs`;
- a row in the "Supported Resources" table of `README.md`.

### Operations under `/api/public/unstable/`

The report counts them separately. They go behind the `unstable` feature:

- `#[cfg(feature = "unstable")]` on the modules in `src/api/mod.rs` and
  `src/models/mod.rs`, and on the field and its construction in `src/lib.rs`;
- the test file starts with `#![cfg(feature = "unstable")]`, serde tests go to
  `tests/serde_unstable_tests.rs`, and the sub-client is added to
  `unstable_sub_clients_exist_with_feature` (`tests/config_tests.rs`);
- a new client file is added to the `UNSTABLE` set of
  `scripts/generate_blocking.py`, so its blocking wrappers are gated too.

### New parameters and properties

- **Query parameter**: an `Option<T>` field with
  `#[serde(skip_serializing_if = "Option::is_none")]` on the params struct.
  JSON-in-query parameters (`filter`, metrics `query`) are `String`s, or typed
  and serialized with `serde_json::to_string`.
- **Response property**: `Deserialize` + `#[serde(rename_all = "camelCase")]`,
  `Option` for nullable or non-required spec fields. Prefer `Option` for a
  property that is new even if the spec marks it required: servers older than
  the spec do not send it.
- **Request property**: `Serialize` with
  `skip_serializing_if = "Option::is_none"`; a required one also goes into the
  struct's `new(required...)` constructor.
- **Wire names** `camelCase` cannot produce need an explicit
  `#[serde(rename = "...")]` (`baseURL`, `row_limit`, filter operators with
  spaces). Check the exact name in `api/openapi.yml`, and cover it with a
  serde or wiremock test.
- **Public option structs** (`#[non_exhaustive]`, listed in `CLAUDE.md`) get
  one `with_<field>` builder per new field.
- Adding a public field to a params or request struct breaks callers that
  wrote the struct literal without `..Default::default()` or
  `..Struct::new(..)`: say so in the changelog.

### New enum values and union variants

- Enums read from responses use `wire_enum!` (`src/models/macros.rs`):
  `#[non_exhaustive]` with an `Unknown` fallback that cannot be serialized.
  Add the variant, and its `(variant, wire value)` pair to the `assert_enum`
  cases of `tests/serde_new_resources_tests.rs` (or
  `tests/serde_unstable_tests.rs`).
- Request-only enums are plain `Serialize` enums: add the variant and an
  `assert_serialized` case.
- Tagged unions (`#[serde(tag = "...")]`, shared base fields
  `#[serde(flatten)]`) stay strict: add the variant with its fields and a test
  that parses a payload of that variant.

### Removed and changed items

- **Removed operation**: the method can only fail. Remove it (and its blocking
  wrapper, test, audit row and README mention) and record the removal under
  a breaking-change note in `CHANGELOG.md`.
- **Removed property**: on a response struct make it `Option` or drop it; on a
  request struct stop sending it only if every supported server ignores it.
- **`no longer required` / now nullable**: relax what the SDK reads, but keep
  sending what every server version accepts. Release 0.11.2 is the precedent:
  the `value` of a `null` filter became optional, and
  `EvaluationRuleFilter::Null` keeps sending `""`.
- **`now required`** on a response: keeping the field `Option` stays
  compatible with older servers; change it only when the stricter type is
  worth a breaking change.
- When the spec and the server disagree, check the real API (the examples
  build their client with `ClientConfig::from_env()`) and document the
  difference in the doc comment, as 0.11.2 did for the `startTime` filter.

### Documentation-only changes

Read the `--verbose` lines (or the log entry) and open the spec at each place.
Mirror in the SDK's doc comments whatever describes behaviour: defaults,
limits, which parameter wins over which. Pure rewording needs no code change,
only the refreshed spec and log.

### Records to update

| What changed | Update |
|---|---|
| Any spec refresh | `api/openapi.yml` and `api/openapi-updates.txt` (both written by the script), committed with the code that implements them |
| Operations added or removed | The rows of `api/operation-coverage-audit.md` (operationId, method + path, SDK method, wiremock test), under a new dated heading |
| Operation counts | "120 operations" / "21 unstable" in `README.md` (features list and Cargo features table) |
| Any user-visible change | `CHANGELOG.md` (Keep a Changelog sections; name the spec date, as 0.11.2 does: "`api/openapi.yml` follows the spec served by Langfuse Cloud on <date>") |
| Release | `version` in `[workspace.package]` together with the `=x.y.z` pin on `langfuse-rust-sdk-macros` in `[workspace.dependencies]` |
| A new workflow worth showing | An example in `examples/`, its `[[example]]` entry when it needs a feature, a section and overview row in `examples/EXAMPLES.md`, and the README table |

The crate is pre-1.0 and its changelog states that minor versions may break:
a spec refresh with compatible additions or doc changes fits a patch release
(0.11.2 was one), a removed method or changed type a minor one.

`langfuse-cli/` is a separate crate that depends on the SDK release from
crates.io and has its own operation table. Update it after publishing the SDK,
following "Adding or changing an operation" in `langfuse-cli/CLAUDE.md`.

### Verify before committing

```bash
python3 scripts/generate_blocking.py         # after any public API client method change
cargo test                                   # iterate without optional features
cargo test --all-features                    # every feature, before committing
cargo clippy --workspace --all-targets --all-features -- -D warnings
python3 scripts/update_openapi.py --dry-run  # must print "No differences"
```

Use this clippy invocation, not plain `cargo clippy`: without
`--all-targets --all-features` it skips the tests, the examples and every
feature-gated module, including the `unstable` clients. Then run the snippet
of step 4 again: the spec and the audit must list the same operations.

## Recommended routine

- Run `python3 scripts/update_openapi.py --dry-run` before every release and
  on a schedule (weekly is enough; `.github/workflows/langfuse-cli.yml`
  already uses a Monday cron for the CLI). In CI, use the `grep` form above so
  the job fails when the spec moved.
- Keep one spec refresh per change set: the spec, its log entry, the code,
  the tests and the changelog entry together, so `api/openapi.yml` never
  claims more than the SDK implements.
- If an upstream change cannot be implemented yet, leave `api/openapi.yml`
  as it is rather than committing a spec the SDK does not cover.
