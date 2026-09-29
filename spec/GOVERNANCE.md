# spec/GOVERNANCE.md — Governança

**Versão:** 0.1.0 (DEVNET) — aprovada e implementada
**Relacionamento:** ADR-0008, REQ-040..045, `ARCHITECTURE.md §31–32`, `SPECIFICATIONS.md §43–47`, THR-GOV-001..006

---

> Implementação de referência: `crates/rz-core/src/governance.rs` e `state.rs`. Os parâmetros numéricos são valores iniciais, ajustáveis pela própria governança (categoria constitucional).

## 1. Ciclo de vida

```text
Discussão (fora da cadeia)
   │
   ▼
SUBMETIDA ── tx Propose + depósito
   │  período de análise: A blocos (sem votos)
   ▼
EM_VOTAÇÃO
   │  período de votação: V blocos
   ▼
APURAÇÃO (determinística na altura de encerramento)
   │
   ├── APROVADA ──► AGENDADA ──(atraso de ativação)──► ATIVADA ──► registrada
   ├── REJEITADA
   └── SEM_QUÓRUM (depósito queimado)
```

## 2. Parâmetros (Genesis)

| Parâmetro | DEVNET | Descrição |
| --- | --- | --- |
| `gov_deposit` | 100 ZERO | depósito de proposta |
| `gov_analysis_blocks` | ~2 dias | período de análise |
| `gov_voting_blocks` | ~7 dias | período de votação |
| `lock_min_blocks` | 1 época (~7 dias) | bloqueio mínimo |
| `lock_max_blocks` | ~1 ano | bloqueio máximo |
| `contribution_half_life` | ~6 meses | meia-vida dos pontos |

Durações são expressas em **blocos**, nunca em tempo de relógio (THR-CON-006).

## 3. Transações

| Tag | Operação | Campos | Efeito |
| --- | --- | --- | --- |
| `0x10` | `LockStake` | `amount, unlock_height` | move ZERO do saldo livre para bloqueado |
| `0x11` | `Unlock` | `lock_id` | devolve após `unlock_height` |
| `0x12` | `Propose` | `category, content_hash, params, release_id?` | cria proposta e debita depósito |
| `0x13` | `Vote` | `proposal_id, choice ∈ {SIM, NÃO, ABSTENÇÃO}` | registra/substitui o voto da conta |
| `0x14` | `ReportEquivocation` | `first, second: (BlockHeader, assinatura)` | zera os pontos de contribuição do produtor que assinou dois cabeçalhos distintos para o mesmo slot |

Formatos exatos:

```text
LockStake          { amount: u64, unlock_height: u64 }              lock_min ≤ unlock_height − h ≤ lock_max
Unlock             { lock_id: u64 }                                  dono, h ≥ unlock_height
Propose            { category: u8, content_hash: fixed[32], params: list<ParamChange> (≤16),
                     release_id: option<fixed[32]>, deposit: u64 }   deposit ≥ gov_deposit vigente
Vote               { proposal: fixed[32], choice: u8 }               voting_start ≤ h < voting_end
ReportEquivocation { first: SignedHeader, second: SignedHeader }     mesmo produtor e slot, ids distintos, assinaturas válidas

ParamChange (tag u8): 0 min_fee u64 | 1 max_block_txs u32 | 2 gov_deposit | 3 analysis_blocks | 4 voting_blocks
                      | 5 ordinary_delay_blocks | 6 constitutional_delay_blocks | 7 lock_min_blocks
                      | 8 lock_max_blocks | 9 contribution_half_life_blocks          (u64, exceto tag 1)
```

`h` é a altura do bloco em que a transação é incluída. O identificador da proposta é o `TxId` da transação `Propose`.

## 3.1 Fim de bloco

Após as transações e a distribuição de taxas, em ordem:

1. o produtor do bloco recebe 1 ponto de contribuição (escala 10⁶), após aplicar o decaimento acumulado;
2. propostas pendentes com `voting_end = h` são apuradas (seção 5); o depósito é devolvido ao proponente se houver quórum nas duas câmaras, ou **queimado** (reduz a oferta total) caso contrário;
3. propostas aprovadas com `activation_height = h` são ativadas: as alterações são aplicadas a uma cópia dos parâmetros, que é validada; se inválida, a proposta fica `ActivationFailed` e nada muda.

Propostas são processadas em ordem crescente de identificador.

* `content_hash` referencia o texto completo, armazenado fora da cadeia (Manifesto §6: a cadeia não é depósito de dados).
* Propostas constitucionais de protocolo **devem** incluir `release_id` que identifique versão, especificação, implementação e testes (`SPEC §63`).
* Um voto pode ser substituído até o fim do período; vale o último.
* O peso de câmara de contribuição é atribuído à identidade de Node; o Node assina seu voto com a chave de Node, separada da Wallet.

