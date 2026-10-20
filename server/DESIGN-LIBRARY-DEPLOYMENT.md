# Biblioteca y pack privado 0.6.5

## Contrato y compatibilidad

Único producto comercial: **Wallet Studio, app +50diseños,6,99EUR**, ID `10459984298313`. Configurar `SHOPIFY_APP_PRODUCT_IDS=10459984298313`, `SHOPIFY_BUNDLE_PRODUCT_IDS=` y `SHOPIFY_PACK_PRODUCT_IDS=`. Los borradores alternativos10464741196105 y10464743457097 están archivados; no se reactivan ni se publican desde este código. Conservar `SHOPIFY_PRODUCT_IDS=10459984298313` como fallback para despliegues anteriores.

Toda licencia activa de la app da acceso al catálogo `studio-50-v1`, incluidas las compras anteriores ya registradas en `purchase_orders`: no requiere reescribir pedidos, sesiones ni una nueva compra. Una devolución/cancelación que retire la última licencia app retira también estos diseños incluidos. Los derechos históricos de pack en `purchase_packs` se conservan para migración, pero jamás conceden licencia app ni originales/teléfono. No hay un pack separado a la venta.

El endpoint antiguo `/v1/auth/session` sigue exigiendo app para que clientes anteriores no interpreten derechos históricos de pack como licencia app. `/v1/auth/account` permite cualquiera de los derechos históricos y devuelve `app_licensed`; el cliente nuevo solo recurre al endpoint antiguo si el servidor devuelve404. Una denegación401/403 nunca permite ese fallback. Los clientes con licencia app continúan accediendo a todas las funciones; una cuenta histórica sin licencia app solo dispone de Biblioteca/Cuenta.

La tabla nueva `purchase_packs` guarda las compras de diseños por pedido, separada de `purchase_orders`. Las filas anteriores de pedidos/licencias, sesiones, usuarios, OTPs y equipos se conservan. PostgreSQL la crea en `wallet_private`, aplica RLS y revoca acceso PUBLIC/anon/authenticated igual que las tablas existentes. No hay importación automática de compras históricas: reconciliarlas mediante eventos/API del servidor de confianza antes del lanzamiento.

## Webhooks

Registrar `orders/paid`, `orders/updated`, `orders/cancelled` y `refunds/create` en `/v1/shopify/webhook`, con el secreto existente `SHOPIFY_WEBHOOK_SECRET` y dominio exacto `SHOPIFY_SHOP_DOMAIN`. Se verifica HMAC del cuerpo original antes de interpretarlo. No usar cookies ni secretos del conector Codex. Los eventos son idempotentes; un estado viejo no revive un pedido. A igual fecha, revocar prevalece.

Los eventos completos de pedido calculan derechos por unidades restantes de cada línea tras sus devoluciones. `refunds/create` no contiene el balance completo: revoca preventivamente ambos derechos de ese pedido, creando un tombstone si llegó antes del pedido. Un `orders/updated` posterior con estado completo puede reconciliar una devolución parcial. Una devolución parcial puede dejar temporalmente el pedido bloqueado hasta recibir esa reconciliación; no se concede acceso con datos incompletos.

## Bucket y catálogo

Solo se versiona `server/design-catalog.json`: schema1, pack_id estable `studio-50-v1`, título y actualmente 56 objetos `designs` con id `ws-001`..`ws-056`, filename correspondiente, title, sha256, bytes, width320, height200. Los 50 objetos anteriores conservan todos sus metadatos y recursos. La validación admite entre50 y100 objetos consecutivos para permitir ampliaciones acotadas. Cada PNG debe ser RGB/RGBA8 sin entrelazado ni animación y máximo1MiB. Los CRC de chunks, tamaño de píxeles descomprimidos, hash y tamaño se verifican. El catálogo tiene máximo64KiB. No introducir imágenes de pago en Git, `windows/designs` ni instaladores.

