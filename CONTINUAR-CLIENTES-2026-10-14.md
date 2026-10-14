# Wallet Studio 0.6.0: continuación y publicación

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

La revisión visual anterior comprobó la app nativa en demo aislada a 960 y 1180 px, desplazamiento, originales pequeños sin marco y vistas alineadas. El nuevo logo y la galería abierta añadidos después todavía requieren revisión visual de la compilación final; no se ha aprobado una entrega 0.6.0. No afirmar que los tests sintéticos equivalen a una prueba física de transferencias. No se realizaron escrituras al iPhone durante este trabajo.

### Checkpoint por cuota, 14/10/2026

- Suite completa: 130 tests unitarios y 1 de motor PASARON; 4 de USB/catálogo privado excluidos y E2E ignorado para ejecución independiente. Siete tests del servidor PASARON.
- E2E anterior PASÓ, pero el E2E ampliado detectó una ruta >260 caracteres al guardar receipts de nube con MoveFileExW. storage::wide_path corrige DOS/UNC con UTF-16 exacto y se reutiliza en el updater. Nueva prueba de ruta larga añadida.
- Se corrigió WINHTTP_DISABLE_REDIRECTS a 0x2 (0x8 desactiva keep-alive). E2E incluye una redirección controlada que debe rechazarse sin reenviar la cabecera de sesión.
- Último rerun E2E en curso al preparar checkpoint: consultar `work/wallet-0.6.0-final-cloud-test.log` fuera del repositorio. No marcar pasado sin comprobar resultado. También quedan por ejecutar los tests `customer_atomic_metadata_replace_supports_long_windows_paths` y `customer_missing_original_waits_for_auth_when_service_or_owner_is_known`.
- Código/recursos finales congelados; falta compilar el ejecutable con recurso 0.6.0, revisar logo/galería/vista vacía y generar/verificar el ZIP. No entregar un ejecutable anterior como esta versión.
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
