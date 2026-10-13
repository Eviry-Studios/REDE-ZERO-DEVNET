# ADR-0022 — Suíte de conformidade em vetores de texto

**Estado:** Aceita (DEVNET)
**Escopo:** DEVNET → TESTNET
**Data:** 2026-10-13
**Relacionamento:** `SPECIFICATIONS.md §68–§69`, REQ-004, REQ-016, THR-TX-005, ADR-0004 (codificação canônica), ADR-0021 (campo `tests` das versões)
**Especificação:** [`spec/CONFORMANCE.md`](../../spec/CONFORMANCE.md)
**Implementação:** `conformance/`, `crates/rz-conformance`

## Contexto

`SPEC §68` exige resultados idênticos entre implementações. `SPEC §69` pede uma suíte de conformidade executável por qualquer implementação.

Até aqui só havia os vetores de criptografia (`spec/CRYPTOGRAPHY.md §8`) e os testes internos da implementação de referência, escritos em Rust contra a API dela. Esses testes não servem a outra implementação.

O manifesto de versão (ADR-0021) já reserva um hash para "os testes", sem uma suíte que ele identifique.

## Decisão

1. **Vetores como dados, não como código.** A suíte é um conjunto de arquivos de texto simples em `conformance/`, com casos de entrada e resultado esperado em hexadecimal e decimal, lidos sem nenhuma biblioteca específica.
2. **Cobertura do que precisa coincidir:**
   * identidade e assinaturas;
   * hashes;
   * valores de ZERO;
   * codificação canônica, com aceitação e rejeição;
   * regras sem estado das transações;
   * uma cadeia completa de blocos com a raiz do estado após cada um, passando por todas as áreas do protocolo, e blocos inválidos que precisam ser rejeitados;
   * mensagens P2P;
   * decaimento de pontos;
   * execução do Runtime (combustível, saída, escritas e erros, que entram no estado);
   * ponte;
   * manifestos de versão.
3. **Vetores publicados são a referência.** A CI verifica a implementação de referência contra eles. Mudar um vetor é mudar o protocolo e exige uma nova versão.
4. **Geração com garantias.** O gerador recusa um cenário que não exercite o que promete e uma rejeição causada por outra regra que não a pretendida.
5. **Ligação com versões.** O campo `tests` do manifesto é o hash de árvore de `conformance/`.

## Alternativas consideradas

| Alternativa | Por que não |
| --- | --- |
| Testes em Rust como referência | Só servem à implementação de referência; outra linguagem não os executa |
| JSON ou outro formato com bibliotecas | Acrescenta dependência e ambiguidades de números e texto; o formato de linhas é trivial de ler em qualquer linguagem |
| Vetores só de funções isoladas | Não testam o determinismo da transição de estado, que é o que `SPEC §68` exige |
| Gerar os vetores a cada execução da CI | Esconderia mudanças de comportamento; o objetivo é detectar qualquer divergência em relação ao que foi publicado |

## Consequências

* Uma segunda implementação tem contra o que se comparar desde o primeiro dia, e uma divergência aponta o bloco e a área exatos.
* Qualquer mudança acidental de comportamento na implementação de referência quebra a CI.
* Mudanças deliberadas de protocolo precisam regenerar os vetores, o que torna a mudança visível na revisão.

## Condições de revisão

* Vetores de interação para consenso (mensagens de voto e certificados) e para o handshake P2P.
* Primeira implementação independente: os vetores que ela contestar devem ser revistos contra a especificação, não contra a implementação de referência.
