# Keylux F75 USB-C

Controle de luz para o AULA F75 no Windows, sem instalar driver extra nem trocar firmware. Este fork usa o modo USB-C do [Keylux, de Sibte Hussain](https://github.com/SibteProf/keylux). O modo simples aceita só o F75 `258A:010C`, depois de confirmar que o teclado respondeu.

## Abrir no Windows

Extraia o ZIP inteiro e abra **Abrir Keylux.cmd**. Com o F75 no USB-C e a chave em modo com fio, clique em **Verificar USB**. A janela já abre pronta para ciano, 35%, 10 segundos. Escolha a cor na paleta, veja a prévia do teclado e clique em **Aplicar**.

**Salvar** guarda um perfil só no Windows; **Carregar** não envia nada ao teclado. **Parar** encerra o envio e **Apagar** manda um quadro preto. Não há macros, gravação no teclado, firmware ou inicialização automática na janela. Se o Windows bloquear o script, não desative a proteção: use o executável pelo terminal.

## Testar ciano por 10 segundos

Conecte em modo com fio. Na pasta do executável, rode no PowerShell:

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
