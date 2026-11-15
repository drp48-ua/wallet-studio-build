# Diseño de recuperación de una importación antigua

Estado actualizado el 15 de noviembre: se implementó y probó una reparación supervisada del marcador antiguo vacío. No autoriza escrituras del iPhone ni retira revisiones de tarjetas.

La auditoría de solo lectura de 34 preparaciones encontró 32 baselines válidos, dos originales cuya procedencia requiere revisión, 33 historiales válidos y uno inválido. Hubo 410 archivos iguales al destino, 200 ausentes y cuatro diferentes. Las diferencias afectan a dos metadatos de baseline y dos imágenes locales; se conservan ambas versiones. Estas cifras cuentan apariciones, no tarjetas únicas. El censo independiente de identidades debe terminar antes de decidir si una barrera global puede convertirse en revisiones concretas.

El censo independiente posterior encontró ocho identidades distintas y cero entradas con identidad desconocida en los metadatos revisados. Esto acredita consistencia de identidad, no originales ni autoridad para escribir. Tres identidades reúnen los archivos ausentes; dos de ellas tienen además diferencias de baseline/imagen. Las otras cinco son candidatas a validar la unión nativa y la lectura completa. Recuento aprobado para el plan preliminar: cero completas verificadas todavía, cinco candidatas y tres revisiones conocidas. El plan con identidades está fuera de Git y conserva `execution_authorized=false` y `marker_action=preserve`.

## Clasificación previa

- Completa y verificada: baseline con procedencia acreditada, identidad exacta, unión de todas las revisiones íntegra sin huecos, estado terminal permitido y lectura completa de todos los destinos. Una preparación o una operación pendiente del teléfono mantiene sus bloqueos propios.
- Identidad conocida en revisión: identidad coherente comprobada en metadatos y evidencia, con conflicto de baseline, procedencia no acreditada, historial incompatible, archivo ausente o falta de vínculo con un archivo recibido. Solo permite crear una revisión duradera; nunca autoriza un original o transferencia.
- Desconocida: identidad ambigua, JSON o archivos ilegibles, enlaces, límites excedidos, manifiestos incompatibles o evidencia huérfana sin identidad. Cualquier entrada así conserva el bloqueo global.

## Transacción propuesta

1. Excluir propietarios activos mediante el bloqueo de operaciones y confirmar un marcador antiguo regular exactamente vacío. Rechazar otros formatos.
2. Inventariar en ambas direcciones preparaciones, archivos recibidos, destinos, revisiones y diarios. Congelar hashes y versiones; no usar solo nombres de carpetas ni una migración exitosa como prueba.
3. Construir un plan privado ligado al inventario. Validar las uniones fuera del almacén original. La reconstrucción de un ZIP solo vincula evidencia si su SHA-256 completo coincide exactamente; no acredita por sí sola procedencia ni devolución al teléfono.
4. Exigir cero identidades desconocidas. Crear y verificar revisiones duraderas para todas las identidades dudosas antes de retirar cualquier barrera. Conservar todas las evidencias y los bloqueos por dispositivo existentes.
5. Escribir un diario y una barrera de recuperación duraderos, reconocidos por el arranque. Revalidar el inventario y el marcador bajo exclusión antes de ejecutar el plan.
6. Conservar el marcador original byte a byte en evidencia. Confirmar el diario de finalización antes de retirar la nueva barrera. Una interrupción debe dejar la barrera o permitir únicamente una reanudación verificada e idempotente del mismo plan.
7. Nunca sobrescribir originales ni revisiones inmutables. Reponer ausencias es un plan separado, permitido únicamente después de validar la unión completa y comprobar cada lectura posterior.
8. Al recuperar la interfaz completa siguen siendo obligatorios la sesión, el inventario autenticado reciente, las revisiones por tarjeta y las recuperaciones del dispositivo. Retirar una barrera global no significa que todas las tarjetas estén verificadas.

Pruebas necesarias: propietario activo, marcador desconocido, inventario modificado, identidades cruzadas, conflictos conservados, falta de archivo, caída antes y después de cada fase, repetición del plan y bloqueo del mismo dispositivo. No se ejecutó esta transacción sobre un perfil real.

## Implementación aceptada y comprobada

La implementación usa una transacción más pequeña que la propuesta inicial: el marcador antiguo permanece como barrera durante toda la preparación. Se escriben y verifican las revisiones necesarias y una auditoría privada en una carpeta hermana; el inventario vuelve a comprobarse bajo exclusión. El único punto de confirmación es archivar el marcador vacío mediante un renombrado duradero, en el mismo volumen y sin reemplazo. Así no se introduce una segunda barrera cuya retirada necesite otra transacción. No se copian ni mezclan originales o historiales.

Las pruebas cubren fallos e interrupciones antes y después del punto de confirmación, concurrencia, inventario modificado y colisión del archivo de evidencia. La repetición posterior se rechaza sin modificar el almacén. Un error de lectura posterior al renombrado intenta restaurar la barrera y mantiene el almacenamiento bloqueado.

La ejecución real supervisada confirmó dos identidades completas, seis en revisión y 2.147 archivos originales conservados. No operó sobre el teléfono. Los registros históricos de transporte completados se conservan como evidencia pasiva: carecen de un hash histórico independiente y nunca acreditan originales o el estado actual del dispositivo. Las recuperaciones del teléfono y revisiones de las seis tarjetas siguen siendo obligatorias antes de permitir sus operaciones.
