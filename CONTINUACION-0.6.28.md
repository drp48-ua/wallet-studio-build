# Continuación Wallet Studio 0.6.28

## Resultado real comprobado

Se distinguieron dos fallos: el marcador antiguo de importación local bloqueaba todas las opciones de cuenta; Render seguía conectado a otro proyecto Supabase y mostraba errores de conexión. El usuario reanudó el proyecto correcto y pidió migrar Render a una cuenta nueva por sus problemas de pagos.

Se creó un servicio gratuito Docker en la nueva cuenta: https://wallet-studio-accounts-v2.onrender.com. Render confirmó Deploy succeeded / Live sobre main 7ac9c66. /health devuelve 200. Usa el Supabase reanudado y Cloudflare R2 privados ya migrados. Se conservaron las claves de cuentas, correo y Shopify existentes. No se guardaron cambios en la configuración antigua ni se borró su servicio. El plan seleccionado fue 0 USD; no se contrató un plan de pago.

Preflight de solo lectura: conexión TLS verificada al Supabase correcto; dos depósitos R2 privados; 15 copias indexadas descargadas y verificadas por SHA256/tamaño; presencia de los 112 objetos de diseños. Una consulta autenticada al servidor nuevo con la sesión existente devolvió 200, identidad coincidente y licencia vigente. No se creó una cuenta ni otra identidad de PC.

Shopify: los cuatro webhooks existentes (pago, actualización, cancelación y reembolso de pedido) se editaron para usar https://wallet-studio-accounts-v2.onrender.com/v1/shopify/webhook. Se preservaron eventos, formato y secreto; se verificaron las cuatro URLs en pantalla. No se enviaron pruebas de pedido, pagos o correos a clientes. El ZIP que Shopify entrega sigue siendo 0.6.27; todavía no se sustituyó por el candidato 0.6.28.

## Corrección local

Cuando el almacén de originales requiere revisión, se permite autenticar únicamente al propietario existente con la identidad DPAPI del mismo PC. No se adopta un propietario ni se registra otro equipo. La sesión y la caché de biblioteca se guardan en una carpeta hermana separada. No se importan copias, se sincroniza el almacén ni se habilitan capturas o transferencias del iPhone. Si no puede verificarse propietario/PC, finalizar el acceso sigue bloqueado; solicitar un código o recuperar acceso permanece disponible.

El transporte de cuentas cambia por configuración compilada a la nueva instancia, conservando el servicio canónico original en Session.server, VaultOwner y claves de caché. Esto evita romper la identidad de las copias. El transporte solo admite rutas /v1 del servicio canónico y un destino HTTPS estricto. El actualizador público de GitHub sigue separado.

Compilar con WALLET_STUDIO_CLOUD_URL=https://wallet-studio-accounts.onrender.com y WALLET_STUDIO_CLOUD_TRANSPORT_URL=https://wallet-studio-accounts-v2.onrender.com. Ambos workflows llevan el transporte nuevo; no cambiar el propietario de los perfiles existentes ni la variable canónica del proveedor para intentar resolver la migración.

En este PC se verificó primero la sesión existente contra el servidor nuevo y se preparó su copia DPAPI en la carpeta de cuenta separada. El archivo original no cambió. No se retiró .cloud-import.lock ni se alteraron originales, cuarentenas o diarios. La 0.6.28 real abrió correctamente la cuenta en modo biblioteca y cargó el catálogo de 56 diseños. Se revisó biblioteca en ventana y cuenta a pantalla completa. Sincronizar aparece deshabilitado y el aviso de revisión sigue visible. No se probó ni modificó ningún iPhone.

## Pruebas y paquete

Worker: compilación GNU y 95 pruebas dirigidas correctas, 7 ignoradas. Transporte: 63 pruebas únicas correctas, 7 ignoradas; último endurecimiento de localhost también probado con extracción exacta de las funciones finales. Los recuentos se solapan.

Lead: suite completa previa al transporte, 254 correctas y dos fallos HTTP por restricciones de conexión local; las 20 pruebas de auto_update, incluidas las dos fallidas, pasaron al permitir su servidor localhost. No presentar esa ejecución inicial como cero fallos. El paquete final GNU con el transporte nuevo compiló correctamente (3m14s) y se probó en la app real. La suite completa sobre la fuente final y Windows/MSVC/CI todavía requieren aceptación.

Paquete local dist/Wallet-Studio-0.6.28-Windows.zip, SHA256 c849afd2ed29d9fa70cc88629f71d10e2fc64a62df5df925e242772b4c7b70e3. EXE dist/candidate-0.6.28/Wallet Studio.exe, SHA256 57cf6a4fa00a3cd16db724a01eecf2f26be698ce67d51f45f2065dd41f3b6d14. Es un candidato GNU local, no una publicación MSVC firmada del canal.

## Pendientes antes de declarar terminado

1. Revisar la aceptación final del cliente HTTP sintético: el nuevo enrutamiento compilado podría impedir el localhost de la integración. Mantener cualquier excepción exclusivamente en pruebas explícitas; jamás relajar el transporte productivo.
2. Ejecutar la suite final y CI Windows/MSVC. Incorporar a main únicamente tras revisión y comprobaciones. La rama conserva trabajo previo de entrega automática de Drive; permanece desactivada y no está resuelta la descarga de su clave.
3. Publicar y verificar la 0.6.28 firmada; confirmar actualización desde clientes anteriores y sustituir la entrega de Shopify cuando esté validada. No afirmar que esos pasos ya ocurrieron.
4. Resolver de forma revisable la importación antigua; la biblioteca ya funciona, pero el iPhone sigue bloqueado. No retirar el marcador con evidencia indeterminada ni inventar originales.
5. Windows limpio sin iTunes sigue aplazado por instrucción del usuario. No se certificó compra real/correo externo ni compatibilidad universal.

Credenciales, copias, registros privados y scripts de operación del host permanecen excluidos de Git. No se usó un crédito de reinicio de cuota.
