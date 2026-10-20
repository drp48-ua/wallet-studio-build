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

El candidato corregido se verificó visualmente en una demostración aislada de 1180×760: navegación y cuadrícula visibles con el nuevo estilo. Se abrió el mismo ejecutable en producción, en ventana maximizada: preparó el almacenamiento, recuperó la sesión existente y entró en Biblioteca con 56 diseños. Las primeras filas cargaron imágenes reales. El usuario interactuó con ambas ventanas; se evitó interrumpir repetidamente sus acciones. No se automatizó su contraseña.

La selección y aplicación física en el teléfono no se repitieron: no había un iPhone conectado. El traslado al editor está cubierto por las pruebas conservadas; no se presenta como nueva prueba manual. No se certifica una transferencia real por esta revisión visual.

EXE final SHA-256: `9794bcda063a3af7ba2bc441cc2b0436141fdfe1c55936c9fa1a11c376df4c0f`.
ZIP final SHA-256: `63a263011799f8dfa526fdd1be7150d3063908016c81a94e326d583fc52c8c42`.
Paquete local de entrega: `C:/Users/danir/Documents/Codex/Wallet-Studio-0.6.11-Windows.zip`.
Proceso de producción abierto desde `windows/Wallet Studio.exe`. La demostración usa la extracción independiente en `outputs/wallet-studio-0.6.11-glass` y no contiene el perfil real. El servidor Node de la maqueta fue detenido.

## Privacidad y cuota

No incluir imágenes comerciales, copias bancarias, sesiones, tokens, identidades ni registros privados en Git o en el ZIP. Comparación antes/después de abrir producción: 20 archivos bancarios antes, 20 después, cero diferencias de longitud o SHA-256. El ZIP comprobado excluye los hashes privados y los runtimes web.

Consultar límites en los puntos de control. Con 3 % restante, guardar y subir esta rama; con 1 %, parar trabajo. No usar el crédito de reinicio.

## Ajuste de distribución solicitado después de la entrega

La revisión 0.6.11 ya estaba subida en 01de0a5. El usuario pidió que la selección de Biblioteca volviera arriba y que Mis tarjetas llenara el ancho, con tres o cuatro tarjetas por fila cuando quepan.

Se cambió únicamente app.rs y glass.rs: selección siempre encima del catálogo; cálculo de una a cuatro columnas según ancho y cantidad, distribuyendo todo el espacio con separaciones de 16 puntos. Se conservan proporciones, selección y controles de seguridad. Revisión del lead y git diff --check aprobados; cinco casos de cálculo de distribución comprobados. No se compiló el cambio ni se verificó visualmente en un nuevo EXE por el margen de cuota restante.

Continuación: ejecutar la prueba de distribución, compilar y empaquetar un nuevo candidato identificable, revisar Biblioteca y Mis tarjetas en ventanas pequeña/grande, y entregar el ZIP actualizado. El ZIP y el proceso 0.6.11 entregados anteriormente mantienen la distribución anterior. No confundir la subida de código con un binario actualizado. No usar el crédito de reinicio sin autorización.

## Entrega 0.6.12 — 20 de octubre

Se completó la compilación pendiente. La revisión visual del primer candidato confirmó Biblioteca con la selección arriba, y Usar diseño trasladó el PNG 1536×969 al editor en demo aislada. Detectó además que horizontal_wrapped podía perder una columna a 1180 puntos aunque llenara la fila maximizada. Ese candidato fue retirado.

La corrección final a4472fb usa filas explícitas y calcula el ancho dentro de su scope. Se conservaron los callbacks. Seis pruebas dirigidas aprobadas, incluida la ejecución del mismo helper y tarjetas reales de dibujo con barra lateral, scrollbar, anchos 1180/1180.35/1920 y tres/ocho tarjetas. Se verifican ambos bordes y el salto de fila. No se repitieron suites ajenas al cambio.

Candidato final compilado correctamente en 2 min 41 s. EXE: 12.712.960 bytes, SHA256 9d54b0869300f453fc93b1c44f03687fe37ca1c8e81751cd78abc30086568e71. ZIP: 8.350.441 bytes, SHA256 03ba676a91f42cc91ef23523a3413b6092a9330a810029c5f8caf2a35ad848b8. Versión Windows 0.6.12, CRC e igualdad del EXE empaquetado correctas; sin imágenes privadas, perfiles, tokens ni runtime web. Los hashes del candidato retirado no son los de entrega.

Entrega: C:/Users/danir/Documents/Codex/Wallet-Studio-0.6.12-Windows.zip. Extracción correcta en outputs/wallet-studio-0.6.12-final. Se abrió el ejecutable final en modo normal. Las ventanas de demostración anteriores pueden seguir abiertas; no se forzó su cierre mientras el usuario interactuaba con el escritorio. No confundir outputs/wallet-studio-0.6.12-layout (candidato retirado) con la carpeta final.

La captura manual del candidato corregido quedó limitada por ventanas minimizadas y actividad del usuario; no se declara aceptación visual completa de ese último binario. La validación de filas final es la prueba renderizada compartida. Para continuar la revisión manual, mantener visible la ventana final y comprobar tres/cuatro columnas y selección superior a distintos tamaños.

Tras abrir producción: 20 archivos bancarios antes y después, cero diferencias de tamaño/SHA256. No se inició captura, aplicación ni restauración en un teléfono. La autorización de esta tarea fue compilar, comprobar y entregar los ajustes de distribución.
