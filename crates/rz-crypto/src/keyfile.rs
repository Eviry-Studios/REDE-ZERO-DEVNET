//! Armazenamento local de chaves (`SPEC-ID-001`, REQ-012,
//! `spec/CRYPTOGRAPHY.md §7`).
//!
//! Dois formatos:
//!
//! * **cifrado** (padrão para chaves novas): a semente é cifrada com
//!   XChaCha20-Poly1305 sob uma chave derivada da senha por Argon2id. O
//!   cabeçalho (versão, parâmetros e sal) é autenticado junto.
//! * **aberto** (legado da DEVNET): a semente em hexadecimal. Continua
//!   legível, para não perder chaves existentes, e pode ser convertido com
//!   [`encrypt_in_place`].
//!
//! Arquivos são criados com permissão `0600` em sistemas Unix e nunca
//! sobrescrevem um arquivo existente. Nunca versione arquivos de chave.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand_core::{OsRng, RngCore};
use zeroize::{Zeroize, Zeroizing};

use crate::{hex, SecretKey};

/// Primeira linha de um arquivo cifrado.
pub const MAGIC: &str = "rede-zero-chave-cifrada v1";
/// Senha mínima aceita ao cifrar.
pub const MIN_PASSPHRASE: usize = 8;

/// Parâmetros do Argon2id gravados no arquivo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KdfParams {
    /// Memória em KiB.
    pub m_kib: u32,
    pub t: u32,
    pub p: u32,
}

impl KdfParams {
    /// Padrão: 64 MiB, 3 passadas, 1 via (RFC 9106 §4, segunda opção).
    pub const DEFAULT: Self = Self {
        m_kib: 64 * 1024,
        t: 3,
        p: 1,
    };

    /// Limites aceitos ao ler: evitam que um arquivo malicioso peça memória
    /// ou tempo absurdos, e recusam parâmetros fracos demais.
    fn check(&self) -> io::Result<()> {
        if !(8 * 1024..=4 * 1024 * 1024).contains(&self.m_kib)
            || !(1..=64).contains(&self.t)
            || !(1..=16).contains(&self.p)
        {
            return Err(invalid("parâmetros de derivação fora dos limites"));
        }
        Ok(())
    }

    fn line(&self) -> String {
        format!("kdf argon2id m={} t={} p={}", self.m_kib, self.t, self.p)
    }

    fn parse(line: &str) -> io::Result<Self> {
        let rest = line
            .strip_prefix("kdf argon2id ")
            .ok_or_else(|| invalid("linha kdf ausente"))?;
        let mut m = None;
        let mut t = None;
        let mut p = None;
        for part in rest.split(' ') {
            let (k, v) = part.split_once('=').ok_or_else(|| invalid("kdf"))?;
            let v: u32 = v.parse().map_err(|_| invalid("kdf"))?;
            match k {
                "m" => m = Some(v),
                "t" => t = Some(v),
                "p" => p = Some(v),
                _ => return Err(invalid("kdf")),
            }
        }
        let params = Self {
            m_kib: m.ok_or_else(|| invalid("kdf"))?,
            t: t.ok_or_else(|| invalid("kdf"))?,
            p: p.ok_or_else(|| invalid("kdf"))?,
        };
        params.check()?;
        Ok(params)
    }
}

fn invalid(what: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("arquivo de chave inválido: {what}"),
    )
}

fn derive(pass: &[u8], salt: &[u8; 16], k: &KdfParams) -> io::Result<Zeroizing<[u8; 32]>> {
    let params = Params::new(k.m_kib, k.t, k.p, Some(32)).map_err(|_| invalid("kdf"))?;
    let mut out = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(pass, salt, out.as_mut())
        .map_err(|_| invalid("kdf"))?;
    Ok(out)
}

fn create_new(path: &Path) -> io::Result<fs::File> {
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
    opts.open(path)
}

/// Texto do arquivo cifrado.
fn encrypted_text(key: &SecretKey, pass: &str, k: &KdfParams) -> io::Result<String> {
    if pass.chars().count() < MIN_PASSPHRASE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("senha curta demais (mínimo {MIN_PASSPHRASE} caracteres)"),
        ));
    }
    k.check()?;
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 24];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce);
    let header = format!("{MAGIC}\n{}\nsal {}\n", k.line(), hex::encode(&salt));
    let dk = derive(pass.as_bytes(), &salt, k)?;
    let cipher = XChaCha20Poly1305::new(dk.as_ref().into());
    let seed = Zeroizing::new(key.seed());
    let ct = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: seed.as_ref(),
                aad: header.as_bytes(),
            },
        )
        .map_err(|_| invalid("cifragem"))?;
    Ok(format!(
        "{header}nonce {}\ncifra {}\n",
        hex::encode(&nonce),
        hex::encode(&ct)
    ))
}

