# ADR-0021 — Atualizações verificáveis adotadas pela governança

**Estado:** Aceita (DEVNET)
**Escopo:** DEVNET → TESTNET
**Data:** 2026-10-13
**Relacionamento:** REQ-092..095, `SPECIFICATIONS.md §63–§65`, `ARCHITECTURE.md §42–§44`, ADR-0008 (governança), AT-UPD-001..004, THR-DEV-002/003/008/009
**Especificação:** [`spec/RELEASES.md`](../../spec/RELEASES.md)
**Implementação:** `crates/rz-core/src/release.rs`, `crates/rz-core/src/state.rs`, `crates/rz-node`, `crates/rz-wallet`

## Contexto

`SPEC §63` exige que uma atualização tenha versão, especificação, implementação, testes e identificador. `SPEC §64` pede que qualquer participante verifique autenticidade e compatibilidade, mas o mecanismo estava A DEFINIR. `SPEC §65` lista três categorias de compatibilidade, sem efeitos definidos.

`ARCHITECTURE.md §43` proíbe que uma única entidade force uma alteração de consenso.

A governança já aceitava `release_id` em propostas, sem nada que o ligasse a um conteúdo verificável.

## Decisão

1. **Manifesto na cadeia.** Uma versão é um manifesto canônico com:
   * versões de software, protocolo e P2P;
   * categoria de compatibilidade;
   * versão anterior;
   * hashes de árvore da especificação, do código e da suíte de conformidade;
   * hashes dos artefatos.

   `release_id` é o hash do manifesto. Qualquer pessoa publica um manifesto (tag `0x80`), pagando com a queima de um depósito de proposta.
2. **A governança é quem assina.** Só uma proposta **constitucional** aprovada adota uma versão. Ela precisa apontar para um manifesto publicado que suceda a versão vigente e que não reduza a versão de protocolo. A adoção fica registrada no estado finalizado. Não existe chave de release de mantenedor.
3. **Compatibilidade com efeito definido.**
   * `COMPATIBLE` e `PARTIALLY_COMPATIBLE` geram aviso de atualização recomendada.
   * `INCOMPATIBLE`, ou uma versão de protocolo maior, gera aviso de atualização obrigatória.
   * O Node nunca se atualiza sozinho.
4. **Verificação local.** A Wallet calcula e confere hashes de arquivos e de árvores (`release-new`, `release-verify`). O Node expõe a versão vigente por P2P e avisa quem opera.

## Alternativas consideradas

| Alternativa | Por que não |
| --- | --- |
| Releases assinadas por chave(s) de mantenedores | Uma entidade (ou um pequeno grupo) passa a decidir o que é a "versão oficial" (`ARCH §43`, THR-DEV-002, THR-DEV-005) |
| Limiar de assinaturas de validadores fora da governança | Dá a validadores um poder que a governança bicameral reparte com a câmara econômica |
| Atualização automática ao adotar | Um erro ou uma captura da governança se propagaria sem intervenção humana; cada operador precisa decidir |
| Só o hash na proposta, sem manifesto publicado | Ninguém saberia, a partir da cadeia, o que foi aprovado nem como verificar |
| Publicação gratuita | O registro limitado poderia ser lotado por spam |

## Consequências

* Quem baixa um binário confere contra a cadeia, não contra o site ou o repositório. Comprometer a distribuição não basta para enganar quem verifica (`ACCEPTANCE_TESTS.md §75`).
* Adotar uma versão leva o tempo de uma proposta constitucional (análise, votação e pelo menos 30 dias de atraso). Correções de segurança urgentes ainda dependem de operadores atualizarem por conta própria antes da adoção formal. A adoção registra o que foi aceito, não impede que alguém rode outro código.
* A garantia para artefatos depende de builds reproduzíveis (THR-DEV-003): `scripts/build-reproduzivel.sh`, com o compilador fixado, verificado na CI em Linux x86_64.

## Condições de revisão

* Verificação de reprodutibilidade em outras plataformas e por compiladores independentes.
* Processo de emergência para correções de segurança, sem criar autoridade única.
* Ativação de regras novas coordenada pela altura de adoção.
