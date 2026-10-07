FROM mcr.microsoft.com/mssql/server:2022-latest@sha256:97b448857967be55e005424a660056fe6d51814435804dc07e8f79f028bab5fb

USER root
COPY mssql/entrypoint.sh /fixture/entrypoint.sh
COPY mssql/start-mssql.sh /fixture/start-mssql.sh
COPY mssql/setup.sql /fixture/setup.sql
RUN mkdir -p /var/opt/mssql/secrets \
    && chown -R mssql:root /fixture /var/opt/mssql/secrets \
    && chmod 500 /fixture/entrypoint.sh /fixture/start-mssql.sh

ENTRYPOINT ["/fixture/entrypoint.sh"]
