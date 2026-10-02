//! Armazenamento local de chaves (`SPEC-ID-001`).
//!
//! DEVNET: a semente é gravada em hexadecimal, em arquivo criado com
//! permissão `0600` em sistemas Unix. Armazenamento cifrado está **A DEFINIR**
//! (`spec/CRYPTOGRAPHY.md §10`). Nunca versione arquivos de chave.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use crate::{hex, SecretKey};

/// Grava a chave em um arquivo **novo**; falha se o arquivo já existir, para
/// nunca sobrescrever uma chave por engano.
pub fn save(path: &Path, key: &SecretKey) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            fs::create_dir_all(dir)?;
        }
    }
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(path)?;
    writeln!(f, "{}", hex::encode(&key.seed()))?;
    f.sync_all()
}

pub fn load(path: &Path) -> io::Result<SecretKey> {
    let text = fs::read_to_string(path)?;
    let seed = hex::decode_array::<32>(text.trim())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "arquivo de chave inválido"))?;
    Ok(SecretKey::from_seed(seed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load_and_no_overwrite() {
        let dir = std::env::temp_dir().join(format!("rz-keyfile-{}", std::process::id()));
        let path = dir.join("k.key");
        let _ = fs::remove_file(&path);
        let k = SecretKey::generate();
        save(&path, &k).unwrap();
        assert_eq!(load(&path).unwrap().public_key(), k.public_key());
        assert!(save(&path, &SecretKey::generate()).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        fs::remove_dir_all(dir).unwrap();
    }
}
