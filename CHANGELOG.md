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

### Adicionado — Voto secreto por compromisso e revelação

* `spec/GOVERNANCE.md` v0.4.0 §3.2 (THR-GOV-006, `SPEC §45`).
* `CommitVote` (tag `0x18`) durante a votação e `RevealVote` (tag `0x19`) no período de revelação. A apuração ocorre no fim da revelação, e compromissos não revelados não contam. Vale o último voto, público ou secreto.
* O modo vem desligado (`vote_reveal_blocks = 0`); quem decide ligá-lo é a governança, por parâmetro constitucional (tag 37). O período vigente na submissão vale para a proposta inteira.
* Comandos `zero-wallet vote --secret [--salt-out]` e `vote-reveal`. Vetores de conformidade regenerados.
* Não é resistente a coerção (o votante pode provar o voto); isso está documentado.

### Adicionado — Envios privados pelo Navegador Zero

* A página `/carteira` mostra o saldo privado, com as notas varridas localmente, e passa a oferecer envio privado (RingCT) para `zs…`, envio transparente para hexadecimal e blindagem.
* Pedidos de publicações aceitam `para=zs…` (envio privado) e `tipo=blindar`, sempre com aprovação na interface e aviso de privacidade.
* Na Wallet do Navegador, a conta é resolvida pela chave (correto também depois de uma rotação, ADR-0023).
* `spec/BROWSER.md` v0.2.0.

### Adicionado — Redes da ponte adotadas por governança

* `spec/BRIDGE.md` v0.2.0 §2.1 resolve a pendência.
* `PublishBridgeChain` (tag `0x75`): publica a configuração de uma nova rede de origem, com a mesma validação do Genesis, só em DEVNET e TESTNET, custo queimado. A adoção depende de proposta constitucional com `bridge_add_chain` (tag 36). Na ativação, a rede e o ativo `LightClientBurn` (oferta zero) entram. Configurações não adotadas expiram.
* Comandos `zero-wallet bridge-chain-new` e `bridge-chain-publish`. THR-BRG-005. Vetores de conformidade regenerados.

### Adicionado — Validade e renovação de nomes `zero://`

* `spec/NAMING.md` v0.2.0 §4.1 resolve a pendência de renovação.
* Os nomes passam a valer `name_period_blocks` (padrão de cerca de 1 ano; parâmetro constitucional, tag 35). A renovação usa `RenewName` (tag `0x45`), com `name_fee` destinada ao Pool; só o dono renova, e no máximo dois períodos à frente.
* Um nome vencido não resolve e não se altera. Depois de uma carência de 1/12 do período, fica livre para qualquer pessoa. Os nomes `.comunidade` não vencem.
* `RESOLVED` ganha `expires_at` (`P2P_VERSION = 13`). Novo comando `zero-wallet name-renew`; `resolve` mostra o vencimento.
* Vetores de conformidade regenerados (mudam o registro de nome e os parâmetros).

### Adicionado — Retirada e restauração de Comunidade por governança

* `spec/COMMUNITIES.md` v0.2.0 §6.1 (REQ-074, THR-COM-003).
* Alterações de governança `community_withdraw` (tag 33) e `community_restore` (tag 34), só em propostas constitucionais. A Comunidade precisa estar no estado de partida tanto na submissão quanto na ativação.
* Novo estado `retirada`. O registro, o nome e o armazenamento ficam, e o nome não pode ser redeclarado. O `.comunidade` deixa de resolver, e posição, vinculação e chamada de módulo (transação e consulta) são recusadas. A restauração devolve tudo.
* `CallModule` e a consulta ao Runtime passam a exigir Comunidade reconhecida.

### Adicionado — Testes de arquitetura: eclipse, Sybil e corrupção

* ADR-0024 e `spec/P2P.md §7.1–§7.2` (THR-P2P-001, `ACCEPTANCE_TESTS.md §74, §77, §79`).
* Livro de endereços com faixas de rede: 32 endereços por `/16` (`/32` em IPv6), 64 por faixa de origem e despejo aleatório da faixa mais cheia. Antes, guardava os primeiros 1 024 para sempre.
* Conexões de saída ao acaso, no máximo uma por faixa, com os pares de partida primeiro. Antes, eram escolhidas na ordem do conjunto.
* Entradas limitadas também por faixa (16).
* Detecção de estagnação: sem blocos novos pela janela, um par de saída é trocado e fica em espera.
* Disco: o arquivo de blocos é cortado no primeiro registro ilegível ou bloco que não verifica, e a cadeia é reconstruída pelos pares. As gravações usam `fsync` e o registro informa o que foi descartado.
* Testes `crates/rz-node/tests/architecture.rs`: inundação de endereços sem eclipse, troca de par parado, conexões Sybil limitadas, disco corrompido cortado e reconstruído. Testes unitários do livro de endereços.
* `THREAT_MODEL.md` v0.15.0, `AUDIT.md` v0.12.0, `spec/P2P.md` v0.13.0.

### Adicionado — Chaves cifradas, rotação e recuperação

