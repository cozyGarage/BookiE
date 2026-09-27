FROM mcr.microsoft.com/mssql/server:2022-latest

USER root
COPY materials/server.crt /certs/server.crt
COPY materials/server.key /certs/server.key
COPY mssql/entrypoint.sh /fixture/entrypoint.sh
COPY mssql/setup.sql /fixture/setup.sql

RUN chown -R mssql:root /certs /fixture \
    && chmod 400 /certs/server.key \
    && chmod 444 /certs/server.crt \
    && chmod 500 /fixture/entrypoint.sh

USER mssql
ENTRYPOINT ["/fixture/entrypoint.sh"]
