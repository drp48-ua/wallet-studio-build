# Continuación Wallet Studio 0.6.19 — 24/10/2026

## Estado del usuario

El usuario confirmó que las capturas con fallo de importación pertenecen a este PC y a 0.6.18. Las nuevas fotos de su amigo muestran 0.6.17 y una exportación pendiente; no atribuir a ese otro PC el diagnóstico local.

El perfil local conserva una escritura en fase `prepared` sin su continuación en la nube. Archivo remoto, baseline preparado e historial remoto superaron el diagnóstico; el bloqueo corresponde al historial local sin terminar. No se han eliminado marcadores, originales, diarios, cuarentenas ni archivos del teléfono.

El usuario conectó y preparó su iPhone habitual. Comprobación de solo lectura de usbmuxd: un único USB y coincidencia con el dispositivo de la operación local. Los diarios de exportación de este PC ya no tienen operaciones pendientes; queda el intento de escritura. La tarjeta no figura en la lista guardada actual, por lo que se preparó un comando limitado que encuentra la única operación interrumpida del iPhone conectado y verifica su baseline sin identidades por argumentos.

## Cambios

- Presentación veraz en el recuadro: espera y su motivo, guardado con tiempo/fase, atención si se prolonga, fallo y recuperación. Ningún estado visual autoriza una escritura.
- Pérdida del canal del trabajador: vaciar primero todos los mensajes, respetar resultados finales y no confundir un escáner terminado con una captura fallida. Nunca reintentar automáticamente. El transporte nativo se recoge también al salir por una excepción interna.
- Detección de DLL y servicio Apple por separado. Descarga explícita dentro de la app del instalador oficial de iTunes de 64 bits; no se descarga si ya están disponibles los componentes. HTTPS y redirecciones limitadas a Apple, máximo 512 MiB/plazo 15 minutos, firma Authenticode válida con editor Apple. La instalación se abre solo al pulsar su botón y sigue bajo control de Windows/usuario.
- Mensaje de sincronización específico para historial pendiente, sin publicar rutas ni identificadores.
- Comandos de soporte `--wallet-studio-review-interrupted-write` (lectura) y `--wallet-studio-restore-interrupted-write` (devolución explícita del original). Sin rutas ni identidades elegibles por el caller, un único USB y una única operación interrumpida; no se eligen diseños que estén intencionalmente aplicados.

Fuentes verificadas: https://www.apple.com/es/itunes/ enlaza https://www.apple.com/itunes/download/win64/; cabeceras oficiales del 24/10/2026 redirigen a Apple y anuncian 202.759.648 bytes. https://support.apple.com/es-es/118290 explica las aplicaciones actuales de Windows. El runtime de Wallet Studio utiliza Apple Mobile Device Support del instalador de escritorio; no se declara compatibilidad del cargador con paquetes Store no comprobados.

## Verificación y recuperación física

Suite GNU Windows: 203 pruebas correctas, ninguna fallida, siete ignoradas y cuatro exclusiones de hardware exigidas por el repositorio. Verificación Apple aislada: tres correctas, incluida firma válida de un ejecutable Apple ya instalado, sin ejecutarlo. Regresión de disposición: una correcta con seis estados a 320 píxeles. Se corrigió el entorno de módulos del proceso Windows PowerShell que verifica la firma; no se modificó la configuración del sistema.

El usuario autorizó expresamente devolver el original de la operación interrumpida. La revisión nativa encontró una sola coincidencia USB y un original local válido. La devolución se detuvo de forma segura en el segundo paso: los bytes actuales no pertenecen al intento local antiguo. El transporte protegido los devolvió y verificó, conservando el original y el diario. No quedaron exportaciones pendientes, pero sigue el intento local preparado; no debe marcarse restaurado manualmente.

La comparación local de solo lectura confirmó que los bytes conservados coinciden exactamente con el diseño aplicado más reciente del archivo de nube retenido. Ese diseño es posterior al intento preparado local; no hay conflicto de secuencia no heredada. El archivo y su historial remoto son válidos, pero todavía falta una vía que pruebe de extremo a extremo la propiedad autenticada de esos bytes antes de permitir su retirada para recuperar el original. No se ha añadido una excepción al control de propiedad.

## Pendientes

Compilación final GNU de producción correcta, versión de recurso 0.6.19. Paquete local Windows generado. Revisión visual nativa aislada a 1182×791: rueda, tiempo y fase aparecen en el recuadro; el botón y el pie quedan debajo de la imagen completa. Revisión a 850×650 pendiente: caducó el permiso del controlador de escritorio antes de obtener la captura. No inferir validación visual de esa ventana a partir de la prueba de disposición.

Resolver la recuperación mediante una prueba autenticada, duradera y limitada de propiedad de la operación remota más reciente, manteniendo lectura protegida, devolución de bytes desconocidos y verificación física de originales. Véase `RECUPERACION-NUBE-DISENO.md` para el contrato y pruebas requeridas; todavía no está implementado. Después reconciliar el historial y la cuarentena sin relajar sus validadores. La autorización del usuario para esa devolución concreta ya consta; no repetir la pregunta. No iniciar una captura nueva ni actuar sobre el teléfono del amigo desde este PC. Su recuperación necesita sus propios diarios y dispositivo.
