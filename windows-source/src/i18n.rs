use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    Spanish,
    English,
    SimplifiedChinese,
}

impl Default for Language {
    fn default() -> Self {
        Self::Spanish
    }
}

impl Language {
    pub fn load() -> Self {
        let path = settings_path();
        if let Ok(contents) = fs::read_to_string(&path) {
            if let Ok(settings) = serde_json::from_str::<Settings>(&contents) {
                if settings.schema_version >= 2 {
                    return settings.language;
                }
                let migrated = migrate_legacy_language(settings.schema_version, settings.language);
                migrated.save();
                return migrated;
            }
        }

        let system_locale = std::env::var("LANG")
            .or_else(|_| std::env::var("LANGUAGE"))
            .or_else(|_| std::env::var("LC_ALL"))
            .unwrap_or_default()
            .to_ascii_lowercase();

        if system_locale.starts_with("zh") {
            Self::SimplifiedChinese
        } else {
            Self::Spanish
        }
    }

    pub fn save(self) {
        let path = settings_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let settings = Settings { schema_version: 2, language: self };
        if let Ok(contents) = serde_json::to_string_pretty(&settings) {
            let _ = fs::write(path, contents);
        }
    }

    pub fn text<'a>(self, source: &'a str) -> &'a str {
        if self == Self::English {
            return source;
        }

        if self == Self::Spanish {
            return match source {
                "Wallet" => "Tarjetas",
                "Passcode" => "Código de acceso",
                "Help" => "Ayuda",
                "Refresh" => "Actualizar",
                "Auto (USB preferred)" => "Automático (USB preferido)",
                "USB only" => "Solo USB",
                "WiFi only" => "Solo Wi-Fi",
                "No device" => "Sin dispositivo",
                "Ready" => "Listo",
                "Unavailable" => "No disponible",
                "Logs" => "Registro",
                "Logs [x]" => "Registro [x]",
                "Copy Logs" => "Copiar registro",
                "Save to File..." => "Guardar en archivo...",
                "Clear" => "Borrar",
                "No events logged yet." => "Todavía no hay eventos.",
                "Language" => "Idioma",
                "English" => "Inglés",
                "Spanish" => "Español",
                "Simplified Chinese" => "Chino simplificado",
                "Language changed." => "Idioma actualizado.",
                "Transport mode" => "Modo de conexión",
                "Ready. Connect iPhone via USB or paired WiFi and unlock it." => "Conecta y desbloquea el iPhone por USB o por Wi-Fi enlazado.",
                "No iPhone connected via USB or paired WiFi." => "No hay ningún iPhone conectado por USB o Wi-Fi enlazado.",
                "Please select a connected iPhone." => "Selecciona un iPhone conectado.",
                "Disconnect the USB cable and refresh to guarantee the full AirTraffic path uses WiFi." => "Desconecta el cable USB y actualiza para usar la conexión Wi-Fi completa.",
                "Please enter or scan a target card hash." => "Selecciona una tarjeta guardada o captura su identificador.",
                "Please choose a card skin image first." => "Primero elige una imagen de diseño.",
                "Crop position updated." => "Se actualizó el encuadre.",
                "Scanning syslog... Open Wallet or tap your card on iPhone." => "Buscando tarjetas… Abre Wallet y toca la tarjeta en el iPhone.",
                "Syslog scanning stopped." => "Se detuvo la búsqueda de tarjetas.",
                "Writing card skin to iPhone..." => "Enviando el diseño al iPhone…",
                "Writing card artwork assets (@3x, @2x, .pdf)..." => "Enviando las imágenes de la tarjeta…",
                "Card skin updated successfully!" => "Diseño de tarjeta actualizado.",
                "Restoring original card artwork..." => "Restaurando las imágenes originales…",
                "Clearing .cache cache..." | "Clearing .pkcache cache..." => "Actualizando la caché de Wallet…",
                "Original card face restored successfully!" => "Imagen original restaurada.",
                "Syslog scan finished" => "Terminó la búsqueda.",
                "Card skin successfully flashed! Force quit Wallet on iPhone and reopen it." => "El diseño se aplicó correctamente. Cierra Wallet en el iPhone y vuelve a abrirlo.",
                "Restoring original card face..." => "Restaurando el diseño original…",
                "Original card face restored. Force close Wallet and reopen it." => "Se restauró el diseño guardado. Cierra Wallet en el iPhone y vuelve a abrirlo.",
                "Original card backup not found." => "Falta una copia original verificable; no se puede restaurar.",
                "Apply a card skin once to create an original backup." => "Se necesita una copia original local, completa y verificable.",
                "Card captured" | "Card detected" => "Tarjeta detectada",
                "Tus tarjetas" => "Tus tarjetas",
                "Elige una tarjeta, prepara un diseño y aplícalo" => "Elige una tarjeta, prepara un diseño y aplícalo",
                "1 · Tarjeta" => "1 · Tarjeta",
                "2 · Diseño" => "2 · Diseño",
                "3 · Aplicar" => "3 · Aplicar",
                "Añadir tarjeta" => "Añadir tarjeta",
                "iPhone conectado" => "iPhone conectado",
                "iPhone no disponible" => "iPhone no disponible",
                "Conecta un iPhone desbloqueado" => "Conecta un iPhone desbloqueado",
                "Buscando tarjeta… abre Wallet y tócala en el iPhone." => "Buscando tarjeta… abre Wallet y tócala en el iPhone.",
                "Tus tarjetas guardadas" => "Tus tarjetas guardadas",
                "Aún no hay tarjetas guardadas" => "Aún no hay tarjetas guardadas",
                "Pulsa «Añadir tarjeta», abre Wallet en el iPhone y toca la tarjeta que quieras personalizar." => "Pulsa «Añadir tarjeta», abre Wallet en el iPhone y toca la tarjeta que quieras personalizar.",
                "Nombre de la tarjeta" => "Nombre de la tarjeta",
                "Por ejemplo, Transporte" => "Por ejemplo, Transporte",
                "Guardar nombre" => "Guardar nombre",
                "No se pudo guardar el nombre. Revisa el texto y vuelve a intentarlo." => "No se pudo guardar el nombre. Revisa el texto y vuelve a intentarlo.",
                "Nombre guardado" => "Nombre guardado",
                "Original guardado" => "Original guardado",
                "Sin copia original verificable" => "Sin copia original verificable",
                "Copia original completa y verificada en este equipo" => "Copia original completa y verificada en este equipo",
                "La miniatura procede de la copia local; no representa una lectura actual del iPhone." => "La miniatura procede de la copia local; no representa una lectura actual del iPhone.",
                "Detalles técnicos" => "Detalles técnicos",
                "Detalles técnicos / introducir identificador" => "Detalles técnicos / introducir identificador",
                "Identificador de tarjeta" => "Identificador de tarjeta",
                "Dispositivo" => "Dispositivo",
                "Selecciona una tarjeta guardada para continuar." => "Selecciona una tarjeta guardada para continuar.",
                "2 · Elige un diseño" => "2 · Elige un diseño",
                "Elige una imagen de tu equipo. La vista previa no modifica el iPhone." => "Elige una imagen de tu equipo. La vista previa no modifica el iPhone.",
                "Elegir imagen…" => "Elegir imagen…",
                "Exportar PNG" => "Exportar PNG",
                "Diseño" => "Diseño",
                "No hay un diseño seleccionado." => "No hay un diseño seleccionado.",
                "Vista previa del diseño seleccionado" => "Vista previa del diseño seleccionado",
                "Arrastra la imagen para ajustar el encuadre." => "Arrastra la imagen para ajustar el encuadre.",
                "Imagen no disponible" => "Imagen no disponible",
                "Copia original verificada" => "Copia original verificada",
                "Guardar original verificable" => "Guardar original verificable",
                "Aplicar diseño" => "Aplicar diseño",
                "Restaurar y aplicar diseño" => "Restaurar y aplicar diseño",
                "DISEÑO ACTUAL" => "DISEÑO ACTUAL",
                "PNG, JPG y WebP · Los GIF animados no se conservan en Wallet." => "PNG, JPG y WebP · Los GIF animados no se conservan en Wallet.",
                "Restaurar copia original" => "Restaurar copia original",
                "Falta una copia original verificable; no se aplicará el diseño." => "Falta una copia original verificable; no se aplicará el diseño.",
                "Hay un cambio anterior sin resolver. La aplicación queda bloqueada para proteger la copia original." => "Hay un cambio anterior sin resolver. La aplicación queda bloqueada para proteger la copia original.",
                "No se pudo restaurar el diseño anterior; el diseño nuevo no se aplicó. Conserva el iPhone conectado y revisa Registro." => "No se pudo restaurar el diseño anterior; el diseño nuevo no se aplicó. Conserva el iPhone conectado y revisa Registro.",
                "Sin una copia original completa y verificable, aplicar y restaurar permanecen desactivados." => "Sin una copia original completa y verificable, aplicar y restaurar permanecen desactivados.",
                "Selecciona una tarjeta antes de guardar su copia original." => "Selecciona una tarjeta antes de guardar su copia original.",
                "Leyendo la copia original del iPhone…" => "Leyendo la copia original del iPhone…",
                "Copia original verificada y guardada en este equipo." => "Copia original verificada y guardada en este equipo.",
                "No se pudo guardar una copia original completa; aplicar y restaurar siguen desactivados." => "No se pudo guardar una copia original completa; aplicar y restaurar siguen desactivados.",
                "No se pudo leer y verificar la copia original. Revisa Registro; aplicar y restaurar siguen desactivados." => "No se pudo leer y verificar la copia original. Revisa Registro; aplicar y restaurar siguen desactivados.",
                "Operation failed. See Logs for technical details." => "No se pudo completar la operación. Consulta Registro para ver detalles técnicos.",
                "No se pudo preparar la imagen. Revisa el formato y vuelve a intentarlo." => "No se pudo preparar la imagen. Revisa el formato y vuelve a intentarlo.",
                "No se pudo abrir la imagen. Comprueba que sea PNG, JPG o WebP." => "No se pudo abrir la imagen. Comprueba que sea PNG, JPG o WebP.",
                "No se pudo actualizar el encuadre. Revisa el registro para ver detalles." => "No se pudo actualizar el encuadre. Revisa el registro para ver detalles.",
                "No se pudo guardar el PNG. Revisa la ubicación y los permisos." => "No se pudo guardar el PNG. Revisa la ubicación y los permisos.",
                "Falta una copia original verificable" => "Falta una copia original verificable",
                "No se encontró una tarjeta compatible en la actividad reciente." => "No se encontró una tarjeta compatible en la actividad reciente.",
                "Selected iPhone has no" => "El iPhone seleccionado no tiene",
                "connection. Refresh devices or change transport mode." => "conexión disponible. Actualiza los dispositivos o cambia el modo de conexión.",
                "Found" => "Encontrados",
                "connected device(s); transport mode:" => "dispositivo(s) conectado(s); modo:",
                "target:" => "destino:",
                "lang:" => "idioma:",
                "bold:" => "negrita:",
                "ON" => "Sí",
                "OFF" => "No",
                "Loaded" => "Cargado",
                "Failed to parse theme:" => "No se pudo leer el tema:",
                "No theme package selected." => "No se ha seleccionado un paquete de tema.",
                "Could not load theme." => "No se pudo cargar el tema.",
                "Could not load image." => "No se pudo abrir la imagen.",
                "Conecta el iPhone" => "Conecta el iPhone",
                "Selecciona una tarjeta o introduce su identificador en los detalles técnicos." => "Selecciona una tarjeta o introduce su identificador en los detalles técnicos.",
                "Identificador técnico de la tarjeta" => "Identificador técnico de la tarjeta",
                "La captura no encontró una copia original accesible. Aplicar y restaurar permanecen desactivados." => "La captura no encontró una copia original accesible. Aplicar y restaurar permanecen desactivados.",
                "La lectura del original puede estar bloqueada por AFC. No se han hecho escrituras; aplicar y restaurar siguen desactivados." => "La lectura del original puede estar bloqueada por AFC. No se han hecho escrituras; aplicar y restaurar siguen desactivados.",
                "La copia original no está disponible mediante AFC. No se han hecho escrituras; aplicar y restaurar siguen desactivados." => "La copia original no está disponible mediante AFC. No se han hecho escrituras; aplicar y restaurar siguen desactivados.",
                "El original se lee mediante AFC. Si iOS deniega el acceso, revisa Registro; las acciones de escritura seguirán bloqueadas." => "El original se lee mediante AFC. Si iOS deniega el acceso, revisa Registro; las acciones de escritura seguirán bloqueadas.",
                "Preparando el diseño de tarjeta…" => "Preparando el diseño de tarjeta…",
                "Preparando la restauración de la copia original…" => "Preparando la restauración de la copia original…",
                "Preparando el tema del teclado…" => "Preparando el tema del teclado…",
                "PNG guardado en:" => "PNG guardado en:",
                "Diseño preparado." => "Diseño preparado.",
                "No se pudo leer el tema. Revisa el archivo e inténtalo de nuevo." => "No se pudo leer el tema. Revisa el archivo e inténtalo de nuevo.",
                "1. Pulsa «Añadir tarjeta» en Tarjetas." => "1. Pulsa «Añadir tarjeta» en Tarjetas.",
                "2. Abre Wallet y toca la tarjeta en el iPhone." => "2. Abre Wallet y toca la tarjeta en el iPhone.",
                "3. La actividad reciente puede detectar y guardar la tarjeta." => "3. La actividad reciente puede detectar y guardar la tarjeta.",
                "4. Puedes cambiar su nombre; el identificador permanece igual." => "4. Puedes cambiar su nombre; el identificador permanece igual.",
                "Guarda y verifica la copia original antes de aplicar un diseño." => "Guarda y verifica la copia original antes de aplicar un diseño.",
                "1. Pulsa «Guardar original verificable» y espera el resultado." => "1. Pulsa «Guardar original verificable» y espera el resultado.",
                "2. Aplicar y restaurar se habilitan solo con los tres originales completos y verificados." => "2. Aplicar y restaurar se habilitan solo con los tres originales completos y verificados.",
                "3. El acceso depende de AFC; si iOS lo deniega, consulta Registro. No se harán escrituras." => "3. El acceso depende de AFC; si iOS lo deniega, consulta Registro. No se harán escrituras.",
                "4. Tras una aplicación confirmada, cierra y vuelve a abrir Wallet para comprobar el resultado." => "4. Tras una aplicación confirmada, cierra y vuelve a abrir Wallet para comprobar el resultado.",
                "Error: " => "No se pudo completar la operación. Consulta el registro para ver detalles: ",
                "Could not enumerate devices:" => "No se pudieron buscar dispositivos:",
                "Scanning syslog..." => "Buscando tarjetas…",
                "Open Wallet on iPhone and tap your card" => "Abre Wallet en el iPhone y toca la tarjeta",
                "Stop" => "Detener",
                "Scan" => "Buscar tarjeta",
                "Card Configuration" => "Tu tarjeta",
                "Target your card and choose replacement artwork" => "Selecciona una tarjeta, elige un diseño y aplícalo",
                "Target Card Hash" => "Identificador de tarjeta",
                "Base64 pass hash..." => "Identificador técnico…",
                "Saved cards" => "Tus tarjetas guardadas",
                "Select..." => "Seleccionar…",
                "Card Skin Artwork" => "Diseño de tarjeta",
                "PNG, JPG, WebP - auto-scaled to 1536x969" => "PNG, JPG o WebP; se adapta al tamaño de Wallet",
                "Drag inside the preview to reposition the crop." => "Arrastra la vista previa para ajustar el encuadre.",
                "Choose Image..." => "Elegir imagen…",
                "Export PNG" => "Exportar PNG",
                "Write to iPhone" => "Aplicar diseño",
                "Apply Card Skin" => "Aplicar diseño",
                "Restore Original" => "Restaurar copia original",
                "connect iPhone" => "conecta el iPhone",
                "choose available transport" => "elige una conexión disponible",
                "enter card hash" => "selecciona una tarjeta",
                "choose image" => "elige un diseño",
                "Need: " => "Falta: ",
                "Wallet Preview" => "Vista previa del diseño",
                "1536 x 969 px pass canvas" => "Vista previa del diseño seleccionado",
                "No artwork loaded" => "Imagen no disponible",
                "No image" => "Diseño no seleccionado",
                "After applying, force close Apple Wallet and reopen it." => "Tras aplicar, cierra Wallet en el iPhone y vuelve a abrirlo.",
                "Passcode Theme" => "Tema del teclado",
                "Custom lockscreen keypad from Cowabunga or Nugget" => "Tema del teclado de bloqueo de Cowabunga o Nugget",
                "Theme Package" => "Paquete de tema",
                "Choose a .passthm archive containing dialer artwork" => "Elige un archivo .passthm con imágenes para el teclado",
                "Choose .passthm..." => "Elegir .passthm…",
                "assets" => "recursos",
                "Target iOS Cache" => "Formato de caché de iOS",
                "Select cache format based on connected iOS version" => "Selecciona el formato según la versión de iOS",
                "Auto (TelephonyUI-10)" => "Automático (TelephonyUI-10)",
                "TelephonyUI-10 (iOS 18+)" => "TelephonyUI-10 (iOS 18+)",
                "TelephonyUI-9 (iOS 16-17)" => "TelephonyUI-9 (iOS 16–17)",
                "TelephonyUI-8 (Legacy)" => "TelephonyUI-8 (anterior)",
                "Keypad Language" => "Idioma del teclado",
                "Subtext alphabet layout (English, Russian, Ukrainian, Japanese, or Universal)" => "Distribución de letras secundaria",
                "Russian" => "Ruso",
                "Ukrainian" => "Ucraniano",
                "Japanese" => "Japonés",
                "All Languages (Universal)" => "Todos los idiomas (universal)",
                "Bold Text (iOS Accessibility)" => "Texto en negrita (accesibilidad de iOS)",
                "Generates *-bold.png for devices with Bold Text turned ON in iPhone Settings -> Display" => "Genera imágenes en negrita para los dispositivos con esa opción activada",
                "Apply Passcode Theme" => "Aplicar tema del teclado",
                "Keypad Preview" => "Vista previa del teclado",
                "Dialer button artwork" => "Imágenes de las teclas",
                "No theme loaded" => "No se ha cargado ningún tema",
                "3x4 Keypad" => "Teclado 3×4",
                "No theme" => "Sin tema",
                "After applying, lock your iPhone to see the new keypad." => "Tras aplicar, bloquea el iPhone para ver el nuevo teclado.",
                "Setup & Card Hash Guide" => "Guía de inicio",
                "Everything you need to connect and capture your card" => "Conecta el iPhone y añade una tarjeta",
                "Prerequisites" => "Antes de empezar",
                "- 64-bit iTunes or Apple Mobile Device Support installed" => "- Instala iTunes de 64 bits o Apple Mobile Device Support",
                "- First-time setup: connect by USB and tap \"Trust this Computer\"" => "- La primera vez, conecta por USB y pulsa «Confiar en este ordenador»",
                "- WiFi: enable WiFi sync, then use the same local network" => "- Para Wi-Fi, activa la sincronización y usa la misma red local",
                "- Select Auto, USB only, or WiFi only in the top bar" => "- Elige el modo de conexión en la barra superior",
                "Finding Your Card Hash" => "Añadir una tarjeta",
                "1. Click \"Scan\" in the Wallet tab" => "1. Pulsa «Buscar tarjeta» en Tarjetas",
                "2. Open Apple Wallet on your iPhone" => "2. Abre Wallet en el iPhone",
                "3. Tap the card you want to customize" => "3. Toca la tarjeta que quieras personalizar",
                "4. Wallet Studio captures the pass hash automatically" => "4. Wallet Studio detecta la tarjeta automáticamente",
                "5. Click \"Stop\" once detected" => "5. Cuando aparezca, pulsa «Detener»",
                "Activation & Theme Guide" => "Aplicar un diseño",
                "Applying skins and dialer keypad packages" => "Diseños de tarjetas y temas del teclado",
                "Activating Apple Wallet Skin" => "Aplicar diseño de tarjeta",
                "1. Click \"Apply Card Skin\" and wait for completion" => "1. Pulsa «Aplicar diseño» y espera a que termine",
                "2. Open App Switcher on iPhone (swipe up from bottom)" => "2. Abre el selector de apps del iPhone",
                "3. Force close Apple Wallet by swiping up on it" => "3. Cierra Wallet deslizándola hacia arriba",
                "4. Reopen Wallet - your new skin appears!" => "4. Abre Wallet para ver el resultado",
                "Passcode Themes (.passthm)" => "Temas del teclado (.passthm)",
                "- Compatible with Cowabunga & Nugget theme packages" => "- Compatible con paquetes de Cowabunga y Nugget",
                "- iOS 18+: Select \"Auto (TelephonyUI-10)\"" => "- iOS 18+: selecciona «Automático (TelephonyUI-10)»",
                "- iOS 16-17: Select \"TelephonyUI-9\"" => "- iOS 16–17: selecciona «TelephonyUI-9»",
                "- Lock screen to verify your updated keypad artwork" => "- Bloquea el iPhone para ver el nuevo teclado",
                _ => source,
            };
        }

        match source {
            "Wallet" => "钱包",
            "Passcode" => "锁屏密码",
            "Help" => "帮助",
            "Refresh" => "刷新",
            "Auto (USB preferred)" => "自动（优先 USB）",
            "USB only" => "仅 USB",
            "WiFi only" => "仅 WiFi",
            "No device" => "没有设备",
            "Ready" => "就绪",
            "Unavailable" => "不可用",
            "Logs" => "日志",
            "Logs [x]" => "日志 [x]",
            "Copy Logs" => "复制日志",
            "Save to File..." => "保存到文件...",
            "Clear" => "清空",
            "No events logged yet." => "暂无日志记录。",
            "Language" => "语言",
            "English" => "英语",
            "Simplified Chinese" => "简体中文",
            "Language changed." => "语言已切换。",
            "Transport mode" => "连接方式",
            "entries" => "条记录",
            "Ready. Connect iPhone via USB or paired WiFi and unlock it." => "就绪。请通过 USB 或已配对的 WiFi 连接并解锁 iPhone。",
            "No iPhone connected via USB or paired WiFi." => "未检测到通过 USB 或已配对 WiFi 连接的 iPhone。",
            "Please select a connected iPhone." => "请选择已连接的 iPhone。",
            "Disconnect the USB cable and refresh to guarantee the full AirTraffic path uses WiFi." => "请断开 USB 线并刷新，以确保完整的 AirTraffic 路径使用 WiFi。",
            "Please enter or scan a target card hash." => "请输入或扫描目标卡片 Hash。",
            "Please choose a card skin image first." => "请先选择卡片皮肤图片。",
            "Crop position updated." => "裁切位置已更新。",
            "Scanning syslog... Open Wallet or tap your card on iPhone." => "正在扫描 syslog... 请在 iPhone 上打开钱包并点击卡片。",
            "Syslog scanning stopped." => "syslog 扫描已停止。",
            "Writing card skin to iPhone..." => "正在将卡片皮肤写入 iPhone...",
            "Please select a .passthm theme file first." => "请先选择一个 .passthm 主题文件。",
            "Writing passcode theme buttons..." => "正在将锁屏密码主题按钮写入 iPhone...",
            "Syslog scan finished" => "syslog 扫描完成",
            "Card skin successfully flashed! Force quit Wallet on iPhone and reopen it." => "卡片皮肤应用成功！请在 iPhone 上强制关闭并重新打开 Wallet。",
            "Passcode theme applied! Lock your iPhone to view the new keypad." => "锁屏密码主题应用成功！锁定 iPhone 查看新的键盘样式。",
            "Card skin updated successfully!" => "卡片皮肤更新成功！",
            "Restoring original card face..." => "正在恢复原卡面...",
            "Restoring original card artwork..." => "正在恢复原卡面图片...",
            "Clearing .cache cache..." => "正在清理 .cache 缓存...",
            "Clearing .pkcache cache..." => "正在清理 .pkcache 缓存...",
            "Original card face restored successfully!" => "原卡面恢复成功！",
            "Original card face restored. Force close Wallet and reopen it." => "原卡面已恢复。请强制关闭并重新打开 Wallet。",
            "Original card backup not found." => "未找到原卡面备份。",
            "Apply a card skin once to create an original backup." => "先应用一次卡片皮肤以创建原卡面备份。",
            "Passcode theme applied successfully!" => "锁屏密码主题应用成功！",
            "Failed to parse theme:" => "解析主题失败：",
            "Card captured" => "已捕获卡片",
            "Loaded" => "已加载",
            "target:" => "目标：",
            "lang:" => "语言：",
            "bold:" => "粗体：",
            "ON" => "开",
            "OFF" => "关",
            "Error: " => "错误：",
            "Found" => "已找到",
            "connected device(s); transport mode:" => "台已连接设备；连接方式：",
            "Could not enumerate devices:" => "无法枚举设备：",
            "Selected iPhone has no" => "选中的 iPhone 没有",
            "connection. Refresh devices or change transport mode." => "连接。请刷新设备或更改连接方式。",
            "Scanning syslog..." => "正在扫描 syslog...",
            "Open Wallet on iPhone and tap your card" => "请在 iPhone 上打开钱包并点击目标卡片",
            "Stop" => "停止",
            "Scan" => "扫描",
            "Card Configuration" => "卡片配置",
            "Target your card and choose replacement artwork" => "选择目标卡片和替换图片",
            "Target Card Hash" => "目标卡片 Hash",
            "Base64 pass hash..." => "Base64 卡片 Hash...",
            "Saved cards" => "已保存的卡片",
            "Select..." => "请选择...",
            "Card Skin Artwork" => "卡片皮肤图片",
            "PNG, JPG, WebP - auto-scaled to 1536x969" => "PNG、JPG、WebP，将自动缩放到 1536x969",
            "Drag inside the preview to reposition the crop." => "在预览区域内拖动以调整裁切位置。",
            "Choose Image..." => "选择图片...",
            "Export PNG" => "导出 PNG",
            "Write to iPhone" => "写入 iPhone",
            "Apply Card Skin" => "应用卡片皮肤",
            "Restore Original" => "恢复原卡面",
            "connect iPhone" => "连接 iPhone",
            "choose available transport" => "选择可用的连接方式",
            "enter card hash" => "输入卡片 Hash",
            "choose image" => "选择图片",
            "select theme" => "选择主题",
            "Need: " => "需要：",
            "Wallet Preview" => "钱包预览",
            "1536 x 969 px pass canvas" => "1536 x 969 像素卡片画布",
            "No artwork loaded" => "尚未加载图片",
            "No image" => "没有图片",
            "After applying, force close Apple Wallet and reopen it." => "应用后，请强制关闭 Apple Wallet 并重新打开。",
            "Passcode Theme" => "锁屏密码主题",
            "Custom lockscreen keypad from Cowabunga or Nugget" => "来自 Cowabunga 或 Nugget 的自定义锁屏键盘",
            "Theme Package" => "主题包",
            "Choose a .passthm archive containing dialer artwork" => "选择包含拨号键盘图片的 .passthm 压缩包",
            "Choose .passthm..." => "选择 .passthm...",
            "assets" => "个资源",
            "Target iOS Cache" => "目标 iOS 缓存",
            "Select cache format based on connected iOS version" => "根据连接设备的 iOS 版本选择缓存格式",
            "Auto (TelephonyUI-10)" => "自动（TelephonyUI-10）",
            "TelephonyUI-10 (iOS 18+)" => "TelephonyUI-10（iOS 18+）",
            "TelephonyUI-9 (iOS 16-17)" => "TelephonyUI-9（iOS 16-17）",
            "TelephonyUI-8 (Legacy)" => "TelephonyUI-8（旧版本）",
            "Keypad Language" => "键盘语言",
            "Subtext alphabet layout (English, Russian, Ukrainian, Japanese, or Universal)" => "按键副文字母布局（英语、俄语、乌克兰语、日语或通用）",
            "Russian" => "俄语",
            "Ukrainian" => "乌克兰语",
            "Japanese" => "日语",
            "All Languages (Universal)" => "全部语言（通用）",
            "Bold Text (iOS Accessibility)" => "粗体文字（iOS 辅助功能）",
            "Generates *-bold.png for devices with Bold Text turned ON in iPhone Settings -> Display" => "为 iPhone 设置 -> 显示与亮度中启用粗体文字的设备生成 *-bold.png",
            "Apply Passcode Theme" => "应用锁屏密码主题",
            "Keypad Preview" => "键盘预览",
            "Dialer button artwork" => "拨号按钮图片",
            "No theme loaded" => "尚未加载主题",
            "3x4 Keypad" => "3x4 键盘",
            "No theme" => "没有主题",
            "After applying, lock your iPhone to see the new keypad." => "应用后，请锁定 iPhone 查看新的键盘样式。",
            "Setup & Card Hash Guide" => "设置与卡片 Hash 指南",
            "Everything you need to connect and capture your card" => "连接设备并捕获卡片所需的全部信息",
            "Prerequisites" => "使用前准备",
            "- 64-bit iTunes or Apple Mobile Device Support installed" => "- 已安装 64 位 iTunes 或 Apple Mobile Device Support",
            "- First-time setup: connect by USB and tap \"Trust this Computer\"" => "- 首次使用：通过 USB 连接并点击“信任此电脑”",
            "- WiFi: enable WiFi sync, then use the same local network" => "- WiFi：启用 WiFi 同步，并确保双方处于同一局域网",
            "- Select Auto, USB only, or WiFi only in the top bar" => "- 在顶部栏选择自动、仅 USB 或仅 WiFi",
            "Finding Your Card Hash" => "查找卡片 Hash",
            "1. Click \"Scan\" in the Wallet tab" => "1. 在钱包页点击“扫描”",
            "2. Open Apple Wallet on your iPhone" => "2. 在 iPhone 上打开 Apple Wallet",
            "3. Tap the card you want to customize" => "3. 点击要自定义的卡片",
            "4. Wallet Studio captures the pass hash automatically" => "4. Wallet Studio 会自动捕获卡片 Hash",
            "5. Click \"Stop\" once detected" => "5. 检测到后点击“停止”",
            "Activation & Theme Guide" => "应用与主题指南",
            "Applying skins and dialer keypad packages" => "应用卡片皮肤和拨号键盘主题",
            "Activating Apple Wallet Skin" => "应用 Apple Wallet 皮肤",
            "1. Click \"Apply Card Skin\" and wait for completion" => "1. 点击“应用卡片皮肤”并等待完成",
            "2. Open App Switcher on iPhone (swipe up from bottom)" => "2. 在 iPhone 上打开 App 切换器（从底部向上滑动）",
            "3. Force close Apple Wallet by swiping up on it" => "3. 向上滑动并强制关闭 Apple Wallet",
            "4. Reopen Wallet - your new skin appears!" => "4. 重新打开钱包，即可看到新的皮肤！",
            "Passcode Themes (.passthm)" => "锁屏密码主题（.passthm）",
            "- Compatible with Cowabunga & Nugget theme packages" => "- 兼容 Cowabunga 和 Nugget 主题包",
            "- iOS 18+: Select \"Auto (TelephonyUI-10)\"" => "- iOS 18+：选择“自动（TelephonyUI-10）”",
            "- iOS 16-17: Select \"TelephonyUI-9\"" => "- iOS 16-17：选择“TelephonyUI-9”",
            "- Lock screen to verify your updated keypad artwork" => "- 锁定屏幕以查看更新后的键盘图片",
            _ => source,
        }
    }

    pub fn option_label(self, option: Self) -> &'static str {
        match (self, option) {
            (Self::Spanish, Self::Spanish) => "Español",
            (Self::Spanish, Self::English) => "Inglés",
            (Self::Spanish, Self::SimplifiedChinese) => "Chino simplificado",
            (Self::English, Self::English) => "English",
            (Self::English, Self::Spanish) => "Spanish",
            (Self::English, Self::SimplifiedChinese) => "Simplified Chinese",
            (Self::SimplifiedChinese, Self::Spanish) => "西班牙语",
            (Self::SimplifiedChinese, Self::English) => "英语",
            (Self::SimplifiedChinese, Self::SimplifiedChinese) => "简体中文",
        }
    }
}

