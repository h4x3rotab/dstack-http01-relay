mod dns;
mod metrics;

use axum::{
    body::Body,
    extract::{Host, Path, Request, State},
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{any, get},
    Router,
};
use futures_util::StreamExt;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tower_http::trace::TraceLayer;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use dns::DnsResolver;

/// Relay mode configuration
#[derive(Clone, Debug, PartialEq)]
enum RelayMode {
    /// Return 307 redirect to the target URL (default)
    Redirect,
    /// Proxy/tunnel traffic to the target URL
    Proxy,
}

impl RelayMode {
    fn from_env() -> Self {
        match std::env::var("RELAY_MODE").as_deref() {
            Ok("proxy") => RelayMode::Proxy,
            Ok("redirect") => RelayMode::Redirect,
            _ => RelayMode::Redirect, // Default
        }
    }
}

/// Shared application state
#[derive(Clone)]
struct AppState {
    dns_resolver: Arc<DnsResolver>,
    http_client: reqwest::Client,
    relay_mode: RelayMode,
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

    // Determine relay mode
    let relay_mode = RelayMode::from_env();
    info!("Relay mode: {:?}", relay_mode);

    // Create HTTP client with connection pooling for proxy mode
    // This client is optimized for high traffic:
    // - Connection pooling enabled by default
    // - Timeouts configured to prevent hanging connections
    // - TLS configured with rustls for better performance
    let http_client = reqwest::Client::builder()
        .pool_max_idle_per_host(100) // Keep up to 100 idle connections per host
        .pool_idle_timeout(Duration::from_secs(90)) // Keep idle connections for 90 seconds
        .connect_timeout(Duration::from_secs(10)) // Connection timeout
        .timeout(Duration::from_secs(30)) // Overall request timeout
        .build()
        .expect("Failed to create HTTP client");

    info!("HTTP client initialized with connection pooling");

    // Create application state
    let state = AppState {
        dns_resolver,
        http_client,
        relay_mode,
    };

    // Build the application router
    let app = Router::new()
        .route("/.well-known/acme-challenge/:token", any(acme_challenge_handler))
        .route("/metrics", any(metrics_handler))
        .route("/health", any(health_handler))
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
    req: Request,
) -> Response {
    let start = Instant::now();
    let path = format!("/.well-known/acme-challenge/{}", token);

    // Extract method, headers, and body from request
    let (parts, body) = req.into_parts();
    let method = parts.method;
    let headers = parts.headers;

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

    // Observe request duration for DNS resolution
    let duration = start.elapsed().as_secs_f64();
    metrics::observe_request_duration("GET", "/.well-known/acme-challenge/*", duration);

    // Handle based on relay mode
    match state.relay_mode {
        RelayMode::Redirect => {
            info!("Redirecting to: {}", app_url);
            metrics::inc_redirects("success");

            // Return a 307 Temporary Redirect to the app URL
            Redirect::temporary(&app_url).into_response()
        }
        RelayMode::Proxy => {
            info!("Proxying request to: {}", app_url);

            // Proxy the request to the target URL, preserving the original request (including Host header)
            match proxy_request(&state.http_client, &app_url, &method, &headers, body).await {
                Ok(response) => {
                    info!("Successfully proxied request to: {}", app_url);
                    metrics::inc_redirects("success");
                    response
                }
                Err(e) => {
                    error!("Failed to proxy request to {}: {}", app_url, e);
                    metrics::inc_redirects("failure");

                    let error_message = format!("Failed to proxy request: {}", e);
                    (StatusCode::BAD_GATEWAY, error_message).into_response()
                }
            }
        }
    }
}

