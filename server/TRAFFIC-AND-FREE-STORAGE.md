# Tráfico de diseños y alternativas de almacenamiento

Revisión del 26 de octubre de 2026. El objetivo es reducir transferencias manteniendo exactamente los PNG originales y sus dimensiones. No se han recomprimido ni sustituido imágenes ni copias bancarias.

## Qué cabe y qué se agotó

La captura del usuario mostraba 0,30 GB de archivos sobre 1 GB disponible. Ese almacenamiento ya cabía. El correo de Supabase notificó 25,42 GB de Cached Egress frente a 5,5 GB de cuota efectiva del ciclo y restricciones hasta el 14 de noviembre de 2026. Egress y Cached Egress tienen contadores independientes. Reducir tráfico futuro no descuenta transferencias ya servidas ni levanta por sí mismo la restricción. [Supabase: egress](https://supabase.com/docs/guides/platform/manage-your-usage/egress).

El catálogo versionado contiene 56 originales, 136,61 MiB en total, y 7,89 MiB en la variante de previsualización antigua. Son tamaños de archivos del catálogo, no una medición del tráfico real. Una descarga completa de ambas variantes sumaría unos 144,50 MiB, pero la app no necesita descargar todos los originales para mostrar la galería. Sin registros del proveedor no se atribuyen los 25,42 GB completos a esta app ni se promete un porcentaje mensual de ahorro.

## Cambios verificados

La app Windows 0.6.23 ya comprueba el permiso con metadatos frescos y reutiliza un PNG local íntegro, en lugar de descargar de nuevo la imagen al usarla. Conserva la verificación de SHA-256, tamaño y dimensiones y rechaza permisos retirados y metadatos cambiados.

El parche adicional de servidor introduce una caché RAM por proceso para PNG verificados: máximo 64 MiB y 128 entradas, con expulsión LRU. La clave vincula origen, ruta del objeto, hash, bytes y dimensiones. La ruta HTTP sigue comprobando sesión, licencia y catálogo vigente antes de cada lectura. Cada acierto vuelve a verificar el PNG; las solicitudes simultáneas del mismo archivo comparten la descarga inicial. No se guardan fallos, secretos, sesiones ni archivos bancarios en esta caché.

El test de una segunda petición caliente del mismo diseño comprobó cero GET de bucket y cero GET de imagen a Supabase, con los bytes, SHA y dimensiones originales intactos. También se verificó rechazo de tokens inválidos o revocados, permiso retirado e ID eliminado, incluso con caché caliente. Suite Python: 83 pruebas en total, cero fallos y tres omitidas; foco de caché: seis pruebas correctas. No se realizaron llamadas al proveedor real.

Límites: los 64 MiB no contienen todo el catálogo de 136,61 MiB. El ahorro depende de los diseños reutilizados, las expulsiones y los reinicios del proceso. La caché se vacía al reiniciar. No garantiza mantenerse bajo 5 GB con cualquier número de clientes. Tampoco reduce el ancho de banda de Render hacia sus clientes cuando Render continúa sirviendo el PNG.

## Opciones revisadas

| Opción | Tramo sin coste relevante | Condiciones y encaje |
| --- | --- | --- |
| Supabase Free | Cuotas independientes de 5 GB de egress y 5 GB de cached egress al mes | Conservar el proveedor y añadir ambas cachés es el cambio menor. La cuota ya agotada se restablece al renovar el ciclo o ampliar el plan. |
| Cloudflare R2 Standard | 10 GB-mes de almacenamiento; 1 millón de operaciones A y 10 millones B al mes; transferencia de salida sin cargo de egress | Es la recomendación para los archivos si se mantiene este volumen de descargas. Requiere una cuenta y activar una suscripción R2 mediante checkout. Hay facturación al superar almacenamiento u operaciones incluidas. |
| Firebase Storage / Google Cloud Storage | En regiones estadounidenses elegibles: 5 GB-mes de almacenamiento, 100 GB de transferencia desde Norteamérica a destinos admitidos, 5.000 operaciones A y 50.000 B al mes | Storage exige Blaze con cuenta de facturación, aunque puede quedar sin coste dentro del tramo elegible. Tiene condiciones regionales y cobra excesos. No es Storage gratuito en Spark sin facturación. |

Fuentes oficiales: [R2: precios y tramo gratuito](https://developers.cloudflare.com/r2/pricing/), [R2: activación](https://developers.cloudflare.com/r2/get-started/), [Firebase: requisito Blaze](https://firebase.google.com/docs/storage/faqs-storage-changes-announced-sept-2024), [Google Cloud Storage: Always Free](https://cloud.google.com/storage/pricing#cloud-storage-always-free). Las cuotas deben volver a comprobarse antes de contratar o migrar; son condiciones consultadas en esta revisión, no una garantía de coste cero para cualquier uso.

## Recomendación y migración pendiente

Preferir R2 Standard para imágenes y, si se aprueba por separado, los objetos privados de copia. El catálogo actual ocupa una pequeña parte de sus 10 GB incluidos. R2 sustituye el almacenamiento de objetos; no sustituye las cuentas ni la base PostgreSQL del servicio. Hay que comprobar por separado que la base de datos y la autenticación actuales sigan disponibles.

Si también hay que sustituir PostgreSQL, Neon Free es una opción compatible para evaluar: publica 1 GB de base de datos por proyecto y 100 CU-horas mensuales, sin tarjeta para el plan gratuito. La base mostrada por el usuario era de 27 MB, por lo que el tamaño cabría; el cómputo y la transferencia necesitan su propio presupuesto. No significa disponibilidad ilimitada. Sería una migración separada de tablas, cuentas y sesiones, conservando identidades y secretos del servicio y comprobando TLS, integridad y permisos. No se ejecuta en este parche. [Neon: plan Free](https://neon.com/pricing).

Para evitar trasladar el coste de imágenes a Render, el diseño objetivo es autorizar en el servidor y emitir un acceso firmado de corta duración a un único objeto privado de R2. El cliente descarga directamente desde R2 y exige el mismo tamaño/hash/dimensiones; las credenciales permanecen en el servidor. Una URL firmada es un permiso temporal y no se debe registrar ni reutilizar como identidad estable de caché. [R2: accesos firmados](https://developers.cloudflare.com/r2/api/s3/presigned-urls/). Las respuestas con imágenes servidas por Render siguen contando como ancho de banda de Render. [Render: outbound bandwidth](https://render.com/docs/outbound-bandwidth).

La migración requeriría activar la cuenta del proveedor elegido, configurar acceso privado solo en el servidor, adaptar la capa de objetos y transferencias, copiar sin sobrescritura y verificar todos los objetos por hash/tamaño. Los originales del proveedor anterior se conservarían hasta comprobar la migración y el retorno seguro. Los diseños pueden partir de los archivos originales locales verificados; las copias bancarias exigen su propio inventario y autorización de destino. No se han creado cuentas, claves, buckets, enlaces públicos ni migraciones de datos en esta evaluación.

## Despliegue de la mejora actual

El parche modifica únicamente designs.py y sus pruebas. El Dockerfile existente ya copia designs.py, y no hay dependencias nuevas. Render tiene autoDeployTrigger desactivado; subir esta rama no activa el parche del servidor. Está preparado y probado para un despliegue posterior, pero no se anuncia ahorro real en producción ni restablecimiento de Supabase sin realizar y comprobar ese despliegue. La app 0.6.23 puede mantenerse; no necesita otro ejecutable para beneficiarse de esta caché del servidor.
