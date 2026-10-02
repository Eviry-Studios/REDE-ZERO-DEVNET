# ADR-0008 — Governança bicameral com compromisso temporal

**Estado:** Aceita (aprovada em 2026-09-29) — implementada na DEVNET
**Escopo:** DEVNET → candidata a TESTNET
**Data:** 2026-09-29
**Relacionamento:** REQ-040..045, REQ-080, REQ-092..095, `ARCHITECTURE.md §31–32`, `SPECIFICATIONS.md §43–47`, THR-GOV-001..006
**Especificação:** [`spec/GOVERNANCE.md`](../../spec/GOVERNANCE.md)

## Contexto

`ARCHITECTURE.md §32` descarta de antemão as duas regras óbvias:

* `1 carteira = 1 voto` — quebrada por criação massiva de identidades (Sybil, REQ-044);
* `1 ZERO = 1 voto` — concentra poder em grandes detentores (REQ-045).

Há um resultado incômodo que precisa orientar a decisão: **sem prova de pessoa única, qualquer regra que dê a uma identidade menos peso que proporcional ao seu recurso é explorável por Sybil.** Votação quadrática, tetos por carteira e peso por raiz quadrada parecem limitar concentração, mas um grande detentor simplesmente divide seus fundos entre muitas identidades e recupera — ou até amplia — seu peso. A Rede Zero também não pode exigir prova de pessoa baseada em identidade civil (REQ-010, REQ-027).

Portanto, limitar concentração **dentro** de uma única métrica não funciona. A proposta é combinar **recursos escassos independentes**, cada um neutro a Sybil, e exigir aprovação em todos.

## Decisão proposta

### 1. Duas câmaras

| Câmara | Recurso | Peso de um participante | Neutro a Sybil? |
| --- | --- | --- | --- |
| **Econômica** | ZERO bloqueado com compromisso temporal | `bloqueado × m(duração)` | Sim — linear no recurso: dividir entre identidades não aumenta o peso total |
| **Contribuição** | Pontos de contribuição verificável, com decaimento | `pontos_decaídos` | Sim — pontos vêm de trabalho verificável; dividir o trabalho entre identidades produz o mesmo total |

Uma proposta só é aprovada se atingir **quórum e limiar nas duas câmaras**. Capital sozinho não aprova nada; contribuição sozinha também não.

### 2. Compromisso temporal na câmara econômica

`m(d)` cresce com a duração do bloqueio: 1× para o bloqueio mínimo (1 época) até 4× para o máximo (~1 ano). Quem vota com muito peso fica exposto por muito tempo às consequências da própria decisão, e ataques de "comprar, votar e vender" ou empréstimo relâmpago deixam de funcionar. Só conta o ZERO bloqueado **antes** da altura de submissão da proposta.

### 3. Contribuição verificável

Pontos surgem **apenas** de eventos que o protocolo verifica sozinho (SPEC §48), com meia-vida de ~6 meses (REQ-048: reputação passada não é imunidade):

* bloco válido produzido e finalizado;
* tarefa de computação verificável concluída (quando existir, Manifesto §15);
* atestação de defesa com contribuição verificável (SPEC §57);
* recompensa de contribuição aprovada pela própria governança (ex.: auditoria, correção de vulnerabilidade).

Evidência de equivocação ou outra violação comprovada zera os pontos da identidade (SPEC §49). Pontos **não podem ser comprados nem transferidos**.

### 4. Categorias

| Categoria | Exemplos | Limiar (ambas as câmaras) | Quórum | Ativação |
| --- | --- | --- | --- | --- |
| Ordinária | parâmetros operacionais (taxa mínima, limites) | > 50% | 10% | ≥ 7 dias |
| Comunidade | publicação oficial de Comunidade (REQ-071..073) | > 50% | 5% | imediata |
| Constitucional | consenso, regras monetárias, privacidade, Pool, a própria governança | ≥ 2/3 | 20% | ≥ 30 dias |

