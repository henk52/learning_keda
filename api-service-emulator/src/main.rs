use axum::{
    Router,
    extract::{Path, Query},
    http::StatusCode,
    response::Json,
    routing::{get, post},
};
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

#[derive(Serialize, Deserialize)]
struct HealthResponse {
    status: String,
    timestamp: String,
    service: String,
}

#[derive(Serialize, Deserialize)]
struct SessionRequest {
    username: String,
    password: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct SessionResponse {
    session_id: String,
    expires_at: String,
    username: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct User {
    id: u32,
    username: String,
    email: String,
    created_at: String,
}

#[derive(Serialize, Deserialize)]
struct UserCreateRequest {
    username: String,
    email: String,
    password: String,
}

#[derive(Deserialize)]
struct UserQuery {
    limit: Option<u32>,
    offset: Option<u32>,
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

async fn create_session(
    Json(payload): Json<SessionRequest>,
) -> Result<Json<SessionResponse>, StatusCode> {
    let tracer = global::tracer("api-service");
    let mut span = tracer.start("create_session");

    span.set_attribute(KeyValue::new("http.method", "POST"));
    span.set_attribute(KeyValue::new("http.route", "/session"));
    span.set_attribute(KeyValue::new("user.name", payload.username.clone()));

    // Simple authentication check (in real app, validate against database)
    if payload.username.is_empty() || payload.password.is_empty() {
        emit_log(
            "ERROR",
            "create_session",
            "Session creation failed - empty username or password",
            vec![KeyValue::new("username", payload.username.clone())],
        );

        span.set_status(Status::error("Empty username or password"));
        span.set_attribute(KeyValue::new("error", "empty_credentials"));
        span.end();
        return Err(StatusCode::BAD_REQUEST);
    }

    // Mock session creation
    let session_response = SessionResponse {
        session_id: uuid::Uuid::new_v4().to_string(),
        expires_at: (chrono::Utc::now() + chrono::Duration::hours(24)).to_rfc3339(),
        username: payload.username.clone(),
    };

    emit_log(
        "INFO",
        "create_session",
        "Session created successfully",
        vec![
            KeyValue::new("username", payload.username),
            KeyValue::new("session_id", session_response.session_id.clone()),
        ],
    );

    span.set_attribute(KeyValue::new(
        "session.id",
        session_response.session_id.clone(),
    ));
    span.set_status(Status::Ok);
    span.end();

    Ok(Json(session_response))
}

async fn get_session(Path(session_id): Path<String>) -> Result<Json<SessionResponse>, StatusCode> {
    let tracer = global::tracer("api-service");
    let mut span = tracer.start("get_session");

    span.set_attribute(KeyValue::new("http.method", "GET"));
    span.set_attribute(KeyValue::new("http.route", "/session/{id}"));
    span.set_attribute(KeyValue::new("session.id", session_id.clone()));

    if session_id.is_empty() {
        span.set_status(Status::error("Empty session ID"));
        span.set_attribute(KeyValue::new("error", "empty_session_id"));
        span.end();
        return Err(StatusCode::BAD_REQUEST);
    }

    // Mock session retrieval (in real app, get from database/cache)
    let session_response = SessionResponse {
        session_id,
        expires_at: (chrono::Utc::now() + chrono::Duration::hours(12)).to_rfc3339(),
        username: "mock_user".to_string(),
    };

    span.set_status(Status::Ok);
    span.end();

    Ok(Json(session_response))
}

async fn get_users(Query(params): Query<UserQuery>) -> Json<Vec<User>> {
    let tracer = global::tracer("api-service");
    let mut span = tracer.start("get_users");

    let limit = params.limit.unwrap_or(10);
    let offset = params.offset.unwrap_or(0);

    span.set_attribute(KeyValue::new("http.method", "GET"));
    span.set_attribute(KeyValue::new("http.route", "/user"));
    span.set_attribute(KeyValue::new("query.limit", limit as i64));
    span.set_attribute(KeyValue::new("query.offset", offset as i64));

    // Mock users data
    let users: Vec<User> = vec![
        User {
            id: 1 + offset,
            username: "alice".to_string(),
            email: "alice@example.com".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        },
        User {
            id: 2 + offset,
            username: "bob".to_string(),
            email: "bob@example.com".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        },
    ]
    .into_iter()
    .take(limit as usize)
    .collect();

    span.set_attribute(KeyValue::new("users.count", users.len() as i64));
    span.set_status(Status::Ok);
    span.end();

    Json(users)
}

async fn get_user(Path(user_id): Path<u32>) -> Result<Json<User>, StatusCode> {
    let tracer = global::tracer("api-service");
    let mut span = tracer.start("get_user");

    span.set_attribute(KeyValue::new("http.method", "GET"));
    span.set_attribute(KeyValue::new("http.route", "/user/{id}"));
    span.set_attribute(KeyValue::new("user.id", user_id as i64));

    if user_id == 0 {
        span.set_status(Status::error("Invalid user ID: 0"));
        span.set_attribute(KeyValue::new("error", "invalid_user_id"));
        span.end();
        return Err(StatusCode::BAD_REQUEST);
    }

    // Mock user retrieval
    let user = User {
        id: user_id,
        username: format!("user_{}", user_id),
        email: format!("user_{}@example.com", user_id),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    span.set_attribute(KeyValue::new("user.name", user.username.clone()));
    span.set_status(Status::Ok);
    span.end();

    Ok(Json(user))
}

async fn create_user(Json(payload): Json<UserCreateRequest>) -> Result<Json<User>, StatusCode> {
    let tracer = global::tracer("api-service");
    let mut span = tracer.start("create_user");

    span.set_attribute(KeyValue::new("http.method", "POST"));
    span.set_attribute(KeyValue::new("http.route", "/user"));
    span.set_attribute(KeyValue::new("user.name", payload.username.clone()));
    span.set_attribute(KeyValue::new("user.email", payload.email.clone()));

    if payload.username.is_empty() || payload.email.is_empty() {
        emit_log(
            "ERROR",
            "create_user",
            "User creation failed - empty username or email",
            vec![
                KeyValue::new("username", payload.username.clone()),
                KeyValue::new("email", payload.email.clone()),
            ],
        );

        span.set_status(Status::error("Empty username or email"));
        span.set_attribute(KeyValue::new("error", "empty_fields"));
        span.end();
        return Err(StatusCode::BAD_REQUEST);
    }

    // Mock user creation
    let user = User {
        id: 999, // Mock ID
        username: payload.username.clone(),
        email: payload.email.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    emit_log(
        "INFO",
        "create_user",
        "User created successfully",
        vec![
            KeyValue::new("user_id", user.id.to_string()),
            KeyValue::new("username", payload.username),
            KeyValue::new("email", payload.email),
        ],
    );

    span.set_attribute(KeyValue::new("user.id", user.id as i64));
    span.set_status(Status::Ok);
    span.end();

    Ok(Json(user))
}

fn create_app() -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/session", post(create_session))
        .route("/session/{id}", get(get_session))
        .route("/user", get(get_users).post(create_user))
        .route("/user/{id}", get(get_user))
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
    println!("  GET  /health - Health check");
    println!("  POST /session - Create session");
    println!("  GET  /session/{{id}} - Get session");
    println!("  GET  /user - List users (with optional ?limit=N&offset=N)");
    println!("  POST /user - Create user");
    println!("  GET  /user/{{id}} - Get user by ID");

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

    #[tokio::test]
    async fn test_health_handler() {
        let response = health_handler().await;
        let health = response.0;
        assert_eq!(health.status, "healthy");
        assert_eq!(health.service, "api-service");
        assert!(!health.timestamp.is_empty());
    }

    #[tokio::test]
    async fn test_create_session_success() {
        let session_request = SessionRequest {
            username: "testuser".to_string(),
            password: "testpass".to_string(),
        };

        let result = create_session(Json(session_request)).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        let session = response.0;
        assert_eq!(session.username, "testuser");
        assert!(!session.session_id.is_empty());
        assert!(!session.expires_at.is_empty());
    }

    #[tokio::test]
    async fn test_create_session_empty_username() {
        let session_request = SessionRequest {
            username: "".to_string(),
            password: "testpass".to_string(),
        };

        let result = create_session(Json(session_request)).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_create_session_empty_password() {
        let session_request = SessionRequest {
            username: "testuser".to_string(),
            password: "".to_string(),
        };

        let result = create_session(Json(session_request)).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_get_session() {
        let session_id = "test-session-123".to_string();
        let result = get_session(Path(session_id.clone())).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        let session = response.0;
        assert_eq!(session.session_id, session_id);
        assert_eq!(session.username, "mock_user");
        assert!(!session.expires_at.is_empty());
    }

    #[tokio::test]
    async fn test_get_session_empty_id() {
        let result = get_session(Path("".to_string())).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_get_users_default() {
        let query = UserQuery {
            limit: None,
            offset: None,
        };

        let response = get_users(Query(query)).await;
        let users = response.0;
        assert_eq!(users.len(), 2);
        assert_eq!(users[0].username, "alice");
        assert_eq!(users[1].username, "bob");
    }

    #[tokio::test]
    async fn test_get_users_with_limit() {
        let query = UserQuery {
            limit: Some(1),
            offset: None,
        };

        let response = get_users(Query(query)).await;
        let users = response.0;
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].username, "alice");
    }

    #[tokio::test]
    async fn test_get_users_with_offset() {
        let query = UserQuery {
            limit: None,
            offset: Some(5),
        };

        let response = get_users(Query(query)).await;
        let users = response.0;
        assert_eq!(users.len(), 2);
        assert_eq!(users[0].id, 6); // 1 + offset 5
        assert_eq!(users[1].id, 7); // 2 + offset 5
    }

    #[tokio::test]
    async fn test_get_user_by_id() {
        let user_id = 123;
        let result = get_user(Path(user_id)).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        let user = response.0;
        assert_eq!(user.id, user_id);
        assert_eq!(user.username, format!("user_{}", user_id));
        assert_eq!(user.email, format!("user_{}@example.com", user_id));
    }

    #[tokio::test]
    async fn test_get_user_by_zero_id() {
        let result = get_user(Path(0)).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_create_user_success() {
        let user_request = UserCreateRequest {
            username: "newuser".to_string(),
            email: "newuser@example.com".to_string(),
            password: "password123".to_string(),
        };

        let result = create_user(Json(user_request)).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        let user = response.0;
        assert_eq!(user.username, "newuser");
        assert_eq!(user.email, "newuser@example.com");
        assert_eq!(user.id, 999);
        assert!(!user.created_at.is_empty());
    }

    #[tokio::test]
    async fn test_create_user_empty_username() {
        let user_request = UserCreateRequest {
            username: "".to_string(),
            email: "test@example.com".to_string(),
            password: "password123".to_string(),
        };

        let result = create_user(Json(user_request)).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_create_user_empty_email() {
        let user_request = UserCreateRequest {
            username: "testuser".to_string(),
            email: "".to_string(),
            password: "password123".to_string(),
        };

        let result = create_user(Json(user_request)).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::BAD_REQUEST);
    }
}