/// Proxy an HTTP request to the target URL
/// This function handles the proxying with connection pooling and streaming
async fn proxy_request(
    client: &reqwest::Client,
    target_url: &str,
    method: &Method,
    original_headers: &HeaderMap,
    body: Body,
) -> Result<Response, String> {
    // Convert method
    let req_method = match method.as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        "HEAD" => reqwest::Method::HEAD,
        "OPTIONS" => reqwest::Method::OPTIONS,
        "PATCH" => reqwest::Method::PATCH,
        _ => reqwest::Method::GET,
    };

    // Convert axum body to a stream and wrap for reqwest
    // This avoids buffering the entire body in memory
    let body_stream = body.into_data_stream().map(|result| {
        result.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    });
    let reqwest_body = reqwest::Body::wrap_stream(body_stream);

    // Build request with method and streaming body
    let mut request_builder = client
        .request(req_method, target_url)
        .body(reqwest_body);

    // Forward all headers, including Host, except hop-by-hop headers
    for (key, value) in original_headers.iter() {
        let key_str = key.as_str().to_lowercase();
        // Skip hop-by-hop headers (but keep host and preserve upgrade/connection for upgrade handling)
        if key_str != "transfer-encoding"
            && key_str != "content-length"  // Let reqwest handle content-length
            && key_str != "te"
            && key_str != "trailer"
            && key_str != "proxy-connection"
            && key_str != "keep-alive" {
            if let Ok(val) = value.to_str() {
                request_builder = request_builder.header(key.as_str(), val);
            }
        }
    }

    // Send the request
    let response = request_builder
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    // Extract status code
    let status = response.status();

    // Extract headers to forward (filtering out connection-specific headers)
    let mut headers = HeaderMap::new();
    for (key, value) in response.headers() {
        let key_str = key.as_str().to_lowercase();
        // Skip connection-specific headers
        if key_str != "connection"
            && key_str != "transfer-encoding"
            && key_str != "content-encoding"
            && key_str != "content-length" {
            if let Ok(val) = value.to_str() {
                if let Ok(header_value) = val.parse() {
                    headers.insert(key.clone(), header_value);
                }
            }
        }
    }

    // Convert the response body to a stream
    // This is important for handling large responses efficiently
    let body_stream = response.bytes_stream();
    let body = Body::from_stream(body_stream);

    // Construct the response
    let mut resp = Response::new(body);
    *resp.status_mut() = status;
    *resp.headers_mut() = headers;

    Ok(resp)
}

/// Check if a request is an upgrade request (WebSocket, HTTP/2, etc.)
fn is_upgrade_request(headers: &HeaderMap) -> bool {
    headers.get("connection")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_lowercase().contains("upgrade"))
        .unwrap_or(false)
}

/// Helper function to relay a request to the backend
async fn relay_to_backend(
    state: &AppState,
    hostname: &str,
    path: &str,
    method: &Method,
    headers: &HeaderMap,
    body: Body,
) -> Response {
    // Check if this is an upgrade request
    if is_upgrade_request(headers) {
        warn!("Upgrade request detected for {} (WebSocket, HTTP/2, etc.)", hostname);

        // For upgrade requests in proxy mode, we currently don't support them
        // because reqwest doesn't handle protocol upgrades
        if state.relay_mode == RelayMode::Proxy {
            warn!("Protocol upgrades are not fully supported in proxy mode yet. Consider using redirect mode (RELAY_MODE=redirect) for WebSocket and other upgrade requests.");
            return (
                StatusCode::NOT_IMPLEMENTED,
                "Protocol upgrades (WebSocket, HTTP/2) are not supported in proxy mode. Please use redirect mode (set RELAY_MODE=redirect) for upgrade requests."
            ).into_response();
        }
    }

    // Resolve the app URL using DNS
    let app_url = match state.dns_resolver.resolve_app_url(hostname, path).await {
        Ok(url) => {
            info!("Successfully resolved app URL: {}", url);
            url
        }
        Err(e) => {
            error!("Failed to resolve app URL for {}: {}", hostname, e);
            let error_message = format!("Failed to resolve DNS records for {}: {}", hostname, e);
            return (StatusCode::BAD_GATEWAY, error_message).into_response();
        }
    };

    // Handle based on relay mode
    match state.relay_mode {
        RelayMode::Redirect => {
            info!("Redirecting to: {}", app_url);
            Redirect::temporary(&app_url).into_response()
        }
        RelayMode::Proxy => {
            info!("Proxying request to: {}", app_url);

            // Proxy the request to the target URL, preserving the original request (including Host header)
            match proxy_request(&state.http_client, &app_url, method, headers, body).await {
                Ok(response) => {
                    info!("Successfully proxied request to: {}", app_url);
                    response
                }
                Err(e) => {
                    error!("Failed to proxy request to {}: {}", app_url, e);
                    let error_message = format!("Failed to proxy request: {}", e);
                    (StatusCode::BAD_GATEWAY, error_message).into_response()
                }
            }
        }
    }
}

