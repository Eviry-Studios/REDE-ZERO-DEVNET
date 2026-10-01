# ADR-0018 — Exonet Runtime: WebAssembly determinístico, isolado por Comunidade

**Estado:** Aceita (DEVNET)
**Escopo:** DEVNET → candidata a TESTNET
**Data:** 2026-10-01
**Relacionamento:** `SPECIFICATIONS.md §60–§62`, `ARCHITECTURE.md §24–§25`, `§57`, `§58`, REQ-068..074, REQ-016, AT-COM-003, `ACCEPTANCE_TESTS.md §73`, THR-COM-001, THR-TX-005, THR-DEV-001, INV-007
**Especificação:** [`spec/RUNTIME.md`](../../spec/RUNTIME.md)
**Implementação:** `crates/rz-core/src/runtime.rs`, `crates/rz-core/src/state.rs`, `crates/rz-node`, `crates/rz-wallet`, `crates/rz-browser`

## Contexto

Uma Comunidade tem interface, conteúdo, **lógica** e **estado** (`ARCHITECTURE §24`). A ADR-0017 cobriu interface e conteúdo; faltava a execução. Os documentos exigem:

* isolamento e menor privilégio (`SPEC §62`);
* que uma Comunidade comprometida não altere consenso, saldos, regras monetárias, identidades de Nodes ou outras Comunidades (`SPEC §61`, teste §73);
* determinismo de toda transição de estado (REQ-016, THR-TX-005).

A linguagem e o mecanismo de execução estavam A DEFINIR.

## Decisão

1. **WebAssembly**, executado por um interpretador em Rust (`wasmi` 2.0.0).
   * É padrão aberto, com várias linguagens compilando para ele (Rust, C, AssemblyScript, Zig).
   * O interpretador é escrito em Rust, já passou por auditoria externa e mede combustível por instrução.
   * Foi preferido a compiladores JIT, pela superfície menor e pelo comportamento previsível.
2. **Determinismo por configuração**:
   * ponto flutuante, SIMD, memória de 64 bits, múltiplas memórias e chamadas de cauda desligados;
   * limites fixos de memória (4 MiB), tabela, recursão e estrutura;
   * despacho portátil (não depende de otimização do compilador);
   * versão exata fixada.

   A versão do interpretador passa a fazer parte do protocolo.
3. **Execução na cadeia, paga por combustível.**
   * Chamadas são transações; o combustível reservado é pago na taxa, mesmo se a chamada falhar.
   * Falhas não gravam nada e não invalidam o bloco: isso impede usar transações que falham como ataque gratuito.
   * Há limite por chamada e por bloco.
4. **Capacidades mínimas.** O módulo recebe só: argumentos, remetente, altura, o armazenamento **da própria Comunidade**, saída e aborto.
   * Funções sobre saldos, ZERO, governança, consenso ou outras Comunidades não existem, então não há o que contornar.
   * A publicação recusa importações desconhecidas.
5. **Vínculo pela regra de decisão.**
   * Só Comunidades reconhecidas vinculam módulos.
   * A vinculação exige o limiar de chaves da Comunidade (ADR-0015), com contador contra repetição.
   * O módulo citado no manifesto é informativo; vale o vínculo na cadeia.
6. **Publicação paga por byte para o Pool**, como a taxa de nome. **Cota de armazenamento** por Comunidade, com custo por byte escrito.
7. **Consultas somente leitura** pelos Nodes (sem gravar, com limite próprio), para interfaces lerem estado sem pagar.

## Alternativas consideradas

| Alternativa | Por que não |
| --- | --- |
| VM e linguagem próprias | Mais auditável em tamanho, mas sem ferramentas nem linguagens; obrigaria desenvolvedores a escrever bytecode |
| EVM | Modelo de contas e de ativos acoplado; ofereceria acesso a saldos que `SPEC §61` proíbe às Comunidades |
| JIT (wasmtime, wasmer) | Rápidos, mas com superfície e dependências muito maiores e determinismo mais difícil de garantir |
| Execução fora da cadeia, só nos Nodes da Comunidade | Estado não verificável por todos; volta a depender de operadores específicos |
| Ponto flutuante com NaN canônico | Ainda há diferenças sutis entre plataformas; nenhum caso de uso da DEVNET precisa |
| Acesso de módulos a ZERO (pagamentos) | Contraria `SPEC §61`; pagamentos ficam com o Grande Mercado e a Wallet |

## Consequências

* Comunidades ganham lógica e estado verificáveis por qualquer Node, sem nenhum poder sobre o resto da rede (teste §73).
* Validadores executam código de terceiros, limitado por combustível, memória e tempo de bloco. A cota e o custo por byte contêm o crescimento do estado.
* A troca de versão do `wasmi` exige o mesmo processo de uma atualização de protocolo.
* Limitações da DEVNET:
  * o armazenamento da Comunidade é copiado a cada chamada;
  * as consultas usam uma cópia do armazenamento;
  * o custo por instrução segue a tabela padrão do interpretador;
  * não há chamadas entre módulos.

## Condições de revisão

* Auditoria do interpretador na configuração usada e da tabela de combustível.
* Aluguel de armazenamento ou expiração de dados.
* Eventos indexáveis para interfaces.
* Chamadas entre módulos, sempre sem acesso a saldos.
* Módulos para aplicações que não são Comunidades.
