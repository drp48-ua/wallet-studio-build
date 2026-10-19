# Instalación y dependencias Windows

La preparación actual es para Windows x64. La compatibilidad de la nueva compilación con modelos de iPhone y versiones de iOS debe comprobarse físicamente antes de anunciarla. No hay una prueba nueva de USB, modificación o restauración en esta preparación.

## Para utilizar Wallet Studio

1. Obtén el paquete desde la entrega de tu compra, verifica su SHA-256 y la firma del editor, y extrae la carpeta completa en una ubicación de tu usuario con permiso de escritura. No uses DLL descargadas de webs de terceros.
2. Instala o repara el software de Apple siguiendo [la página oficial para Windows](https://support.apple.com/es-es/118290). Apple ofrece Dispositivos Apple e iTunes; la disponibilidad de esas aplicaciones no garantiza que esta integración encuentre las bibliotecas que necesita.
3. El cargador existente exige **CoreFoundation.dll, MobileDevice.dll y AirTrafficHost.dll juntas** en `C:\Program Files\Common Files\Apple\Mobile Device Support` o `C:\Program Files (x86)\Common Files\Apple\Mobile Device Support`. Deben ser compatibles con el ejecutable x64 y con sus dependencias. No copies archivos a mano entre instalaciones de 32 y 64 bits.
4. Reinicia Wallet Studio después de instalar o reparar. Si persiste el aviso, contacta con javicarmelosomanager@gmail.com e indica únicamente el texto del error y tu versión de Windows y Wallet Studio. No adjuntes datos de tarjetas.
5. Conecta el iPhone desbloqueado y acepta «Confiar». Que las DLL carguen no demuestra que la conexión ni las operaciones de Wallet funcionen.

No distribuimos componentes propietarios Apple ni modificamos su cargador o protocolo. Una instalación de Microsoft Store puede utilizar otras ubicaciones; todavía requiere validación específica. Si no cumple la detección, no prometemos compatibilidad ni recomendamos descargas antiguas no verificadas.

## Solo para compilar

El usuario final no necesita Rust, Cargo, MSVC ni Windows SDK. El desarrollador sí necesita:

- [Rust estable con rustup](https://rustup.rs), host `x86_64-pc-windows-msvc`.
- [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022), herramientas C++ x64, compilador y enlazador MSVC.
- Windows SDK, incluido `rc.exe`, disponible en una consola de desarrollo x64. Comprueba también las dependencias de runtime MSVC del ejecutable final; si se requiere el redistribuible, usa [la distribución oficial de Microsoft](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist).

Desde el repositorio: `./scripts/check-windows-dependencies.ps1 -Mode All`. El modo Runtime comprueba archivos en las rutas del cargador; no prueba arquitectura, carga ni USB. La aplicación comprueba la carga real de las bibliotecas. El modo Build comprueba herramientas y Rust estable; la compilación es la prueba definitiva del SDK y enlazador.
