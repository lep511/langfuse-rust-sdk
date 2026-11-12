import sys

import boto3

from rekg.tracing import Trace

BUCKET = 'data-bucket-4938-40001'
MIN_CONFIDENCE = 75  # solo se muestran resultados con confianza >= este umbral (%)

client = boto3.client('rekognition', region_name='us-east-2')


def record(span, response, model_version, results):
    """Deja en la observación de Langfuse lo que devolvió Rekognition.

    `results` son los pares (resultado, confianza) que se muestran; su confianza
    media es el score `confidence` de la observación.
    """
    span.output = [
        {'value': value, 'confidence': round(confidence, 2)} for value, confidence in results
    ]
    span.metadata = {
        'request_id': response['ResponseMetadata']['RequestId'],
        'model_version': response[model_version],
    }
    if results:
        mean = sum(confidence for _, confidence in results) / len(results)
        span.score(
            'confidence',
            round(mean, 2),
            comment=f"Confianza media (%) de {len(results)} resultados",
        )


# Cada corrida es una traza de Langfuse, con una observación por llamada a
# Rekognition. Se envía con el CLI (bin/langfuse) al salir del `with`.
with Trace(
    'rekg',
    input={'bucket': BUCKET, 'min_confidence': MIN_CONFIDENCE},
    tags=['rekg'],
) as trace:
    print(f"Umbral de confianza: {MIN_CONFIDENCE}%")

    # --- Detección de etiquetas ---
    request = {
        'Image': {
            'S3Object': {
                'Bucket': BUCKET,
                'Name': 'ap_19073628668117.jpg'
            }
        },
        'MaxLabels': 10,
        'MinConfidence': MIN_CONFIDENCE
    }
    with trace.span('rekognition.detect_labels', input=request) as span:
        response = client.detect_labels(**request)
        labels = [(label['Name'], label['Confidence']) for label in response['Labels']]
        record(span, response, 'LabelModelVersion', labels)

    print("\nEtiquetas detectadas en ap_19073628668117.jpg:")
    for name, _ in labels:
        print(name)

    # --- Extracción de texto ---
    request = {
        'Image': {
            'S3Object': {
                'Bucket': BUCKET,
                'Name': 'receipt.jpeg'
            }
        }
    }
    with trace.span('rekognition.detect_text', input=request) as span:
        response = client.detect_text(**request)
        # Rekognition devuelve cada línea (LINE) y cada palabra (WORD); se muestran solo las líneas.
        lines = [
            (detection['DetectedText'], detection['Confidence'])
            for detection in response['TextDetections']
            if detection['Type'] == 'LINE' and detection['Confidence'] >= MIN_CONFIDENCE
        ]
        record(span, response, 'TextModelVersion', lines)

    print("\nTexto detectado en receipt.jpeg:")
    for text, _ in lines:
        print(text)

    trace.root.output = {
        'labels': [name for name, _ in labels],
        'text': [text for text, _ in lines],
    }

if trace.url:
    print(f"\nTraza en Langfuse: {trace.url}")
if trace.error:
    print(f"\nLangfuse: {trace.error}", file=sys.stderr)
