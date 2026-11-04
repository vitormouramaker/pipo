// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Leitor de PDF isolado, usado pelo índice de busca nos arquivos.
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 3 && args[1] == "--pdf-text" {
        pipo_lib::pdf_text_cli(&args[2]);
        return;
    }
    pipo_lib::run()
}
