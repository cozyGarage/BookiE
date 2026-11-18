FROM mongo:7

COPY materials/server.pem /certs/server.pem
COPY materials/ca.crt /certs/ca.crt
COPY seed/ /docker-entrypoint-initdb.d/

RUN chown -R mongodb:mongodb /certs \
    && chmod 600 /certs/server.pem \
    && chmod 644 /certs/ca.crt
