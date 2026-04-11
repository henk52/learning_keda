# API Service

### Crates needed

- async-nats - nats communication
- [axum](https://docs.rs/axum/latest/axum/) - web application framework.
- [opentelemetry](https://docs.rs/opentelemetry/latest/opentelemetry/) - Implements the API component of OpenTelemetry.
- [opentelemetry_sdk](https://crates.io/crates/opentelemetry_sdk) - encompassing several aspects of OpenTelemetry, such as context management and propagation, logging, tracing, and metrics.
  - TODO should these feature be enabled?
    - rt-tokio
    - opentelemetry-http
    - serde
    - serde_json
    - tokio
- [opentelemetry-otlp](https://crates.io/crates/opentelemetry-otlp) - enables exporting telemetry data in the OpenTelemetry Protocol (OTLP) format to compatible backends.
- serde - for serde_json
- [serde_json](https://docs.rs/serde_json/latest/serde_json/) - json
- [tokio](https://docs.rs/tokio/1.47.1/tokio/) - threads etc
  - TODO should these feature be enabled?
    - tracing

## Requirements

- must respond to app health on /health

## Accessing the telemetry output

### Accessing the traces

- In the Grafana UI (http://localhost:3000, admin/admin):
- Home menu → Explore (compass icon)
- In the datasource dropdown, select Tempo
- Query options:
- Search tab — filter by service name api-service, then click Run query to see recent traces
- TraceQL tab — run a query like:
- Click any trace in the results to open the flame graph / waterfall view showing all spans

## TODO

- web api
  - HANDSON_MICROSERVICES_WITH_RUST-9781789342758.pdf p14
  - please create an api server in api-service that handles /health, /session /user using axum
- log
  - receieve message, from where and what payload
  - log whether connection was authenticated or not.
- metrics
  - number of connections
    - current
    - total
  - size of payload
    - possibly per client
- trace
  - each api call

- first get trace up and running
- send to the otel-lmgt
- start logs
- start metrics
- switch to send telemetry via otlp-collector

Source: https://github.com/open-telemetry/opentelemetry-rust/tree/main
If you are starting fresh, we recommend using tracing as your logging API. It supports structured logging and is actively maintained. OpenTelemetry itself uses tracing for its internal logging.

Project versioning information and stability guarantees can be found here.

## Installation

- cargo add opentelemetry_semantic_conventions --features semconv_experimental
- cargo add opentelemetry-stdout



curl -X POST http://localhost:8080/slow -H "Content-Type: application/json" -d '{"sleep_ms": 900}'
curl -X POST http://localhost:8080/fast -H "Content-Type: application/json" -d '{"sleep_ms": 8}'
curl http://localhost:8080/fast
curl http://localhost:8080/slow