//! Armazenamento local de conteúdo da Exonet (`spec/CONTENT.md`).
//!
//! O Node só hospeda objetos e manifestos **referenciados pelo estado** (alvo
//! de um nome `zero://` ou interface de uma Comunidade reconhecida), dentro
//! de uma cota local. Tudo é verificado contra o identificador ao chegar e
//! novamente ao carregar do disco.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rz_core::content::{describe, ContentInfo, Manifest, CHUNK_SIZE, MAX_MANIFEST, MAX_OBJECT};
use rz_crypto::Hash32;

/// Downloads simultâneos (cada um ocupa até 16 MiB de memória).
pub const MAX_DOWNLOADS: usize = 4;
/// Prazo para concluir um download.
pub const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);
/// Prazo sem nenhum pedaço novo: libera o lugar ocupado por quem anuncia e
/// não envia.
pub const DOWNLOAD_IDLE: Duration = Duration::from_secs(15);
/// Prazo para um pedido de conteúdo aos pares ser respondido.
pub const WANT_TIMEOUT: Duration = Duration::from_secs(20);

pub struct ContentStore {
    dir: PathBuf,
    objects: BTreeMap<Hash32, ContentInfo>,
    /// Manifestos guardados e o que cada um referencia (interface).
    manifests: BTreeMap<Hash32, Option<Hash32>>,
    used: u64,
    quota: u64,
}

impl ContentStore {
    /// Abre o diretório e reverifica cada arquivo. Arquivos inválidos são
    /// removidos (o disco também é fonte não confiável).
    pub fn open(dir: &Path, quota: u64) -> io::Result<Self> {
        let objects_dir = dir.join("objects");
        let manifests_dir = dir.join("manifests");
        fs::create_dir_all(&objects_dir)?;
        fs::create_dir_all(&manifests_dir)?;
        let mut s = Self {
            dir: dir.to_path_buf(),
            objects: BTreeMap::new(),
            manifests: BTreeMap::new(),
            used: 0,
            quota,
        };
        for entry in fs::read_dir(&objects_dir)? {
            let path = entry?.path();
            let ok = (|| {
                let id = Hash32::from_hex(path.file_name()?.to_str()?)?;
                if fs::metadata(&path).ok()?.len() > MAX_OBJECT as u64 {
                    return None;
                }
                let bytes = fs::read(&path).ok()?;
                let info = describe(&bytes).ok()?;
                (info.id() == id).then_some((id, info))
            })();
            match ok {
                Some((id, info)) => {
                    s.used += info.len;
                    s.objects.insert(id, info);
                }
                None => {
                    let _ = fs::remove_file(&path);
                }
            }
        }
        for entry in fs::read_dir(&manifests_dir)? {
            let path = entry?.path();
            let ok = (|| {
                let h = Hash32::from_hex(path.file_name()?.to_str()?)?;
                if fs::metadata(&path).ok()?.len() > MAX_MANIFEST as u64 {
                    return None;
                }
                let bytes = fs::read(&path).ok()?;
                (Manifest::hash_of(&bytes) == h).then_some((h, bytes))
            })();
            match ok {
                Some((h, bytes)) => {
                    s.used += bytes.len() as u64;
                    s.manifests
                        .insert(h, Manifest::from_bytes(&bytes).and_then(|m| m.frontend));
                }
                None => {
                    let _ = fs::remove_file(&path);
                }
            }
        }
        Ok(s)
    }

    fn object_path(&self, id: &Hash32) -> PathBuf {
        self.dir.join("objects").join(id.to_hex())
    }

    fn manifest_path(&self, h: &Hash32) -> PathBuf {
        self.dir.join("manifests").join(h.to_hex())
    }

    pub fn has(&self, id: &Hash32) -> bool {
        self.objects.contains_key(id)
    }

    pub fn info(&self, id: &Hash32) -> Option<ContentInfo> {
        self.objects.get(id).cloned()
    }

    pub fn fits(&self, len: u64) -> bool {
        self.used.saturating_add(len) <= self.quota
    }

    pub fn used(&self) -> u64 {
        self.used
    }

    /// Lê o pedaço `i` do disco.
    pub fn chunk(&self, id: &Hash32, i: usize) -> Option<Vec<u8>> {
        let info = self.objects.get(id)?;
        let len = info.chunk_len(i)?;
        let mut f = File::open(self.object_path(id)).ok()?;
        f.seek(SeekFrom::Start((i * CHUNK_SIZE) as u64)).ok()?;
        let mut buf = vec![0u8; len];
        f.read_exact(&mut buf).ok()?;
        info.check_chunk(i, &buf).then_some(buf)
    }

    /// Lê o objeto inteiro.
    pub fn object(&self, id: &Hash32) -> Option<Vec<u8>> {
        self.objects.get(id)?;
        let bytes = fs::read(self.object_path(id)).ok()?;
        rz_core::content::verify(id, &bytes).then_some(bytes)
    }

    /// Grava um objeto já verificado. `false` se exceder a cota.
    pub fn put_object(&mut self, info: ContentInfo, bytes: &[u8]) -> io::Result<bool> {
        let id = info.id();
        if self.has(&id) {
            return Ok(true);
        }
        if !self.fits(info.len) || !rz_core::content::verify(&id, bytes) {
            return Ok(false);
        }
        write_atomic(&self.object_path(&id), bytes)?;
        self.used += info.len;
        self.objects.insert(id, info);
        Ok(true)
    }

