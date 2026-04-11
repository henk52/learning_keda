#[allow(unused_imports)]
use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::get,
};
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
use std::time::Duration;
use log::{Level, info};
use opentelemetry::{
    KeyValue, global,
    trace::{Span, Status, Tracer},
};
use opentelemetry_appender_log::OpenTelemetryLogBridge;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    Resource, logs::SdkLoggerProvider, metrics::SdkMeterProvider, trace::SdkTracerProvider,
};
use opentelemetry_semantic_conventions::resource::{
    K8S_NAMESPACE_NAME, SERVICE_NAME, SERVICE_VERSION,
};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

// TODO I think this initialize the tracer globally for the application.
fn init_tracer() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let endpoint = std::env::var("OTLP_TRACE_BACKEND_URL")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());

    let otlp_exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .with_timeout(std::time::Duration::from_secs(3))
        .build()?;

    // Create a resource with service information
    // https://docs.rs/opentelemetry_sdk/latest/opentelemetry_sdk/struct.Resource.html
    let resource = Resource::builder_empty()
        .with_attribute(KeyValue::new("service.name", "api-service"))
        .with_attribute(KeyValue::new("service.version", "0.1.0"))
        .with_attribute(KeyValue::new("service.namespace", "microservice-simulator"))
        .build();

    // https://docs.rs/opentelemetry_sdk/latest/opentelemetry_sdk/trace/struct.SdkTracerProvider.html
    /*
       SdkTracerProvider - https://docs.rs/opentelemetry_sdk/latest/opentelemetry_sdk/trace/struct.SdkTracerProvider.html#method.builder
       SdkTracerProvider::builder() returns - TracerProviderBuilder  - https://docs.rs/opentelemetry_sdk/latest/opentelemetry_sdk/trace/struct.TracerProviderBuilder.html
       TracerProviderBuilder.with_batch_exporter returns - self
       TracerProviderBuilder.with_resource returns - self
       TracerProviderBuilder.build() returns - SdkTracerProvider
    */
    let tracer_provider = SdkTracerProvider::builder()
        .with_batch_exporter(otlp_exporter)
        .with_resource(resource)
        .build();

    global::set_tracer_provider(tracer_provider);
    Ok(())
}

// Initialize OpenTelemetry logging with direct OTLP export via HTTP client
// fn init_logger() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
fn init_logger() -> SdkLoggerProvider {
    let endpoint_url = std::env::var("OTLP_LOGGING_BACKEND_URL")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());

    // Create a resource with service information
    // https://docs.rs/opentelemetry_sdk/latest/opentelemetry_sdk/struct.Resource.html
    let resource = Resource::builder_empty()
        .with_attribute(KeyValue::new(SERVICE_NAME, "api-service"))
        .with_attribute(KeyValue::new(SERVICE_VERSION, "0.1.0"))
        .with_attribute(KeyValue::new(K8S_NAMESPACE_NAME, "microservice-simulator"))
        .build();

    // https://opentelemetry.io/docs/languages/rust/exporters/
    // https://docs.rs/opentelemetry-otlp/latest/opentelemetry_otlp/
    // https://github.com/open-telemetry/opentelemetry-rust/blob/main/opentelemetry-otlp/src/logs.rs
    // https://docs.rs/crate/tonic/latest
    // https://github.com/open-telemetry/opentelemetry-rust/blob/main/opentelemetry-otlp/examples/basic-otlp/src/main.rs
    // https://github.com/open-telemetry/opentelemetry-rust
    /*
     with_tonic(): Use the tonic gRPC client for exporting data. Requires grpc-tonic feature to be enabled for opentelemetry-otlp..
     with_endpoint(endpoint_url): Specifies the OTLP endpoint URL where the logs will be sent.

     TODO the exporter is what is connecting to the backend/collector.
    */
    let otlp_exporter = opentelemetry_otlp::LogExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint_url)
        .build()
        .expect("Failed to create log exporter");

    // Create stdout exporter
    let stdout_exporter = opentelemetry_stdout::LogExporter::default();

    let log_provider = SdkLoggerProvider::builder()
        .with_batch_exporter(otlp_exporter)
        .with_simple_exporter(stdout_exporter)
        .with_resource(resource)
        .build();

    // Setup Log Appender for the log crate.
    let otel_log_appender = OpenTelemetryLogBridge::new(&log_provider);
    // TODO what does this do?
    log::set_boxed_logger(Box::new(otel_log_appender)).unwrap();
    // TODO what does this do?
    log::set_max_level(Level::Info.to_level_filter());

    // TODO does it need to be made global? probably not since it is being bridged to the log crate.
    // TODO how to send the logs both to OTLP and stdout?

    log_provider
}

