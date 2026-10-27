# Continuación: migración privada, 26 de octubre de 2026

## Estado vigente: migración activa y verificada el 27 de octubre

El usuario autorizó continuar corrigiendo errores. Se corrigió la carga del auxiliar privado mediante `runpy.run_path`, sin cambiar código de la app. La importación del script se comprobó antes de ejecutar. Se suspendió de nuevo el escritor de Render y se verificó la pausa en su interfaz; después se ejecutó la migración con una confirmación de pausa nueva.

Resultado real: **15 ZIP de copias copiados a R2 mediante creación condicional y lectura posterior exacta; ocho tablas y 37 filas copiadas a Supabase nuevo en una única transacción con comparación exacta**. Se conservaron IDs, hashes, sesiones, licencias, equipos e historial. El inventario de `storage.objects` del bucket de copias coincidió exhaustivamente con los 15 objetos indexados. El origen quedó intacto. El resultado privado está en `server/test-runtime/verified-local-migration-result.json`.

Se guardó el entorno del mismo servicio Render con PostgreSQL de destino y proveedor explícito R2, y se cambió su rama a `codex/recovery-onboarding-2026-10-24`. La importación inicial añadió una segunda fila DATABASE_URL: se marcó esa fila nueva para retirar y se actualizó la fila original antes de guardar. No se modificaron WALLET_AUTH_SECRET, correo, webhooks ni productos. Se reanudó el servicio y se desplegó manualmente el commit **60bcc97f0bee7bd1f8b48bf633aef421b391cbb7**, que figura Live. La URL pública se conservó y `/health` respondió HTTP 200 con `status: ok`.

Validación de extremo a extremo desde el ejecutable existente Wallet Studio 0.6.23: **112 peticiones realizadas, 56 originales y 56 miniaturas verificadas, cero fallos, cero originales ausentes**, resultado completo y proceso con salida cero. No fue una prueba sintética: utilizó la sesión guardada y el servidor desplegado. Otra comprobación autenticada confirmó la cuenta, el inventario completo de sus 15 copias y la descarga de la copia mayor: **33 364 712 bytes con SHA-256 exacto**. Por tanto, la sesión existente sigue aceptada sin extraer o publicar el secreto de autenticación.

La revisión automática rechazó exportar todas las variables de Render al portapapeles por la exposición innecesaria de secretos. No se repitió ni se sustituyó por otra extracción de secretos. La continuidad de autenticación se comprobó mediante uso legítimo de la sesión existente contra el servicio nuevo.

La biblioteca activa tiene 56 originales de 1536×969 y sus 56 miniaturas. Las 180 variantes históricas de diseños del almacenamiento antiguo siguen conservadas en el origen; no se afirma haber copiado todas las variantes retiradas. Todas las copias bancarias indexadas sí fueron preservadas y verificadas. Los archivos privados, recetas, imágenes, ZIP y claves siguen ignorados localmente. Firebase no se usó: no fue necesario para este reparto.

La aplicación distribuida sigue en 0.6.23, sin cambios Rust ni necesidad de recompilar para este cambio de servidor. No se realizó una nueva prueba física de captura/restauración con iPhone, ni se repitió la devolución ya completada. No prometer ausencia universal de errores. El siguiente paso de aceptación física, si se solicita, es comprobar con un dispositivo preparado sin alterar originales ni fabricar estados de recuperación.

Los apartados siguientes son antecedentes y quedan superados por este estado vigente. Los commits posteriores que solo documentan el resultado no requieren otro despliegue; Render tiene despliegue automático desactivado.

## Último punto de continuación: migración abortada antes de escribir

Los dos ZIP faltantes se reconstruyeron con el mismo codificador `zip 2.4.2`, los manifiestos canónicos de los recibos y miembros locales verificados. Ambos resultados coinciden exactamente en SHA-256 y tamaño con los registros PostgreSQL originales. Hay **15 de 15 ZIP indexados disponibles localmente**, incluida la historia completa que registra la base de datos. El índice privado actualizado está en `server/test-runtime/migration-local-backups-audit.json`; no publicar sus rutas ni contenido.

