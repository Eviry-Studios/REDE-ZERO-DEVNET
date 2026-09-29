# THREAT_MODEL.md

# Rede Zero / Exonet

**Versão:** 0.5.0
**Status:** Modelo de ameaças — fase de definição (0.5.0: Grande Mercado e Pool permanente; 0.4.0: consenso Zero-BFT com participação vinculada; 0.3.0: governança implementada; proteção do IP do usuário)
**Natureza:** Documento normativo de segurança
**Relacionamento:**

* `Manifesto Exonet`
* `REQUIREMENTS.md v0.2.0`
* `ARCHITECTURE.md v0.1.0`
* `SPECIFICATIONS.md v0.1.0`
* `ACCEPTANCE_CRITERIA.md v0.1.0`
* `ACCEPTANCE_TESTS.md v0.1.0`

---

# 1. Objetivo

Este documento responde à pergunta:

> **Contra o quê a Rede Zero precisa se defender?**

Ele identifica:

* quem pode atacar a Rede Zero;
* quais recursos podem ser atacados;
* quais são os objetivos possíveis de um atacante;
* quais informações e capacidades um atacante pode possuir;
* quais ataques devem ser considerados;
* quais ataques não fazem parte do modelo;
* quais propriedades precisam ser protegidas;
* quais mecanismos são necessários para reduzir cada ameaça.

Este documento não escolhe tecnologias. Quando uma mitigação depender de uma decisão ainda **A DEFINIR**, isso será indicado explicitamente.

A arquitetura e as especificações da Rede Zero deverão ser confrontadas com este modelo antes da implementação de componentes críticos (`REQ-081`).

---

# 2. Princípios do modelo

## TM-P-001 — Participantes maliciosos existem

O modelo parte da hipótese de que uma parcela dos participantes será maliciosa, desonesta, negligente ou comprometida.

A segurança da Rede Zero não pode depender da boa-fé de todos.

## TM-P-002 — Código aberto não é ameaça

O código, as especificações e este próprio modelo são públicos.

A segurança não pode depender de segredo de implementação. Um atacante deve ser assumido como conhecedor completo do protocolo e do código.

## TM-P-003 — A melhor defesa de uma informação é não possuí-la

Informação que o protocolo não coleta não pode ser vazada, roubada, requisitada ou correlacionada a partir da própria rede.

## TM-P-004 — Nenhuma camada é suficiente sozinha

Cada mitigação deve assumir que outra camada pode falhar.

## TM-P-005 — Pesquisa não é ataque

Leitura de código, compilação, testes, fuzzing em ambientes próprios, auditoria e divulgação responsável não são ameaças (`REQ-050`, `SPEC §50`).

O modelo trata como ameaça o **abuso da infraestrutura de terceiros**, não o estudo do sistema.

## TM-P-006 — Não prometer o impossível

O modelo não promete anonimato absoluto, segurança absoluta nem disponibilidade absoluta (`REQ-028`). Ele descreve riscos residuais de forma explícita.

---

# 3. Escopo

## 3.1 Dentro do escopo

* protocolo da Rede Zero;
* Nodes e suas identidades criptográficas;
* Wallets e chaves;
* transações, blocos, estado e consenso;
* camada P2P e sincronização;
* ZERO e regras monetárias;
* Grande Mercado e Pool permanente;
* governança e votação;
* reputação;
* modos de defesa e credenciais defensivas;
* Navegador Zero e interfaces alternativas;
* Comunidades;
* Agente Zero;
* processo de desenvolvimento, atualização e distribuição de software.

## 3.2 Fora do escopo

Ver seção 12.

---

# 4. Ativos a proteger

| ID | Ativo | Descrição |
| --- | --- | --- |
| AS-01 | Chaves privadas de Wallet | Autorizam movimentação de ZERO e ativos |
| AS-02 | Chaves privadas de Node | Autenticam a participação técnica de um Node |
| AS-03 | Estado da rede | Saldos, Pool, propostas, reputação, registros |
| AS-04 | Histórico da cadeia | Sequência verificável de blocos |
| AS-05 | Integridade monetária do ZERO | Oferta e regras de emissão |
| AS-06 | Liveness da rede | Capacidade de continuar produzindo e finalizando estado |
| AS-07 | Privacidade dos participantes | Relações entre Wallets, endereços de rede, identidades civis |
| AS-08 | Regras de governança | Propostas, votos, resultados |
| AS-09 | Reputação | Histórico criptográfico dos Nodes |
| AS-10 | Credenciais defensivas | Privilégios temporários concedidos em incidentes |
| AS-11 | Código e especificações | Repositórios, releases e documentação |
| AS-12 | Comunidades | Frontend, backend, dados e regras publicadas |
| AS-13 | Base de conhecimento do Agente Zero | Documentação, incidentes, propostas e versões |
| AS-14 | Pool permanente | Ativos depositados sob regra de permanência |

---

# 5. Propriedades de segurança

| ID | Propriedade | Ativos |
| --- | --- | --- |
| PR-01 | **Autenticidade** — somente o detentor da chave autoriza operações protegidas | AS-01, AS-02 |
| PR-02 | **Integridade do estado** — nenhuma transição inválida produz estado aceito | AS-03, AS-04 |
| PR-03 | **Integridade monetária** — ZERO não é criado nem destruído fora das regras | AS-05, AS-14 |
| PR-04 | **Determinismo** — mesmas entradas produzem o mesmo estado em qualquer implementação | AS-03 |
| PR-05 | **Liveness** — a rede continua operando sob falhas parciais | AS-06 |
| PR-06 | **Privacidade** — minimização de dados e da capacidade de correlação | AS-07 |
| PR-07 | **Ausência de autoridade absoluta** — nenhum participante controla sozinho o sistema | AS-03, AS-08, AS-10 |
| PR-08 | **Auditabilidade** — decisões relevantes são verificáveis posteriormente | AS-04, AS-08, AS-09 |
| PR-09 | **Temporalidade de privilégios** — privilégios excepcionais expiram | AS-10 |
| PR-10 | **Continuidade** — o projeto sobrevive à perda de pessoas ou infraestrutura | AS-11 |
| PR-11 | **Isolamento** — o comprometimento de um componente não se propaga automaticamente | AS-12, AS-03 |

---

# 6. Adversários

## ADV-01 — Usuário malicioso

Participa normalmente da rede e tenta obter vantagem indevida: gastar duas vezes, criar ZERO, burlar taxas, fraudar votações.

