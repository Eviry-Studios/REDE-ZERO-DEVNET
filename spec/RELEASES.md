# spec/RELEASES.md — Atualizações verificáveis

**Versão:** 0.1.0 (DEVNET)
**Decisão:** ADR-0021
**Relacionamento:** REQ-092..095, `SPECIFICATIONS.md §63–§65`, `ARCHITECTURE.md §42–§44`, `spec/GOVERNANCE.md §3`, AT-UPD-001..004, `ACCEPTANCE_TESTS.md §75–§76`, THR-DEV-002/003/008/009
**Implementação de referência:** `crates/rz-core/src/release.rs`, `crates/rz-core/src/state.rs`, `crates/rz-node`, `crates/rz-wallet`

---

## 1. Princípio

Uma atualização não acontece porque alguém publicou código novo (`ARCHITECTURE.md §42`). Na Rede Zero:

* qualquer pessoa **publica** o manifesto de uma versão na cadeia;
* só a **governança adota** uma versão, por proposta constitucional aprovada nas duas câmaras;
* qualquer pessoa **verifica** um binário, a especificação ou o código-fonte contra o manifesto adotado.

Não existe chave de mantenedor que assine releases. A autenticidade de uma versão (`SPEC §64`) vem da sua adoção registrada numa cadeia finalizada por mais de 2/3 do poder de voto. Um atacante que controle o repositório, o site ou o canal de distribuição não consegue fazer um binário adulterado passar na verificação (THR-DEV-002, `ACCEPTANCE_TESTS.md §75`).

## 2. Manifesto

```text
Release {
  version:          str (≤ 32; [0-9a-z.-]+)
  protocol_version: u16
  p2p_version:      u16
  compatibility:    u8  (0 COMPATIBLE | 1 PARTIALLY_COMPATIBLE | 2 INCOMPATIBLE)
  previous:         option<fixed[32]>     versão adotada que esta sucede
  specification:    fixed[32]             árvore de spec/ e docs/
  implementation:   fixed[32]             árvore de crates/, Cargo.toml, Cargo.lock
  tests:            fixed[32]             árvore de conformance/ (spec/CONFORMANCE.md)
  artifacts:        list<Artifact> (≤ 32), nomes em ordem estritamente crescente
  notes:            fixed[32]             hash das notas da versão (texto fora da cadeia)
}
Artifact { name: str (≤ 64; [A-Za-z0-9._-]+), hash: fixed[32] }

release_id = H("rede-zero/release/v1", encode(Release))
```

Cada campo exigido por `SPEC §63` está no manifesto: versão (`version`, `protocol_version`, `p2p_version`), especificação, implementação, testes e o identificador (`release_id`). Um manifesto que não cumpre as regras acima é rejeitado na decodificação.

## 3. Hashes de arquivos e árvores

```text
file_hash(bytes) = H("rede-zero/release-file/v1", bytes)
tree_hash(files) = H("rede-zero/release-tree/v1",
                     u64(n) ‖ Σ_{em ordem de caminho} ( str(caminho) ‖ file_hash(conteúdo) ))
```

* Caminhos são relativos à raiz do repositório e usam `/`.
* Diretórios são percorridos recursivamente.
* Entradas cujo nome começa com `.` são ignoradas, assim como links simbólicos.

Por isso o hash não depende do sistema de arquivos nem da ordem de leitura. A Wallet calcula esses hashes com `release-new` e confere com `release-verify --root`.

## 4. Publicação e adoção

| Tag | Operação | Campos | Regras |
| --- | --- | --- | --- |
| `0x80` | `PublishRelease` | `release: Release` | manifesto válido; ainda não publicado; registro com menos de 256 versões; queima `gov_deposit` |

A adoção usa a proposta já existente (`spec/GOVERNANCE.md §3`, tag `0x12`) com `release_id` preenchido. A proposta só é aceita se:

1. a categoria for **constitucional** (limiar ≥ 2/3, quórum de 20%, atraso de ≥ 30 dias);
2. a versão estiver publicada e ainda não adotada;
3. `previous` for igual à versão vigente (nenhuma, antes da primeira adoção);
4. `protocol_version` não for menor que o da versão vigente (sem downgrade, `ACCEPTANCE_TESTS.md §76`).

Na ativação, a versão é adotada (`adopted_at = h`) e passa a ser a vigente. As alterações de parâmetros da mesma proposta valem junto. Se, entre a submissão e a ativação, outra versão tiver sido adotada, `previous` deixa de suceder a vigente. A proposta fica então `ActivationFailed`, e nada muda.

