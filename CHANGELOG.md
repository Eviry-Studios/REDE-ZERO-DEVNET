# Changelog

Todas as alterações relevantes do projeto (`REQ-086`, `REQ-094`). O formato segue [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).

## [Não lançado]

### Adicionado

* `docs/THREAT_MODEL.md` v0.1.0, completando a cadeia documental.
* Registros de decisão (ADR-0001 a ADR-0007) com escopo explícito.
* `SECURITY.md`, `CONTRIBUTING.md`.
* Especificações DEVNET: `spec/ENCODING.md`, `CRYPTOGRAPHY.md`, `TRANSACTIONS.md`, `STATE.md`, `BLOCKS.md`, `CONSENSUS.md`, `P2P.md`.
* Implementação de referência em Rust:
  * `rz-codec` — codificação canônica estrita;
  * `rz-crypto` — BLAKE3 com separação de domínio, Ed25519 estrito, identidades, arquivo de chave;
  * `rz-core` — transações, estado, ZERO, blocos, Genesis;
  * `rz-chain` — interface de consenso, autoridade rotativa (DEVNET), escolha de fork, finalidade, mempool;
  * `rz-p2p` — mensagens, enquadramento, handshake, limite de taxa, pontuação, cliente;
  * `rz-node` — node com sincronização, produção de blocos e persistência;
  * `rz-wallet` — wallet de linha de comando.
* `scripts/devnet.sh` para DEVNET local.

### Adicionado — privacidade e governança

* Licença MIT.
* Proposta de governança bicameral (ADR-0008, `spec/GOVERNANCE.md`).
* Privacidade transacional RingCT sobre Ristretto255 (ADR-0009, `spec/PRIVACY.md`):
  * crate `rz-privacy` — endereços furtivos, compromissos de Pedersen, Bulletproofs, CLSAG, prova de excesso;
  * transações privadas, operação `Shield`, retirada (`unshield`), controle público da oferta privada;
  * Wallet privada por padrão com varredura local de notas.
* Privacidade de rede Dandelion++ (ADR-0010).
* `THREAT_MODEL.md` v0.2.0.

### Adicionado — governança e proteção de IP

* Governança bicameral implementada (ADR-0008 aceita): `LockStake`, `Unlock`, `Propose`, `Vote`, `ReportEquivocation`; parâmetros do protocolo no estado; apuração nas duas câmaras; depósito devolvido ou queimado; comandos de governança na Wallet.
* Proteção do IP do usuário (ADR-0011): canal cifrado e autenticado com preenchimento; Tor/I2P via SOCKS5 com endereços `.onion`; Node privado (`--no-listen`); Wallet recusa conexão direta a Node remoto sem consentimento; logs sem endereços de clientes.
* `THREAT_MODEL.md` v0.3.0.

### Adicionado — consenso Zero-BFT e Democracia Orgânica

* Consenso **Zero-BFT** (ADR-0012, `spec/CONSENSUS.md` v0.2.0), substituindo a autoridade rotativa (ADR-0006):
  * algoritmo de Tendermint (proposta, pré-voto, pré-compromisso, trava e rodadas) com finalidade imediata e sem reorganização;
  * poder de voto igual ao ZERO vinculado; conjunto aberto recomputado por época; proponente por sorteio ponderado;
  * operações `Bond`, `Unbond` (com período de desvinculação) e `ReportDoubleVote`; punição queima `slash_bps`, exclui o validador e zera seus pontos de contribuição;
  * certificados `Commit` verificados na importação e na sincronização (`CommittedBlock`);
  * parâmetros de consenso no estado e alteráveis por governança constitucional;
  * máquina de estados determinística em `rz-chain` testada em rede simulada (proponente offline, voto duplo, partição, falta de quórum);
  * comandos `bond` e `unbond` na Wallet; `init-devnet --block-ms --stake` e padrão de 4 validadores.
* `docs/DEMOCRACIA_ORGANICA.md` incorporado com notas de conformidade (ADR-0013).
* `THREAT_MODEL.md` v0.4.0.

### Adicionado — Comunidades e nomes `zero://`

