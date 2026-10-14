# Wallet Studio 0.6.0 — propuesta en validación

Esta versión para Windows incorpora almacenamiento independiente de la carpeta del programa, migración sin sobrescribir copias, cuenta de sincronización, servicio desplegable de originales privados y actualización automática en segundo plano desde versiones compiladas de main.

El editor incorpora el nuevo logo proporcionado por el propietario, una paleta grafito/plata/violeta/turquesa, navegación lateral y galería de tarjetas abierta al entrar. Cada tarjeta muestra el último diseño verificado localmente y su original pequeño sin marco. Las vistas del editor están alineadas y las acciones de 44 px siempre visibles. Las consultas de disco, miniaturas y sincronización se realizan fuera del renderizado; desplazarse no vuelve a verificar originales en cada frame. Una referencia visual de otro iPhone nunca habilita una transferencia al teléfono conectado.

Las copias se identifican por iPhone y referencia exacta de tarjeta, no por su nombre. Duplicados válidos se reutilizan y conflictos conocidos bloquean solo su tarjeta. La captura nueva exige procedencia comprobada; un diseño personalizado no sustituye al original. Otra cuenta no puede apropiarse del almacén vinculado.

La actualización comprueba firmas RSA-PSS/SHA-256, contenido y versión antes de preparar la nueva build. No interrumpe la sesión actual y conserva originales fuera de los binarios.

No es una versión comercial aprobada. El servicio no está desplegado y el workflow de publicaciones todavía no está integrado en main. Consulta CLIENTES-Y-NUBE.md y CONTINUAR-CLIENTES-2026-10-14.md antes de publicar o distribuir.

Las copias originales siguen vinculadas al dispositivo y referencia literal de la tarjeta. Los diseños personalizados no deben convertirse en copias originales al iniciar una instalación nueva. Los registros inciertos y los conflictos mantienen el bloqueo y su evidencia.

La animación funcional en Apple Wallet y la paridad macOS no forman parte de esta entrega.