**Capacidades:** uma ou algumas identidades, recursos computacionais comuns, conhecimento completo do protocolo.

## ADV-02 — Operador de Node malicioso

Controla um ou mais Nodes e altera o software livremente.

**Capacidades:** enviar mensagens arbitrárias, omitir mensagens, mentir sobre o estado, atrasar propagação, registrar metadados de rede.

## ADV-03 — Atacante Sybil

Cria grande número de identidades criptográficas.

**Objetivo:** dominar votações, reputação, descoberta de pares ou consenso.

**Observação:** criar chaves é barato. Identidade criptográfica prova controle de chave, não unicidade de pessoa (`SPEC §47`).

## ADV-04 — Coalizão de validadores

Um conjunto de participantes do consenso atuando em conjunto.

**Objetivo:** censurar transações, reescrever histórico, produzir forks, travar a rede.

## ADV-05 — Observador de rede passivo

Provedor, operador de backbone, agência ou qualquer entidade capaz de observar tráfego em larga escala.

**Objetivo:** correlacionar endereços IP, horários e volumes com transações e identidades.

## ADV-06 — Atacante de rede ativo

Capaz de interceptar, modificar, bloquear ou injetar tráfego.

**Objetivo:** eclipse, partição, man-in-the-middle, negação de serviço.

## ADV-07 — Atacante econômico

Detém grande quantidade de ZERO ou capital externo.

**Objetivo:** concentração de governança, manipulação do Grande Mercado, front-running, ataques ao Pool.

## ADV-08 — Autor de Comunidade malicioso

Publica uma Comunidade com código hostil ou que se torna hostil após atualização.

**Objetivo:** roubar chaves, espionar usuários, atacar outros componentes, abusar de recursos de Nodes.

## ADV-09 — Atacante da cadeia de suprimentos

Compromete dependências, ferramentas de build, repositórios, contas de mantenedores ou canais de distribuição.

**Objetivo:** inserir código malicioso em releases oficiais.

## ADV-10 — Defensor abusivo

Participante que recebeu credenciais defensivas legítimas e tenta usá-las além do escopo ou prolongar a emergência.

**Objetivo:** acumular poder, punir adversários políticos, perpetuar privilégios.

## ADV-11 — Manipulador do Agente Zero

Tenta influenciar o Agente Zero por dados envenenados, injeção de instruções ou manipulação da base de conhecimento.

**Objetivo:** provocar recomendações falsas, alarmes falsos, ocultar incidentes reais ou induzir propostas prejudiciais.

## ADV-12 — Coerção externa

Entidade com poder jurídico, político ou físico sobre pessoas do projeto, operadores ou infraestrutura.

**Objetivo:** forçar alteração do protocolo, remoção de conteúdo, entrega de dados ou encerramento do projeto.

## ADV-13 — Atacante de dispositivo final

Malware, acesso físico ou comprometimento do sistema operacional do usuário.

**Objetivo:** roubar chaves, observar a atividade do usuário, assinar operações sem consentimento.

---

# 7. Suposições

As mitigações deste documento dependem das seguintes suposições. Se alguma for violada, o risco correspondente aumenta.

| ID | Suposição |
| --- | --- |
| SUP-01 | As primitivas criptográficas escolhidas permanecem seguras nos parâmetros adotados |
| SUP-02 | Uma parcela suficiente dos participantes do consenso é honesta (limite exato depende do algoritmo **A DEFINIR**) |
| SUP-03 | Um Node honesto consegue se conectar a pelo menos um par honesto |
| SUP-04 | O dispositivo do usuário não está totalmente comprometido no momento da assinatura |
| SUP-05 | Relógios dos Nodes possuem desvio limitado quando o protocolo usar dados temporais |
| SUP-06 | Existem múltiplas cópias independentes do código e das especificações |
| SUP-07 | Revisores independentes existem e analisam alterações críticas |

---

# 8. Limites de confiança

```text
┌───────────────────────────────────────────────────────────────┐
│ Dispositivo do usuário                                        │
│                                                               │
│   ┌──────────────┐     ┌──────────────────┐                   │
│   │   Wallet     │     │  Navegador Zero  │                   │
│   │ (chaves)     │     │  / interface     │                   │
│   └──────┬───────┘     └────────┬─────────┘                   │
│          │ LT-1                 │ LT-2                        │
│          ▼                      ▼                             │
│   ┌──────────────────────────────────────┐   ┌──────────────┐ │
│   │               Node local             │◄──┤ Comunidade   │ │
│   │                                      │LT-3│ (sandbox)   │ │
│   └──────────────────┬───────────────────┘   └──────────────┘ │
└──────────────────────┼────────────────────────────────────────┘
                       │ LT-4 (Internet / não confiável)
                       ▼
         ┌──────────────────────────────┐
         │  Outros Nodes (não confiáveis)│
         └──────────────┬───────────────┘
                        │ LT-5
                        ▼
         ┌──────────────────────────────┐
         │ Consenso / estado verificável │
         └──────────────────────────────┘

LT-6: Repositórios, dependências e releases → Node
LT-7: Agente Zero → Governança
LT-8: Redes externas → Pool / Grande Mercado
```

| ID | Limite | Regra |
| --- | --- | --- |
| LT-1 | Wallet → Node | O Node nunca recebe a chave privada da Wallet (`SPEC-WAL-003`) |
| LT-2 | Interface → Node | A interface não possui autoridade superior ao protocolo (`INV-008`) |
| LT-3 | Comunidade → Node | Comunidades executam isoladas, sem acesso a chaves ou consenso (`INV-007`) |
| LT-4 | Rede → Node | Toda mensagem recebida é não confiável até ser validada |
| LT-5 | Pares → estado | Estado recebido é verificado, nunca aceito cegamente (`SPEC §28`) |
| LT-6 | Distribuição → Node | Software é verificado antes de ser executado (`SPEC §64`) |
| LT-7 | Agente Zero → decisões | O Agente Zero recomenda; não decide |
| LT-8 | Externo → Pool | Ativos externos exigem verificação, não declaração de custodiante (`SPEC §42`) |

---

# 9. Ameaças

Cada ameaça possui: identificador, descrição, adversários, propriedades afetadas, mitigações e rastreabilidade.

Classificação de severidade (inicial, sujeita a revisão):

