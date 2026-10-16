# ADR-0024 — Resistência a eclipse e a corrupção local no Node

**Estado:** Aceita (DEVNET)
**Escopo:** DEVNET → TESTNET
**Data:** 2026-10-16
**Relacionamento:** THR-P2P-001, ADV-03, ADV-06, `SPECIFICATIONS.md §28–§29`, `ACCEPTANCE_TESTS.md §74, §77, §79`, ADR-0007 (P2P da DEVNET)
**Especificação:** [`spec/P2P.md §7.1–§7.2`](../../spec/P2P.md)
**Implementação:** `crates/rz-node/src/addrbook.rs`, `crates/rz-node/src/node.rs`, `crates/rz-node/src/store.rs`

## Contexto

O modelo de ameaças lista contra o eclipse (THR-P2P-001):

* diversidade de pares por faixa de endereço e por origem;
* conexões de saída escolhidas pelo próprio Node;
* detecção de estagnação.

A estratégia final estava A DEFINIR após testes. Os testes de arquitetura mostraram três problemas no Node:

* o livro de endereços guardava os primeiros 1 024 anunciados e nunca os trocava;
* as conexões de saída eram escolhidas na ordem do conjunto, e não ao acaso;
* não havia detecção de estagnação.

O armazenamento já tratava o disco como fonte não confiável, mas um bloco corrompido no meio do arquivo continuava gravado, e blocos novos eram anexados depois dele.

## Decisão

1. **Faixas de rede.** Cada endereço pertence a um `/16` (IPv4) ou `/32` (IPv6). O livro limita endereços por faixa (32) e por faixa de origem (64); cheio, despeja ao acaso da faixa mais cheia.
2. **Saída diversa e aleatória.** Os pares de partida vêm primeiro, depois o livro; no máximo uma conexão de saída por faixa, escolhida ao acaso. Entradas também são limitadas por faixa (16).
3. **Estagnação.** Sem blocos novos por 30 intervalos de bloco (mínimo de 30 s), um par de saída é trocado, e o trocado fica em espera.
4. **Disco.** O arquivo é cortado no primeiro registro ilegível ou bloco que não verifica, e a cadeia é reconstruída pelos pares. As gravações usam `fsync`.

## Alternativas consideradas

| Alternativa | Por que não |
| --- | --- |
| Tabelas "novo/testado" com hash secreto por bucket (estilo Bitcoin Core) | Mais forte contra atacantes adaptativos, mas mais complexa; os limites por faixa e origem cobrem a inundação, que é o ataque de menor custo. Fica como condição de revisão |
| Limitar só por IP | Um atacante com um bloco de endereços vizinhos ocuparia todas as conexões |
| Reconstruir o estado a partir do bloco corrompido, ignorando-o | Os blocos seguintes dependem dele; aceitar a sequência seria aceitar estado não verificado |
| Soma de verificação por registro | Cada bloco já é verificado por certificado e assinaturas no replay; a soma não acrescenta garantia |

## Consequências

* Para eclipsar um Node, o atacante precisa de endereços em muitas faixas, e ainda assim só ocupa o livro na proporção dessas faixas. Os pares de partida continuam sendo a âncora.
* Uma rede legitimamente parada (mais de 1/3 dos validadores fora) provoca trocas de par periódicas, o que é inofensivo.
* Endereços onion são baratos e cada um forma a própria faixa. Contra eles, a defesa são os pares de partida e a estagnação.
* Um disco corrompido custa só a ressincronização do trecho afetado.

## Condições de revisão

* Buckets com hash secreto e conexões de saída "âncora" persistidas entre reinícios.
* Dados de TESTNET sobre a rotação por estagnação.
