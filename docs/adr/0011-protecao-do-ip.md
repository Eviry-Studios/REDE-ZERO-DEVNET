# ADR-0011 — Proteção do endereço IP do usuário

**Estado:** Aceita
**Escopo:** DEVNET
**Data:** 2026-09-29
**Relacionamento:** REQ-023, REQ-025, REQ-026, REQ-028, REQ-089, `ARCHITECTURE.md §19`, `SPECIFICATIONS.md §30`, THR-PRIV-002, THR-PRIV-004, THR-P2P-005
**Complementa:** ADR-0007 (P2P), ADR-0010 (Dandelion++)

## Contexto

Com Dandelion++ (ADR-0010), a rede não consegue apontar qual Node originou uma transação. Mas a Wallet se conecta a um Node por TCP, e isso deixava três pontos expostos:

1. **O operador do Node** vê o IP de quem se conecta. O software de referência não grava nem repassa esse IP, mas o código é aberto e qualquer operador pode modificá-lo para registrar tudo. A proteção não pode depender da honestidade do operador (TM-P-001).
2. **Quem observa a rede** (provedor, rede Wi-Fi, observador de backbone) via a conexão e, como o canal era aberto, também o conteúdo, inclusive o envio de uma transação.
3. **Metadados** de tamanho de mensagem permitiam distinguir tipos de mensagem mesmo sem ler o conteúdo.

## Decisão

Defesa em camadas, todas compatíveis com os documentos normativos:

### 1. Canal cifrado e autenticado (THR-P2P-005, SPEC §30)

* Handshake com chaves efêmeras X25519, novas a cada conexão.
* ChaCha20-Poly1305 com contador de nonce por direção e chaves distintas por sentido.
* O Node que aceita a conexão prova sua identidade assinando o handshake com a chave de Node, vinculada ao identificador da rede. Quem inicia (Wallet ou Node) permanece **anônimo**: não possui chave persistente.
* A Wallet pode **fixar** a identidade esperada (`--node-id`) para impedir interceptação ativa.
* **Preenchimento** de toda mensagem até múltiplos de 256 bytes (REQ-025): um `PING` e uma transação pequena produzem quadros do mesmo tamanho.

Efeito: um observador de rede vê apenas bytes cifrados de tamanho arredondado.

### 2. Tor / I2P via SOCKS5 (ARCHITECTURE §19)

* Wallet e Node aceitam `--proxy` (SOCKS5, ex.: Tor em `127.0.0.1:9050`).
* Endereços de par podem ser **nomes**, inclusive `nome.onion:porta`. Nomes são enviados ao proxy sem resolução local: nenhuma consulta DNS sai do dispositivo.
* Um Node pode operar como **serviço onion** (`--listen 127.0.0.1:P` + `--advertise nome.onion:P`), anunciando o endereço onion em vez do IP.
* Atrás de um serviço onion, todas as conexões chegam do proxy local. Por isso, banir o IP de loopback isolaria todos os pares onion: nesse caso, um par mal-comportado é apenas desconectado.

Com proxy, o Node vê o endereço de saída do Tor, não o IP do usuário; com serviço onion, nem o Node nem a rede conhecem o IP do outro lado.

O uso é **opcional**: a rede funciona sem Tor (REQ-089, nenhuma dependência crítica única), e o Tor **não** é apresentado como anonimato absoluto (REQ-028, ARCHITECTURE §19).

### 3. Node privado (`--no-listen`)

Um usuário pode rodar o próprio Node sem aceitar conexões. Ele disca para a rede, recebe blocos e transações (o `HELLO` passou a ter o campo `relay`), mas:

* nunca é anunciado a outros pares, pois não tem endereço de escuta;
* nunca revela sua identidade de Node, pois só quem aceita conexões assina o handshake;
* sua Wallet conecta em `127.0.0.1`, e o IP do usuário não chega a nenhum Node de terceiro como origem de Wallet;
* ao ser escolhido como relay de haste pelos pares, também encaminha transações alheias, o que dá negação plausível sobre a origem das suas.

### 4. Privacidade por padrão na Wallet (REQ-023)

Sem proxy, a Wallet **recusa** conectar a Node remoto. Ela exige uma de três escolhas explícitas:

* Node local (`127.0.0.1`);
* `--proxy` (Tor/I2P);
* `--direct`, aceitando conscientemente expor o IP.

### 5. Registros locais (THR-PRIV-004)

O Node não registra endereços de clientes em nenhum nível de log. Endereços de pares Node (infraestrutura pública) aparecem apenas em modo detalhado.

## O que o protocolo já garantia e continua garantindo

* O `HELLO` de uma Wallet não contém identificadores: versão, rede, Genesis, altura 0, porta 0.
* Wallets nunca aparecem em listas de pares (`PEERS`).
* A Wallet baixa todas as saídas e imagens de chave: o Node não aprende quais notas pertencem a quem.
* Transações não carregam IP, horário local ou qualquer metadado de origem.

## Alternativas consideradas

| Alternativa | Decisão |
| --- | --- |
| Tor obrigatório | Rejeitada: criaria dependência crítica única (REQ-089) |
| Mixnet própria com tráfego de cobertura | Adiada: custo alto; evolução futura para observador global |
| Noise Protocol com identidade mútua | Rejeitada para clientes: uma chave persistente da Wallet permitiria rastrear o usuário entre conexões |
| Consultas privadas por PIR | Desnecessária agora: a Wallet já baixa todos os dados |

## Limitações honestas (REQ-028)

* **Sem Tor, o primeiro Node ainda vê o IP da conexão** (se o usuário escolher `--direct`). Com Node próprio, quem vê é o próprio usuário.
* O Node privado expõe seu IP aos pares a que se conecta, como qualquer participante P2P. Ele não é distinguível, por esses pares, de outros Nodes que também encaminham transações.
* Os bytes mágicos do handshake identificam o protocolo para um observador de rede. Dentro do Tor, isso fica oculto.
* Um observador global passivo que correlacione todo o tráfego (THREAT_MODEL ADV-05) não é derrotado por estas medidas.
* A identidade de um Node que aceita conexões é estável e permite reconhecê-lo entre mudanças de IP. Nodes que não querem isso podem usar `--no-listen`.

## Condições de revisão

Auditoria do canal cifrado; avaliação de tráfego de cobertura; suporte nativo a I2P (SAM); resistência a análise de temporização.