* **CRÍTICA** — compromete integridade monetária, consenso ou chaves em larga escala;
* **ALTA** — compromete privacidade em larga escala, governança ou disponibilidade prolongada;
* **MÉDIA** — impacto limitado a participantes ou componentes específicos;
* **BAIXA** — incômodo, degradação temporária ou exigência de condições improváveis.

---

## 9.1 Identidade e chaves

### THR-ID-001 — Roubo de chave privada de Wallet

**Severidade:** CRÍTICA (para o usuário afetado)
**Adversários:** ADV-13, ADV-08, ADV-09
**Propriedades:** PR-01

**Mitigações:**

* assinatura local na Wallet (`SPEC-WAL-002`);
* chaves nunca transmitidas pelo protocolo (`SPEC-ID-001`);
* armazenamento cifrado quando houver alternativa segura;
* separação entre Wallet e Node (`REQ-011`);
* Comunidades sem acesso às chaves (LT-3);
* suporte futuro a dispositivos de assinatura externos (**A DEFINIR**).

**Risco residual:** um dispositivo totalmente comprometido pode assinar operações. Nenhum protocolo elimina esse risco (seção 13).

### THR-ID-002 — Falsificação de assinatura

**Severidade:** CRÍTICA
**Adversários:** todos
**Propriedades:** PR-01, PR-02

**Mitigações:**

* algoritmo de assinatura amplamente analisado (**A DEFINIR** em `spec/CRYPTOGRAPHY.md`);
* separação de domínio: a mensagem assinada inclui um prefixo específico por tipo de objeto e o identificador da rede, impedindo reutilização de assinaturas entre contextos e redes;
* rejeição de assinaturas malformadas ou não canônicas;
* testes `AT-ID-004` a `AT-ID-006`.

### THR-ID-003 — Replay entre redes

**Severidade:** ALTA
**Adversários:** ADV-01, ADV-02
**Descrição:** uma transação assinada na DEVNET ou TESTNET é retransmitida na MAINNET (ou vice-versa).

**Mitigações:**

* identificador da rede incluído no conteúdo assinado;
* Nodes rejeitam dados de outra rede (`SPEC §66`, `AT-GEN-002`).

### THR-ID-004 — Replay dentro da mesma rede

**Severidade:** CRÍTICA
**Adversários:** ADV-01, ADV-02
**Descrição:** uma transação válida já aplicada é reenviada para ser aplicada novamente.

**Mitigações:**

* cada transação consome uma referência de estado única (nonce de conta, UTXO ou equivalente — **A DEFINIR** conforme o modelo de transação);
* rejeição de transações duplicadas (`AT-TX-007`).

### THR-ID-005 — Chave comprometida sem recuperação

**Severidade:** MÉDIA
**Adversários:** ADV-13
**Mitigações:** mecanismo de rotação de chaves (`SPEC-ID-005`, **A DEFINIR**); separação entre chave de Node e chave de Wallet.

### THR-ID-006 — Associação de identidade civil

**Severidade:** ALTA
**Adversários:** ADV-05, ADV-12, ADV-02
**Descrição:** o protocolo, uma interface ou uma Comunidade passa a exigir ou registrar dados civis.

**Mitigações:**

* nenhum campo protocolar para identidade civil (`REQ-010`, `SPEC-ID-004`);
* ausência de registro central (`REQ-027`);
* revisão obrigatória de novos campos segundo a regra de minimização (`SPEC §32`).

---

## 9.2 Transações, estado e ZERO

### THR-TX-001 — Double spend

**Severidade:** CRÍTICA
**Adversários:** ADV-01, ADV-04
**Propriedades:** PR-02, PR-03

**Mitigações:**

* validação contra o estado antes da aceitação (`SPEC §10`);
* ordenação determinística de transações em blocos;
* consumo único de referências de estado;
* finalidade explícita (`SPEC §21`);
* testes `AT-DS-001`, `AT-DS-002`.

**Risco residual:** antes da finalidade, reorganizações podem reverter transações incluídas. Interfaces devem distinguir claramente "incluída" de "finalizada".

### THR-TX-002 — Criação arbitrária de ZERO

**Severidade:** CRÍTICA
**Adversários:** ADV-02, ADV-04
**Mitigações:**

* no conjunto privado, onde valores são ocultos: equação de balanço de compromissos, provas de faixa, e **controle público da oferta privada**, que nunca pode ficar negativa. Uma falha criptográfica que crie moeda oculta não consegue retirar do conjunto privado mais do que entrou;

* emissão exclusivamente por regras protocolares (`SPEC §34`);
* todo Node verifica a soma monetária a cada bloco (`SPEC §35`);
* aritmética com verificação de overflow/underflow;
* invariantes `INV-003`; testes `AT-MONEY-002`, `AT-ZERO-003`.

### THR-TX-003 — Overflow e erros aritméticos

**Severidade:** CRÍTICA
**Descrição:** valores próximos aos limites numéricos produzem saldos incorretos ou criação implícita de moeda.

**Mitigações:**

* ZERO representado por inteiros (`SPEC §33`), nunca ponto flutuante;
* toda operação aritmética monetária verificada;
* testes de propriedade e fuzzing sobre a função de transição.

### THR-TX-004 — Maleabilidade de transações

**Severidade:** ALTA
**Descrição:** um terceiro altera a representação de uma transação sem invalidar a assinatura, alterando seu identificador.

**Mitigações:**

* codificação canônica única (`SPEC §8`);
* identificador derivado do conteúdo canônico (`SPEC §7`);
* rejeição de codificações não canônicas e de assinaturas não canônicas.

### THR-TX-005 — Não determinismo entre implementações

**Severidade:** CRÍTICA
**Descrição:** duas implementações divergem para o mesmo bloco (ordem de iteração de mapas, ponto flutuante, dependência de relógio local, diferenças de bibliotecas).

**Mitigações:**

* estruturas ordenadas deterministicamente;
* proibição de ponto flutuante e de fontes de aleatoriedade locais na transição de estado;
* vetores de teste públicos e suíte de conformidade (`SPEC §69`);
* testes `AT-DET-*`.

### THR-TX-006 — Manipulação de taxas

**Severidade:** MÉDIA
**Descrição:** taxas negativas, ausentes, desviadas ou usadas para criar ZERO.

**Mitigações:** taxa explícita e verificável com destino determinístico (`SPEC §36`); testes `AT-FEE-*`.

---

## 9.3 Blocos e consenso

### THR-CON-001 — Reescrita de histórico

