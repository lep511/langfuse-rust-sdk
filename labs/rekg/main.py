import boto3

BUCKET = 'data-bucket-4938-40001'
MIN_CONFIDENCE = 75  # solo se muestran resultados con confianza >= este umbral (%)

client = boto3.client('rekognition', region_name='us-east-2')

print(f"Umbral de confianza: {MIN_CONFIDENCE}%")

# --- Detección de etiquetas ---
response = client.detect_labels(
    Image={
        'S3Object': {
            'Bucket': BUCKET,
            'Name': 'ap_19073628668117.jpg'
        }
    },
    MaxLabels=10,
    MinConfidence=MIN_CONFIDENCE
)

print("\nEtiquetas detectadas en ap_19073628668117.jpg:")
for label in response['Labels']:
    print(label['Name'])

# --- Extracción de texto ---
response = client.detect_text(
    Image={
        'S3Object': {
            'Bucket': BUCKET,
            'Name': 'receipt.jpeg'
        }
    }
)

# Rekognition devuelve cada línea (LINE) y cada palabra (WORD); se muestran solo las líneas.
print("\nTexto detectado en receipt.jpeg:")
for detection in response['TextDetections']:
    if detection['Type'] == 'LINE' and detection['Confidence'] >= MIN_CONFIDENCE:
        print(detection['DetectedText'])
