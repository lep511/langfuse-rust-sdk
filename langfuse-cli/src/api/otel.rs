//! `langfuse api otel` (aliases `otel-v1`, `opentelemetry`).

use crate::operation::{HttpMethod, OperationMeta};

json_op! {
    ExportTraces {}
    meta = OperationMeta::new(
        "otel", "export-traces", "opentelemetry_exportTraces",
        HttpMethod::Post, "/api/public/otel/v1/traces",
    )
    // Real-time ingestion: the spec requires this header from direct exporters.
    .headers(&[("x-langfuse-ingestion-version", "4")]);
}

resource! {
    pub enum Command {
        /// Ingest OpenTelemetry traces (OTLP/HTTP JSON)
        ///
        /// Sent with `x-langfuse-ingestion-version: 4` (real-time ingestion). Together with the
        /// Observations and Metrics APIs v2 this is the real-time path.
        #[command(after_help = "The body is an OTLP ExportTraceServiceRequest: \
            {\"resourceSpans\": [{\"resource\": {…}, \"scopeSpans\": [{\"spans\": […]}]}]}.\n\n\
            Example:\n  langfuse api otel export-traces --body-file spans.json")]
        ExportTraces(ExportTraces),
    }
}
