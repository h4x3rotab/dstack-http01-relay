FROM certbot/certbot:latest

# Install nginx for serving challenges
RUN apk add --no-cache nginx

# Create necessary directories with proper permissions
RUN mkdir -p /var/www/certbot/.well-known/acme-challenge && \
    mkdir -p /run/nginx && \
    mkdir -p /var/log/nginx && \
    chmod -R 777 /var/www/certbot

# Copy nginx configuration
COPY nginx.conf /etc/nginx/nginx.conf

# Create a simple startup script
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

# Expose port 80
EXPOSE 80

# Use the entrypoint script
ENTRYPOINT ["/entrypoint.sh"]
