# Cuentas y copias privadas

El servidor en `server/` conserva paquetes privados inmutables, separados por cuenta. La app consulta las copias de la cuenta antes de plantear una nueva captura. Instalar una versión nueva no permite reconstruir una copia histórica que nunca se guardó o sincronizó.

## Desplegar

1. En un servidor con Docker, ejecuta `docker compose -f server/compose.yaml up -d --build`.
2. Publica el servicio detrás de un proxy HTTPS con certificado válido. El puerto 8766 solo está expuesto en localhost. Configura límites de tamaño de petición, tiempos de espera y limitación de intentos de acceso en el proxy.
3. Usa un volumen cifrado para `/data`, copias externas cifradas y acceso administrativo restringido. El servicio autentica usuarios y separa objetos por propietario; no ofrece cifrado de extremo a extremo. Nunca publiques ese volumen en GitHub.
4. Crea una cuenta para cada cliente con `docker compose -f server/compose.yaml exec wallet-cloud python app.py --data /data --create-user cliente@example.com`. La contraseña se introduce interactivamente y se guarda como PBKDF2 con sal. El registro público está desactivado por defecto.
5. Configura en la app la URL HTTPS del servicio e inicia sesión. Vincula únicamente las copias que pertenecen a esa cuenta.

No hay servidor desplegado ni cuentas de clientes reales configuradas por este cambio. Las pruebas usan cuentas y archivos sintéticos locales. La integración con compras de Shopify y el alta automática de compradores requiere un flujo posterior de provisión de cuentas.

## Conservación

Una misma copia se identifica por dispositivo y referencia literal de tarjeta. No se sustituye al cambiar nombres ni diseños. Los conflictos conservan evidencia y bloquean escrituras. Las recuperaciones inciertas no se consideran originales completos ni se suben como si estuvieran resueltas.

Las versiones anteriores sin este mecanismo necesitan una instalación manual inicial. No se puede añadir sincronización o actualización automática a un ejecutable antiguo de forma retroactiva. La base macOS sigue pendiente de portar y validar; este cambio de escritorio se desarrolla para Windows.

## Actualizaciones

El workflow `desktop-release.yml` prueba y compila cada cambio de `main`, publica un paquete y un manifiesto en GitHub Releases y asigna un número de build. El cliente consulta ese canal en segundo plano, valida repositorio, versión, tamaño, SHA-256 y rutas del archivo ZIP y prepara la actualización fuera de las copias privadas. Se activa al volver a abrir la app.

El canal confía en la cuenta y repositorio de GitHub a través de HTTPS. No equivale a una firma Authenticode del fabricante. Antes de una distribución comercial general deben configurarse firma de código, protección de `main`, recuperación del servicio y validación de funcionamiento en equipos limpios. Mientras este workflow esté solo en una rama de propuesta, no publica versiones de `main` ni activa ese canal.
