# Interceptacao local de downloads

O gDownloader pode subir um proxy local em `127.0.0.1:9667` para capturar respostas HTTP com cara de arquivo e enviar para a fila.

## Como usar

1. Abra Configuracoes > Interceptacao local.
2. Mude o modo para `Proxy local`.
3. Abra as configuracoes de proxy do sistema e aponte HTTP para `127.0.0.1`, porta `9667`.
4. Em apps que aceitam proxy do sistema, inicie o download normalmente.

O backend detecta `Content-Disposition: attachment` ou mimes permitidos, respeita o tamanho minimo configurado e enfileira o link no downloader HTTP generico preservando headers capturados como `User-Agent`, `Cookie` e `Referer`.

## Certificado CA

O backend gera uma CA local em `database/proxy-ca/gdownloader-local-ca.pem`. O botao `Instalar CA` abre o arquivo para instalacao manual.

Instalacao por sistema:

- macOS: abra o certificado no Keychain Access e marque como confiavel.
- Windows: importe o `.pem` em Autoridades Raiz Confiaveis.
- Linux: copie para `/usr/local/share/ca-certificates/` e rode `update-ca-certificates`.

## Limitacoes e troubleshooting

- HTTPS com `CONNECT` e apps com certificate pinning podem recusar MITM.
- HSTS e pinning sao esperados em bancos, contas e apps sensiveis; adicione esses dominios em `Dominios ignorados`.
- A interceptacao e local. Nao exponha `127.0.0.1:9667` para fora da maquina.
- Se um app nao usa proxy do sistema, configure o proxy dentro do proprio app.
