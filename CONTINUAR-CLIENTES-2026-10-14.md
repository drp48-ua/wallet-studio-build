# Wallet Studio: continuación y publicación

## Actualización 15/10/2026: 0.6.1, cuentas obligatorias

- El icono ahora tiene esquinas transparentes y su marco metálico como límite visible. Se comparten recursos PNG/ICO/ICNS/SVG.
- Acceso obligatorio antes de consultar el teléfono o capturar imágenes. Activación con el correo de compra, código enviado a ese correo y contraseña repetida. El backend exige compra activa del producto autorizado, no simplemente una cuenta creada.
- Máximo tres equipos, identidad aleatoria protegida por DPAPI y compartida entre versiones. Sesión persistente de 90 días, con caducidad por 30 días de inactividad. Revalidación al abrir y en reposo. Recuperación por correo puede revocar las plazas anteriores si el propietario lo solicita expresamente.
- Producto Wallet Studio creado como **borrador**, ID `10459984298313`, en la tienda correcta `store-09240616prrz2-itsdpkjq.myshopify.com`. Sigue sin publicar; precio pendiente. Otros productos no conceden licencia.
- El propietario exige alojamiento gratuito. `render.yaml` usa Render Free con PostgreSQL y Storage privados de Supabase Free, y Brevo Free para códigos por HTTPS. Se descarta el disco de pago de Render. **No contratar pagos, ni confundir archivos de despliegue con servicio online.** Consultar cuotas/pausas en `server/RENDER.md`. Los originales nunca deben depender del disco efímero de Render.
- Antes de distribuir: alojar backend con disco persistente, configurar SMTP y suscripciones de Shopify, probar compra/activación/devolución reales y compilar con la URL oficial fija. El binario sin URL cierra el acceso; conservar la versión 0.6.0 para trabajo local mientras se completa la infraestructura. No habilitar todavía publicación automática ni reemplazar instalaciones del cliente.
- Los documentos de despliegue de cuentas y Render prevalecen sobre las instrucciones históricas de alta administrativa o acceso sin conexión de 0.6.0 que se conservan debajo.

## Historial: versión 0.6.0

Repositorio: https://github.com/drp48-ua/wallet-studio-build
Rama: `codex/customer-ready-storage-updates-ui-2026-10-14`. Parte de la corrección 0.5.4. No se ha sobrescrito main ni el trabajo del colaborador.

## Estado del proyecto

- Originales inmutables vinculados al iPhone y referencia exacta de tarjeta; almacenamiento fuera del directorio del ejecutable y migración sin reemplazos. Alias existentes se reutilizan. Conflictos conocidos se conservan y bloquean solo su tarjeta; identidad desconocida o raíz insegura bloquea el almacén.
- Captura automática tras seleccionar tarjeta, con prueba de procedencia de cada recurso presente. Un diseño personalizado no se convierte en original. La captura antigua sin procedencia se ha cerrado; originales antiguos completos siguen disponibles localmente.
- Nube preparada: servicio Python/SQLite con cuentas administradas, objetos inmutables aislados por propietario, sesiones protegidas con DPAPI en Windows y vinculación de cuenta/servidor del almacén. Sincronización al iniciar, tras operaciones y cada 60 s en reposo. El caché comprueba los archivos locales actuales; vuelve a importar los ausentes y conserva/confina conflictos.
- Actualización preparada: workflow de main prueba, compila, empaqueta y firma manifiestos RSA-PSS/SHA-256. Descarga en segundo plano, comprobación de firma, hashes, rutas y versión; activación del paquete preparado en la siguiente apertura. Binarios y originales separados. Las versiones antiguas necesitan instalación manual inicial.
- Nuevo logo del propietario convertido sin cambiar sus píxeles a PNG e iconos; metadata externa eliminada. Paleta grafito/plata con cian/lila. Galería abierta de todas las tarjetas con último diseño local verificado y original pequeño. Vistas de editor alineadas, acciones fijas de 44 px y renderizado sin I/O de originales por frame. Las referencias visuales de otro iPhone no autorizan transferencias.

## Validación

Las pruebas usan exclusivamente cuentas, imágenes, teléfonos simulados y carpetas temporales sintéticas. El servidor tiene siete pruebas. La suite Windows excluye únicamente cuatro pruebas de hardware/catálogo privado. La integración adicional ejecuta el cliente real WinHTTP/DPAPI contra el servicio local: dos cuentas, dos perfiles, importación inmutable, recuperación de archivo perdido, conflicto sin sobrescritura y rechazo del cambio de propietario.

La compilación final 0.6.0 para Windows se completó el 15/10/2026. La revisión nativa en demo aislada comprobó el nuevo logo, paleta, galería abierta de dos/tres columnas, selección, desplazamiento, originales pequeños sin marco y acciones fijas a 960/1180 px y maximizada. La vista vacía muestra instrucciones y mantiene las transferencias bloqueadas. No afirmar que los tests sintéticos equivalen a una prueba física de transferencias. No se realizaron escrituras al iPhone durante este trabajo.

