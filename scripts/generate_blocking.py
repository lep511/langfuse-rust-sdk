"""Generate src/blocking/api.rs: blocking wrappers of every client in src/api/*.rs.

Run from the repository root after adding or changing an API method:

    python3 scripts/generate_blocking.py

`tests/blocking_tests.rs` fails when a public client method has no wrapper.
"""
import glob
import json
import os
import re
import subprocess
import sys

def scan(root):
    out = {}
    for path in sorted(glob.glob(os.path.join(root, "src/api/*.rs"))):
        src = open(path).read()
        # find impl blocks for *Client
        for m in re.finditer(r'^impl(?:<[^>]*>)? (\w+Client)(?:<[^>]*>)? \{', src, re.M):
            name = m.group(1)
            start = m.end()
            depth = 1; i = start
            while depth:
                c = src[i]
                if c == '{': depth += 1
                elif c == '}': depth -= 1
                i += 1
            body = src[start:i-1]
            methods = []
            for fm in re.finditer(r'^    (?:#\[cfg[^\n]*\]\n    )?pub (async )?fn (\w+)(<[^(]*>)?\(([^)]*)\)\s*(?:->\s*([^{]+?))?\s*(?:where[^{]*)?\{', body, re.M | re.S):
                methods.append({"async": bool(fm.group(1)), "name": fm.group(2), "generics": fm.group(3), "args": " ".join(fm.group(4).split()), "ret": " ".join((fm.group(5) or "()").split())})
            out.setdefault(os.path.basename(path), {})[name] = methods
    return out


root = sys.argv[1] if len(sys.argv) > 1 else os.getcwd()
methods = scan(root)

UNSTABLE = {"unstable_dashboard_widgets.rs", "unstable_dashboards.rs", "unstable_topics.rs"}


def top_level_uses(src):
    out = []
    for m in re.finditer(r"^use [^;]*;", src, re.M | re.S):
        out.append(m.group(0))
    return out


def arg_names(args):
    names = []
    for part in split_args(args):
        part = part.strip()
        if not part or part.endswith("self"):
            continue
        names.append(part.split(":")[0].strip())
    return names


def split_args(args):
    depth = 0
    cur = ""
    parts = []
    for c in args:
        if c in "<([":
            depth += 1
        elif c in ">)]":
            depth -= 1
        if c == "," and depth == 0:
            parts.append(cur)
            cur = ""
        else:
            cur += c
    parts.append(cur)
    return [p.strip() for p in parts if p.strip()]


lines = [
    "//! Blocking wrappers of the API clients (generated from `src/api/*.rs`;",
    "//! the `blocking_tests` test checks that none is missing).",
    "",
]
fields = []
for fname in sorted(methods):
    clients = methods[fname]
    mod = fname[:-3]
    src = open(os.path.join(root, "src/api", fname)).read()
    uses = top_level_uses(src)
    cfg = '#[cfg(feature = "unstable")]\n' if fname in UNSTABLE else ""
    lines.append(f"/// Blocking clients of [`crate::api::{mod}`].")
    lines.append(f"{cfg}pub mod {mod} {{")
    lines.append("    #![allow(unused_imports)]")
    lines.append(f"    pub use crate::api::{mod}::*;")
    for use in uses:
        lines.append("    " + use.replace("\n", "\n    "))
    lines.append("    use crate::blocking::{BlockingPages, Runtime};")
    lines.append("")
    for client, ms in clients.items():
        path = f"crate::api::{mod}::{client}"
        lines.append(f"    /// Blocking version of [`{client}`]({path}).")
        lines.append("    #[derive(Clone)]")
        lines.append(f"    pub struct {client} {{")
        lines.append(f"        pub(crate) inner: {path},")
        lines.append("        pub(crate) runtime: std::sync::Arc<Runtime>,")
        lines.append("    }")
        lines.append("")
        lines.append(f"    impl std::fmt::Debug for {client} {{")
        lines.append("        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {")
        lines.append(f'            f.debug_struct("{client}").finish_non_exhaustive()')
        lines.append("        }")
        lines.append("    }")
        lines.append("")
        lines.append(f"    impl {client} {{")
        lines.append("        crate::blocking::blocking_methods! {")
        for m in ms:
            if not m["async"]:
                continue
            args = ", ".join(split_args(m["args"]))
            lines.append(f"            /// Blocking [`{m['name']}`]({path}::{m['name']}).")
            lines.append(f"            fn {m['name']}({args}) -> {m['ret']};")
        lines.append("        }")
        for m in ms:
            if m["async"]:
                continue
            args = ", ".join(split_args(m["args"]))
            call = ", ".join(arg_names(m["args"]))
            generics = m["generics"] or ""
            if m["ret"].startswith("Pages<"):
                ret = "BlockingPages" + m["ret"][len("Pages"):]
                lines.append("")
                lines.append(f"        /// Blocking [`{m['name']}`]({path}::{m['name']}): an iterator over the pages.")
                lines.append(f"        pub fn {m['name']}{generics}({args}) -> {ret} {{")
                lines.append(f"            BlockingPages::new(self.inner.{m['name']}({call}), &self.runtime)")
                lines.append("        }")
            else:
                ret = "" if m["ret"] == "()" else f" -> {m['ret']}"
                lines.append("")
                lines.append(f"        /// See [`{m['name']}`]({path}::{m['name']}).")
                lines.append(f"        pub fn {m['name']}{generics}({args}){ret} {{")
                lines.append(f"            self.inner.{m['name']}({call})")
                lines.append("        }")
        lines.append("    }")
        lines.append("")
    lines.append("}")
    lines.append("")

