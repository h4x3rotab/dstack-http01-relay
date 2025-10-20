# dstack ACME Client

A Docker image for serving ACME HTTP-01 challenges in dstack.

## Features

- Runs nginx on port 80
- Serves ACME challenge files from `/.well-known/acme-challenge/`
- Includes certbot for certificate management
- Logs all requests for debugging

## Building

```bash
docker build -t dstack-acme-client:latest .
```

## Running Locally

### Docker Run
```bash
# Run the container
docker run -d -p 80:80 --name acme-client dstack-acme-client:latest

# Create a test challenge
docker exec acme-client sh -c 'echo "test-response" > /var/www/certbot/.well-known/acme-challenge/test-token'

# Test it
curl http://localhost/.well-known/acme-challenge/test-token
```

### Docker Compose
```bash
# Start the service
docker-compose up -d

# Create a test challenge
docker exec dstack-acme-client sh -c 'echo "test-response" > /var/www/certbot/.well-known/acme-challenge/test-token'

# Test it
curl http://localhost/.well-known/acme-challenge/test-token
```

## Deploying to dstack

1. Build and push the image to a registry:
   ```bash
   docker tag dstack-acme-client:latest your-registry/dstack-acme-client:latest
   docker push your-registry/dstack-acme-client:latest
   ```

2. Deploy to dstack with port 80 exposed

3. Note the assigned `app-id` from dstack

4. Configure DNS records for your custom domain (see main [README.md](../README.md) for DNS setup)

## Requesting a Certificate

Once deployed and DNS is configured:

```bash
# Exec into the container
docker exec -it dstack-acme-client sh

# Request a certificate using webroot mode
certbot certonly \
  --webroot \
  -w /var/www/certbot \
  --non-interactive \
  --agree-tos \
  --email your-email@example.com \
  -d your-custom-domain.com

# Certificates will be in /etc/letsencrypt/live/your-custom-domain.com/
```

**Prerequisites:**
- DNS records properly configured
- Relay server running and accessible on port 80
- ACME client deployed in dstack with assigned app-id

## Endpoints

- `/.well-known/acme-challenge/*` - ACME challenge files
- `/health` - Health check (returns "OK")
- `/` - File browser (for debugging)

## Logs

View nginx logs:
```bash
docker exec <container-id> tail -f /var/log/nginx/access.log
docker exec <container-id> tail -f /var/log/nginx/acme-challenge.log
```
