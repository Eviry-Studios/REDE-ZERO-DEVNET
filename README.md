# REDE-ZERO-DEVNET

Implementação de referência e documentação da **Rede Zero / Exonet**, em fase **DEVNET**.

> *"A Internet fornece o caminho. A Exonet define o espaço."*

A Rede Zero é uma rede descentralizada sobreposta à Internet, sem administrador central nem chave-mestra, com moeda nativa única (**ZERO**), privacidade por padrão, governança comunitária e continuidade independente de qualquer pessoa, empresa ou servidor. Veja o [Manifesto](docs/Manifesto_Exonet.pdf).

> ⚠️ **DEVNET** — ambiente de desenvolvimento. O ZERO desta rede **não possui valor econômico**. Algumas propriedades exigidas pelos requisitos (consenso definitivo, canal cifrado, governança) ainda **não** estão implementadas, e a camada de privacidade ainda **não foi auditada**; veja [limitações](#limitações-conhecidas-da-devnet).

---

## Início rápido

Pré-requisito: [Rust](https://rustup.rs) estável.

```bash
# Compila e roda todos os testes
cargo test --workspace

# Sobe uma DEVNET local com 3 validadores (Ctrl+C encerra)
scripts/devnet.sh 3
```

Em outro terminal:

```bash
W=target/release/zero-wallet
G=devnet-data/genesis.bin

# Estado da rede
$W status  --genesis $G --node 127.0.0.1:7100

# Nova carteira (mostra o endereço privado zs… e o transparente)
$W new --key devnet-data/alice.key
ALICE=$($W address --key devnet-data/alice.key | awk '/privado/{print $2}')

# O faucet do Genesis é transparente: primeiro blinda parte do saldo…
$W shield  --genesis $G --node 127.0.0.1:7100 --key devnet-data/faucet.key --amount 1000
# …depois envia de forma PRIVADA (remetente, destinatário e valor ocultos)
$W send    --genesis $G --node 127.0.0.1:7101 --key devnet-data/faucet.key --to $ALICE --amount 10
# Alice vê o saldo privado varrendo as notas localmente, em qualquer node
$W balance --genesis $G --node 127.0.0.1:7102 --key devnet-data/alice.key
```

Espere alguns segundos entre os comandos, para a transação anterior entrar em um bloco.

Chaves e dados ficam em `devnet-data/`, que **nunca** é versionado.

---

## Estrutura

```text
docs/                 documentos normativos (Manifesto → Requisitos → Ameaças → Arquitetura → Especificações)
docs/adr/             registros de decisão técnica, com escopo (DEVNET/TESTNET/MAINNET)
spec/                 especificações especializadas, testáveis, com vetores
crates/
  rz-codec            codificação canônica estrita             spec/ENCODING.md
  rz-crypto           BLAKE3, Ed25519, identidades, chaves     spec/CRYPTOGRAPHY.md
  rz-privacy          endereços furtivos, Pedersen, Bulletproofs, CLSAG            spec/PRIVACY.md
  rz-core             transações (conta e privadas), estado, ZERO, blocos, Genesis spec/TRANSACTIONS.md, STATE.md, BLOCKS.md
  rz-chain            consenso (interface + DEVNET), forks, finalidade, mempool   spec/CONSENSUS.md
  rz-p2p              mensagens, enquadramento, handshake, limites, cliente       spec/P2P.md
  rz-node             node: rede, Dandelion++, sincronização, produção de blocos, disco
  rz-wallet           wallet privada por padrão (biblioteca + CLI, separada do node)
scripts/devnet.sh     DEVNET local com N validadores
```

## Documentos

| Documento | Pergunta | Versão |
|---|---|---|
| [Manifesto Exonet](docs/Manifesto_Exonet.pdf) | Por quê? | inicial |
| [REQUIREMENTS.md](docs/REQUIREMENTS.md) | O que deve existir? | 0.2.0 |
| [THREAT_MODEL.md](docs/THREAT_MODEL.md) | Contra o quê? | 0.2.0 |
| [ARCHITECTURE.md](docs/ARCHITECTURE.md) | Como os componentes se organizam? | 0.1.0 |
| [SPECIFICATIONS.md](docs/SPECIFICATIONS.md) | Quais são as regras técnicas? | 0.1.0 |
| [spec/](spec/) | Regras exatas e testáveis por componente | DEVNET 0.1.0 |
| [ACCEPTANCE_CRITERIA.md](docs/ACCEPTANCE_CRITERIA.md) | Quando uma implementação é conforme? | 0.1.0 |
| [ACCEPTANCE_TESTS.md](docs/ACCEPTANCE_TESTS.md) | Como verificar? | 0.1.0 |
| [docs/adr/](docs/adr/) | Por que cada escolha técnica? | — |

## Estado da implementação

Seguindo a ordem recomendada em `SPECIFICATIONS.md §73`:

| Componente | Estado | Testes de aceitação |
|---|---|---|
| Criptografia e identidade | ✅ DEVNET | AT-ID-001..006 |
| Wallet (separada, assinatura local) | ✅ DEVNET | AT-WAL-001..003 |
| Codificação canônica | ✅ DEVNET | AT-CAN-001..003 |
| Transações e Transaction ID | ✅ DEVNET | AT-TX-001..007, AT-TID-001..002 |
| Estado, ZERO e taxas | ✅ DEVNET | AT-STATE, AT-MONEY, AT-ZERO, AT-FEE, AT-DS |
| Blocos e Genesis | ✅ DEVNET | AT-BLOCK-001..005, AT-GEN-001..002 |
| P2P e sincronização | ✅ DEVNET | AT-P2P-001..005, AT-SYNC-001..003 |
| Consenso | ⚠️ provisório (DEVNET apenas) | AT-CON-001..003, AT-FORK-001..002 |
| Privacidade transacional (RingCT) | ✅ DEVNET — auditoria pendente | testes em `rz-privacy`, `rz-core`, `rz-wallet` |
| Privacidade de rede (Dandelion++) | ✅ DEVNET | teste de haste/embargo em `rz-node` |
| Governança | 📝 proposta ([ADR-0008](docs/adr/0008-governanca-bicameral.md)) | AT-GOV previstos |
| Exonet, Comunidades, Navegador Zero | ⏳ | — |
| Grande Mercado e Pool | ⏳ | — |
| Defesa | ⏳ | — |

## Limitações conhecidas da DEVNET

Aceitas temporariamente e registradas em [`THREAT_MODEL.md §11`](docs/THREAT_MODEL.md):

* **Consenso** por autoridade rotativa com validadores fixos no Genesis ([ADR-0006](docs/adr/0006-consenso-devnet.md)) — não atende `REQ-005`.
* **Privacidade não auditada**: RingCT ([ADR-0009](docs/adr/0009-privacidade-transacional.md)) oferece anonimato probabilístico (anel de 11), não absoluto (`REQ-028`). Blindagens e retiradas são públicas.
* **Canal P2P sem cifragem**; Dandelion++ ([ADR-0010](docs/adr/0010-privacidade-de-rede.md)) protege a origem contra a rede, mas a Wallet revela seu IP ao primeiro node.
* **Governança** ainda não implementada (proposta em [ADR-0008](docs/adr/0008-governanca-bicameral.md)).
* **Chaves em arquivo local** sem cifragem (permissão `0600`).
* Sem emissão após o Genesis; política monetária a definir.

## Contribuição e segurança

* [CONTRIBUTING.md](CONTRIBUTING.md) — como contribuir
* [SECURITY.md](SECURITY.md) — como reportar vulnerabilidades
* [CHANGELOG.md](CHANGELOG.md) — histórico de alterações

## Licença

[MIT](LICENSE). Código e documentação podem ser usados, modificados e redistribuídos livremente, inclusive em forks (`REQ-080`).
