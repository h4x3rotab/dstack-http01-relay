use hickory_resolver::config::{ResolverConfig, ResolverOpts};
use hickory_resolver::proto::rr::RecordType;
use hickory_resolver::TokioAsyncResolver;
use regex::Regex;
use std::error::Error;
use std::fmt;
use tracing::{debug, info, warn};

#[derive(Debug)]
pub enum DnsError {
    LookupFailed(String),
    NoRecordsFound(String),
    ParseError(String),
}

impl fmt::Display for DnsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DnsError::LookupFailed(msg) => write!(f, "DNS lookup failed: {}", msg),
            DnsError::NoRecordsFound(msg) => write!(f, "No DNS records found: {}", msg),
            DnsError::ParseError(msg) => write!(f, "Failed to parse DNS record: {}", msg),
        }
    }
}

impl Error for DnsError {}

/// DNS resolver for looking up dstack app configuration
pub struct DnsResolver {
    resolver: TokioAsyncResolver,
    fallback_gateway_domain: Option<String>,
    allowed_domain_regex: Option<Regex>,
    gateway_domain_capture_group: usize,
}

impl DnsResolver {
    /// Create a new DNS resolver with default configuration
    pub fn new() -> Result<Self, DnsError> {
        let resolver = TokioAsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default());

        // Read environment variables
        let fallback_gateway_domain = std::env::var("FALLBACK_GATEWAY_DOMAIN").ok();

        // ALLOWED_DOMAIN_REGEX should include a capture group to extract the gateway domain
        // Default: ^_\.(.+\.phala\.network)$ - matches "_.prod5.phala.network" and captures "prod5.phala.network"
        let allowed_domain_regex = std::env::var("ALLOWED_DOMAIN_REGEX")
            .ok()
            .or_else(|| Some(r"^_\.(.+\.phala\.network)$".to_string()))
            .and_then(|pattern| {
                Regex::new(&pattern).ok()
            });

        // Which capture group to use for extracting the gateway domain (default: 1)
        let gateway_domain_capture_group = std::env::var("GATEWAY_DOMAIN_CAPTURE_GROUP")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(1);

        if let Some(ref domain) = fallback_gateway_domain {
            info!("Using fallback gateway domain: {}", domain);
        }

        if let Some(ref regex) = allowed_domain_regex {
            info!("Using allowed domain regex: {} (capture group {} will be used as gateway domain)", regex.as_str(), gateway_domain_capture_group);
        }