**Severidade:** CRÍTICA
**Adversários:** ADV-04
**Mitigações:**

* encadeamento criptográfico (`SPEC §16`);
* finalidade explícita e imediata: todo bloco aceito carrega certificado de > 2/3 do poder de voto (ADR-0012, `spec/CONSENSUS.md §4.3`), e não há reorganização;
* pontos de verificação do operador contra ataque de longo alcance com chaves desvinculadas (`--checkpoint`, subjetividade fraca; sem checkpoints embutidos no protocolo, REQ-005). A distribuição social desses pontos pelas Comunidades fica **A DEFINIR**;
* testes `AT-BLOCK-003`, `AT-BLOCK-004`.

### THR-CON-002 — Captura do consenso

**Severidade:** CRÍTICA
**Adversários:** ADV-03, ADV-04, ADV-07
**Descrição:** um participante ou coalizão passa a controlar a produção ou finalização de blocos.

**Mitigações:**

* algoritmo de consenso com limite de tolerância a falhas explicitamente documentado: Zero-BFT tolera menos de 1/3 do poder de voto bizantino (ADR-0012);
* participação no consenso não baseada apenas em número de identidades (anti-Sybil): o poder de voto é o ZERO vinculado (`Bond`), aberto a qualquer conta que atinja `min_bond`;
* monitoramento de concentração (`REQ-045`);
* mudanças no conjunto de participantes do consenso apenas por regras protocolares.

**Nota sobre a DEVNET:** até a versão 0.3.0 a DEVNET usou um conjunto fixo de validadores (ADR-0006). Ele foi substituído pelo conjunto aberto e recomputado por época do Zero-BFT (ADR-0012). O Genesis ainda define o poder inicial, e a concentração inicial deve ser avaliada antes de qualquer rede com valor.

### THR-CON-003 — Censura de transações

**Severidade:** ALTA
**Adversários:** ADV-04, ADV-12
**Mitigações:**

* rotação de produtores de bloco: o proponente muda a cada rodada, por sorteio ponderado e determinístico;
* múltiplos caminhos de propagação;
* métricas públicas de inclusão;
* mecanismos anti-censura adicionais (**A DEFINIR**).

### THR-CON-004 — Equivocação (produção de blocos conflitantes)

**Severidade:** ALTA
**Adversários:** ADV-02, ADV-04
**Descrição:** um produtor assina dois blocos diferentes para a mesma posição.

**Mitigações:**

* dois blocos assinados pela mesma identidade para a mesma altura/rodada constituem evidência objetiva e verificável;
* penalização protocolar baseada nessa evidência (`SPEC §49`): `ReportEquivocation` (propostas) e `ReportDoubleVote` (votos) queimam `slash_bps` do vínculo e das desvinculações pendentes, excluem o validador do conjunto imediatamente e de forma permanente e zeram seus pontos de contribuição (`spec/CONSENSUS.md §6`);
* o período de desvinculação mantém o ZERO punível depois da saída;
* Nodes detectam e denunciam automaticamente as evidências observadas.

### THR-CON-005 — Bloco malformado ou excessivo

**Severidade:** MÉDIA
**Mitigações:** limites de tamanho e de quantidade de transações (`MAX_BLOCK_TX_BYTES` = 2 MiB, para que todo bloco válido caiba num quadro P2P); validação completa antes de propagação; rejeição (`AT-BLOCK-002`, `oversized_block_rejected`).

### THR-CON-006 — Manipulação temporal

**Severidade:** MÉDIA
**Descrição:** produtores manipulam timestamps para afetar regras dependentes de tempo.

**Mitigações:** limites de desvio aceitável; regras dependentes de altura sempre que possível em vez de relógio (SUP-05). No Zero-BFT o bloco não tem timestamp nem slot: todo prazo protocolar é medido em blocos, e o relógio local só controla temporizadores de rodada, que não afetam a validade.

### THR-CON-007 — Partição de rede

**Severidade:** ALTA
**Adversários:** ADV-06, ADV-12
**Mitigações:** preferência por segurança sobre disponibilidade: sem > 2/3 do poder conectado nenhum lado da partição finaliza blocos, e a rede retoma ao se reconectar (retransmissão de propostas e votos); não há forks a resolver (ADR-0012, testes de partição em `rz-chain`).

---

## 9.4 P2P e sincronização

### THR-P2P-001 — Ataque eclipse

**Severidade:** ALTA
**Adversários:** ADV-02, ADV-03, ADV-06
**Descrição:** o atacante controla todas as conexões de um Node, oferecendo uma visão falsa da rede.

**Mitigações:**

* diversidade de pares (limites por faixa de endereço e por origem);
* múltiplas fontes de descoberta;
* conexões de saída escolhidas pelo próprio Node;
* validação completa do estado recebido;
* detecção de estagnação (ausência de novos blocos válidos);
* estratégia final **A DEFINIR** após testes (`SPEC §29`).

### THR-P2P-002 — Negação de serviço por flooding

**Severidade:** ALTA
**Adversários:** ADV-02, ADV-06
**Mitigações:**

* tamanho máximo de mensagem verificado antes de alocar memória;
* limites de taxa por par;
* pontuação de comportamento local com desconexão e quarentena (`SPEC §26`);
* validação barata antes de validação cara (formato → assinatura → estado);
* mensagens de consenso: só de validadores, rodadas limitadas, repasse só após aceitação, fila limitada (`docs/AUDIT.md` RZ-IR-02..04);
* testes `AT-P2P-004`, `AT-P2P-005`, limites de memória em `rz-chain bft_tests`.

* balde de consenso por conexão proporcional ao número de validadores; excedente descartado sem banir pares honestos (RZ-IR-06).

### THR-P2P-003 — Mensagens malformadas

**Severidade:** MÉDIA
**Descrição:** mensagens construídas para explorar o parser (pânico, consumo excessivo, corrupção de memória).

**Mitigações:** linguagem com segurança de memória (`unsafe` proibido); parser sem pânico sobre entrada não confiável; testes de mutação sobre todos os tipos de mensagem, com verificação de canonicidade e de ausência de pânico na validação de estado (`rz-p2p/tests/robustness.rs`); fuzzing contínuo com libFuzzer (**A DEFINIR**); teste `AT-P2P-003`.

### THR-P2P-004 — Sincronização envenenada

**Severidade:** CRÍTICA
**Adversários:** ADV-02
**Descrição:** um par entrega blocos ou estado adulterado a um Node novo.

