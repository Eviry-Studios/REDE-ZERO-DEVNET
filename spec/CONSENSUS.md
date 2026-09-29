# spec/CONSENSUS.md — Consenso Zero-BFT

**Versão:** 0.2.0
**Escopo:** DEVNET; candidato à TESTNET pública após auditoria (`docs/AUDIT.md`)
**Decisão:** ADR-0012 (substitui ADR-0006)
**Relacionamento:** `SPECIFICATIONS.md §19–§22`, `§49`, REQ-005, REQ-015, THR-CON-001..007
**Implementação de referência:** `crates/rz-core/src/consensus.rs` (tipos e verificação), `crates/rz-chain/src/bft.rs` (máquina de estados), `crates/rz-chain/src/chain.rs` (cadeia finalizada), `crates/rz-node` (integração)

---

## 1. Visão geral

Zero-BFT é um consenso bizantino tolerante a falhas, com **finalidade imediata**, sobre um **conjunto aberto de validadores** escolhido pelo ZERO vinculado (`Bond`). O algoritmo de acordo é o de Tendermint (Buchman, Kwon, Milosevic, 2018): proposta, pré-voto e pré-compromisso, com trava (*lock*) e rodadas.

Premissas:

* **segurança**: nenhum par de blocos conflitantes é finalizado na mesma altura enquanto menos de 1/3 do poder de voto for bizantino, sob qualquer atraso de rede;
* **vivacidade**: blocos são finalizados após o período de sincronia parcial (GST) enquanto mais de 2/3 do poder estiver honesto e conectado;
* com 1/3 ou mais do poder offline, a rede **para** em vez de divergir (preferência explícita por segurança, `SPEC §21`).

## 2. Parâmetros (`ConsensusParams`)

Declarados no Genesis e alteráveis por governança (categoria constitucional, `spec/GOVERNANCE.md`), vivem no estado em `ProtocolParams.consensus`.

| Campo | Tipo | Significado | DEVNET (`devnet`) |
| --- | --- | --- | --- |
| `block_interval_ms` | u64 | espera após uma decisão antes da próxima altura | 2 000 |
| `epoch_blocks` | u64 | blocos por época (recomputa o conjunto) | 100 |
| `max_validators` | u32 | tamanho máximo do conjunto | 100 |
| `min_bond` | u64 | vínculo mínimo de um validador (unidades) | 1 000 ZERO |
| `unbonding_blocks` | u64 | retenção de uma desvinculação | ≈ 21 dias |
| `slash_bps` | u32 | fração queimada na punição (pontos-base) | 500 (5 %) |
| `timeout_propose_ms` | u64 | espera pela proposta na rodada 0 | 1 500 |
| `timeout_prevote_ms` | u64 | espera após > 2/3 de pré-votos quaisquer | 500 |
| `timeout_precommit_ms` | u64 | espera após > 2/3 de pré-compromissos quaisquer | 500 |
| `timeout_delta_ms` | u64 | acréscimo por rodada em cada temporizador | 500 |

`validate()` exige `epoch_blocks` e `unbonding_blocks` maiores que zero, `block_interval_ms` e os três temporizadores base em `1..=600 000` ms (`MAX_TIME_PARAM_MS`), `timeout_delta_ms ≤ 600 000`, `1 ≤ max_validators ≤ 1 000` e `slash_bps ≤ 10 000`. O limite superior dos tempos impede que um parâmetro aprovado por governança paralise os Nodes (`docs/AUDIT.md` RZ-IR-01).

## 3. Conjunto de validadores

### 3.1 Poder de voto

O poder de voto de um validador é o seu ZERO vinculado. O conjunto `ValidatorSet` é ordenado pela chave pública, sem duplicatas; `total` é a soma do poder.

```text
maioria qualificada(p)  ⇔  3·p > 2·total
mais de um terço(p)     ⇔  3·p > total
```

### 3.2 Seleção por época

