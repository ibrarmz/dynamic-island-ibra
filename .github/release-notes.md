## Descargas

| Sistema | Archivo |
| --- | --- |
| Windows 10/11 | `DynamicIsland_{{VERSION}}_x64-setup.exe`: instala por usuario, sin permisos de administrador |
| Debian / Ubuntu | `dynamic-island_{{VERSION}}_amd64.deb`: `sudo apt install ./dynamic-island_{{VERSION}}_amd64.deb` |
| Fedora | `dynamic-island-{{VERSION}}-1.x86_64.rpm`: `sudo dnf install ./dynamic-island-{{VERSION}}-1.x86_64.rpm` |

Los instaladores se compilan automáticamente en GitHub y aparecen aquí unos minutos después de publicar la versión.

- **Windows:** el instalador no está firmado; si SmartScreen avisa: "Más información" → "Ejecutar de todas formas". Para actualizar, instala encima de la versión anterior.
- **Linux:** la isla corre por X11/XWayland. Antes de desinstalar, corre `dynamic-island hooks uninstall` si conectaste Claude Code.

Para conectar Claude Code: en la isla, sección Claude → Conectar.