**Mitigações:** o Node reexecuta ou verifica criptograficamente tudo o que recebe; Genesis conhecido e fixo; nenhum estado aceito apenas por declaração (`SPEC §28`, `AT-SYNC-002`, `AT-SYNC-003`).

### THR-P2P-005 — Man-in-the-middle

**Severidade:** MÉDIA
**Mitigações:** conteúdo protocolar autenticado por assinaturas; canal cifrado (X25519 + ChaCha20-Poly1305) com o Node respondedor autenticado pela sua identidade e fixação opcional da identidade esperada (ADR-0011); a integridade do estado não depende do canal.

**Risco residual:** sem fixação de identidade (`--node-id`) ou serviço onion, um intermediário ativo pode se passar pelo Node na primeira conexão.

### THR-P2P-006 — Incompatibilidade de versão explorável

**Severidade:** BAIXA
**Mitigações:** handshake com versão, rede e capacidades; rejeição explícita de versões incompatíveis (`SPEC §24`, `AT-P2P-002`).

---

## 9.5 Privacidade

### THR-PRIV-001 — Análise do grafo de transações

**Severidade:** ALTA
**Adversários:** ADV-05, ADV-02, qualquer observador da cadeia
**Descrição:** transações públicas permitem vincular remetente, destinatário, valor e histórico.

**Mitigações:** RingCT sobre Ristretto255 (ADR-0009, `spec/PRIVACY.md`): endereços furtivos ocultam o destinatário, compromissos de Pedersen com Bulletproofs ocultam valores, e assinaturas em anel CLSAG (anel de 11) com imagens de chave ocultam o remetente e a ligação entre transações. Wallet privada por padrão; saída de troco sempre presente; minimização de campos públicos (`SPEC §31`).

**Risco residual:** anonimato probabilístico — análise estatística de anéis, correlação temporal entre blindagem, gasto e retirada, e valores característicos em operações públicas (`spec/PRIVACY.md §11`). Transações de conta transparentes continuam existindo e são públicas por natureza.

### THR-PRIV-002 — Correlação de endereço IP

**Severidade:** ALTA
**Adversários:** ADV-05, ADV-02
**Descrição:** o primeiro Node a propagar uma transação revela a provável origem; o Node ao qual a Wallet se conecta e observadores de rede veem o IP do usuário.

**Mitigações:**

* Dandelion++ (ADR-0010): a origem aparente é aleatória ao longo da haste;
* canal cifrado com preenchimento (ADR-0011): observadores de rede não leem nem classificam mensagens pelo tamanho;
* Tor/I2P via SOCKS5 e serviços onion (ADR-0011): o Node não vê o IP do usuário;
* Node privado (`--no-listen`): a Wallet usa um Node próprio em `127.0.0.1`, que nunca é anunciado e também encaminha transações alheias;
* a Wallet recusa, por padrão, conexão direta a Node remoto sem proxy (REQ-023).

**Risco residual:** quem escolhe `--direct` expõe o IP ao Node escolhido; um Node privado expõe seu IP aos pares que disca, como qualquer participante P2P; observadores globais ou adversários com muitos Nodes da haste ainda podem correlacionar.

### THR-PRIV-003 — Metadados excessivos

**Severidade:** MÉDIA
**Descrição:** campos de versão de cliente, fuso horário, idioma, timestamps precisos ou identificadores persistentes facilitam fingerprinting.

**Mitigações:** handshake mínimo; revisão de campos (`SPEC §32`); testes `AT-PRIV-002`.

### THR-PRIV-004 — Registros locais

**Severidade:** MÉDIA
**Descrição:** logs de Nodes armazenam endereços IP, relações ou atividade de usuários indefinidamente.

**Mitigações:** endereços de clientes nunca são registrados, em nenhum nível de log; endereços de pares Node só em modo detalhado; nenhuma coleta por conveniência (ADR-0011).

### THR-PRIV-005 — Correlação por comportamento

**Severidade:** MÉDIA (risco residual)
**Descrição:** padrões temporais, reutilização de endereços e comportamento do usuário permitem correlação mesmo com proteções técnicas.

**Mitigações:** orientação ao usuário; boas práticas nas Wallets (novos endereços, atrasos aleatórios **A DEFINIR**). Risco reconhecido pelo `REQ-028`.

---

## 9.6 Governança

### THR-GOV-001 — Captura por Sybil

**Severidade:** ALTA
**Adversários:** ADV-03
**Mitigações:** o voto não pode assumir `1 Node = 1 pessoa` (`SPEC §47`); mecanismo anti-Sybil **A DEFINIR** (`REQ-044`).

### THR-GOV-002 — Captura por concentração econômica

**Severidade:** ALTA
**Adversários:** ADV-07
**Mitigações:** limitação de concentração (`REQ-045`); fórmula de voto que equilibre participação e capital (**A DEFINIR**); quórum e limiares definidos previamente.

### THR-GOV-003 — Spam de propostas

**Severidade:** BAIXA
**Mitigações:** depósito simbólico em ZERO (`REQ-042`, `SPEC §44`); testes `AT-GOV-003`.

### THR-GOV-004 — Adulteração de votos

**Severidade:** ALTA
**Mitigações:** votos assinados, vinculados a uma proposta e período específicos, registrados no estado (`SPEC §45`); testes `AT-GOV-004`, `AT-GOV-005`.

### THR-GOV-005 — Mudança de regras sem processo

**Severidade:** CRÍTICA
**Adversários:** ADV-09, ADV-12, mantenedores comprometidos
**Descrição:** uma alteração protocolar é introduzida via release de software sem passar pela governança.

**Mitigações:** ciclo proposta → análise → testes → votação → implantação (`REQ-092`); ativação por versão e altura; Nodes escolhem executar ou não uma versão; releases reprodutíveis (`REQ-004`).

### THR-GOV-006 — Compra de votos e coerção de votantes

**Severidade:** MÉDIA
**Mitigações:** parcialmente mitigável; voto secreto ou mecanismos resistentes a coerção são **A DEFINIR** e possuem trade-off com auditabilidade.

---

## 9.7 Grande Mercado e Pool

### THR-MKT-001 — Front-running e manipulação de ordem

**Severidade:** ALTA
**Adversários:** ADV-04, ADV-07
**Descrição:** produtores de bloco ou observadores reordenam, inserem ou atrasam ordens para lucro próprio.

