# spec/BLOCKS.md — Blocos e Genesis

**Versão:** 0.3.0 (DEVNET)
**Relacionamento:** `SPECIFICATIONS.md §15–§18, §66–§67`, ADR-0012, THR-CON-001, THR-CON-005, THR-CON-006
**Implementação de referência:** `crates/rz-core/src/block.rs`, `crates/rz-core/src/genesis.rs`

---

## 1. Cabeçalho

```text
BlockHeader {
  version    : u16         // = 1
  height     : u64
  parent     : fixed[32]   // BlockId do pai (hash do Genesis para height = 1)
  round      : u64         // rodada Zero-BFT em que o bloco foi montado
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

Não há timestamp nem slot: a ordem é dada pela altura, e o tempo protocolar é medido em blocos (THR-CON-006). O bloco só é aceito na cadeia acompanhado do certificado de finalização:

```text
CommittedBlock {
  block  : Block
  commit : Commit      // pré-compromissos de > 2/3 do poder (spec/CONSENSUS.md §4.3)
}
```

É o `CommittedBlock` que é gravado em disco e trocado na sincronização.

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
| 4 | `tx_root` confere | `TxRootMismatch` |
| 5 | Assinatura do `proposer` válida | `Signature` |
| 6 | `len(txs) ≤ max_block_txs` vigente | `TooManyTransactions` |
| 6a | `Σ len(enc(tx)) ≤ MAX_BLOCK_TX_BYTES` (2 MiB), para que bloco, proposta e certificado caibam num quadro P2P | `TooLarge` |
| 7 | Toda transação válida, aplicada em ordem | `Transaction { index }` |
| 8 | Invariante monetária preservada | `State` |
| 9 | `state_root` igual à raiz do estado resultante | `StateRootMismatch` |

Regras de consenso (proponente sorteado para a altura e rodada, certificado de finalização) estão em `spec/CONSENSUS.md`. Ao fim de cada bloco, o estado executa as regras de participação (liberação de desvinculações, recomputação do conjunto na época) e de governança (`spec/STATE.md`).

## 4. Encadeamento

Alterar qualquer campo de um bloco altera seu BlockId e, portanto, invalida o campo `parent` de todos os descendentes (`SPEC §16`).

## 5. Genesis

```text
Genesis {
  protocol_version : u16           // = 1
  kind             : u8            // 0 DEVNET, 1 TESTNET, 2 STAGING, 3 MAINNET
  network_id       : string        // máx. 64, [a-z0-9-], deve conter o nome do ambiente
  consensus        : ConsensusParams   // spec/CONSENSUS.md §2
  min_fee          : u64
  max_block_txs    : u32           // 1..=10 000
  validators       : list<{ key: fixed[32], stake: u64 }>
                     // não vazia, sem repetição, ≤ max_validators, stake ≥ min_bond
  allocations      : list<{ address: fixed[32], amount: u64 }>  // ordenada, sem repetição, amount > 0
  governance       : GovernanceParams  // spec/GOVERNANCE.md
  assets           : list<GenesisAsset> // ≤ 64; somente DEVNET (spec/MARKET.md §1)
}

GenesisAsset {
  network     : string
  asset_ref   : string
  decimals    : u8
  allocations : list<{ address, amount }>  // ordenada, sem repetição, amount > 0
}

oferta_total = Σ allocations.amount + Σ validators.stake

genesis_hash = H(GENESIS, enc(Genesis))
```

Um Genesis com `assets` não vazio só é válido em DEVNET: os ativos são de **teste** e não representam nada na rede de origem (ADR-0014). Os `stake` do Genesis tornam-se os vínculos iniciais e formam o conjunto de validadores da altura 1.

Um Node só aceita dados cujo `network_id` e `genesis_hash` coincidam com os seus (`SPEC §66`, `AT-GEN-002`).

## 6. Testes de aceitação cobertos

AT-BLOCK-001..005, AT-GEN-001..002, AT-DET-003.
