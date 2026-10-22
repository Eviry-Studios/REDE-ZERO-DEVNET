# spec/STATE.md — Estado e ZERO

**Versão:** 0.11.0 (DEVNET)
**Relacionamento:** `SPECIFICATIONS.md §12–§14, §33–§37`, ADR-0005, ADR-0025, THR-TX-002, THR-TX-003, THR-TX-005
**Implementação de referência:** `crates/rz-core/src/state.rs`, `crates/rz-core/src/amount.rs`

---

## 1. Unidade ZERO

```text
1 ZERO = 100 000 000 unidades mínimas
```

Todos os valores protocolares são `u64` de unidades mínimas. Representações decimais (`"1.5"`) existem apenas em interfaces e têm no máximo 8 casas.

## 2. Estado

```text
State {
  accounts     : mapa ordenado Address → Account { balance: u64, nonce: u64 }
  total_supply : u64
}
```

Contas ausentes equivalem a `{ balance: 0, nonce: 0 }`. Contas com `nonce > 0` nunca são removidas (proteção contra replay).

A parte privada do estado (notas, imagens de chave, oferta privada) é especificada em `spec/PRIVACY.md §8`.

## 3. Compromisso do estado

```text
state_root = H(STATE_ROOT,
               u64(total_supply) ‖ u64(shielded_supply) ‖
               u32(n) ‖
               (address ‖ u64(balance) ‖ u64(nonce))  para cada conta, em ordem crescente de address ‖
               u64(total_saídas) ‖ output_acc ‖
               u64(total_imagens_de_chave) ‖ key_image_acc ‖
               governance_root ‖            // spec/GOVERNANCE.md (inclui participação no consenso)
               market_root ‖                // spec/MARKET.md §8
               community_root ‖             // spec/COMMUNITIES.md §9
               defense_root ‖               // spec/DEFENSE.md §7
               runtime_root ‖               // spec/RUNTIME.md §2
               bridge_root ‖                // spec/BRIDGE.md §6
               reputation_root ‖            // spec/REPUTATION.md §3
               releases_root ‖              // spec/RELEASES.md §9
               keys_root)                   // spec/KEYS.md §7
```

## 4. Estado inicial

Definido pelo Genesis (`spec/BLOCKS.md §5`): cada alocação cria uma conta com o saldo indicado e `nonce = 0`; cada validador do Genesis recebe um vínculo igual ao seu `stake`. `total_supply` é a soma das alocações e dos `stake`.

## 5. Emissão

Política monetária da ADR-0025: emissão **constante por bloco, sem teto**.

* Ao fim de cada bloco, o protocolo cria `block_reward` unidades e as credita ao endereço do proponente, junto com a sua parte das taxas (§6).
* `block_reward` vem do Genesis (`spec/BLOCKS.md §5`) e só muda por proposta constitucional (tag 38, `spec/GOVERNANCE.md`), com teto `MAX_BLOCK_REWARD = 100 ZERO`.
* O valor é por altura: o protocolo não tem relógio (`spec/BLOCKS.md §1`).
* Valores da TESTNET: `block_reward = 0,1 ZERO` e Genesis de 100 000 000 ZERO, cerca de 1 % ao ano no primeiro ano com blocos de 2 s. A inflação percentual cai com o tempo, porque o valor absoluto é fixo.
* Não há alocação a fundadores: o Genesis cobre o vínculo dos validadores iniciais e a distribuição inicial, e o resto da oferta nasce pela emissão.

Um Genesis com `block_reward = 0` mantém a regra anterior, sem emissão.

Nenhuma operação pública permite alterar saldos fora das regras de transação e de distribuição de taxas (INV-003, `AT-MONEY-004`).

## 6. Taxas

As taxas de todas as transações de um bloco são somadas depois da execução das transações:

```text
queimado  = ⌊taxas · fee_burn_bps / 10 000⌋
produtor  += taxas − queimado + block_reward
total_supply += block_reward − queimado
```

`fee_burn_bps` (0..=10 000) vem do Genesis e só muda por proposta constitucional (tag 39). Na TESTNET é 5 000: metade das taxas é queimada. Com pouco uso, a emissão paga os validadores; com muito uso, a queima compensa a emissão e liga a oferta ao uso da rede. A queima também impede que o proponente inclua transações próprias de graça.

## 7. Invariante monetária

Após aplicar cada bloco:

```text
Σ balance(conta) + shielded_supply
  + Σ bloqueios de governança + Σ depósitos retidos
  + Σ vínculos de validador + Σ desvinculações pendentes
  + Σ ZERO reservado em compras + ZERO no Pool                == total_supply
```

Para cada ativo externo `a` (`spec/MARKET.md §7`):

```text
Σ saldos de a + Σ reservas de venda de a + Pool[a] == supply(a)
```

E o Pool só cresce.

`total_supply` muda só por regras explícitas do protocolo: cresce pela emissão (§5) e diminui pela queima de taxas (§6), pela punição (`spec/CONSENSUS.md §6.2`), pelo depósito de proposta sem quórum (`spec/GOVERNANCE.md`) e pelos custos de publicação queimados (`spec/BRIDGE.md`, `spec/RELEASES.md`). Nenhuma transação cria ZERO (AT-ZERO-003).

Um bloco que viole esta igualdade é inválido. A verificação é redundante com as regras de transação, e existe como defesa em profundidade.

## 8. Determinismo

A função de transição não usa relógio, aleatoriedade, ponto flutuante nem estruturas com ordem de iteração indefinida (THR-TX-005).

## 9. Testes de aceitação cobertos

AT-STATE-001..003, AT-MONEY-001..003, AT-ZERO-001..002, AT-DET-001, AT-FEE-001..003.
