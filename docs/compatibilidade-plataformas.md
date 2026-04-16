# Compatibilidade por Plataforma

## Estado Atual

| Plataforma | Estado | Pontos ja cobertos | Pontos que ainda precisam de rodada dedicada |
| --- | --- | --- | --- |
| macOS | Validado em desenvolvimento | Electron, backend Rust, shell integration, reveal/open de pasta, `df`, `unzip`, `tar`, binario Rust empacotado como recurso | validar extracao de `rar/7z` com utilitario externo e notarizacao final |
| Linux | Parcialmente preparado | backend Rust, `df`, `tar`, `unzip`, `which`, abertura de diretorio por `openPath`, binario Rust empacotado como recurso | validar variacoes de distro, comportamento de reveal e empacotamento final |
| Windows | Parcialmente preparado | backend Rust, `powershell` para espaco em disco, `Expand-Archive`, `where`, `tar`, nome do binario `.exe` tratado no Electron | validar build final, assinatura, SmartScreen e extracao `rar/7z` com ferramenta externa |

## Diferencas Tecnicas Relevantes

- espaco em disco:
  - macOS/Linux usam `df -k`
  - Windows usa `Get-CimInstance` no PowerShell
- extracao:
  - `zip` no Windows usa `Expand-Archive`
  - `zip` no macOS/Linux usa `unzip`
  - `tar.*` usa `tar`
  - `rar` e `7z` dependem de `unar`, `7z` ou `7za`
- descoberta de binarios:
  - macOS/Linux usam `which`
  - Windows usa `where`
- abrir pasta:
  - arquivo usa reveal da pasta
  - diretorio abre o caminho diretamente quando existir
- empacotamento do backend:
  - Electron dev usa `backend/target/debug/<binario>`
  - build empacotado copia `gdownloader-backend` ou `gdownloader-backend.exe` para `resources/`

## Riscos por Plataforma

- macOS:
  - notarizacao, permissao e comportamento de app empacotado
- Linux:
  - disponibilidade irregular de utilitarios externos
  - diferencas entre GNOME/KDE/XDG
- Windows:
  - quoting em caminhos com espacos
  - antivirus/SmartScreen
  - diferencas entre `tar` embutido e ferramentas externas

## Proxima Rodada Recomendada

1. smoke manual de arquivo, pasta, pause/resume, retry e extracao em cada OS
2. validar reveal/open folder para arquivo e diretorio
3. validar `rar/7z` com e sem utilitario externo instalado
4. revisar pipeline de build, assinatura e assets por plataforma
5. rodar smoke de build real: `npm run build:mac`, `npm run build:linux`, `npm run build:win`
