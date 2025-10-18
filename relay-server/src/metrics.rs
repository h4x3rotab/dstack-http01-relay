use prometheus::{
    register_int_counter_vec, register_histogram_vec, IntCounterVec, HistogramVec, Encoder, TextEncoder,
};
use std::sync::OnceLock;

static REQUESTS_TOTAL: OnceLock<IntCounterVec> = OnceLock::new();
static REQUEST_DURATION: OnceLock<HistogramVec> = OnceLock::new();
static DNS_LOOKUPS_TOTAL: OnceLock<IntCounterVec> = OnceLock::new();
static REDIRECTS_TOTAL: OnceLock<IntCounterVec> = OnceLock::new();

/// Initialize Prometheus metrics
pub fn init_metrics() {
    REQUESTS_TOTAL.get_or_init(|| {
        register_int_counter_vec!(
            "http_requests_total",
            "Total number of HTTP requests",
            &["method", "path", "status"]
        )
        .unwrap()
    });

    REQUEST_DURATION.get_or_init(|| {
        register_histogram_vec!(
            "http_request_duration_seconds",
            "HTTP request duration in seconds",
            &["method", "path"]
        )
        .unwrap()
    });

    DNS_LOOKUPS_TOTAL.get_or_init(|| {
        register_int_counter_vec!(
            "dns_lookups_total",
            "Total number of DNS lookups",
            &["type", "status"]
        )
        .unwrap()
    });

    REDIRECTS_TOTAL.get_or_init(|| {
        register_int_counter_vec!(
            "redirects_total",
            "Total number of redirects",
            &["status"]
        )
        .unwrap()
    });
}

/// Increment HTTP request counter
pub fn inc_requests(method: &str, path: &str, status: u16) {
    if let Some(counter) = REQUESTS_TOTAL.get() {
        counter
            .with_label_values(&[method, path, &status.to_string()])
            .inc();
    }
}

/// Observe HTTP request duration
pub fn observe_request_duration(method: &str, path: &str, duration: f64) {
    if let Some(histogram) = REQUEST_DURATION.get() {
        histogram
            .with_label_values(&[method, path])
            .observe(duration);
    }
}

/// Increment DNS lookup counter
pub fn inc_dns_lookups(lookup_type: &str, status: &str) {
    if let Some(counter) = DNS_LOOKUPS_TOTAL.get() {
        counter.with_label_values(&[lookup_type, status]).inc();
    }
}

/// Increment redirect counter
pub fn inc_redirects(status: &str) {
    if let Some(counter) = REDIRECTS_TOTAL.get() {
        counter.with_label_values(&[status]).inc();
    }
}

/// Gather and encode all metrics for Prometheus scraping
pub fn gather_metrics() -> Vec<u8> {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();
    buffer
}
