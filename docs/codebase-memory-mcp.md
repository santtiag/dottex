# codebase-memory-mcp — guía rápida

No hay comandos manuales que ejecutar: es una herramienta MCP que Claude usa
directamente desde el chat. Para indexar o consultar el código, simplemente
se le pide a Claude (ej: "indexá esta carpeta", "quién llama a X").

## Comandos principales

| Comando | Para qué sirve |
|---|---|
| `index_repository` | Escanea la carpeta y construye el grafo de código (funciones, clases, llamadas, imports, rutas HTTP, etc). Es el primer paso obligatorio. |
| `index_status` / `list_projects` | Ver si un proyecto ya está indexado y su estado. |
| `search_graph` | Buscar funciones/clases por nombre, patrón, o filtros (ej: código muerto, funciones muy conectadas). |
| `trace_path` | Rastrear quién llama a una función (`inbound`) o qué llama esa función (`outbound`). |
| `get_code_snippet` | Traer el código fuente exacto de un símbolo ya localizado. |
| `query_graph` | Consultas Cypher a mano para casos complejos (máx 200 filas). |
| `get_architecture` | Vista general de la estructura del proyecto. |
| `detect_changes` | Qué símbolos afecta el diff actual de git. |
| `search_code` | Búsqueda de texto tipo grep pero apoyada en el grafo. |

## Flujo normal

1. `index_repository(repo_path=...)` — una vez por proyecto (y de nuevo si
   cambia mucho el código).
2. Después, para explorar el código se usa `search_graph` / `trace_path` /
   `get_code_snippet` en vez de leer archivo por archivo.

## Edge types (relaciones en el grafo)

CALLS, HTTP_CALLS, ASYNC_CALLS, IMPORTS, DEFINES, DEFINES_METHOD, HANDLES,
IMPLEMENTS, OVERRIDE, USAGE, FILE_CHANGES_WITH, CONTAINS_FILE,
CONTAINS_FOLDER, CONTAINS_PACKAGE

## Ejemplos de Cypher (para `query_graph`)

```
MATCH (a)-[r:HTTP_CALLS]->(b) RETURN a.name, b.name, r.url_path, r.confidence LIMIT 20
MATCH (f:Function) WHERE f.name =~ '.*Handler.*' RETURN f.name, f.file_path
MATCH (a)-[r:CALLS]->(b) WHERE a.name = 'main' RETURN b.name
```