Se suspendió el servicio Render antiguo para drenar escrituras. La primera ejecución del script privado `migrate-from-verified-local.py` terminó al importar un auxiliar: `ModuleNotFoundError: No module named 'stage_local_library_r2'`. El archivo auxiliar existente se llama `stage-local-library-r2.py`, con guiones. El fallo ocurrió en los imports, antes de entrar en `run()`: no se copió ningún ZIP a R2, no se abrieron conexiones de migración y no se escribió ninguna tabla de destino. No se corrigió ni reintentó tras el error, por la condición del usuario de detenerse ante fallos.

Se revirtió inmediatamente la suspensión con `Resume Web Service`. La interfaz retiró el estado Suspended y volvió a ofrecer `Suspend Web Service`; la configuración del proveedor, rama y credenciales del servicio anterior no se modificó. No se ha probado su salud después de reanudar; la restricción conocida de almacenamiento del Supabase antiguo no se ha resuelto.

Siguiente ejecución autorizada: corregir únicamente la carga del auxiliar privado (el módulo con guiones no se puede importar por el nombre con guiones bajos). Revisar la herramienta antes de ejecutarla. Volver a pausar y verificar el escritor, copiar los 15 ZIP de forma condicional con lectura posterior exacta, copiar y comparar las ocho tablas en una transacción y conservar el origen. El script privado ya comprueba el inventario exhaustivo de copias mediante `storage.objects` y lo compara con todos los índices de `wallet_private.bundles`; el destino solo se activa después de validar la copia. No reutilizar un marcador de pausa de una ejecución anterior.

Los 112 objetos de la biblioteca siguen verificados en R2; las credenciales limitadas permanecen en el archivo privado de destino. El servidor aún no utiliza esa biblioteca nueva. Tras migrar, actualizar la rama y el entorno del mismo servicio Render, mantener `WALLET_AUTH_SECRET` y URL pública, desplegar y comprobar la aplicación 0.6.23. No afirmar que la app nueva ya está activa ni repetir operaciones físicas del teléfono.

## Actualización del 27 de octubre: biblioteca recuperada

El usuario autorizó utilizar las seis variantes suministradas siempre que conservaran al menos 1536×969. La biblioteca activa está completa: 56 PNG de 1536×969 y 56 miniaturas separadas de 320×200. Se actualizaron únicamente seis huellas/tamaños nativos y seis de miniaturas en los catálogos. Los PNG grandes conservan sus bytes, sin ampliación ni compresión nueva. Los 112 objetos fueron creados de forma condicional y leídos después desde el bucket privado R2 para comprobar igualdad exacta. No se activó el proveedor en producción.

Suite Python tras los catálogos: 97 pruebas ejecutadas, cero fallos y tres omitidas. La preparación independiente verificó los 112 PNG y sus dimensiones. No hubo cambios Rust ni nueva compilación de la aplicación. Los artefactos, imágenes y registros quedan ignorados en `server/test-runtime`; nunca se incluyen en Git.

Se crearon las dos credenciales R2 después de confirmación del usuario. Una tiene acceso de objetos restringido a los dos buckets; la otra tiene lectura R2 de la cuenta, incluido contenido y configuración, alcance que se explicó y confirmó por separado. Las credenciales permanecen en configuración privada, sin imprimirse ni publicarse. Ambos buckets superaron la comprobación real de privacidad y acceso S3.

El inventario completo de objetos se pudo consultar mediante PostgreSQL de origen, sin usar el API de almacenamiento bloqueada: 15 copias y 180 objetos de diseños, incluidos históricos. Se localizaron 13 ZIP locales con SHA y tamaño exactos. Para los dos restantes existen recibos y todos los miembros locales con huellas exactas; todavía debe verificarse si es posible reconstruir los ZIP idénticos. No afirmar que esos dos ya están recuperados. El esquema de las ocho tablas de origen (37 filas), sus relaciones y permisos superó la validación real de solo lectura; el destino no tiene tablas de Wallet Studio.

Antes de migrar cuentas o activar el servidor: terminar la verificación de los 15 ZIP, pausar y drenar al escritor de origen, copiar de forma inmutable y verificar todos los ZIP, copiar y comparar exactamente las ocho tablas en una transacción. Conservar metadatos de copias e identidades, secreto de autenticación y URL pública. Los diseños históricos del origen siguen conservados allí; la biblioteca nueva no implica que se hayan recuperado sus 180 variantes antiguas.

## Estado real