fn migrate_legacy_language(schema_version: u32, language: Language) -> Language {
    if schema_version < 2 && language == Language::English {
        Language::Spanish
    } else {
        language
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Settings {
    #[serde(default)]
    schema_version: u32,
    language: Language,
}

fn settings_path() -> PathBuf {
    let local_app_data = std::env::var("LOCALAPPDATA")
        .unwrap_or_else(|_| r"C:\Users\Default\AppData\Local".to_string());
    PathBuf::from(local_app_data)
        .join("AirCard")
        .join("settings.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chinese_translation_has_english_fallback() {
        assert_eq!(Language::SimplifiedChinese.text("Wallet"), "钱包");
        assert_eq!(
            Language::SimplifiedChinese.text("unknown string"),
            "unknown string"
        );
    }

    #[test]
    fn language_option_labels_follow_current_language() {
        assert_eq!(Language::default(), Language::Spanish);
        assert_eq!(Language::Spanish.option_label(Language::Spanish), "Español");
        assert_eq!(
            Language::English.option_label(Language::SimplifiedChinese),
            "Simplified Chinese"
        );
        assert_eq!(
            Language::SimplifiedChinese.option_label(Language::SimplifiedChinese),
            "简体中文"
        );
    }

    #[test]
    fn spanish_is_default_and_legacy_english_preferences_migrate_once() {
        assert_eq!(migrate_legacy_language(0, Language::English), Language::Spanish);
        assert_eq!(migrate_legacy_language(0, Language::SimplifiedChinese), Language::SimplifiedChinese);
        assert_eq!(migrate_legacy_language(2, Language::English), Language::English);
    }

    #[test]
    fn spanish_ui_uses_complete_translations_for_core_wallet_flow() {
        assert_eq!(Language::Spanish.text("Wallet"), "Tarjetas");
        assert_eq!(Language::Spanish.text("Choose Image..."), "Elegir imagen…");
        assert_eq!(Language::Spanish.text("Original card backup not found."), "Falta una copia original verificable; no se puede restaurar.");
        assert_eq!(Language::Spanish.text("unknown technical detail"), "unknown technical detail");
    }
}