/// Grava a chave **cifrada** em um arquivo novo (padrão de derivação).
pub fn save_encrypted(path: &Path, key: &SecretKey, pass: &str) -> io::Result<()> {
    save_encrypted_with(path, key, pass, &KdfParams::DEFAULT)
}

pub fn save_encrypted_with(
    path: &Path,
    key: &SecretKey,
    pass: &str,
    k: &KdfParams,
) -> io::Result<()> {
    let text = encrypted_text(key, pass, k)?;
    let mut f = create_new(path)?;
    f.write_all(text.as_bytes())?;
    f.sync_all()
}

/// Grava a chave **em aberto** num arquivo novo. Só para DEVNET e testes;
/// chaves com valor devem usar [`save_encrypted`].
pub fn save(path: &Path, key: &SecretKey) -> io::Result<()> {
    let mut f = create_new(path)?;
    let mut line = Zeroizing::new(hex::encode(&key.seed()));
    line.push('\n');
    f.write_all(line.as_bytes())?;
    f.sync_all()
}

/// O arquivo está cifrado?
pub fn is_encrypted(path: &Path) -> io::Result<bool> {
    let text = fs::read_to_string(path)?;
    Ok(text.lines().next() == Some(MAGIC))
}

/// Lê uma chave. `pass` só é chamado se o arquivo estiver cifrado; uma
/// senha errada e um arquivo adulterado dão o mesmo erro.
pub fn load_with(path: &Path, pass: impl FnOnce() -> io::Result<String>) -> io::Result<SecretKey> {
    let text = Zeroizing::new(fs::read_to_string(path)?);
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("").trim();
    if first != MAGIC {
        let seed = Zeroizing::new(
            hex::decode_array::<32>(first).ok_or_else(|| invalid("formato desconhecido"))?,
        );
        if lines.any(|l| !l.trim().is_empty()) {
            return Err(invalid("conteúdo após a semente"));
        }
        return Ok(SecretKey::from_seed(*seed));
    }
    let kdf_line = lines.next().ok_or_else(|| invalid("kdf"))?;
    let k = KdfParams::parse(kdf_line)?;
    let salt_hex = lines
        .next()
        .and_then(|l| l.strip_prefix("sal "))
        .ok_or_else(|| invalid("sal"))?;
    let salt = hex::decode_array::<16>(salt_hex).ok_or_else(|| invalid("sal"))?;
    let nonce = lines
        .next()
        .and_then(|l| l.strip_prefix("nonce "))
        .and_then(hex::decode_array::<24>)
        .ok_or_else(|| invalid("nonce"))?;
    let ct = lines
        .next()
        .and_then(|l| l.strip_prefix("cifra "))
        .and_then(hex::decode)
        .ok_or_else(|| invalid("cifra"))?;
    if lines.any(|l| !l.trim().is_empty()) {
        return Err(invalid("conteúdo após a cifra"));
    }
    // O cabeçalho autenticado é reconstruído na forma canônica: qualquer
    // alteração de parâmetros ou sal invalida a etiqueta.
    let header = format!("{MAGIC}\n{}\nsal {}\n", k.line(), hex::encode(&salt));
    let mut pass = pass()?;
    let dk = derive(pass.as_bytes(), &salt, &k);
    pass.zeroize();
    let dk = dk?;
    let cipher = XChaCha20Poly1305::new(dk.as_ref().into());
    let seed = Zeroizing::new(
        cipher
            .decrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &ct,
                    aad: header.as_bytes(),
                },
            )
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "senha incorreta ou arquivo de chave adulterado",
                )
            })?,
    );
    let seed: [u8; 32] = seed
        .as_slice()
        .try_into()
        .map_err(|_| invalid("tamanho da semente"))?;
    Ok(SecretKey::from_seed(seed))
}

/// Lê uma chave em aberto; falha com orientação se estiver cifrada.
pub fn load(path: &Path) -> io::Result<SecretKey> {
    load_with(path, || {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "arquivo de chave cifrado: informe a senha",
        ))
    })
}

/// Converte um arquivo em aberto para o formato cifrado, sem deixar cópia
/// em aberto: grava o novo arquivo ao lado, confere e substitui.
pub fn encrypt_in_place(path: &Path, pass: &str) -> io::Result<()> {
    encrypt_in_place_with(path, pass, &KdfParams::DEFAULT)
}

