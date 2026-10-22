# ADR-0026 — Distribuição inicial da MAINNET por depósito permanente no Pool

**Estado:** Aceita (implementação antes da MAINNET)
**Escopo:** MAINNET
**Data:** 2026-10-22
**Relacionamento:** REQ-020..022, REQ-099, `SPECIFICATIONS.md §34`, §37, §38, Manifesto §5, §12, §13, THR-CON-001, ADR-0014 (Pool), ADR-0019 (ponte), ADR-0025 (política monetária)

## Contexto

A ADR-0025 fixou a emissão e decidiu que o Genesis não tem alocação a fundadores. Faltava decidir quem recebe os 100 000 000 ZERO da MAINNET.

As restrições dos documentos são as mesmas da ADR-0025:

* ninguém escolhe quem recebe;
* só regras explícitas e verificáveis criam ZERO;
* o Pool não paga ninguém;
* a rede não pede identidade;
* a concentração econômica é um risco declarado.

## Decisão

1. **Vínculo de arranque.** O Genesis da MAINNET contém só o vínculo dos validadores iniciais.
   * Esse vínculo não pode ser transferido nem desvinculado.
   * É **queimado** ao fim do período de distribuição.
   * Os validadores iniciais operam o consenso, mas não recebem ZERO por isso.
2. **Período de distribuição.** Dura `D` blocos, da ordem de 30 dias.
   * Qualquer pessoa queima BTC na origem com o script de queima da ponte (`spec/BRIDGE.md §4`), destinando a representação a um **depósito de distribuição** no Pool permanente.
   * Esse depósito é irreversível, como todo depósito no Pool.
3. **Divisão proporcional.** No fim do período, `100 000 000 ZERO` são criados e divididos **na proporção do BTC depositado** por endereço.
   * O arredondamento é para baixo, e o resto é queimado.
   * Um único ativo de referência dispensa oráculo de preço.
4. **Emissão depois.** O `block_reward` da ADR-0025 só começa depois do período.
5. **Sem exceções.** Pessoas fundadoras, validadores iniciais e quem opera o faucet da TESTNET recebem ZERO da MAINNET só pelo mesmo mecanismo, como qualquer pessoa.

## Alternativas consideradas

| Alternativa | Por que não |
| --- | --- |
| Alocação a fundadores ou equipe | Manifesto §13, REQ-099, THR-CON-001 |
| Só emissão, sem Genesis | Concentra a oferta nos validadores iniciais (Manifesto §13) |
| Distribuição a participantes da TESTNET | O ZERO da TESTNET é gratuito, então contas falsas fraudam a distribuição |
| Distribuição por identidade, uma pessoa uma parte | Exige identificar pessoas, o que a rede não faz (Manifesto §7, REQ-023) |
| Venda por uma entidade | Custódia e um ponto central de confiança (`SPEC §42`, Manifesto §12) |
| Vários ativos de referência | Exige oráculo de preço, uma autoridade sobre a regra de divisão |

## Consequências

* O Pool nasce com liquidez permanente em BTC, que dá base ao Grande Mercado.
* O custo real da queima resiste a contas falsas: dividir o depósito entre muitas contas não dá vantagem.
* **Concentração por riqueza:** quem queima mais recebe mais. Sem identidade, não há como limitar por pessoa; a câmara de contribuição da governança compensa parte disso.
* **Depende da ponte auditada:** a ponte é proibida na MAINNET até a auditoria (ADR-0019), e essa auditoria vira pré-requisito da MAINNET.
* O BTC queimado é destruído na origem.

## A implementar antes da MAINNET

* vínculo de arranque com queima ao fim do período;
* depósito de distribuição e divisão proporcional, com aritmética exata e testes de conservação;
* atraso do início da emissão;
* vetores de conformidade e um teste com Nodes reais do período inteiro;
* ensaio completo numa TESTNET dedicada antes do lançamento.

## Condições de revisão

* Resultado da auditoria da ponte.
* Ensaio do período de distribuição na TESTNET.