O Genesis cria os vínculos iniciais (`validators[i].stake ≥ min_bond`, no máximo `max_validators`). Ao fim do bloco de altura `h` com `h mod epoch_blocks = 0`, o conjunto **do bloco seguinte** é recomputado:

```text
candidatos = { (k, b) ∈ bonds : b ≥ min_bond ∧ k ∉ jailed }
ordem      = vínculo decrescente, depois chave crescente
conjunto   = primeiros max_validators
```

Se o resultado for vazio, o conjunto anterior é mantido. Vínculos e desvinculações entram no conjunto apenas na época seguinte; a **punição** é imediata (§6).

O estado guarda `validators` (conjunto que validará o próximo bloco) e `last_validators` (o que validou o último bloco). Ambos entram na raiz de governança do estado.

### 3.3 Proponente

```text
x         = u128_be(H("rede-zero/proposer/v1", prev ‖ u64(h) ‖ u32(r))[0..16])
alvo      = x mod total
proponente = primeiro validador, na ordem do conjunto, em que alvo < poder acumulado
```

`prev` é o Block ID do bloco anterior (o hash do Genesis na altura 1). A escolha é determinística, ponderada pelo poder, e muda a cada rodada, então um proponente ausente só atrasa uma rodada.

## 4. Mensagens

Todas as assinaturas são Ed25519 com separação de domínio e vínculo à rede (`spec/CRYPTOGRAPHY.md`): `string(ctx) ‖ string(network_id) ‖ payload`.

### 4.1 Proposta

```text
Proposal {
  height    : u64
  round     : u32
  pol_round : Option<u32>   // rodada em que o bloco obteve > 2/3 de pré-votos (re-proposta)
  block     : Block
  proposer  : PublicKey
  signature : Signature     // ctx "rede-zero/proposal/v1"
}
payload = u64(height) ‖ u32(round) ‖ option(pol_round) ‖ block_id
```

Aceita se a assinatura confere, `proposer` é o proponente de (`height`, `round`), o bloco é da altura corrente com pai na ponta e `pol_round < round`. O cabeçalho do bloco guarda a rodada em que ele foi **montado** (`header.round`); numa re-proposta essa rodada é anterior a `round`, e o bloco continua assinado pelo proponente original. Duas propostas diferentes do mesmo proponente para a mesma rodada geram evidência de equivocação.

### 4.2 Voto

```text
Vote {
  kind      : u8            // 1 = Prevote, 2 = Precommit
  height    : u64
  round     : u32
  block     : Option<BlockId>   // None = voto nulo
  validator : PublicKey
  signature : Signature     // ctx "rede-zero/vote/v1"
}
payload = u8(kind) ‖ u64(height) ‖ u32(round) ‖ option(block)
```

Votos de quem não pertence ao conjunto vigente são descartados.

### 4.3 Certificado de finalização

```text
CommitSig { validator : PublicKey, signature : Signature }
Commit    { height : u64, round : u32, block : BlockId, signatures : [CommitSig] }
CommittedBlock { block : Block, commit : Commit }
```

Cada `CommitSig` é a assinatura de um `Precommit` para (`height`, `round`, `Some(block)`). `Commit::verify(h, id, conjunto, rede)` exige, nesta ordem:

| Regra | Erro |
| --- | --- |
| `commit.height = h` e `commit.block = id` | `WrongBlock` |
| todo signatário pertence ao conjunto | `UnknownValidator` |
| nenhum signatário repetido | `DuplicateValidator` |
| toda assinatura confere | `BadSignature` |
| poder dos signatários é maioria qualificada | `InsufficientPower` |

O conjunto usado é `state.validators()` do estado **anterior** ao bloco. Um Node que sincroniza verifica cada bloco pelo seu certificado, sem confiar em quem o enviou (AC-CON-002).

## 5. Algoritmo por altura

Estado local: `round`, `step ∈ {Propose, Prevote, Precommit, Commit}`, `locked (bloco, rodada)`, `valid (bloco, rodada)`.

