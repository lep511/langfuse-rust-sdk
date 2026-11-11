#!/usr/bin/env python3
"""Download the Langfuse OpenAPI spec over api/openapi.yml and report what is new.

    python3 scripts/update_openapi.py             # report, overwrite the file, log the report
    python3 scripts/update_openapi.py --dry-run   # report only, no file is written
    python3 scripts/update_openapi.py --verbose   # also print documentation-only changes

The report compares the local spec with the downloaded one: new operations,
parameters, schemas, properties, enum values and union variants first, then
whatever was removed or changed. List items are matched by what identifies
them (parameter name, `$ref`, enum value), so a reordered list is not a change.

Every run also appends its report, dated, to api/openapi-updates.txt. As
each run compares with the spec the previous one wrote, an entry holds what
changed since the entry before it. The log always lists the documentation-only
changes, whatever --verbose says.

The spec is written exactly as downloaded, and only after the download parsed
as an OpenAPI document. Requires PyYAML (`pip install pyyaml`).
"""
import argparse
import itertools
import json
import sys
import urllib.request
from datetime import datetime
from pathlib import Path

try:
    import yaml
except ImportError:
    sys.exit("error: update_openapi.py needs PyYAML (pip install pyyaml)")

DEFAULT_URL = "https://cloud.langfuse.com/generated/api/openapi.yml"
REPOSITORY = Path(__file__).resolve().parent.parent
DEFAULT_OUTPUT = REPOSITORY / "api" / "openapi.yml"
DEFAULT_REPORT = REPOSITORY / "api" / "openapi-updates.txt"
REPORT_HEADER = """\
Langfuse OpenAPI spec updates

Written by scripts/update_openapi.py: every run appends an entry with the
differences between the spec it found and the one it downloaded, so each
entry holds what changed since the entry before it. Newest entry last.
"""
MAX_BYTES = 64 * 1024 * 1024
YAML_LOADER = getattr(yaml, "CSafeLoader", yaml.SafeLoader)

HTTP_METHODS = ("get", "put", "post", "delete", "options", "head", "patch", "trace")
# Operations under this prefix are the ones the SDK puts behind its `unstable` feature.
UNSTABLE_PREFIX = "/api/public/unstable/"

OTHER = "Other additions"
SECTIONS = (
    "New operations",
    "New parameters",
    "New schemas",
    "New properties",
    "New enum values",
    "New union variants",
    OTHER,
    "Removed",
    "Changed",
)
# Keywords whose children are identified by a name (map keys, or list items
# with an identity) instead of being further keywords: the noun for one child
# and the report section a new child goes to. Every child of `components` is
# treated the same way, whatever its keyword.
NAMED = {
    "paths": ("operation", "New operations"),
    "schemas": ("schema", "New schemas"),
    "properties": ("property", "New properties"),
    "parameters": ("", "New parameters"),  # list items are keyed `query parameter ...`
    "enum": ("enum value", "New enum values"),
    "oneOf": ("oneOf variant", "New union variants"),
    "anyOf": ("anyOf variant", "New union variants"),
    "allOf": ("allOf member", OTHER),
    "required": ("required property", "Changed"),
    "responses": ("response", OTHER),
    "content": ("content type", OTHER),
    "headers": ("header", OTHER),
    "tags": ("tag", OTHER),
    "securitySchemes": ("security scheme", OTHER),
    "mapping": ("discriminator mapping", OTHER),
}
# Keywords holding arbitrary data, compared as a whole.
OPAQUE = {"example", "examples", "default", "const", "externalDocs"}
# Keywords that do not change what the API accepts or returns.
DOCUMENTATION = {"description", "summary", "title", "example", "examples", "externalDocs"}
# How a keyword extends the label of the place it is found in (default: its name).
KEYWORD_LABELS = {
    "components": "",
    "schema": "",
    "requestBody": " request body",
    "items": "[]",
    "additionalProperties": ".*",
}


