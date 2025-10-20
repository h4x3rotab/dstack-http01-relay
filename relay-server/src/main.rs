mod dns;
mod metrics;

use axum::{
    extract::{Host, Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Router,
};
use std::sync::Arc;
use std::time::Instant;
use tower_http::trace::TraceLayer;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use dns::DnsResolver;

/// Shared application state
#[derive(Clone)]
struct AppState {
    dns_resolver: Arc<DnsResolver>,
}

#[tokio::main]
async fn main() {
    // Load .env file if present (optional, won't fail if missing)
    let _ = dotenvy::dotenv();

    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "relay_server=info,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Initialize metrics
    metrics::init_metrics();
    info!("Metrics initialized");

    // Create DNS resolver
    let dns_resolver = match DnsResolver::new() {
        Ok(resolver) => Arc::new(resolver),
        Err(e) => {
            error!("Failed to create DNS resolver: {}", e);
            std::process::exit(1);
        }
    };

    info!("DNS resolver initialized");

    // Create application state
    let state = AppState { dns_resolver };

    // Build the application router
    let app = Router::new()
        .route("/.well-known/acme-challenge/:token", get(acme_challenge_handler))
        .route("/metrics", get(metrics_handler))
        .route("/health", get(health_handler))
        .route("/", get(root_handler))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Get port from environment variable or use default 8081
    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8081);

    let bind_addr = format!("0.0.0.0:{}", port);

    // Bind to the configured port
    let listener = match tokio::net::TcpListener::bind(&bind_addr).await {
        Ok(listener) => listener,
        Err(e) => {
            error!("Failed to bind to {}: {}", bind_addr, e);
            if port < 1024 {
                error!("Port {} requires root/sudo permissions", port);
            }
            std::process::exit(1);
        }
    };

    info!("Relay server listening on http://{}", bind_addr);
    info!("Metrics endpoint: http://{}/metrics", bind_addr);
    info!("Health endpoint: http://{}/health", bind_addr);

    // Start the server
    if let Err(e) = axum::serve(listener, app).await {
        error!("Server error: {}", e);
        std::process::exit(1);
    }
}

/// Handle ACME challenge requests
/// This is the core function that implements the HTTP-01 challenge relay
async fn acme_challenge_handler(
    Host(hostname): Host,
    Path(token): Path<String>,
    State(state): State<AppState>,
) -> Response {
    let start = Instant::now();
    let path = format!("/.well-known/acme-challenge/{}", token);

    info!(
        "Received ACME challenge request for domain: {} token: {}",
        hostname, token
    );

    // Increment metrics
    metrics::inc_requests("GET", "/.well-known/acme-challenge/*", 200);

    // Resolve the app URL using DNS
    let app_url = match state.dns_resolver.resolve_app_url(&hostname, &path).await {
        Ok(url) => {
            info!("Successfully resolved app URL: {}", url);
            metrics::inc_dns_lookups("combined", "success");
            metrics::inc_redirects("success");
            url
        }
        Err(e) => {
            error!("Failed to resolve app URL for {}: {}", hostname, e);
            metrics::inc_dns_lookups("combined", "failure");
            metrics::inc_redirects("failure");

            let error_message = format!("Failed to resolve DNS records for {}: {}", hostname, e);
            return (StatusCode::BAD_GATEWAY, error_message).into_response();
        }
    };

    // Observe request duration
    let duration = start.elapsed().as_secs_f64();
    metrics::observe_request_duration("GET", "/.well-known/acme-challenge/*", duration);

    info!("Redirecting to: {}", app_url);

    // Return a 307 Temporary Redirect to the app URL
    Redirect::temporary(&app_url).into_response()
}

/// Metrics endpoint for Prometheus scraping
async fn metrics_handler() -> Response {
    let metrics = metrics::gather_metrics();
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4")],
        metrics,
    )
        .into_response()
}

/// Health check endpoint
async fn health_handler() -> &'static str {
    "OK"
}

/// Root handler for informational purposes
async fn root_handler() -> Response {
    let info = r#"
dstack HTTP-01 ACME Challenge Relay Server

This server relays ACME HTTP-01 challenges to dstack applications.

Endpoints:
- /.well-known/acme-challenge/:token - ACME challenge endpoint
- /metrics - Prometheus metrics
- /health - Health check

How it works:
1. Let's Encrypt requests http://{custom-domain}/.well-known/acme-challenge/{token}
2. This server looks up DNS records:
   - TXT _dstack-app-address.{custom-domain} -> {app-id}:port
   - CNAME {custom-domain} -> _.{gateway-base-domain}
3. Redirects to https://{app-id}.{gateway-base-domain}/.well-known/acme-challenge/{token}
4. The ACME client in dstack responds with the challenge

Status: Running
"#;

    (StatusCode::OK, info).into_response()
}