* ADR-0015, `spec/COMMUNITIES.md` e `spec/NAMING.md`, conforme as notas N-1..N-8 da Democracia Orgânica:
  * declaração de Comunidade com manifesto e regra de decisão (limiar de chaves); membros não são registrados;
  * reconhecimento por proposta de categoria Comunidade que aponta para a declaração (validação técnica);
  * posição comunitária verificável, registrada e não vinculante;
  * atualização versionada aprovada pela regra vigente, com histórico;
  * nomes `zero://nome.tipo` com sintaxe objetiva, nomes reservados e rejeição de nomes confundíveis; `.comunidade` só para Comunidades reconhecidas; taxa de nome para o Pool.
* Operações `0x40–0x44`; parâmetros `name_fee` e `community_declaration_ttl_blocks`; consultas P2P `GET_COMMUNITY`/`COMMUNITY`, `RESOLVE`/`RESOLVED` (`P2P_VERSION = 5`); comandos da Wallet.
* Testes AT-COM-001..004, incluindo reconhecimento por governança e replicação com Nodes reais.

### Adicionado — Navegador Zero e conteúdo da Exonet

* ADR-0017, `spec/CONTENT.md` e `spec/BROWSER.md`:
  * **conteúdo endereçado por hash**, em pedaços de 1 MiB (até 16 MiB por objeto), verificado ao chegar em cada Node e no cliente;
  * Nodes hospedam só o que o estado referencia (alvo de nome, manifesto e interface de Comunidade reconhecida), com cota local, e replicam entre si por anúncio e sob demanda;
  * manifesto estruturado de Comunidade com `frontend` e `module`;
  * crate `rz-browser` (`zero-navegador`): interface local que usa o navegador do sistema, com uma origem isolada por publicação, política de conteúdo que impede servidores externos, interface sem scripts, pedidos de assinatura aprovados só na interface (origem + token), identidade por site que não revela a Wallet, fixação na primeira visita e alerta de nomes parecidos;
  * páginas de Wallet, Grande Mercado, governança, Comunidades, defesa e pedidos.
* Mensagens `GET_MANIFEST`/`MANIFEST`, `GET_CONTENT`/`CONTENT_INFO`, `GET_CHUNK`/`CHUNK`, `HAVE_CONTENT` (`P2P_VERSION = 7`); opção `--content-quota-mb` do Node; comandos da Wallet `content-pack`, `content-publish`, `content-get`, `manifest-new`, `manifest-publish`.
* Testes AT-BRW-001..003, replicação de interface de Comunidade entre Nodes, robustez das novas mensagens.
* `THREAT_MODEL.md` v0.7.0 (THR-BRW-003, THR-CNT-001/002), `AUDIT.md` v0.5.0.

### Adicionado — Defesa da Exonet

* ADR-0016 e `spec/DEFENSE.md`:
  * modos NORMAL → VIGILÂNCIA → INCIDENTE → GUERRA_CIBERNÉTICA decididos por atestações de validadores ponderadas por poder de voto (> 1/3 para vigilância, > 2/3 para incidente e guerra; descer exige só > 1/3), com evidência obrigatória e contador contra repetição;
  * todo modo tem duração máxima e desce um nível sozinho sem renovação (THR-DEF-001); guerra só com incidente persistente;
  * credenciais temporárias ligadas ao incidente, com escopos fechados e defensivos (diagnóstico, isolamento, recuperação, coordenação, evidências), validade máxima, revogação e registro de cada uso; só para validadores ou Nodes com contribuição verificável;
  * encerramento verificável Aberto → Contido → Recuperado → Encerrado, com retorno gradual à vigilância;
  * registro de contribuição separado de participação, só após o encerramento;
  * isolamento: Nodes deixam de se conectar à identidade isolada e derrubam conexões existentes.
* Operações `0x50–0x56`; parâmetros constitucionais `defense_vigilance_max_blocks`, `defense_incident_max_blocks`, `defense_war_max_blocks`; consulta P2P `GET_DEFENSE`/`DEFENSE` (`P2P_VERSION = 6`); comandos da Wallet `defense`, `defense-attest`, `defense-submit`, `defense-action`.
* Testes AT-DEF-001..003, AT-CYBER-001..006, AT-INC-001..003, vencimento dos modos, robustez e isolamento com Nodes reais.
* `THREAT_MODEL.md` v0.6.0, `AUDIT.md` v0.4.0.

### Adicionado — Grande Mercado e Pool permanente

