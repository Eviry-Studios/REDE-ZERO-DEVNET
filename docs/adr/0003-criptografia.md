# ADR-0003 — Ed25519 e BLAKE3

**Estado:** Aceita
**Escopo:** DEVNET
**Data:** 2026-09-29
**Relacionamento:** REQ-009, SPEC §3, SPEC §5, SPEC §9, THR-ID-002, THR-ID-003, THR-TX-004

## Contexto

`SPEC-ID-003`, `SPEC §5` e `SPEC §9` deixam em aberto a função de derivação do Node ID, os algoritmos de identificadores e o algoritmo de assinatura.

## Decisão

* **Assinaturas:** Ed25519 (RFC 8032), com **verificação estrita** (rejeita pontos de ordem pequena e codificações não canônicas de `R` e `s`), mitigando maleabilidade (THR-TX-004).
* **Hash:** BLAKE3 com saída de 32 bytes.
* **Separação de domínio:** todo hash e toda assinatura usam um contexto textual explícito e versionado, no formato `rede-zero/<objeto>/v<n>`, e toda mensagem assinada inclui o identificador da rede (THR-ID-003).
* **Node ID / endereço de Wallet:** `BLAKE3-derive-key(contexto, chave_pública)` com contextos distintos para Node e Wallet, de modo que a mesma chave não produza o mesmo identificador nos dois papéis.

Os detalhes normativos e vetores de teste estão em `spec/CRYPTOGRAPHY.md`.

## Alternativas consideradas

* **secp256k1 / ECDSA** — maleabilidade de assinatura exige cuidados extras; nonce determinístico é obrigatório para segurança.
* **SHA-256** — sólido e universal, porém sem modo nativo de derivação com contexto; BLAKE3 oferece separação de domínio embutida e melhor desempenho.
* **Assinaturas pós-quânticas** — ainda com tamanhos grandes e ecossistema imaturo; permanecem como evolução futura (`THREAT_MODEL.md §12`).

## Consequências

* Chaves públicas de 32 bytes, assinaturas de 64 bytes.
* Privacidade transacional (assinaturas em anel, provas de conhecimento zero etc.) **não** é resolvida por esta ADR.

## Condições de revisão

Adoção de modelo de privacidade transacional ou exigência de resistência pós-quântica.
