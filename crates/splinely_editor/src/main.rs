mod app;
mod export;
mod raster;
mod viewer;

const RUNTIME: &str = include_str!("../../../runtime/src/splinely.js");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--help" | "-h") => {
            println!("Splinely {}\nUsage: splinely [project.sply | --export project.sply output.svg | --demo-export output.svg]\nWithout arguments, opens the desktop editor. Legacy JSON projects remain supported.", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("--version" | "-V") => {
            println!("Splinely {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("--export") if args.len() == 4 => {
            app::export_file(&args[2], &args[3])?;
            Ok(())
        }
        Some("--demo-export") if args.len() == 3 => {
            app::export_demo(&args[2])?;
            Ok(())
        }
        Some(path) if args.len() == 2 && !path.starts_with('-') => {
            run_editor(app::Editor::open_project_file(path)?)
        }
        Some(_) => {
            Err("Usage: splinely [project.sply | --export project.sply output.svg | --demo-export output.svg]".into())
        }
        None => {
            let mut editor = app::Editor::default();
            editor.check_recovery();
            run_editor(editor)
        }
    }
}

fn run_editor(editor: app::Editor) -> Result<(), Box<dyn std::error::Error>> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native("Splinely", options, Box::new(|_| Ok(Box::new(editor))))
        .map_err(|e| e.to_string().into())
}