* ADR-0023 e `spec/KEYS.md` (SPEC-ID-001, SPEC-ID-005, REQ-012).
* Arquivo de chave cifrado com Argon2id e XChaCha20-Poly1305: cabeçalho autenticado e limites de derivação na leitura. Os arquivos em aberto continuam legíveis. Chaves novas da Wallet são cifradas por padrão (`--plaintext` só para a DEVNET). Novo comando `zero-wallet key-encrypt`. A senha vem de `--passphrase-file`, de `RZ_KEY_PASSPHRASE` ou do terminal, na Wallet, no Node (`--validator-key`) e no Navegador.
* Rotação de chave de conta sem trocar de endereço (`RotateKey`, tag `0x15`), com prova de posse da nova chave. A chave antiga é recusada no mempool e nos blocos.
* Recuperação por chave de recuperação com prazo contestável (`SetRecovery` `0x16`, `RecoverAccount` `0x17`).
* Pontos de contribuição, vínculos de validador e notas privadas não migram (GOV-INV-5). Chaves delegadas não se vinculam. Transferências para o endereço de origem de uma chave delegada são recusadas.
* Estado `keys_root`; consulta `GET_KEY`/`KEY` (`P2P_VERSION = 12`). Comandos `key-info`, `key-rotate`, `recovery-set` e `recover`. A Wallet resolve a conta da chave antes de assinar.
* Vetores de conformidade regenerados: a raiz do estado ganhou a parte de chaves, e a cadeia passa a incluir rotação, recuperação e bloco com chave substituída.
* `spec/CRYPTOGRAPHY.md` v0.2.0, `THREAT_MODEL.md` v0.14.0, `AUDIT.md` v0.11.0 (INV-30).

### Adicionado — Suíte de conformidade

* ADR-0022 e `spec/CONFORMANCE.md` (`SPEC §68–§69`): vetores em texto simples em `conformance/`, executáveis por qualquer implementação. Cobrem identidade e assinaturas, hashes, ZERO, codificação canônica, regras sem estado das transações, P2P, decaimento de pontos, Runtime, ponte e manifestos de versão.
* O conjunto `cadeia.txt` traz um Genesis e 31 blocos com a raiz do estado após cada um. Passa por transferências, privacidade, governança, Comunidades, nomes, Runtime, Mercado e Pool, ponte e troca atômica, defesa e adoção de versão, e inclui blocos inválidos que precisam ser rejeitados pela regra indicada.
* Crate `rz-conformance` com o verificador de referência e o gerador (`zero-conformidade verificar|gerar`); verificação na CI. O campo `tests` dos manifestos de versão é o hash de árvore de `conformance/`.
* `THREAT_MODEL.md` v0.13.0, `AUDIT.md` v0.10.0.

### Adicionado — Atualizações verificáveis

* ADR-0021 e `spec/RELEASES.md`: manifesto de versão (`SPEC §63`) com versões de software, protocolo e P2P, compatibilidade, versão anterior, hashes de árvore da especificação, do código e dos testes, e hashes dos artefatos. `release_id` é o hash do manifesto.
* `PublishRelease` (tag `0x80`, custo queimado); propostas com `release_id` só são aceitas se forem constitucionais e apontarem para uma versão publicada, não adotada, sucessora da vigente e sem regressão de protocolo. Na ativação, a versão é adotada. Versões não adotadas expiram do registro.
* Efeitos das categorias de compatibilidade (`SPEC §65`); aviso de atualização no Node (`NodeStatus.upgrade`), sem atualização automática.
* Estado `releases_root`; consulta `GET_RELEASES`/`RELEASES` (`P2P_VERSION = 11`); comandos `zero-wallet release-new`, `release-publish`, `releases`, `release-verify`.
* `THREAT_MODEL.md` v0.12.0 (THR-DEV-008/009), `AUDIT.md` v0.9.0 (INV-29).

### Adicionado — Reputação

* ADR-0020 e `spec/REPUTATION.md`: a reputação é o saldo de pontos de contribuição da identidade criptográfica, com decaimento por meia-vida. Pontuam só eventos verificáveis: bloco produzido e contribuição defensiva atestada por mais de 2/3 após o encerramento (50 pontos, parâmetro constitucional `defense_contribution_points`, tag 32). Punições com evidência zeram os pontos.
* Registro de eventos de reputação no estado (`reputation_root`); consulta `GET_REPUTATION`/`REPUTATION` (`P2P_VERSION = 10`); comando `zero-wallet reputation`.
* `THREAT_MODEL.md` v0.11.0, `AUDIT.md` v0.8.0.

### Alterado — retirada do Agente Zero

* O Agente Zero, já retirado do núcleo pelos Requisitos (v0.2.0, §23), não será implementado. Referências residuais removidas do modelo de ameaças (v0.10.0), da auditoria e das ADR-0008 e ADR-0016. Os identificadores AS-13, ADV-11, LT-7 e THR-AGT-001..003 ficam reservados. O nome `zero://agente-zero` continua reservado contra imitação.
* `spec/COMMUNITIES.md` atualizado: o Exonet Runtime existe (ADR-0018).