        Ok(Self {
            resolver,
            fallback_gateway_domain,
            allowed_domain_regex,
            gateway_domain_capture_group,
        })
    }

    /// Look up the TXT record for _dstack-app-address.{domain}
    /// Returns the app-id and port in format "app-id:port"
    pub async fn lookup_app_address(&self, domain: &str) -> Result<(String, String), DnsError> {
        let txt_domain = format!("_dstack-app-address.{}", domain);

        info!("Looking up TXT record for: {}", txt_domain);

        let response = self.resolver.txt_lookup(&txt_domain)
            .await
            .map_err(|e| DnsError::LookupFailed(format!("TXT lookup failed for {}: {}", txt_domain, e)))?;

        // Get the first TXT record
        let record = response.iter().next()
            .ok_or_else(|| DnsError::NoRecordsFound(format!("No TXT records for {}", txt_domain)))?;

        // Parse the TXT record - it should be in format "app-id:port"
        let txt_value = record.to_string();
        debug!("Found TXT record: {}", txt_value);

        let parts: Vec<&str> = txt_value.split(':').collect();
        if parts.len() != 2 {
            return Err(DnsError::ParseError(format!(
                "Expected 'app-id:port' format, got: {}",
                txt_value
            )));
        }

        Ok((parts[0].to_string(), parts[1].to_string()))
    }

    /// Look up the CNAME record for {domain}
    /// Returns the gateway base domain (e.g., "_.prod5.phala.network" or "prod5.phala.network")
    /// Falls back to FALLBACK_GATEWAY_DOMAIN if CNAME doesn't match ALLOWED_DOMAIN_REGEX
    pub async fn lookup_gateway_domain(&self, domain: &str) -> Result<String, DnsError> {
        info!("Looking up CNAME record for: {}", domain);

        let cname_result = self.resolver.lookup(domain, RecordType::CNAME).await;

        let gateway_domain = match cname_result {
            Ok(response) => {
                // Get the first CNAME record
                let record = response.record_iter().next()
                    .ok_or_else(|| DnsError::NoRecordsFound(format!("No CNAME records for {}", domain)))?;

                let cname_value = record.data()
                    .ok_or_else(|| DnsError::NoRecordsFound(format!("CNAME record has no data for {}", domain)))?
                    .as_cname()
                    .ok_or_else(|| DnsError::ParseError(format!("Expected CNAME record for {}", domain)))?
                    .to_string();

                debug!("Found CNAME record: {}", cname_value);

                // Extract gateway domain (remove trailing dot if present)
                let gateway = cname_value.trim_end_matches('.').to_string();

                // Check if CNAME matches the allowed domain regex and extract gateway domain
                if let Some(ref regex) = self.allowed_domain_regex {
                    if let Some(captures) = regex.captures(&gateway) {
                        // Try to get the specified capture group (the gateway domain)
                        if let Some(captured_gateway) = captures.get(self.gateway_domain_capture_group) {
                            let extracted_domain = captured_gateway.as_str().to_string();
                            info!("CNAME '{}' matches allowed domain regex, extracted gateway from group {}: {}",
                                  gateway, self.gateway_domain_capture_group, extracted_domain);
                            extracted_domain
                        } else {
                            // Capture group doesn't exist, use the whole match
                            warn!("CNAME '{}' matches regex but capture group {} not found, using whole match",
                                  gateway, self.gateway_domain_capture_group);
                            gateway
                        }
                    } else {
                        warn!("CNAME '{}' does not match allowed domain regex", gateway);
                        // Fall back to fallback domain
                        if let Some(ref fallback) = self.fallback_gateway_domain {
                            warn!("Using fallback gateway domain: {}", fallback);
                            fallback.clone()
                        } else {
                            return Err(DnsError::ParseError(format!(
                                "CNAME '{}' does not match allowed domain regex and no fallback configured",
                                gateway
                            )));
                        }
                    }
                } else {
                    // No regex check, use CNAME as-is (strip "_." prefix if present)
                    if gateway.starts_with("_.") {
                        gateway[2..].to_string()
                    } else {
                        gateway
                    }
                }
            }
            Err(e) => {
                warn!("CNAME lookup failed for {}: {}", domain, e);
                // Fall back to fallback domain
                if let Some(ref fallback) = self.fallback_gateway_domain {
                    warn!("Using fallback gateway domain: {}", fallback);
                    fallback.clone()
                } else {
                    return Err(DnsError::LookupFailed(format!("CNAME lookup failed for {}: {}", domain, e)));
                }
            }
        };

        Ok(gateway_domain)
    }

    /// Resolve the complete app URL for a given custom domain
    /// Returns the full https:// URL to redirect to
    pub async fn resolve_app_url(&self, custom_domain: &str, path: &str) -> Result<String, DnsError> {
        info!("Resolving app URL for domain: {} with path: {}", custom_domain, path);

        // Look up both TXT and CNAME records
        let (app_id, _port) = self.lookup_app_address(custom_domain).await?;
        let gateway_domain = self.lookup_gateway_domain(custom_domain).await?;

        // Construct the full URL: https://{app-id}.{gateway-domain}{path}
        let app_url = format!("https://{}.{}{}", app_id, gateway_domain, path);

        info!("Resolved app URL: {}", app_url);
        Ok(app_url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dns_resolver_creation() {
        let resolver = DnsResolver::new();
        assert!(resolver.is_ok());
    }
}
