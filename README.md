# dstack http01 challenge demo

Goal is to create a http01 challenge solver for dstack. dstack expose docker http ports to https automatically via dstack-gateway using auto allocated subdomains. However, if the client wants to generate a custom cert for their custom domain, the most commonly used acme challenge http-01 doesn't work, because we don't expose port 80.

To solve this problem, we can create a standalone rust web service that listen to 80 port. The dev needs to set up:
- `TXT _dstack-app-address.{custom-domain}`: `{app_id}:port`
- `CNAME {custom-domain}`: `_.{gateway-base-domain}`
- an acme client listenting to port 80 in dstack, exposed as `https://{app-id}.{gateway-base-domain}`

Then we can follow the protocol as below to allow http-01 challenge:

## Protocol

1. acme client generates a secret s for its account
2. acme client ask let's encrypt to generate a token t
3. acme client serves the token under `https://{app-id}.{domain}/.well-known/acme-challenge/{t}` with `hash(s)`
4. let's encrypt request `http://{custom-domain}/.well-known/acme-challenge/{t}`
5. our relay server received the request
	1. look up dns record `TXT _dstack-app-address.{custom-domain}`, get `{app-id}`
	2. look up dns record `CNAME {custom-domain}`, get `_.{gateway-base-domain}`
6. redirect to `https://{app-id}.{domain}/.well-known/acme-challenge/{t}`
7. let's encrypt accept the solution, and authenticate s
8. acme client request the certificate using s

Notes:
- acme client runs inside dstack (a docker env in a confidential vm)
- dstack auto serve http in https usin it's allocated domain (via the gateway)

## Tasks

- [x] Set up TXT and CNAME
    - custom-domain=http01-test.phala.systems
- [x] Prepare a docker compose manifest of a simple acme client that listen to 80 and print logs
- [ ] Deploy an acme client in dstack
    - app-id=
    - gateway-base-domain=prod5.phala.network
- [x] Write a relay server that implements the protocol as described above using rust
- [ ] Test issue a certificate from the acme client

## Components

### 1. ACME Client (Docker)

Located in `acme-client/`. See [acme-client/README.md](acme-client/README.md) for details.

**Quick start:**
```bash
cd acme-client

# Build the image
docker build -t dstack-acme-client:latest .

# Run locally for testing
docker run -p 80:80 dstack-acme-client:latest
```

### 2. Relay Server (Rust)

Located in `relay-server/`. See [relay-server/README.md](relay-server/README.md) for details.

**Quick start:**
```bash
cd relay-server

# Build
cargo build --release

# Run (requires sudo for port 80)
sudo ./target/release/relay-server

# Or using Docker
docker pull h4x3rotab/dstack-http01-relay-server:latest
docker run -p 8081:8081 h4x3rotab/dstack-http01-relay-server:latest
```

## Deployment Guide

### Step 1: Deploy Relay Server

Deploy the relay server on a machine with:
- Public IP address
- Port 80 accessible from the internet (or use nginx to proxy)
- DNS resolution capability

**Option A: Direct on port 80**
```bash
docker pull h4x3rotab/dstack-http01-relay-server:latest
docker run -d \
  --name relay-server \
  --restart unless-stopped \
  -p 80:80 \
  -e PORT=80 \
  -e FALLBACK_GATEWAY_DOMAIN=prod5.phala.network \
  h4x3rotab/dstack-http01-relay-server:latest
```

**Option B: Behind nginx on port 8081 (recommended)**
```bash
docker pull h4x3rotab/dstack-http01-relay-server:latest
docker run -d \
  --name relay-server \
  --restart unless-stopped \
  -p 8081:8081 \
  -e FALLBACK_GATEWAY_DOMAIN=prod5.phala.network \
  h4x3rotab/dstack-http01-relay-server:latest

# Then configure nginx to proxy port 80 to localhost:8081
```

### Step 2: Configure DNS

For your custom domain (e.g., `http01-test.phala.systems`):

```
# Point custom domain to relay server's IP
A http01-test.phala.systems         <relay-server-ip>

# Configure dstack app address
TXT _dstack-app-address.http01-test.phala.systems  {app-id}:80

# Configure gateway domain
CNAME http01-test.phala.systems     _.prod5.phala.network
```

### Step 3: Deploy ACME Client in dstack

1. Build and push your ACME client Docker image
2. Deploy to dstack with port 80 exposed
3. Note the assigned `app-id` from dstack
4. Update the DNS TXT record with the actual `app-id`

### Step 4: Test Certificate Issuance

From your ACME client in dstack:
```bash
# Request a certificate using certbot or your ACME client
certbot certonly --standalone -d http01-test.phala.systems
```

The flow:
1. Let's Encrypt requests `http://http01-test.phala.systems/.well-known/acme-challenge/{token}`
2. Relay server looks up DNS records
3. Relay server redirects to `https://{app-id}.prod5.phala.network/.well-known/acme-challenge/{token}`
4. ACME client responds with the challenge
5. Certificate issued!

## Monitoring

The relay server exposes metrics at `http://<relay-server>:80/metrics`:
- HTTP request counts and durations
- DNS lookup statistics
- Redirect success/failure rates

## Other notes

- use deepwiki (Dstack-TEE/dstack for gateway) to learn more details
- use PhalaDocs to learn dstack basics
