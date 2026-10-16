# ADR-0020 — Reputação como pontos de contribuição verificáveis

**Estado:** Aceita (DEVNET)
**Escopo:** DEVNET → TESTNET
**Data:** 2026-10-13
**Relacionamento:** REQ-046..050, `SPECIFICATIONS.md §48–§50`, `§57`, ADR-0008 (câmara de contribuição), ADR-0016 (registro de contribuição), THR-REP-001..003, THR-DEF-005, THR-GOV-001/002
**Especificação:** [`spec/REPUTATION.md`](../../spec/REPUTATION.md)
**Implementação:** `crates/rz-core/src/reputation.rs`, `crates/rz-core/src/state.rs`, `crates/rz-node`, `crates/rz-wallet`

## Contexto

`SPEC §48` pede reputação derivada de eventos verificáveis: participação válida, contribuições verificadas, violações comprovadas e participação defensiva verificada. A fórmula estava A DEFINIR.

Já existiam duas peças:

* os pontos de contribuição da governança (ADR-0008), que só contavam blocos produzidos;
* o registro de contribuição defensiva (ADR-0016), cujos pontos ficaram A DEFINIR.

O resultado era uma câmara de contribuição que só pontuava validadores (THR-GOV-001/002).

## Decisão

1. **Uma só reputação:** os pontos de contribuição. Eles ficam ligados à chave pública, decaem com meia-vida e são a mesma medida usada na câmara de contribuição e na exigência de histórico para credenciais defensivas. Não há segundo número que possa divergir.
2. **Eventos que pontuam:**
   * bloco produzido (1 ponto);
   * contribuição defensiva atestada por mais de 2/3 após o encerramento do incidente: 50 pontos, valor constitucional, no máximo 1 000.

   Uma contribuição defensiva verificada vale tanto quanto muitas horas de produção de blocos e é o caminho para quem não é validador ganhar peso.
3. **Eventos que tiram:** só punições com evidência assinada (equivocação, voto duplo, proposta dupla), que zeram os pontos. Nada subjetivo.
4. **Registro no estado** dos eventos de defesa e de punição, com consulta P2P, para que qualquer pessoa verifique a origem de cada mudança.
5. **Nada além disso por enquanto.** Hospedar conteúdo, retransmitir cabeçalhos ou executar computação não pontuam, porque o protocolo ainda não consegue verificá-los sem confiar em quem declara.

## Alternativas consideradas

| Alternativa | Por que não |
| --- | --- |
| Reputação separada dos pontos de governança | Duas medidas que podem divergir e ser exploradas uma contra a outra |
| Pontuar presença, tempo conectado ou saldo | Recompensa mera presença (THR-DEF-005) ou capital (que já tem a câmara econômica) |
| Avaliação entre pares, votos de confiança | Subjetiva e manipulável por Sybil (THR-REP-001, `SPEC §49`) |
| Pontuar hospedagem de conteúdo ou retransmissão de cabeçalhos | Não verificável sem confiar na declaração de quem hospeda ou retransmite; pode ser fabricado |
| Penalizar por pontuação local do P2P | Opinião local de um Node, não evidência verificável por todos |

## Consequências

* Nodes que não são validadores passam a ter peso na câmara de contribuição por contribuição defensiva verificada. O viés para validadores diminui, mas não desaparece.
* A defesa ganha um incentivo não monetário, sem pagar por mera presença.
* Novas fontes de pontos exigem prova verificável e uma nova decisão.

## Condições de revisão

* Computação verificada, como tarefas com resultado checável por todos.
* Incentivo verificável à retransmissão de cabeçalhos da ponte e à disponibilidade de conteúdo.
* Revisão do peso relativo após dados de TESTNET.
