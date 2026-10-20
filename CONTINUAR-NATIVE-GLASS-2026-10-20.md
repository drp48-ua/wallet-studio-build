# Wallet Studio nativo con glassmorphism

## Objetivo autorizado

El usuario rechazó la maqueta web de `wallet-studio-glassmorphism`: quiere un ejecutable de Windows con las funciones de 0.6.10 y el nuevo aspecto. La implementación usa el motor Rust/egui existente; no incorpora Node, HTML, datos falsos ni acciones simuladas.

Rama de trabajo: `codex/native-glassmorphism-2026-10-20`, en el repositorio `drp48-ua/wallet-studio-build`. Se conserva el repositorio web de referencia sin modificaciones. No se sobrescribe main.

## Trabajo de colaboradores conservado

Se encontraron cambios posteriores a nuestra revisión anterior en `codex/library-originals-2k-2026-10-19`. Se incorporaron los commits 6bc7094 y c651e49 mediante cherry-pick: catálogo privado de 56 imágenes a 1536×969, compatibilidad legacy y liberación de conexiones HTTP inactivas. El servicio ya estaba desplegado en c651e49; esta ejecución no volvió a desplegarlo ni cambió secretos.

La contribución visual 8e11473 fue revisada. Se adaptaron sus marcos de importación de temas y teclado. Se mantiene un módulo de pintura integrado y acotado, controles nativos y las acciones originales; no se muestran métricas ficticias de copias verificadas.

## Verificación real del servicio

La prueba agregada inicial encontró fallos transitorios; una muestra de cuatro IDs verificó 1536×969. Después, el diagnóstico Rust explícitamente habilitado, de solo lectura, verificó 112/112 peticiones: 56 vistas previas y 56 originales, sin fallos. Duración: 184,66 segundos. Comprobó bytes, SHA-256, PNG y dimensiones con la sesión existente internamente; no imprimió identidad, tokens o contenido de imágenes, no modificó perfiles y no inicializó Apple ni el teléfono. Los registros privados están excluidos de Git.

Esto demuestra disponibilidad e integridad en esa ejecución, no una garantía permanente de red ni una prueba física de aplicación en Wallet.

La suite Python integrada final pasó 80 pruebas, con dos omisiones opcionales. La suite Rust pasó 170 pruebas de la app y una del motor; integración sintética WinHTTP/DPAPI y doce pruebas de catálogo finales aprobadas. Tras el fallo visual confirmado se aprobaron cinco pruebas específicas de glass, incluida la composición final de las formas; no se repitieron suites ajenas sin cambios.

## Aceptación visual

El primer candidato compilado tenía una capa de fondo que ocultaba los controles, aunque existían en accesibilidad. Fue rechazado en el host. Se corrige pintando sobre la capa de fondo predeterminada de egui antes de los paneles; una regresión comprueba el orden de pintura. No distribuir los hashes del primer candidato como definitivos.

Pendiente de registrar al terminar: hashes del candidato corregido, revisión visual pequeña y grande, selección de diseño y traslado al editor, apertura de producción y comparación final de huellas de las copias originales. No hay autorización nueva para transferencias al teléfono en esta revisión.

## Privacidad y cuota

No incluir imágenes comerciales, copias bancarias, sesiones, tokens, identidades ni registros privados en Git o en el ZIP. El snapshot privado anterior a abrir producción contiene 20 archivos de copias bancarias. Comparar solo conteos y huellas.

Consultar límites en los puntos de control. Con 3 % restante, guardar y subir esta rama; con 1 %, parar trabajo. No usar el crédito de reinicio.