/// Metrics endpoint for Prometheus scraping
/// Serves relay server metrics if Host is not a dstack domain, otherwise relays to backend
async fn metrics_handler(
    State(state): State<AppState>,
    req: Request,
) -> Response {
    let (parts, body) = req.into_parts();

    // Extract Host header
    let hostname = parts.headers.get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    // Check if this is a dstack custom domain
    if state.dns_resolver.is_dstack_custom_domain(&hostname).await {
        info!("Metrics endpoint accessed with dstack custom domain: {}, relaying to backend", hostname);

        // Relay to the backend with full request
        return relay_to_backend(&state, &hostname, "/metrics", &parts.method, &parts.headers, body).await;
    }

    info!("Metrics endpoint accessed with non-dstack domain: {}, serving relay server metrics", hostname);
    let metrics = metrics::gather_metrics();
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4")],
        metrics,
    )
        .into_response()
}

/// Health check endpoint
/// Serves relay server health if Host is not a dstack domain, otherwise relays to backend
async fn health_handler(
    State(state): State<AppState>,
    req: Request,
) -> Response {
    let (parts, body) = req.into_parts();

    // Extract Host header
    let hostname = parts.headers.get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    // Check if this is a dstack custom domain
    if state.dns_resolver.is_dstack_custom_domain(&hostname).await {
        info!("Health endpoint accessed with dstack custom domain: {}, relaying to backend", hostname);

        // Relay to the backend with full request
        return relay_to_backend(&state, &hostname, "/health", &parts.method, &parts.headers, body).await;
    }

    info!("Health endpoint accessed with non-dstack domain: {}, serving relay server health", hostname);
    (StatusCode::OK, "OK").into_response()
}

/// Root handler for informational purposes
async fn root_handler(State(state): State<AppState>) -> Response {
    let mode_description = match state.relay_mode {
        RelayMode::Redirect => "307 redirect (default)",
        RelayMode::Proxy => "HTTP proxy/tunnel",
    };

    let info = format!(
        r#"
dstack HTTP-01 ACME Challenge Relay Server

This server relays ACME HTTP-01 challenges to dstack applications.

Current Mode: {}

Endpoints:
- /.well-known/acme-challenge/:token - ACME challenge endpoint
- /metrics - Prometheus metrics
- /health - Health check

How it works:
1. Let's Encrypt requests http://{{custom-domain}}/.well-known/acme-challenge/{{token}}
2. This server looks up DNS records:
   - TXT _dstack-app-address.{{custom-domain}} -> {{app-id}}:port
   - CNAME {{custom-domain}} -> _.{{gateway-base-domain}}
3. In redirect mode: Returns 307 redirect to https://{{app-id}}.{{gateway-base-domain}}/.well-known/acme-challenge/{{token}}
   In proxy mode: Proxies the request directly to the target HTTPS endpoint
4. The ACME client in dstack responds with the challenge

Proxy Mode Features:
- Connection pooling (up to 100 idle connections per host)
- Request streaming for efficient memory usage
- Configured timeouts for reliability
- Optimized for high traffic scenarios

Status: Running
"#,
        mode_description
    );

    (StatusCode::OK, info).into_response()
}
