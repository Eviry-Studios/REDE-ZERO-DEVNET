# ADR-0014 — Grande Mercado por leilão de preço uniforme e Pool permanente sem saída

**Estado:** Aceita (DEVNET)
**Escopo:** DEVNET → candidata a TESTNET (exceto ativos externos, ver §4)
**Data:** 2026-09-29
**Relacionamento:** REQ-032..039, `SPECIFICATIONS.md §38–§42`, `ARCHITECTURE.md §27–§30`, AC-MKT-001..004, AC-POOL-001..005, AT-MKT-001..004, AT-POOL-001..004, THR-MKT-001..003, THR-POOL-001..003, Manifesto §5, §11, §12
**Especificação:** [`spec/MARKET.md`](../../spec/MARKET.md) v0.1.0
**Implementação:** `crates/rz-core/src/market.rs` (tipos e leilão), `crates/rz-core/src/state.rs` (aplicação), `crates/rz-wallet` (comandos)

## Contexto

`SPEC §71` deixou em aberto "o algoritmo do Grande Mercado" e "a matemática do Pool". Os documentos fixam as restrições:

* o Grande Mercado é **um único** mecanismo oficial, parte do protocolo e acessível por várias interfaces, sem autoridade central (REQ-032..035, Manifesto §11);
* existe **um único** Pool permanente; ativos depositados **permanecem** nele; não pode haver retirada administrativa (REQ-036..038). A `SPEC §40` vai além: **qualquer** mecanismo de movimentação para fora do Pool é incompatível com a permanência, salvo alteração explícita da especificação pela governança;
* o Pool não é caixa da governança nem fonte de pagamento de operadores (Manifesto §12);
* ativos externos exigem identificação de origem, não podem ser confundidos com ZERO e sua entrada precisa ser verificável, sem depender de um custodiante (REQ-039, `SPEC §41–§42`);
* front-running é ameaça ALTA; a mitigação sugerida são leilões em lote ou ordens cifradas (THR-MKT-001);
* uma única unidade monetária nativa, ZERO (Manifesto §5).

## Decisão

### 1. Pool permanente: reserva sem nenhuma operação de saída

O Pool é uma parte do estado (`pool: ativo → quantidade`). Só a operação `PoolDeposit` e a tarifa do mercado (§3) o alteram, e **só para cima**. Não existe operação de retirada, administrativa ou não (AT-POOL-002). Isso vale para ZERO e para ativos externos.

Consequência deliberada: o Pool **não** é contraparte de trocas. Um formador de mercado automático (AMM) clássico tira do pool o ativo comprado em cada troca. Isso é "movimentação para fora do Pool", proibida pela `SPEC §40`. A liquidez do Grande Mercado vem das ordens dos participantes, e o Pool é capital permanente, publicamente verificável.

### 2. Grande Mercado: livro de ordens com leilão de preço uniforme por bloco

* **Pares:** todo par é `ativo externo / ZERO`. Uma única unidade de conta evita livros duplicados (A/B e B/A) e segue o Manifesto §5.
* **Ordens-limite** (`PlaceOrder`) com quantidade, preço-limite e validade em blocos. O valor é **reservado** na colocação (ZERO numa compra, ativo numa venda). `CancelOrder` só é aceito do autor (AT-MKT-003/004). Ordens vencidas saem com reembolso.
* **Leilão:** no fim de cada bloco, para cada ativo cujo livro mudou, calcula-se **um único preço**: o que maximiza o volume executado, com desempate pelo menor desequilíbrio e depois pela mediana. Todas as ordens que cruzam executam a esse preço, por prioridade de preço, depois antiguidade, depois identificador.
* **Por que leilão em lote:** num livro contínuo, a ordem das transações no bloco decide quem executa e a que preço, e o proponente controla essa ordem. No leilão, o resultado depende só do **conjunto** de ordens (testado em `block_order_does_not_change_market_outcome`), então reordenar não dá vantagem (THR-MKT-001). O proponente ainda pode **omitir** uma ordem, mas só por um bloco, porque o proponente é rotativo.

### 3. Tarifa do mercado

Parâmetro `market_fee_bps` (constitucional, limite de 10%), cobrado do ZERO recebido pelo vendedor e **depositado no Pool**. O valor inicial é **0**. A decisão econômica de ativá-la fica com a governança; o protocolo só garante que, se existir, ela vai para o Pool e não para operadores (Manifesto §12).

### 4. Ativos externos

* Identificador `AssetId = H("rede-zero/asset/v1", network ‖ asset_ref)`. ZERO usa o identificador nulo, e nenhum ativo externo pode tê-lo. Não se registra ativo externo chamado ZERO nem com origem na Rede Zero (AT-POOL-004).
* **Não há ponte nesta versão.** Uma ponte verificável, com prova da rede de origem e sem custodiante único (`SPEC §42`, THR-POOL-002), é pesquisa de alto risco (`ARCHITECTURE §30`) e fica **A DEFINIR** numa ADR própria.
* Para testar o mercado, a DEVNET aceita **ativos de teste** declarados no Genesis (`Verification::DevnetGenesis`). Eles não representam nada na rede de origem e são **rejeitados** em Genesis de TESTNET, STAGING e MAINNET.

### 5. Privacidade

Ordens, depósitos no Pool e saldos de ativos externos são **públicos** nesta versão: contas transparentes, como bloqueios de governança e vínculos. Negociação privada, por exemplo com ordens cifradas até a inclusão, é pesquisa futura e também mitigaria THR-MKT-001 por completo.

## Alternativas consideradas

| Alternativa | Por que não |
| --- | --- |
| AMM com o Pool como contraparte | Move ativos para fora do Pool a cada troca: viola `SPEC §40` |
| AMM com pools separados, retiráveis | Seriam vários pools com retirada: violaria REQ-036 (Pool único) e criaria um segundo mecanismo de liquidez |
| Livro contínuo (execução por ordem de chegada) | O proponente decide a ordem das transações e pode fazer front-running dentro do bloco (THR-MKT-001) |
| Ordens cifradas (commit-reveal) | Mais forte contra front-running, mas duas fases, custo e complexidade; ficam como evolução |
| Tokens emitidos por usuários | Criaria uma segunda infraestrutura monetária nativa (Manifesto §5) |
| Ponte por custodiante/multisig | Ponto central de confiança (`SPEC §42`, Manifesto §12) |

## Consequências

* O Pool só cresce, por depósito voluntário e irreversível ou pela tarifa, se aprovada. Quem deposita aceita a permanência (Manifesto §12); a Wallet exige `--permanente`.
* O livro tem limites globais e por conta (`max_open_orders`, `max_orders_per_account`), e ordens de valor nulo são rejeitadas. Isso limita o estado e o custo do leilão por bloco.
* Invariantes novas: conservação de cada ativo externo e Pool monotônico, além do ZERO reservado em ordens na invariante monetária (`spec/STATE.md`).
* Sem ponte, o mercado da DEVNET só negocia ativos de teste. O mecanismo (ordens, leilão, reservas, Pool) é o mesmo que uma ponte futura usará.

## Condições de revisão

* Proposta de ponte verificável (nova ADR), obrigatória antes de qualquer ativo externo fora da DEVNET.
* Análise econômica da tarifa e do uso do Pool, se a governança quiser atribuir função adicional ao capital permanente, sempre sem saída (`SPEC §40`).
* Ordens cifradas ou negociação privada.
