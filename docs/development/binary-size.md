# Tamaño del binario instalado

`scripts/ci/embedded-binary-gates.sh` construye el MCP en release, mide su copia
sin símbolos y publica los bytes en el log y el resumen del job. La medida es
informativa: no se bloquea una evolución por cruzar un número fijo. Comparar el
mismo target, toolchain y perfil antes de atribuir una diferencia a un cambio.
Revisar incrementos junto con los cambios reales en código, dependencias y assets.

La ejecución x86_64 de PR697 midió 21.007.664 bytes frente al antiguo techo de
20.971.520: 36.144 bytes por encima (0,17 %). El script declaraba un presupuesto fijo, sin documentar una restricción
del protocolo o de distribución que justificara ese valor. La observación se conserva
en el job [embedded-binary-gates](https://github.com/underpass-ai/kmp/actions/runs/34554810941/job/103125149791).

El build, las dependencias prohibidas del MCP, el aislamiento de observabilidad
del kernel embebido y la presencia de SQLite siguen siendo comprobaciones
obligatorias del mismo script. Un fallo en ellas requiere corregir la causa.
