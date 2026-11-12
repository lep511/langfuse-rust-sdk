"""El CLI de Langfuse desde Python.

Todo pasa por `bin/langfuse`, que elige el binario y le da las credenciales
de `.env`: este módulo sólo lo ejecuta e interpreta su salida.
"""

import json
import shlex
import subprocess
from pathlib import Path

LAUNCHER = Path(__file__).resolve().parents[2] / "bin" / "langfuse"

# El CLI ya corta cada petición a los 30 s y la reintenta dos veces.
TIMEOUT_SECONDS = 120


class LangfuseCliError(RuntimeError):
    """El CLI no se pudo ejecutar o terminó con un código distinto de 0."""


def run(*args, body=None):
    """Ejecuta `langfuse <args>` y devuelve su salida estándar.

    `body` es el cuerpo JSON de la petición: va por la entrada estándar
    (`--body-file -`), no en la línea de comandos.
    """
    command = [str(LAUNCHER), *args]
    if body is None:
        stdin = {"stdin": subprocess.DEVNULL}
    else:
        command += ["--body-file", "-"]
        stdin = {"input": json.dumps(body, ensure_ascii=False)}
    try:
        done = subprocess.run(
            command,
            capture_output=True,
            encoding="utf-8",
            timeout=TIMEOUT_SECONDS,
            **stdin,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise LangfuseCliError(f"no se pudo ejecutar {LAUNCHER}: {error}") from error
    if done.returncode != 0:
        raise LangfuseCliError(
            f"`langfuse {' '.join(args)}` terminó con código {done.returncode}: {_failure(done)}"
        )
    return done.stdout


def api(resource, action, *args, body=None):
    """Llama a la API (`langfuse --json api <resource> <action> ...`).

    Devuelve la respuesta como la imprime `--json`: `{status, headers, body}`.
    """
    stdout = run("--json", "api", resource, action, *args, body=body)
    try:
        return json.loads(stdout)
    except ValueError as error:
        raise LangfuseCliError(f"`langfuse api {resource} {action}` no devolvió JSON") from error


def base_url():
    """La URL de Langfuse que usa el CLI (flag, `.env`, entorno o la de por defecto)."""
    # `--curl` imprime la petición sin enviarla: `curl --request GET <url> --header ...`.
    preview = shlex.split(run("api", "health", "get", "--curl"))
    suffix = "/api/public/health"
    for word in preview:
        if word.startswith(("https://", "http://")) and word.endswith(suffix):
            return word.removesuffix(suffix)
    raise LangfuseCliError("`langfuse api health get --curl` no muestra la URL de la petición")


def trace_url(trace_id):
    """Dónde muestra Langfuse una traza del proyecto de las credenciales."""
    response = api("projects", "list")
    try:
        project = response["body"]["data"][0]["id"]
    except (LookupError, TypeError) as error:
        raise LangfuseCliError("`langfuse api projects list` no devolvió el proyecto") from error
    return f"{base_url()}/project/{project}/traces/{trace_id}"


def _failure(done):
    """Por qué falló el CLI, en una línea."""
    detail = done.stderr.strip()
    if detail:
        return detail
    # Ante un error HTTP (código 5) el CLI imprime igual la respuesta.
    try:
        response = json.loads(done.stdout)
        return f"HTTP {response['status']}: {json.dumps(response['body'], ensure_ascii=False)}"
    except (ValueError, KeyError, TypeError):
        return done.stdout.strip()
