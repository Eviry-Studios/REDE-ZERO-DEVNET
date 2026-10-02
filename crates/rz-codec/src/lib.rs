//! Codificação canônica da Rede Zero.
//!
//! Implementa `spec/ENCODING.md` (ADR-0004). Cada valor possui exatamente uma
//! representação válida em bytes; a decodificação é estrita e rejeita qualquer
//! desvio (bytes restantes, booleanos inválidos, comprimentos excessivos).

use std::fmt;

/// Erro de decodificação. Nunca provoca pânico: dados vindos da rede ou do
/// disco são tratados como não confiáveis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// A entrada terminou antes do esperado.
    UnexpectedEnd,
    /// Sobraram bytes após o objeto (codificação não canônica).
    TrailingBytes(usize),
    /// Booleano diferente de `0x00`/`0x01`.
    InvalidBool(u8),
    /// Tag de opcional ou de variante desconhecida.
    InvalidTag(u8),
    /// Comprimento declarado acima do limite permitido.
    LengthExceeded { declared: u64, max: u64 },
    /// Valor fora do domínio do tipo.
    InvalidValue(&'static str),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEnd => write!(f, "fim inesperado da entrada"),
            Self::TrailingBytes(n) => write!(f, "{n} bytes restantes após o objeto"),
            Self::InvalidBool(b) => write!(f, "booleano inválido: {b:#04x}"),
            Self::InvalidTag(t) => write!(f, "tag inválida: {t:#04x}"),
            Self::LengthExceeded { declared, max } => {
                write!(f, "comprimento {declared} excede o máximo {max}")
            }
            Self::InvalidValue(what) => write!(f, "valor inválido: {what}"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Limite padrão para sequências de bytes sem limite específico (1 MiB).
pub const DEFAULT_MAX_BYTES: usize = 1 << 20;

/// Acumulador de bytes canônicos.
#[derive(Default, Debug)]
pub struct Encoder {
    buf: Vec<u8>,
}

impl Encoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.buf.push(v);
        self
    }

    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }

    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }

    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }

    pub fn bool(&mut self, v: bool) -> &mut Self {
        self.u8(u8::from(v))
    }

    /// Bytes de tamanho fixo, sem prefixo (o tamanho é definido pelo tipo).
    pub fn fixed(&mut self, v: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(v);
        self
    }

    /// Bytes de tamanho variável com prefixo `u32`.
    ///
    /// # Panics
    /// Se `v` tiver mais de `u32::MAX` bytes — impossível para objetos
    /// protocolares, cujos limites são muito menores.
    pub fn bytes(&mut self, v: &[u8]) -> &mut Self {
        let len = u32::try_from(v.len()).expect("comprimento excede u32");
        self.u32(len);
        self.fixed(v)
    }

    pub fn str(&mut self, v: &str) -> &mut Self {
        self.bytes(v.as_bytes())
    }

    /// Lista com prefixo `u32` de quantidade de itens.
    pub fn list<T: Encode>(&mut self, items: &[T]) -> &mut Self {
        let len = u32::try_from(items.len()).expect("quantidade excede u32");
        self.u32(len);
        for item in items {
            item.encode(self);
        }
        self
    }

    /// Opcional explícito: `0x00` ausente, `0x01` seguido do valor.
    pub fn option<T: Encode>(&mut self, v: &Option<T>) -> &mut Self {
        match v {
            None => self.u8(0),
            Some(x) => {
                self.u8(1);
                x.encode(self);
                self
            }
        }
    }

    pub fn put<T: Encode + ?Sized>(&mut self, v: &T) -> &mut Self {
        v.encode(self);
        self
    }
}

/// Leitor estrito de bytes canônicos.
#[derive(Debug)]
pub struct Decoder<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Decoder<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        if self.remaining() < n {
            return Err(DecodeError::UnexpectedEnd);
        }
        let out = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    pub fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16, DecodeError> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    pub fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    pub fn u64(&mut self) -> Result<u64, DecodeError> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    pub fn bool(&mut self) -> Result<bool, DecodeError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            b => Err(DecodeError::InvalidBool(b)),
        }
    }

    pub fn fixed<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        self.array()
    }

    /// Lê o prefixo de comprimento e valida o limite **antes** de ler o corpo.
    fn length(&mut self, max: usize) -> Result<usize, DecodeError> {
        let declared = self.u32()?;
        if declared as u64 > max as u64 {
            return Err(DecodeError::LengthExceeded {
                declared: declared.into(),
                max: max as u64,
            });
        }
        Ok(declared as usize)
    }

    pub fn bytes(&mut self, max: usize) -> Result<Vec<u8>, DecodeError> {
        let len = self.length(max)?;
        Ok(self.take(len)?.to_vec())
    }

    pub fn str(&mut self, max: usize) -> Result<String, DecodeError> {
        String::from_utf8(self.bytes(max)?).map_err(|_| DecodeError::InvalidValue("UTF-8"))
    }

    pub fn list<T: Decode>(&mut self, max_items: usize) -> Result<Vec<T>, DecodeError> {
        let len = self.length(max_items)?;
        // Não pré-aloca com base no valor declarado: cada item ainda precisa
        // existir de fato na entrada.
        let mut out = Vec::with_capacity(len.min(1024));
        for _ in 0..len {
            out.push(T::decode(self)?);
        }
        Ok(out)
    }

    pub fn option<T: Decode>(&mut self) -> Result<Option<T>, DecodeError> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(T::decode(self)?)),
            t => Err(DecodeError::InvalidTag(t)),
        }
    }

    pub fn get<T: Decode>(&mut self) -> Result<T, DecodeError> {
        T::decode(self)
    }

    /// Exige que toda a entrada tenha sido consumida.
    pub fn finish(self) -> Result<(), DecodeError> {
        match self.remaining() {
            0 => Ok(()),
            n => Err(DecodeError::TrailingBytes(n)),
        }
    }
}

