# ADR-0023 — Chaves cifradas em disco, rotação e recuperação de conta

**Estado:** Aceita (DEVNET)
**Escopo:** DEVNET → TESTNET
**Data:** 2026-10-15
**Relacionamento:** REQ-010..012, REQ-046, SPEC-ID-001, SPEC-ID-005, GOV-INV-5, THR-ID-001, THR-ID-005, ADR-0005 (modelo de contas)
**Especificação:** [`spec/KEYS.md`](../../spec/KEYS.md)
**Implementação:** `crates/rz-crypto/src/keyfile.rs`, `crates/rz-core/src/rotation.rs`, `crates/rz-core/src/state.rs`, `crates/rz-node`, `crates/rz-wallet`

## Contexto

* SPEC-ID-001 diz que a chave privada não deve ficar em texto aberto quando houver alternativa segura. Na DEVNET, a semente ficava em hexadecimal num arquivo `0600`.
* SPEC-ID-005 exige um mecanismo para substituir chaves comprometidas ou inutilizadas, e o procedimento estava A DEFINIR.
* O endereço de conta é o hash da chave (ADR-0005). Trocar de chave significava abandonar o endereço, junto com bloqueios, nomes, ordens e histórico.

## Decisão

1. **Arquivo cifrado por padrão.** A chave é derivada da senha por Argon2id (64 MiB, 3 passadas) e cifrada com XChaCha20-Poly1305; o cabeçalho é autenticado. Os arquivos legados continuam legíveis e podem ser convertidos no lugar.
2. **Endereço estável, chave substituível.** O estado guarda a chave autorizada de cada conta que girou. `RotateKey` é assinada pela chave atual e traz a prova de posse da nova chave, ligada à conta. A chave antiga deixa de assinar no mesmo bloco.
3. **Recuperação com prazo contestável.** A conta registra uma chave de recuperação e um prazo. A chave de recuperação agenda a troca, que a chave atual pode cancelar até o prazo vencer.
4. **Só migra o que já é transferível.** Pontos de contribuição, vínculos de validador, credenciais e chaves de Comunidades continuam presos à chave. Notas privadas pertencem à chave de visualização.

## Alternativas consideradas

| Alternativa | Por que não |
| --- | --- |
| Rotação = transferir tudo para o endereço da nova chave | Exige migrar bloqueios, nomes, ordens, HTLCs e o resto, área por área, e cada omissão vira perda de fundos |
| Levar pontos de contribuição na rotação | Permitiria vendê-los girando para a chave do comprador (GOV-INV-5) |
| Recuperação imediata pela chave de recuperação | Quem roubasse a chave de recuperação tomaria a conta sem chance de reação |
| Recuperação social (guardiões por limiar) | Mais complexa; pode ser construída depois sobre o mesmo mecanismo, com uma Comunidade como recuperação |
| Cifrar também a chave de identidade do Node | Impede o reinício sem intervenção, e essa chave não move fundos |

## Consequências

* A senha é lida do terminal sem eco (crate `rpassword`); arquivo de senha e variável de ambiente cobrem serviços sem terminal.
* Uma chave vazada deixa de ser uma perda definitiva: a pessoa gira a chave, e com isso dispensa o prazo de recuperação, desde que chegue antes do ladrão.
* A Wallet precisa consultar o Node para saber por qual conta a chave assina (P2P `GET_KEY`).
* O endereço de origem de uma chave delegada não é controlado por ninguém, então transferências para ele são recusadas.
* Validadores trocam de chave desvinculando e vinculando de novo, e recomeçam os pontos de contribuição.

## Condições de revisão

* Recuperação por limiar de guardiões.
* Carteiras de hardware.
