# Wallet Studio 0.6.20

- Las copias con conflictos de historial o contenido se conservan pendientes. Las demás copias verificables pueden seguir descargándose y subiendo; una sincronización parcial no se anuncia como completa.
- Los archivos en cuarentena se reutilizan únicamente si coinciden en tamaño y SHA-256 con el inventario autenticado actual, evitando descargar el mismo archivo grande en cada reintento. No se sobrescribe evidencia dañada.
- Las tarjetas con originales importados aparecen en la lista aunque no se hayan detectado antes en ese PC. Se conservan sus nombres guardados y las comprobaciones de identidad al transferir.
- Reanudar una restauración exige volver a comprobar todos los archivos originales presentes y ausentes, incluido el manifiesto. Un checkpoint antiguo no acredita el estado actual del iPhone.
- Una revisión de cuarentena solo se libera tras conciliar positivamente evidencia de la tarjeta exacta. La ausencia de archivos o un ZIP ilegible no se interpretan como recuperación completada.
- «Restaurar original» puede resolver el caso de una operación local interrumpida y un diseño posterior de la misma cuenta. Verifica el inventario completo, la igualdad del original y el historial remoto antes de crear un diario durable. Solo retira bytes acreditados, verifica todos los originales y comprueba las cachés antes de cerrar los historiales afectados. La preparación de la nube tiene un límite de diez minutos y de 512 MiB; nunca autoriza una retirada parcial si falta evidencia.
- Si un trabajador de captura o restauración termina sin resultado, la interfaz sale del estado ocupado, conserva las copias y pide comprobar la recuperación antes de reintentar.

Las recuperaciones sin verificar conservan originales y diarios y bloquean otra escritura sobre el dispositivo afectado. Esta versión no promete soporte para versiones de iOS o dispositivos que no hayan sido validados.
