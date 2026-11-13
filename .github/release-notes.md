## Descargas

| Sistema | Archivo |
| --- | --- |
| Windows 10/11 | `DynamicIsland_{{VERSION}}_x64-setup.exe`: recomendado; instala por usuario, sin permisos de administrador |
| Windows 10/11 (portable) | `DynamicIsland_{{VERSION}}_x64-portable.zip`: **si tu empresa bloquea las descargas de .exe**; descomprime y abre `dynamic-island.exe`, sin instalar ni pedir administrador |
| Windows 10/11 (MSI) | `DynamicIsland_{{VERSION}}_x64.msi`: para todo el equipo; pide administrador, sirve para Intune / GPO |
| Debian / Ubuntu | `dynamic-island_{{VERSION}}_amd64.deb`: `sudo apt install ./dynamic-island_{{VERSION}}_amd64.deb` |
| Fedora | `dynamic-island-{{VERSION}}-1.x86_64.rpm`: `sudo dnf install ./dynamic-island-{{VERSION}}-1.x86_64.rpm` |

Los instaladores se compilan automáticamente en GitHub y aparecen aquí unos minutos después de publicar la versión.

- **Windows:** usa solo una de las tres opciones. No están firmados; si SmartScreen avisa: "Más información" → "Ejecutar de todas formas". Para actualizar, instala encima de la versión anterior.
- **Linux:** la isla corre por X11/XWayland. Antes de desinstalar, corre `dynamic-island hooks uninstall` si conectaste Claude Code.

Para conectar Claude Code: en la isla, sección Claude → Conectar.