```text
início da rodada r:
  step ← Propose
  se sou o proponente de (h, r):
      bloco ← valid.bloco se existir, senão monta um novo (mempool)
      difunde Proposal(h, r, valid.rodada, bloco)
  senão agenda timeout_propose(r)

ao receber Proposal(h, r, pol = None, B) em Propose:
  pré-vota B se B é válido e (locked vazio ou locked.bloco = B), senão nulo

ao receber Proposal(h, r, pol = vr, B) com > 2/3 de pré-votos para B em vr < r, em Propose:
  pré-vota B se B é válido e (locked.rodada ≤ vr ou locked.bloco = B), senão nulo

timeout_propose(r) em Propose                       → pré-voto nulo

> 2/3 de pré-votos quaisquer em r, em Prevote       → agenda timeout_prevote(r) (uma vez)
> 2/3 de pré-votos para B válido em r (step ≥ Prevote):
    em Prevote: locked ← (B, r); pré-compromete B
    valid ← (B, r)
> 2/3 de pré-votos nulos em r, em Prevote           → pré-compromisso nulo
timeout_prevote(r) em Prevote                       → pré-compromisso nulo

> 2/3 de pré-compromissos quaisquer em r            → agenda timeout_precommit(r) (uma vez)
timeout_precommit(r)                                → rodada r + 1

> 2/3 de pré-compromissos para B em qualquer rodada, com a proposta de B conhecida e válida:
    DECIDE B com o Commit formado por esses pré-compromissos

> 1/3 de poder com mensagens de uma rodada r' > r   → salta para r'
```

Temporizadores crescem com a rodada: `timeout_x(r) = timeout_x_ms + r · timeout_delta_ms`.

**Validade de bloco** (`App::validate_block`) é a mesma do import (`Chain::check_next`): altura e pai corretos, proponente sorteado para (`height`, `header.round`) e todas as regras de `spec/BLOCKS.md` e `spec/STATE.md`.

**Retransmissão.** Periodicamente, cada Node reenvia a proposta da rodada corrente e os próprios votos da rodada corrente. Isso recupera mensagens perdidas durante partições sem aumentar a superfície de ataque, porque só retransmite o que ele mesmo assinou ou já validou.

**Mensagens futuras e limites de memória** (`FUTURE_ROUND_WINDOW = 2`, `docs/AUDIT.md` RZ-IR-02/03):

* propostas e votos da altura `h + 1` só são guardados (buffer de até 4 096) se o autor pertence ao conjunto vigente e a rodada é `≤ FUTURE_ROUND_WINDOW`; são processados ao iniciar a nova altura;
* na altura corrente, propostas de rodada `> rodada_atual + FUTURE_ROUND_WINDOW` são descartadas;
* para rodadas futuras, guarda-se só o voto de rodada **mais alta** de cada validador e tipo. Isso basta para o salto de rodada (> 1/3 do poder numa mesma rodada à frente) e limita a memória a um voto futuro por validador;
* alturas mais distantes são descartadas; o Node recupera o atraso sincronizando blocos finalizados.

Mensagens descartadas não são repassadas nem marcadas como vistas; a retransmissão as entrega de novo quando a rodada chegar.

**Após decidir**, o Node grava o `CommittedBlock`, difunde `Block` e inicia a altura seguinte após `block_interval_ms`.

## 6. Participação e punição

### 6.1 Operações

| Operação (`TxKind`) | Tag | Efeito |
| --- | --- | --- |
| `Bond { amount }` | 0x20 | debita `amount` e soma ao vínculo; exige `vínculo total ≥ min_bond` e remetente fora de `jailed` |
| `Unbond { amount }` | 0x21 | retira `amount` do vínculo (`amount ≤ vínculo`, restante `= 0` ou `≥ min_bond`) e cria `Unbonding { owner, amount, release_height = h + unbonding_blocks }` |
| `ReportDoubleVote { first, second }` | 0x22 | evidência de voto duplo; pune o validador (§6.2) |

