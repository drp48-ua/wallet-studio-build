# Wallet Studio 0.6.26: acceso visible y biblioteca

## Alcance autorizado

El usuario solicita mostrar los requisitos Apple al entrar, tanto cuando faltan como cuando ya están instalados; acelerar la biblioteca sin bajar la resolución; y evitar duplicados en Mis diseños al aplicar una imagen existente. Mantiene la autorización previa de publicar en main después de verificar los cambios y la publicación firmada automática. No se modifica el perfil real ni se realizan transferencias al iPhone durante las pruebas.

## Base y trabajo

Main comprobado el 28/10/2026: fcd5e0849be594709a59e939f5b08073611e5079, versión 0.6.25. Rama de trabajo: codex/library-onboarding-20261028. Se conserva la documentación posterior de la reparación local del marcador huérfano. Un único implementador configurado crack_builder modifica la interfaz, el cargador y las pruebas; el responsable revisa, cambia la versión, prueba la interfaz y publica.

Se identificaron tres esperas en biblioteca: el permiso de vistas previas dependía de la sincronización global de copias; un mutex serializaba los trabajadores de red; y cada miniatura completada repetía el filtrado con procesamiento de diseños nativos. La corrección conserva el catálogo autenticado, la propiedad de sesión y el control de acceso. La concurrencia de red debe seguir acotada.

## Estado del checkpoint

La revisión del responsable detectó y corrigió una segunda ruta de duplicación: el mensaje DesignSaved podía promover la caché de transferencia a Mis diseños. El guardado comunica ahora por separado la entrada personal y el archivo exacto de transferencia. El manejador rechaza rutas fuera de la carpeta personal. La prueba de app comprueba tres aplicaciones repetidas sin introducir la caché en la lista.

Compilación final GNU correcta. Biblioteca: 21 pruebas correctas y 2 ignoradas; autenticación: 4 correctas; app: 28 correctas. La suite completa restringida obtuvo 243 correctas y dos fallos existentes del transporte local WinHTTP. El responsable repitió el mismo binario con acceso normal: 245 correctas, cero fallidas, 7 ignoradas y 4 exclusiones de hardware en 31,39 s. No se modificaron ni debilitaron esas pruebas. Registros ignorados: server/test-runtime/library-onboarding-0626-*.log.

La revisión visual CUA se completó en demostraciones aisladas: inicio pequeño y maximizado, estados Apple ausente/listo, biblioteca con imagen seleccionada, paso al editor, controles habilitados/deshabilitados y tarjetas de ejemplo/estado vacío sin teléfono. Se corrigió un solapamiento de los rótulos de la tarjeta decorativa en poco espacio: ahora se omite esa decoración cuando no cabe. Una quinta prueba de autenticación comprueba las formas de texto dibujadas, no solo los límites de la página. Las cinco pruebas de autenticación pasan. Capturas locales fuera de Git: wallet-studio-0626-inicio-pequeno.png y wallet-studio-0626-inicio-grande.png en Documents/Codex. Las demostraciones desactivan descargas, cuentas y teléfono; no son una prueba física ni una medición de velocidad del CDN real.

Paquete GNU final compilado: dist/Wallet-Studio-0.6.26-Windows.zip. EXE SHA-256: 5aa07ebc6aa8fe5a2731817438e592b4e907bbcfeb234825299b787367cb498a. ZIP SHA-256: 4d615842018e38673dba540c3ef3ddc4b98d514781cde15f0c0421cb76785070. Repetición completa final del responsable: 246 correctas, cero fallidas, 7 ignoradas y 4 exclusiones de hardware en 38,58 s. Registro privado ignorado: server/test-runtime/library-onboarding-0626-final-full.log. Pendiente: comprobación Windows/MSVC antes de integrar/publicar. PR de revisión: https://github.com/drp48-ua/wallet-studio-build/pull/9.

## Integración en main

Las dos comprobaciones Windows/MSVC del commit final 792abd9a635504dc17dc177a78f36bf5dc8d60c7 terminaron correctamente: https://github.com/drp48-ua/wallet-studio-build/actions/runs/40651538509 y https://github.com/drp48-ua/wallet-studio-build/actions/runs/40651532452. Incluyen compilación, suite nativa, pruebas del servidor con datos ficticios, integración real del cliente con perfiles ficticios y paquete portable. PR #9 integrada mediante merge normal: main 8917fc72133c4f5f79bb9f9ad924639f2747911d. Sin force-push ni cambios de perfil reales.

La publicación firmada build 3 está en curso: https://github.com/drp48-ua/wallet-studio-build/actions/runs/40653265089. Falta confirmar su resultado y verificar firma, tamaño y hashes de los archivos públicos reales. Verificador privado preparado: server/test-runtime/verify-published-0626.py; recibe el SHA de main esperado como argumento. No considerar la actualización publicada hasta completar esa comprobación.

## Publicación final verificada

La ejecución 40653265089 terminó correctamente. Release estable más reciente: https://github.com/drp48-ua/wallet-studio-build/releases/tag/desktop-build-3. Versión 0.6.26, build 3, main 8917fc72133c4f5f79bb9f9ad924639f2747911d. El responsable descargó los archivos públicos y comprobó la firma RSA-PSS/SHA-256 con la clave pública fijada del cliente, tamaño y SHA-256 del ZIP, hash del ejecutable y las 676 entradas sin archivos privados ni rutas de escape.

ZIP MSVC publicado SHA-256: 5efbd2b001370b41f38ffd68c3f8bf8987dce0242d85ac3c8502867507120155. EXE publicado SHA-256: f1039e73cbc22c7a30794a40e403cbc19531b22d6857f7a0c8bc322bb9a1c0c1. Copia local de entrega: Documents/Codex/Wallet-Studio-0.6.26-Windows-publicada.zip. La aplicación real 0.6.25 y su perfil se conservaron; el usuario puede usar Ayuda, Buscar actualizaciones y reiniciar normalmente cuando la actualización esté preparada. No se presenta esta revisión como una transferencia física ni como una medición de velocidad en su conexión.

La prevención segura de importaciones interrumpidas descrita en CONTINUACION-0.6.25.md queda fuera de este cambio y sigue pendiente. No retirar marcadores, diarios ni cuarentenas de forma general.
