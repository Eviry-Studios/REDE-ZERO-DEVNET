# spec/MARKET.md — Grande Mercado e Pool permanente

**Versão:** 0.1.0 (DEVNET)
**Decisão:** ADR-0014
**Relacionamento:** `SPECIFICATIONS.md §38–§42`, REQ-032..039, AT-MKT-001..004, AT-POOL-001..004, THR-MKT-001..003, THR-POOL-001..003
**Implementação de referência:** `crates/rz-core/src/market.rs`, `crates/rz-core/src/state.rs`

---

## 1. Ativos

```text
AssetId  = fixed[32]
ZERO     = 0x00…00
externo  = H("rede-zero/asset/v1", string(network) ‖ string(asset_ref))

AssetInfo {
  network      : string   // 1..32, [A-Za-z0-9-_.:]; não pode começar com "rede-zero"
  asset_ref    : string   // 1..64, mesmo alfabeto; não pode ser "ZERO" (sem distinção de caixa)
  decimals     : u8
  verification : u8       // 0 = DevnetGenesis (único método nesta versão)
  supply       : u64      // quantidade representada na Rede Zero
}
```

Ativos externos só são registrados pelo Genesis (`GenesisAsset`, `spec/BLOCKS.md §5`) e **somente** em DEVNET, como ativos de teste. Pontes verificáveis estão A DEFINIR (ADR-0014 §4).

ZERO continua em `Account.balance`. Saldos de ativos externos ficam em `balances[(endereço, ativo)]`.

## 2. Operações

| Tag | Operação | Campos | Débito inicial | Regras |
| --- | --- | --- | --- | --- |
| `0x30` | `TransferAsset` | `asset, to, amount` | taxa | ativo externo registrado; saldo do ativo ≥ `amount` |
| `0x31` | `PoolDeposit` | `asset, amount` | taxa (+ `amount` se ZERO) | ZERO ou ativo registrado; saldo suficiente. **Irreversível** |
| `0x32` | `PlaceOrder` | `asset, side, amount, price, expires_at` | taxa (+ reserva se compra) | ver §3 |
| `0x33` | `CancelOrder` | `order` | taxa | a ordem existe e pertence ao remetente |

`amount = 0` e `price = 0` são inválidos sem consultar o estado.

**Não existe** operação que retire ativos do Pool (AT-POOL-002). As tags de `TxKind` são exatamente `0x00, 0x01, 0x10–0x14, 0x20–0x23, 0x30–0x33`; qualquer outra é `InvalidTag`.

## 3. Ordens

```text
Order {
  id         : fixed[32]  // TxId da PlaceOrder
  owner      : Address
  asset      : AssetId    // ativo-base; a cotação é sempre em ZERO
  side       : u8         // 0 compra, 1 venda
  price      : u64        // unidades mínimas de ZERO por unidade mínima do ativo × 10^8
  remaining  : u64        // quantidade restante do ativo
  escrow     : u64        // reservado: ZERO (compra) ou ativo (venda)
  placed_at  : u64
  expires_at : u64
}

valor(q, p)      = ⌊q · p / 10^8⌋
reserva_compra   = ⌈amount · price / 10^8⌉
```

`PlaceOrder` exige, com `h` = altura do bloco:

1. `asset` externo e registrado (ZERO não é ativo-base);
2. `h < expires_at ≤ h + order_lifetime_blocks`;
3. `ordens abertas < max_open_orders` e `ordens do remetente < max_orders_per_account`;
4. `valor(amount, price) ≥ 1` (sem ordens de valor nulo);
5. compra: ZERO ≥ `reserva_compra + taxa`; venda: saldo do ativo ≥ `amount`.

O valor reservado sai do saldo e volta integralmente no cancelamento ou no vencimento.

## 4. Leilão por bloco

No fim de cada bloco (antes das regras de participação e governança):

