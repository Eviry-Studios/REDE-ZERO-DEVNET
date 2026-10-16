//! Livro de endereços com diversidade de rede (THR-P2P-001, `spec/P2P.md
//! §7`, ADR-0024).
//!
//! Um ataque eclipse funciona quando o atacante consegue que todas as
//! conexões de um Node levem a ele. As defesas aqui:
//!
//! * cada endereço pertence a uma **faixa de rede** (`/16` em IPv4, `/32` em
//!   IPv6); o livro guarda no máximo [`MAX_PER_GROUP`] endereços por faixa e
//!   [`MAX_PER_SOURCE`] aprendidos de uma mesma faixa de origem;
//! * cheio, o livro substitui uma entrada **ao acaso** da faixa mais cheia,
//!   em vez de recusar novidades (quem chegou primeiro não fica para sempre);
//! * conexões de saída são escolhidas **ao acaso**, no máximo uma por faixa.
//!
//! Endereços de loopback e de redes privadas formam uma faixa por endereço
//! (redes locais de teste); endereços onion, uma por endereço (são baratos
//! de criar: a defesa contra eles são os pares de partida e a detecção de
//! estagnação).

use std::collections::{BTreeMap, BTreeSet};
use std::net::IpAddr;

use rand_core::{OsRng, RngCore};
use rz_p2p::PeerAddr;

/// Total de endereços guardados.
pub const MAX_ADDRESSES: usize = 1024;
/// Endereços por faixa de rede.
pub const MAX_PER_GROUP: usize = 32;
/// Endereços aprendidos de uma mesma faixa de origem.
pub const MAX_PER_SOURCE: usize = 64;

/// Faixa de rede de um endereço.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NetGroup {
    V4([u8; 2]),
    V6([u8; 4]),
    /// Loopback, rede privada, onion ou nome: um grupo por endereço.
    Single(String),
}

fn local_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            v.is_loopback() || v.is_private() || v.is_link_local() || v.is_unspecified()
        }
        IpAddr::V6(v) => {
            v.is_loopback() || v.is_unspecified() || (v.segments()[0] & 0xfe00) == 0xfc00
        }
    }
}

/// Faixa de um endereço de par (conexões de saída e livro).
pub fn netgroup(addr: &PeerAddr) -> NetGroup {
    match addr {
        PeerAddr::Ip(a) if local_ip(&a.ip()) => NetGroup::Single(a.to_string()),
        PeerAddr::Ip(a) => ip_group(&a.ip()),
        PeerAddr::Host { host, port } => NetGroup::Single(format!("{host}:{port}")),
    }
}

/// Faixa de um IP de origem (conexões de entrada: a porta é efêmera).
pub fn ip_group(ip: &IpAddr) -> NetGroup {
    match ip {
        IpAddr::V4(v) if !local_ip(ip) => {
            let o = v.octets();
            NetGroup::V4([o[0], o[1]])
        }
        IpAddr::V6(v) if !local_ip(ip) => {
            let o = v.octets();
            NetGroup::V6([o[0], o[1], o[2], o[3]])
        }
        _ => NetGroup::Single(ip.to_string()),
    }
}

fn pick(n: usize) -> usize {
    (OsRng.next_u64() % n as u64) as usize
}

/// Embaralha no lugar (Fisher–Yates).
pub fn shuffle<T>(v: &mut [T]) {
    for i in (1..v.len()).rev() {
        v.swap(i, pick(i + 1));
    }
}

#[derive(Clone, Debug)]
struct Entry {
    group: NetGroup,
    source: Option<NetGroup>,
}

#[derive(Debug, Default)]
pub struct AddrBook {
    entries: BTreeMap<PeerAddr, Entry>,
}

impl AddrBook {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains(&self, a: &PeerAddr) -> bool {
        self.entries.contains_key(a)
    }

    pub fn iter(&self) -> impl Iterator<Item = &PeerAddr> {
        self.entries.keys()
    }

    fn count_group(&self, g: &NetGroup) -> usize {
        self.entries.values().filter(|e| &e.group == g).count()
    }

    fn count_source(&self, s: &NetGroup) -> usize {
        self.entries
            .values()
            .filter(|e| e.source.as_ref() == Some(s))
            .count()
    }

    /// Acrescenta `addr`, aprendido de `source` (`None` para endereços
    /// confirmados pelo próprio Node). Retorna se entrou.
    pub fn insert(&mut self, addr: PeerAddr, source: Option<NetGroup>) -> bool {
        if self.entries.contains_key(&addr) {
            return false;
        }
        let group = netgroup(&addr);
        if self.count_group(&group) >= MAX_PER_GROUP {
            return false;
        }
        if let Some(s) = &source {
            if self.count_source(s) >= MAX_PER_SOURCE {
                return false;
            }
        }
        if self.entries.len() >= MAX_ADDRESSES {
            self.evict();
        }
        self.entries.insert(addr, Entry { group, source });
        true
    }