`amount = 0` é inválido. Uma desvinculação continua **punível** até ser liberada; ao atingir `release_height`, o valor volta ao saldo transparente do dono no fim do bloco.

### 6.2 Evidência e punição

Dois votos formam evidência se forem do mesmo validador, tipo, altura e rodada, para blocos diferentes, com ambas as assinaturas válidas (`Vote::conflicts_with`). A infração é identificada por:

```text
evidence_id = H("rede-zero/evidence/v1", validador ‖ u64(h) ‖ u32(r) ‖ u8(tipo))
```

O mesmo vale para dois cabeçalhos distintos assinados pelo mesmo proponente para a mesma altura e rodada (`ReportEquivocation`, `spec/GOVERNANCE.md`).

A punição, aplicada **uma única vez** por `evidence_id`:

1. queima `slash_bps` do vínculo e de cada desvinculação pendente do validador, reduzindo `total_supply`;
2. inclui o validador em `jailed`: ele não volta ao conjunto nem pode vincular de novo;
3. zera os seus pontos de contribuição (câmara de contribuição);
4. retira-o imediatamente do conjunto do próximo bloco (sem esperar a época), exceto se ele for o único validador.

Qualquer conta pode enviar a evidência, pagando a taxa normal. Nodes que detectam voto ou proposta duplicada enviam a evidência automaticamente com a chave do validador local, se houver.

## 7. Finalidade e estados de transação

Não há escolha de fork nem reorganização: um bloco só entra na cadeia com um `Commit` válido, e um bloco aceito é **final** (`SPEC §21`). A importação é:

```text
CommittedBlock recebido
  ├── altura ≤ altura local          → ignorar
  ├── Commit inválido para o conjunto → Commit(..)      (par penalizado)
  ├── altura ≠ local + 1             → NotNext           (solicitar sincronização se à frente)
  ├── pai ≠ ponta                    → WrongParent       (par penalizado)
  ├── proponente ≠ sorteado          → WrongProposer     (par penalizado)
  ├── regras de bloco                → Block(..)         (par penalizado)
  └── acrescentar, gravar, podar o mempool, difundir
```

| Estado da transação | Condição |
| --- | --- |
| recebida | aceita no mempool |
| finalizada | presente em um bloco da cadeia (com `Commit`) |

`status.finalized_height` é igual à altura da cadeia.

## 8. Mensagens P2P

`ConsensusProposal` (0x16) e `ConsensusVote` (0x17) transportam as mensagens de §4; `Block` (0x06) e `Blocks` (0x08) transportam `CommittedBlock`. Ver `spec/P2P.md`. Propostas e votos são deduplicados por identificador, marcados como vistos e repassados **somente quando aceitos** pela máquina de consenso. A fila para a thread de consenso é limitada (8 192); sob inundação, o excedente é descartado.

## 9. Limitações conhecidas

* **anti-Sybil econômico**: a entrada no conjunto depende de ZERO vinculado; a distribuição inicial define o poder inicial (THR-CON-001).
* **ataque de longo alcance**: um Node novo confia no Genesis e na cadeia de certificados; chaves antigas desvinculadas podem forjar histórico alternativo. Mitigação prevista: pontos de verificação sociais (*weak subjectivity*) — **A DEFINIR**.
* **censura pelo proponente**: um proponente pode omitir transações, mas só por uma rodada; a rotação ponderada limita o efeito.
* sem recompensa de bloco além das taxas (`SPEC §40`, A DEFINIR).
* sem punição por inatividade.
* a máquina de consenso foi verificada por simulação (`crates/rz-chain/src/bft_tests.rs`), não por prova formal.

## 10. Testes de aceitação cobertos

AT-CON-001..003, AT-SYNC-002..003, AT-DS-002 e os cenários de simulação: validadores honestos concordam; proponente offline; voto duplo detectado; partição para e depois se recupera; 2 de 4 offline não progridem; validador único; decisão carrega certificado válido.