* ADR-0014 e `spec/MARKET.md`:
  * **Grande Mercado** por livro de ordens com leilão de preço uniforme a cada bloco; todo par é cotado em ZERO; o resultado não depende da ordem das transações no bloco (THR-MKT-001);
  * **Pool permanente** sem nenhuma operação de saída; só cresce, por depósito irreversível ou pela tarifa do mercado (padrão 0);
  * **ativos externos** com identificador derivado da origem, impossíveis de confundir com ZERO; sem ponte nesta versão, apenas ativos de teste no Genesis da DEVNET.
* Operações `TransferAsset` (0x30), `PoolDeposit` (0x31), `PlaceOrder` (0x32), `CancelOrder` (0x33); parâmetros `market_fee_bps` e `market_order_lifetime_blocks` governáveis.
* Consultas P2P `GET_ASSETS`/`ASSETS`, `GET_MARKET`/`MARKET`; comandos da Wallet `assets`, `market`, `order`, `cancel`, `send-asset`, `pool-deposit --permanente`; `init-devnet --test-asset`.
* Testes AT-MKT-001..004, AT-POOL-001..004, propriedades sob operações aleatórias (conservação por ativo, Pool monotônico, livro não cruzado) e ponta a ponta com Nodes reais.
* `THREAT_MODEL.md` v0.5.0.

### Corrigido — achados abertos da auditoria interna

* Limite de taxa próprio para mensagens de consenso, proporcional ao número de validadores; excedente descartado sem banir (RZ-IR-06).
* `ReportDoubleProposal` (0x23): evidência compacta de proposta dupla, válida também para re-propostas (RZ-IR-07).
* Pontos de verificação do operador (`--checkpoint ALTURA:ID`) contra ataque de longo alcance (RZ-IR-08).
* Simulação adversarial aleatória do consenso (rede assíncrona, perdas, bizantinos que equivocam para metades da rede) (RZ-IR-12). Ela encontrou e levou à correção de duas falhas de vivacidade:
  * votos conflitantes de bizantinos eram descartados, e metade dos honestos não completava a prova de > 2/3 (RZ-IR-13);
  * a retransmissão não incluía a prova do bloco travado (RZ-IR-14).

### Adicionado — preparação para auditoria

* `docs/AUDIT.md`: escopo priorizado, fronteiras de confiança, invariantes a atacar, como reproduzir, dependências criptográficas e resultado da revisão interna.
* Testes de robustez por mutação sobre todos os tipos de mensagem e transação (`rz-p2p/tests/robustness.rs`): sem pânico, canonicidade, rejeição de mutantes pela verificação de estado, valores extremos assinados, quadros adulterados ou repetidos no canal cifrado.

### Corrigido — revisão interna de segurança

* Temporizadores e intervalo de bloco com limite superior (10 min); prazos somados ao relógio sem estouro (RZ-IR-01).
* Consenso: mensagens da altura seguinte só de validadores e das primeiras rodadas; janela de rodadas futuras para propostas; um voto futuro por validador (RZ-IR-02, RZ-IR-03).
* Node: mensagens de consenso marcadas como vistas só após aceitas; fila limitada para a thread de consenso (RZ-IR-04).
* Blocos limitados a 2 MiB de transações, respeitado na seleção do mempool (RZ-IR-05).

### Protocolo

* **Incompatível** com Nodes anteriores ao Navegador Zero: novas mensagens de conteúdo, `P2P_VERSION = 7` (o estado não muda).
* **Incompatível** com DEVNETs anteriores às Comunidades e à Defesa: parâmetros de Comunidades e defesa no estado, raiz do estado com `community_root` e `defense_root`, `P2P_VERSION = 6`.
* **Incompatível** com DEVNETs anteriores ao Grande Mercado: Genesis com `assets`, parâmetros do mercado no estado, raiz do estado com `market_root`, `P2P_VERSION = 4`.
* **Incompatível** com DEVNETs anteriores ao Zero-BFT: o cabeçalho troca `slot` por `round`, o Genesis troca tempo/slot/finalidade por `ConsensusParams` e validadores com vínculo, blocos trafegam com certificado e `P2P_VERSION = 3`. Recrie a DEVNET (`rm -rf devnet-data`).

* `PROTOCOL_VERSION = 1` (DEVNET, não congelado). O Genesis passou a incluir parâmetros de governança.
* **Incompatível** com blocos anteriores à camada privada: `Transaction` passou a ter tag de tipo, e a raiz do estado inclui a parte privada. DEVNETs antigas devem ser recriadas (`rm -rf devnet-data`).
