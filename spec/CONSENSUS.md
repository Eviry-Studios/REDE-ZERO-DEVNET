# spec/CONSENSUS.md — Consenso da DEVNET

**Versão:** 0.1.0 (DEVNET)
**Escopo:** **somente DEVNET** — ver ADR-0006
**Relacionamento:** `SPECIFICATIONS.md §19–§22`, THR-CON-001..007
**Implementação de referência:** `crates/rz-chain`

---

> Este documento especifica um consenso **provisório**, suficiente para desenvolver e testar os demais componentes. Ele **não** satisfaz `REQ-005` e não pode ser herdado pela TESTNET pública ou MAINNET.

## 1. Interface

Todo mecanismo de consenso da Rede Zero implementa três funções:

| Função | Responsabilidade |
| --- | --- |
| `expected_proposer(slot)` | Quem pode produzir o bloco de um slot |
| `check_header(header, now)` | Regras de consenso do cabeçalho |
| `prefer(a, b)` | Escolha de fork entre duas pontas |

Transações, estado, blocos e P2P não dependem do mecanismo escolhido.

## 2. Slots

```text
slot(t)       = ⌊(t − genesis_time_ms) / slot_duration_ms⌋
início(slot)  = genesis_time_ms + slot × slot_duration_ms
```

## 3. Produtor do slot

```text
produtor(slot) = genesis.validators[slot mod len(validators)]
```

Um bloco é inválido se `header.proposer ≠ produtor(header.slot)` (`WrongProposer`).

## 4. Regra temporal

Ao receber um bloco pela rede, o Node rejeita blocos cujo slot ainda não começou, com tolerância de `max_future_ms` (padrão 1 000 ms) para desvio de relógio (`FutureSlot`).

A regra **não** se aplica ao reprocessar blocos persistidos localmente.

## 5. Escolha de fork

Entre duas pontas válidas `a` e `b`:

```text
prefer(a, b) = a.height > b.height  ∨  (a.height = b.height ∧ a.id < b.id)
```

A comparação de `id` é lexicográfica sobre os 32 bytes. Todos os Nodes que conhecem o mesmo conjunto de blocos escolhem a mesma ponta (`AT-FORK-001`).

## 6. Finalidade

Com `k = genesis.finality_depth`:

```text
finalizado = ancestral da ponta na altura (altura_da_ponta − k)
```

A finalidade só avança. Um bloco cujo pai não descende do bloco finalizado é rejeitado (`ConflictsWithFinality`).

Estados de transação, conforme `SPEC §21`:

| Estado | Condição |
| --- | --- |
| recebida | aceita no mempool |
| incluída | presente em um bloco conhecido |
| confirmada | presente na cadeia preferida |
| finalizada | presente em bloco de altura ≤ altura finalizada |

## 7. Evidência de equivocação

Dois cabeçalhos distintos, ambos com assinatura válida do mesmo `proposer` para o mesmo `slot`, constituem evidência verificável (THR-CON-004). Na DEVNET a evidência é registrada; a penalização protocolar está **A DEFINIR** (`SPEC §49`).

## 8. Importação de bloco

```text
bloco recebido
  │
  ├── já conhecido?                      → ignorar
  ├── pai conhecido?            NÃO      → UnknownParent (solicitar sincronização)
  ├── descende do finalizado?   NÃO      → ConflictsWithFinality
  ├── regras de consenso        falha    → Consensus
  ├── regras de bloco (BLOCKS.md) falha  → Block
  ├── registrar slot (equivocação)
  └── escolha de fork → nova ponta? → atualizar finalidade e podar
```

## 9. Limitações conhecidas

* conjunto de validadores fixo;
* sem tolerância bizantina na finalidade;
* sem anti-Sybil;
* validador ausente gera slot vazio (liveness degrada linearmente).

## 10. Testes de aceitação cobertos

AT-CON-001..003, AT-FORK-001..002, AT-DS-002.