**Mitigações:** leilão de preço uniforme por bloco (ADR-0014): todas as ordens que cruzam num bloco executam ao mesmo preço, e o resultado depende só do conjunto de ordens, não da sua ordem no bloco (`block_order_does_not_change_market_outcome`); prioridade determinística e verificável. **Residual:** o proponente pode omitir uma ordem por um bloco (rotação limita); ordens são públicas antes da inclusão, e ordens cifradas até a inclusão ficam **A DEFINIR**.

### THR-MKT-002 — Cancelamento não autorizado

**Severidade:** ALTA
**Mitigações:** somente o autor da ordem pode cancelá-la, comprovado por assinatura (`AT-MKT-003`, `AT-MKT-004`, `at_mkt_003_004_cancel_only_by_owner`).

### THR-MKT-003 — DEX paralela dentro da Rede Zero

**Severidade:** MÉDIA
**Descrição:** uma Comunidade implementa uma segunda exchange, contrariando `REQ-032`/`REQ-033`.

**Mitigações:** Comunidades não possuem capacidade de custodiar ou liquidar ZERO fora das operações protocolares; a regra é propriedade técnica, não poder administrativo.

### THR-POOL-001 — Retirada administrativa do Pool

**Severidade:** CRÍTICA
**Adversários:** ADV-04, ADV-10, ADV-12, mantenedores
**Mitigações:** inexistência de **qualquer** operação que reduza o Pool, administrativa ou não (`SPEC §39–§40`, ADR-0014). O conjunto de tags de transação é fechado e testado (`at_pool_002_no_withdrawal_operation_exists`). O Pool monotônico é verificado a cada bloco em testes de propriedade (`at_pool_003_pool_never_decreases`, `randomized_market_invariants`). O Pool não é contraparte de trocas.

### THR-POOL-002 — Ponte externa comprometida

**Severidade:** CRÍTICA
**Adversários:** ADV-07, ADV-09
**Descrição:** um ativo externo é representado no Pool sem existir ou estar bloqueado na rede de origem.

**Mitigações:** verificação criptográfica da origem; proibição de dependência exclusiva de custodiante (`SPEC §42`); mecanismo **A DEFINIR**. Enquanto não existir, nenhum ativo externo real é aceito: só ativos de teste no Genesis da DEVNET, rejeitados nos demais ambientes (ADR-0014 §4).

### THR-POOL-003 — Confusão entre ZERO e ativo externo

**Severidade:** ALTA
**Mitigações:** identificadores de ativo derivados da origem (`H(network ‖ asset_ref)`), com ZERO no identificador nulo; nomes "ZERO" e origem "rede-zero" rejeitados; operações de ativo externo recusam ZERO (`SPEC §41`, `AT-POOL-004`, `at_pool_004_external_asset_never_zero`).

---

## 9.8 Comunidades e Navegador Zero

### THR-COM-001 — Comunidade maliciosa

**Severidade:** ALTA
**Adversários:** ADV-08
**Mitigações:** sandbox (`SPEC §62`); menor privilégio; nenhuma autoridade sobre consenso, saldos, regras monetárias ou outras Comunidades (`SPEC §61`); assinatura de operações sempre confirmada pela Wallet, fora do controle da Comunidade.

### THR-COM-002 — Atualização maliciosa de Comunidade

**Severidade:** ALTA
**Descrição:** uma Comunidade legítima publica uma versão hostil.

**Mitigações:** versões identificadas por hash; verificação de origem e versão (`SPEC §60`); possibilidade de o usuário fixar versões.

### THR-COM-003 — Censura arbitrária de Comunidades

**Severidade:** MÉDIA
**Adversários:** ADV-12, maiorias de governança
**Mitigações:** aprovação apenas por regras objetivas (`REQ-073`); conteúdo controverso não é violação técnica (`REQ-074`); aplicações privadas não dependem de aprovação.

### THR-BRW-001 — Interface maliciosa ou falsificada

**Severidade:** ALTA
**Descrição:** uma interface alternativa ou falsa induz o usuário a assinar operações diferentes das exibidas.

**Mitigações:** a Wallet exibe o conteúdo canônico que será assinado; nenhuma interface possui autoridade especial (`INV-008`); verificação de releases.

### THR-BRW-002 — Phishing de endereços da Exonet

**Severidade:** MÉDIA
**Descrição:** identificadores visualmente parecidos com `zero://comunidade` legítimos.

**Mitigações:** identificadores derivados de material criptográfico; nomes legíveis como camada adicional (**A DEFINIR**); alertas de similaridade na interface.

---

## 9.9 Reputação

### THR-REP-001 — Manipulação de reputação

**Severidade:** MÉDIA
**Adversários:** ADV-03, ADV-02
**Mitigações:** reputação derivada apenas de eventos verificáveis (`SPEC §48`); reputação não é autorização eterna (`REQ-048`).

### THR-REP-002 — Penalização injusta

**Severidade:** MÉDIA
**Descrição:** acusações sem evidência, pesquisa tratada como ataque, penalização por opinião.

**Mitigações:** penalização exige causa, regra, evidência verificável e efeito definido (`SPEC §49`); pesquisa não é ataque (`REQ-050`).

### THR-REP-003 — Marcação física de máquinas

**Severidade:** ALTA
**Descrição:** implementações tentam instalar marcas persistentes no sistema do usuário como punição.

**Mitigações:** proibido pelo Manifesto; reputação acompanha a identidade criptográfica, nunca o hardware.

---

## 9.10 Defesa da Exonet

### THR-DEF-001 — Perpetuação do estado de emergência

**Severidade:** CRÍTICA
**Adversários:** ADV-10
**Mitigações:** duração máxima, revisões obrigatórias e expiração automática de modos e credenciais (`REQ-058`, `REQ-059`, `INV-010`).

### THR-DEF-002 — Abuso de credenciais defensivas

**Severidade:** ALTA
**Adversários:** ADV-10
**Mitigações:** escopo, duração, autoridade verificável, revogação e registro (`SPEC §54`); ações de alto impacto com confirmação coletiva.

### THR-DEF-003 — Falso incidente

**Severidade:** ALTA
**Adversários:** ADV-10, ADV-11
**Descrição:** um incidente é declarado sem base para obter privilégios.

**Mitigações:** transições entre modos exigem evidência verificável e múltiplos participantes (`SPEC §52`, `SPEC §53`); o Agente Zero não declara incidentes sozinho.

