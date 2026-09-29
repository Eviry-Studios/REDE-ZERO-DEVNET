# spec/BLOCKS.md — Blocos e Genesis

**Versão:** 0.1.0 (DEVNET)
**Relacionamento:** `SPECIFICATIONS.md §15–§18, §66–§67`, ADR-0006, THR-CON-001, THR-CON-005, THR-CON-006
**Implementação de referência:** `crates/rz-core/src/block.rs`, `crates/rz-core/src/genesis.rs`

---

## 1. Cabeçalho

```text
BlockHeader {
  version    : u16         // = 1
  height     : u64
  parent     : fixed[32]   // BlockId do pai (hash do Genesis para height = 1)
  slot       : u64
  proposer   : fixed[32]   // chave pública do produtor
  tx_root    : fixed[32]
  state_root : fixed[32]
}

Block {
  header    : BlockHeader
  txs       : list<Transaction>   // máx. 10 000 na decodificação
  signature : fixed[64]
}
```

Não há timestamp livre. O instante de um bloco é `genesis_time_ms + slot × slot_duration_ms` (THR-CON-006).

## 2. Identificadores e compromissos

```text
BlockId   = H(BLOCK_ID, enc(BlockHeader))
tx_root   = H(TX_ROOT, list<TxId>)       // na ordem das transações no bloco
signature = Ed25519_sign(sk_proposer, string(BLOCK_SIGNATURE) ‖ string(network_id) ‖ enc(BlockHeader))
```

O BlockId do "bloco 0" é o hash do Genesis.

## 3. Validação estrutural (independente de consenso)

| # | Regra | Erro |
| --- | --- | --- |
| 1 | `version == 1` | `UnsupportedVersion` |
| 2 | `parent` igual ao BlockId do pai | `WrongParent` |
| 3 | `height == pai.height + 1` | `WrongHeight` |
| 4 | `slot > pai.slot` (pai diferente do Genesis) | `SlotNotIncreasing` |
| 5 | `tx_root` confere | `TxRootMismatch` |
| 6 | Assinatura do `proposer` válida | `Signature` |
| 7 | `len(txs) ≤ genesis.max_block_txs` | `TooManyTransactions` |
| 8 | Toda transação válida, aplicada em ordem | `Transaction { index }` |
| 9 | Invariante monetária preservada | `State` |
| 10 | `state_root` igual à raiz do estado resultante | `StateRootMismatch` |

Regras de consenso (quem pode produzir cada slot, limites temporais, escolha de fork e finalidade) estão em `spec/CONSENSUS.md`.

## 4. Encadeamento

Alterar qualquer campo de um bloco altera seu BlockId e, portanto, invalida o campo `parent` de todos os descendentes (`SPEC §16`).

## 5. Genesis

```text
Genesis {
  protocol_version : u16           // = 1
  kind             : u8            // 0 DEVNET, 1 TESTNET, 2 STAGING, 3 MAINNET
  network_id       : string        // máx. 64, [a-z0-9-], deve conter o nome do ambiente
  genesis_time_ms  : u64
  slot_duration_ms : u64           // > 0
  finality_depth   : u32           // > 0
  min_fee          : u64
  max_block_txs    : u32           // 1..=10 000
  validators       : list<fixed[32]>   // não vazia, sem repetição
  allocations      : list<{ address: fixed[32], amount: u64 }>  // ordenada, sem repetição, amount > 0
}

genesis_hash = H(GENESIS, enc(Genesis))
```

Um Node só aceita dados cujo `network_id` e `genesis_hash` coincidam com os seus (`SPEC §66`, `AT-GEN-002`).

## 6. Testes de aceitação cobertos

AT-BLOCK-001..005, AT-GEN-001..002, AT-DET-003.