1. ordens com `expires_at ≤ h` saem do livro com reembolso;
2. para cada ativo cujo livro mudou no bloco (colocação ou cancelamento):
   1. candidatos = preços-limite presentes; para cada `p`: `D(p) = Σ remaining` das compras com `price ≥ p`, `S(p) = Σ remaining` das vendas com `price ≤ p`, volume `min(D, S)`;
   2. `p*` = maior volume; empate → menor `|D − S|`; empate → mediana inferior dos candidatos restantes (em ordem crescente);
   3. compras elegíveis (`price ≥ p*`) ordenadas por preço decrescente, altura, `id`; vendas elegíveis (`price ≤ p*`) por preço crescente, altura, `id`;
   4. pares formados em ordem, com `m = min(restantes)`, pagam `q = valor(m, p*)`. Se `q = 0` e `valor(m, preço_da_compra) ≥ 1`, então `q = 1`; se ainda `q = 0`, o par é pulado;
   5. para cada par: o comprador recebe `m` do ativo e sua reserva diminui `q`; o vendedor entrega `m` da reserva e recebe `q − tarifa` em ZERO; `tarifa = ⌊q · fee_bps / 10 000⌋` vai para o Pool;
   6. saem do livro, com reembolso do que restar reservado, as ordens executadas por completo e as de **valor nulo ao próprio limite** (`valor(remaining, price) = 0`);
   7. `last_price[ativo] = p*`.

O resultado depende apenas do conjunto de ordens, não da ordem das transações no bloco (THR-MKT-001). A reserva de uma compra cobre sempre o restante ao seu limite, porque `q ≤ m · price_compra / 10^8`.

## 5. Pool permanente

```text
pool : AssetId → u64
```

Só aumenta, por `PoolDeposit` ou pela tarifa do mercado. Nenhuma operação o reduz (INV-POOL, AT-POOL-003).

## 6. Parâmetros (`ProtocolParams.market`)

| Campo | Padrão | Governança |
| --- | --- | --- |
| `fee_bps` | 0 (limite 1 000 = 10%) | `market_fee_bps` (tag 20), constitucional |
| `order_lifetime_blocks` | 1 296 000 (~30 dias com blocos de 2 s) | `market_order_lifetime_blocks` (tag 21), constitucional |
| `max_open_orders` | 100 000 | — |
| `max_orders_per_account` | 256 | — |

## 7. Invariantes

```text
ZERO:     Σ saldos + … (spec/STATE.md §7) + Σ reservas de compra + pool[ZERO] = total_supply
externo:  Σ balances[·, a] + Σ reservas de venda de a + pool[a] = supply(a)
Pool:     pool[a] em h+1 ≥ pool[a] em h, para todo a
```

Violadas, o bloco é inválido (`SupplyMismatch`, `AssetMismatch`).

## 8. Raiz do estado

```text
market_root = H("rede-zero/market-root/v1",
    u64(n) ‖ (id ‖ AssetInfo)* ‖
    u64(n) ‖ (endereço ‖ ativo ‖ u64)* ‖
    u64(n) ‖ (ativo ‖ u64)*            // Pool
    u64(n) ‖ Order*                    // em ordem de id
    u64(n) ‖ (ativo ‖ u64)*)           // último preço
```

Todos os mapas em ordem crescente de chave. `market_root` entra na raiz do estado depois da raiz de governança (`spec/STATE.md §3`).

## 9. Consultas P2P

`GET_ASSETS` (0x18) → `ASSETS` (0x19): ativos, Pool e saldos de um endereço. `GET_MARKET` (0x1a) → `MARKET` (0x1b): livro agregado de um ativo (até 256 níveis por lado) e as ordens de um endereço. Ver `spec/P2P.md`.

## 10. Testes de aceitação cobertos

AT-MKT-001..004, AT-POOL-001..004 (`crates/rz-core/src/market_tests.rs`), leilão (`market::tests`), propriedades sob operações aleatórias (`randomized_market_invariants`) e ponta a ponta com Nodes reais (`crates/rz-wallet/tests/market_devnet.rs`).
