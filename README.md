# REDE-ZERO-DEVNET

Implementação de referência e documentação da **Rede Zero / Exonet**, em fase **DEVNET**.

> *"A Internet fornece o caminho. A Exonet define o espaço."*

A Rede Zero é uma rede descentralizada sobreposta à Internet, sem administrador central nem chave-mestra, com moeda nativa única (**ZERO**), privacidade por padrão, governança comunitária e continuidade independente de qualquer pessoa, empresa ou servidor. Veja o [Manifesto](docs/Manifesto_Exonet.pdf).

> ⚠️ **DEVNET** — ambiente de desenvolvimento. O ZERO desta rede **não possui valor econômico**. O consenso Zero-BFT e as camadas de privacidade (transações, canal cifrado) ainda **não foram auditados**; veja [limitações](#limitações-conhecidas-da-devnet).

---

## Início rápido

Pré-requisito: [Rust](https://rustup.rs) estável.

```bash
# Compila e roda todos os testes
cargo test --workspace

# Sobe uma DEVNET local com 4 validadores (Ctrl+C encerra)
scripts/devnet.sh 4
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

### Governança

```bash
# Câmara econômica: bloqueie ZERO (1× a 4× conforme a duração)
$W lock    --genesis $G --node 127.0.0.1:7100 --key devnet-data/faucet.key --amount 100000 --blocks 400000
# Proposta ordinária (o texto fica fora da cadeia; vai só o hash)
echo "Reduzir a taxa mínima" > proposta.txt
$W propose --genesis $G --node 127.0.0.1:7100 --key devnet-data/faucet.key \
           --category ordinaria --content proposta.txt --params min_fee=500
# Listar propostas e votar (validadores votam na câmara de contribuição com a chave de validador)
$W governance --genesis $G --node 127.0.0.1:7100
$W vote    --genesis $G --node 127.0.0.1:7100 --key devnet-data/validator-0.key --proposal ID --choice sim
```

### Tornar-se validador (Zero-BFT)

```bash
# Vincule ao menos o mínimo (1000 ZERO na DEVNET); entra no conjunto na próxima época
$W bond   --genesis $G --node 127.0.0.1:7100 --key devnet-data/faucet.key --amount 5000
# Rode um node com essa chave como validador
target/release/rede-zero-node run --genesis $G --data devnet-data/node-extra \
    --listen 127.0.0.1:7110 --peer 127.0.0.1:7100 --validator-key devnet-data/faucet.key
# Saída: o valor fica retido (e punível) durante o período de desvinculação
$W unbond --genesis $G --node 127.0.0.1:7100 --key devnet-data/faucet.key --amount 5000
```

Todo bloco é final assim que recebe pré-compromissos de mais de 2/3 do poder de voto. Não há reorganização. Votar duas vezes na mesma rodada queima parte do vínculo e exclui o validador (`spec/CONSENSUS.md §6`).

### Privacidade do seu IP

Por padrão a Wallet só conecta a nodes **locais**. Para usar um node remoto sem expor seu IP, use Tor:

```bash
$W balance --genesis $G --key alice.key --proxy 127.0.0.1:9050 --node exemplo.onion:7100 --node-id ID_DO_NODE
```

Melhor ainda: rode seu próprio node privado, que não aceita conexões nem é anunciado, e aponte a Wallet para ele:

```bash
target/release/rede-zero-node run --genesis $G --data meu-node --no-listen --peer 127.0.0.1:7100 [--proxy 127.0.0.1:9050]
```

Detalhes em [ADR-0011](docs/adr/0011-protecao-do-ip.md).

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
  rz-core             transações, estado, ZERO, governança, blocos, Genesis      spec/TRANSACTIONS.md, STATE.md, BLOCKS.md, GOVERNANCE.md
  rz-chain            consenso Zero-BFT, cadeia finalizada, mempool               spec/CONSENSUS.md
  rz-p2p              canal cifrado, SOCKS5/Tor, mensagens, handshake, limites    spec/P2P.md
  rz-node             node: rede, Dandelion++, sincronização, produção de blocos, disco
  rz-wallet           wallet privada por padrão (biblioteca + CLI, separada do node)
scripts/devnet.sh     DEVNET local com N validadores
```

## Documentos

| Documento | Pergunta | Versão |
|---|---|---|
| [Manifesto Exonet](docs/Manifesto_Exonet.pdf) | Por quê? | inicial |
| [REQUIREMENTS.md](docs/REQUIREMENTS.md) | O que deve existir? | 0.2.0 |
| [THREAT_MODEL.md](docs/THREAT_MODEL.md) | Contra o quê? | 0.4.0 |
| [DEMOCRACIA_ORGANICA.md](docs/DEMOCRACIA_ORGANICA.md) | Como indivíduos e Comunidades participam? | 0.1.0 (conceitual) |
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
| Consenso Zero-BFT (finalidade imediata, validadores por vínculo, punição) | ✅ DEVNET — auditoria pendente ([ADR-0012](docs/adr/0012-consenso-zero-bft.md)) | AT-CON-001..003, simulação em `rz-chain` |
| Privacidade transacional (RingCT) | ✅ DEVNET — auditoria pendente | testes em `rz-privacy`, `rz-core`, `rz-wallet` |
| Privacidade de rede (Dandelion++) | ✅ DEVNET | teste de haste/embargo em `rz-node` |
| Proteção do IP (canal cifrado, Tor, node privado) | ✅ DEVNET — auditoria pendente | `rz-p2p`, `rz-wallet/tests/network_privacy.rs` |
| Governança bicameral | ✅ DEVNET ([ADR-0008](docs/adr/0008-governanca-bicameral.md)) | AT-GOV-001..005 + propriedades |
| Exonet, Comunidades, Navegador Zero | ⏳ | — |
| Grande Mercado e Pool | ⏳ | — |
| Defesa | ⏳ | — |

## Limitações conhecidas da DEVNET

Aceitas temporariamente e registradas em [`THREAT_MODEL.md §11`](docs/THREAT_MODEL.md):

* **Consenso Zero-BFT não auditado** ([ADR-0012](docs/adr/0012-consenso-zero-bft.md)): tolera menos de 1/3 do poder bizantino; com 1/3 ou mais offline a rede para (segurança antes de disponibilidade). Sem checkpoints contra ataque de longo alcance; o poder inicial vem do Genesis.
* **Privacidade não auditada**: RingCT ([ADR-0009](docs/adr/0009-privacidade-transacional.md)) oferece anonimato probabilístico (anel de 11), não absoluto (`REQ-028`). Blindagens e retiradas são públicas.
* **Canal cifrado e proteção de IP não auditados** ([ADR-0011](docs/adr/0011-protecao-do-ip.md)). Sem Tor e com `--direct`, o node escolhido vê seu IP.
* **Governança**: a câmara de contribuição só pontua a produção de blocos, o que favorece validadores; votos são públicos nesta versão.
* **Chaves em arquivo local** sem cifragem (permissão `0600`).
* Sem emissão após o Genesis; política monetária a definir.

## Contribuição e segurança

* [CONTRIBUTING.md](CONTRIBUTING.md) — como contribuir
* [SECURITY.md](SECURITY.md) — como reportar vulnerabilidades
* [CHANGELOG.md](CHANGELOG.md) — histórico de alterações

## Licença

[MIT](LICENSE). Código e documentação podem ser usados, modificados e redistribuídos livremente, inclusive em forks (`REQ-080`).
