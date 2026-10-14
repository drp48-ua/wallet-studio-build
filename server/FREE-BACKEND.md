# Backend gratuito: Render + Supabase + Brevo

Adaptación implementada. Las pruebas de adaptadores son simuladas: no acreditan conexión real a PostgreSQL, Supabase Storage ni Brevo. No se han recibido ni guardado credenciales de esos servicios.

## Configuración

- Render Free: `WALLET_STORAGE_MODE=remote`, `WALLET_MAX_CONCURRENT_REQUESTS=1`. Sin configuración remota completa el servidor no arranca; en Render no se permite SQLite/almacenamiento efímero alternativo. `WALLET_DATA` deja de almacenar cuentas/copias en este modo.
- `DATABASE_URL`: URI exacta de Supabase **Transaction pooler IPv4**, puerto 6543, usuario y contraseña de base de datos; codificar caracteres especiales del password. No es la URL pública REST del proyecto. Psycopg 3.3.6 desactiva sentencias preparadas y fuerza `sslmode=verify-full` y el archivo CA del contenedor; `DATABASE_SSL_ROOT_CERT` permite otro certificado CA válido, nunca una degradación de TLS.
- `SUPABASE_URL`: URL pública del proyecto; `SUPABASE_SERVICE_ROLE_KEY`: clave de servidor, nunca publicable/anon ni incluida en cliente, GitHub o logs; `SUPABASE_STORAGE_BUCKET=wallet-originals`. El servicio verifica que el bucket sea privado y puede crearlo si la API devuelve 404. Un bucket público impide arrancar. Si el panel muestra bucket inexistente con otro código, crearlo privado desde el panel y volver a arrancar.
- `BREVO_API_KEY`, `BREVO_SENDER_EMAIL`, `BREVO_SENDER_NAME`: clave API de servidor y remitente previamente verificado en Brevo. Se envía por HTTPS a `/v3/smtp/email`; no por SMTP. Render Free bloquea los puertos SMTP habituales. Sin clave Brevo el modo remoto no recurre a SMTP ni imprime códigos: la activación falla de forma explícita.
- Mantener configuración Shopify y `WALLET_AUTH_SECRET` de `.env.example`. Ninguna clave de Supabase/Brevo se entrega a Wallet Studio: el cliente conserva la misma API autenticada del servicio.

## Integridad y concurrencia

Tablas en `wallet_private`, separado del esquema API público. Se revocan permisos de esquema y tablas a PUBLIC/anon/authenticated, y se habilita RLS sin políticas públicas. Conectar como propietario de este esquema/servicio; no dar esa credencial a clientes.

Una transacción PostgreSQL mantiene `pg_advisory_xact_lock` durante **toda** la operación de cuenta. Los bloques anidados reutilizan exactamente la misma conexión con savepoints. Así se serializan entre procesos el límite de tres equipos, consumo de códigos, cambio de contraseña y orden de eventos de Shopify. No se usan bloqueos de sesión, `search_path` persistente ni sentencias preparadas, incompatibles con el pooler de transacciones. Las comprobaciones de cuenta fallidas conservan los contadores OTP ya guardados; una excepción SQL revierte la operación.

Objetos privados: `bucket/UUID_USUARIO/SHA256`. Se crea con `x-upsert:false`, sin sobrescribir, seguido de descarga autenticada y verificación exacta. Un objeto huérfano de una transacción interrumpida solo se reutiliza si coincide con el hash y los bytes esperados. No se producen URLs públicas ni enlaces firmados para saltar la cuenta. Un fallo de almacenamiento no crea un índice de copia válido.

Límite remoto de paquete: **32 MiB** (el cliente antiguo puede preparar hasta 100 MiB; paquetes mayores se rechazan con HTTP 413 y los originales locales quedan intactos). Las revisiones acumuladas pueden alcanzar ese límite; no se truncan ni descartan automáticamente. Mantener una copia privada independiente del proyecto y sus objetos antes de uso comercial. Las cuotas gratuitas de almacenamiento/correo y las pausas del alojamiento siguen aplicándose; no se promete disponibilidad comercial ilimitada.

## Pruebas

`python -m unittest discover -s server/tests -v` mantiene las pruebas locales de cuentas y añade pruebas simuladas del adaptador SQL, savepoints, TLS obligatorio, objetos inmutables, bucket público rechazado, correo HTTPS y bloqueo de persistencia efímera. Estos tests fuerzan modo local para no conectarse accidentalmente a una base de producción.

La prueba PostgreSQL real es optativa y no se ha ejecutado aquí: instalar `requirements.txt`, usar una **base dedicada a datos sintéticos**, configurar `WALLET_TEST_POSTGRES_URL`, `DATABASE_SSL_ROOT_CERT` apropiado y `WALLET_TEST_POSTGRES_CONFIRM=isolated-synthetic-database`. Ejecutar `python -m unittest discover -s server/tests -p test_postgres_integration.py -v`. Crea únicamente sus registros sintéticos y los elimina al finalizar; no usa `DATABASE_URL` implícitamente. Antes del despliegue comprobar también una entrega Brevo real y subida/descarga privada de una copia sintética.

Documentación oficial consultada: [pooler y conexión](https://supabase.com/docs/guides/database/connecting-to-postgres), [preparadas desactivadas](https://supabase.com/docs/guides/troubleshooting/disabling-prepared-statements-qL8lEL), [descarga autenticada](https://supabase.com/docs/guides/storage/serving/downloads), [subida sin sobrescritura](https://supabase.com/docs/guides/storage/uploads/standard-uploads), [control de Storage](https://supabase.com/docs/guides/storage/security/access-control), [transacciones anidadas Psycopg](https://www.psycopg.org/psycopg3/docs/basic/transactions.html), [correo Brevo](https://developers.brevo.com/docs/send-a-transactional-email).
