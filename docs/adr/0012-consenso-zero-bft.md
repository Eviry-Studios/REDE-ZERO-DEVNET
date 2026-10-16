# ADR-0012 — Consenso Zero-BFT: BFT com participação por ZERO bloqueado

**Estado:** Aceita (DEVNET) — substitui a ADR-0006; sujeita a auditoria antes da TESTNET
**Escopo:** DEVNET → candidata a TESTNET
**Data:** 2026-09-29
**Relacionamento:** REQ-005, REQ-015..018, REQ-045, REQ-049, `ARCHITECTURE.md §12`, `SPECIFICATIONS.md §19–§22`, AC-CON-001..004, THR-CON-001..007
**Especificação:** [`spec/CONSENSUS.md`](../../spec/CONSENSUS.md) v0.2.0
**Implementação:** `crates/rz-core/src/consensus.rs` (tipos), `crates/rz-core/src/state.rs` (staking), `crates/rz-chain/src/bft.rs` (máquina de estados), `crates/rz-chain/src/chain.rs` (cadeia finalizada)

## Contexto

A ADR-0006 adotou autoridade rotativa com validadores fixos apenas para destravar a DEVNET e exigiu sua substituição antes de qualquer rede pública, por um algoritmo analisado contra `SPEC §20`. `ARCHITECTURE.md §12` define o que o consenso deve tratar e os critérios de decisão.

## Decisão

**Zero-BFT**: consenso bizantino tolerante a falhas no estilo Tendermint, em que o poder de voto de cada validador é o ZERO que ele mantém bloqueado.

1. **Conjunto aberto:** qualquer conta pode bloquear ZERO (`Bond`, com mínimo `min_bond`) e se candidatar. A cada `epoch_blocks` o conjunto ativo é recalculado a partir do estado, com os maiores bloqueios até `max_validators`.
2. **Rodadas:** proposta → pré-voto → pré-compromisso. O proponente de cada rodada é sorteado com probabilidade proporcional ao poder, a partir do bloco anterior. É determinístico e imprevisível antes de o bloco anterior existir.
3. **Finalidade:** um bloco com pré-compromissos de mais de 2/3 do poder é **final**. O certificado ([`Commit`]) acompanha o bloco e é verificável por qualquer Node (AC-CON-001).
4. **Travamento:** quem pré-compromete um bloco fica travado nele e só o abandona diante de prova de que mais de 2/3 pré-votaram outro em rodada posterior. Isso impede que dois blocos sejam finalizados na mesma altura.
5. **Desvinculação:** `Unbond` só devolve o ZERO após `unbonding_blocks` (~21 dias). Durante esse período o valor continua punível, o que impede fugir da punição saindo do conjunto.
6. **Punição objetiva** (REQ-049, SPEC §49): dois votos assinados pelo mesmo validador, na mesma altura, rodada e tipo, para blocos diferentes (`ReportDoubleVote`), ou duas propostas distintas para a mesma rodada (`ReportEquivocation`), fazem o seguinte:
   * queimam `slash_bps` (5%) do bloqueio e das desvinculações pendentes;
   * excluem o validador do conjunto imediatamente;
   * zeram seus pontos de contribuição.

   Cada infração só é punida uma vez.
7. **Sem relógio:** blocos não têm timestamp. O ritmo vem de temporizadores locais crescentes por rodada, e a validade não depende de relógio (THR-CON-006).

## Análise contra `SPEC §20` e `ARCHITECTURE.md §12`

| Ameaça | Tratamento |
| --- | --- |
| Gasto duplo | Finalidade imediata e determinística; blocos finais nunca são revertidos |
| Forks | Com menos de 1/3 do poder bizantino, é impossível finalizar dois blocos na mesma altura. Não há escolha de fork |
| Reorganizações | Inexistentes após o commit |
| Sybil | O poder é proporcional ao ZERO bloqueado: criar identidades não aumenta o poder total (mesmo argumento de neutralidade da ADR-0008) |
| Nodes maliciosos | Tolera até 1/3 do poder bizantino. Duplas assinaturas geram evidência e punição |
| Censura | O proponente muda a cada rodada e altura, sorteado por poder. Um proponente que censura não impede que outro inclua a transação. Censura persistente exige mais de 1/3 do poder (limitação registrada) |
| Partições | A cadeia **para** em vez de se dividir. Retoma sozinha após a reconexão (THR-CON-007: segurança antes de disponibilidade) |
| Participantes offline | Progride enquanto mais de 2/3 do poder estiver online. A rodada muda por temporizador quando o proponente está ausente |
| Concentração (REQ-045) | Limite de tolerância explícito (1/3). Conjunto aberto e recalculado por época. Governança bicameral separada do consenso. O ZERO bloqueado é público, permitindo medir a concentração |
| Disponibilidade | Latência de um bloco por altura (três trocas de mensagem) |

Critérios de decisão (`ARCHITECTURE §12`):

| Critério | Zero-BFT |
| --- | --- |
| Segurança | Prova formal publicada para o Tendermint (segurança com ≥ 2/3 honestos) |
| Descentralização | Conjunto aberto por bloqueio de ZERO; limitado a `max_validators` por comunicação O(n²) |
| Disponibilidade | Alta com mais de 2/3 online; para em partição (escolha consciente) |
| Desempenho | Finalidade em segundos |
| Custo | Sem gasto energético de mineração |
| Resistência a ataques | Punição econômica verificável; ataques de longo alcance mitigados pela desvinculação |
| Implementação | Máquina de estados sem E/S (`rz-chain/src/bft.rs`), ~500 linhas |
| Auditoria | Algoritmo publicado e estudado; testes com rede simulada, validadores bizantinos e partições |

## Alternativas consideradas

| Alternativa | Motivo da rejeição |
| --- | --- |
| Manter a autoridade rotativa (ADR-0006) | Viola REQ-005 |
| Prova de trabalho | Finalidade probabilística (reorganizações e gasto duplo sob maioria de hash); custo energético; concentração em hardware especializado; Manifesto §15: potência de GPU não prova trabalho útil |
| Prova de participação com cadeia mais longa (tipo Ouroboros) | Finalidade probabilística; mais sensível a relógio (THR-CON-006) |
| Consenso por reputação | Reputação é produzida pelo próprio consenso (circular) e pode ser acumulada por Sybil ao longo do tempo |
| Prova de pessoa | Exige identidade civil ou autoridade emissora (REQ-010, REQ-027) |

## Consequências

* O Genesis define validadores iniciais, com ZERO já bloqueado, e os parâmetros de consenso. Depois disso, a entrada e a saída de validadores são abertas.
* O ZERO bloqueado e o conjunto de validadores são **públicos**, por natureza, como os bloqueios de governança.
* Não há emissão nova: validadores recebem apenas taxas. A política monetária continua **A DEFINIR** (`REQUIREMENTS.md §23`).

## Limitações honestas

* **Até 1/3 do poder** pode travar a rede (sem violar a segurança). **Mais de 1/3** pode censurar indefinidamente. **Mais de 2/3** pode finalizar blocos inválidos para quem não verifica o estado, mas Nodes que reexecutam os blocos os rejeitam (AC-CON-002).
* Ausência prolongada (inatividade) não é punida nesta versão.
* Validadores excluídos por punição não podem voltar com a mesma chave (sem reabilitação).
* Implementação **não auditada**.

## Condições de revisão

Auditoria; simulação com latência e perda de mensagens reais; punição por inatividade; política monetária; avaliação de agregação de assinaturas para conjuntos maiores.
