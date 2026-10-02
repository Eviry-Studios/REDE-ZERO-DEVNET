# ADR-0015 — Comunidades com regra de decisão declarada e nomes `zero://` sobre identificadores

**Estado:** Aceita (DEVNET)
**Escopo:** DEVNET → candidata a TESTNET
**Data:** 2026-09-29
**Relacionamento:** REQ-068..074, REQ-043, REQ-044, REQ-023..027, `SPECIFICATIONS.md §60–§62`, `ARCHITECTURE.md §32`, AC-COM-001..005, AT-COM-001..004, THR-COM-001..003, THR-BRW-002, INV-007, `docs/DEMOCRACIA_ORGANICA.md` N-1..N-8, ADR-0008, ADR-0013
**Especificações:** [`spec/COMMUNITIES.md`](../../spec/COMMUNITIES.md), [`spec/NAMING.md`](../../spec/NAMING.md)
**Implementação:** `crates/rz-core/src/community.rs`, `crates/rz-core/src/state.rs`, `crates/rz-wallet`

## Contexto

A Democracia Orgânica (ADR-0013) foi incorporada com notas de conformidade que já decidem o essencial:

* reconhecimento pela categoria Comunidade da governança (N-7);
* posição comunitária verificável e não vinculante (N-1, N-6);
* filiação pseudônima (N-5);
* nomes como camada sobre identificadores (N-3);
* `.market` nunca como segunda DEX (N-4).

Faltavam o formato no protocolo e as regras objetivas (REQ-073).

## Decisão

1. **Declaração no protocolo** (`DeclareCommunity`): nome, hash do manifesto (identidade, versão, componentes e regras, `SPEC §60`) e **regra de decisão**, isto é, `threshold` de até 32 chaves.
   * A Rede Zero não sabe quem são os membros (N-5) nem como as chaves decidem internamente (voto direto, representantes, consenso).
   * Ela só verifica a **conformidade** com a regra declarada (N-6).
   * A declaração expira se não for reconhecida, o que libera o nome.
2. **Reconhecimento** = proposta aprovada da categoria Comunidade cujo `content_hash` é o id de uma declaração pendente (N-7). A proposta sem declaração válida é rejeitada; essa é a validação técnica.
   * A apuração continua bicameral (ADR-0008).
   * Conteúdo controverso não é motivo técnico de rejeição (REQ-074).
3. **Posição comunitária** (`CommunityPosition`): aceita só durante a votação, de Comunidade reconhecida e com aprovações válidas de pelo menos `threshold` chaves.
   * Fica **registrada** junto da proposta e **não altera** a apuração (N-1).
   * Dar peso decisório exige nova ADR com análise anti-Sybil (REQ-044).
4. **Atualização** (`UpdateCommunity`): nova versão do manifesto e, opcionalmente, nova regra, aprovadas pela regra **vigente**.
   * A versão só aumenta.
   * Guarda-se um histórico de versões para fixação (THR-COM-002).
5. **Nomes** `zero://nome.tipo`:
   * sintaxe objetiva e nomes reservados às funções do protocolo;
   * `.comunidade` só para Comunidades reconhecidas (N-3);
   * os demais tipos são registrados por qualquer conta, com dono e alvo criptográfico;
   * nomes do mesmo tipo com o mesmo **esqueleto** visual são rejeitados (`banco`/`banc0`, `modern`/`rnodern`) (THR-BRW-002);
   * a taxa de registro vai para o Pool permanente, como antispam, sem pagar ninguém (Manifesto §12).
6. **Isolamento** (`SPEC §61`, INV-007): as operações de Comunidade só alteram o registro da própria Comunidade, as posições e os nomes. Não existe operação de Comunidade sobre consenso, saldos de terceiros, regras monetárias ou outras Comunidades (AT-COM-003).

## Alternativas consideradas

| Alternativa | Por que não |
| --- | --- |
| Lista pública de membros e voto por cabeça | Viola N-5 e REQ-023/025; favorece Sybil (N-2) |
| Posição comunitária com peso na apuração | Multiplica influência criando Comunidades (seção 5 da Democracia Orgânica); exige ADR anti-Sybil (N-1) |
| Reconhecimento por um conselho | Autoridade arbitrária e permanente (REQ-074) |
| Nomes sem regras de semelhança | Phishing de endereços (THR-BRW-002) |
| Nomes com taxa paga a operadores ou à governança | O Pool não é fonte de pagamento (Manifesto §12); a taxa só serve contra spam |

## Consequências

* Criar uma Comunidade custa o depósito de governança, devolvido com quórum, e a taxa da transação de declaração. Registrar nome custa `name_fee`, que vai para o Pool.
* Remoção de Comunidade: **não existe** operação unilateral (REQ-074). A remoção por governança, com regras objetivas, fica A DEFINIR.
* Nomes não expiram nesta versão. Renovação e expiração ficam A DEFINIR.
* O Exonet Runtime (execução isolada e distribuída de aplicações, `SPEC §62`, Democracia Orgânica §9–§13) e o Navegador Zero não fazem parte desta decisão. O estado de uma Comunidade reconhecido pela rede é o registro no estado da cadeia, replicado pelo consenso (AT-COM-004).

## Condições de revisão

* Mecanismo de provas de pertinência para votação interna sem revelar membros (N-5).
* ADR anti-Sybil que dê peso às posições comunitárias (N-1).
* Especificação do Exonet Runtime (`spec/RUNTIME.md`).
