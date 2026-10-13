# ADR-0019 — Ponte sem custodiante: prova de queima por cliente leve e trocas atômicas

**Estado:** Aceita (DEVNET), auditoria pendente
**Escopo:** DEVNET → TESTNET (proibida na MAINNET até auditoria)
**Data:** 2026-10-13
**Relacionamento:** REQ-036..039, `SPECIFICATIONS.md §38–§42`, `ARCHITECTURE.md §30`, AC-POOL-004/005, AT-POOL-004, THR-POOL-002, ADR-0014 (§4: ponte A DEFINIR), Manifesto §11, §12
**Especificação:** [`spec/BRIDGE.md`](../../spec/BRIDGE.md)
**Implementação:** `crates/rz-core/src/bridge.rs`, `crates/rz-core/src/state.rs`, `crates/rz-node`, `crates/rz-wallet`

## Contexto

A ADR-0014 deixou a ponte em aberto, porque os documentos exigem três coisas:

* um mecanismo verificável de que o ativo existe ou foi bloqueado na origem, sem depender só da declaração de um custodiante (`SPEC §42`);
* que uma ponte centralizada não seja considerada automaticamente compatível com a descentralização (`ARCHITECTURE §30`);
* que ativos externos continuem identificáveis e nunca virem moeda nativa (REQ-039, AT-POOL-004).

Além disso, o Pool não tem saída (`SPEC §40`).

`ARCHITECTURE §30` manda estudar pontes, clientes leves, provas criptográficas, trocas atômicas e mecanismos sem custodiante.

Qualquer ponte "de ida e volta" precisa que alguém guarde o ativo na origem para liberá-lo depois. Esse alguém pode ser uma pessoa, um multisig ou uma federação, e em todos os casos é um custodiante.

## Decisão

1. **Entrada por prova de queima, verificada por cliente leve na própria cadeia.**
   * A Rede Zero valida os cabeçalhos da rede de origem com as regras de prova de trabalho dela: dificuldade exigida, reajuste do Bitcoin, mediana de tempo e maior trabalho acumulado.
   * Ela aceita uma prova de Merkle de uma transação que destrói o valor numa saída `OP_RETURN` comprometida com esta rede Zero e com o destinatário.
   * A representação é cunhada só depois das confirmações configuradas.
   * Ninguém guarda nada, então não há a quem pedir resgate, nem quem possa fugir com os fundos.
2. **Mão única, por escolha.** O ativo representado é permanente na Rede Zero, coerente com a permanência do Pool. Quem quer voltar à origem troca com outra pessoa.
3. **Trocas atômicas (HTLC)** com SHA-256, pré-imagem de 32 bytes e prazo em altura. São compatíveis com os contratos equivalentes do Bitcoin e de redes afins. Duas pessoas trocam ZERO ou ativos por ativos de outra rede sem intermediário: ou as duas pernas acontecem, ou nenhuma.
4. **Redes de origem só pelo Genesis**, com ponto de partida e número de confirmações explícitos. Nesta versão a ponte fica proibida na MAINNET até haver auditoria. A inclusão posterior pela governança fica A DEFINIR.
5. **Sem relógio:** as regras de cabeçalho que dependem de hora local são omitidas. O custo da prova de trabalho substitui essa defesa.

## Alternativas consideradas

| Alternativa | Por que não |
| --- | --- |
| Custodiante, multisig ou federação que guarda e libera | É custódia (`SPEC §42`, `ARCHITECTURE §30`, Manifesto §12) |
| Oráculo que declara depósitos | Volta a depender da declaração de alguém |
| Ponte de ida e volta com garantia em ZERO (estilo tBTC) | Continua precisando de alguém para assinar a liberação na origem; colateral e liquidações são uma superfície econômica grande |
| Cliente leve de cadeias de prova de participação | Exige verificar assinaturas e conjuntos de validadores de cada rede; fica para ADRs específicas |
| Aceitar o cabeçalho mais recente informado | Sem prova de trabalho acumulada, qualquer um falsificaria depósitos |

## Consequências

* Ativos de outras redes entram de forma verificável por qualquer Node, sem confiança em pessoas. A segurança é a da prova de trabalho da origem, com as confirmações exigidas.
* Um atacante com maioria do poder de mineração da origem, ou capaz de produzir uma cadeia mais pesada que a real a partir do ponto de partida, poderia cunhar representações falsas. Mais confirmações reduzem esse risco.
* Retransmissores precisam enviar cabeçalhos; não há incentivo nesta versão (A DEFINIR).
* Fork da origem mais profundo que 4 032 blocos não é acompanhado.
* Trocas atômicas exigem atenção das pessoas: o prazo do lado de quem recebe o segredo deve ser menor, com folga, que o do outro lado.

## Condições de revisão

* Auditoria do cliente leve e das regras de HTLC antes de qualquer uso fora de DEVNET e TESTNET.
* Inclusão de redes de origem e atualização de pontos de partida pela governança.
* Incentivo a retransmissores de cabeçalhos.
* Clientes leves de outras famílias de consenso.
