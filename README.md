# Keylux F75 USB-C

Controle de luz para o AULA F75 no Windows, sem instalar driver extra nem trocar firmware. Este fork usa o modo USB-C do [Keylux, de Sibte Hussain](https://github.com/SibteProf/keylux). O modo simples aceita só o F75 `258A:010C`, depois de confirmar que o teclado respondeu.

## Ciano por 10 segundos

Conecte o teclado no cabo e coloque a chave no modo com fio. No PowerShell, na pasta do executável:

```powershell
.\f75-usb-c-hidapi.exe check
if ($LASTEXITCODE -eq 0) {
    .\f75-usb-c-hidapi.exe apply 00FFFF
    if ($LASTEXITCODE -eq 0) {
        Start-Sleep -Seconds 10
        .\f75-usb-c-hidapi.exe apply off
    }
}
```

Isso manda ciano e apaga depois. Não restaura a cor anterior. Use `Fn + \` para voltar aos efeitos do teclado. O efeito nativo pode repintar a luz antes dos 10 segundos.

## Reaplicar a cor no Windows

Deixe o [script](packaging/windows/f75-usb-watch.ps1) junto do executável em uma pasta fixa. O exemplo usa ciano com brilho de 35%. Teste na janela primeiro:

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
