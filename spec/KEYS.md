# spec/KEYS.md — Guarda, rotação e recuperação de chaves

**Versão:** 0.2.0 (DEVNET)
**Decisão:** ADR-0023
**Relacionamento:** REQ-010..012, REQ-046, `SPECIFICATIONS.md` SPEC-ID-001, SPEC-ID-005, SPEC-WAL-002/003, `spec/GOVERNANCE.md §7` (GOV-INV-5), THR-ID-001, THR-ID-005
**Implementação de referência:** `crates/rz-crypto/src/keyfile.rs`, `crates/rz-core/src/rotation.rs`, `crates/rz-core/src/state.rs`, `crates/rz-wallet`

---

## 1. Arquivo de chave cifrado (REQ-012, SPEC-ID-001)

A chave privada não fica em texto aberto quando há alternativa segura. O arquivo cifrado é texto:

```text
rede-zero-chave-cifrada v1
kdf argon2id m=<KiB> t=<passadas> p=<vias>
sal <16 bytes hex>
nonce <24 bytes hex>
cifra <XChaCha20-Poly1305(semente de 32 bytes) com etiqueta, hex>
```

* `chave = Argon2id(senha, sal, m, t, p)` gera 32 bytes. O padrão é `m = 65 536` (64 MiB), `t = 3` e `p = 1` (RFC 9106 §4).
* A cifragem é XChaCha20-Poly1305. Os dados associados são as três primeiras linhas, em forma canônica, então trocar parâmetros ou sal invalida a etiqueta.
* Na leitura, parâmetros fora de `8 MiB ≤ m ≤ 4 GiB`, `1 ≤ t ≤ 64` e `1 ≤ p ≤ 16` são recusados antes de derivar. Assim, um arquivo malicioso não esgota memória nem tempo.
* Senha errada e arquivo adulterado dão o **mesmo** erro.
* A senha tem no mínimo 8 caracteres. A semente e a senha são apagadas da memória depois do uso.

**Formato legado:** uma linha com a semente em hexadecimal, sem cifragem. Continua legível. `zero-wallet key-encrypt` converte o arquivo no lugar, sobrescreve o conteúdo em aberto e confere a leitura antes de substituir. Chaves novas da Wallet são cifradas por padrão; `--plaintext` só existe para a DEVNET.

**Senha:** a Wallet, o Node (`--validator-key`) e o Navegador a obtêm de três fontes, nesta ordem:

1. `--passphrase-file ARQUIVO` (primeira linha);
2. a variável `RZ_KEY_PASSPHRASE`;
3. o terminal, **sem eco**; sem terminal (serviços), uma linha da entrada padrão.

Arquivos são criados com permissão `0600` e nunca sobrescrevem um arquivo existente.

A chave de identidade do Node (`node.key`, só identidade P2P, sem fundos) continua em aberto para permitir reinício sem intervenção.

## 2. Conta e chave

O endereço de uma conta é o hash da sua primeira chave (`spec/CRYPTOGRAPHY.md §4`) e **não muda**. O estado guarda, por conta, a chave hoje autorizada, quando ela difere da de origem:

```text
status(chave):
  se chave ∈ delegadas            → assina pela conta delegadas[chave]
  senão, a = H(chave):
    se autorizada[a] existe e ≠ chave → SUBSTITUÍDA (não assina por ninguém)
    senão                             → assina pela conta a
```

Uma transação de conta é executada sobre a conta indicada por `status(remetente)`. Uma chave substituída é recusada (`KeyReplaced`) no mempool e nos blocos. O nonce é o da conta, então nenhuma transação antiga pode ser repetida com a nova chave.

## 3. Rotação

| Tag | Operação | Assinada por | Campos |
| --- | --- | --- | --- |
| `0x15` | `RotateKey` | chave atual | `new_key: PublicKey, proof: Signature` |
| `0x16` | `SetRecovery` | chave atual | `recovery: option<{ guardians: list<PublicKey> (1..16), threshold: u8, delay: u64 }>` |
| `0x17` | `RecoverAccount` | qualquer conta (paga a taxa) | `account: Address, new_key: PublicKey, proof: Signature, approvals: list<Approval> (≤16)` |

`proof` é a assinatura da **nova** chave com o contexto `rede-zero/key-rotation/v1` sobre `encode(account ‖ new_key)`. Ela prova posse da chave e o consentimento em controlar aquela conta. Sem ela, alguém poderia girar para uma chave que ninguém controla, ou para a de outra pessoa.

Regras da nova chave:

1. ser diferente da chave atual da conta;
2. não controlar outra conta;
3. ter o endereço de origem sem uso (sem conta, sem rotação, sem vínculo de validador). A exceção é voltar para a chave de origem da própria conta.

A chave atual não pode ter vínculo de validador (§5).

Efeito imediato de `RotateKey`:

* a nova chave passa a assinar pela conta, e a anterior deixa de assinar;
* uma chave delegada que é substituída fica livre e volta a controlar apenas o próprio endereço;
* uma recuperação agendada é cancelada.

O endereço de origem de uma chave delegada a outra conta não é controlado por ninguém. Por isso, transferências para ele são recusadas.

## 4. Recuperação de chave perdida (guardiões por limiar)

