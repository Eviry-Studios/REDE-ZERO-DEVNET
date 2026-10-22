//! Conteúdo da Exonet endereçado por hash (`spec/CONTENT.md`, ADR-0017).
//!
//! Interfaces, páginas e arquivos de Comunidades e aplicações ficam fora da
//! cadeia (Manifesto §6). A cadeia guarda só identificadores: o `target` de um
//! nome `zero://` ou o `frontend` do manifesto de uma Comunidade. Qualquer
//! cópia, vinda de qualquer Node, é verificada contra esse identificador.
//!
//! Um objeto é dividido em pedaços de [`CHUNK_SIZE`]. O identificador
//! compromete o tamanho e o hash de cada pedaço, de modo que cada pedaço é
//! verificado ao chegar, sem confiar em quem o enviou.

use rz_codec::{Decode, DecodeError, Decoder, Encode, Encoder};
use rz_crypto::{hash, Hash32};

/// Contexto do identificador de um objeto de conteúdo.
pub const CONTENT_ID: &str = "rede-zero/content/v1";
/// Contexto do hash de um pedaço.
pub const CONTENT_CHUNK: &str = "rede-zero/content-chunk/v1";

/// Contexto da identidade por site derivada pela interface (`spec/CONTENT.md §6`).
pub const EXONET_IDENTITY: &str = "rede-zero/exonet-identity/v1";
/// Contexto das assinaturas de autenticação em publicações da Exonet.
pub const EXONET_LOGIN: &str = "rede-zero/exonet-login/v1";

/// Tamanho de cada pedaço (o último pode ser menor).
pub const CHUNK_SIZE: usize = 1 << 20;
/// Número máximo de pedaços por objeto.
pub const MAX_CHUNKS: usize = 16;
/// Tamanho máximo de um objeto (16 MiB).
pub const MAX_OBJECT: usize = CHUNK_SIZE * MAX_CHUNKS;
/// Tamanho máximo de um manifesto de Comunidade.
pub const MAX_MANIFEST: usize = 64 * 1024;

/// Arquivos por pacote.
pub const MAX_FILES: usize = 1024;
pub const MAX_PATH: usize = 256;
pub const MAX_MIME: usize = 128;
const MAX_DESCRIPTION: usize = 1024;

/// Descrição verificável de um objeto: tamanho e hash de cada pedaço.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentInfo {
    pub len: u64,
    pub chunks: Vec<Hash32>,
}

impl ContentInfo {
    /// `id = H("rede-zero/content/v1", enc(info))`.
    pub fn id(&self) -> Hash32 {
        hash(CONTENT_ID, &self.to_canonical_bytes())
    }

    /// Coerência entre tamanho e número de pedaços.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.len == 0 || self.len > MAX_OBJECT as u64 {
            return Err("tamanho de objeto fora dos limites");
        }
        if self.chunks.len() != chunk_count(self.len) {
            return Err("número de pedaços incoerente com o tamanho");
        }
        Ok(())
    }

    /// Tamanho esperado do pedaço `i`.
    pub fn chunk_len(&self, i: usize) -> Option<usize> {
        if i >= self.chunks.len() {
            return None;
        }
        let start = i * CHUNK_SIZE;
        Some((self.len as usize - start).min(CHUNK_SIZE))
    }

    /// Verifica um pedaço recebido.
    pub fn check_chunk(&self, i: usize, data: &[u8]) -> bool {
        self.chunk_len(i) == Some(data.len()) && chunk_hash(data) == self.chunks[i]
    }
}

impl Encode for ContentInfo {
    fn encode(&self, e: &mut Encoder) {
        e.u64(self.len).list(&self.chunks);
    }
}

impl Decode for ContentInfo {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        let info = Self {
            len: d.u64()?,
            chunks: d.list(MAX_CHUNKS)?,
        };
        info.validate().map_err(DecodeError::InvalidValue)?;
        Ok(info)
    }
}

fn chunk_count(len: u64) -> usize {
    (len as usize).div_ceil(CHUNK_SIZE)
}

pub fn chunk_hash(data: &[u8]) -> Hash32 {
    hash(CONTENT_CHUNK, data)
}

/// Descreve um objeto. Erro se vazio ou acima de [`MAX_OBJECT`].
pub fn describe(bytes: &[u8]) -> Result<ContentInfo, &'static str> {
    if bytes.is_empty() || bytes.len() > MAX_OBJECT {
        return Err("tamanho de objeto fora dos limites");
    }
    Ok(ContentInfo {
        len: bytes.len() as u64,
        chunks: bytes.chunks(CHUNK_SIZE).map(chunk_hash).collect(),
    })
}

/// Verifica um objeto completo contra seu identificador.
pub fn verify(id: &Hash32, bytes: &[u8]) -> bool {
    describe(bytes).is_ok_and(|info| info.id() == *id)
}