Usar el bucket **privado** `wallet-designs`, claves `studio-50-v1/ws-001.png`..`ws-056.png`, MIME `image/png`, límite al menos1MiB. No crear políticas de lectura/escritura de ese bucket para anon/authenticated ni enlaces públicos. El servidor usa exclusivamente la service role mediante `SUPABASE_URL` y `SUPABASE_SERVICE_ROLE_KEY`, ya configuradas para copias; esas credenciales nunca llegan al cliente. El bucket bancario existente no cambia. `WALLET_DESIGN_CATALOG` es opcional; por defecto se carga `design-catalog.json` junto a app.py, copiado por Docker. El arranque no descarga imágenes ni exige consultar Storage para este pack; ausencia/corrupción del catálogo o bucket se informa como503 en su API.

Aprovisionamiento desde carpeta privada, con secretos únicamente en variables de entorno:

```powershell
python server/upload_designs.py --source 'RUTA_PRIVADA/studio-50-v1' --catalog server/design-catalog.json
```

Sin filtro, primero valida todos los archivos del catálogo. Crea bucket privado PNG si falta; rechaza bucket público; sube sin sobrescritura y descarga de nuevo para verificar cada objeto. Repetir es idempotente si bytes/hash coinciden; un conflicto no sustituye el objeto existente. El script solo imprime un conteo o un error genérico, sin secretos ni rutas. No ejecutarlo contra otro proyecto ni pegar secretos en terminal/chats. Las pruebas usan fixtures temporales y adaptador HTTP simulado.

Para añadir únicamente los seis diseños nuevos, prepara una carpeta privada con `ws-051.png`..`ws-056.png` y utiliza:

```sh
python server/upload_designs.py --source 'RUTA_PRIVADA/nuevos-seis' --catalog server/design-catalog.json --only ws-051 ws-052 ws-053 ws-054 ws-055 ws-056
```

El filtro valida el catálogo completo y todos los archivos seleccionados antes de cualquier llamada remota. Rechaza IDs desconocidos o repetidos y conserva el modo de creación sin sobrescritura y la lectura posterior. No exige tener en esa carpeta los50archivos anteriores. Publicar los nuevos objetos debe preceder al despliegue que anuncia56diseños. Esta documentación no ejecuta la subida ni el despliegue.

## API y caché

- GET `/v1/designs`: Bearer de sesión vigente; con licencia app activa y cabecera `X-Wallet-Studio-Catalog: expanded-v1` devuelve el catálogo ampliado, actualmente56metadatos, también para compradores anteriores. Sin esa capacidad exacta devuelve los primeros50metadatos para conservar compatibilidad con0.6.5–0.6.7. La cabecera no cambia los derechos. Una cuenta con derechos históricos de pack los conserva sin recibir licencia app. Sin derecho al catálogo responde schema1/entitledfalse/designs vacío. No expone URL de Storage. La app0.6.8 admite tanto una respuesta anterior de50 como la ampliada, con máximo100objetos y64KiB.
- GET `/v1/designs/ws-001`: Bearer y derecho al pack vigentes; devuelve PNG verificado. No acepta user_id, producto, bucket ni rutas del cliente. Un cliente sin sesión recibe401; sin derecho al catálogo recibe403 en descarga. Revocar el equipo invalida su sesión.
- GET/PUT `/v1/bundles` y GET `/v1/auth/session` continúan exigiendo licencia app. GET `/v1/auth/account` devuelve identidad, licencia app y equipos para Biblioteca/Cuenta.

El cliente guarda diseños propios en `%LOCALAPPDATA%/WalletStudio/design-library/SHA256_SERVIDOR_CUENTA/own`, y caché de pago separada en `purchased/SHA256.png`. Importa/guarda sin sobrescribir, verifica lectura y conserva los diseños de aplicación antiguos `guardado-*.png` copiándolos una sola vez a la cuenta, dejando los originales. La biblioteca no sincroniza imágenes personales: originales/historial existentes mantienen su sincronización aparte.