class Report:
    """Report lines grouped by section."""

    def __init__(self):
        self.lines = {section: [] for section in SECTIONS}
        self.documentation = []

    def add(self, section, line):
        self.lines[section].append(line)

    def change(self, where, key, old, new):
        """Record a keyword (or scalar child) whose value was set, unset or replaced."""
        if key in DOCUMENTATION:
            verb = "added" if old is None else "removed" if new is None else "changed"
            self.documentation.append(at(where, f"{key} {verb}"))
        else:
            self.add("Changed", at(where, f"{key} {short(old)} -> {short(new)}"))

    def render(self, verbose):
        """The lines of the report, with the documentation-only changes if `verbose`."""
        sections = dict(self.lines)
        if verbose:
            sections["Changed"] = sections["Changed"] + self.documentation
        out = []
        if not any(lines for section, lines in sections.items() if section not in ("Removed", "Changed")):
            out += ["Nothing new: no operation, parameter, schema, property or enum value was added.", ""]
        for section, lines in sections.items():
            if lines:
                marker = {"Removed": "-", "Changed": "~"}.get(section, "+")
                out.append(f"{section} ({len(lines)})")
                out += [f"  {marker} {printable(line)}" for line in lines]
                out.append("")
        if self.documentation and not verbose:
            out.append(
                f"{len(self.documentation)} documentation-only changes "
                "(descriptions, examples) are not listed, see --verbose."
            )
            out.append("")
        return out


def at(where, text):
    """Prefix a report line with the place it is about, if it has one."""
    return f"{where}: {text}" if where else str(text)


def short(value, limit=70):
    """Render a value on one line for the report."""
    if value is None:
        return "(unset)"
    text = json.dumps(value, default=str)
    return text if len(text) <= limit else text[: limit - 3] + "..."


def printable(text):
    """Keep control characters of the downloaded document out of the terminal."""
    return "".join(char if char.isprintable() else "?" for char in text)


def describe(schema):
    """Summarize a schema in a few words: its type, shape or the schema it refers to."""
    if not isinstance(schema, dict):
        return "any"
    union = next((key for key in ("oneOf", "anyOf", "allOf") if isinstance(schema.get(key), list)), None)
    if isinstance(schema.get("$ref"), str):
        text = schema["$ref"].rsplit("/", 1)[-1]
    elif isinstance(schema.get("enum"), list):
        text = "enum " + ", ".join(map(str, schema["enum"]))
    elif union:
        text = f"{union} of " + ", ".join(map(str, keyed(schema[union])))
    else:
        kind = schema.get("type", "any")
        text = " | ".join(map(str, kind)) if isinstance(kind, list) else str(kind)
        if kind == "array":
            text = f"array of {describe(schema.get('items'))}"
        elif isinstance(schema.get("properties"), dict):
            count = len(schema["properties"])
            text += f" with {count} propert{'y' if count == 1 else 'ies'}"
        elif "format" in schema:
            text += f" ({schema['format']})"
    return text + (", nullable" if schema.get("nullable") is True else "")


def keyed(node):
    """Return the children of a NAMED keyword as a dict, whether a map or a list.

    List items are keyed by what identifies them, so that inserting or
    reordering items is not reported as a change of every following one.
    """
    if isinstance(node, dict):
        return node
    keys, anonymous = [], itertools.count(1)
    for item in node or []:
        if isinstance(item, list):
            key = json.dumps(item, default=str)
        elif not isinstance(item, dict):
            key = item
        elif isinstance(item.get("$ref"), str):
            key = item["$ref"].rsplit("/", 1)[-1]
        elif "in" in item and "name" in item:
            key = f"{item['in']} parameter `{item['name']}`"
        elif isinstance(item.get("name", item.get("title")), str):
            key = item.get("name", item.get("title"))
        else:
            key = f"#{next(anonymous)}"
        keys.append(key)
    if len(set(keys)) < len(keys):
        keys = [f"#{index}" for index in range(1, len(keys) + 1)]
    return dict(zip(keys, node or []))


def operations(path, item):
    """The `METHOD path (operationId)` label of every operation of a path item."""
    labels = []
    for method, operation in item.items() if isinstance(item, dict) else ():
        if method in HTTP_METHODS:
            operation_id = operation.get("operationId") if isinstance(operation, dict) else None
            labels.append(f"{method.upper()} {path}" + (f" ({operation_id})" if operation_id else ""))
    return labels


def child_label(kind, name, where):
    """The label of the place `name` is, as a child of the NAMED keyword `kind` in `where`."""
    if kind in ("paths", "schemas"):
        return str(name)
    if kind == "properties":
        return f"{where}.{name}"
    if kind == "parameters":
        return f"{where} {name}".strip()
    if kind == "content":
        return where if name == "application/json" else f"{where} ({name})"
    if kind in ("oneOf", "anyOf", "allOf"):
        return f"{where} ({kind} {name})"
    return f"{where} {NAMED.get(kind, (kind,))[0]} {name}".strip()


