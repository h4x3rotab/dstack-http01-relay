# Docker Deployment Guide

This directory contains multiple Docker deployment options for the relay server.

## Files

- `Dockerfile` - Build definition for the relay server
- `docker-compose.yml` - Basic deployment (port 8081)
- `docker-compose.nginx.yml` - Deployment with nginx proxy (port 80)
- `nginx.conf` - Nginx configuration for proxying
- `.env.example` - Environment variable template

## Quick Start

### Option 1: Basic Deployment (Port 8081)

```bash
# Start the relay server
docker-compose up -d

# View logs
docker-compose logs -f relay-server

# Check health
curl http://localhost:8081/health
```

This runs the relay server on port 8081. You'll need to expose it to the internet using nginx or another reverse proxy.

### Option 2: With Nginx Proxy (Port 80)

```bash
# Start both relay server and nginx
docker-compose -f docker-compose.nginx.yml up -d

# View logs
docker-compose -f docker-compose.nginx.yml logs -f

# Check health
curl http://localhost/health
```

This runs the relay server on port 8081 internally and nginx on port 80 externally.

## Configuration

### Environment Variables

Edit the environment section in `docker-compose.yml` or create a `.env` file:

```bash
# Copy the example
cp .env.example .env

# Edit the values
nano .env
```

Key variables:
- `PORT` - Port to listen on (default: 8081)
- `FALLBACK_GATEWAY_DOMAIN` - Fallback gateway domain (default: prod5.phala.network)
- `ALLOWED_DOMAIN_REGEX` - Regex to match CNAME records
- `RUST_LOG` - Logging level (relay_server=info)

### Nginx Configuration

The `nginx.conf` file configures nginx to:
- Listen on port 80
- Proxy all requests to `relay-server:8081`
- Preserve the Host header (critical!)
- Forward client IP addresses
- Set appropriate timeouts

You can customize it by editing `nginx.conf` and restarting:
```bash
docker-compose -f docker-compose.nginx.yml restart nginx
```

## Testing

### Test DNS Resolution

```bash
# Test with a custom domain
curl -v http://localhost:8081/.well-known/acme-challenge/test-token \
  -H "Host: http01-test.phala.systems"

# Should return 307 redirect to:
# https://{app-id}.prod5.phala.network/.well-known/acme-challenge/test-token
```

### View Metrics

```bash
# Prometheus metrics
curl http://localhost:8081/metrics

# With nginx
curl http://localhost/metrics
```

## Logs

### View Logs

```bash
# Relay server logs
docker-compose logs -f relay-server

# Nginx logs (if using nginx setup)
docker-compose -f docker-compose.nginx.yml logs -f nginx

# Both services
docker-compose -f docker-compose.nginx.yml logs -f
```

### Log Configuration

Logs are configured to rotate automatically:
- Max size: 10MB per file
- Max files: 3 files
- Driver: json-file

## Management

### Start/Stop

```bash
# Start
docker-compose up -d
# or
docker-compose -f docker-compose.nginx.yml up -d

# Stop
docker-compose down
# or
docker-compose -f docker-compose.nginx.yml down

# Restart
docker-compose restart
# or
docker-compose -f docker-compose.nginx.yml restart
```

### Update

```bash
# Pull latest image
docker-compose pull

# Restart with new image
docker-compose up -d

# Or force recreate
docker-compose up -d --force-recreate
```

### Health Checks

The containers include health checks:

```bash
# Check container health
docker ps

# View health check logs
docker inspect dstack-relay-server | jq '.[0].State.Health'
```

## Troubleshooting

### Container won't start

```bash
# Check logs
docker-compose logs relay-server

# Check if port is in use
sudo lsof -i :8081
# or
sudo lsof -i :80
```

### DNS resolution not working

```bash
# Exec into container
docker exec -it dstack-relay-server sh

# Test DNS
nslookup http01-test.phala.systems
dig TXT _dstack-app-address.http01-test.phala.systems
```

### Nginx can't connect to relay-server

```bash
# Check if relay server is healthy
docker ps
curl http://localhost:8081/health

# Check nginx logs
docker-compose -f docker-compose.nginx.yml logs nginx

# Verify network connectivity
docker exec relay-nginx wget -O- http://relay-server:8081/health
```

## Production Recommendations

1. **Use the nginx setup** (`docker-compose.nginx.yml`) for production
2. **Enable HTTPS** on nginx for security (add SSL/TLS configuration)
3. **Set up monitoring** using the `/metrics` endpoint
4. **Use a `.env` file** for sensitive configuration
5. **Set proper DNS records** before deploying
6. **Test thoroughly** with curl before requesting real certificates

## Examples

### Production Deployment

```bash
# 1. Configure environment
cat > .env <<EOF
PORT=8081
FALLBACK_GATEWAY_DOMAIN=prod5.phala.network
RUST_LOG=relay_server=info
EOF

# 2. Start with nginx
docker-compose -f docker-compose.nginx.yml up -d

# 3. Verify
curl http://localhost/health
curl -v http://localhost/.well-known/acme-challenge/test -H "Host: myapp.example.com"

# 4. Monitor
docker-compose -f docker-compose.nginx.yml logs -f
```

### Development/Testing

```bash
# Run on a different port for testing
PORT=9000 docker-compose up -d

# Test
curl http://localhost:9000/health
```
