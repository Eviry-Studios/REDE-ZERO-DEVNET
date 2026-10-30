# Guia do operador — TESTNET

**Versão:** 0.3.0
**Relacionamento:** `docs/TESTNET.md`, ADR-0011 (IP), ADR-0012 (consenso), ADR-0021 (versões), ADR-0023 (chaves), ADR-0024 (eclipse e corrupção)

Este guia é para quem vai operar um Node validador da TESTNET. O ZERO da TESTNET **não tem valor econômico**.

## 1. Requisitos

* Linux x86_64. A reprodutibilidade do build foi verificada só nessa plataforma.
* 2 núcleos, 4 GiB de memória e 20 GiB de disco.
* Conexão estável. A porta do Node é aberta, ou fica atrás de um serviço onion.
* Relógio sincronizado não é necessário: o protocolo mede o tempo em blocos.

## 2. Instalar a partir do código, conferindo os binários

```bash
git clone https://github.com/Eviry-Studios/REDE-ZERO-DEVNET.git rede-zero
cd rede-zero
git checkout v0.1.0-testnet
scripts/build-reproduzivel.sh
cat target/reproduzivel/artefatos.txt
```

O compilador é o de `rust-toolchain.toml`. Os hashes impressos têm de ser **iguais** aos publicados com a versão e aos do manifesto na cadeia (`zero-wallet release-verify`).

Se forem diferentes, não rode o binário e relate o caso por `SECURITY.md`.

```bash
export PATH="$PWD/target/reproduzivel/release:$PATH"
```

## 3. Chave do validador

Gere a chave **na própria máquina**:

```bash
zero-wallet new --key ~/rede-zero/validador.key
```

* A senha tem no mínimo 8 caracteres e é lida sem eco.
* O arquivo é cifrado com Argon2id + XChaCha20-Poly1305 e criado com permissão `0600`.
* A saída mostra a **chave pública**. Envie à coordenação só a chave pública e o endereço público do seu Node (§5).
* **Nunca envie o arquivo `.key`**, nem por mensagem e nem para "conferência".
* Faça uma cópia de segurança cifrada do arquivo fora da máquina. A senha deve ficar guardada separada do arquivo.

### Recuperação por guardiões (recomendado)

Depois do lançamento, registre guardiões: outras chaves suas guardadas em outro lugar, ou pessoas de confiança.

```bash
zero-wallet recovery-set --genesis genesis.bin --node 127.0.0.1:7100 --key validador.key \
    --guardians HEX1,HEX2,HEX3 --threshold 2 --delay-blocks 302400
```

O exemplo exige 2 de 3 guardiões e um prazo de cerca de 7 dias com blocos de 2 s. Durante o prazo, a sua chave atual pode cancelar a recuperação (`spec/KEYS.md §4`).

Chaves com vínculo de validador não giram. Para trocar a chave de um validador, desvincule, espere o período de desvinculação e vincule uma chave nova.

## 4. Conferir o Genesis

```bash
rede-zero-node genesis-info --genesis genesis.bin
```

Confira, por canais diferentes:

* o `genesis` (hash) com o publicado em `testnet/` e com pelo menos dois outros operadores;
* que a sua chave pública está na lista de validadores;
* que a política monetária é a da ADR-0025: 0,1 ZERO por bloco e 5 000 bps das taxas queimados.

## 5. Rodar o Node

### Endereço público direto

```bash
rede-zero-node run --genesis genesis.bin --data ~/rede-zero/dados \
    --listen 0.0.0.0:7100 --advertise SEU_IP:7100 \
    --peer NODE_INICIAL_1:7100 --peer NODE_INICIAL_2:7100 \
    --validator-key ~/rede-zero/validador.key --passphrase-file ~/rede-zero/senha
```

### Só por Tor (o IP do operador não aparece)

No `torrc`:

```text
HiddenServiceDir /var/lib/tor/rede-zero/
HiddenServicePort 7100 127.0.0.1:7100
```

Depois:

```bash
rede-zero-node run --genesis genesis.bin --data ~/rede-zero/dados \
    --listen 127.0.0.1:7100 --proxy 127.0.0.1:9050 \
    --advertise $(sudo cat /var/lib/tor/rede-zero/hostname):7100 \
    --peer NODE_INICIAL.onion:7100 \
    --validator-key ~/rede-zero/validador.key --passphrase-file ~/rede-zero/senha
```

**Pares iniciais:** use vários, de operadores diferentes. O Node diversifica os pares por faixa de rede e detecta estagnação (ADR-0024), mas começar com um único par facilita um eclipse.

**Pontos de verificação:** ao entrar depois do lançamento, use `--checkpoint ALTURA:ID` com um bloco recente. Obtenha o ID de mais de uma fonte em que confie.

### Vários validadores na mesma máquina (§5.1)

Cada validador precisa de **chave, porta e pasta de dados próprias**. Nunca use a mesma chave em dois Nodes: isso conta como voto duplo e é punido.

```bash
mkdir -p ~/rz && (umask 077; printf '%s\n' 'sua senha longa' > ~/rz/senha)
scripts/testnet-chaves.sh 7 ~/rz          # gera validador-1..7.key e ~/rz/publicas.txt
```

Envie à coordenação **só** o `publicas.txt`. Depois de receber o `genesis.bin`:

```bash
# VPS com IP público: escuta em 0.0.0.0 e anuncia IP:porta
ANUNCIAR=IP_DA_VPS scripts/testnet-nos.sh iniciar genesis.bin ~/rz 7100
# Aparelho atrás de roteador (PC, celular): só disca para a VPS
scripts/testnet-nos.sh iniciar genesis.bin ~/rz 7100 IP_DA_VPS:7100 IP_DA_VPS:7101

scripts/testnet-nos.sh estado ~/rz        # última altura de cada Node
scripts/testnet-nos.sh parar ~/rz
```