def entry(kind, name, where, noun):
    """The report line of the child `name` of the NAMED keyword `kind` in `where`."""
    if kind == "properties":
        return f"{noun} {where}.{name}".strip()
    return at(where, f"{noun} {name}".strip())


def details(kind, name, value, owner):
    """What the report says about a new child `name` of the NAMED keyword `kind`."""
    if kind == "schemas":
        return f": {describe(value)}"
    if kind == "properties":
        required = ", required" if name in (owner.get("required") or []) else ""
        return f": {describe(value)}{required}"
    if kind == "parameters":
        required = "required" if value.get("required") is True else "optional"
        return f" ({describe(value.get('schema'))}, {required})"
    return ""


def diff_named(kind, old_owner, new_owner, where, report):
    """Compare the children of the NAMED keyword `kind` of two objects found in `where`."""
    old, new = keyed(old_owner.get(kind)), keyed(new_owner.get(kind))
    noun, section = NAMED.get(kind, (kind, OTHER))
    added = [name for name in new if name not in old]
    removed = [name for name in old if name not in new]

    if kind == "required":
        # A property that is itself new or gone is reported as such, not here.
        old_properties = old_owner.get("properties") or {}
        new_properties = new_owner.get("properties") or {}
        added = [name for name in added if name in old_properties or name not in new_properties]
        removed = [name for name in removed if name in new_properties or name not in old_properties]
        if added:
            report.add("Changed", at(where, "now required: " + ", ".join(map(str, added))))
        if removed:
            report.add("Changed", at(where, "no longer required: " + ", ".join(map(str, removed))))
        return

    if kind == "paths":
        for name in added:
            for label in operations(name, new[name]):
                report.add(section, label)
        for name in removed:
            for label in operations(name, old[name]):
                report.add("Removed", f"{noun} {label}")
    else:
        for names, children, target in ((added, new, section), (removed, old, "Removed")):
            # A section about one kind of child does not repeat its noun on every line.
            prefix = noun if target in (OTHER, "Removed") else ""
            # Scalar children (enum values, tags) share one line, objects get one each.
            scalars = [str(name) for name in names if not isinstance(children[name], dict)]
            if scalars:
                report.add(target, entry(kind, ", ".join(scalars), where, prefix))
            for name in names:
                if isinstance(children[name], dict):
                    detail = "" if target == "Removed" else details(kind, name, children[name], new_owner)
                    report.add(target, entry(kind, name, where, prefix) + detail)

    for name, value in old.items():
        if name not in new or value == new[name]:
            continue
        if isinstance(value, dict) and isinstance(new[name], dict):
            diff_object(value, new[name], child_label(kind, name, where), kind, report)
        else:
            report.change(where, f"{noun} {name}".strip(), value, new[name])


def diff_object(old, new, where, kind, report):
    """Compare two objects keyed by OpenAPI keywords: a schema, an operation, a response...

    `where` is the label of their place in the document and `kind` the keyword
    they were found under.
    """
    for key in [*new, *(key for key in old if key not in new)]:
        before, after = old.get(key), new.get(key)
        if before == after:
            continue
        present = [value for value in (before, after) if value is not None]
        if key in OPAQUE or not all(isinstance(value, (dict, list)) for value in present):
            report.change(where, key, before, after)
        elif kind == "paths" and key in HTTP_METHODS and all(isinstance(value, dict) for value in present):
            # An operation: `where` is its path.
            if before is None:
                report.add("New operations", operations(where, {key: after})[0])
            elif after is None:
                report.add("Removed", "operation " + operations(where, {key: before})[0])
            else:
                diff_object(before, after, f"{key.upper()} {where}", key, report)
        elif key in NAMED or kind == "components":
            diff_named(key, old, new, where, report)
        elif all(isinstance(value, dict) for value in present):
            label = (where + KEYWORD_LABELS.get(key, f" {key}")).strip()
            diff_object(before or {}, after or {}, label, key, report)
        else:
            report.change(where, key, before, after)


def download(url, timeout):
    request = urllib.request.Request(url, headers={"User-Agent": "langfuse-rust-sdk-update-openapi"})
    with urllib.request.urlopen(request, timeout=timeout) as response:
        data = response.read(MAX_BYTES + 1)
    if len(data) > MAX_BYTES:
        raise ValueError(f"{url} is larger than {MAX_BYTES} bytes")
    return data