pub fn encrypt_in_place_with(path: &Path, pass: &str, k: &KdfParams) -> io::Result<()> {
    if is_encrypted(path)? {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "o arquivo já está cifrado",
        ));
    }
    let key = load(path)?;
    let tmp = path.with_extension("cifrando");
    let _ = fs::remove_file(&tmp);
    save_encrypted_with(&tmp, &key, pass, k)?;
    let back = load_with(&tmp, || Ok(pass.to_string()))?;
    if back.public_key() != key.public_key() {
        let _ = fs::remove_file(&tmp);
        return Err(invalid("verificação após cifrar"));
    }
    // Sobrescreve o conteúdo em aberto antes de substituir o arquivo.
    if let Ok(len) = fs::metadata(path).map(|m| m.len()) {
        if let Ok(mut f) = OpenOptions::new().write(true).open(path) {
            let _ = f.write_all(&vec![0u8; len as usize]);
            let _ = f.sync_all();
        }
    }
    fs::rename(&tmp, path)
}

/// Variável de ambiente com a senha (para serviços sem terminal).
pub const PASSPHRASE_ENV: &str = "RZ_KEY_PASSPHRASE";

/// Obtém a senha, nesta ordem: primeira linha de `file`, a variável
/// [`PASSPHRASE_ENV`] ou uma linha da entrada padrão (com aviso em `stderr`,
/// pois a digitação fica visível).
pub fn passphrase(file: Option<&Path>) -> io::Result<String> {
    if let Some(f) = file {
        let text = Zeroizing::new(fs::read_to_string(f)?);
        return Ok(text.lines().next().unwrap_or("").to_string());
    }
    if let Ok(v) = std::env::var(PASSPHRASE_ENV) {
        return Ok(v);
    }
    eprint!("senha da chave (a digitação aparece na tela; prefira --passphrase-file): ");
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    if line.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "senha não informada (use --passphrase-file ou RZ_KEY_PASSPHRASE)",
        ));
    }
    let pass = line.trim_end_matches(['\n', '\r']).to_string();
    line.zeroize();
    Ok(pass)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parâmetros mínimos aceitos, para testes rápidos.
    const FAST: KdfParams = KdfParams {
        m_kib: 8 * 1024,
        t: 1,
        p: 1,
    };

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("rz-keyfile-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn save_load_and_no_overwrite() {
        let d = dir("aberto");
        let path = d.join("k.key");
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
        fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn encrypted_roundtrip_wrong_pass_and_tamper() {
        let d = dir("cifrado");
        let path = d.join("k.key");
        let k = SecretKey::generate();
        assert!(save_encrypted_with(&path, &k, "curta", &FAST).is_err());
        save_encrypted_with(&path, &k, "senha longa de teste", &FAST).unwrap();
        assert!(is_encrypted(&path).unwrap());
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains(&hex::encode(&k.seed())), "semente em aberto");
        assert!(load(&path).is_err(), "sem senha");
        let back = load_with(&path, || Ok("senha longa de teste".into())).unwrap();
        assert_eq!(back.public_key(), k.public_key());
        assert_eq!(
            load_with(&path, || Ok("senha errada!!".into()))
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
        // Parâmetros rebaixados ou sal trocado: a etiqueta não confere.
        let weaker = text.replace("t=1", "t=2");
        let p2 = d.join("t.key");
        fs::write(&p2, weaker).unwrap();
        assert!(load_with(&p2, || Ok("senha longa de teste".into())).is_err());
        // Parâmetros absurdos são recusados antes de derivar.
        fs::write(&p2, text.replace("m=8192", "m=999999999")).unwrap();
        assert_eq!(
            load_with(&p2, || Ok("x".into())).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn plaintext_converted_in_place() {
        let d = dir("converter");
        let path = d.join("k.key");
        let k = SecretKey::generate();
        save(&path, &k).unwrap();
        encrypt_in_place_with(&path, "outra senha longa", &FAST).unwrap();
        assert!(is_encrypted(&path).unwrap());
        assert!(encrypt_in_place_with(&path, "outra senha longa", &FAST).is_err());
        let back = load_with(&path, || Ok("outra senha longa".into())).unwrap();
        assert_eq!(back.public_key(), k.public_key());
        assert!(!d.join("k.cifrando").exists());
        fs::remove_dir_all(d).unwrap();
    }
}