Na VPS, libere as portas usadas (no exemplo, 7100 a 7106) no firewall da máquina e no painel do provedor.

Os Nodes da mesma máquina chegam uns aos outros pelo mesmo IP. Com o limite padrão de 8 conexões de entrada por IP (`--max-inbound-per-ip`, ADR-0024), o validador da porta base fica cheio, e a Wallet local é recusada nele. Para a Wallet e o faucet, rode um Node **não validador** só local, sem anunciar endereço, e aponte `--node` para ele:

```bash
rede-zero-node run --genesis genesis.bin --data ~/rz/dados-local \
    --listen 127.0.0.1:7110 --peer 127.0.0.1:7100 --peer 127.0.0.1:7101
```

O que conta para a rede é o poder de voto **por aparelho**, não o número de processos: nenhum aparelho, ou grupo de aparelhos que caem juntos, deve passar de 1/3 sem que o restante tenha mais de 2/3 (`docs/TESTNET.md §1`).

### Serviço do sistema

`/etc/systemd/system/rede-zero.service`:

```ini
[Unit]
Description=Node da Rede Zero (TESTNET)
After=network-online.target tor.service

[Service]
User=rede-zero
ExecStart=/home/rede-zero/bin/rede-zero-node run --genesis /home/rede-zero/genesis.bin \
    --data /home/rede-zero/dados --listen 127.0.0.1:7100 --proxy 127.0.0.1:9050 \
    --advertise ENDERECO.onion:7100 --peer NODE_INICIAL.onion:7100 \
    --validator-key /home/rede-zero/validador.key --passphrase-file /home/rede-zero/senha
Restart=on-failure
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=/home/rede-zero/dados

[Install]
WantedBy=multi-user.target
```

O arquivo de senha fica com permissão `0600` e pertence ao usuário do serviço. Quem tem acesso de administrador à máquina consegue ler a senha, então proteja a máquina como protegeria a chave.

## 6. Acompanhar

```bash
zero-wallet status  --genesis genesis.bin --node 127.0.0.1:7100
zero-wallet balance --genesis genesis.bin --node 127.0.0.1:7100 --key validador.key
```

* A altura deve subir a cada `block_interval_ms`, cerca de 2 s.
* O saldo do validador cresce com a emissão e com as taxas dos blocos que ele propõe.
* Validador fora do ar não propõe e não recebe.
* Os registros não contêm endereços de pares. Use `--verbose` só para diagnóstico.

## 7. Incidentes

Na máquina de coordenação da TESTNET (10 validadores): [docs/testnet/RUNBOOK.md](testnet/RUNBOOK.md).

| Situação | O que fazer |
| --- | --- |
| O Node para de avançar | Confira os pares e a rede. O Node troca de par sozinho quando detecta estagnação |
| Disco corrompido | Reinicie: o Node corta o trecho corrompido e reconstrói pela rede (ADR-0024) |
| Chave do validador vazada | Desvincule imediatamente (`zero-wallet unbond`) e avise a coordenação; o vínculo fica retido durante a desvinculação |
| Dois Nodes com a mesma chave | **Nunca faça isso.** Votar duas vezes na mesma rodada queima 5 % do vínculo e exclui o validador |
| Atualização de versão | Só depois de adotada por proposta constitucional; confira o manifesto com `zero-wallet release-verify` e recompile pelo §2 |
| Vulnerabilidade encontrada | Relate por `SECURITY.md`, sem publicar antes da correção |

## 8. Celular com Termux

Ainda **não testado** pelo projeto: o build reproduzível foi verificado só em Linux x86_64. Num celular Android ARM, os binários ficam diferentes, o que é aceitável na TESTNET.

1. Instale o **Termux pelo F-Droid**; a versão da Play Store está desatualizada.
2. Instale as ferramentas e compile com uma tarefa por vez, para não esgotar a memória:

   ```bash
   pkg update && pkg install git rust
   git clone https://github.com/Eviry-Studios/REDE-ZERO-DEVNET.git rede-zero && cd rede-zero
   cargo build --release --locked -j 1 -p rz-node -p rz-wallet
   export PATH="$PWD/target/release:$PATH"
   ```

   O Termux usa o compilador do próprio pacote, e não o de `rust-toolchain.toml`. Se a compilação falhar por versão, relate o erro.
3. Para o Android não encerrar o Node:
   * rode `termux-wake-lock`, que deixa uma notificação fixa;
   * em Configurações → Apps → Termux → Bateria, escolha **Sem restrições**;
   * deixe o celular carregando, com Wi-Fi ou dados móveis estáveis.
4. Gere a chave e rode o Node como no §5.1, sem `ANUNCIAR`, discando para a VPS.
5. Opcional: o app **Termux:Boot** inicia o Node quando o celular liga.

O celular não precisa receber conexões, porque disca para a VPS. Uma queda do celular só tira o poder dele da rede.

## 9. Faucet

ZERO de teste para qualquer pessoa:

```bash
zero-wallet faucet --genesis genesis.bin --faucet ENDERECO_DO_FAUCET --key minha.key --proxy 127.0.0.1:9050
```

Cada endereço recebe 1 000 ZERO a cada 24 h. A prova de trabalho leva alguns segundos.

O faucet **não vincula ZERO e não vota**. Qualquer pessoa pode conferir isso na cadeia pelo endereço dele, publicado em `testnet/`.
