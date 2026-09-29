# Especificações especializadas

Especificações modulares previstas em `docs/SPECIFICATIONS.md §72`. Cada documento transforma conceitos em regras testáveis:

```text
entrada → operação → saída esperada → regra de validação → casos de erro → teste de conformidade
```

| Documento | Estado | Implementação |
| --- | --- | --- |
| [ENCODING.md](ENCODING.md) | DEVNET v0.1.0 | `crates/rz-codec` |
| [CRYPTOGRAPHY.md](CRYPTOGRAPHY.md) | DEVNET v0.1.0 | `crates/rz-crypto` |
| [TRANSACTIONS.md](TRANSACTIONS.md) | DEVNET v0.1.0 | `crates/rz-core` |
| [STATE.md](STATE.md) | DEVNET v0.1.0 | `crates/rz-core` |
| [BLOCKS.md](BLOCKS.md) | DEVNET v0.1.0 | `crates/rz-core` |
| [CONSENSUS.md](CONSENSUS.md) | DEVNET v0.1.0 (provisório) | `crates/rz-chain` |
| [P2P.md](P2P.md) | DEVNET v0.1.0 | `crates/rz-p2p`, `crates/rz-node` |

Todas as especificações deste diretório estão em estado **DEVNET** — não congeladas (`SPECIFICATIONS.md §74`). Em caso de divergência entre código e especificação, a divergência é um defeito a ser resolvido por processo formal.
