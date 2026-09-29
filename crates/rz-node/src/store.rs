//! Armazenamento local append-only de blocos finalizados (com certificado).
//!
//! Formato: sequência de `u32 comprimento ‖ enc(CommittedBlock)`. Na inicialização o
//! Node reprocessa todos os blocos desde o Genesis, verificando cada um
//! (`SPEC §28`): o disco também é tratado como fonte não confiável.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};

use rz_codec::{Decode, Encode};
use rz_core::CommittedBlock;

const MAX_STORED_BLOCK: u32 = 64 * 1024 * 1024;

pub struct BlockStore {
    path: PathBuf,
    file: File,
}

impl BlockStore {
    /// Abre (ou cria) o arquivo e retorna os blocos já gravados. Um final
    /// truncado ou corrompido (ex.: queda de energia) é descartado.
    pub fn open(dir: &Path) -> io::Result<(Self, Vec<CommittedBlock>)> {
        fs::create_dir_all(dir)?;
        let path = dir.join("blocks.log");
        let (blocks, valid_len) = Self::read_all(&path)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)?;
        file.set_len(valid_len)?;
        let mut store = Self { path, file };
        store.seek_end()?;
        Ok((store, blocks))
    }

    fn seek_end(&mut self) -> io::Result<()> {
        use std::io::Seek;
        self.file.seek(io::SeekFrom::End(0))?;
        Ok(())
    }

    fn read_all(path: &Path) -> io::Result<(Vec<CommittedBlock>, u64)> {
        let f = match File::open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok((Vec::new(), 0)),
            Err(e) => return Err(e),
        };
        let mut r = BufReader::new(f);
        let mut blocks = Vec::new();
        let mut offset = 0u64;
        loop {
            let mut len = [0u8; 4];
            if r.read_exact(&mut len).is_err() {
                break;
            }
            let len = u32::from_be_bytes(len);
            if len > MAX_STORED_BLOCK {
                break;
            }
            let mut body = vec![0u8; len as usize];
            if r.read_exact(&mut body).is_err() {
                break;
            }
            match CommittedBlock::from_canonical_bytes(&body) {
                Ok(b) => blocks.push(b),
                Err(_) => break,
            }
            offset += 4 + u64::from(len);
        }
        Ok((blocks, offset))
    }

    pub fn append(&mut self, block: &CommittedBlock) -> io::Result<()> {
        let body = block.to_canonical_bytes();
        let len = u32::try_from(body.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "bloco grande demais"))?;
        let mut buf = Vec::with_capacity(4 + body.len());
        buf.extend_from_slice(&len.to_be_bytes());
        buf.extend_from_slice(&body);
        self.file.write_all(&buf)?;
        self.file.flush()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}
