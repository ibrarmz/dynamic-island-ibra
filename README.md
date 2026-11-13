# Dynamic Island para escritorio

Una isla dinámica estilo macOS para Windows, Fedora y Debian, hecha con **Tauri 2 + Rust + Svelte 5**.

## Instalar

Descarga el instalador de la [última versión](https://github.com/ibrarmz/dynamic-island-ibra/releases/latest).

| Sistema | Archivo | Notas |
| --- | --- | --- |
| Windows 10/11 | `DynamicIsland_<versión>_x64-setup.exe` | Recomendado. Instala por usuario, sin permisos de administrador. |
| Windows 10/11 | `DynamicIsland_<versión>_x64.msi` | Para todo el equipo, en Program Files. Pide administrador; útil para desplegar con Intune / GPO (`msiexec /i … /qn`). |
| Debian / Ubuntu | `dynamic-island_<versión>_amd64.deb` | `sudo apt install ./dynamic-island_<versión>_amd64.deb` |
| Fedora | `dynamic-island-<versión>-1.x86_64.rpm` | `sudo dnf install ./dynamic-island-<versión>-1.x86_64.rpm` |

Usa uno de los dos de Windows, no ambos. Los dos, al desinstalar, quitan los hooks de Claude y el inicio automático (`dynamic-island uninstall-cleanup`). No están firmados: si SmartScreen avisa, "Más información" → "Ejecutar de todas formas".

En Linux, antes de desinstalar, corre `dynamic-island hooks uninstall` si conectaste Claude Code y apaga "Iniciar con el sistema" en Ajustes: el paquete se desinstala como root y no puede tocar la configuración de tu usuario.

## Compilar

Requisitos: Node 20+ y Rust (toolchain MSVC en Windows).

```sh
npm install
npm run tauri dev                    # desarrollo con recarga en caliente
npx tauri build                      # instalador: src-tauri/target/release/bundle/nsis/*.exe (Windows)
```

`island-hook` se compila primero (`npm run build:hook`, lo hace solo `tauri build`) y el instalador lo incluye junto a la app.

**Linux** necesita las librerías de WebKitGTK:

```sh
# Debian/Ubuntu
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev pkg-config build-essential
# Fedora
sudo dnf install webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel librsvg2-devel libxdo-devel openssl-devel pkgconf-pkg-config
```

O sin instalar nada, los `.deb` y `.rpm` desde Docker (también desde Windows): ver `ci/linux-bundle.Dockerfile`.

La isla vive arriba de la pantalla principal; el ícono de bandeja tiene **Configuración**, **Mostrar / ocultar** y **Salir**. Solo corre una instancia: si ya hay una, la segunda se cierra sola.

## Publicar una versión

```sh
npm run release -- 0.1.2
```

Pone la versión en `package.json`, `tauri.conf.json` y `Cargo.toml`, hace commit, crea el tag `v0.1.2` y lo sube. GitHub Actions (`.github/workflows/release.yml`) compila el instalador de Windows y los paquetes `.deb`/`.rpm` y los publica en el release, en unos 15–20 minutos.

## Ajustes

Ícono de engrane en el dock, o bandeja → **Configuración**. Se guardan en `settings.json` dentro de la carpeta de configuración de la app.

- **Iniciar con el sistema** (registro de Windows / `~/.config/autostart` en Linux)
- **Posición:** izquierda, centro o derecha
- **Espera para abrir** al pasar el cursor, sin y con actividad (100 ms en 100 ms)
- **Modo fantasma** y **esconder con ventanas maximizadas**
- **Focus:** minutos de focus y de descanso (la regla del panel Focus también los guarda)
- **Secciones del dock** visibles
- **Claude Code:** conectar / desconectar, y si la isla se abre cuando pide permiso

### Conectar Claude Code

En la isla, sección **Claude** → **Conectar**. Eso agrega hooks a `~/.claude/settings.json` (guarda antes un respaldo `settings.json.bak-dynamic-island`) y aplica a las sesiones nuevas de Claude Code. También desde terminal: `dynamic-island hooks install` / `hooks uninstall`.

Los hooks llaman a `island-hook`, un ejecutable de 200 KB que solo reenvía el evento por un canal local (named pipe en Windows, socket Unix en Linux; nada de puertos de red). Van con `"async": true`, así que Claude nunca espera a la isla.

### Cómo se aparta

- **Sin actividad** es solo una línea de 64×5 px. Con música, focus o Claude trabajando muestra la píldora.
- **Modo fantasma** (Windows): si el cursor solo pasa por encima, la isla se desvanece al 15 % y los clics van a lo que hay debajo. Se abre si dejas el cursor quieto 0.4 s (0.6 s si hay algo activo) o si mantienes `Ctrl`. En Linux se abre igual con la espera o `Ctrl`, pero la isla misma no deja pasar clics.
- **Ventana maximizada o pantalla completa** en ese monitor: la isla sube y deja 2 px visibles. Se asoma sola cuando termina un focus o cuando Claude pide permiso.

## Estado

| Pieza | Windows | Linux |
| --- | --- | --- |
| Ventana transparente, siempre encima, clic a través | Probado | Compila; ventana creada en su lugar (Xvfb) |
| Píldora, expansión, modo fantasma, se esconde con ventanas maximizadas | Probado | Compila; sin probar en escritorio real |
| Inicio, Focus | Probado | Compila |
| Música | SMTC, probado | MPRIS, compila; sin probar |
| Claude: fases, diff, permisos, terminó | Probado con eventos simulados | Canal y `island-hook` probados en contenedor |
| Notas: nota rápida con guardado automático, botón **+** del dock | Probado (render) | Compila |
| Sistema: CPU con historial, memoria, disco, batería, tiempo encendido, RAM de la isla | Probado | Compila |
| Ajustes, posición, autoarranque | Probado en la app instalada | Compila |
| Instalador / desinstalador | NSIS por usuario, probado instalar → usar → desinstalar | `.deb` / `.rpm` generados en Docker, sin instalar en un escritorio real |

Las notas se guardan en `notes.md` dentro de la carpeta de datos de la app (`%APPDATA%\dev.dynamicisland.island` en Windows, `~/.local/share/dev.dynamicisland.island` en Linux). Mientras escribes, la isla no se cierra al sacar el cursor; se cierra al hacer clic en otra app, y en Windows le devuelve el teclado a la app donde estabas.

En Linux la isla corre por X11/XWayland en todos los escritorios (GNOME no permite a una app de Wayland colocarse sola ni quedarse encima). En una sesión Wayland, "ventana maximizada" solo detecta apps X11.

## Rendimiento medido (Windows 11, build release)

| Métrica | Valor |
| --- | --- |
| Binarios | 3.9 MB + 0.2 MB (`island-hook`) |
| RAM privada, árbol completo (app + WebView2) | ~140–170 MB (con todos los paneles; el renderer crece con el uso y baja al cerrar) |
| — de eso, el núcleo en Rust | ~10 MB |
| CPU en reposo | 0.3–1.3 % de un núcleo (el núcleo en Rust, ~0.05 %) |

La meta inicial de < 60 MB no se alcanza con WebView2: su piso son ~100 MB repartidos en 6 procesos. Lo aplicado para bajar desde 165 MB:

- Sin proceso de GPU (`--disable-gpu`): −55 MB. La isla es pequeña, el render por software basta.
- Un solo renderer, sin extensiones, sin crashpad, V8 en *lite mode*.
- El cursor se lee con `GetCursorPos` directo y el reloj solo marca segundos cuando algo los muestra.
- La música solo avisa a la UI cuando algo cambia de verdad.

Si se quiere bajar de 60 MB, el plan B sigue vigente: dibujar la píldora compacta en nativo y crear el WebView solo al expandir.

## Estructura

```
src/                       UI (Svelte 5)
  App.svelte               isla, expansión, dock, tinte del vidrio, hover en Linux
  lib/store.svelte.js      estado compartido; Rust empuja eventos
  lib/Pill|Home|Claude|Music|Focus|Notes|System|Settings.svelte
src-tauri/src/
  main.rs                  arranque, bandeja, CLI (hook / hooks install)
  hit.rs                   posición, clic a través, modo fantasma, ventanas maximizadas
  focus.rs                 temporizador
  claude/                  sesiones de Claude Code, canal local, instalador de hooks
  media/                   música: windows.rs (SMTC), linux.rs (MPRIS)
  notes.rs                 guardar / cargar la nota
  settings.rs              ajustes, inicio automático
src-tauri/hook/            island-hook: cliente mínimo que llama Claude Code
src-tauri/windows/hooks.nsh  limpieza al desinstalar (Windows)
  sys.rs                   estadísticas del sistema, RAM de la isla
ci/                        compilación y prueba de humo de Linux en Docker
```

`ISLAND_PEEK=home,claude,music,focus,notes,sys` abre cada panel al iniciar (útil para capturas).
