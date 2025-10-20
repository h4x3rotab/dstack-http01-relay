# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This project implements an HTTP-01 ACME challenge solver for dstack. The core problem being solved: dstack exposes Docker HTTP ports via HTTPS automatically using auto-allocated subdomains through dstack-gateway, but doesn't expose port 80, which breaks the standard ACME HTTP-01 challenge flow for custom domain certificates.

## Docker Images

- **Relay Server**: `h4x3rotab/dstack-http01-relay-server:latest`
- **ACME Client**: Build from `acme-client/Dockerfile`

## Solution Architecture

The solution consists of:

1. **Relay Server** (Rust): A standalone web service listening on port 8081 (default) that intercepts HTTP-01 challenge requests
2. **ACME Client**: Runs inside dstack (Docker environment in confidential VM), listening on port 80, exposed as `https://{app-id}.{gateway-base-domain}`

### DNS Configuration Required

For each custom domain:
- `TXT _dstack-app-address.{custom-domain}`: `{app_id}:port`
- `CNAME {custom-domain}`: `_.{gateway-base-domain}`

### HTTP-01 Challenge Protocol Flow

1. ACME client generates account secret `s`
2. ACME client requests token `t` from Let's Encrypt
3. ACME client serves token at `https://{app-id}.{domain}/.well-known/acme-challenge/{t}` with `hash(s)`
4. Let's Encrypt requests `http://{custom-domain}/.well-known/acme-challenge/{t}`
5. Relay server:
   - Looks up `TXT _dstack-app-address.{custom-domain}` to get `{app-id}`
   - Looks up `CNAME {custom-domain}` to get `_.{gateway-base-domain}`
6. Relay server redirects to `https://{app-id}.{domain}/.well-known/acme-challenge/{t}`
7. Let's Encrypt validates and authenticates `s`
8. ACME client requests certificate using `s`

## Key Implementation Details

### Relay Server Configuration

Environment variables (see `relay-server/.env.example`):
- `PORT`: Listen port (default: 8081)
- `FALLBACK_GATEWAY_DOMAIN`: Fallback gateway when CNAME doesn't match regex (default: `prod5.phala.network`)
- `ALLOWED_DOMAIN_REGEX`: Regex to match and extract gateway from CNAME (default: `^_\.(.+\.phala\.network)$`)
- `GATEWAY_DOMAIN_CAPTURE_GROUP`: Which capture group to extract (default: 1)
- `RUST_LOG`: Logging level

### Relay Server Commands

```bash
# Local development
cd relay-server
cargo run

# Docker
docker pull h4x3rotab/dstack-http01-relay-server:latest
docker run -p 8081:8081 -e FALLBACK_GATEWAY_DOMAIN=prod5.phala.network h4x3rotab/dstack-http01-relay-server:latest
```

### ACME Client

```bash
# Build
cd acme-client
docker build -t acme-client .

# Run with docker-compose
docker-compose up -d
```

## Test Environment

- custom-domain: `http01-test.phala.systems`
- app-id: `5b912f5cec6c02d851db76bc20f410700f01fc65`
- gateway-base-domain: `prod5.phala.network`
- DNS TXT: `_dstack-app-address.http01-test.phala.systems` → `5b912f5cec6c02d851db76bc20f410700f01fc65:443`
- DNS CNAME: `http01-test.phala.systems` → `tdx-lab-pub.phala.systems`

## Additional Context

- Learn about dstack-gateway details: Use deepwiki (Dstack-TEE/dstack repository)
- Learn dstack basics: Use PhalaDocs