El usuario autorizó migrar Wallet Studio a sus nuevos servicios y publicar el código necesario en una rama separada. R2 está activado. Se crearon `wallet-designs` y `wallet-private`, ambos Standard, con acceso público desactivado; todavía están vacíos. El nuevo Supabase está disponible. Firebase tiene sesión abierta, pero solo muestra dos proyectos ajenos a Wallet Studio; no se modificaron.

No se migraron datos, no se cambió el proveedor activo ni se desplegó el servidor. La aplicación distribuida sigue siendo 0.6.23. No se redujo ni transformó ninguna imagen.

## Parada solicitada ante errores

La preparación inicial de `psycopg[binary]==3.3.6` falló con permiso denegado en el directorio temporal de Windows. Tras la nueva orden explícita de continuar, se usó una carpeta temporal dentro del espacio de trabajo y se habilitó acceso de red. La instalación oficial terminó correctamente, únicamente en una carpeta local ignorada.

Comprobación real posterior, sin escrituras: PostgreSQL de origen y destino conectaron con TLS y una consulta de solo lectura. El acceso a los archivos del Supabase de origen devolvió **HTTP 402**. Se detuvo la comprobación en ese primer rechazo, sin reintentar ni copiar archivos. El bloqueo vigente es el servicio de almacenamiento del origen, no la contraseña del nuevo proyecto. No ejecutar la migración hasta recuperar acceso de lectura al inventario y a todos los originales. No interpretar la conexión PostgreSQL como una migración realizada.

## Código preparado

- `server/remote.py`: adaptador R2 mediante S3 SigV4, endpoint derivado de la cuenta, comprobación independiente de privacidad de dominios administrados y propios, creación condicional y lectura posterior exacta. Supabase sigue siendo el proveedor predeterminado.
- `server/app.py` y `server/designs.py`: selección explícita de proveedor; mantiene autenticación, permisos, catálogo, validación PNG y caché RAM. No introduce imágenes públicas ni modifica sus bytes.
- `server/migrate_storage.py`: comprobación previa de solo lectura por defecto. Inventaría todos los objetos, incluidos históricos y huérfanos. La copia explícita exige que el operador haya pausado y drenado las escrituras. Copia objetos sin sobrescribir, verifica hashes y lectura posterior; copia las ocho tablas en una transacción de destino, verifica filas exactas, relaciones, permisos y RLS. No activa automáticamente el destino ni borra el origen.

Revisión del responsable realizada sobre los cambios y archivos nuevos. Un único implementador configurado `crack_builder`, sin acceso a servicios, perfiles privados o credenciales. Suite Python final: **97 pruebas ejecutadas, cero fallos y tres omitidas**. Foco de migración: 14 correctas. El responsable comprobó únicamente las conexiones reales PostgreSQL de ambos proyectos y encontró HTTP 402 en almacenamiento de origen; el migrador completo y R2 no se probaron en vivo. No hubo cambios Rust, nueva compilación nativa ni pruebas físicas.

## Configuración privada y continuación

La configuración de origen se exportó desde el servicio existente a un archivo local ignorado. El usuario guardó la contraseña del nuevo Supabase en otro archivo ignorado. No imprimir valores ni añadir esos archivos a Git. Los tokens R2 todavía no se crearon. Se requiere acceso a objetos limitado a los dos buckets y acceso de lectura de configuración para demostrar su privacidad; la creación de credenciales persistentes requiere confirmación en la interfaz.

Antes de copiar: verificar acceso al origen pese a su restricción de cuota, inventario completo y destino vacío. Mantener el mismo secreto de autenticación, URL pública del backend e identidades. Pausar y drenar todas las escrituras, incluidas sesiones y webhooks, antes de confirmar el bloqueo de escritura del migrador. El marcador de configuración no pausa servicios por sí mismo. Mantener la pausa hasta el corte coordinado o su reversión.

Después de una copia verificada: desplegar y cambiar configuración de forma coordinada, validar salud, cuenta, permisos, catálogo e integridad de originales e historiales. Resolver positivamente las dos cuarentenas pendientes con evidencia; no fabricar estados ni repetir por rutina la restauración física ya completada. Conservar origen y evidencias locales para reversión.

Los registros de pruebas y archivos privados permanecen bajo `server/test-runtime`, ignorado. No subir cuentas, sesiones, claves, identificadores de teléfonos, originales bancarios ni diseños privados. El checkpoint contiene únicamente código, pruebas sintéticas y documentación. No modificar main ni usar force-push.