/// Um arquivo de um pacote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BundleFile {
    pub path: String,
    pub mime: String,
    pub data: Vec<u8>,
}

/// Pacote de arquivos (uma interface, um site, uma aplicação). Os caminhos
/// estão em ordem estritamente crescente, o que torna a codificação única.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Bundle {
    pub files: Vec<BundleFile>,
}

impl Bundle {
    /// Monta um pacote, ordenando os arquivos e validando caminhos.
    pub fn new(mut files: Vec<BundleFile>) -> Result<Self, &'static str> {
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let b = Self { files };
        b.validate()?;
        Ok(b)
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.files.is_empty() || self.files.len() > MAX_FILES {
            return Err("número de arquivos fora dos limites");
        }
        for (i, f) in self.files.iter().enumerate() {
            check_path(&f.path)?;
            check_mime(&f.mime)?;
            if i > 0 && self.files[i - 1].path >= f.path {
                return Err("caminhos repetidos ou fora de ordem");
            }
        }
        Ok(())
    }

    pub fn get(&self, path: &str) -> Option<&BundleFile> {
        self.files
            .binary_search_by(|f| f.path.as_str().cmp(path))
            .ok()
            .map(|i| &self.files[i])
    }
}

impl Encode for BundleFile {
    fn encode(&self, e: &mut Encoder) {
        e.str(&self.path).str(&self.mime).bytes(&self.data);
    }
}

impl Decode for BundleFile {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            path: d.str(MAX_PATH)?,
            mime: d.str(MAX_MIME)?,
            data: d.bytes(MAX_OBJECT)?,
        })
    }
}

impl Encode for Bundle {
    fn encode(&self, e: &mut Encoder) {
        e.list(&self.files);
    }
}

impl Decode for Bundle {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        let b = Self {
            files: d.list(MAX_FILES)?,
        };
        b.validate().map_err(DecodeError::InvalidValue)?;
        Ok(b)
    }
}

/// Caminho relativo, sem `..`, sem barra inicial ou final, só `[A-Za-z0-9._/-]`.
pub fn check_path(p: &str) -> Result<(), &'static str> {
    if p.is_empty() || p.len() > MAX_PATH {
        return Err("caminho vazio ou longo demais");
    }
    if !p
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-' | b'/'))
    {
        return Err("caractere inválido no caminho");
    }
    if p.split('/')
        .any(|seg| seg.is_empty() || seg == "." || seg == "..")
    {
        return Err("segmento de caminho inválido");
    }
    Ok(())
}

fn check_mime(m: &str) -> Result<(), &'static str> {
    let ok = !m.is_empty()
        && m.len() <= MAX_MIME
        && m.contains('/')
        && m.bytes().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, b'/' | b'.' | b'+' | b'-' | b';' | b'=' | b' ')
        });
    if ok {
        Ok(())
    } else {
        Err("tipo de mídia inválido")
    }
}

/// Tipo de mídia pela extensão (usado ao empacotar um diretório).
pub fn mime_for(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "txt" | "md" => "text/plain; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

/// Manifesto estruturado de uma Comunidade (`SPEC §60`). Seu hash com o
/// contexto `COMMUNITY_MANIFEST` é o `manifest_hash` registrado na cadeia.
/// Um manifesto que não decodifica neste formato continua válido para o
/// protocolo; as interfaces só não conseguem abrir a Comunidade.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub name: String,
    pub version: u32,
    pub description: String,
    /// Pacote da interface (objeto de conteúdo).
    pub frontend: Option<Hash32>,
    /// Módulo de lógica para o Exonet Runtime (`spec/RUNTIME.md`).
    pub module: Option<Hash32>,
}

/// Prefixo que identifica um manifesto estruturado.
pub const MANIFEST_MAGIC: &[u8; 8] = b"RZMANIF1";

impl Manifest {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = MANIFEST_MAGIC.to_vec();
        out.extend(self.to_canonical_bytes());
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let rest = bytes.strip_prefix(MANIFEST_MAGIC.as_slice())?;
        Self::from_canonical_bytes(rest).ok()
    }

    pub fn hash_of(bytes: &[u8]) -> Hash32 {
        hash(crate::community::COMMUNITY_MANIFEST, bytes)
    }
}

impl Encode for Manifest {
    fn encode(&self, e: &mut Encoder) {
        e.str(&self.name)
            .u32(self.version)
            .str(&self.description)
            .option(&self.frontend)
            .option(&self.module);
    }
}

impl Decode for Manifest {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            name: d.str(crate::community::MAX_NAME_LEN)?,
            version: d.u32()?,
            description: d.str(MAX_DESCRIPTION)?,
            frontend: d.option()?,
            module: d.option()?,
        })
    }
}

