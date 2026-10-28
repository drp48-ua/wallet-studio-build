# Wallet Studio 0.6.26: acceso visible y biblioteca

## Alcance autorizado

El usuario solicita mostrar los requisitos Apple al entrar, tanto cuando faltan como cuando ya están instalados; acelerar la biblioteca sin bajar la resolución; y evitar duplicados en Mis diseños al aplicar una imagen existente. Mantiene la autorización previa de publicar en main después de verificar los cambios y la publicación firmada automática. No se modifica el perfil real ni se realizan transferencias al iPhone durante las pruebas.

## Base y trabajo

Main comprobado el 28/10/2026: fcd5e0849be594709a59e939f5b08073611e5079, versión 0.6.25. Rama de trabajo: codex/library-onboarding-20261028. Se conserva la documentación posterior de la reparación local del marcador huérfano. Un único implementador configurado crack_builder modifica la interfaz, el cargador y las pruebas; el responsable revisa, cambia la versión, prueba la interfaz y publica.

Se identificaron tres esperas en biblioteca: el permiso de vistas previas dependía de la sincronización global de copias; un mutex serializaba los trabajadores de red; y cada miniatura completada repetía el filtrado con procesamiento de diseños nativos. La corrección conserva el catálogo autenticado, la propiedad de sesión y el control de acceso. La concurrencia de red debe seguir acotada.

## Estado del checkpoint

La revisión del responsable detectó y corrigió una segunda ruta de duplicación: el mensaje DesignSaved podía promover la caché de transferencia a Mis diseños. El guardado comunica ahora por separado la entrada personal y el archivo exacto de transferencia. El manejador rechaza rutas fuera de la carpeta personal. La prueba de app comprueba tres aplicaciones repetidas sin introducir la caché en la lista.

Compilación final GNU correcta. Biblioteca: 21 pruebas correctas y 2 ignoradas; autenticación: 4 correctas; app: 28 correctas. La suite completa restringida obtuvo 243 correctas y dos fallos existentes del transporte local WinHTTP. El responsable repitió el mismo binario con acceso normal: 245 correctas, cero fallidas, 7 ignoradas y 4 exclusiones de hardware en 31,39 s. No se modificaron ni debilitaron esas pruebas. Registros ignorados: server/test-runtime/library-onboarding-0626-*.log.

Paquete GNU local compilado: dist/Wallet-Studio-0.6.26-Windows.zip. EXE SHA-256: 29c4aed2679096036ff20bf0f51a5fd98587663a3aecfb0ff87570a9edb4c47e. ZIP SHA-256: be6338137a469e6a8b81366cadbc30757c98b10f7555dab08f4668fed9152a34. La demostración aislada está preparada; el primer intento de autorización de control caducó. Falta revisión visual y comprobación Windows/MSVC antes de integrar/publicar.

No se ha publicado la 0.6.26 todavía. La prevención segura de importaciones interrumpidas descrita en CONTINUACION-0.6.25.md queda fuera de este cambio y sigue pendiente. No retirar marcadores, diarios ni cuarentenas de forma general.
