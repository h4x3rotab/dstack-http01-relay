# dstack HTTP-01 Challenge Relay

This project implements an HTTP-01 ACME challenge solver for dstack, enabling custom domain certificates.

## Problem

dstack exposes Docker HTTP ports via HTTPS automatically using auto-allocated subdomains through dstack-gateway, but doesn't expose port 80. This breaks the standard ACME HTTP-01 challenge flow for custom domain certificates.

## Solution

A two-component system:

1. **Relay Server** - Standalone Rust service that listens on port 80, performs DNS lookups, and redirects ACME challenges to dstack HTTPS endpoints
2. **ACME Client** - Docker container running inside dstack that serves ACME challenges over HTTPS

## Architecture

```
Let's Encrypt
    ↓ (HTTP-01 challenge request)
Relay Server (port 80)
    ↓ (DNS lookup + redirect)
ACME Client in dstack (HTTPS)
    ↓ (challenge response)
Let's Encrypt validates → Certificate issued
```

### DNS Setup Required

For each custom domain:
```
A {custom-domain}                           {relay-server-ip}
TXT _dstack-app-address.{custom-domain}     {app-id}:80
CNAME {custom-domain}                       _.{gateway-base-domain}
```

## Components

### Relay Server
- **Location:** `relay-server/`
- **Docker Image:** `h4x3rotab/dstack-http01-relay-server:latest`
- **Documentation:** [relay-server/README.md](relay-server/README.md)

### ACME Client
- **Location:** `acme-client/`
- **Documentation:** [acme-client/README.md](acme-client/README.md)

## Quick Start

1. **Deploy relay server** (see [relay-server/README.md](relay-server/README.md)):
   ```bash
   docker pull h4x3rotab/dstack-http01-relay-server:latest
   docker run -d -p 8081:8081 \
     -e FALLBACK_GATEWAY_DOMAIN=prod5.phala.network \
     h4x3rotab/dstack-http01-relay-server:latest
   ```

2. **Configure DNS** for your custom domain (A, TXT, CNAME records)

3. **Deploy ACME client** to dstack (see [acme-client/README.md](acme-client/README.md))

4. **Request certificate** using certbot from within the ACME client

## Resources

- **dstack Gateway:** Use deepwiki (Dstack-TEE/dstack)
- **dstack Basics:** Use PhalaDocs