O custo de publicação é **queimado**, como o depósito de propostas sem quórum, para que encher o registro custe caro. Uma versão não adotada sai do registro depois de `analysis + voting + constitutional_delay` blocos contados da publicação, se nenhuma proposta pendente ou aprovada a referenciar. Versões adotadas ficam para sempre, como histórico verificável (REQ-094).

## 5. Compatibilidade (`SPEC §65`)

| Categoria | Significado | Efeito no Node |
| --- | --- | --- |
| `COMPATIBLE` | sem mudança de regras nem de mensagens | atualização recomendada |
| `PARTIALLY_COMPATIBLE` | acrescenta mensagens ou comportamento opcional; versões anteriores continuam validando a cadeia | atualização recomendada |
| `INCOMPATIBLE` | muda regras de validade | atualização **obrigatória** antes que as novas regras entrem em vigor |

Uma versão que aumente `protocol_version` é tratada como incompatível, qualquer que seja a categoria declarada.

## 6. Aviso no Node

O Node compara a própria versão (`CARGO_PKG_VERSION`, `PROTOCOL_VERSION`) com a versão vigente:

* `UpToDate`: a mesma versão, ou nenhuma adotada;
* `Recommended`: há versão adotada mais nova e compatível;
* `Required`: a versão adotada é incompatível.

O aviso aparece no registro ao iniciar e quando a cadeia adota uma versão, e fica em `NodeStatus.upgrade`. O Node **nunca se atualiza sozinho**: quem opera decide, verifica e instala (AT-UPD-003). Nesta versão, a adoção registra a versão sem mudar regras de validação por si só. Uma mudança de regras exige o novo código, com ativação na altura que ele definir.

## 7. Verificação

```text
zero-wallet release-verify --genesis G --node N --artifact rede-zero-node --file ./rede-zero-node
zero-wallet release-verify --manifest manifesto.bin --root ./REDE-ZERO-DEVNET
```

* **Artefato:** `file_hash` do arquivo baixado comparado ao do manifesto adotado (AT-UPD-002: um artefato alterado não confere).
* **Código e especificação:** `tree_hash` de um clone, comparado a `implementation`, `specification` e `tests`.
* **Não adotada:** uma versão só publicada é sinalizada como não adotada (AT-UPD-004).

## 8. Mensagens P2P

| Tag | Mensagem | Campos |
| --- | --- | --- |
| `0x35` | `GET_RELEASES` | — |
| `0x36` | `RELEASES` | `height: u64, current: option<fixed[32]>, releases: list<ReleaseRecord> (≤ 256)` |

`ReleaseRecord { release: Release, published_at: u64, adopted_at: option<u64> }`.

## 9. Estado

A parte de versões entra na raiz do estado depois da reputação:

```text
releases_root = H("rede-zero/releases-root/v1",
                  u64(n) ‖ Σ_{em ordem de id} (id ‖ ReleaseRecord) ‖ option(current))
```

## 10. Limitações

* **Builds reproduzíveis** (THR-DEV-003, REQ-004): `scripts/build-reproduzivel.sh` produz os binários com o compilador fixado em `rust-toolchain.toml`, o `Cargo.lock` e os caminhos locais substituídos por prefixos fixos. Com o mesmo commit, qualquer pessoa obtém os mesmos bytes, em qualquer diretório. A CI compila duas vezes em caminhos diferentes e compara (job "build reproduzível"). Assim, o hash de artefato do manifesto pode ser conferido por terceiros que compilam, e quem não compila confia em quem compilou e conferiu. Ainda não há verificação entre sistemas operacionais e arquiteturas diferentes de Linux x86_64.
* A adoção não troca regras automaticamente. A coordenação de ativação de regras novas fica no código de cada versão.

## 11. Testes

| Teste | Verificação |
| --- | --- |
| `release::tests::manifest_identity_and_rules` | identificador muda com qualquer artefato; ordem e nomes validados |
| `release::tests::tree_hash_is_order_independent_and_content_sensitive` | hash de árvore determinístico |
| `release::tests::upgrade_notices` | categorias de compatibilidade |
| `release_published_then_adopted_by_constitutional_proposal` | publicar não adota; só constitucional; sucessão; sem downgrade (AT-UPD-001/004, §76) |
| `competing_release_fails_at_activation` | versões concorrentes não se sobrepõem |
| `unadopted_release_expires` | registro não cresce sem limite |
| `rz-wallet release_devnet` | Nodes reais: publicação, adoção, aviso `Required`, verificação de artefato (AT-UPD-002/003) |
