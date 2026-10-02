//! Balde de fichas para limitar taxa de mensagens por conexão (`SPEC §26`).

use std::time::Instant;

#[derive(Debug, Clone)]
pub struct TokenBucket {
    capacity: f64,
    tokens: f64,
    refill_per_sec: f64,
    last: Instant,
}

impl TokenBucket {
    pub fn new(capacity: u32, refill_per_sec: u32) -> Self {
        Self {
            capacity: capacity.into(),
            tokens: capacity.into(),
            refill_per_sec: refill_per_sec.into(),
            last: Instant::now(),
        }
    }

    /// Altera capacidade e recarga (ex.: quando o conjunto de validadores
    /// muda de tamanho), preservando o saldo até a nova capacidade.
    pub fn set_rate(&mut self, capacity: u32, refill_per_sec: u32) {
        self.capacity = capacity.into();
        self.refill_per_sec = refill_per_sec.into();
        self.tokens = self.tokens.min(self.capacity);
    }

    /// Consome `cost` fichas; retorna `false` se não houver saldo.
    pub fn try_take(&mut self, cost: u32) -> bool {
        self.try_take_at(cost, Instant::now())
    }

    pub fn try_take_at(&mut self, cost: u32, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        let cost = f64::from(cost);
        if self.tokens >= cost {
            self.tokens -= cost;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn limits_and_refills() {
        let t0 = Instant::now();
        let mut b = TokenBucket::new(3, 1);
        b.last = t0;
        assert!(b.try_take_at(1, t0));
        assert!(b.try_take_at(1, t0));
        assert!(b.try_take_at(1, t0));
        assert!(!b.try_take_at(1, t0));
        assert!(b.try_take_at(1, t0 + Duration::from_secs(1)));
    }
}
