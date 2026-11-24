# Keylux F75 USB-C

Controle de luz para o AULA F75 no Windows, sem instalar driver extra nem trocar firmware. Este fork usa o modo USB-C do [Keylux, de Sibte Hussain](https://github.com/SibteProf/keylux). O modo simples aceita só o F75 `258A:010C`, depois de confirmar que o teclado respondeu.

## Abrir no Windows

Extraia o ZIP e abra **Keylux-F75.exe**. Não precisa de PowerShell, instalador ou outro executável. Coloque o F75 no modo com fio, conecte pelo USB-C e clique em **Verificar USB**.

A prévia mostra a cor no brilho escolhido. **Aplicar** vem com ciano, 35%, por 10 segundos; marque **Até eu parar** para manter o envio. **Salvar perfil** grava só neste PC, no mesmo arquivo usado pela janela anterior. Nada é gravado no teclado.

A tela usa o controle de brilho direto do [Twinkle Tray](https://github.com/xanderfrangos/twinkle-tray) e a ideia de destacar o desenho do teclado do [Zumap](https://github.com/Keylab-dev/zumap) como referências visuais; não usa código ou imagens desses projetos.

Para compilar no Windows: `cargo build --release --locked -p aula-protocol --example f75_panel`. A CLI e o script opcional continuam no código-fonte, não são necessários para abrir este painel.

## Testar ciano por 10 segundos

No painel, basta clicar em **Aplicar** com os valores iniciais. Se estiver usando o pacote separado da CLI, rode no PowerShell:

```powershell
.\f75-usb-c-hidapi.exe hold 00FFFF 35 10
```

O comando verifica o teclado e mantém o envio por 10 segundos. Sem o último número, continua até **Ctrl+C**. Parar o envio não apaga nem restaura a cor anterior. Para apagar, use `apply off`; para voltar aos efeitos do teclado, use `Fn + \`. Se a cor não aparecer, o efeito nativo pode estar passando por cima. Não precisa mexer no firmware para testar.

## Reaplicar a cor no Windows

Deixe o [script](packaging/windows/f75-usb-watch.ps1) junto do executável em uma pasta fixa. O exemplo usa ciano com brilho de 35%. O monitor manda só um quadro por conexão; se a cor não ficar visível, teste `hold` antes de ativar no login.

```powershell
.\f75-usb-watch.ps1 -Action Watch -Color 00FFFF -Brightness 35
```

Pare com **Ctrl+C**. Se quiser ligar no próximo login, rode explicitamente:

```powershell
.\f75-usb-watch.ps1 -Action Install -Color 00FFFF -Brightness 35
```

Para desfazer: `.\f75-usb-watch.ps1 -Action Remove`. A cor é reaplicada quando o Windows detecta o cabo de novo (deixe uns 5 segundos fora antes de reconectar). Isso não grava a cor no teclado e não funciona fora do Windows. O script confere o hash do executável e não aplica a luz se o teclado não passar no `check`. Não desative as proteções do Windows.

[Compilar e diagnosticar](docs/F75-USB-LOCAL.md) · [Protocolo e segurança](docs/PROTOCOL.md) · [Projeto original](https://github.com/SibteProf/keylux)

Licença [MIT](LICENSE-MIT) ou [Apache-2.0](LICENSE-APACHE).