/// Tipos com representação canônica.
pub trait Encode {
    fn encode(&self, e: &mut Encoder);

    fn to_canonical_bytes(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        self.encode(&mut e);
        e.into_bytes()
    }
}

/// Tipos decodificáveis a partir da representação canônica.
pub trait Decode: Sized {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError>;

    /// Decodifica exigindo que a entrada contenha exatamente um objeto.
    fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut d = Decoder::new(bytes);
        let v = Self::decode(&mut d)?;
        d.finish()?;
        Ok(v)
    }
}

macro_rules! impl_int {
    ($($t:ident),*) => {$(
        impl Encode for $t {
            fn encode(&self, e: &mut Encoder) { e.$t(*self); }
        }
        impl Decode for $t {
            fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> { d.$t() }
        }
    )*};
}
impl_int!(u8, u16, u32, u64);

impl Encode for bool {
    fn encode(&self, e: &mut Encoder) {
        e.bool(*self);
    }
}

impl Decode for bool {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        d.bool()
    }
}

impl<const N: usize> Encode for [u8; N] {
    fn encode(&self, e: &mut Encoder) {
        e.fixed(self);
    }
}

impl<const N: usize> Decode for [u8; N] {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        d.fixed()
    }
}

impl<T: Encode + ?Sized> Encode for &T {
    fn encode(&self, e: &mut Encoder) {
        (*self).encode(e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_are_big_endian() {
        let mut e = Encoder::new();
        e.u8(1)
            .u16(0x0203)
            .u32(0x0405_0607)
            .u64(0x0809_0a0b_0c0d_0e0f);
        assert_eq!(
            e.into_bytes(),
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
        );
    }

    #[test]
    fn bytes_are_length_prefixed() {
        let mut e = Encoder::new();
        e.bytes(b"abc");
        assert_eq!(e.into_bytes(), [0, 0, 0, 3, b'a', b'b', b'c']);
    }

    #[test]
    fn roundtrip_mixed() {
        let mut e = Encoder::new();
        e.u64(42)
            .bool(true)
            .str("zero")
            .option(&Some(7u32))
            .list(&[1u16, 2, 3]);
        let bytes = e.into_bytes();
        let mut d = Decoder::new(&bytes);
        assert_eq!(d.u64().unwrap(), 42);
        assert!(d.bool().unwrap());
        assert_eq!(d.str(16).unwrap(), "zero");
        assert_eq!(d.option::<u32>().unwrap(), Some(7));
        assert_eq!(d.list::<u16>(8).unwrap(), vec![1, 2, 3]);
        d.finish().unwrap();
    }

    #[test]
    fn rejects_trailing_bytes() {
        assert_eq!(
            u32::from_canonical_bytes(&[0, 0, 0, 1, 0]),
            Err(DecodeError::TrailingBytes(1))
        );
    }

    #[test]
    fn rejects_truncated_input() {
        assert_eq!(
            u64::from_canonical_bytes(&[0, 0, 0]),
            Err(DecodeError::UnexpectedEnd)
        );
    }

    #[test]
    fn rejects_non_canonical_bool() {
        assert_eq!(
            bool::from_canonical_bytes(&[2]),
            Err(DecodeError::InvalidBool(2))
        );
    }

    #[test]
    fn rejects_invalid_option_tag() {
        let mut d = Decoder::new(&[5]);
        assert_eq!(d.option::<u8>(), Err(DecodeError::InvalidTag(5)));
    }

    #[test]
    fn length_checked_before_reading_body() {
        // Declara 4 GiB sem fornecer os dados: deve falhar pelo limite, sem alocar.
        let mut d = Decoder::new(&[0xff, 0xff, 0xff, 0xff]);
        assert!(matches!(
            d.bytes(1024),
            Err(DecodeError::LengthExceeded { .. })
        ));
        let mut d = Decoder::new(&[0xff, 0xff, 0xff, 0xff]);
        assert!(matches!(
            d.list::<u8>(10),
            Err(DecodeError::LengthExceeded { .. })
        ));
    }

    #[test]
    fn list_with_lying_length_fails_without_panic() {
        // Declara 3 itens mas fornece 1.
        let mut d = Decoder::new(&[0, 0, 0, 3, 9]);
        assert_eq!(d.list::<u8>(10), Err(DecodeError::UnexpectedEnd));
    }

    #[test]
    fn invalid_utf8_rejected() {
        let mut d = Decoder::new(&[0, 0, 0, 1, 0xff]);
        assert_eq!(d.str(8), Err(DecodeError::InvalidValue("UTF-8")));
    }
}