1. Enquanto tem a chave, a pessoa registra com `SetRecovery`:
   * de 1 a 16 **guardiões**, que podem ser chaves próprias guardadas em outro lugar, pessoas de confiança ou chaves de uma Comunidade;
   * o **limiar** `k`, de 1 até o número de guardiões;
   * o **prazo**, de 10 a 10 000 000 blocos.

   Uma chave de recuperação única é o caso 1-de-1. A própria chave atual não pode ser guardiã.
2. Se perder a chave, reúne a aprovação de `k` guardiões distintos. Cada um assina, fora da cadeia, com o contexto `rede-zero/key-recovery/v1`:

   ```text
   recovery_payload = encode(conta ‖ nova_chave ‖ u64(contador))
   ```

   O `contador` é o de recuperação da conta (consulta `GET_RECOVERY`). Ele muda a cada `SetRecovery`, a cada agendamento e a cada rotação, então aprovações de uma recuperação cancelada **não valem de novo**.
3. Qualquer conta submete `RecoverAccount` com as aprovações e paga a taxa. A troca fica **agendada** para `h + prazo`.
4. Durante o prazo, a chave atual cancela a troca com qualquer `SetRecovery` ou `RotateKey`. É a defesa contra guardiões coniventes ou roubados. `zero-wallet key-info` mostra a recuperação agendada.
5. No fim do bloco da altura agendada, a troca é aplicada se as regras do §3 ainda valerem. Se não valerem, é descartada.

Pagar a taxa não basta para tomar uma conta: são precisos `k` guardiões escolhidos pela própria dona, e a espera de um prazo que ela escolheu e pode interromper.

Na Wallet:

* `recovery-set --guardians HEX,... --threshold K --delay-blocks N`;
* cada guardião roda `recovery-approve --account HEX --new-public-key HEX` e entrega a aprovação em hexadecimal;
* quem recupera roda `recover --account HEX --new-key ARQUIVO --approvals HEX,...`.

## 5. O que não migra

* **Pontos de contribuição** e o peso na câmara de contribuição estão ligados à chave pública (REQ-046), e não são transferíveis (GOV-INV-5). Uma rotação que os levasse permitiria vendê-los: bastaria girar para a chave do comprador.
* **Vínculo de validador:** pertence ao endereço da própria chave. Chaves com vínculo não giram; o validador desvincula, espera o período de desvinculação e vincula uma chave nova. Chaves delegadas não se vinculam.
* **Credenciais de defesa e chaves de regras de Comunidades:** são identidades. A troca é feita pelos mecanismos próprios: revogação e concessão atestadas (`spec/DEFENSE.md`), e `UpdateCommunity` (`spec/COMMUNITIES.md`).
* **Notas privadas:** pertencem à chave de visualização derivada da semente. Antes de descartar a chave antiga, envie as notas ao endereço `zs…` da nova chave. Se a chave vazou, quem a tem também vê e gasta as notas, e a rotação da conta transparente não protege as notas.

O que migra, por continuar no mesmo endereço, é tudo o que já é transferível: saldo, ativos, bloqueios, nomes, ordens e trocas atômicas.

## 6. Consulta P2P

| Tag | Mensagem | Campos |
| --- | --- | --- |
| `0x37` | `GET_KEY` | `PublicKey key` |
| `0x38` | `KEY` | `u64 height, PublicKey key, option<Address> account, option<PublicKey> current, option<Recovery>, option<PendingRecovery>` |

| `0x39` | `GET_RECOVERY` | `Address account` |
| `0x3a` | `RECOVERY` | `u64 height, Address account, option<PublicKey> current, option<Recovery>, u64 nonce, option<PendingRecovery>` |

`account` é `None` para uma chave substituída. A Wallet usa `GET_KEY` para saber por qual conta a chave assina (nonce, saldo, bloqueios), e `GET_RECOVERY`, pelo endereço, para recuperar uma conta cuja chave se perdeu.

## 7. Estado

```text
keys_root = H("rede-zero/keys-root/v1",
              u64(n) ‖ Σ (conta ‖ chave_autorizada) ‖
              u64(n) ‖ Σ (conta ‖ recuperação) ‖
              u64(n) ‖ Σ (conta ‖ recuperação_agendada) ‖
              u64(n) ‖ Σ (conta ‖ u64(contador_de_recuperação)))
```

Ela entra na raiz do estado depois de `releases_root` (`spec/STATE.md §3`).

## 8. Testes

| Teste | Verificação |
| --- | --- |
| `keyfile::tests::*` | cifra e decifra, senha errada, adulteração do cabeçalho, parâmetros absurdos, conversão no lugar |
| `rotation::tests::rotation_chain_and_return_to_origin` | encadeamento de rotações e volta à chave de origem |
| `key_rotation_keeps_account_and_disables_old_key` | prova de posse, chave em uso, chave antiga recusada, endereço de origem protegido, delegada não se vincula |
| `recovery_after_delay_and_cancellation` | só a aprovação da guardiã agenda; a chave atual cancela; aprovações antigas não valem de novo; a troca só vale após o prazo |
| `guardian_threshold_recovery` | 2 de 3 guardiões; um só não basta; aprovação repetida não conta duas vezes |
| `bonded_key_cannot_rotate` | vínculo de validador preso à chave |
| `rz-wallet rotation_devnet` | Nodes reais: rotação, consulta `KEY`, chave antiga recusada, nova chave movendo a mesma conta |
| `conformance/cadeia.txt` | rotação, recuperação registrada e bloco com chave substituída rejeitado |