## 4. Pesos

### 4.1 Câmara econômica

```text
d    = unlock_height − altura_do_bloqueio          (lock_min ≤ d, limitado a lock_max)
m(d) = 1 + 3 × (min(d, lock_max) − lock_min) / (lock_max − lock_min)       // de 1× a 4×
peso_E(conta) = Σ bloqueios_válidos( amount × m(d) )
```

Apenas bloqueios existentes **antes** da altura de submissão contam, e com `unlock_height ≥` fim da votação.

Aritmética inteira com escala fixa de `10⁶`, sem ponto flutuante.

### 4.2 Câmara de contribuição

```text
peso_C(node) = Σ eventos( pontos_evento × 2^(−idade/meia_vida) )
```

O decaimento é calculado em aritmética inteira:

```text
decay(v, dt, hl) = (v >> ⌊dt/hl⌋) × T[⌊16·(dt mod hl)/hl⌋] / 10⁶
T[i] = round(10⁶ × 2^(−i/16)),  i = 0..15
     = 1000000, 957603, 917004, 878126, 840896, 805245, 771105, 738413,
       707107, 677128, 648420, 620929, 594604, 569394, 545254, 522137
```

| Evento verificável | Pontos |
| --- | --- |
| Bloco produzido e finalizado | 1 |
| Tarefa de computação verificada | a definir |
| Atestação de contribuição defensiva | a definir |
| Recompensa aprovada por governança | definida na proposta |
| Violação comprovada (`ReportEquivocation`) | zera os pontos |

## 5. Apuração

Para cada câmara `X ∈ {E, C}`:

```text
participação_X = Σ peso dos votantes (SIM + NÃO + ABSTENÇÃO)
quórum_X      = participação_X ≥ q × peso_total_elegível_X
aprovação_X   = SIM / (SIM + NÃO) ≥ limiar
```

Resultado:

```text
se ¬quórum_E ∨ ¬quórum_C            → SEM_QUÓRUM (queima o depósito)
senão se aprovação_E ∧ aprovação_C  → APROVADA  (devolve o depósito)
senão                                → REJEITADA (devolve o depósito)
```

**Freio de concentração:** se um único votante tiver mais de 1/3 da participação de uma câmara, o limiar da outra câmara passa a ser 2/3.

Comparações em frações inteiras (`a × d ≥ b × c`), sem divisão.

## 6. Categorias

| Categoria | Limiar | Quórum | Atraso de ativação |
| --- | --- | --- | --- |
| Ordinária | > 1/2 | 10% | ≥ 7 dias |
| Comunidade | > 1/2 | 5% | 0 |
| Constitucional | ≥ 2/3 | 20% | ≥ 30 dias |

A categoria de uma proposta é verificável: cada parâmetro do protocolo pertence a uma categoria fixa nesta especificação. Uma proposta que altere parâmetro constitucional com categoria ordinária é **inválida**.

## 7. Invariantes

* **GOV-INV-1** — Nenhuma proposta altera o saldo de um endereço específico.
* **GOV-INV-2** — Nenhuma proposta retira ativos do Pool (REQ-038).
* **GOV-INV-3** — A apuração depende apenas do estado na altura de encerramento.
* **GOV-INV-4** — Peso econômico é linear no valor bloqueado (neutralidade a Sybil).
* **GOV-INV-5** — Pontos de contribuição não são transferíveis.

## 8. Estado e invariante monetária

A parte de governança do estado (parâmetros vigentes, bloqueios, próximo id de bloqueio, propostas com votos e apuração, pontos de contribuição e comunidades aprovadas) entra na raiz do estado via `H("rede-zero/governance-root/v1", …)`.

```text
Σ saldos + oferta_privada + Σ bloqueios + Σ depósitos_de_propostas_pendentes == oferta_total
```

## 9. Testes de aceitação

| Teste | Verificação |
| --- | --- |
| AT-GOV-001 | Proposta válida com depósito é registrada |
| AT-GOV-002 | Proposta malformada ou com categoria errada é rejeitada |
| AT-GOV-003 | Depósito insuficiente é rejeitado |
| AT-GOV-004 | Voto fora do período é rejeitado |
| AT-GOV-005 | Voto repetido substitui o anterior, nunca soma |
| Propriedade | Dividir um bloqueio entre N contas não altera o peso total |
| Propriedade | Apuração idêntica em Nodes diferentes |
| Propriedade | Aprovação exige as duas câmaras |

Implementados em `crates/rz-core/src/governance.rs`, `governance_tests.rs` e `crates/rz-wallet/tests/governance_devnet.rs`.
