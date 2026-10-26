# Wallet Studio 0.6.22

- Los historiales grandes se sincronizan mediante partes independientes de hasta 32 MiB. Cada parte contiene el original completo y operaciones completas. La compresión no cambia las imágenes ni elimina revisiones.
- Una subida interrumpida conserva los recibos de las partes confirmadas y puede reanudarse. No se anuncia una sincronización completa mientras falte una parte.
- La revisión de copias antiguas valida solo el historial de la tarjeta correspondiente. Otras tarjetas del perfil no provocan un rechazo por identidad mezclada.
- Las copias que llegan en distinto orden se concilian después de reunir la evidencia necesaria. Los errores de lectura, integridad e identidad siguen bloqueando; no se convierten en éxito.
- Los archivos antiguos de nombres de tarjetas se conservan sin confundirse con copias bancarias. Solo se reconocen mediante su nombre, hash y estructura exactos; los archivos desconocidos siguen bloqueados.
- Los archivos antiguos que contienen diseños modificados entre sus supuestos originales se conservan separados. Solo una copia fiable del mismo emisor permite resolver su revisión.

La sincronización no mueve imágenes del iPhone. Las transferencias físicas mantienen identidad exacta, original verificable, diario durable y lectura posterior. Una copia principal antigua sin procedencia válida no se sobrescribe automáticamente. Si un original o una operación indivisible supera el límite, la app conserva los archivos y requiere resolver ese caso por separado.

Validación nativa: 230 pruebas correctas, cero fallidas, siete ignoradas y cuatro exclusiones de hardware. Incluye historiales mayores de 32 MiB, importación inversa, recuperación desde partes, interrupción de subida y numeración de versiones antiguas. La prueba física anterior sigue siendo específica del dispositivo comprobado; no acredita todos los modelos o versiones de iOS.