### THR-DEF-004 — Uso ofensivo da infraestrutura defensiva

**Severidade:** CRÍTICA
**Descrição:** ferramentas de defesa são usadas contra sistemas externos.

**Mitigações:** o protocolo não possui operações ofensivas (`SPEC §55`); a defesa opera apenas dentro da Rede Zero e sistemas voluntariamente participantes.

### THR-DEF-005 — Recompensa por mera presença

**Severidade:** BAIXA
**Mitigações:** distinção entre participação e contribuição verificável (`SPEC §57`).

---

## 9.11 Agente Zero

### THR-AGT-001 — Envenenamento da base de conhecimento

**Severidade:** ALTA
**Adversários:** ADV-11
**Mitigações:** versões da base identificadas por hash; origem dos documentos rastreável; análises reproduzíveis.

### THR-AGT-002 — Injeção de instruções

**Severidade:** ALTA
**Adversários:** ADV-11, ADV-08
**Descrição:** conteúdo de Comunidades, propostas ou mensagens tenta instruir o Agente Zero a agir de forma diferente.

**Mitigações:** o Agente Zero não possui autoridade de execução sobre consenso, fundos ou credenciais (LT-7); dados externos tratados como dados, nunca como instruções; ferramentas com privilégio mínimo.

### THR-AGT-003 — Recomendação tratada como decisão

**Severidade:** ALTA
**Descrição:** participantes passam a aceitar automaticamente recomendações do agente, transformando-o em autoridade de fato.

**Mitigações:** o consenso é determinístico e não depende de IA (`Manifesto §6`); propostas passam pelo ciclo de governança; recomendações registradas com a versão do agente.

---

## 9.12 Desenvolvimento, cadeia de suprimentos e continuidade

### THR-DEV-001 — Dependência maliciosa

**Severidade:** CRÍTICA
**Adversários:** ADV-09
**Mitigações:** dependências mínimas e justificadas (`REQ-084`); versões fixadas por lockfile; auditoria de dependências automatizada; revisão de novas dependências.

### THR-DEV-002 — Comprometimento de conta de mantenedor

**Severidade:** CRÍTICA
**Adversários:** ADV-09, ADV-12
**Mitigações:** revisão de código obrigatória (`REQ-077`); proteção de branches; assinatura de commits e releases (**A DEFINIR**); nenhum mantenedor com poder de publicar sozinho alterações protocolares.

### THR-DEV-003 — Release não reprodutível

**Severidade:** ALTA
**Mitigações:** builds reprodutíveis (`REQ-004`); binários verificáveis contra o código-fonte.

### THR-DEV-004 — Dependência de plataforma única

**Severidade:** MÉDIA
**Adversários:** ADV-12
**Descrição:** remoção do repositório, bloqueio de conta ou indisponibilidade da plataforma de hospedagem.

**Mitigações:** independência do GitHub (`REQ-090`); espelhos; o repositório Git completo é autossuficiente; documentação versionada junto ao código (`REQ-091`).

### THR-DEV-005 — Coerção de pessoas do projeto

**Severidade:** ALTA
**Adversários:** ADV-12
**Mitigações:** nenhuma pessoa indispensável (`REQ-096`); ausência de chaves-mestras; forks legítimos (`REQ-080`); continuidade sem fundador (`REQ-099`).

### THR-DEV-006 — Vulnerabilidade divulgada sem coordenação

**Severidade:** ALTA
**Mitigações:** política de divulgação responsável (`REQ-082`) publicada em `SECURITY.md`.

### THR-DEV-007 — Segredos publicados no repositório

**Severidade:** ALTA
**Descrição:** como o repositório é público, chaves, tokens ou dados pessoais publicados por engano ficam expostos permanentemente no histórico.

**Mitigações:** nenhum segredo no repositório; chaves da DEVNET geradas localmente e nunca versionadas, exceto chaves explicitamente marcadas como **apenas para desenvolvimento, sem valor**; varredura de segredos na CI.

---

# 10. Matriz de rastreabilidade (resumo)

| Ameaça | Requisitos | Especificações | Testes |
| --- | --- | --- | --- |
| THR-ID-001 | REQ-011, REQ-012 | SPEC-ID-001, SPEC-WAL-002/003 | AT-WAL-*, AT-PRIV-003 |
| THR-ID-002 | REQ-009 | SPEC §9 | AT-ID-004..006 |
| THR-ID-003 | REQ-007 | SPEC §66 | AT-GEN-002 |
| THR-ID-004 | REQ-018 | SPEC §10 | AT-TX-007 |
| THR-TX-001 | REQ-014..018 | SPEC §11, §21 | AT-DS-* |
| THR-TX-002 | REQ-021, REQ-022 | SPEC §34, §35 | AT-MONEY-*, AT-ZERO-003 |
| THR-TX-004 | REQ-016 | SPEC §7, §8 | AT-CAN-*, AT-TID-* |
| THR-TX-005 | REQ-004, REQ-016 | SPEC §68 | AT-DET-* |
| THR-CON-001 | REQ-016 | SPEC §16 | AT-BLOCK-003/004 |
| THR-CON-002 | REQ-005, REQ-015 | SPEC §19, §20, spec/CONSENSUS.md | AT-CON-*, rz-chain bft_tests |
| THR-CON-007 | REQ-015 | SPEC §22, spec/CONSENSUS.md §1 | rz-chain partition_halts_then_recovers, randomized_adversarial_* |
| THR-CON-001 | REQ-016 | spec/CONSENSUS.md §9 | rz-chain checkpoint_rejects_alternative_history |
| THR-P2P-001 | REQ-017 | SPEC §29 | — (a definir) |
| THR-P2P-002 | REQ-053 | SPEC §26 | AT-P2P-004/005, rz-chain bft_tests (limites) |
| THR-P2P-003 | REQ-053 | SPEC §8, spec/ENCODING.md | AT-P2P-003, rz-p2p robustness |
| THR-P2P-004 | REQ-016 | SPEC §28 | AT-SYNC-* |
| THR-PRIV-001 | REQ-024 | SPEC §31, spec/PRIVACY.md | rz-core private::tests, rz-wallet private_devnet |
| THR-PRIV-002 | REQ-026 | SPEC §30, spec/P2P.md §4 | rz-node dandelion_stem_then_embargo_fluff |
| THR-GOV-001 | REQ-044 | SPEC §47, spec/GOVERNANCE.md | rz-core governance_tests (neutralidade a Sybil), rz-wallet governance_devnet |
| THR-P2P-005 | — | SPEC §30, spec/P2P.md §1.1 | rz-p2p secure::tests |
| THR-CON-004 | REQ-049 | SPEC §49, spec/CONSENSUS.md §6 | rz-core equivocation_report_slashes_contribution, double_vote_slashes_bond_and_jails, double_proposal_slashes_even_for_reproposal |
| THR-POOL-001 | REQ-037, REQ-038 | SPEC §39, §40, spec/MARKET.md §5 | AT-POOL-002, AT-POOL-003, randomized_market_invariants |
| THR-POOL-003 | REQ-039 | SPEC §41, spec/MARKET.md §1 | AT-POOL-004 |
| THR-MKT-001 | REQ-035 | spec/MARKET.md §4 | block_order_does_not_change_market_outcome |
| THR-MKT-002 | REQ-035 | spec/MARKET.md §2 | AT-MKT-003, AT-MKT-004 |
| THR-DEF-001 | REQ-058, REQ-059 | SPEC §53, §54 | — (a definir) |
| THR-DEV-001 | REQ-084 | — | CI |
| THR-DEV-004 | REQ-090 | — | — |

