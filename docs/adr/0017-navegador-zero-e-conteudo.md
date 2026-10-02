# ADR-0017 — Navegador Zero como interface local com isolamento por origem, e conteúdo endereçado por hash

**Estado:** Aceita (DEVNET)
**Escopo:** DEVNET → candidata a TESTNET
**Data:** 2026-10-01
**Relacionamento:** REQ-064..070, REQ-023..026, `SPECIFICATIONS.md §58–§62`, `ARCHITECTURE.md §22–§25`, `§57` (Navegador: arquitetura, sandbox, segurança, integração com carteira), AC-BRW-001..004, AT-BRW-001..003, THR-BRW-001/002, THR-COM-001/002, INV-008, Manifesto §6, §10
**Especificações:** [`spec/CONTENT.md`](../../spec/CONTENT.md), [`spec/BROWSER.md`](../../spec/BROWSER.md)
**Implementação:** `crates/rz-core/src/content.rs`, `crates/rz-node/src/content.rs`, `crates/rz-browser`, `crates/rz-wallet`

## Contexto

`ARCHITECTURE §57` deixa em aberto a arquitetura, o sandbox, a segurança e a integração com a carteira do Navegador Zero. Os requisitos fixam o essencial:

* interface dedicada, sem autoridade sobre o protocolo e sem ser a única implementação (REQ-064..066, INV-008);
* acesso por provas criptográficas, nunca pela identificação do software (AC-BRW-004);
* Comunidades isoladas e sem privilégios desnecessários (`SPEC §62`);
* privacidade do IP (REQ-023, ADR-0011).

Faltava também onde fica o conteúdo das publicações. O Manifesto (§6) separa dados volumosos da cadeia, e as Comunidades devem continuar disponíveis quando participantes saem (REQ-070).

## Decisão

### Conteúdo

1. **Endereçado por hash e verificado por pedaço.** O identificador compromete o tamanho e o hash de cada pedaço de 1 MiB (até 16 MiB por objeto). Qualquer Node pode servir; nenhum precisa ser confiável.
2. **Só se hospeda o que o estado referencia:** alvos de nomes `zero://`, manifestos de Comunidades e interfaces de Comunidades reconhecidas. Publicar exige uma referência paga na cadeia, o que limita o abuso de armazenamento sem um moderador. Cada Node define sua cota.
3. **Replicação entre Nodes** por anúncio (`HAVE_CONTENT`) e sob demanda: a publicação sobrevive ao Node que a recebeu primeiro (REQ-070).
4. **Manifesto estruturado** com `frontend` e `module` (este para o Exonet Runtime), compatível com os manifestos livres da ADR-0015.

### Navegador Zero

5. **Processo local + navegador web do sistema.** O `zero-navegador` roda na máquina da pessoa, escuta só em loopback e usa o navegador já instalado como tela. Escrever um motor de renderização próprio é inviável e inseguro; o navegador do sistema traz um sandbox maduro e atualizado.
6. **Uma origem por publicação** (`NOME.TIPO.localhost`), com política de conteúdo que só permite a própria origem.
   * Publicações não leem umas às outras nem a interface.
   * Não contatam servidores externos, então não revelam o IP do leitor a terceiros.
   * Não acessam câmera, microfone, localização ou dispositivos.
7. **Interface sem scripts**, em outra origem. Toda ação exige o cabeçalho `Origin` da interface e um token por execução.
8. **Chaves nunca chegam às publicações.** Uma publicação cria um *pedido*; a pessoa vê a origem e o conteúdo exato na interface e aprova ali (THR-BRW-001).
9. **Identidade por site.** A pessoa se autentica numa publicação com uma chave derivada da semente e do endereço do site. Sites diferentes não correlacionam a mesma pessoa, e nenhum vê o endereço da Wallet.
10. **Fixação na primeira visita.** O identificador resolvido é fixado no primeiro acesso. Se um nome passar a apontar para outro conteúdo, ou uma Comunidade mudar de versão, o Navegador não abre nada sem revisão. Nomes parecidos com os já fixados geram alerta (THR-BRW-002, THR-COM-002).

## Alternativas consideradas

| Alternativa | Por que não |
| --- | --- |
| Motor de renderização próprio | Enorme superfície de ataque, sem o histórico de segurança dos navegadores existentes |
| Webview embutido como única interface | Viável depois como casca opcional; nesta fase dependeria de bibliotecas nativas pesadas. O gateway local serve a qualquer navegador e não impede um webview futuro |
| Gateway público na web (HTTP) | Centraliza, vê o IP e o histórico de todos, e vira ponto de controle (REQ-065, REQ-023) |
| Todas as publicações numa origem só, separadas por caminho | Uma publicação leria dados e pedidos de outra; sem isolamento real |
| Conteúdo na cadeia | Infla o estado de todos os validadores; contraria o Manifesto §6 |
| Hospedar qualquer conteúdo enviado | Armazenamento gratuito para abuso; exigiria moderação, que viraria censura arbitrária (THR-COM-003) |
| DHT aberta para localizar conteúdo | Mais complexa e exposta a Sybil e eclipse; a rede de Nodes existente basta na DEVNET |
| Entregar a chave ou uma sessão de assinatura à publicação | Qualquer publicação comprometida drenaria a Wallet |

## Consequências

* Qualquer pessoa pode publicar um site pagando a taxa de nome. Leitores verificam o conteúdo sem confiar em nenhum Node.
* Interfaces alternativas usam exatamente o mesmo protocolo; o Navegador não tem caminho privilegiado.
* Navegadores que não resolvem `*.localhost` para loopback precisam de entradas no arquivo de hosts (Chrome e Firefox resolvem).
* Limitações da DEVNET:
  * envios privados ficam na `zero-wallet`, pela varredura local de notas;
  * links para a web comum saem da Exonet;
  * o Node sabe quais identificadores foram pedidos (use Tor para desvincular de quem pediu).
* Novas mensagens P2P (`P2P_VERSION = 7`).

## Condições de revisão

* Casca nativa com webview e chaves em armazenamento do sistema.
* Envios privados e varredura de notas na interface.
* Contrato de mídia e streaming para objetos maiores.
* Pagamento ou incentivo pela hospedagem de conteúdo.
