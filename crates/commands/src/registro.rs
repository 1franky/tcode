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
        Comando { id: "config.recargar", descripcion: "Configuración: Recargar" },
        Comando { id: "tema.seleccionar", descripcion: "Tema: Seleccionar (con preview en vivo)" },
        Comando { id: "tema.editor_visual", descripcion: "Tema: Editor visual (colores por código hex)" },
        Comando { id: "admin.abrir_panel", descripcion: "Panel de administración: Abrir" },
        Comando { id: "markdown.alternar_preview", descripcion: "Markdown: Alternar preview" },
        Comando { id: "markdown.preview_solo", descripcion: "Markdown: Ver solo preview" },
        Comando { id: "csv.alternar_vista_tabla", descripcion: "CSV: Alternar vista de tabla" },
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
