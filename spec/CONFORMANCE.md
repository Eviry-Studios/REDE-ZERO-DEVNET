# spec/CONFORMANCE.md — Suíte de conformidade

**Versão:** 0.1.0 (DEVNET)
**Decisão:** ADR-0022
**Relacionamento:** `SPECIFICATIONS.md §68–§69`, REQ-004, REQ-016, THR-TX-005, `spec/RELEASES.md` (campo `tests`)
**Vetores:** [`conformance/`](../conformance)
**Verificador de referência:** `crates/rz-conformance` (`zero-conformidade verificar`)

---

## 1. Propósito

`SPEC §68` exige o mesmo resultado em duas implementações que recebam o mesmo bloco, o mesmo estado e as mesmas regras. `SPEC §69` pede uma suíte executável por qualquer implementação.

Os vetores em `conformance/` são essa suíte. São arquivos de texto simples, independentes de linguagem e de biblioteca. Uma implementação é **conforme** quando, para cada caso:

* aceita o que o vetor aceita e rejeita o que ele rejeita;
* produz exatamente os valores esperados (identificadores, hashes, raízes de estado, saídas).

O campo `motivo` de uma rejeição é informativo. Ele mostra o erro da implementação de referência, e o texto do erro não precisa coincidir. A exceção são os erros do Runtime (`erro`), que entram nos recibos e portanto no estado (§3.9).

Divergência é falha de compatibilidade até ser investigada (`SPEC §68`). Os vetores publicados têm precedência sobre o comportamento de qualquer implementação, inclusive a de referência. Mudar um vetor é mudar o protocolo e exige uma nova versão (ADR-0021).

Todas as chaves dos vetores são derivadas de sementes públicas: são **apenas para desenvolvimento, sem valor**.

## 2. Formato

```text
# título (primeira linha de comentário)
[caso nome-do-caso]
campo = valor
campo = valor
```

* UTF-8, uma entrada por linha; linhas vazias e linhas iniciadas por `#` são ignoradas.
* O valor é tudo após o primeiro `=`, sem espaços nas pontas.
* Binário em hexadecimal minúsculo; números em decimal; texto entre aspas (`"…"`), sem escapes.
* Campos podem se repetir (`caminho`, `armazenamento`, `escrita`, `arquivo`); a ordem dos repetidos importa.
* A ordem dos casos importa em `cadeia.txt` e `runtime.txt`.
* `resultado` é `ok` ou `rejeitado` (no Runtime: `ok` ou `falha`).

Rede usada nos vetores: `rede-zero-devnet-conformidade`.

## 3. Conjuntos

### 3.1 `identidade.txt` — chaves e assinaturas (`spec/CRYPTOGRAPHY.md`)

* Com `semente`: derivar a chave Ed25519 da semente; conferir `chave_publica`, `node_id` e `endereco`.
* Em todo caso: verificar `assinatura` sobre `mensagem` com `contexto` e `rede` (`spec/CRYPTOGRAPHY.md §3`). O resultado precisa ser `valida` (`sim`/`nao`).

### 3.2 `hash.txt`

`saida = H(contexto, entrada)` (BLAKE3 com separação de domínio). O contexto especial `sha256d` significa SHA-256 aplicado duas vezes, que é a função das redes de origem da ponte.

### 3.3 `zero.txt` — valores de ZERO (`spec/STATE.md §1`)

* `ler`: interpretar `texto` como ZERO, com até 8 casas decimais e ponto como separador. Esperado: `unidades`, ou rejeição.
* `escrever`: formatar `unidades` como texto canônico.

### 3.4 `codificacao.txt` — codificação canônica (`spec/ENCODING.md`)

Decodificar `bytes` como `tipo`:

* `transacao`: Transaction;
* `genesis`: Genesis, que também precisa ser válido;
* `versao`: manifesto de versão.

Uma decodificação aceita precisa consumir todos os bytes, e a reencodificação precisa devolver os mesmos bytes. Esperado: `id` (TxId, hash do Genesis ou `release_id`), ou rejeição. Cobre bytes sobrando, truncamento, tags desconhecidas, `option` com tag 2 e listas fora de ordem.

### 3.5 `transacoes.txt` — regras sem estado (`spec/TRANSACTIONS.md`)

Decodificar a transação de conta e conferir `id`. Em seguida, aplicar as regras que não dependem do estado:

* versão;
* valor não nulo;
* taxa ≥ `taxa_minima`;
* formato de cada operação;
* assinatura sobre a `rede`.

O resultado precisa ser o esperado.

### 3.6 `cadeia.txt` — blocos e estado (`spec/BLOCKS.md`, `spec/STATE.md`, `SPEC §68`)

