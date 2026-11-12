"""Trazas de Langfuse, enviadas con el CLI.

Una corrida del programa es una traza. `Trace` crea su observación raíz y cada
`Trace.span` agrega debajo una observación, con su entrada, su salida, sus
metadatos y sus scores. Al salir del `with` todo se envía de una vez:

- las observaciones, como spans OTLP/JSON con los atributos `langfuse.*`
  (`langfuse api otel export-traces`);
- los scores (`langfuse api scores create`).

Si el CLI falla (sin credenciales, sin red, error de la API) el programa sigue:
`Trace.error` dice por qué.
"""

import json
import secrets
import time
from contextlib import contextmanager
from datetime import datetime, timezone

from . import langfuse_cli

SERVICE = "rekg"

SPAN_KIND_INTERNAL = 1
STATUS_CODE_ERROR = 2


class Observation:
    """Un paso de la traza: un span de OpenTelemetry con atributos de Langfuse."""

    def __init__(self, name, input=None, parent=None):
        self.id = secrets.token_hex(8)
        self.parent = parent
        self.name = name
        self.input = input
        self.output = None
        self.metadata = {}
        self.error = None
        self.scores = []
        self.started = time.time_ns()
        self.ended = None

    def score(self, name, value, comment=None):
        """Agrega un score numérico a la observación."""
        self.scores.append({"name": name, "value": value, "comment": comment})

    def _end(self, error):
        self.ended = time.time_ns()
        if error is not None:
            self.error = f"{type(error).__name__}: {error}"


class Trace:
    """Una traza de Langfuse, que se envía al salir del `with`.

    - `root` es la observación raíz: su entrada y su salida son las de la traza.
    - `url` es dónde la muestra Langfuse, una vez enviada.
    - `error` es por qué no se pudo enviar la traza o alguno de sus scores.
    """

    def __init__(self, name, input=None, tags=()):
        self.id = secrets.token_hex(16)
        self.name = name
        self.tags = list(tags)
        self.root = Observation(name, input)
        self.observations = [self.root]
        self.url = None
        self.error = None

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, traceback):
        self.root._end(exc)
        self._send()

    @contextmanager
    def span(self, name, input=None):
        """Una observación bajo la raíz, que dura lo que dura el `with`.

        Una excepción la deja con nivel ERROR y sigue su camino.
        """
        observation = Observation(name, input, parent=self.root)
        self.observations.append(observation)
        try:
            yield observation
        except BaseException as error:
            observation._end(error)
            raise
        observation._end(None)

    def _send(self):
        try:
            # Primero la URL: unas credenciales que no sirven se notan antes de enviar nada.
            url = langfuse_cli.trace_url(self.id)
            langfuse_cli.api("otel", "export-traces", body=self._otlp())
            self.url = url
            scores = self._scores()
            if scores:
                response = langfuse_cli.api("scores", "create", body=scores)
                # 202: todos aceptados. 207: algunos rechazados, con el motivo de cada uno.
                if response["status"] == 207:
                    rejected = json.dumps(response["body"], ensure_ascii=False)
                    raise langfuse_cli.LangfuseCliError(f"Langfuse rechazó scores: {rejected}")
        except langfuse_cli.LangfuseCliError as error:
            self.error = str(error)

    def _otlp(self):
        """La traza como `ExportTraceServiceRequest` de OTLP/JSON."""
        return {
            "resourceSpans": [
                {
                    "resource": {"attributes": [_attribute("service.name", SERVICE)]},
                    "scopeSpans": [
                        {
                            "scope": {"name": SERVICE},
                            "spans": [self._span(observation) for observation in self.observations],
                        }
                    ],
                }
            ]
        }

    def _span(self, observation):
        # Los atributos de la traza van en todos sus spans: Langfuse filtra y
        # agrega por observación.
        attributes = [_attribute("langfuse.trace.name", self.name)]
        if self.tags:
            attributes.append(_attribute("langfuse.trace.tags", self.tags))
        if observation.input is not None:
            attributes.append(_attribute("langfuse.observation.input", observation.input))
        if observation.output is not None:
            attributes.append(_attribute("langfuse.observation.output", observation.output))
        for key, value in observation.metadata.items():
            # Langfuse lee cada valor como JSON: sin codificar, el texto "3.0" sería el número 3.
            attributes.append(_attribute(f"langfuse.observation.metadata.{key}", _json(value)))

        span = {
            "traceId": self.id,
            "spanId": observation.id,
            "name": observation.name,
            "kind": SPAN_KIND_INTERNAL,
            "startTimeUnixNano": str(observation.started),
            "endTimeUnixNano": str(observation.ended),
            "attributes": attributes,
        }
        if observation.parent is not None:
            span["parentSpanId"] = observation.parent.id
        if observation.error is not None:
            # Langfuse lo muestra como nivel ERROR, con el mensaje.
            span["status"] = {"code": STATUS_CODE_ERROR, "message": observation.error}
        return span

    def _scores(self):
        """Los scores como cuerpo de `POST /api/public/scores`."""
        return [
            {
                # Con `id` y `timestamp` fijos, un reintento del CLI no duplica el score.
                "id": f"{self.id}-{observation.id}-{score['name']}",
                "timestamp": _iso(observation.ended),
                "traceId": self.id,
                "observationId": observation.id,
                "name": score["name"],
                "value": score["value"],
                "dataType": "NUMERIC",
                "comment": score["comment"],
            }
            for observation in self.observations
            for score in observation.scores
        ]


def _attribute(key, value):
    """Un atributo de OTLP/JSON: lista de textos, o un texto (JSON si el valor no es un texto)."""
    if isinstance(value, (list, tuple)) and all(isinstance(item, str) for item in value):
        any_value = {"arrayValue": {"values": [{"stringValue": item} for item in value]}}
    elif isinstance(value, str):
        any_value = {"stringValue": value}
    else:
        any_value = {"stringValue": _json(value)}
    return {"key": key, "value": any_value}


def _json(value):
    return json.dumps(value, ensure_ascii=False, default=str)


def _iso(nanoseconds):
    return datetime.fromtimestamp(nanoseconds / 1e9, timezone.utc).isoformat()
