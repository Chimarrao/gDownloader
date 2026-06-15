# Roadmap Possivel do gDownloader

## Fase 1: Solidez do Core

- endurecer pause/resume com retomada parcial por `Range`
- retries por item e por host com classificacao de erro
- transformar o limitador atual em limitador global real por host e por app
- notificacoes nativas tambem para falha, retry e limite atingido
- revisao completa de extracao pos-download
- melhorar logs e diagnostico em provedores
- consolidar limpeza/remocao com historico persistente

## Fase 2: Experiencia de Uso

- aprofundar o Link Grabber no estilo JDownloader com filtros, colunas e acoes em lote
- fila com grupos por pasta e por host
- filtros por status, servidor e disponibilidade
- acoes em lote: baixar, pausar, reiniciar, copiar links
- historico mais rico com reuso de URLs

## Fase 3: Performance

- download em partes para hosts compatveis com `Range`
- limite por host e fairness entre downloads
- retomada robusta de downloads parciais
- scheduler por horario e prioridade

## Fase 4: Ecossistema

- suporte a mais provedores
- sistema de plugins/provedores
- importacao/exportacao de lista de links
- creditos visuais e biblioteca consolidada de icones

## Fase 5: Plataforma

- endurecer Linux, Windows e macOS
- pipeline de build e testes multi-OS
- empacotamento, assinatura e atualizacao automatica

## Riscos Principais

- provedores mudam HTML/API com frequencia
- limites por IP e quota deixam testes instaveis
- download em partes exige suporte real a `Range`
- extracao pos-download varia muito entre macOS, Linux e Windows