/// Chave de identidade de uma pessoa **num site específico** (`spec/CONTENT.md §6`).
///
/// Derivada da semente da Wallet e do endereço canônico do site: cada site vê
/// uma chave diferente, e sites distintos não conseguem correlacionar a mesma
/// pessoa. Nenhuma das chaves revela o endereço da Wallet.
pub fn site_secret(seed: &[u8; 32], address: &str) -> rz_crypto::SecretKey {
    let mut data = seed.to_vec();
    data.extend_from_slice(address.as_bytes());
    rz_crypto::SecretKey::from_seed(hash(EXONET_IDENTITY, &data).0)
}

/// Mensagem assinada numa autenticação: `str(endereço do site) ‖ str(mensagem)`.
pub fn login_payload(address: &str, message: &str) -> Vec<u8> {
    let mut e = Encoder::new();
    e.str(address).str(message);
    e.into_bytes()
}

/// Verificação de uma autenticação, para uso pelas próprias publicações.
pub fn verify_login(
    key: &rz_crypto::PublicKey,
    network_id: &str,
    address: &str,
    message: &str,
    signature: &rz_crypto::Signature,
) -> bool {
    key.verify(
        EXONET_LOGIN,
        network_id,
        &login_payload(address, message),
        signature,
    )
    .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunked_identity_and_verification() {
        let data: Vec<u8> = (0..(CHUNK_SIZE * 2 + 10)).map(|i| i as u8).collect();
        let info = describe(&data).unwrap();
        assert_eq!(info.chunks.len(), 3);
        assert!(info.validate().is_ok());
        assert!(verify(&info.id(), &data));
        assert!(info.check_chunk(2, &data[CHUNK_SIZE * 2..]));
        assert!(!info.check_chunk(1, &data[CHUNK_SIZE * 2..]));
        let mut bad = data.clone();
        bad[5] ^= 1;
        assert!(!verify(&info.id(), &bad));
        assert!(!info.check_chunk(0, &bad[..CHUNK_SIZE]));
        // Informação incoerente não decodifica.
        let mut forged = info.clone();
        forged.len = 5;
        assert!(ContentInfo::from_canonical_bytes(&forged.to_canonical_bytes()).is_err());
        assert!(describe(&[]).is_err());
        assert!(describe(&vec![0u8; MAX_OBJECT + 1]).is_err());
    }

    #[test]
    fn bundle_paths_and_canonical_order() {
        let f = |p: &str| BundleFile {
            path: p.into(),
            mime: mime_for(p).into(),
            data: p.as_bytes().to_vec(),
        };
        let b = Bundle::new(vec![f("index.html"), f("css/a.css")]).unwrap();
        assert_eq!(b.files[0].path, "css/a.css");
        assert_eq!(b.get("index.html").unwrap().data, b"index.html");
        assert!(b.get("nada").is_none());
        let back = Bundle::from_canonical_bytes(&b.to_canonical_bytes()).unwrap();
        assert_eq!(back, b);
        for bad in ["../x", "/x", "a//b", "a/./b", "a b", "x/", ""] {
            assert!(check_path(bad).is_err(), "{bad}");
        }
        assert!(Bundle::new(vec![f("a"), f("a")]).is_err());
        // Ordem diferente na codificação é rejeitada.
        let mut e = Encoder::new();
        e.list(&[f("b"), f("a")]);
        assert!(Bundle::from_canonical_bytes(&e.into_bytes()).is_err());
    }

    #[test]
    fn manifest_roundtrip() {
        let m = Manifest {
            name: "cientistas".into(),
            version: 2,
            description: "Comunidade de ciência".into(),
            frontend: Some(Hash32([7; 32])),
            module: None,
        };
        let bytes = m.to_bytes();
        assert_eq!(Manifest::from_bytes(&bytes), Some(m));
        assert_eq!(Manifest::from_bytes(b"texto livre"), None);
    }

    #[test]
    fn site_identities_are_unlinkable_and_verifiable() {
        let seed = [9u8; 32];
        let a = site_secret(&seed, "zero://banco.app");
        let b = site_secret(&seed, "zero://loja.market");
        assert_ne!(a.public_key(), b.public_key());
        assert_eq!(
            a.public_key(),
            site_secret(&seed, "zero://banco.app").public_key()
        );
        let sig = a.sign(
            EXONET_LOGIN,
            "net",
            &login_payload("zero://banco.app", "desafio"),
        );
        assert!(verify_login(
            &a.public_key(),
            "net",
            "zero://banco.app",
            "desafio",
            &sig
        ));
        // Assinatura de um site não vale em outro.
        assert!(!verify_login(
            &a.public_key(),
            "net",
            "zero://loja.market",
            "desafio",
            &sig
        ));
        assert!(!verify_login(
            &a.public_key(),
            "outra",
            "zero://banco.app",
            "desafio",
            &sig
        ));
    }
}
