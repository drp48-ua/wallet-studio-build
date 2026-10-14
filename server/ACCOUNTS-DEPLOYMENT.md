# Cuentas de compradores y tres equipos

Implementado en código; este documento no acredita un despliegue online.

Producto preparado en Shopify: `10459984298313` (Wallet Studio), en estado **borrador**, sin publicar; precio comercial pendiente. `.env.example` incluye exclusivamente ese producto y el dominio `store-09240616prrz2-itsdpkjq.myshopify.com`. Los productos anteriores de la tienda no conceden acceso.

## Configuración del servicio

1. Desplegar `server/` con Python 3.12 o Docker Compose, volumen privado persistente y copias del volumen. SQLite usa WAL; ejecutar una instancia del servicio. `compose.yaml` solo publica el puerto en localhost: situar un proxy HTTPS delante.
2. Copiar `.env.example` a `.env` en el servidor. Configurar el dominio real `*.myshopify.com`, los **IDs exactos del producto Wallet Studio**, el secreto de firma de Shopify, un secreto aleatorio de autenticación de al menos 32 caracteres y SMTP autenticado. No subir `.env`, la base de datos ni el volumen a GitHub. SMTP requiere STARTTLS y validación del certificado.
3. Registrar webhooks `orders/paid`, `orders/updated` y `orders/cancelled`, todos en `https://SERVICIO/v1/shopify/webhook`. Se valida HMAC-SHA256 sobre bytes originales y dominio de tienda. Los eventos son idempotentes; los antiguos no pueden reactivar una compra revocada. La devolución íntegra de las unidades del producto autorizado revoca acceso aunque otros productos del pedido sigan pagados.
4. Las compras anteriores al alta del webhook **no aparecen automáticamente**. Debe realizarse una reconciliación autenticada con la API de Shopify, con la misma política de producto/pago/devolución. No existe importación pública de compras ni formulario que conceda acceso por conocer un correo.
5. Compilar el cliente con `WALLET_STUDIO_CLOUD_URL=https://SERVICIO`. Los binarios de distribución fijan ese servicio: no se puede sustituir por una URL escrita en la interfaz. Sin servicio configurado/operativo no hay acceso. Publicar la versión probada mediante el canal de actualizaciones existente.

## Activación y sesiones

- `POST /v1/auth/start`: `{email}`. Respuesta genérica; si hay compra activa y cuenta todavía no creada, entrega un código por correo. Caduca a los 10 minutos, máximo cinco intentos, reenvío después de 60 segundos y límites de frecuencia por correo/IP. No se imprime el código ni el correo en logs.
- `POST /v1/auth/activate`: `{email,code,password,password_confirmation,device_id}`. Verifica posesión del correo y crea la contraseña (12–128 caracteres). El código se consume y no puede repetirse. Contraseñas PBKDF2-HMAC-SHA256 con sal aleatoria y 600.000 iteraciones.
- `POST /v1/auth/login`: `{email,password,device_id}`. Comprobar contraseña no basta: deben existir compra activa y plaza de equipo.
- El equipo tiene un identificador aleatorio de 256 bits, guardado con DPAPI de Windows, compartido por versiones en el almacenamiento estable. No se identifica un equipo por IP, ni por el iPhone o la tarjeta. La dirección del par de red se conserva únicamente como HMAC por equipo; cambiar de red no consume plazas. Detrás de Render u otro proxy, esa dirección puede ser la del proxy y **no permite atribuir la IP del cliente**. No se confía en cabeceras reenviadas arbitrarias. Hace falta configurar proxies de confianza y retención antes de usar la IP final como señal de abuso; el límite de tres equipos no depende de ella.
- Registro de equipos atómico (transacción SQLite): máximo **tres**. Reinstalar conservando el almacenamiento local reutiliza la plaza. Borrar todos los datos locales o pasar a otro usuario de Windows crea otro equipo. En Cuenta se pueden consultar y desvincular los otros equipos desde un PC autorizado.
- Desde un cuarto PC o si se olvidó la contraseña: elegir «Recuperar contraseña / liberar equipos». `POST /v1/auth/start` con `{email,purpose:"recover"}` envía otro código solo al correo del comprador existente. `POST /v1/auth/recover` recibe `{email,code,password,password_confirmation,device_id,reset_devices}`. Verifica el código, cambia la contraseña e invalida las sesiones anteriores. Si el usuario marca expresamente `reset_devices:true`, elimina sus plazas previas y registra el nuevo equipo. Sin esa autorización, un cuarto equipo sigue rechazado, conservando contraseña/código anteriores. Ninguna lista de originales o sesión de acceso se entrega antes de verificar el correo. Los códigos de activación no sirven para recuperación, ni a la inversa.
- Sesión DPAPI persistente: caducidad absoluta de 90 días y por inactividad de 30 días. El servidor comprueba compra, equipo y revocación en cada petición protegida. El cliente revalida al abrir y cada 60 segundos cuando no opera sobre el teléfono; no ofrece acceso sin conexión. Cerrar sesión local conserva originales y propietario del archivo.
- `GET /v1/auth/session`: requiere Bearer, devuelve propietario y equipos. `POST /v1/auth/devices/revoke` con Bearer y `{device_id}` elimina plaza y sesiones de ese equipo. `POST /v1/auth/logout` revoca el token actual.

## Protección de las copias

La puerta de acceso precede a las consultas del teléfono y capturas automáticas. Vincular una cuenta distinta no cambia el propietario de originales existentes. Los originales e historiales siguen siendo inmutables y segregados por cuenta, iPhone y tarjeta. La CLI de operaciones requiere sesión validada; la devolución de un recurso de caché ya preservado permanece accesible para recuperación de emergencia.

Si caduca la cuenta o cae el servicio durante una devolución pendiente: `"Wallet Studio.exe" --wallet-studio-list-pending` enumera exclusivamente los tokens/fases de diarios locales existentes; `"Wallet Studio.exe" --wallet-studio-return-pending TOKEN` exige un diario pendiente que coincida exactamente, copia preservada con hash verificado y el mismo único iPhone USB. No acepta rutas arbitrarias, capturas nuevas ni diseños. El motor sigue comprobando la fase y la preimagen de Libros antes de devolver. `--wallet-studio-restore-cache TOKEN` conserva también su validación estricta de devolución de caché existente. Estas rutas de emergencia no requieren licencia online.

Para revisión visual aislada de la pantalla de acceso: `--wallet-studio-auth-demo`, o una copia del binario llamada `WSDEMOAU.exe`. Solo previsualiza los pasos; desactiva cuentas, red y teléfono usando la misma raíz temporal de demostración existente.

Las versiones antiguas instaladas necesitan actualizarse: una cuenta obligatoria no puede imponerse retroactivamente a un ejecutable viejo que no incluye esta lógica. El código abierto tampoco constituye DRM inviolable. No se ha validado ni portado este flujo a macOS.

## Verificación sin clientes reales

`python -m unittest discover -s server/tests -v` utiliza únicamente pedidos/correos/equipos ficticios. `allow_registration=True` solo existe como constructor de fixtures de integración; el arranque de producción ya no permite activarlo mediante una variable de entorno. Falta la comprobación completa sobre HTTPS/SMTP/Shopify reales y el primer arranque de un binario distribuido apuntando al servicio desplegado.