### Entrega local de prueba, 15/10/2026

- ZIP Windows 0.6.0: 684 entradas, 6.568.355 bytes. CRC de todos los archivos y coincidencia exacta del ejecutable comprobados. No contiene originales, sesiones, diarios ni claves privadas.
- SHA-256 ZIP: `1dc93973b8a9853db459ef3bb7f4c9b6c944543444eede9acba01f51571cf5dd`.
- SHA-256 EXE: `6558f7dae5c0c0ea30c25af4b222aa2cde664081653dc5b660a58253bc28a913`.
- Recurso Windows FileVersion/ProductVersion 0.6.0. Sin firma Authenticode; compilación local de prueba, no publicación comercial general.
- El servicio de nube y el canal de actualización todavía necesitan despliegue/configuración según CLIENTES-Y-NUBE.md. Main del colaborador se conserva. Las versiones antiguas requieren primera instalación manual.

### Checkpoint por cuota, 14/10/2026

- Suite completa: 130 tests unitarios y 1 de motor PASARON; 4 de USB/catálogo privado excluidos y E2E ignorado para ejecución independiente. Siete tests del servidor PASARON.
- E2E anterior PASÓ, pero el E2E ampliado detectó una ruta >260 caracteres al guardar receipts de nube con MoveFileExW. storage::wide_path corrige DOS/UNC con UTF-16 exacto y se reutiliza en el updater. Nueva prueba de ruta larga añadida.
- Se corrigió WINHTTP_DISABLE_REDIRECTS a 0x2 (0x8 desactiva keep-alive). E2E incluye una redirección controlada que debe rechazarse sin reenviar la cabecera de sesión.
- Último rerun E2E PASÓ tras la corrección de rutas largas: recuperación de archivo perdido, conservación/bloqueo del archivo alterado, separación de cuentas y rechazo de redirección con sesión. Registro local `work/wallet-0.6.0-final-cloud-test.log`, fuera del repositorio.
- Rerun estrecho customer_: 15 pruebas PASARON, incluida ruta >260, autenticación obligatoria antes de capturar original ausente, receipts/historial y separación de conflictos. E2E ignorado en ese rerun porque se ejecutó por separado. Registro local `work/wallet-0.6.0-final-customer-tests.log`.
- Este checkpoint histórico quedó superado por la compilación, revisión visual y ZIP verificado del 15/10/2026, descritos arriba.
- Demo mediante flag --wallet-studio-ui-demo o copias de binario con nombres cortos WSDEMO96.exe, WSDEMO11.exe, WSDEMOEM.exe. Nombres largos pueden llegar abreviados por el lanzador Windows. Demo tiene botón Vista vacía/Cargar ejemplos y nunca consulta USB/nube/updates. El usuario ha autorizado dejar la ventana libre para revisar.

## Antes de distribuir a clientes

1. Revisar/integrar la rama conservando main del colaborador. No usar force-push.
2. Desplegar el servicio detrás de HTTPS, volumen privado cifrado y backup externo. No hay servidor online configurado todavía. Consultar CLIENTES-Y-NUBE.md.
3. Crear cuentas de clientes administrativamente; las altas automáticas por compras de Shopify siguen pendientes.
4. Configurar la URL pública como variable WALLET_STUDIO_CLOUD_URL y la clave privada del editor como secreto WALLET_STUDIO_UPDATE_SIGNING_KEY. La clave privada se conserva fuera del repositorio y nunca va en el ZIP de clientes; hacer backup privado cifrado antes de cambiar de PC.
5. Validar en equipos Windows limpios y con el teléfono antes de habilitar WALLET_STUDIO_RELEASE_APPROVED=true. Configurar Authenticode y protección de main. El workflow permanece gated mientras no se habilite.
6. Portar y validar macOS. Los iconos de marca están preparados; no hay binario Mac probado de esta versión.
7. La nube actual admite originales bancarios e historial completo validado; respaldos de pases legacy se conservan localmente, pero su sincronización remota sigue pendiente. No se ofrece cifrado de extremo a extremo.
8. GIF en el editor y extracción de fotogramas funcionan; la prueba física anterior de Apple Pay mostró imagen fija. No anunciar animación funcional en el teléfono.

## Privacidad y cuota

Nunca subir AppData, originales, diarios personales, manifiestos bancarios, identificadores, sesiones, tokens, claves privadas ni datos del servidor a GitHub. La clave pública y fixtures sintéticas sí pertenecen al código. Consultar cuota al inicio y en checkpoints; ≤3 % guardar, ≤1 % detener investigación/implementación/tests y cerrar. No consumir crédito de reinicio sin autorización.
