# ADR-0025 — Política monetária do ZERO

**Estado:** Aceita (opção C, decidida em 2026-10-22)
**Escopo:** TESTNET → MAINNET
**Data:** 2026-10-22
**Relacionamento:** REQ-019..022, REQ-099, `REQUIREMENTS.md §23`, `ARCHITECTURE.md §15`, `SPECIFICATIONS.md §33–§38`, Manifesto §5, §12, §13, §15, THR-CON-001, AS-05, ADR-0005, ADR-0012, ADR-0014
**Especificação:** `spec/STATE.md §5–§7`, `spec/BLOCKS.md §5`, `spec/GOVERNANCE.md` (tags 38 e 39), `spec/CONSENSUS.md §9`
**Implementação:** `crates/rz-core/src/monetary.rs`, `crates/rz-core/src/state.rs` (`credit_fees`), `crates/rz-core/src/genesis.rs`, `crates/rz-node` (`init --block-reward --fee-burn-bps`)

## Contexto

A DEVNET não emite ZERO depois do Genesis: os validadores recebem só as taxas (`spec/STATE.md §5`, ADR-0012). `ARCHITECTURE.md §15` pede que a política definitiva fixe emissão inicial e futura, limite, recompensas, taxas, distribuição, unidade mínima e regras de criação e destruição.

Já está definido:

* **Unidade:** `1 ZERO = 10⁸` unidades mínimas, em `u64` (`spec/STATE.md §1`).
* **Taxas:** vão para o produtor do bloco (`spec/STATE.md §6`).
* **Destruição:** a punição queima vínculo (`spec/CONSENSUS.md §6.2`), e o depósito de proposta sem quórum é queimado (`spec/GOVERNANCE.md`).

### Restrições dos documentos

Uma opção que contradiga qualquer item abaixo fica descartada:

| Origem | Restrição |
| --- | --- |
| Manifesto §5, REQ-020 | Uma única moeda nativa, com regras públicas e previsíveis |
| `SPEC §34`, REQ-021, AT-ZERO-003 | Só mecanismos explícitos do protocolo criam ZERO; nenhum `saldo += X` fora de uma transição válida |
| `SPEC §37`, `ARCHITECTURE §15` | Nenhum administrador cria recompensas, e nenhum participante cria ZERO arbitrariamente |
| `SPEC §38` | Nada pode permitir que alguém escolha arbitrariamente quem recebe recursos |
| Manifesto §12 | O Pool não paga Nodes nem contribuidores, e não é caixa da governança; as recompensas vêm das regras econômicas próprias do protocolo |
| Manifesto §15 | Recompensa acompanha contribuição útil e verificável, não potência de máquina |
| Manifesto §13, THR-CON-001 | Concentração econômica é um risco declarado; a distribuição inicial define o poder inicial do consenso |
| REQ-022 | Qualquer pessoa verifica a criação e a destruição de ZERO |
| `spec/BLOCKS.md` | Não há relógio protocolar: o tempo é medido em blocos |

## Opções descartadas por contradizerem os documentos

| Opção | Contradiz |
| --- | --- |
| Pagar validadores ou contribuidores com o Pool | Manifesto §12 |
| Tesouro em que a governança escolhe quem recebe | `SPEC §38`, `SPEC §37` |
| Proposta que cunha ZERO para um endereço | `SPEC §34`, `SPEC §37` |
| Recompensa por capacidade de hardware declarada | Manifesto §15 |

## Opções compatíveis

### A — Oferta fixa no Genesis

Esta é a regra atual. Todo ZERO nasce no Genesis, e os validadores vivem só de taxas.

* **A favor:** é a regra mais simples, e a oferta nunca cresce.
* **Contra:**
  * A segurança depende de haver volume de taxas desde o início.
  * Toda a distribuição se decide no Genesis, o que agrava THR-CON-001.

### B — Teto com emissão decrescente

A recompensa por bloco cai pela metade a cada `H` blocos, e a oferta converge para um teto.

* **A favor:** o teto é conhecido e fácil de explicar.
* **Contra:**
  * A recompensa tende a zero, e depois sobram só as taxas, o mesmo problema de A, adiado.
  * Os primeiros validadores acumulam a maior parte da emissão.

### C — Emissão constante por bloco, sem teto (adotada)

O protocolo cria `block_reward` unidades a cada bloco finalizado.

* **A favor:**
  * Há um orçamento de segurança permanente.
  * A inflação percentual cai sozinha com o tempo: 1 % no primeiro ano, cerca de 0,5 % no centésimo.
  * A regra é a mais previsível das três, porque o valor por bloco é sempre o mesmo.
