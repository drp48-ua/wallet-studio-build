# Continuación: migración privada, 26 de octubre de 2026

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
