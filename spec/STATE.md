# spec/STATE.md — Estado e ZERO

**Versão:** 0.2.0 (DEVNET)
**Relacionamento:** `SPECIFICATIONS.md §12–§14, §33–§37`, ADR-0005, THR-TX-002, THR-TX-003, THR-TX-005
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
               u64(total_imagens_de_chave) ‖ key_image_acc)
```

## 4. Estado inicial

Definido pelo Genesis (`spec/BLOCKS.md §5`): cada alocação cria uma conta com o saldo indicado e `nonce = 0`; cada validador do Genesis recebe um vínculo igual ao seu `stake`. `total_supply` é a soma das alocações e dos `stake`.

## 5. Emissão

Na DEVNET **não há emissão após o Genesis**: recompensa de bloco é zero. A política monetária definitiva está **A DEFINIR** (`REQUIREMENTS.md §23`).

Nenhuma operação pública permite alterar saldos fora das regras de transação e de distribuição de taxas (INV-003, `AT-MONEY-004`).

## 6. Taxas

As taxas de todas as transações de um bloco são somadas e creditadas ao endereço derivado da chave pública do produtor do bloco, após a execução das transações.

## 7. Invariante monetária

Após aplicar cada bloco:

```text
Σ balance(conta) + shielded_supply
  + Σ bloqueios de governança + Σ depósitos retidos
  + Σ vínculos de validador + Σ desvinculações pendentes      == total_supply
```

A punição (`spec/CONSENSUS.md §6.2`) é a única operação que reduz `total_supply`: o valor queimado sai dos vínculos e da oferta ao mesmo tempo.

Um bloco que viole esta igualdade é inválido. A verificação é redundante com as regras de transação, e existe como defesa em profundidade.

## 8. Determinismo

A função de transição não usa relógio, aleatoriedade, ponto flutuante nem estruturas com ordem de iteração indefinida (THR-TX-005).

## 9. Testes de aceitação cobertos

AT-STATE-001..003, AT-MONEY-001..003, AT-ZERO-001..002, AT-DET-001, AT-FEE-001..003.