### Adicionado — Ponte sem custodiante e trocas atômicas

* ADR-0019 e `spec/BRIDGE.md`:
  * **cliente leve** de redes de prova de trabalho na cadeia: dificuldade exigida, reajuste do Bitcoin (vetores do Bitcoin Core), mediana de tempo, maior trabalho acumulado, reorganização e poda;
  * **prova de queima**: transação sem testemunhas com saída `OP_RETURN` (`RZ1` + etiqueta da rede + destinatário), prova de Merkle, confirmações mínimas, cunhagem única; extração do caminho a partir de `merkleblock`;
  * **trocas atômicas (HTLC)** com SHA-256, pré-imagem de 32 bytes e prazo em altura, para ZERO e ativos externos;
  * redes de origem no Genesis (`bridges`), só em DEVNET e TESTNET; ativo com `Verification::LightClientBurn` e oferta inicial zero.
* Operações `0x70–0x74`; mensagens `GET_BRIDGE`/`BRIDGE`, `GET_HTLC`/`HTLC` (`P2P_VERSION = 9`); comandos da Wallet `bridge`, `bridge-burn-script`, `bridge-headers`, `bridge-deposit`, `htlc-secret`, `htlc-lock`, `htlc-claim`, `htlc-refund`, `htlc`.
* Testes de estado, de extração de `merkleblock` e ponta a ponta com Nodes reais.
* `THREAT_MODEL.md` v0.9.0 (THR-BRG-001..004), `AUDIT.md` v0.7.0.

### Adicionado — Exonet Runtime

* ADR-0018 e `spec/RUNTIME.md`:
  * lógica de Comunidades em **WebAssembly** executado por `wasmi` 2.0.0 (versão fixada, parte do protocolo), sem ponto flutuante, com despacho portátil, limites fixos de memória e pilha, e combustível medido por instrução;
  * módulos publicados na cadeia (taxa por byte para o Pool) e vinculados pela regra de decisão da Comunidade reconhecida, com contador contra repetição;
  * chamadas como transações: combustível pago mesmo em falha; falhas não gravam nada nem invalidam o bloco; recibos no estado; limite de combustível por chamada e por bloco;
  * cada módulo só alcança o armazenamento da própria Comunidade, com cota; não existe função sobre saldos, ZERO, consenso, governança ou outras Comunidades.
* Operações `PublishModule` (0x60), `BindModule` (0x61), `CallModule` (0x62); parâmetros constitucionais `runtime_*` (tags 27–31); mensagens `QUERY`/`QUERY_RESULT`, `GET_RECEIPT`/`RECEIPT`, `GET_MODULE_INFO`/`MODULE_INFO` (`P2P_VERSION = 8`).
* Wallet: `module-check`, `module-publish`, `module-bind-approve`, `module-bind`, `call`, `query`, `receipt`. Navegador: `/.zero/consulta`, `/.zero/recibo/TX` e pedidos `tipo=chamada`.
* Exemplo `examples/runtime/contador.wat`, testado de ponta a ponta.
* Testes do teste de isolamento §73, determinismo na reaplicação do bloco, limites e falhas.
* `THREAT_MODEL.md` v0.8.0 (THR-RT-001/002), `AUDIT.md` v0.6.0.

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

* **Incompatível** com DEVNETs anteriores à reputação: parâmetros de defesa com `contribution_points`, raiz do estado com `reputation_root`, `P2P_VERSION = 10`.
* **Incompatível** com DEVNETs anteriores à ponte: Genesis com `bridges`, raiz do estado com `bridge_root`, `P2P_VERSION = 9`.
* **Incompatível** com DEVNETs anteriores ao Exonet Runtime: parâmetros do Runtime no estado, raiz do estado com `runtime_root`, `P2P_VERSION = 8`.
* **Incompatível** com Nodes anteriores ao Navegador Zero: novas mensagens de conteúdo, `P2P_VERSION = 7` (o estado não muda).
* **Incompatível** com DEVNETs anteriores às Comunidades e à Defesa: parâmetros de Comunidades e defesa no estado, raiz do estado com `community_root` e `defense_root`, `P2P_VERSION = 6`.
* **Incompatível** com DEVNETs anteriores ao Grande Mercado: Genesis com `assets`, parâmetros do mercado no estado, raiz do estado com `market_root`, `P2P_VERSION = 4`.
* **Incompatível** com DEVNETs anteriores ao Zero-BFT: o cabeçalho troca `slot` por `round`, o Genesis troca tempo/slot/finalidade por `ConsensusParams` e validadores com vínculo, blocos trafegam com certificado e `P2P_VERSION = 3`. Recrie a DEVNET (`rm -rf devnet-data`).

* `PROTOCOL_VERSION = 1` (DEVNET, não congelado). O Genesis passou a incluir parâmetros de governança.
* **Incompatível** com blocos anteriores à camada privada: `Transaction` passou a ter tag de tipo, e a raiz do estado inclui a parte privada. DEVNETs antigas devem ser recriadas (`rm -rf devnet-data`).
