# Arquivos públicos da TESTNET

Esta pasta recebe, na cerimônia do Genesis (`docs/TESTNET.md`, Fase 1):

| Arquivo | Conteúdo |
| --- | --- |
| `validadores.txt` | uma chave pública de validador por linha, com o nome público do operador em comentário (`# nome`) |
| `genesis.bin` | o Genesis gerado por `rede-zero-node genesis-testnet` |
| `genesis.txt` | a saída de `rede-zero-node genesis-info`, incluindo o hash |
| `pares.txt` | endereços dos Nodes iniciais (`IP:porta` ou `nome.onion:porta`) |
| `faucet.txt` | endereço do faucet e o endereço transparente dele, para conferir que não vincula nem vota |

Nenhuma chave privada, senha ou dado pessoal entra aqui. A CI recusa arquivos de chave no repositório.

Para conferir o Genesis:

```bash
rede-zero-node genesis-testnet --out /tmp/conferencia.bin --validators testnet/validadores.txt \
    --faucet CHAVE_PUBLICA_DO_FAUCET
cmp /tmp/conferencia.bin testnet/genesis.bin
```

O Genesis é determinístico, então qualquer pessoa obtém o mesmo arquivo com as mesmas chaves e opções.