O primeiro caso traz o Genesis: `bytes`, `id` (hash) e `raiz_do_estado` inicial. Cada caso seguinte traz um bloco (`bytes`), aplicado sobre a ponta atual com todas as regras que não dependem do consenso:

* versão, pai, altura, `tx_root` e assinatura do produtor;
* execução das transações;
* taxas e regras de fim de bloco;
* raiz do estado.

`ok` avança a ponta e confere `id` e `raiz_do_estado`. `rejeitado` testa o bloco sobre a ponta e o descarta.

O cenário passa por:

* transferências, blindagens e uma transferência privada (RingCT), com gasto duplo rejeitado;
* bloqueio, propostas ordinária e de Comunidade, votos, apuração e ativação;
* nome `zero://`, módulo do Runtime publicado, vinculado e chamado (com uma chamada que falha);
* leilão do Grande Mercado e depósito no Pool;
* cabeçalhos da ponte, cunhagem por prova de queima (com reaproveitamento rejeitado) e troca atômica;
* transição de defesa atestada;
* publicação e adoção de versão;
* rotação de chave de conta e registro de recuperação.

Rejeições cobrem nonce, saldo, raiz do estado, assinatura, pai, altura, raiz de transações e chave substituída.

A raiz do estado compromete todo o estado (`spec/STATE.md §3`). Coincidir em todas as raízes é coincidir em todo o resultado.

### 3.7 `p2p.txt` — mensagens (`spec/P2P.md`)

Decodificar `bytes` como mensagem P2P (sem moldura) e conferir `nome`, ou rejeitar.

### 3.8 `governanca.txt`

`decaimento`: `resultado = decay(valor, idade, meia_vida)` (`spec/GOVERNANCE.md §4.2`).

### 3.9 `runtime.txt` — Exonet Runtime (`spec/RUNTIME.md`)

* `validar`: conferir o `id` do `modulo` e se ele é aceito (sem ponto flutuante, só importações do módulo `zero`). O nome do caso identifica o módulo para os casos seguintes.
* `executar`: executar `metodo` do módulo nomeado com `comunidade`, `chamador`, `altura`, `entrada`, `combustivel`, `cota` e o armazenamento inicial (`armazenamento = chave:valor`). Conferir:
  * `resultado`;
  * `combustivel_usado`;
  * `saida`;
  * em caso de falha, `erro`, exatamente;
  * a lista ordenada de `escrita = chave:valor` (`-` para remoção).

### 3.10 `ponte.txt` — ponte (`spec/BRIDGE.md`)

* `compacto`: expandir `bits` (forma compacta) no `alvo` de 256 bits em big-endian, ou rejeitar (negativo, com estouro de 256 bits ou nulo).
* `cabecalho`: conferir o `hash` do cabeçalho de 80 bytes e se o trabalho é válido sob `limite`.
* `queima`: script `OP_RETURN` de queima para `rede` e `destinatario`.
* `merkle`: raiz a partir de `folha`, `caminho` e `indice`.

### 3.11 `versoes.txt` — atualizações (`spec/RELEASES.md`)

* `manifesto`: `release_id` do manifesto.
* `arquivo`: hash do arquivo.
* `arvore`: hash de árvore dos arquivos listados (`arquivo = caminho:conteúdo_hex`).

## 4. Execução

```bash
cargo run -p rz-conformance --bin zero-conformidade -- verificar conformance
```

A CI executa a suíte em todo commit. O teste `rz-conformance/tests/published.rs` verifica três coisas:

* os vetores publicados;
* vetores recém-gerados;
* que um vetor adulterado é detectado.

## 5. Geração

```bash
cargo run -p rz-conformance --bin zero-conformidade -- gerar DIR
```

O gerador recusa um cenário que não exercite o que promete. Ele também recusa uma rejeição causada por outra regra que não a pretendida.

Provas privadas usam aleatoriedade, então duas gerações produzem bytes diferentes. Ambas são válidas, mas só os vetores publicados valem como referência. Regenerar só faz sentido junto com uma mudança de protocolo.

## 6. Ligação com versões

O campo `tests` de um manifesto de versão (`spec/RELEASES.md §2`) é o hash de árvore de `conformance/`. Quem vota a adoção de uma versão sabe exatamente qual suíte ela precisa passar, e quem implementa sabe contra o que se comparar.

## 7. Limitações

* Consenso, P2P em rede e Wallet não têm vetores de interação nesta versão. São cobertos pelos testes da implementação de referência. Para o consenso, a interface coberta é a transição de estado por bloco.
* Os vetores foram produzidos pela implementação de referência. Uma segunda implementação independente é o que de fato testará a especificação (`SPEC §68`).