// Helper function to emit structured logs with OTLP-compatible format
fn emit_log(level: &str, operation: &str, message: &str, attributes: Vec<KeyValue>) {
    let endpoint = std::env::var("OTLP_LOGGING_BACKEND_URL")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());

    // Create OTLP-compatible structured log entry
    let timestamp = chrono::Utc::now().to_rfc3339();
    let attrs_str = attributes
        .iter()
        .map(|kv| format!("\"{}\":\"{}\"", kv.key, kv.value.as_str()))
        .collect::<Vec<_>>()
        .join(",");

    let log_entry = if attrs_str.is_empty() {
        format!(
            "{{\"timestamp\":\"{}\",\"level\":\"{}\",\"service\":\"api-service\",\"operation\":\"{}\",\"message\":\"{}\",\"otlp_endpoint\":\"{}\"}}",
            timestamp, level, operation, message, endpoint
        )
    } else {
        format!(
            "{{\"timestamp\":\"{}\",\"level\":\"{}\",\"service\":\"api-service\",\"operation\":\"{}\",\"message\":\"{}\",{},\"otlp_endpoint\":\"{}\"}}",
            timestamp, level, operation, message, attrs_str, endpoint
        )
    };

    // Output structured log (can be collected by log agents and sent to OTLP backend)
    println!("{}", log_entry);
    // TODO tech-debt find a good way to send this information.
    info!(target: "my-target-emit-log", "{}", log_entry);
}

/**
 * This
 *   - initializes the otlp grpc connection to the backend
 *   - map the metric api(provider) to the exporter
 *   - stores the metric provider in the global registry so that it can be used by the rest of the application
 */
fn init_metrics() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let endpoint = std::env::var("OTLP_METRICS_BACKEND_URL")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());

    let otlp_exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .with_timeout(std::time::Duration::from_secs(3))
        .build()?;

    let resource = Resource::builder_empty()
        .with_attribute(KeyValue::new("service.name", "api-service"))
        .with_attribute(KeyValue::new("service.version", "0.1.0"))
        .with_attribute(KeyValue::new("service.namespace", "microservice-simulator"))
        .build();

    let matrics_provider = SdkMeterProvider::builder()
        .with_periodic_exporter(otlp_exporter)
        .with_resource(resource)
        .build();

    global::set_meter_provider(matrics_provider);
    Ok(())
}

#[derive(Deserialize)]
struct SleepConfig {
    sleep_ms: u64,
}

#[derive(Serialize)]
struct SleepResponse {
    slept_ms: u64,
    timestamp: String,
}

struct AppState {
    fast_sleep_ms: AtomicU64,
    slow_sleep_ms: AtomicU64,
}

#[derive(Serialize, Deserialize)]
struct HealthResponse {
    status: String,
    timestamp: String,
    service: String,
}


async fn health_handler() -> Json<HealthResponse> {
    // Grabs the global tracer
    let tracer = global::tracer("api-service");
    //and starts a new span for the health check operation
    let mut span = tracer.start("health_check");

    let meter = global::meter("mylibraryname");
    let counter = meter
        .u64_counter("health_check_count")
        .with_description("Number of requests currently being executed")
        .build();
    counter.add(1, &[KeyValue::new("service", "api-service")]);

    let response = HealthResponse {
        status: "healthy".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        service: "api-service".to_string(),
    };

    // Send structured log to OTLP backend
    emit_log(
        "INFO",
        "health_check",
        "Health check completed",
        vec![KeyValue::new("status", response.status.clone())],
    );

    span.set_attribute(KeyValue::new("http.method", "GET"));
    span.set_attribute(KeyValue::new("http.route", "/health"));
    span.set_status(Status::Ok);
    span.end();

    Json(response)
}