    /// Remove uma entrada ao acaso da faixa mais cheia.
    fn evict(&mut self) {
        let mut counts: BTreeMap<&NetGroup, usize> = BTreeMap::new();
        for e in self.entries.values() {
            *counts.entry(&e.group).or_default() += 1;
        }
        let Some(max) = counts.values().max().copied() else {
            return;
        };
        let crowded: BTreeSet<NetGroup> = counts
            .into_iter()
            .filter(|(_, c)| *c == max)
            .map(|(g, _)| g.clone())
            .collect();
        let victims: Vec<PeerAddr> = self
            .entries
            .iter()
            .filter(|(_, e)| crowded.contains(&e.group))
            .map(|(a, _)| a.clone())
            .collect();
        if !victims.is_empty() {
            let v = victims[pick(victims.len())].clone();
            self.entries.remove(&v);
        }
    }

    pub fn remove(&mut self, addr: &PeerAddr) {
        self.entries.remove(addr);
    }
}

/// Escolhe até `n` endereços de `candidates`, ao acaso, sem repetir faixa
/// entre si nem com `used` (faixas das conexões de saída atuais).
pub fn select_diverse(
    candidates: impl IntoIterator<Item = PeerAddr>,
    used: &BTreeSet<NetGroup>,
    n: usize,
) -> Vec<PeerAddr> {
    let mut pool: Vec<PeerAddr> = candidates.into_iter().collect();
    shuffle(&mut pool);
    let mut groups = used.clone();
    let mut out = Vec::new();
    for a in pool {
        if out.len() >= n {
            break;
        }
        if groups.insert(netgroup(&a)) {
            out.push(a);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> PeerAddr {
        PeerAddr::Ip(s.parse().unwrap_or_else(|_| panic!("{s}")))
    }

    #[test]
    fn groups() {
        assert_eq!(netgroup(&ip("8.8.4.4:1")), netgroup(&ip("8.8.200.1:2")));
        assert_ne!(netgroup(&ip("8.8.4.4:1")), netgroup(&ip("8.9.4.4:1")));
        assert_ne!(netgroup(&ip("127.0.0.1:1")), netgroup(&ip("127.0.0.1:2")));
        assert_eq!(
            ip_group(&"203.0.113.9".parse().unwrap_or_else(|_| panic!())),
            NetGroup::V4([203, 0])
        );
    }

    // Um atacante numa só faixa não ocupa o livro.
    #[test]
    fn flood_from_one_group_is_capped() {
        let mut b = AddrBook::default();
        let src = Some(NetGroup::V4([198, 51]));
        for i in 0..2_000u32 {
            b.insert(
                ip(&format!("198.51.{}.{}:7000", (i / 250) % 250, i % 250)),
                src.clone(),
            );
        }
        assert_eq!(b.len(), MAX_PER_GROUP);
        assert!(b.insert(ip("9.9.9.9:7000"), None), "honesto ainda entra");
    }

    // Uma origem não injeta mais que MAX_PER_SOURCE endereços, mesmo
    // espalhados por muitas faixas.
    #[test]
    fn flood_from_one_source_is_capped() {
        let mut b = AddrBook::default();
        let src = Some(NetGroup::V4([203, 0]));
        for i in 0..1_000u32 {
            b.insert(
                ip(&format!("{}.{}.1.1:7000", 11 + i / 200, i % 200)),
                src.clone(),
            );
        }
        assert_eq!(b.len(), MAX_PER_SOURCE);
    }

    // Cheio, o livro despeja da faixa mais cheia: novidades continuam entrando.
    #[test]
    fn full_book_evicts_from_crowded_group() {
        let mut b = AddrBook::default();
        let mut n = 0;
        'outer: for a in 1..=250u8 {
            for c in 0..MAX_PER_GROUP as u8 {
                if b.len() >= MAX_ADDRESSES {
                    break 'outer;
                }
                b.insert(ip(&format!("{a}.1.{c}.1:7000")), None);
                n += 1;
            }
        }
        assert_eq!(n, MAX_ADDRESSES);
        assert!(b.insert(ip("251.1.0.1:7000"), None));
        assert_eq!(b.len(), MAX_ADDRESSES);
        assert!(b.contains(&ip("251.1.0.1:7000")));
    }

    #[test]
    fn selection_is_diverse_and_random() {
        let mut cands = Vec::new();
        for i in 0..50u8 {
            cands.push(ip(&format!("198.51.0.{i}:7000")));
        }
        cands.push(ip("9.9.9.9:7000"));
        cands.push(ip("8.8.8.8:7000"));
        let used: BTreeSet<NetGroup> = [NetGroup::V4([8, 8])].into_iter().collect();
        let s = select_diverse(cands.clone(), &used, 8);
        assert_eq!(s.len(), 2, "uma por faixa, e a faixa já usada fica de fora");
        assert!(s.contains(&ip("9.9.9.9:7000")));
        let mut seen = BTreeSet::new();
        for _ in 0..64 {
            seen.insert(select_diverse(cands.clone(), &BTreeSet::new(), 1)[0].clone());
        }
        assert!(seen.len() > 1, "a escolha não é fixa");
    }
}