def parse_spec(data, source):
    """Parse an OpenAPI document, refusing anything that does not look like one."""
    try:
        spec = yaml.load(data, Loader=YAML_LOADER)
    except yaml.YAMLError as error:
        raise ValueError(f"{source} is not valid YAML ({' '.join(str(error).split())})") from error
    if not isinstance(spec, dict) or "openapi" not in spec or not isinstance(spec.get("paths"), dict):
        raise ValueError(f"{source} is not an OpenAPI document (no `openapi` version or `paths`)")
    return spec


def counts(spec):
    """Number of operations, of unstable operations and of schemas."""
    paths = [
        path
        for path, item in spec["paths"].items()
        if isinstance(item, dict)
        for method in item
        if method in HTTP_METHODS
    ]
    components = spec.get("components")
    schemas = components.get("schemas") if isinstance(components, dict) else None
    return (
        len(paths),
        sum(str(path).startswith(UNSTABLE_PREFIX) for path in paths),
        len(schemas) if isinstance(schemas, dict) else 0,
    )


def display(path):
    """A path as the report shows it: relative to the repository when inside it."""
    path = path.resolve()
    return str(path.relative_to(REPOSITORY) if REPOSITORY in path.parents else path)


def append_report(path, started, url, target, lines):
    """Append the dated entry of one run to the report log, creating it if needed."""
    path.parent.mkdir(parents=True, exist_ok=True)
    entry = [
        "=" * 78,
        started.strftime("%Y-%m-%d %H:%M:%S %z"),
        f"Source: {url}",
        f"Target: {target}",
        "=" * 78,
        "",
        *lines,
    ]
    is_new = not path.exists() or path.stat().st_size == 0
    with open(path, "a", encoding="utf-8") as log:
        log.write((REPORT_HEADER if is_new else "") + "\n" + "\n".join(entry) + "\n")


def main():
    parser = argparse.ArgumentParser(
        description="Download the Langfuse OpenAPI spec over api/openapi.yml and report what is new."
    )
    parser.add_argument("--url", default=DEFAULT_URL, help="spec to download (default: %(default)s)")
    parser.add_argument(
        "--output",
        type=Path,
        default=DEFAULT_OUTPUT,
        help="file to compare with and overwrite (default: api/openapi.yml of this repository)",
    )
    parser.add_argument(
        "--report",
        type=Path,
        default=DEFAULT_REPORT,
        help="text file the dated report of every run is appended to "
        "(default: api/openapi-updates.txt of this repository)",
    )
    parser.add_argument("--dry-run", action="store_true", help="print the report without writing any file")
    parser.add_argument("--verbose", action="store_true", help="also print documentation-only changes")
    parser.add_argument("--timeout", type=float, default=60, help="download timeout in seconds (default: %(default)s)")
    args = parser.parse_args()
    started = datetime.now().astimezone()
    target, log = display(args.output), display(args.report)

    try:
        data = download(args.url, args.timeout)
        new = parse_spec(data, args.url)
    except (OSError, ValueError) as error:
        sys.exit(f"error: {error}; {target} was left untouched")
    print(f"Downloaded {args.url} ({len(data)} bytes)\n")

    # What the terminal shows and what the log records: the same report, the
    # log always with the documentation-only changes.
    previous = args.output.read_bytes() if args.output.exists() else None
    if previous == data:
        shown = logged = [f"No differences: {target} is already up to date."]
    else:
        old = None
        if previous is not None:
            try:
                old = parse_spec(previous, target)
            except ValueError as error:
                print(f"warning: {error}\n", file=sys.stderr)
        if old is None:
            totals = counts(new)
            shown = logged = [f"No previous spec to compare with at {target}.", ""]
        else:
            report = Report()
            diff_object(old, new, "", None, report)
            totals = [f"{was} -> {now}" if was != now else str(now) for was, now in zip(counts(old), counts(new))]
            shown, logged = report.render(args.verbose), report.render(True)
        summary = "Operations: {} ({} unstable), schemas: {}".format(*totals)
        shown, logged = shown + [summary], logged + [summary]
    print("\n".join(shown))

    if args.dry_run:
        print(f"Dry run: neither {target} nor {log} was modified.")
        return
    try:
        if previous != data:
            args.output.write_bytes(data)
            print(f"Wrote {target}.")
        append_report(args.report, started, args.url, target, logged)
    except OSError as error:
        sys.exit(f"error: {error}")
    print(f"Appended the report to {log}.")


if __name__ == "__main__":
    main()
