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

Para um Node novo se proteger de histórico alternativo forjado com chaves antigas (ataque de longo alcance), fixe pontos de verificação obtidos de fontes em que confia:

```bash
target/release/rede-zero-node run --genesis $G --data devnet-data/node-novo \
    --listen 127.0.0.1:7120 --peer 127.0.0.1:7100 --checkpoint ALTURA:ID_DO_BLOCO
```

Todo bloco é final assim que recebe pré-compromissos de mais de 2/3 do poder de voto. Não há reorganização. Votar duas vezes na mesma rodada queima parte do vínculo e exclui o validador (`spec/CONSENSUS.md §6`).

### Grande Mercado e Pool permanente

Crie a DEVNET com um ativo **de teste** (não representa nada na rede de origem; pontes estão A DEFINIR):

```bash
rm -rf devnet-data && RZ_TEST_ASSET=testnet-externa:ATV scripts/devnet.sh 4
# (em outro terminal)
$W assets --genesis $G --node 127.0.0.1:7100 --key devnet-data/faucet.key
# O faucet vende 100 ATV a 2 ZERO cada; um validador (que tem ZERO transparente) compra
$W order  --genesis $G --node 127.0.0.1:7100 --key devnet-data/faucet.key \
          --asset testnet-externa:ATV --side venda --amount 100 --price 2
$W order  --genesis $G --node 127.0.0.1:7100 --key devnet-data/validator-1.key \
          --asset testnet-externa:ATV --side compra --amount 50 --price 3
$W market --genesis $G --node 127.0.0.1:7100 --asset testnet-externa:ATV
# Depósito PERMANENTE no Pool: não existe operação de retirada
$W pool-deposit --genesis $G --node 127.0.0.1:7100 --key devnet-data/faucet.key \
          --asset ZERO --amount 10 --permanente
```

As ordens que cruzam num bloco executam todas ao mesmo preço (leilão por bloco): a ordem das transações no bloco não dá vantagem a ninguém. Ordens e depósitos são públicos.

### Comunidades e nomes `zero://`

```bash
# Declara zero://cientistas.comunidade com regra de decisão 2 de 3 chaves
$W community-declare --genesis $G --node 127.0.0.1:7100 --key devnet-data/faucet.key \
    --name cientistas --manifest manifesto.txt --keys HEX1,HEX2,HEX3 --threshold 2
# Reconhecimento: proposta de categoria comunidade apontando para o id da declaração
$W propose --genesis $G --node 127.0.0.1:7100 --key devnet-data/faucet.key \
    --category comunidade --content-hash ID_DA_COMUNIDADE
$W community --genesis $G --node 127.0.0.1:7100 --name cientistas
# Nomes para outras publicações
$W name-register --genesis $G --node 127.0.0.1:7100 --key devnet-data/faucet.key \
    --name zero://laboratorio.app --target HASH_DA_APLICACAO
$W resolve --genesis $G --node 127.0.0.1:7100 --name zero://laboratorio.app
```

A Rede Zero não conhece os membros de uma Comunidade: ela só verifica que suas manifestações foram aprovadas pelo limiar de chaves declarado. A posição de uma Comunidade numa votação fica registrada, mas não altera o resultado oficial.

### Defesa da Exonet

```bash
$W defense --genesis $G --node 127.0.0.1:7100
# Cada validador assina a mesma decisão (com o seq vigente) e entrega a atestação
$W defense-attest --genesis $G --node 127.0.0.1:7100 --key devnet-data/validator-0.key \
    --decision transicao --to incidente --evidence HASH_DO_PACOTE_DE_EVIDENCIAS
# Qualquer conta envia a decisão com as atestações reunidas
$W defense-submit --genesis $G --node 127.0.0.1:7100 --key devnet-data/faucet.key \
    --decision transicao --to incidente --evidence HASH_DO_PACOTE_DE_EVIDENCIAS \
    --attestations ATT1,ATT2,ATT3
# O portador de uma credencial de isolamento usa o escopo
$W defense-action --genesis $G --node 127.0.0.1:7100 --key portador.key \
    --credential ID --scope isolamento --subject ID_DO_NODE
```

Subir para vigilância exige mais de 1/3 do poder de voto; incidente, guerra cibernética, credenciais e encerramento exigem mais de 2/3; descer e revogar, mais de 1/3. Todo modo vence e desce um nível sozinho sem renovação, e não existe operação ofensiva.

### Navegador Zero e publicações `zero://`

```bash
# Publicar um site: empacotar, registrar o nome apontando para o id, enviar ao node
$W content-pack --dir meu-site --out site.pacote            # mostra o id do conteúdo
$W name-register --genesis $G --node 127.0.0.1:7100 --key devnet-data/faucet.key \
    --name zero://laboratorio.app --target ID_DO_CONTEUDO
$W content-publish --genesis $G --node 127.0.0.1:7100 --file site.pacote

# Navegar
target/release/zero-navegador --genesis $G --node 127.0.0.1:7100 --key alice.key
# abra http://navegador.localhost:7300 e digite zero://laboratorio.app
```

