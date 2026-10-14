# Cuentas y copias privadas

El servidor en `server/` conserva paquetes privados inmutables, separados por cuenta. La app consulta las copias de la cuenta antes de plantear una nueva captura. Instalar una versión nueva no permite reconstruir una copia histórica que nunca se guardó o sincronizó.

## Desplegar

1. En un servidor con Docker, ejecuta `docker compose -f server/compose.yaml up -d --build`.
2. Publica el servicio detrás de un proxy HTTPS con certificado válido. El puerto 8766 solo está expuesto en localhost. Configura límites de tamaño de petición, tiempos de espera y limitación de intentos de acceso en el proxy.
3. Usa un volumen cifrado para `/data`, copias externas cifradas y acceso administrativo restringido. El servicio autentica usuarios y separa objetos por propietario; no ofrece cifrado de extremo a extremo. Nunca publiques ese volumen en GitHub.
4. Crea una cuenta para cada cliente con `docker compose -f server/compose.yaml exec wallet-cloud python app.py --data /data --create-user cliente@example.com`. La contraseña se introduce interactivamente y se guarda como PBKDF2 con sal. El registro público está desactivado por defecto.
5. Define la variable de repositorio `WALLET_STUDIO_CLOUD_URL` con la URL HTTPS del servicio antes de compilar la versión para clientes. El cliente recibe esa dirección integrada y solo introduce correo y contraseña. Para pruebas o administración, existe configuración avanzada del endpoint. La primera vinculación requiere confirmar que todas las copias locales pertenecen a esa cuenta. El almacén queda vinculado a servidor y propietario; se rechaza cambiarlo a otra cuenta para evitar enviar copias ajenas.

No hay servidor desplegado ni cuentas de clientes reales configuradas por este cambio. Las pruebas usan cuentas y archivos sintéticos locales. La integración con compras de Shopify y el alta automática de compradores requiere un flujo posterior de provisión de cuentas.

## Conservación

Una misma copia se identifica por dispositivo y referencia literal de tarjeta. No se sustituye al cambiar nombres ni diseños. Los conflictos con identidad conocida conservan evidencia y bloquean esa tarjeta; una identidad ilegible o raíz insegura bloquea el almacén. Las recuperaciones inciertas no se consideran originales completos ni se suben como si estuvieran resueltas. La sincronización remota de esta versión admite baselines bancarios e historial completo validado; los respaldos antiguos de pases migran localmente, pero aún no se sincronizan por la nube.

El caché de sincronización comprueba los archivos locales actuales, no solo que se descargaron alguna vez. Si falta un archivo, vuelve a consultar e importar su copia histórica desde la cuenta; si existe contenido distinto, conserva ambas evidencias y bloquea esa tarjeta para revisión en lugar de sobrescribirla.

Las versiones anteriores sin este mecanismo necesitan una instalación manual inicial. No se puede añadir sincronización o actualización automática a un ejecutable antiguo de forma retroactiva. La base macOS sigue pendiente de portar y validar; este cambio de escritorio se desarrolla para Windows.

## Actualizaciones

El workflow `desktop-release.yml` prueba y compila cada cambio de `main`, publica un paquete y un manifiesto en GitHub Releases y asigna un número de build. El cliente consulta ese canal en segundo plano, valida repositorio, versión, tamaño, SHA-256 y rutas del archivo ZIP y prepara la actualización fuera de las copias privadas. Se activa al volver a abrir la app.

Además de HTTPS, cada manifiesto se verifica con RSA-PSS/SHA-256 y una clave pública integrada. Se rechazan firmas ajenas, modificaciones de un build publicado, retrocesos de versión, rutas peligrosas, enlaces y datos privados en el paquete. Esto no equivale a una firma Authenticode del fabricante. Antes de una distribución comercial general deben configurarse firma de código, protección de `main`, recuperación del servicio y validación de funcionamiento en equipos limpios. Mientras este workflow esté solo en una rama de propuesta, no publica versiones de `main` ni activa ese canal.

### Activar el canal de publicación

1. Revisa e integra esta rama en `main` conservando los cambios de colaboradores.
2. El propietario debe guardar la clave PEM privada del editor como secreto de GitHub `WALLET_STUDIO_UPDATE_SIGNING_KEY`. No se incluye en el repositorio ni en el ZIP de clientes. Conserva una copia privada cifrada: las apps confiarán en su clave pública integrada. `scripts/sign-update.ps1` rechaza claves diferentes.
3. Configura `WALLET_STUDIO_CLOUD_URL` y, después de validar el paquete para distribución, la variable `WALLET_STUDIO_RELEASE_APPROVED=true`.
4. Cada push de `main` probado genera una publicación firmada. Una app con este mecanismo la descarga en segundo plano y la usa en la siguiente apertura. Sin red sigue funcionando con sus originales locales ya verificados. Un fallo de sincronización impide capturar como original una imagen nueva cuando podría existir una copia histórica en la cuenta.

La clave privada generada para esta propuesta permanece fuera del repositorio, en el PC de desarrollo. No se ha configurado ningún secreto remoto ni se ha habilitado la publicación automática. La firma Authenticode y la versión para macOS siguen pendientes.
