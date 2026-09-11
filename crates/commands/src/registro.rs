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
        Comando { id: "app.salir", descripcion: "Aplicación: Salir del editor" },
        Comando { id: "editor.deshacer", descripcion: "Editor: Deshacer" },
        Comando { id: "editor.rehacer", descripcion: "Editor: Rehacer" },
        Comando { id: "panel.alternar_lateral", descripcion: "Ver: Alternar explorador de archivos" },
        Comando { id: "config.recargar", descripcion: "Configuración: Recargar" },
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