Cada publicação abre numa origem própria (`laboratorio.app.localhost:7300`), verificada pedaço a pedaço contra o identificador registrado, isolada das outras e sem acesso a servidores externos. Assim, ela não revela seu IP a terceiros. As chaves nunca chegam às publicações: quando um site pede uma operação, você revisa e aprova em **Pedidos**, na interface do Navegador. Cada site vê uma identidade diferente, que não revela sua Wallet.

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
  rz-browser          Navegador Zero: interface local para a Exonet (zero-navegador)
scripts/devnet.sh     DEVNET local com N validadores
```

## Documentos

| Documento | Pergunta | Versão |
|---|---|---|
| [Manifesto Exonet](docs/Manifesto_Exonet.pdf) | Por quê? | inicial |
| [REQUIREMENTS.md](docs/REQUIREMENTS.md) | O que deve existir? | 0.2.0 |
| [THREAT_MODEL.md](docs/THREAT_MODEL.md) | Contra o quê? | 0.7.0 |
| [DEMOCRACIA_ORGANICA.md](docs/DEMOCRACIA_ORGANICA.md) | Como indivíduos e Comunidades participam? | 0.1.0 (conceitual) |
| [ARCHITECTURE.md](docs/ARCHITECTURE.md) | Como os componentes se organizam? | 0.1.0 |
| [SPECIFICATIONS.md](docs/SPECIFICATIONS.md) | Quais são as regras técnicas? | 0.1.0 |
| [spec/](spec/) | Regras exatas e testáveis por componente | DEVNET 0.1.0 |
| [AUDIT.md](docs/AUDIT.md) | O que auditar, como reproduzir e o que a revisão interna encontrou? | 0.5.0 |
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
| Comunidades (declaração, reconhecimento, posição, versões) e nomes `zero://` | ✅ DEVNET ([ADR-0015](docs/adr/0015-comunidades-e-nomes.md)) | AT-COM-001..004 |
| Navegador Zero (origens isoladas, pedidos aprovados na interface, identidade por site) e conteúdo endereçado por hash replicado entre Nodes | ✅ DEVNET — auditoria pendente ([ADR-0017](docs/adr/0017-navegador-zero-e-conteudo.md)) | AT-BRW-001..003 |
| Exonet Runtime | ⏳ | — |
| Grande Mercado (leilão por bloco) e Pool permanente | ✅ DEVNET ([ADR-0014](docs/adr/0014-grande-mercado-e-pool.md)); ponte de ativos externos A DEFINIR | AT-MKT-001..004, AT-POOL-001..004, propriedades |
| Defesa da Exonet (modos atestados com prazo, credenciais temporárias, isolamento, encerramento verificável) | ✅ DEVNET — auditoria pendente ([ADR-0016](docs/adr/0016-defesa-da-exonet.md)) | AT-DEF-001..003, AT-CYBER-001..006, AT-INC-001..003 |

## Limitações conhecidas da DEVNET

Aceitas temporariamente e registradas em [`THREAT_MODEL.md §11`](docs/THREAT_MODEL.md):

* **Consenso Zero-BFT não auditado** ([ADR-0012](docs/adr/0012-consenso-zero-bft.md)): tolera menos de 1/3 do poder bizantino; com 1/3 ou mais offline a rede para (segurança antes de disponibilidade). Sem checkpoints contra ataque de longo alcance; o poder inicial vem do Genesis.
* **Privacidade não auditada**: RingCT ([ADR-0009](docs/adr/0009-privacidade-transacional.md)) oferece anonimato probabilístico (anel de 11), não absoluto (`REQ-028`). Blindagens e retiradas são públicas.
* **Canal cifrado e proteção de IP não auditados** ([ADR-0011](docs/adr/0011-protecao-do-ip.md)). Sem Tor e com `--direct`, o node escolhido vê seu IP.
* **Navegador Zero não auditado** ([ADR-0017](docs/adr/0017-navegador-zero-e-conteudo.md)): o isolamento depende do navegador do sistema; envios privados ainda pela `zero-wallet`; links para a web comum saem da Exonet.
* **Defesa não auditada** ([ADR-0016](docs/adr/0016-defesa-da-exonet.md)): depende de > 2/3 dos validadores; o isolamento só alcança conexões de saída, porque pares de entrada são anônimos; a detecção de anomalias é local.
* **Governança**: a câmara de contribuição só pontua a produção de blocos, o que favorece validadores; votos são públicos nesta versão.
* **Chaves em arquivo local** sem cifragem (permissão `0600`).
* Sem emissão após o Genesis; política monetária a definir.

## Contribuição e segurança

* [CONTRIBUTING.md](CONTRIBUTING.md) — como contribuir
* [SECURITY.md](SECURITY.md) — como reportar vulnerabilidades
* [CHANGELOG.md](CHANGELOG.md) — histórico de alterações

## Licença

[MIT](LICENSE). Código e documentação podem ser usados, modificados e redistribuídos livremente, inclusive em forks (`REQ-080`).
