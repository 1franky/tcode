/// Un comando disponible en la paleta de comandos (`Ctrl+Shift+P`,
/// PLAN.md §4). `id` es el mismo identificador que usa `tcode-keymap`
/// (`"archivo.guardar"`); `descripcion` es lo que se busca y se muestra,
/// con el formato `Categoría: Acción` (igual que VSCode) para que sea
/// fácil de escanear visualmente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Comando {
    pub id: &'static str,
    pub descripcion: &'static str,
}

/// Comandos que tiene sentido invocar *por nombre* desde la paleta — no
/// todo lo que existe en el keymap encaja aquí: mover el cursor letra a
/// letra o insertar un salto de línea no son "acciones" que alguien busca
/// por nombre, son atajos de uso continuo. Cuando se añade un comando
/// nuevo con sentido de paleta (a `ejecutar_comando` en `app`), hay que
/// añadirlo aquí también — no hay (todavía) un registro dinámico.
pub fn comandos_disponibles() -> &'static [Comando] {
    &[
        Comando { id: "archivo.guardar", descripcion: "Archivo: Guardar" },
        Comando { id: "archivo.guardar_como", descripcion: "Archivo: Guardar como..." },
        Comando { id: "app.salir", descripcion: "Aplicación: Salir del editor" },
        Comando { id: "editor.deshacer", descripcion: "Editor: Deshacer" },
        Comando { id: "editor.rehacer", descripcion: "Editor: Rehacer" },
        Comando { id: "panel.alternar_lateral", descripcion: "Ver: Alternar explorador de archivos" },
        Comando { id: "explorador.saltar", descripcion: "Ver: Saltar a un archivo (etiquetas de una tecla)" },
        Comando { id: "explorador.nuevo_archivo", descripcion: "Explorador: Nuevo archivo" },
        Comando { id: "explorador.nueva_carpeta", descripcion: "Explorador: Nueva carpeta" },
        Comando { id: "explorador.renombrar", descripcion: "Explorador: Renombrar selección" },
        Comando { id: "config.recargar", descripcion: "Configuración: Recargar" },
        Comando { id: "tema.seleccionar", descripcion: "Tema: Seleccionar (con preview en vivo)" },
        Comando { id: "tema.editor_visual", descripcion: "Tema: Editor visual (colores por código hex)" },
        Comando { id: "admin.abrir_panel", descripcion: "Panel de administración: Abrir" },
        Comando { id: "lsp.ver_logs", descripcion: "LSP: Ver logs de la sesión activa" },
        Comando { id: "markdown.alternar_preview", descripcion: "Markdown: Alternar preview" },
        Comando { id: "markdown.preview_solo", descripcion: "Markdown: Ver solo preview" },
        Comando { id: "csv.alternar_vista_tabla", descripcion: "CSV: Alternar vista de tabla" },
        Comando { id: "csv.ordenar", descripcion: "CSV: Ordenar por la columna actual (alterna asc/desc)" },
        Comando { id: "csv.filtrar", descripcion: "CSV: Filtrar por la columna actual" },
        Comando { id: "csv.quitar_filtro", descripcion: "CSV: Quitar filtro" },
        Comando { id: "csv.insertar_fila_debajo", descripcion: "CSV: Insertar fila debajo" },
        Comando { id: "csv.insertar_fila_arriba", descripcion: "CSV: Insertar fila arriba" },
        Comando { id: "csv.insertar_columna_derecha", descripcion: "CSV: Insertar columna a la derecha" },
        Comando { id: "csv.insertar_columna_izquierda", descripcion: "CSV: Insertar columna a la izquierda" },
        Comando { id: "csv.eliminar_fila", descripcion: "CSV: Eliminar fila" },
        Comando { id: "csv.eliminar_columna", descripcion: "CSV: Eliminar columna" },
        Comando { id: "csv.ensanchar_columna", descripcion: "CSV: Ensanchar columna" },
        Comando { id: "csv.angostar_columna", descripcion: "CSV: Angostar columna" },
        Comando { id: "csv.restablecer_ancho", descripcion: "CSV: Restablecer ancho automático de columna" },
        Comando {
            id: "cursor.seleccionar_siguiente_ocurrencia",
            descripcion: "Selección: Agregar la siguiente ocurrencia",
        },
        Comando { id: "cursor.seleccionar_todas_ocurrencias", descripcion: "Selección: Seleccionar todas las ocurrencias" },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_comandos_disponibles_tienen_id_y_descripcion_unicos() {
        let comandos = comandos_disponibles();
        assert!(!comandos.is_empty());

        let mut ids: Vec<&str> = comandos.iter().map(|c| c.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), comandos.len(), "hay ids de comando repetidos");

        assert!(comandos.iter().all(|c| !c.id.is_empty() && !c.descripcion.is_empty()));
    }
}