    pub fn manifest(&self, h: &Hash32) -> Option<Vec<u8>> {
        self.manifests.get(h)?;
        let bytes = fs::read(self.manifest_path(h)).ok()?;
        (Manifest::hash_of(&bytes) == *h).then_some(bytes)
    }

    pub fn has_manifest(&self, h: &Hash32) -> bool {
        self.manifests.contains_key(h)
    }

    /// Interface referenciada por um manifesto guardado.
    pub fn manifest_frontend(&self, h: &Hash32) -> Option<Hash32> {
        self.manifests.get(h).copied().flatten()
    }

    pub fn put_manifest(&mut self, h: Hash32, bytes: &[u8]) -> io::Result<bool> {
        if self.has_manifest(&h) {
            return Ok(true);
        }
        if bytes.len() > MAX_MANIFEST
            || Manifest::hash_of(bytes) != h
            || !self.fits(bytes.len() as u64)
        {
            return Ok(false);
        }
        write_atomic(&self.manifest_path(&h), bytes)?;
        self.used += bytes.len() as u64;
        self.manifests
            .insert(h, Manifest::from_bytes(bytes).and_then(|m| m.frontend));
        Ok(true)
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(tmp, path)
}

/// Um objeto em transferência, pedaço a pedaço.
pub struct Download {
    pub info: ContentInfo,
    pub chunks: Vec<Option<Vec<u8>>>,
    /// Conexão de origem: só ela pode enviar pedaços deste objeto.
    pub source: u64,
    pub started: Instant,
    pub last_progress: Instant,
}

impl Download {
    pub fn new(info: ContentInfo, source: u64) -> Self {
        let n = info.chunks.len();
        Self {
            info,
            chunks: vec![None; n],
            source,
            started: Instant::now(),
            last_progress: Instant::now(),
        }
    }

    pub fn complete(&self) -> bool {
        self.chunks.iter().all(Option::is_some)
    }

    pub fn assemble(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.info.len as usize);
        for c in self.chunks.iter().flatten() {
            out.extend_from_slice(c);
        }
        out
    }
}

/// Estado de conteúdo do Node.
pub struct ContentCtl {
    pub store: ContentStore,
    pub downloads: BTreeMap<Hash32, Download>,
    /// Objetos pedidos aos pares, com o instante do pedido.
    pub wanted: BTreeMap<Hash32, Instant>,
    /// Manifestos pedidos aos pares.
    pub wanted_manifests: BTreeMap<Hash32, Instant>,
}

impl ContentCtl {
    pub fn new(store: ContentStore) -> Self {
        Self {
            store,
            downloads: BTreeMap::new(),
            wanted: BTreeMap::new(),
            wanted_manifests: BTreeMap::new(),
        }
    }

    /// Descarta downloads e pedidos vencidos.
    pub fn expire(&mut self) {
        let now = Instant::now();
        self.downloads.retain(|_, d| {
            now.duration_since(d.started) < DOWNLOAD_TIMEOUT
                && now.duration_since(d.last_progress) < DOWNLOAD_IDLE
        });
        self.wanted
            .retain(|_, t| now.duration_since(*t) < WANT_TIMEOUT);
        self.wanted_manifests
            .retain(|_, t| now.duration_since(*t) < WANT_TIMEOUT);
    }

    /// Downloads em andamento vindos da conexão `source` (no máximo um).
    pub fn downloads_by(&self, source: u64) -> usize {
        self.downloads
            .values()
            .filter(|d| d.source == source)
            .count()
    }

    /// Bytes reservados por downloads em andamento.
    pub fn reserved(&self) -> u64 {
        self.downloads.values().map(|d| d.info.len).sum()
    }

    /// Interfaces referenciadas pelos manifestos indicados.
    pub fn frontends_of(&self, manifests: &[Hash32]) -> BTreeSet<Hash32> {
        manifests
            .iter()
            .filter_map(|m| self.store.manifest_frontend(m))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rz-content-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn store_verifies_and_reloads() {
        let dir = tmp("store");
        let data: Vec<u8> = (0..(CHUNK_SIZE + 77)).map(|i| (i % 251) as u8).collect();
        let info = describe(&data).unwrap();
        let id = info.id();
        {
            let mut s = ContentStore::open(&dir, 64 << 20).unwrap();
            assert!(s.put_object(info.clone(), &data).unwrap());
            assert_eq!(s.chunk(&id, 1).unwrap(), &data[CHUNK_SIZE..]);
            assert!(s.chunk(&id, 2).is_none());
            let m = Manifest {
                name: "x".into(),
                version: 1,
                description: String::new(),
                frontend: Some(id),
                module: None,
            }
            .to_bytes();
            let h = Manifest::hash_of(&m);
            assert!(!s.put_manifest(Hash32([1; 32]), &m).unwrap());
            assert!(s.put_manifest(h, &m).unwrap());
            assert_eq!(s.manifest_frontend(&h), Some(id));
        }
        // Arquivo adulterado no disco é descartado ao reabrir.
        let bad = describe(b"outro").unwrap().id();
        fs::write(dir.join("objects").join(bad.to_hex()), b"adulterado").unwrap();
        let s = ContentStore::open(&dir, 64 << 20).unwrap();
        assert!(s.has(&id));
        assert!(!s.has(&bad));
        assert_eq!(s.object(&id).unwrap(), data);
        // Cota.
        let mut small = ContentStore::open(&tmp("quota"), 10).unwrap();
        assert!(!small.put_object(info, &data).unwrap());
        let _ = fs::remove_dir_all(&dir);
    }
}