# Fields of LangfuseClient, in declaration order.
lib = open(os.path.join(root, "src/lib.rs")).read()
struct = lib[lib.index("pub struct LangfuseClient {"):]
struct = struct[: struct.index("\n}\n")]
client_modules = {c: f[:-3] for f, cs in methods.items() for c in cs}
field_lines = []
pending_cfg = ""
pending_doc = []
for line in struct.splitlines()[1:]:
    s = line.strip()
    if s.startswith("///"):
        pending_doc.append(s)
        continue
    if s.startswith("#[cfg"):
        pending_cfg = s
        continue
    m = re.match(r"pub (\w+): (\w+Client),", s)
    if m:
        fields.append((m.group(1), m.group(2), client_modules[m.group(2)], pending_cfg, pending_doc))
    pending_cfg = ""
    pending_doc = []

lines.append("/// Blocking entry point of the SDK: the same sub-clients as")
lines.append("/// [`crate::LangfuseClient`], whose methods block until the request finishes.")
lines.append("///")
lines.append("/// See the [module docs](crate::blocking) for the runtime it runs on.")
lines.append("#[derive(Clone)]")
lines.append("#[non_exhaustive]")
lines.append("pub struct LangfuseClient {")
for name, ty, mod, cfg, doc in fields:
    for d in doc:
        lines.append(f"    {d}")
    if cfg:
        lines.append(f"    {cfg}")
    lines.append(f"    pub {name}: {mod}::{ty},")
lines.append("    pub(crate) inner: crate::LangfuseClient,")
lines.append("    pub(crate) runtime: std::sync::Arc<Runtime>,")
lines.append("}")
lines.append("")
lines.append("impl LangfuseClient {")
lines.append("    /// Wrap `client`, running its requests on `runtime`.")
lines.append("    pub(crate) fn assemble(client: crate::LangfuseClient, runtime: std::sync::Arc<Runtime>) -> Self {")
lines.append("        Self {")
for name, ty, mod, cfg, doc in fields:
    if cfg:
        lines.append(f"            {cfg}")
    lines.append(f"            {name}: {mod}::{ty} {{ inner: client.{name}.clone(), runtime: std::sync::Arc::clone(&runtime) }},")
lines.append("            inner: client,")
lines.append("            runtime,")
lines.append("        }")
lines.append("    }")
lines.append("}")
lines.append("")
lines.append("use crate::blocking::Runtime;")
open(os.path.join(root, "src/blocking/api.rs"), "w").write("\n".join(lines) + "\n")
subprocess.run(["rustfmt", "--edition", "2024", os.path.join(root, "src/blocking/api.rs")], check=True)
print(f"src/blocking/api.rs: {len(fields)} sub-clients")
