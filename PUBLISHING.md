# Publishing langfuse-rust-sdk to crates.io

Step-by-step instructions for preparing, packaging, and publishing the `langfuse-rust-sdk` crate to [crates.io](https://crates.io/).

---

## Prerequisites

### 1. Rust toolchain

You need `cargo` installed (ships with [rustup](https://rustup.rs/)):

```bash
rustup update stable
cargo --version   # 1.85+ required for edition 2024
```

### 2. crates.io account

1. Go to [https://crates.io/](https://crates.io/) and click **Log in with GitHub**.
2. Authorise the crates.io GitHub OAuth app.
3. You now have an account tied to your GitHub identity.

### 3. API token

1. Navigate to Account Settings -> [API Tokens](https://crates.io/settings/tokens).
2. Click **New Token**.
3. Give it a name (e.g. `publish-langfuse`) and select the **publish-new** and **publish-update** scopes.
4. Copy the token and log in locally:

```bash
cargo login
```

Then paste the token at the prompt and press Enter. This saves the token to `~/.cargo/credentials.toml`. Do **not** commit this file.

---

## Cargo.toml requirements

crates.io enforces several metadata fields. The current `Cargo.toml` is missing a few — here is the complete `[package]` section you need:

```toml
[package]
name = "langfuse-rust-sdk"
version = "1.0.1"
edition = "2024"
description = "..."
license = "MIT"
repository = "https://github.com/<owner>/langfuse-rust-sdk"
homepage = "https://github.com/<owner>/langfuse-rust-sdk"
documentation = "https://docs.rs/langfuse-rust-sdk"
readme = "README.md"
keywords = ["langfuse"]
categories = [""]
```

### Required fields

| Field | Why | Status |
|-------|-----|--------|
| `name` | Unique crate name on crates.io | Present |
| `version` | [SemVer](https://semver.org/) version | Present |
| `edition` | Rust edition | Present |
| `description` | Short summary (shown in search results) | Present |
| `license` **or** `license-file` | Must be a valid [SPDX expression](https://spdx.org/licenses/). `"MIT"` works since `LICENSE.txt` exists | Present |

### Strongly recommended fields

| Field | Why | Status |
|-------|-----|--------|
| `repository` | Link to source code (shown on crates.io page) | **Missing** |
| `homepage` | Project homepage | **Missing** |
| `documentation` | Link to docs (defaults to docs.rs) | **Missing** |
| `readme` | Path to README displayed on crates.io | **Missing** |
| `keywords` | Up to 5 keywords for search (lowercase, no spaces) | **Missing** |
| `categories` | Up to 5 [valid categories](https://crates.io/category_slugs) | **Missing** |

### Naming rules

- Crate names must be ASCII, start with a letter, and contain only letters, digits, `-`, or `_`.
- Maximum 64 characters.
- Names are first-come, first-served. Check availability: `cargo search langfuse-rust-sdk`.

---

## Files included in the package

By default `cargo package` includes:

- `Cargo.toml` (rewritten with absolute dependency versions)
- `src/**/*.rs`
- `examples/**/*.rs`
- `README.md` (if declared via `readme`)
- `LICENSE.txt` (if declared via `license-file`, or auto-detected)

To see exactly what will be published:

```bash
cargo package --list
```

### .gitignore / .ignore

Make sure `target/`, `.env`, and any secrets are excluded. Create a `.gitignore` if one doesn't exist:

```gitignore
/target
.env
*.swp
```

### Size limit

crates.io enforces a **10 MB** maximum for the compressed package. Check with:

```bash
cargo package --allow-dirty
ls -lh target/package/langfuse-rust-sdk-1.9.0.crate
```

---

## Pre-publish checklist

Run all of these and make sure they pass:

```bash
# 1. Format
cargo fmt --check

# 2. Lint
cargo clippy --all-targets

# 3. Unit tests + doc-tests
cargo test --lib

# 4. Build docs (catch broken links)
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps

# (powershell)
$env:RUSTDOCFLAGS="-D warnings"; cargo doc --no-deps

# 5. Check for missing docs on public items
cargo rustc --lib -- -W missing_docs

# 6. Dry-run package (catches metadata issues)
cargo package
```

All files must be committed to git before `cargo package` will succeed (or pass `--allow-dirty` for testing).

---

## Step-by-step publishing

### 1. Update Cargo.toml metadata

Add the missing fields listed above. Replace `<owner>` with your GitHub username or org:

```bash
# Edit Cargo.toml — add repository, homepage, documentation, readme, keywords, categories
```

### 2. Commit everything

```bash
git add -A
git commit -m "Prepare for crates.io release v1.9.0"
```

### 3. Create a git tag (recommended)

```bash
git tag v1.9.0
git push origin main --tags
```

### 4. Dry-run publish

```bash
cargo publish --dry-run
```

This runs `cargo package` and verifies everything without actually uploading. Fix any errors or warnings before proceeding.

### 5. Publish for real

```bash
cargo publish
```

This:
- Packages the crate into a `.crate` archive
- Uploads it to crates.io
- Makes it publicly available and **permanent** (you cannot delete a published version)

### 6. Verify

- Visit `https://crates.io/crates/langfuse-rust-sdk`
- Docs will auto-build at `https://docs.rs/langfuse-rust-sdk` within a few minutes

---

## Publishing a New Version

This section covers the full end-to-end workflow for releasing an update to an already-published crate.

### Versioning strategy (SemVer)

Follow [Semantic Versioning](https://semver.org/) when choosing the next version number:

| Change type | Version bump | Example |
|-------------|-------------|---------|
| Bug fix, docs, internal refactor (no API change) | **Patch** | `1.2.3` → `1.2.4` |
| New public API, backwards-compatible feature | **Minor** | `1.2.3` → `1.3.0` |
| Breaking change to public API | **Major** | `1.2.3` → `2.0.0` |

> You **cannot** re-publish the same version. If you made a mistake, bump the patch version and republish.

---

### Step 1 — Update the version in `Cargo.toml`

`langfuse-rust-sdk` and `langfuse-rust-sdk-macros` share one version, set in the root `Cargo.toml`. Bump it together with the exact pin on the macros crate:

```toml
[workspace.package]
version = "1.9.1"   # was 1.9.0

[workspace.dependencies]
langfuse-rust-sdk-macros = { version = "=1.9.1", path = "macros" }
```

If the crate has a `Cargo.lock` checked in (binary/application crates), update it too:

```bash
cargo update --workspace
```

---

### Step 2 — Update the CHANGELOG

Document what changed. A minimal entry looks like this:

```markdown
## [1.9.1] - 2025-07-10

### Fixed
- Resolved incorrect timeout on retry logic (#42)

### Added
- New `rerank_batch` helper for bulk reranking
```

Keeping a changelog makes it easier for users to understand what changed and decide whether to upgrade.

---

### Step 3 — Run the pre-publish checklist

```bash
# Format
cargo fmt --check

# Lint — treat warnings as errors to keep the codebase clean (every workspace member)
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Tests (unit + integration + doc-tests), then the rig / AWS SDK tests of the
# unpublished integration-tests member
cargo test --all-features
cargo test -p langfuse-rust-sdk-integration-tests --all-features

# The package must not contain integration-tests/ (it is a separate, unpublished member)
cargo package --list | grep -c '^integration-tests/'   # prints 0

# Docs — no broken links or missing public items
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps

# (powershell)
$env:RUSTDOCFLAGS="-D warnings"; cargo doc --no-deps

# Dry-run of both crates: catches metadata errors before uploading, and verifies the
# main crate against the new macros version before either is on crates.io
cargo publish --dry-run -p langfuse-rust-sdk-macros -p langfuse-rust-sdk
```

Fix every error or warning before continuing. The dry-run simulates the full publish process without uploading.

---

### Step 4 — Commit and tag

All changes must be committed. The tag acts as a permanent reference to the exact source for this release:

```bash
git add Cargo.toml Cargo.lock CHANGELOG.md   # add any other changed files
git commit -m "Release v1.9.1"

git tag v1.9.1
git push origin main --tags
```

> **Tip:** Push the tag before publishing so the crates.io page and the git history stay in sync.

---

### Step 5 — Publish

```bash
cargo publish -p langfuse-rust-sdk-macros -p langfuse-rust-sdk
```

Cargo publishes them in dependency order (the macros crate first, since the main crate pins its exact version), uploads each one, and prints a confirmation URL. The new version is live immediately but docs.rs may take a few minutes to build.

---

### Step 6 — Verify the release

1. Check the crates.io page:
   ```
   https://crates.io/crates/langfuse-rust-sdk
   ```
2. Confirm the new version appears at the top of the version list.
3. Wait ~5 minutes, then verify docs.rs built successfully:
   ```
   https://docs.rs/langfuse-rust-sdk/<new-version>
   ```
4. Optionally, create a GitHub Release pointing to the tag and paste in the CHANGELOG entry.

---

### Updating dependencies in downstream projects

After publishing, users can upgrade with:

```bash
cargo update -p langfuse-rust-sdk
```

Or by editing their `Cargo.toml` to pin the new version:

```toml
[dependencies]
langfuse-rust-sdk = "1.9.1"
```

---

### Common issues when publishing updates

| Error | Cause | Fix |
|-------|-------|-----|
| `cannot publish a new version of a crate that already exists` with same version | Version not bumped | Increment `version` in `Cargo.toml` |
| `working directory contains uncommitted changes` | Dirty git state | `git add -A && git commit` |
| `failed to verify package tarball` | Code doesn't compile from the packaged archive | Run `cargo package` locally and fix build errors |
| `package size exceeds 10MB` | Large test fixtures or generated files included | Add paths to `.gitignore` or use `exclude` in `Cargo.toml` |
| Docs.rs build fails | Feature-gated code or platform-specific dependencies | Check the build log at `https://docs.rs/crate/langfuse-rust-sdk/<version>/builds` |

---

## Yanking a version

If a published version has a critical bug, you can yank it (prevents new projects from depending on it, but existing lockfiles still work):

```bash
cargo yank --version 1.9.0
```

To undo:

```bash
cargo yank --version 1.9.0 --undo
```

---

## Common errors and fixes

| Error | Cause | Fix |
|-------|-------|-----|
| `manifest has no documentation, homepage or repository` | Warning — missing recommended fields | Add `repository`, `homepage`, `documentation` to `[package]` |
| `files in the working directory contain changes that were not yet committed` | Uncommitted changes | `git add -A && git commit` |
| `failed to verify package tarball` | Code doesn't compile from the packaged archive | Run `cargo package` and fix build errors |
| `the `license` field is not a valid SPDX expression` | Invalid license string | Use a valid SPDX identifier like `"MIT"`, `"Apache-2.0"`, or `"MIT OR Apache-2.0"` |
| `package exceeds the 10MB limit` | Too many or too large files included | Add unneeded files to `.gitignore` or use `exclude` in `Cargo.toml` |

---


Once those fields are added, all pre-publish checks pass, and the code is committed and tagged — you're ready to `cargo publish`.