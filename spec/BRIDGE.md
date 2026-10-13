# spec/BRIDGE.md — Ponte para ativos externos

**Versão:** 0.1.0 (DEVNET)
**Decisão:** ADR-0019
**Relacionamento:** `SPECIFICATIONS.md §38–§42`, `ARCHITECTURE.md §30`, REQ-036..039, AC-POOL-004/005, AT-POOL-004, THR-POOL-002, THR-BRG-001..004, ADR-0014
**Implementação de referência:** `crates/rz-core/src/bridge.rs`, `crates/rz-core/src/state.rs`, `crates/rz-wallet`

---

## 1. Visão geral

A ponte tem duas peças, **nenhuma com custodiante** (`SPEC §42`):

| Peça | Para quê | Direção |
| --- | --- | --- |
| Prova de queima verificada por cliente leve | trazer a representação de um ativo de outra rede | mão única: a origem é destruída de forma comprovável |
| Troca atômica (HTLC) | trocar ZERO ou ativos por ativos de outra rede entre duas pessoas | os dois sentidos, sem intermediário |

Ninguém guarda o ativo de origem e ninguém precisa ser confiável para liberá-lo. Por isso não existe "resgate" pela ponte: quem quer o ativo de origem de volta faz uma troca atômica com outra pessoa. O Pool continua sem saída (`SPEC §40`).

## 2. Configuração (Genesis)

```text
ChainConfig {
  name          : string (≤32)     // rede de origem, ex.: "bitcoin"
  asset_ref     : string           // ativo nativo, ex.: "BTC"
  decimals      : u8 (≤18)
  rules         : PowRules { pow_limit: fixed[32] (big-endian), retarget: bool, interval: u32, target_timespan: u32 }
  checkpoint    : fixed[80]        // cabeçalho de partida (aceito por configuração)
  checkpoint_height : u64          // múltiplo de interval quando retarget
  confirmations : u32 (1..=1000)
}
Genesis.bridges : list<ChainConfig> (≤16)
```

Validação:

* só DEVNET e TESTNET enquanto não houver auditoria;
* nomes válidos, sem repetição e sem se passar por ZERO ou pela Rede Zero (AT-POOL-004);
* o ponto de partida tem prova de trabalho válida.

Cada rede registra o ativo `AssetId::external(name, asset_ref)` com `Verification::LightClientBurn` e oferta inicial **zero**.

Adicionar redes depois do Genesis exige uma versão nova do software, aprovada pela governança. Uma operação própria para isso fica A DEFINIR.

## 3. Cliente leve

Cabeçalhos de 80 bytes no formato do Bitcoin. `hash = SHA-256(SHA-256(cabeçalho))`; valores de 256 bits em little-endian.

Um cabeçalho é aceito se:

1. não é repetido e se conecta a um cabeçalho conhecido (do estado ou anterior na mesma transação);
2. `bits` é exatamente o exigido:
   * sem reajuste: igual ao do pai;
   * com reajuste, nas alturas múltiplas de `interval`: `CalculateNextWorkRequired` do Bitcoin, com o tempo do período limitado a `[T/4, 4T]` e o alvo limitado a `pow_limit`;
3. a prova de trabalho vale: `hash ≤ alvo(bits) ≤ pow_limit`;
4. `time` > mediana dos até 11 ancestrais disponíveis.

**Nenhuma regra usa relógio** (THR-CON-006): o limite de "2 horas no futuro" do Bitcoin não é aplicado, e quem quisesse adiantar o tempo pagaria a prova de trabalho correspondente.

Trabalho acumulado: `Σ 2^256 / (alvo + 1)`. A ponta é o cabeçalho de maior trabalho acumulado. O índice altura → bloco da cadeia de maior trabalho é refeito a cada mudança de ponta.

Limites de armazenamento:

* são guardados os cabeçalhos até `KEEP_HEADERS = 4032` abaixo da ponta, e o resto é podado; reorganizações mais profundas que isso não são aceitas;
* há um teto de `2 · KEEP_HEADERS` cabeçalhos por rede; acima dele, só entra o que aumenta o trabalho da ponta.

## 4. Prova de queima

Saída de queima na transação de origem:

```text
script = OP_RETURN ‖ push(43) ‖ "RZ1" ‖ etiqueta (8) ‖ endereço Rede Zero (32)
etiqueta = primeiros 8 bytes de H("rede-zero/bridge-tag/v1", network_id)
```

