//! `zero-conformidade` — verificador de referência e gerador dos vetores.
//!
//! ```text
//! zero-conformidade verificar [DIR]   executa os vetores (padrão: conformance)
//! zero-conformidade gerar DIR         gera novos vetores a partir desta implementação
//! ```

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = args.get(1).map_or("conformance", String::as_str);
    match args.first().map(String::as_str) {
        Some("verificar") => {
            let r = rz_conformance::verify::verify(Path::new(dir));
            for (file, ok, total) in &r.suites {
                println!("{file:<18} {ok:>4}/{total:<4}");
            }
            for f in &r.failures {
                println!("FALHA {f}");
            }
            if r.ok() {
                println!("conforme: todos os vetores coincidem");
                ExitCode::SUCCESS
            } else {
                println!("NÃO conforme: {} falha(s)", r.failures.len());
                ExitCode::FAILURE
            }
        }
        Some("gerar") if args.len() == 2 => {
            match rz_conformance::generate::generate(Path::new(dir)) {
                Ok(()) => {
                    println!("vetores gerados em {dir}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("erro: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        _ => {
            eprintln!("uso: zero-conformidade (verificar [DIR] | gerar DIR)");
            ExitCode::FAILURE
        }
    }
}
