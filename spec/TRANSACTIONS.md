# spec/TRANSACTIONS.md — Transações

**Versão:** 0.1.0 (DEVNET)
**Relacionamento:** `SPECIFICATIONS.md §6–§11`, ADR-0005, THR-ID-004, THR-TX-001, THR-TX-004, THR-TX-006
**Implementação de referência:** `crates/rz-core/src/tx.rs`

---

## 0. Tipos de transação

```text
Transaction (tag u8):
  0x00 AccountTx   // transparente, descrita neste documento
  0x01 PrivateTx   // privada, descrita em spec/PRIVACY.md §7.2
```

A operação `TxKind::Shield` (tag `0x01`) está descrita em `spec/PRIVACY.md §7.1`.

## 1. Estrutura

```text
TxBody {
  version : u16          // = 1
  sender  : fixed[32]    // chave pública Ed25519 do remetente
  nonce   : u64          // nonce atual da conta remetente
  fee     : u64          // taxa em unidades mínimas
  kind    : TxKind
}

TxKind (tag u8):
  0x00 Transfer { to: fixed[32] (Address), amount: u64 }

Transaction {
  body      : TxBody
  signature : fixed[64]
}
```

Codificação: `spec/ENCODING.md`, campos na ordem acima.

Não existem timestamp, memo nem outros metadados opcionais (`SPEC §32`, THR-PRIV-003).

## 2. Identificador

```text
TxId = H(TX_ID, enc(TxBody))
```

A assinatura não participa do identificador: o ID não pode ser alterado por terceiros sem invalidar a assinatura (THR-TX-004).

## 3. Assinatura

```text
signature = Ed25519_sign(sk, string(TX_SIGNATURE) ‖ string(network_id) ‖ enc(TxBody))
```

Ver `spec/CRYPTOGRAPHY.md §5`. A Wallet assina localmente; o Node recebe apenas a transação assinada (`SPEC-WAL-002`, `SPEC-WAL-003`).

## 4. Validação

A validação segue a ordem abaixo, da verificação mais barata para a mais cara (THR-P2P-002):

| # | Regra | Erro |
| --- | --- | --- |
| 1 | Decodificação canônica estrita | erro de decodificação |
| 2 | `version == 1` | `UnsupportedVersion` |
| 3 | `amount > 0` | `ZeroAmount` |
| 4 | `fee ≥ genesis.min_fee` | `FeeTooLow` |
| 5 | Assinatura válida para `sender` e `network_id` | `Signature` |
| 6 | `nonce == conta(sender).nonce` | `BadNonce` |
| 7 | `amount + fee` não excede `u64` | `Overflow` |
| 8 | `conta(sender).balance ≥ amount + fee` | `InsufficientBalance` |
| 9 | Saldo do destinatário + `amount` não excede `u64` | `Overflow` |

## 5. Efeito (Transfer)

```text
sender.balance -= amount + fee
sender.nonce   += 1
to.balance     += amount
fees_do_bloco  += fee
```

Se `to == address(sender)`, o efeito líquido é apenas `-fee` e `nonce += 1`.

A aplicação é atômica: se qualquer regra falhar, o estado não é alterado.

## 6. Replay e double spend

* A mesma transação não pode ser aplicada duas vezes: após a primeira aplicação, o nonce da conta muda e a regra 6 falha.
* Duas transações diferentes com o mesmo nonce são mutuamente exclusivas: apenas uma pode ser incluída na cadeia (`SPEC §11`).
* Transações de outra rede falham na regra 5 (THR-ID-003).

## 7. Testes de aceitação cobertos

AT-CAN-001..003, AT-TID-001..002, AT-TX-001..007, AT-DS-001.
