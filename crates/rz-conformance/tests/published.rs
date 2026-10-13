//! A implementação de referência passa nos vetores publicados em
//! `conformance/`, e o gerador produz vetores que ela também passa.

use std::path::Path;

use rz_conformance::{generate, verify};

fn published() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance")
}

#[test]
fn reference_passes_published_vectors() {
    let r = verify::verify(&published());
    assert!(r.ok(), "falhas: {:#?}", r.failures);
    assert_eq!(r.suites.len(), rz_conformance::SUITES.len());
}

#[test]
fn generated_vectors_verify() {
    let dir = std::env::temp_dir().join(format!("rz-conformidade-{}", std::process::id()));
    generate::generate(&dir).unwrap_or_else(|e| panic!("{e}"));
    let r = verify::verify(&dir);
    assert!(r.ok(), "falhas: {:#?}", r.failures);
}

/// Um vetor alterado é detectado: a suíte não passa por acaso.
#[test]
fn tampered_vector_fails() {
    let dir = std::env::temp_dir().join(format!("rz-conformidade-alt-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
    for (file, _) in rz_conformance::SUITES {
        std::fs::copy(published().join(file), dir.join(file)).unwrap_or_else(|e| panic!("{e}"));
    }
    let path = dir.join("cadeia.txt");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{e}"));
    let pos = text
        .find("raiz_do_estado = ")
        .unwrap_or_else(|| panic!("campo"))
        + 17;
    let mut b = text.into_bytes();
    b[pos] = if b[pos] == b'0' { b'1' } else { b'0' };
    std::fs::write(&path, b).unwrap_or_else(|e| panic!("{e}"));
    let r = verify::verify(&dir);
    assert!(!r.ok());
}
