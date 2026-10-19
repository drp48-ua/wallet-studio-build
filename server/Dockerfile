FROM python:3.12-slim
WORKDIR /service
COPY requirements.txt /service/requirements.txt
RUN pip install --no-cache-dir -r requirements.txt && test -s /etc/ssl/certs/ca-certificates.crt
COPY app.py /service/app.py
COPY accounts.py /service/accounts.py
COPY designs.py design-catalog.json design-catalog.legacy.json /service/
COPY remote.py /service/remote.py
COPY certs/supabase-root-2021.crt /service/certs/supabase-root-2021.crt
RUN cat /etc/ssl/certs/ca-certificates.crt /service/certs/supabase-root-2021.crt > /service/certs/postgres-trust.pem
ENV DATABASE_SSL_ROOT_CERT=/service/certs/postgres-trust.pem
RUN useradd --uid 10001 --create-home wallet && mkdir /data && chown wallet:wallet /data
USER wallet
EXPOSE 8766
CMD ["python", "app.py", "--host", "0.0.0.0", "--data", "/data"]