Actualizar compras borra entradas anteriores primero; un error o revocación nunca habilita el pack desde caché. La consulta de metadatos se ejecuta en un worker al pulsar Actualizar. Solo las miniaturas visibles y ausentes programan descarga/lectura/decodificación en otro worker; los PNG ya cacheados se verifican y reutilizan. Las miniaturas se decodifican al entrar en pantalla y se conservan en caché GPU, incluso los fallos para evitar bucles de decodificación por frame. Usar diseño verifica la descarga autenticada de nuevo, aunque exista caché. El diseño preparado/importado/guardado por el usuario es una copia local editable: la app no puede retirar imágenes que el usuario ya haya exportado o guardado.

## Despliegue y revisión

1. Conservar copia privada externa de PostgreSQL/Storage. Mantener `WALLET_AUTH_SECRET`; no borrar sesiones ni tablas.
2. Configurar APP_PRODUCT_IDS con el único producto10459984298313; dejar BUNDLE_PRODUCT_IDS y PACK_PRODUCT_IDS vacíos. Guardar sin publicar productos. Mantener configuración Free y secretos existentes. No habilitar planes de pago.
3. Aprovisionar pack y comprobar que no existen políticas públicas para `wallet-designs`.
4. Revisar/compilar la imagen del servidor con `designs.py` y el catálogo. Desplegar la rama revisada solo con autorización del lead/propietario. `/health` no certifica compras ni Storage.
5. Validar con cuentas/pedidos sintéticos: compra actual app+50; comprador anterior app recibe50sin nueva compra; derechos históricos pack sin originales/phone; HMAC incorrecto; duplicado/orden viejo; cancelación; devolución/cancelación de la última compra app retira50, otra compra app vigente conserva50; equipo revocado; otra cuenta no obtiene archivos; PNG corrupto503. Probar reconciliación de `refunds/create`.
6. Compilar0.6.5 con el endpoint oficial existente `WALLET_STUDIO_CLOUD_URL`. El empaquetador copia solo README en designs, nunca PNGs. Mantiene la firma/canal de actualización existente.

## Revisión visual aislada

Ejecutar el candidato con `--wallet-studio-library-demo --demo-size=960x640` o `--demo-size=1920x1080`. Ese modo crea una raíz temporal nueva y no consulta cuentas ni teléfono. Para mostrar los50 PNG privados en esa demo, configurar `WALLET_STUDIO_DEMO_DESIGNS_DIR` a su carpeta antes de lanzar: se leen imágenes acotadas, verifican320x200 y se copian solo a esa raíz demo temporal. No cambia derechos ni sesiones de producción y no empaqueta las imágenes. Sin esa variable se muestra un diseño propio sintético y el estado vacío del pack. `WALLET_STUDIO_DEMO_EMPTY=1` conserva el escenario vacío.

## Detección de tarjeta

La búsqueda termina con la primera identidad exacta de tarjeta: cancela el bucle, invalida el relay, cierra la sesión de dispositivo y después notifica a la UI. Un evento no sustituye el nombre de una tarjeta ya conocida. Detener búsqueda está habilitado mientras busca; espera al cierre del worker antes de habilitar otra operación. La captura automática ya existente espera nube verificada, USB/identidad exactos, ausencia de recuperaciones y estado original seguro; nunca recertifica un diseño modificado como original. Si nube queda pendiente, la UI muestra su estado concreto, separado de la búsqueda ya terminada. La prueba física requiere autorización humana y no forma parte de las pruebas sintéticas.

Referencias oficiales verificadas: [firma HMAC y duplicados](https://shopify.dev/docs/apps/build/webhooks/verify-deliveries), [payloads de webhooks 2026-10, incluidos pedidos y refunds/create](https://shopify.dev/docs/api/webhooks/latest). El cierre preventivo de una devolución parcial es una decisión de esta implementación; no una garantía de entrega u orden de Shopify.
