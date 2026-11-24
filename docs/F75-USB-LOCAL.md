# F75 USB-C: modo local e temporário

Este modo usa o protocolo F75 já implementado no Keylux. Ele é separado do
aplicativo desktop: não usa o cliente de rede OpenRGB, os efeitos externos Rhai,
o serviço de inicialização ou o instalador. Não registra nem lê teclas pressionadas.
Não usa administrador nem atualiza firmware. A leitura de identificação consulta
somente o ID USB já validado no upstream, `258a:010c`; não force outras variantes
(inclusive `1a2c:9407`), receptor ou Bluetooth.

No terminal, a partir da raiz do código:

```powershell
cargo build --release --locked -p aula-protocol --example f75_usb
.\target\release\examples\f75_usb.exe preview warm 35
.\target\release\examples\f75_usb.exe check
.\target\release\examples\f75_usb.exe apply warm 35
```

Outros exemplos: `preview "#8245ED" 65`, `apply focus 40`, `apply off`.
O comando sem argumentos mostra ajuda. Somente `apply` é capaz de enviar uma
cor, em um único quadro temporário. O efeito nativo pode repintar o teclado ou
não aceitar a cor até que o modo adequado seja selecionado nele. O programa
não força esse modo nem salva configuração no teclado. Para voltar ao efeito
nativo, use `Fn + \` no F75.

## O que foi revisado e o que não foi verificado

- Base upstream: `b397bcffe602e8666360bddeda8a82874fc5aea9`. Licenças originais preservadas.
- O modo novo chama identificação HID e o envio volátil existente. Não chama
  rotinas de gravação de modo, de cor persistente, de rede ou de inicialização.
- O checkout upstream inclui outros recursos, como cliente OpenRGB, scripts
  locais, inicialização opcional e scripts de empacotamento. A revisão do modo
  local não deve ser lida como aprovação de todos esses recursos nem de um
  binário de terceiros. Compilar com `--locked` evita trocar versões durante
  o build, mas não elimina risco de dependências. Não há garantia de “sem vírus”.
- Teste local sem hardware:
  `cargo test --locked -p aula-protocol --example f75_usb`.
  O teste físico em Windows ainda precisa confirmar o ID e a reação visual.