O valor dessa saída é destruído na origem. A etiqueta impede que a mesma queima seja usada em duas redes Zero.

`BridgeDeposit` é aceito se:

* o bloco está na cadeia de maior trabalho, com `confirmations` ou mais (contando o próprio bloco);
* a transação tem a serialização **sem testemunhas** (a que define o identificador), não tem 64 bytes (CVE-2017-12842) e tem no máximo 100 000 bytes;
* a prova de Merkle (`path`, `index`, até 32 níveis) leva do identificador à `merkle_root` do cabeçalho;
* há **exatamente uma** saída de queima para esta rede, com valor positivo;
* o identificador ainda não foi usado.

Efeito: o destinatário gravado na queima (não quem envia a prova) recebe `valor` unidades do ativo, a oferta do ativo aumenta o mesmo tanto e o identificador fica marcado como usado.

`path_from_merkleblock` extrai cabeçalho, caminho e posição da prova `merkleblock` devolvida por `gettxoutproof` nos nós da origem.

## 5. Trocas atômicas

```text
Htlc { id (TxId do bloqueio), sender, recipient, asset (ZERO ou externo), amount, hashlock: fixed[32] (SHA-256), timeout: u64 (altura) }
```

| Tag | Operação | Regras |
| --- | --- | --- |
| `0x72` | `HtlcLock` | `amount > 0`; `timeout > altura`; saldo suficiente (ZERO ou ativo) |
| `0x73` | `HtlcClaim { id, preimage }` | aberto; `altura < timeout`; `len(preimage) = 32`; `SHA-256(preimage) = hashlock`; paga ao destinatário |
| `0x74` | `HtlcRefund { id }` | aberto; `altura ≥ timeout`; devolve ao remetente |

Qualquer conta pode enviar o resgate ou a devolução, mas o valor sempre vai para a parte definida no contrato. A liquidação fica registrada (as 4096 mais recentes), com a pré-imagem quando houve resgate. É com ela que a outra parte resgata na rede de origem.

Uma troca típica de BTC por ZERO funciona assim:

1. Quem tem BTC gera o segredo e cria na origem um HTLC com `OP_SHA256 <hash> OP_EQUALVERIFY` e `OP_SIZE 32 OP_EQUALVERIFY`, com prazo longo.
2. Quem tem ZERO cria o `HtlcLock` com o mesmo hash e prazo **mais curto**.
3. Quem tem BTC resgata na Rede Zero, revelando o segredo.
4. A outra parte usa o segredo para resgatar na origem.

Os valores retidos entram nas invariantes:

* `zero_escrow` entra na oferta de ZERO;
* `asset_escrow` entra na conservação de cada ativo (`check_supply`).

## 6. Operações e mensagens

| Tag | Operação |
| --- | --- |
| `0x70` | `BridgeHeaders { chain, headers: list<fixed[80]> (≤500) }` |
| `0x71` | `BridgeDeposit { chain, block, tx, path, index }` |
| `0x72–0x74` | trocas atômicas (§5) |

| Mensagem | Resposta |
| --- | --- |
| `GET_BRIDGE chain` (0x2f) | `BRIDGE {height, chain, best?: (bloco, altura), confirmations, asset, claimed}` (0x30) |
| `GET_HTLC id` (0x31) | `HTLC {height, htlc?, settled?}` (0x32) |

Raiz: `bridge_root = H("rede-zero/bridge-root/v1", redes (configuração, ponta, cabeçalhos, queimas usadas) ‖ HTLCs ‖ liquidações)`. Ela entra na raiz do estado depois de `runtime_root`.

## 7. Testes

* `bridge::tests`:
  * prova de trabalho do bloco gênese do Bitcoin;
  * vetores de reajuste do Bitcoin Core;
  * forma compacta;
  * saídas de queima, Merkle e extração de `merkleblock` em todas as posições de árvores de 1 a 13 transações.
* `bridge_tests` (estado):
  * regras do Genesis;
  * cunhagem única após confirmações;
  * queima de outra rede e transação de 64 bytes;
  * regras de cabeçalho e reorganização;
  * HTLC com ZERO e com ativo externo.
* `bridge_devnet`: Nodes reais, queima, `merkleblock` e troca atômica.
* `robustness.rs`.
