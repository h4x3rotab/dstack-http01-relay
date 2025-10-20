# Quick Start Guide

## Local Testing

### 1. Test the Relay Server

```bash
# Build and run the relay server
cd relay-server
cargo build --release

# In one terminal (requires sudo for port 80)
sudo RUST_LOG=relay_server=info ./target/release/relay-server

# In another terminal, test with a mock DNS lookup
# Note: This will fail DNS lookup but demonstrates the server is running
curl -v http://localhost/.well-known/acme-challenge/test-token \
  -H "Host: test.example.com"
```

Expected: Server logs showing DNS lookup attempt and a 502 Bad Gateway response (since DNS records don't exist).

### 2. Test the ACME Client Setup

```bash
# Build and run the ACME client
cd acme-client
docker build -t dstack-acme-client:latest .
docker run -d -p 80:80 --name acme-test dstack-acme-client:latest

# Create a test challenge file
docker exec acme-test sh -c 'echo "test-response" > /var/www/certbot/.well-known/acme-challenge/test-token'

# Test the challenge file is served
curl http://localhost/.well-known/acme-challenge/test-token
```

Expected: "test-response" is returned.

## Production Deployment

### Prerequisites

1. A server with a public IP for the relay server
2. Access to configure DNS for your custom domain
3. A dstack deployment with an ACME client listening on port 80

### Deployment Steps

1. **Deploy Relay Server**
   ```bash
   # Pull the pre-built image
   docker pull h4x3rotab/dstack-http01-relay-server:latest

   # Run on port 80 (requires root)
   docker run -d \
     --name relay-server \
     --restart unless-stopped \
     -p 80:80 \
     -e PORT=80 \
     -e RUST_LOG=relay_server=info \
     -e FALLBACK_GATEWAY_DOMAIN=prod5.phala.network \
     h4x3rotab/dstack-http01-relay-server:latest

   # OR run on 8081 and proxy with nginx (recommended)
   docker run -d \
     --name relay-server \
     --restart unless-stopped \
     -p 8081:8081 \
     -e RUST_LOG=relay_server=info \
     -e FALLBACK_GATEWAY_DOMAIN=prod5.phala.network \
     h4x3rotab/dstack-http01-relay-server:latest
   ```

2. **Configure DNS**

   Replace with your actual values:
   - `{relay-ip}`: Public IP of your relay server
   - `{app-id}`: Your dstack application ID
   - `{custom-domain}`: Your custom domain (e.g., `app.example.com`)

   ```
   A {custom-domain}                            {relay-ip}
   TXT _dstack-app-address.{custom-domain}      {app-id}:80
   CNAME {custom-domain}                        _.prod5.phala.network
   ```

3. **Deploy ACME Client to dstack**

   Use the docker-compose.yml as a reference to create your dstack deployment with certbot.

4. **Test End-to-End**
   ```bash
   # From your ACME client in dstack
   certbot certonly \
     --standalone \
     --non-interactive \
     --agree-tos \
     --email your-email@example.com \
     -d {custom-domain}
   ```

### Monitoring

Check relay server logs:
```bash
docker logs -f relay-server
```

Check metrics:
```bash
curl http://{relay-ip}/metrics
```

Check health:
```bash
curl http://{relay-ip}/health
```

## Troubleshooting

### DNS Lookup Failures

Check DNS records are configured correctly:
```bash
# Check TXT record
dig TXT _dstack-app-address.{custom-domain}

# Check CNAME record
dig CNAME {custom-domain}
```

### Relay Server Not Receiving Requests

1. Verify the A record points to the relay server
2. Check firewall allows port 80
3. Verify the relay server is running: `curl http://{relay-ip}/health`

### ACME Client Not Responding

1. Check the app is running in dstack
2. Verify it's accessible at `https://{app-id}.prod5.phala.network/`
3. Test the challenge endpoint directly:
   ```bash
   curl https://{app-id}.prod5.phala.network/.well-known/acme-challenge/test
   ```

## Example: Complete Flow

```bash
# Given:
# - Relay server at: 203.0.113.10
# - Custom domain: myapp.example.com
# - dstack app-id: my-app-12345
# - Gateway domain: prod5.phala.network

# 1. DNS Configuration
A myapp.example.com                               203.0.113.10
TXT _dstack-app-address.myapp.example.com         my-app-12345:80
CNAME myapp.example.com                           _.prod5.phala.network

# 2. Test the flow
curl -v http://myapp.example.com/.well-known/acme-challenge/test-token

# Expected: 307 redirect to:
# https://my-app-12345.prod5.phala.network/.well-known/acme-challenge/test-token

# 3. Get certificate
certbot certonly --standalone -d myapp.example.com
```