O atraso de ativação das propostas constitucionais existe para garantir **direito de saída**: quem discordar tem tempo para migrar ou manter um fork (REQ-080).

### 5. Salvaguardas por construção

* Não existe tipo de proposta capaz de alterar saldos de endereços específicos, confiscar fundos ou retirar ativos do Pool (REQ-038) — a operação simplesmente não existe no protocolo.
* A governança não pode prolongar modos de defesa além dos limites da especificação de defesa (REQ-059).
* O Agente Zero pode anexar análises (referenciadas por hash), mas **não vota e não tem peso**.

### 6. Freio de concentração

Se um único votante detiver mais de 1/3 do peso participante em uma câmara, o limiar da **outra** câmara sobe para 2/3. A regra é objetiva e determinística. Ela é contornável por divisão de identidades e está documentada como **obstáculo adicional**, não como proteção principal — a proteção principal é a exigência bicameral.

### 7. Anti-spam

Depósito fixo em ZERO (parâmetro do Genesis). É **devolvido** se a proposta atingir quórum nas duas câmaras (aprovada ou não) e **queimado** caso contrário. A queima não beneficia ninguém, portanto não cria incentivo para barrar propostas alheias.

### 8. Verificabilidade

Propostas, bloqueios e votos são transações assinadas registradas no estado. A apuração é uma função determinística do estado na altura de encerramento, recalculável por qualquer Node (REQ-043).

## Alternativas consideradas

| Alternativa | Motivo da rejeição |
| --- | --- |
| 1 carteira = 1 voto | Sybil trivial (`ARCHITECTURE §32`) |
| 1 ZERO = 1 voto sem bloqueio | Concentração, compra de votos momentânea, empréstimo relâmpago |
| Votação quadrática | Explorável por divisão de fundos entre identidades |
| Teto por carteira | Idem |
| Prova de pessoa | Exige identidade civil ou autoridade emissora — viola REQ-010, REQ-027 |
| Conselho eleito | Cria autoridade permanente e ponto de coerção (THR-DEV-005) |
| Delegação líquida | Tende a concentrar em poucos delegados; adiada para avaliação futura |

## Consequências

* Nenhum grupo com um único tipo de recurso aprova mudanças sozinho.
* Participar da câmara econômica expõe publicamente o valor bloqueado. É uma escolha voluntária: recomenda-se um endereço dedicado e transparente para governança, separado dos fundos privados (ADR-0009).
* Votos são públicos nesta versão, o que favorece a auditabilidade mas deixa votantes expostos a coerção (THR-GOV-006). Voto secreto (commit-reveal ou prova de conhecimento-zero) fica como evolução.
* **Limitação na DEVNET:** enquanto o consenso usar validadores fixos (ADR-0006), a câmara de contribuição será dominada por eles. Ela só se torna significativa com o consenso definitivo e a computação verificável.

## Riscos residuais

* Conluio entre grandes detentores e grandes contribuidores.
* Baixa participação — mitigada por quórum, mas não eliminada.
* Coerção de votantes enquanto os votos forem públicos.

## Implementação

`crates/rz-core/src/governance.rs` (regras e apuração), `state.rs` (efeitos e fim de bloco), `governance_tests.rs` (AT-GOV-001..005 e propriedades), `crates/rz-wallet/tests/governance_devnet.rs` (ponta a ponta).

A evidência de equivocação passou a ser uma transação (`ReportEquivocation`): dois cabeçalhos assinados pelo mesmo produtor para o mesmo slot zeram seus pontos de contribuição — penalização objetiva com evidência verificável (`SPEC §49`).

## Condições antes da TESTNET

1. Revisão pública desta decisão.
2. Simulação dos parâmetros (quórum, `m(d)`, meia-vida) com cenários adversariais.
3. Implementação com testes de aceitação `AT-GOV-001..005` e testes de propriedade da apuração.