* **Contra:** não há teto absoluto. A oferta cresce sem limite, mas cada vez mais devagar em termos relativos.

## Decisão

1. **Emissão:** C, com `block_reward` fixo por bloco.
   * O valor vem do Genesis (`monetary.block_reward`) e muda só por proposta constitucional (tag 38), com teto de 100 ZERO. Um Genesis com 0 mantém a regra anterior.
   * O valor vale por altura, coerente com a ausência de relógio protocolar.
2. **Destino da recompensa:** o **proponente** do bloco finalizado, no mesmo ponto em que recebe as taxas.
   * É o mesmo evento que já lhe dá `+1` ponto de contribuição (`spec/REPUTATION.md §2`). O evento é verificável pelo próprio bloco e pelo certificado.
   * A escolha do proponente é ponderada pelo vínculo, então a recompensa esperada é proporcional ao ZERO vinculado.
   * Um validador fora do ar não propõe e não recebe. Isso é uma punição branda por inatividade, que hoje não existe.
3. **Taxas:** `fee_burn_bps` das taxas de cada bloco são queimadas, e o resto vai para o produtor (tag 39, constitucional). Valor adotado: **5 000** (metade).
   * Com pouco uso, a emissão paga os validadores e mantém a rede segura enquanto as taxas ainda são pequenas.
   * Com muito uso, a queima compensa a emissão: a oferta líquida se estabiliza ou cai, e o valor do ZERO fica ligado ao uso real.
   * O proponente fica com metade, então continua incentivado a incluir transações. Encher o próprio bloco com transações próprias passa a custar metade das taxas.
4. **Distribuição inicial:** **sem alocação a fundadores ou à equipe.**
   * Na TESTNET, o Genesis tem o vínculo dos validadores e o faucet.
   * Na MAINNET, a distribuição é por depósito permanente no Pool (ADR-0026).
   * Uma alocação a fundadores não seria contradição literal, mas iria contra o Manifesto §13 (concentração), REQ-099 (continuidade sem fundador) e THR-CON-001.
5. **Conservação:** a invariante passa a ser verificada após cada bloco.

   ```text
   total_supply(h) = total_supply(h − 1) + block_reward − queimado(h)
   ```

   A emissão é uma regra explícita do protocolo, então AT-ZERO-003 continua valendo: nenhuma transação cria ZERO.
6. **Valores adotados para a TESTNET** (`MonetaryParams::TESTNET`, padrão de `rede-zero-node init`):
   * `G = 100 000 000 ZERO` no Genesis;
   * `block_reward = 0,1 ZERO` (`10 000 000` unidades);
   * com `block_interval_ms = 2 000` e cerca de 10 milhões de blocos por ano, isso dá cerca de 1 % de `G` ao ano;
   * não há risco de estouro de `u64` em escala de séculos.

### Recompensa à contribuição verificável (Manifesto §15)

Fica **fora** desta decisão. Hoje, os únicos eventos verificáveis além da produção de blocos são as contribuições defensivas, atestadas por mais de 2/3 dos validadores.

Pagá-las com emissão daria aos validadores o poder de cunhar ZERO para quem atestarem, o que fica perto demais do que `SPEC §38` proíbe. Isso deve ser revisto quando houver computação verificável por todos (ADR-0020, condições de revisão).

## Registro da decisão

Decidido em 2026-10-22: opção C; Genesis sem alocação a fundadores, de 100 000 000 ZERO; `block_reward = 0,1 ZERO`; queima de 5 000 bps das taxas, escolhida para manter a economia viva com pouco e com muito uso.

## Consequências

* `total_supply` cresce `block_reward` por bloco e diminui pela queima. `check_supply` continua verificando a igualdade após cada bloco, e os vetores de conformidade cobrem a emissão e a queima.
* **Riscos:**
  * Uma maioria qualificada de validadores poderia reduzir `block_interval_ms` localmente e acelerar a emissão no tempo de relógio. A emissão por altura continua previsível.
  * Mudar `block_reward` exige proposta constitucional.

## Condições de revisão

* Ajuste de `fee_burn_bps` com dados de uso da TESTNET.
* Recompensa à computação verificável (Manifesto §15).
* Recompensa aos signatários do certificado em vez de só ao proponente. Isso exige levar o certificado do bloco anterior no cabeçalho.
* Dados de participação e de taxas da TESTNET.
