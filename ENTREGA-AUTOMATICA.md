# Publicación y entrega automática

## Estado confirmado

Shopify entrega Wallet Studio 0.6.27 desde el único producto existente, por 6,99 EUR. La sustitución y la descripción se verificaron mediante lectura del conector. No se enviaron correos a clientes ni se crearon pedidos.

Digital Products solo admite enlaces de determinados proveedores; GitHub no está incluido. Se comprobó en la interfaz y en https://help.shopify.com/en/manual/products/digital-service-product/digital-downloads. Un ZIP subido a Shopify no cambia cuando cambia el archivo de GitHub.

## Implementación preparada en esta rama

El flujo de main comprueba las suites, compila, firma y prepara un alias Wallet-Studio-Windows.zip byteidéntico al paquete versionado. Comprueba firma RSA-PSS, SHA256 y tamaño. Los assets se verifican en un borrador antes de promoverlo a estable. No se promueve un main desactualizado ni se retrocede el número de compilación estable.

URL prevista, que todavía necesita publicación del nuevo flujo:

https://github.com/drp48-ua/wallet-studio-build/releases/latest/download/Wallet-Studio-Windows.zip

Después de publicar, el paso Drive puede reemplazar los bytes de un único archivo existente, conservando su ID, nombre y permisos. Utiliza una cuenta técnica sin roles de Google Cloud ni delegación de dominio, con permiso de escritura otorgado únicamente al ZIP. El alcance OAuth Drive se limita en la práctica mediante esa ACL; no se le comparten otros archivos privados. El script no crea ni elimina archivos, ni envía correos.

Configuración necesaria en GitHub Actions:

- Variable WALLET_STUDIO_DRIVE_SYNC_ENABLED=true, solo después de configurar y revisar el acceso.
- Variable WALLET_STUDIO_DRIVE_FILE_ID: ID del único archivo autorizado.
- Variable WALLET_STUDIO_DRIVE_INITIAL_SHA256: hash del ZIP revisado antes de su primera sincronización.
- Secreto WALLET_STUDIO_DRIVE_SERVICE_ACCOUNT_JSON: credencial de esa cuenta técnica. Nunca guardar su contenido en Git ni registros.

El hash inicial del paquete público 0.6.27 es 8530bca9916e0cc8e3591d20122d1d69ef969ce15848e19db0cc00e973ade783. Su tamaño es 9.098.672 bytes. Existe una copia privada preparada en Drive; no se ha hecho pública ni se ha compartido con una cuenta técnica.

La sincronización exige firma, vinculación con repositorio/main/commit/build, canal estable actual, condición ETag/version antes de escribir y comprobación del contenido posterior. En la primera inscripción exige el SHA256 previamente autorizado. En operaciones posteriores usa metadatos privados de la aplicación para impedir retrocesos. Si no se obtienen las condiciones esperadas, falla sin declarar éxito. Si falla después de publicar en GitHub, la entrega requiere revisión; no asumir que el archivo Drive ya contiene la publicación nueva.

## Validación y límites

11 pruebas sintéticas de alias, 13 pruebas con HTTP y credenciales ficticios, y 7 guardas del workflow pasaron. El responsable volvió a ejecutar las 11 y las 13; además preparó el alias con la firma y el ZIP públicos reales de 0.6.27. No se han usado credenciales privadas reales de Drive ni se ha ejecutado una actualización remota automática. El comportamiento real de ETag, sesión de carga y lectura posterior sigue pendiente de aceptación contra Google Drive.

El usuario autorizó crear una cuenta técnica limitada al ZIP, guardar su clave como secreto de GitHub y dar lectura pública al ZIP para los compradores. Tras el bloqueo inicial de la revisión automática, autorizó expresamente crear el proyecto aislado «Wallet Studio Releases» sin asociar facturación. El proyecto ya se creó y se seleccionó; no se vinculó facturación ni se inició una prueba de pago. También se creó la cuenta técnica sin asignarle roles opcionales de Cloud ni delegación. El conector de Drive confirmó permiso de escritura para esa cuenta únicamente sobre el ZIP preparado; no se le compartieron carpetas u otros archivos.

Google confirmó la creación de una clave y la tabla, leída de nuevo, muestra una clave activa. Sin embargo, el navegador no entregó su descarga al entorno de herramientas y el archivo con el nombre exacto observado no se encontró en las ubicaciones locales comprobadas. No se volvió a generar otra clave. Ningún contenido privado se imprimió ni se subió al repositorio. El secreto de Actions no está configurado y la sincronización continúa desactivada. Antes de activar nada se debe recuperar esa descarga o sustituir de forma controlada la clave no recuperada, conservando el mismo alcance. Los identificadores y el estado exacto se guardan solo en la continuación local excluida de Git.

## Continuación

1. Proyecto y cuenta ya creados bajo autorización expresa; no repetir su creación ni utilizar proyectos ajenos a Wallet Studio.
2. Resolver la descarga de la única clave creada o su sustitución controlada. Mantenerla fuera de Git y de la salida de herramientas; no crear claves adicionales a ciegas. Verificar que Drive API está habilitada en el proyecto, sin asociar facturación.
3. Configurar las variables y el secreto de Actions. Hacer público únicamente el paquete distribuible, que ya es público en GitHub, nunca copias del teléfono, cuentas o diseños privados.
4. Revisar e incorporar el cambio mediante PR, conservando los cambios de colaboradores y sin force-push. Comprobar publicación y sincronización reales.
5. Añadir a Digital Products el enlace permanente de Drive con nombre y botón de descarga claros, manteniendo entrega automática y acceso ilimitado. No retirar la descarga actual hasta validar el reemplazo y tener la autorización necesaria para retirarla.
6. Verificar tamaño y hashes del archivo realmente entregado. No afirmar que una compra real o un correo externo fueron probados si no lo fueron.

No hay automatización de navegador ni tarea periódica: el disparador será cada publicación válida de main. Una compilación o firma fallida conserva la publicación anterior; una sincronización fallida debe quedar visible como error.
