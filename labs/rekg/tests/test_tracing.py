"""Pruebas de `rekg.tracing` y de `bin/langfuse`, sin red: un CLI falso anota
cada invocación (argumentos y cuerpo) y responde lo que diga `config.json`.

    uv run python -m unittest discover -s tests
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from rekg import langfuse_cli
from rekg.tracing import Trace

PROJECT = Path(__file__).resolve().parents[1]

FAKE_CLI = """
import json
import sys
from pathlib import Path

here = Path(__file__).parent
config = json.loads((here / "config.json").read_text())
args = sys.argv[1:]
body = json.load(sys.stdin) if "--body-file" in args else None
with open(here / "calls.jsonl", "a") as calls:
    calls.write(json.dumps({"args": args, "body": body}) + "\\n")

responses = {"projects": {"status": 200, "body": {"data": [{"id": "project-1"}]}}}
responses.update(config.get("responses", {}))
if "stderr" in config:
    sys.stderr.write(config["stderr"] + "\\n")
elif "--curl" in args:
    print("curl --request GET https://langfuse.test/api/public/health --header 'accept: application/json'")
elif "api" in args:
    response = responses.get(args[args.index("api") + 1], {"status": 200, "body": {}})
    print(json.dumps({"headers": {}, **response}))
sys.exit(config.get("exit", 0))
"""


class FakeCli:
    """Un CLI falso en un directorio temporal."""

    def __init__(self, test, **config):
        directory = tempfile.TemporaryDirectory()
        test.addCleanup(directory.cleanup)
        self.directory = Path(directory.name)
        self.path = self.directory / "langfuse"
        self.path.write_text(f"#!{sys.executable}\n{FAKE_CLI}")
        self.path.chmod(0o755)
        (self.directory / "config.json").write_text(json.dumps(config))

    def calls(self):
        """Las invocaciones, en orden: `{"args": [...], "body": ...}`."""
        log = self.directory / "calls.jsonl"
        return [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []


def attributes(span):
    """Los atributos de un span OTLP/JSON como diccionario."""
    values = {}
    for attribute in span["attributes"]:
        value = attribute["value"]
        if "arrayValue" in value:
            values[attribute["key"]] = [item["stringValue"] for item in value["arrayValue"]["values"]]
        else:
            values[attribute["key"]] = value["stringValue"]
    return values


class TraceTests(unittest.TestCase):
    def fake_cli(self, **config):
        cli = FakeCli(self, **config)
        patch = mock.patch.object(langfuse_cli, "LAUNCHER", cli.path)
        patch.start()
        self.addCleanup(patch.stop)
        return cli

    def test_the_trace_and_its_scores_are_sent_with_the_cli(self):
        cli = self.fake_cli()

        with Trace("run", input={"bucket": "b"}, tags=["rekg"]) as trace:
            with trace.span("step", input="texto") as span:
                span.output = [{"value": "Egg", "confidence": 96.9}]
                span.metadata = {"model_version": "3.0", "attempts": 1}
                span.score("confidence", 96.9, comment="uno")
            trace.root.output = {"labels": ["Egg"]}

        self.assertIsNone(trace.error)
        self.assertEqual(trace.url, f"https://langfuse.test/project/project-1/traces/{trace.id}")
        projects, preview, export, scores = cli.calls()
        self.assertEqual(projects["args"], ["--json", "api", "projects", "list"])
        self.assertEqual(preview["args"], ["api", "health", "get", "--curl"])
        self.assertEqual(
            export["args"], ["--json", "api", "otel", "export-traces", "--body-file", "-"]
        )
        self.assertEqual(scores["args"], ["--json", "api", "scores", "create", "--body-file", "-"])

        (resource_spans,) = export["body"]["resourceSpans"]
        root, step = resource_spans["scopeSpans"][0]["spans"]
        self.assertEqual(len(trace.id), 32)
        self.assertEqual(len(root["spanId"]), 16)
        self.assertEqual((root["traceId"], step["traceId"]), (trace.id, trace.id))
        self.assertNotIn("parentSpanId", root)
        self.assertEqual(step["parentSpanId"], root["spanId"])
        self.assertLessEqual(int(root["startTimeUnixNano"]), int(step["startTimeUnixNano"]))
        self.assertLessEqual(int(step["endTimeUnixNano"]), int(root["endTimeUnixNano"]))
        self.assertNotIn("status", step)
        self.assertEqual(
            attributes(root),
            {
                "langfuse.trace.name": "run",
                "langfuse.trace.tags": ["rekg"],
                "langfuse.observation.input": '{"bucket": "b"}',
                "langfuse.observation.output": '{"labels": ["Egg"]}',
            },
        )
        self.assertEqual(
            attributes(step),
            {
                "langfuse.trace.name": "run",
                "langfuse.trace.tags": ["rekg"],
                "langfuse.observation.input": "texto",
                "langfuse.observation.output": '[{"value": "Egg", "confidence": 96.9}]',
                # Como JSON: el texto "3.0" no debe llegar como el número 3.
                "langfuse.observation.metadata.model_version": '"3.0"',
                "langfuse.observation.metadata.attempts": "1",
            },
        )

        (score,) = scores["body"]
        self.assertEqual(
            {key: score[key] for key in ("traceId", "observationId", "name", "value", "comment")},
            {
                "traceId": trace.id,
                "observationId": step["spanId"],
                "name": "confidence",
                "value": 96.9,
                "comment": "uno",
            },
        )
        self.assertEqual(score["dataType"], "NUMERIC")
        self.assertEqual(score["id"], f"{trace.id}-{step['spanId']}-confidence")
        self.assertTrue(score["timestamp"].endswith("+00:00"))

    def test_a_trace_without_scores_does_not_call_scores_create(self):
        cli = self.fake_cli()

        with Trace("run"):
            pass

        self.assertEqual(
            [call["args"][-3:] for call in cli.calls()],
            [
                ["api", "projects", "list"],
                ["health", "get", "--curl"],
                ["export-traces", "--body-file", "-"],
            ],
        )

    def test_an_exception_is_recorded_as_an_error_and_goes_on(self):
        cli = self.fake_cli()

        with self.assertRaisesRegex(ValueError, "sin imagen"):
            with Trace("run") as trace:
                with trace.span("step"):
                    raise ValueError("sin imagen")

        self.assertIsNotNone(trace.url)
        export = cli.calls()[2]
        for span in export["body"]["resourceSpans"][0]["scopeSpans"][0]["spans"]:
            self.assertEqual(span["status"], {"code": 2, "message": "ValueError: sin imagen"})

    def test_a_cli_failure_is_reported_and_nothing_is_sent(self):
        cli = self.fake_cli(exit=3, stderr="This operation requires LANGFUSE_PUBLIC_KEY")

        with Trace("run") as trace:
            pass

        self.assertIsNone(trace.url)
        self.assertIn("código 3", trace.error)
        self.assertIn("This operation requires LANGFUSE_PUBLIC_KEY", trace.error)
        self.assertEqual(len(cli.calls()), 1)

    def test_an_http_error_shows_the_response_of_the_api(self):
        unauthorized = {"status": 401, "body": {"message": "Invalid credentials"}}
        self.fake_cli(exit=5, responses={"projects": unauthorized})

        with Trace("run") as trace:
            pass

        self.assertIsNone(trace.url)
        self.assertIn('código 5: HTTP 401: {"message": "Invalid credentials"}', trace.error)

    def test_rejected_scores_are_reported_with_the_trace_sent(self):
        rejected = {"accepted": 0, "rejected": 1, "errors": [{"index": 0, "message": "bad"}]}
        self.fake_cli(responses={"scores": {"status": 207, "body": rejected}})

        with Trace("run") as trace:
            trace.root.score("confidence", 1)

        self.assertIsNotNone(trace.url)
        self.assertIn("rechazó scores", trace.error)
        self.assertIn('"message": "bad"', trace.error)

    def test_a_missing_cli_is_reported(self):
        patch = mock.patch.object(langfuse_cli, "LAUNCHER", Path("/nonexistent/langfuse"))
        patch.start()
        self.addCleanup(patch.stop)

        with Trace("run") as trace:
            pass

        self.assertIsNone(trace.url)
        self.assertIn("no se pudo ejecutar /nonexistent/langfuse", trace.error)


class LauncherTests(unittest.TestCase):
    """`bin/langfuse`, copiado a un proyecto temporal."""

    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name).resolve()
        (self.root / "bin").mkdir()
        self.launcher = shutil.copy(PROJECT / "bin" / "langfuse", self.root / "bin" / "langfuse")
        self.cli = FakeCli(self)

    def run_launcher(self, *args, cli=None):
        environment = {**os.environ, "LANGFUSE_CLI": str(cli or self.cli.path)}
        return subprocess.run([self.launcher, *args], env=environment, capture_output=True, text=True)

    def test_the_env_file_of_the_project_is_passed_to_the_cli(self):
        (self.root / ".env").write_text("LANGFUSE_PUBLIC_KEY=pk-lf-test\n")

        done = self.run_launcher("api", "health", "get")

        self.assertEqual(done.returncode, 0, done.stderr)
        (call,) = self.cli.calls()
        self.assertEqual(call["args"], ["--env", str(self.root / ".env"), "api", "health", "get"])

    def test_without_an_env_file_the_cli_reads_the_environment(self):
        done = self.run_launcher("api", "health", "get")

        self.assertEqual(done.returncode, 0, done.stderr)
        (call,) = self.cli.calls()
        self.assertEqual(call["args"], ["api", "health", "get"])

    def test_a_missing_binary_is_exit_127(self):
        done = self.run_launcher("--version", cli="/nonexistent/langfuse")

        self.assertEqual(done.returncode, 127)
        self.assertIn("/nonexistent/langfuse", done.stderr)


if __name__ == "__main__":
    unittest.main()