async fn fast_get_handler(State(state): State<Arc<AppState>>) -> Json<SleepResponse> {
    let tracer = global::tracer("api-service");
    let mut span = tracer.start("fast_get");
    span.set_attribute(KeyValue::new("http.method", "GET"));
    span.set_attribute(KeyValue::new("http.route", "/fast"));

    let sleep_ms = state.fast_sleep_ms.load(Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(sleep_ms)).await;

    span.set_attribute(KeyValue::new("sleep_ms", sleep_ms as i64));
    span.set_status(opentelemetry::trace::Status::Ok);
    span.end();

    Json(SleepResponse {
        slept_ms: sleep_ms,
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

async fn fast_post_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SleepConfig>,
) -> StatusCode {
    state.fast_sleep_ms.store(body.sleep_ms, Ordering::Relaxed);
    emit_log("INFO", "fast_post", "Updated fast sleep_ms", vec![KeyValue::new("sleep_ms", body.sleep_ms.to_string())]);
    StatusCode::OK
}

async fn slow_get_handler(State(state): State<Arc<AppState>>) -> Json<SleepResponse> {
    let tracer = global::tracer("api-service");
    let mut span = tracer.start("slow_get");
    span.set_attribute(KeyValue::new("http.method", "GET"));
    span.set_attribute(KeyValue::new("http.route", "/slow"));

    let sleep_ms = state.slow_sleep_ms.load(Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(sleep_ms)).await;

    span.set_attribute(KeyValue::new("sleep_ms", sleep_ms as i64));
    span.set_status(opentelemetry::trace::Status::Ok);
    span.end();

    Json(SleepResponse {
        slept_ms: sleep_ms,
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

async fn slow_post_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SleepConfig>,
) -> StatusCode {
    state.slow_sleep_ms.store(body.sleep_ms, Ordering::Relaxed);
    emit_log("INFO", "slow_post", "Updated slow sleep_ms", vec![KeyValue::new("sleep_ms", body.sleep_ms.to_string())]);
    StatusCode::OK
}

fn create_app() -> Router {
    let state = Arc::new(AppState {
        fast_sleep_ms: AtomicU64::new(10),
        slow_sleep_ms: AtomicU64::new(500),
    });

    Router::new()
        .route("/health", get(health_handler))
        .route("/fast", get(fast_get_handler).post(fast_post_handler))
        .route("/slow", get(slow_get_handler).post(slow_post_handler))
        .with_state(state)
}

//            #     #    #      ###   #     #
//            ##   ##   # #      #    ##    #
//            # # # #  #   #     #    # #   #
//            #  #  # #     #    #    #  #  #
//            #     # #######    #    #   # #
//            #     # #     #    #    #    ##
//            #     # #     #   ###   #     #

#[tokio::main]
async fn main() {
    // Initialize OpenTelemetry tracing
    if let Err(err) = init_tracer() {
        eprintln!("Failed to initialize OpenTelemetry tracer: {}", err);
        eprintln!("Continuing without tracing...");
    } else {
        // TODO actually get this information from the tracer, not count on this being the same code as in init_tracer()
        let endpoint = std::env::var("OTLP_TRACE_BACKEND_URL")
            .unwrap_or_else(|_| "http://localhost:4317".to_string());
        println!(
            "OpenTelemetry tracer initialized - sending traces to {}",
            endpoint
        );
    }

    // Initialize OpenTelemetry logging (placeholder for now)
    let log_provider: SdkLoggerProvider = init_logger();

    emit_log(
        "INFO",
        "system_startup",
        "API service starting with OTLP logging enabled",
        vec![]
    );
    info!(target: "my-target", "API service starting with OTLP logging enabled");

    if let Err(err) = init_metrics() {
        eprintln!("Failed to initialize OpenTelemetry metrics: {}", err);
        eprintln!("Continuing without metrics...");
    } else {
        // TODO actually get this information from the metrics provider, not count on this being the same code as in init_metrics()
        let endpoint = std::env::var("OTLP_METRICS_BACKEND_URL")
            .unwrap_or_else(|_| "http://localhost:4317".to_string());
        println!(
            "OpenTelemetry metrics initialized - sending metrics to {}",
            endpoint
        );
    }

    println!("Starting API service with OTLP tracing and structured logging enabled...");

    let app = create_app();

    let listener = TcpListener::bind("0.0.0.0:8080")
        .await
        .expect("Failed to bind to address");

    println!("API server listening on http://0.0.0.0:8080");
    println!("Available endpoints:");
    println!("  GET  /health       - Health check");
    println!("  GET  /fast         - Sleep for fast_sleep_ms (default 10ms)");
    println!("  POST /fast         - Set fast sleep_ms: {{\"sleep_ms\": N}}");
    println!("  GET  /slow         - Sleep for slow_sleep_ms (default 500ms)");
    println!("  POST /slow         - Set slow sleep_ms: {{\"sleep_ms\": N}}");

    axum::serve(listener, app)
        .await
        .expect("Server failed to start");

    println!("Flushing logs...");
    if let Err(e) = log_provider.force_flush() {
        eprintln!("Failed to flush logs: {:?}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    fn make_state(fast_ms: u64, slow_ms: u64) -> Arc<AppState> {
        Arc::new(AppState {
            fast_sleep_ms: AtomicU64::new(fast_ms),
            slow_sleep_ms: AtomicU64::new(slow_ms),
        })
    }

    #[tokio::test]
    async fn test_health_handler() {
        let response = health_handler().await;
        let health = response.0;
        assert_eq!(health.status, "healthy");
        assert_eq!(health.service, "api-service");
        assert!(!health.timestamp.is_empty());
    }

    #[tokio::test]
    async fn test_fast_post_sets_sleep_ms() {
        let state = make_state(10, 500);
        let status = fast_post_handler(State(state.clone()), Json(SleepConfig { sleep_ms: 42 })).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(state.fast_sleep_ms.load(Ordering::Relaxed), 42);
    }

    #[tokio::test]
    async fn test_fast_get_sleeps_and_returns() {
        let state = make_state(0, 500); // 0ms so the test doesn't actually wait
        let response = fast_get_handler(State(state)).await;
        let body = response.0;
        assert_eq!(body.slept_ms, 0);
        assert!(!body.timestamp.is_empty());
    }

    #[tokio::test]
    async fn test_slow_post_sets_sleep_ms() {
        let state = make_state(10, 500);
        let status = slow_post_handler(State(state.clone()), Json(SleepConfig { sleep_ms: 900 })).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(state.slow_sleep_ms.load(Ordering::Relaxed), 900);
    }

    #[tokio::test]
    async fn test_slow_get_sleeps_and_returns() {
        let state = make_state(10, 0); // 0ms so the test doesn't actually wait
        let response = slow_get_handler(State(state)).await;
        let body = response.0;
        assert_eq!(body.slept_ms, 0);
        assert!(!body.timestamp.is_empty());
    }

    #[tokio::test]
    async fn test_fast_and_slow_state_are_independent() {
        let state = make_state(10, 500);
        fast_post_handler(State(state.clone()), Json(SleepConfig { sleep_ms: 77 })).await;
        assert_eq!(state.fast_sleep_ms.load(Ordering::Relaxed), 77);
        assert_eq!(state.slow_sleep_ms.load(Ordering::Relaxed), 500);

        slow_post_handler(State(state.clone()), Json(SleepConfig { sleep_ms: 999 })).await;
        assert_eq!(state.fast_sleep_ms.load(Ordering::Relaxed), 77);
        assert_eq!(state.slow_sleep_ms.load(Ordering::Relaxed), 999);
    }
}
