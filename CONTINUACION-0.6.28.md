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


## Reanudación del 9 de noviembre: sesión ausente y diagnóstico acotado

La carpeta hermana WalletStudio-AccountRecovery no estaba presente al retomar. El original cloud-session.dpapi y el marcador vacío .cloud-import.lock seguían conservados. No se conoce qué retiró la carpeta separada; no atribuirlo a expiración o a una acción del usuario sin evidencia. La consulta autenticada de solo lectura al servidor nuevo volvió a devolver 200, propietario coincidente y licencia vigente. Wallet Studio estaba cerrado.

Se prepara una acción explícita «Recuperar mi sesión guardada» que comprueba la sesión original, propietario y equipo contra el servidor antes de guardar acceso solo a biblioteca. No hay restauración silenciosa al iniciar. Un cierre de sesión o un acceso más reciente de otra instancia deben ganar frente a una recuperación concurrente. Esta acción no resuelve por sí sola el bloqueo del almacén ni habilita el iPhone.

El diagnóstico anterior usaba un límite global de 1 GiB y agrupaba el agotamiento de lectura con evidencia insegura. El inventario estructural de solo lectura encontró 34 carpetas de preparación, 614 archivos y 586.741.912 bytes, sin enlaces, tamaños de archivo excesivos ni nombres inesperados. Esto no demuestra integridad ni autoriza retirar el marcador. El diagnóstico nativo debe clasificar todas las carpetas por separado y distinguir límite, integridad e identidad antes de proponer cualquier recuperación.

Los resultados finales de compilación, pruebas y recuperación aún deben añadirse. No se retiró el marcador ni se modificaron originales o datos del iPhone durante este diagnóstico.

### Aceptación de la corrección de acceso del 9 de noviembre

La fuente final de esta fase compiló con las dos direcciones oficiales. Suite nativa GNU: 264 correctas, cero fallos, 11 ignoradas y cuatro pruebas físicas excluidas; integración con dos perfiles ficticios y servidor real local: una correcta. Pruebas dirigidas: 82 correctas, con recuentos solapados. La excepción de conexión local está exclusivamente en cfg(test), requiere dos opciones explícitas y una dirección loopback/puerto exactos; el binario de clientes conserva la dirección oficial compilada.

Se protegen cambios concurrentes de sesión, propietario y equipo durante la recuperación. Un cierre de sesión en modo normal limpia también la sesión separada mediante bloqueos secuenciales; los rechazos solo eliminan el token concreto rechazado. La sesión original de un almacén bloqueado se conserva.

La auditoría nativa completa separó 32 baselines válidos y dos de procedencia no acreditada; 33 historiales válidos y uno inválido. Hubo 18 vínculos con ZIP y 15 reconstrucciones cuyo ZIP coincide exactamente; tres preparaciones siguen sin ese vínculo. Comparación: 410 archivos exactos, 200 ausentes y cuatro diferentes, correspondientes a dos baseline.json y dos PNG. Se conservaron todos los archivos.

El censo independiente encontró ocho identidades distintas y cero entradas con identidad desconocida dentro de su ámbito de metadatos. Esto NO es autoridad para retirar el marcador: faltan el plan íntegro, unión de historiales, procedencia y lectura posterior por identidad. Cinco identidades son candidatas a verificación adicional; tres requieren revisión por ausencias/diferencias. No afirmar cinco originales recuperados. El diseño público propuesto está en RECUPERACION-IMPORTACION-LEGACY-DISENO.md; el plan con identidades queda en test-runtime ignorado.

Candidato GNU actualizado: EXE SHA256 93f05f64dde9ed9913e87c5a4d412eb8c541d165ea0368735c0cb717ef23ca31; ZIP SHA256 1c1dd333d43751429968a6be0a7055d328562768d235915c52c24b0782987fec. Compilación productiva correcta en 5m28. Todavía no es una publicación MSVC firmada del canal.

GitHub Actions no inició los trabajos de d4979d7: run 42373001571 muestra bloqueo de cuenta por facturación. El panel actual muestra GitHub Free, uso facturable cero, sin método de pago ni historial; no hay una deuda visible que permita atribuir causa concreta. Reintentar y, si persiste sin incidencia visible, solicitar revisión a soporte. No cambiar presupuestos ni añadir tarjetas automáticamente. La publicación automática permanece pendiente.

El usuario confirmó su iPhone habitual conectado, desbloqueado y con Wallet y Libros cerrados. Windows enumera tres nodos USB Apple sin errores; eso no significa tres teléfonos. No se ha aplicado ni restaurado ninguna imagen durante esta fase. El diagnóstico explícito de recuperación de cuenta está preparado para guardar solo la sesión separada y comparar hashes de los archivos originales antes/después; aún no se ejecutó al escribir este checkpoint.
