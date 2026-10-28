# Plano de implementação — ADR-0026 (distribuição inicial da MAINNET)

**Estado:** proposta de plano (RZ7, 2026-10-28) · **Base:** `main` `22001c3` · **ADR:** `docs/adr/0026-distribuicao-da-mainnet.md`

Este documento não muda nenhuma regra: só confere o que a ADR-0026 pede contra o código de hoje, quebra o trabalho em
incrementos verificáveis e lista as perguntas que precisam de decisão antes do código.

## 1. O que já existe

| Peça da ADR | Onde está hoje | Situação |
| --- | --- | --- |
| Genesis com validadores e vínculos | `rz-core/src/genesis.rs` (`Genesis.validators`, `allocations`), `state.rs` (`bonds`) | existe; o vínculo do Genesis é um vínculo comum (pode desvincular) |
| Distinguir MAINNET | `NetworkKind::Mainnet`, `network_id` conferido no `validate` | existe |
| Queima na origem com destino no `OP_RETURN` | `TxKind::BridgeDeposit` (`0x71`), `spec/BRIDGE.md §4` | existe; cunha a representação para o destinatário |
| Ponte proibida na MAINNET sem auditoria | `bridge_tests.rs` ("sem auditoria, nunca na MAINNET") | existe — continua sendo pré-requisito |
| Pool permanente, depósito irreversível | `TxKind::PoolDeposit` (`0x31`) | existe |
| Emissão por bloco + queima de taxas | `monetary.rs` (`block_reward`, `fee_burn_bps`), `state.rs` ~1299 | existe; emite desde o bloco 1 |

## 2. O que falta (lacunas)

1. **Vínculo de arranque.** Não há como marcar um vínculo do Genesis como intransferível e não desvinculável, nem como
   queimá-lo numa altura fixa. Hoje `Unbond` (`0x21`) vale para qualquer vínculo.
2. **Período de distribuição no Genesis.** Não há parâmetro de duração `D` nem de ativo de referência (BTC); `validate` da
   MAINNET não exige `allocations` vazio.
3. **Depósito de distribuição.** `BridgeDeposit` cunha a representação para o destinatário; não existe a variante que manda a
   representação direto ao Pool **e** registra, por endereço, quanto BTC (sats) foi depositado para a divisão.
4. **Divisão proporcional.** Não existe a liquidação na altura `D`: criar `100 000 000 ZERO`, dividir por
   `floor(total × dᵢ / Σd)` em aritmética exata (`u128`), queimar o resto, em ordem determinística.
5. **Atraso da emissão.** `block_reward` é creditado desde o primeiro bloco; a ADR pede que comece só depois de `D`.
6. **Conformidade e ensaio.** Faltam vetores de conformidade das novas transações/regras, teste com Nodes reais do período
   inteiro e o ensaio numa TESTNET dedicada.

## 3. Incrementos propostos (um PR cada, nesta ordem)

| # | Incremento | Aceite (testes) |
| --- | --- | --- |
| D1 | `Genesis.distribution: Option<DistributionParams { duration_blocks, reference_chain }>` + versão do protocolo; `validate` da MAINNET exige `distribution` presente, `allocations` vazio e ponte da `reference_chain` configurada | codec ida-e-volta; MAINNET sem distribuição/ com alocação é recusada; DEVNET/TESTNET inalteradas |
| D2 | Vínculo de arranque: os vínculos dos `validators` do Genesis ficam marcados; `Unbond`/transferência recusados para eles; na altura `D` são removidos (queimados) | não desvincula durante `D`; some exatamente em `D`; o supply não muda (o vínculo de arranque não é ZERO em circulação) |
| D3 | `TxKind::DistributionDeposit` (tag nova): mesma prova do `BridgeDeposit`, só aceita na `reference_chain` e com altura `< D`; a representação vai ao Pool permanente e `distribution_deposits[endereço] += sats` | prova inválida/ fora do período/ outra cadeia recusadas; depósito não é retirável; soma por endereço confere |
| D4 | Liquidação em `D`: cria `100 000 000 ZERO`, divide proporcionalmente com `u128`, arredonda para baixo, queima o resto | **conservação**: Σ creditado + resto = 100 M; propriedade (proptest) com muitos depositantes; um depositante só recebe tudo; ordem dos depósitos não muda o resultado |
| D5 | `block_reward` só a partir de `D + 1` (taxas do período: ver Q2) | nenhuma emissão antes; emissão normal depois |
| D6 | Vetores de conformidade (`rz-conformance`) das regras D1–D5 | suíte regenerada e conferida por um segundo cliente/ferramenta |
| D7 | Teste com Nodes reais do período inteiro (D curto) + ensaio numa TESTNET dedicada | relatório do ensaio em `docs/` |

O lançamento da MAINNET continua bloqueado pela **auditoria da ponte** (ADR-0019), como a ADR-0026 já diz.

## 4. Perguntas para decidir antes do D2–D5

* **Q1 — continuidade do consenso:** se ao fim de `D` o vínculo de arranque é queimado e ninguém se vinculou com ZERO recém
  distribuído, a rede fica sem validadores. Proposta: a queima acontece em `D`, mas os validadores de arranque só saem do
  conjunto ativo quando os vínculos comuns somarem o quórum mínimo (com prazo máximo); ou a liquidação acontece algumas
  épocas antes da queima, para dar tempo de vincular.
* **Q2 — taxas durante o período:** sem emissão e sem ZERO em circulação, quem paga taxa no período (os depósitos de
  distribuição precisam de taxa?) e para onde ela vai (100 % queimada, já que os validadores de arranque não recebem)?
* **Q3 — valor de `D`:** "da ordem de 30 dias" em blocos depende do tempo de bloco da MAINNET; fixar no Genesis.
* **Q4 — nenhum depósito:** se `Σd = 0` em `D`, o lançamento é abortado (rede parada) ou o período se estende?
* **Q5 — destinatário:** o endereço que recebe a parte é o gravado no `OP_RETURN` (como hoje) — confirmar que não há outro.

## 5. Fora do escopo deste plano

Política monetária depois do período (ADR-0025), Grande Mercado e câmara de contribuição da governança.
