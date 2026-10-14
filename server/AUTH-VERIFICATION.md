# Comprobaciones de cuentas · 2026-10-15

Datos usados: pedidos, correos, tokens, equipos y copias exclusivamente sintéticos. No se leyó ni escribió ninguna tarjeta física.

## Resultado

- Python: `python -m unittest discover -s server/tests -p 'test_*.py' -v`: **22 pruebas correctas**, 20,344 s en la última suite completa. Incluye acceso por compra concreta, activación por código, caducidad/repetición/intentos, contraseñas no coincidentes sin consumir código, solicitudes HTTP malformadas, HMAC y devoluciones, eventos repetidos/antiguos/con igual fecha, rechazo de fechas sin zona horaria, usuarios creados por administrador sin compra, sesiones antiguas, expiración/inactividad, carrera de tres plazas y recuperación de un cuarto equipo por correo.
- Regresión de concurrencia: un inicio con contraseña antigua no puede emitir un token válido después de la recuperación que cambia contraseña e invalida sesiones. Se coordinaron dos hilos alrededor de la comprobación de contraseña y se comprobó revocación del token antiguo y validez del nuevo.
- Windows: suite `build-wallet-0.6.0.ps1 -TestOnly`: **132 correctas**, una integración ignorada expresamente y cuatro pruebas de hardware/catálogo personal filtradas. Prueba de motor adicional: **1 correcta**.
- Cliente Windows real, WinHTTP + DPAPI + servidor temporal: `run-wallet-cloud-fixture.py`: **1 integración correcta**, 2,84 s. Compila las fuentes finales de cliente y comprueba propietario, aislamiento, ausencia de sobrescritura, recuperación de archivo ausente, conservación de archivo alterado, no seguimiento de redirecciones autenticadas, persistencia/cifrado del identificador del equipo y separación entre perfiles sintéticos.
- `git diff --check`: sin errores (solo avisos existentes de conversión LF/CRLF).

## Fallos encontrados y corregidos

- La primera ejecución Python sin elevación falló por permisos sobre archivos temporales virtualizados de Windows; se repitió fuera de esa virtualización con carpeta temporal de la tarea.
- La primera respuesta de registro deshabilitado era 401: se mantuvo el contrato 403 de la suite anterior.
- Revisión del responsable detectó una carrera entre verificar contraseña y recuperar cuenta. Se extendió el bloqueo de acceso y se añadió la regresión concurrente.
- La integración inicial era correcta pero el servidor estándar imprimía un cierre normal de socket incluyendo dirección de par. Se interceptaron desconexiones normales y se restringió el error inesperado a un mensaje fijo; la integración repetida terminó limpia.

## Pendientes de aceptación/despliegue

Ampliación gratuita: suite de Python ejecutó 30 pruebas: **29 correctas y 1 integración PostgreSQL omitida expresamente**, sin servidor local ni credenciales. Siete pruebas nuevas de adaptadores son simuladas; no sustituyen PostgreSQL/Storage/Brevo reales. `server/tests/postgres_smoke.sql` permite al responsable comprobar el motor PostgreSQL desde su panel sin compartir claves; usa una transacción con ROLLBACK y datos sintéticos. La entrega HTTP real del cliente Windows continúa utilizando el contrato anterior, sin cambios Rust en esta ampliación.

El responsable comprobó el icono transparente y el formulario centrado de correo en Windows 0.6.1. Los demás pasos visuales siguen pendientes. El ajuste final de espera HTTP requiere recompilación y repetición de la integración de red.

Comprobación adicional en PostgreSQL real de Supabase: prueba SQL sintética con siete tablas/tipos, índices, upserts, RLS y tres plazas devolvió `SQL_SMOKE_OK_ROLLBACK_NEXT`; consulta posterior `to_regnamespace('wallet_private') IS NULL` devolvió `true`. Todos los cambios de esa prueba se revirtieron. No valida todavía Psycopg/Storage/Brevo reales.

Pendientes HTTPS/correo reales, webhooks de Shopify y compra de prueba del producto autorizado, aún en borrador. El usuario creó Supabase Free y accedió a Render/Brevo; Render acepta el Blueprint gratuito pero no se ha desplegado ni se han configurado secretos. Un binario sin endpoint oficial no concede acceso. No se probó macOS ni se promete atribución de IP final detrás de un proxy. Checkpoint al 2 % restante de ventana corta y 10 % semanal; no se utilizó crédito de reinicio. Estado operativo detallado en CONTINUAR-CLIENTES-2026-10-14.md.