---

# 11. Priorização para a DEVNET

A DEVNET é um ambiente de desenvolvimento sem valor econômico. As ameaças abaixo devem ser tratadas **desde a primeira versão de código**, porque são estruturais e difíceis de corrigir depois:

1. THR-TX-004 — codificação canônica e maleabilidade;
2. THR-TX-005 — determinismo;
3. THR-ID-002 / THR-ID-003 — assinaturas com separação de domínio e identificador de rede;
4. THR-ID-004 — proteção contra replay;
5. THR-TX-001 / THR-TX-002 / THR-TX-003 — double spend, emissão e aritmética;
6. THR-CON-001 / THR-CON-004 — encadeamento e evidência de equivocação;
7. THR-P2P-002 / THR-P2P-003 / THR-P2P-004 — limites de mensagem, parser robusto, sincronização verificada;
8. THR-DEV-001 / THR-DEV-007 — dependências mínimas e ausência de segredos.

Ameaças aceitas **temporariamente** na DEVNET, com registro explícito:

| Ameaça | Aceitação temporária | Condição para sair da DEVNET |
| --- | --- | --- |
| THR-CON-001/002 | Zero-BFT (ADR-0012) implementado **sem auditoria** nem prova formal (verificado por simulação adversarial); checkpoints só por configuração do operador; poder inicial definido no Genesis | Auditoria independente; distribuição social de checkpoints; distribuição inicial avaliada |
| THR-PRIV-001 | RingCT implementado **sem auditoria**; anel de 11 | Auditoria independente da implementação (ADR-0009) |
| THR-PRIV-002 | Dandelion++, canal cifrado e Tor opcional **não auditados** | Auditoria do canal; avaliação de tráfego de cobertura (ADR-0011) |
| THR-P2P-005 | Canal cifrado sem fixação obrigatória de identidade | Distribuição verificável de identidades de Nodes ou uso de serviços onion |
| THR-POOL-002 | Sem ponte: apenas ativos **de teste** no Genesis da DEVNET | Ponte verificável sem custodiante único (nova ADR) |
| THR-MKT-001 | Ordens públicas antes da inclusão; omissão por um bloco possível | Avaliação de ordens cifradas até a inclusão |
| THR-GOV-001/002 | Governança implementada (ADR-0008); a câmara de contribuição só pontua produção de blocos, então favorece validadores | Computação e contribuição defensiva verificáveis |

Nenhuma dessas aceitações temporárias pode ser herdada pela TESTNET pública ou pela MAINNET sem nova avaliação.

---

# 12. Fora do escopo

Os seguintes cenários não são mitigados pelo protocolo:

* **comprometimento total do dispositivo do usuário** no momento da assinatura (ADV-13 com controle completo);
* **revelação voluntária** de identidade pelo próprio usuário;
* **ataques físicos** contra pessoas (coerção direta pode forçar a entrega de chaves);
* **quebra das primitivas criptográficas** (incluindo computação quântica suficientemente poderosa — tratada como risco futuro, sujeita a evolução do protocolo);
* **controle de maioria do consenso** além do limite tolerado pelo algoritmo escolhido;
* **ataques à infraestrutura física da Internet** em escala global;
* **segurança de sistemas externos** integrados por pontes, além da verificação exigida pelo protocolo;
* **legalidade** de atividades específicas em jurisdições específicas.

Estar fora do escopo não significa ser ignorado: significa que o protocolo não promete impedir esses cenários, e que eles devem ser comunicados honestamente aos usuários.

---

# 13. Riscos residuais

Mesmo com todas as mitigações:

* um usuário com dispositivo comprometido pode perder fundos;
* correlação temporal e comportamental pode reduzir a privacidade;
* sem > 2/3 do poder conectado a rede para, e transações ficam pendentes até a retomada;
* uma coalizão acima do limite de tolerância pode comprometer o consenso;
* governança pode tomar decisões ruins dentro das regras;
* o Agente Zero pode produzir recomendações erradas;
* vulnerabilidades desconhecidas existirão.

A resposta da Rede Zero a esses riscos não é prometer que não acontecerão, mas garantir: detecção, evidência, recuperação, evolução do protocolo e ausência de ponto único de falha.

---

# 14. Manutenção deste documento

Este modelo deverá ser revisado quando:

* um componente novo for especificado;
* uma decisão marcada como **A DEFINIR** for tomada;
* um incidente real ou simulado revelar ameaça não prevista;
* uma dependência crítica for adicionada;
* o ambiente mudar de fase (DEVNET → TESTNET → MAINNET).

Toda alteração deverá possuir histórico, justificativa e versão (`REQ-086`, `REQ-087`).

---

# 15. Status

**THREAT_MODEL.md v0.5.0**

> **Modelo de ameaças inicial — não congelado.**

# 16. Próxima etapa

Com este documento, a cadeia documental prevista em `SPECIFICATIONS.md §75` fica completa:

```text
MANIFESTO → REQUIREMENTS → THREAT_MODEL → ARCHITECTURE → SPECIFICATIONS → IMPLEMENTATION
```

A próxima etapa é iniciar as especificações especializadas (`spec/CRYPTOGRAPHY.md` em diante) e o protótipo da DEVNET, tratando primeiro as ameaças priorizadas na seção 11.
