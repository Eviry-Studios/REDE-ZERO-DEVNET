# Contribuindo com a Rede Zero

Obrigado pelo interesse. A Rede Zero é projetada para **não depender de nenhuma pessoa indispensável** (`REQ-096`–`REQ-099`), por isso contribuições externas são parte do próprio projeto (`REQ-076`).

## Antes de começar

Leia, nesta ordem:

1. [`docs/Manifesto_Exonet.pdf`](docs/Manifesto_Exonet.pdf) — por quê;
2. [`docs/REQUIREMENTS.md`](docs/REQUIREMENTS.md) — o que deve existir;
3. [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md) — contra o quê;
4. [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — como os componentes se organizam;
5. [`docs/SPECIFICATIONS.md`](docs/SPECIFICATIONS.md) e [`spec/`](spec/) — regras técnicas;
6. [`docs/adr/`](docs/adr/) — decisões já tomadas e seus escopos.

## Hierarquia documental

```text
Manifesto → Requirements → Threat Model → Architecture → Specifications → Implementação
```

O código **implementa** a especificação; não a substitui. Se o código e a especificação divergirem, abra uma issue: ou o código está errado, ou a especificação precisa passar por alteração formal.

## Tipos de contribuição

* **Documentação e especificação** — correções, esclarecimentos, vetores de teste.
* **Código** — implementação dos componentes em `crates/`.
* **Testes** — novos testes de aceitação (`AT-*`), testes de propriedade, fuzzing.
* **Decisões técnicas** — proponha uma nova ADR em `docs/adr/` usando `template.md`.
* **Segurança** — veja [`SECURITY.md`](SECURITY.md).

## Fluxo

1. Abra uma issue descrevendo o problema ou a proposta (exceto correções triviais).
2. Crie um branch a partir de `main`.
3. Faça commits pequenos e com mensagens descritivas.
4. Rode as verificações locais:

   ```bash
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```

5. Abra um Pull Request referenciando os identificadores afetados (`REQ-*`, `SPEC *`, `THR-*`, `AT-*`).
6. Toda alteração passa por revisão (`REQ-077`).

## Regras do código

* `#![forbid(unsafe_code)]` em todos os crates.
* Nada de `unwrap()`/`expect()`/`panic!` sobre dados vindos da rede ou do disco.
* Nada de ponto flutuante, relógio local ou aleatoriedade dentro da função de transição de estado.
* Toda aritmética monetária usa operações verificadas (`checked_*`).
* Novas dependências exigem justificativa na ADR correspondente (`REQ-084`).
* Testes de aceitação devem citar o identificador do documento (`AT-TX-003`, por exemplo) no nome ou comentário do teste.

## Alterações protocolares

Mudanças que alterem regras de consenso, formato de dados, regras monetárias ou identificadores devem:

* atualizar a especificação correspondente em `spec/`;
* atualizar vetores de teste;
* declarar compatibilidade (`SPEC §65`);
* ser registradas em `CHANGELOG.md`.

## Licença

Ao contribuir, você concorda que sua contribuição será distribuída sob a [licença MIT](LICENSE).

## Conduta

Discorde de ideias, não de pessoas. Críticas técnicas são bem-vindas; ataques pessoais não.
