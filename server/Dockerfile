FROM python:3.12-slim
WORKDIR /service
COPY app.py /service/app.py
COPY accounts.py /service/accounts.py
RUN useradd --uid 10001 --create-home wallet && mkdir /data && chown wallet:wallet /data
USER wallet
EXPOSE 8766
CMD ["python", "app.py", "--host", "0.0.0.0", "--data", "/data"]
